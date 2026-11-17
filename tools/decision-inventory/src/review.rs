//! Source-bound references and reviewed source pairs. Reference matching never
//! upgrades a historical disposition or claims test execution/compiler MC/DC.
use super::{Result, hash};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};
use syn::visit::Visit;

fn ensure(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn strings(value: &Value) -> Result<Vec<&str>> {
    value
        .as_array()
        .ok_or("expected string array")?
        .iter()
        .map(|v| v.as_str().ok_or_else(|| "expected string".into()))
        .collect()
}
fn read(root: &Path, name: &str) -> Result<Vec<u8>> {
    let relative = Path::new(name);
    ensure(
        !name.is_empty()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "repository-relative file required",
    )?;
    let path = root.join(relative).canonicalize()?;
    ensure(path.starts_with(root), "reference outside source root")?;
    Ok(fs::read(path)?)
}
fn pins(root: &Path, value: &Value) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for (name, expected) in value.as_object().ok_or("file pins required")? {
        let actual = hash(&read(root, name)?);
        ensure(
            expected.as_str() == Some(&actual),
            format!("stale file pin: {name}"),
        )?;
        out.insert(name.clone(), actual);
    }
    Ok(out)
}
fn matches(file: &str, pattern: &str) -> bool {
    if pattern.ends_with('/') {
        file.starts_with(pattern)
    } else {
        file == pattern
    }
}
fn location(loc: &Value, sources: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let name = loc["file"]
        .as_str()
        .ok_or("source location file required")?;
    let bytes = sources
        .get(name)
        .ok_or("location outside inventoried source")?;
    let start = usize::try_from(loc["start_byte"].as_u64().ok_or("source start required")?)?;
    let end = usize::try_from(loc["end_byte"].as_u64().ok_or("source end required")?)?;
    let slice = bytes.get(start..end).ok_or("source range outside file")?;
    ensure(
        loc["source"].as_str().map(str::as_bytes) == Some(slice)
            && loc["sha256"].as_str() == Some(&hash(slice)),
        "source range text/hash mismatch",
    )
}
fn table(control: &Value, width: usize) -> Result<()> {
    let rows = control["table"]
        .as_array()
        .ok_or("decision table required")?;
    let mut parsed = Vec::new();
    for row in rows {
        let values = row["conditions"]
            .as_array()
            .ok_or("table conditions required")?;
        ensure(
            values.len() == width,
            "table width differs from source conditions",
        )?;
        let values = values
            .iter()
            .map(|v| v.as_bool().ok_or("Boolean table condition required"))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let outcome = row["outcome"].as_bool().ok_or("Boolean outcome required")?;
        ensure(
            !parsed.iter().any(|(v, _)| v == &values),
            "duplicate table input",
        )?;
        parsed.push((values, outcome));
    }
    ensure(width > 0, "empty source condition set")?;
    for i in 0..width {
        ensure(
            parsed.iter().any(|(a, x)| {
                parsed.iter().any(|(b, y)| {
                    x != y && a[i] != b[i] && (0..width).all(|j| i == j || a[j] == b[j])
                })
            }),
            format!("no independent pair for condition {i}"),
        )?;
    }
    Ok(())
}
#[derive(Default)]
struct Tests(BTreeMap<String, usize>);
impl<'a> Visit<'a> for Tests {
    fn visit_item_fn(&mut self, node: &'a syn::ItemFn) {
        if node.attrs.iter().any(|a| a.path().is_ident("test")) {
            *self.0.entry(node.sig.ident.to_string()).or_default() += 1;
        }
        syn::visit::visit_item_fn(self, node);
    }
}

// A keyword hash alone cannot identify a decision: almost every residual has
// the same `?` hash. Reuse explicit rows only under an unchanged full-file pin,
// exact source position/range, kind and owner. Ambiguity remains manual work.
fn historical_rows(text: &str, file: &str, d: &Value) -> Vec<Value> {
    let mut section = None;
    let mut target_column = None;
    let basename = Path::new(file).file_name().unwrap().to_str().unwrap();
    let mut out = Vec::new();
    for (line, row) in text.lines().enumerate() {
        if row.starts_with('#') && row.contains(".rs") {
            section = row
                .split_whitespace()
                .map(|s| s.trim_matches('`'))
                .find(|s| s.ends_with(".rs"));
        }
        if !row.starts_with('|') {
            target_column = None;
            continue;
        }
        let cells = row
            .split('|')
            .map(|c| c.trim().trim_matches('`'))
            .collect::<Vec<_>>();
        if let Some(column) = cells.iter().position(|c| c.eq_ignore_ascii_case("target")) {
            target_column = Some(column);
        }
        if target_column.is_some_and(|column| cells.get(column).copied() != d["target"].as_str()) {
            continue;
        }
        let Some(status) = cells
            .iter()
            .find(|c| matches!(**c, "P" | "E" | "C" | "I" | "D" | "B" | "G"))
        else {
            continue;
        };
        let kind = d["kind"].as_str().unwrap();
        let owner = d["owner"].as_str().unwrap();
        if !cells.iter().any(|c| {
            c.split_whitespace()
                .any(|word| word.trim_matches('`') == kind)
        }) {
            continue;
        }
        if !cells.iter().any(|c| {
            *c == owner || c.starts_with(&format!("{owner}`")) || c.contains(&format!("`{owner}`"))
        }) {
            continue;
        }
        let explicit = cells.iter().find(|c| c.contains(".rs:"));
        let span = if let Some(span) = explicit {
            if !(span.starts_with(&format!("{file}:")) || span.starts_with(&format!("{basename}:")))
            {
                continue;
            }
            span.split_once(".rs:").unwrap().1
        } else {
            if section != Some(file) {
                continue;
            }
            let Some(span) = cells
                .iter()
                .find(|c| c.as_bytes().first().is_some_and(u8::is_ascii_digit))
            else {
                continue;
            };
            span
        };
        let byte_range = format!(
            "{}..{}",
            d["location"]["start_byte"], d["location"]["end_byte"]
        );
        let bracket_range = format!(
            "[{}–{}]",
            d["location"]["start_byte"], d["location"]["end_byte"]
        );
        let position = format!("{}:{}", d["location"]["line"], d["location"]["column"]);
        let exact_position = span
            .strip_prefix(&position)
            .is_some_and(|rest| rest.as_bytes().first().is_some_and(|b| !b.is_ascii_digit()));
        let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let has_bytes = cells.iter().any(|cell| {
            cell.split_once("..")
                .is_some_and(|(a, b)| numeric(a) && numeric(b))
                || cell
                    .split_once('[')
                    .and_then(|(_, rest)| rest.split_once(']'))
                    .is_some_and(|(range, _)| {
                        range
                            .split_once('–')
                            .is_some_and(|(a, b)| numeric(a) && numeric(b))
                    })
        });
        let identity = if has_bytes {
            cells.contains(&byte_range.as_str()) || row.contains(&bracket_range)
        } else {
            exact_position
        };
        if !identity {
            continue;
        }
        out.push(json!({"ledger_line":line+1,"status":status,"row":row}));
    }
    if out.len() == 1 { out } else { Vec::new() }
}
pub(super) fn reconcile(root: &Path, report: &Value, manifest: &Value) -> Result<Value> {
    ensure(manifest["schema_version"] == 1, "unknown review schema")?;
    ensure(
        report["failures"].as_array().is_some_and(Vec::is_empty),
        "incomplete inventory cannot be reconciled",
    )?;
    let sources = pins(root, &manifest["source_files"])?;
    ensure(
        serde_json::to_value(&sources)? == report["files"],
        "source scope differs from pinned inventory",
    )?;
    ensure(
        report["source_set_sha256"].as_str() == Some(&hash(&serde_json::to_vec(&sources)?)),
        "source-set digest mismatch",
    )?;
    let references = pins(root, &manifest["reference_files"])?;
    let markdown_sources = references
        .keys()
        .filter(|name| name.ends_with(".md"))
        .map(|name| {
            let bytes = read(root, name)?;
            ensure(
                hash(&bytes) == references[name],
                "reference changed during review",
            )?;
            Ok((name.clone(), String::from_utf8(bytes)?))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let groups = manifest["groups"]
        .as_array()
        .ok_or("review groups required")?;
    let mut names = BTreeSet::new();
    for g in groups {
        let name = g["name"].as_str().ok_or("group name required")?;
        ensure(
            !name.is_empty() && names.insert(name),
            "duplicate/empty review group",
        )?;
        ensure(
            matches!(g["scope"].as_str(), Some("runtime" | "driver" | "tooling")),
            "unknown review scope",
        )?;
        ensure(
            g["residual"].as_str().is_some_and(|s| !s.is_empty()),
            "explicit residual scope required",
        )?;
        let refs = strings(&g["references"])?;
        ensure(!refs.is_empty(), "empty group references")?;
        for name in refs {
            ensure(
                references.contains_key(name),
                format!("unpinned reference: {name}"),
            )?;
        }
    }
    let mut bindings = BTreeMap::new();
    for (file, sha) in &sources {
        let selected = groups
            .iter()
            .filter(|g| {
                strings(&g["paths"]).is_ok_and(|paths| paths.iter().any(|p| matches(file, p)))
            })
            .collect::<Vec<_>>();
        ensure(
            selected.len() == 1,
            format!("expected exactly one source disposition: {file}"),
        )?;
        let g = selected[0];
        let mut legacy_matches = Vec::new();
        for reference in strings(&g["references"])? {
            if reference.ends_with(".md") {
                let text = &markdown_sources[reference];
                if text.lines().any(|line| {
                    line.starts_with('|')
                        && line.contains(&format!("`{file}`"))
                        && line.contains(sha)
                }) {
                    legacy_matches.push(reference);
                }
            }
        }
        bindings.insert(file.clone(), json!({"source_sha256":sha,"group":g["name"],"scope":g["scope"],"references":g["references"],"residual":g["residual"],"matching_historical_full_file_pins":legacy_matches}));
    }
    // No basename matching: target, owner, kind and complete condition text all
    // have to match exactly once. A changed source pin rejects before this point.
    let mutations: Value = serde_json::from_slice(&read(root, "xtask/tests/mutations.json")?)?;
    ensure(
        references.contains_key("xtask/tests/mutations.json"),
        "mutation fixture must be pinned",
    )?;
    let decisions = report["decisions"].as_array().ok_or("decisions required")?;
    let mut ids = BTreeSet::new();
    let source_bytes = sources
        .keys()
        .map(|name| Ok((name.clone(), read(root, name)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    for (name, bytes) in &source_bytes {
        ensure(hash(bytes) == sources[name], "source changed during review")?;
    }
    let mut target_counts = BTreeMap::<String, usize>::new();
    for d in decisions {
        *target_counts
            .entry(d["target"].as_str().ok_or("target required")?.into())
            .or_default() += 1;
        location(&d["location"], &source_bytes)?;
        for condition in d["conditions"].as_array().ok_or("conditions required")? {
            location(condition, &source_bytes)?;
        }
        ensure(
            ids.insert(d["id"].as_str().ok_or("decision ID required")?),
            "duplicate decision ID",
        )?;
    }
    ensure(
        serde_json::to_value(&target_counts)? == manifest["target_records"],
        "source decision denominator changed",
    )?;
    for boundary in ["blind_spots", "exclusions"] {
        for record in report[boundary]
            .as_array()
            .ok_or("source boundaries required")?
        {
            location(&record["location"], &source_bytes)?;
        }
    }
    let mut control_names = BTreeSet::new();
    let mut controls = Vec::new();
    for c in manifest["controls"].as_array().ok_or("controls required")? {
        let name = c["name"].as_str().ok_or("control name required")?;
        ensure(control_names.insert(name), "duplicate control name")?;
        let s = &c["selector"];
        let selected = decisions
            .iter()
            .filter(|d| {
                d["target"] == s["target"]
                    && d["location"]["file"] == s["file"]
                    && d["owner"] == s["owner"]
                    && d["kind"] == s["kind"]
                    && json!(
                        d["conditions"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|x| x["source"].clone())
                            .collect::<Vec<_>>()
                    ) == s["conditions"]
            })
            .collect::<Vec<_>>();
        ensure(
            selected.len() == 1,
            format!("ambiguous or missing decision: {name}"),
        )?;
        let d = selected[0];
        match c["disposition"].as_str() {
            Some("directed_source_pair") => table(c, d["conditions"].as_array().unwrap().len())?,
            Some("propagation_control") => ensure(
                d["kind"] == "try_propagation",
                "propagation control must select a residual",
            )?,
            _ => return Err("unknown control disposition".into()),
        }
        let tests = c["tests"].as_array().ok_or("test references required")?;
        ensure(!tests.is_empty(), "empty test reference set")?;
        for test in tests {
            let file = test["file"].as_str().ok_or("test file required")?;
            ensure(
                references.contains_key(file) || sources.contains_key(file),
                "unpinned test source",
            )?;
            let mut names = Tests::default();
            names.visit_file(&syn::parse_file(&String::from_utf8(read(root, file)?)?)?);
            ensure(
                test["name"]
                    .as_str()
                    .is_some_and(|n| names.0.get(n) == Some(&1)),
                format!("missing/ambiguous #[test] for {name}"),
            )?;
        }
        let guards = strings(&c["mutations"])?;
        ensure(!guards.is_empty(), "mutation control required")?;
        for guard in guards {
            let selected = mutations["cases"]
                .as_array()
                .ok_or("mutation cases required")?
                .iter()
                .filter(|m| m["name"] == guard && m["file"] == s["file"])
                .count();
            ensure(
                selected == 1,
                format!("missing/ambiguous mutation: {guard}"),
            )?;
        }
        let mut c = c.clone();
        c["decision_id"] = d["id"].clone();
        c["execution"] = json!("not inferred from source references; see recorded campaigns");
        controls.push(c);
    }
    let mut counts = BTreeMap::<String, usize>::new();
    let mut reused = BTreeMap::<String, usize>::new();
    let records = decisions.iter().map(|d| {
        let b = &bindings[d["location"]["file"].as_str().unwrap()];
        *counts.entry(b["scope"].as_str().unwrap().into()).or_default() += 1;
        let direct = controls.iter().filter(|c| c["decision_id"] == d["id"]).map(|c| c["name"].clone()).collect::<Vec<_>>();
        let mut historical = Vec::new();
        for reference in b["matching_historical_full_file_pins"].as_array().unwrap() {
            let reference = reference.as_str().unwrap();
            for mut row in historical_rows(&markdown_sources[reference],d["location"]["file"].as_str().unwrap(),d) {
                row["reference"] = json!(reference);
                *reused.entry(row["status"].as_str().unwrap().into()).or_default() += 1;
                historical.push(row);
            }
        }
        json!({"id":d["id"],"group":b["group"],"scope":b["scope"],"disposition":if historical.is_empty() {"scoped_reference_only"} else {"historical_scoped_disposition"},"historical_rows":historical,"controls":direct})
    }).collect::<Vec<_>>();
    Ok(
        json!({"schema_version":1,"baseline":manifest["baseline"],"source_set_sha256":report["source_set_sha256"],"scope":"Current source bindings and explicit source pairs; no inferred execution, complete independence claim or compiler MC/DC percentage","source_files":bindings,"reference_files":references,"record_counts":counts,"historical_status_counts":reused,"records":records,"controls":controls,"blind_spots":report["blind_spots"],"exclusions":report["exclusions"],"compiler_mcdc_measured":false}),
    )
}
pub(super) fn markdown(review: &Value) -> String {
    let mut out = format!(
        "# Current source reconciliation\n\nBaseline `{}`. References preserve their original scope; source matching does not establish test execution or compiler MC/DC.\n\n| Scope | Source records |\n|---|---:|\n",
        review["baseline"].as_str().unwrap()
    );
    for (name, count) in review["record_counts"].as_object().unwrap() {
        out.push_str(&format!("| {name} | {count} |\n"));
    }
    out.push_str("\n| Source | Group | Matching historical file pin | Residual obligation |\n|---|---|---|---|\n");
    for (name, b) in review["source_files"].as_object().unwrap() {
        out.push_str(&format!(
            "| {name} | {} | {} | {} |\n",
            b["group"].as_str().unwrap(),
            !b["matching_historical_full_file_pins"]
                .as_array()
                .unwrap()
                .is_empty(),
            b["residual"].as_str().unwrap()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: std::path::PathBuf,
        report: Value,
        manifest: Value,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "decision-review-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let source = "fn check(a:bool,b:bool) { if a || b {} }\n#[test] fn pair() {}\n";
            fs::write(root.join("lib.rs"), source).unwrap();
            let mut audit = crate::Audit {
                root: root.clone(),
                ..Default::default()
            };
            audit.file(
                &root.join("lib.rs"),
                root.clone(),
                Vec::new(),
                "sample::lib",
            );
            let decision = audit.decisions.iter().find(|d| d["kind"] == "if").unwrap();
            let report = json!({"files":audit.files,"source_set_sha256":hash(&serde_json::to_vec(&audit.files).unwrap()),"decisions":audit.decisions,"blind_spots":audit.blind_spots,"exclusions":audit.exclusions,"failures":audit.failures});
            fs::write(
                root.join("ledger.md"),
                format!("| `lib.rs` | `{}` |\n", hash(source.as_bytes())),
            )
            .unwrap();
            fs::create_dir_all(root.join("xtask/tests")).unwrap();
            fs::write(
                root.join("xtask/tests/mutations.json"),
                r#"{"cases":[{"name":"a","file":"lib.rs"},{"name":"b","file":"lib.rs"}]}"#,
            )
            .unwrap();
            let mut refs = BTreeMap::new();
            for p in ["ledger.md", "xtask/tests/mutations.json"] {
                refs.insert(p, hash(&read(&root, p).unwrap()));
            }
            let manifest = json!({"schema_version":1,"baseline":"fixture","source_files":report["files"],"reference_files":refs,"target_records":{"sample::lib":report["decisions"].as_array().unwrap().len()},
                "groups":[{"name":"sample","scope":"runtime","paths":["lib.rs"],"references":["ledger.md"],"residual":"macro/dependency branches remain separate"}],
                "controls":[{"name":"or-pair","disposition":"directed_source_pair","selector":{"target":"sample::lib","file":"lib.rs","owner":"check","kind":"if","conditions":decision["conditions"].as_array().unwrap().iter().map(|c| c["source"].clone()).collect::<Vec<_>>()},"table":[{"conditions":[false,false],"outcome":false},{"conditions":[true,false],"outcome":true},{"conditions":[false,true],"outcome":true}],"tests":[{"file":"lib.rs","name":"pair"}],"mutations":["a","b"]}]});
            Self {
                root,
                report,
                manifest,
            }
        }
        fn review(&self) -> Result<Value> {
            reconcile(&self.root, &self.report, &self.manifest)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn historical_rows_require_exact_positions_owners_and_target_profiles() {
        let f = Fixture::new();
        let d = f.report["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["kind"] == "if")
            .unwrap();
        let row = format!(
            "| P | sample::lib | lib.rs:{}:{}–1:99 | {}..{} | if | check | pair |",
            d["location"]["line"],
            d["location"]["column"],
            d["location"]["start_byte"],
            d["location"]["end_byte"]
        );
        let text =
            format!("| Status | Target | Source span | Bytes | Kind | Owner | Evidence |\n{row}");
        assert_eq!(historical_rows(&text, "lib.rs", d)[0]["status"], "P");
        let range = format!(
            "{}..{}",
            d["location"]["start_byte"], d["location"]["end_byte"]
        );
        assert!(historical_rows(&text.replace(&range, "0..0"), "lib.rs", d).is_empty());
        assert!(
            historical_rows(&text.replace("sample::lib", "other::lib"), "lib.rs", d).is_empty()
        );
        assert!(
            historical_rows(&text.replace("| check |", "| different |"), "lib.rs", d).is_empty()
        );
        assert!(historical_rows(&text.replace("| if |", "| match |"), "lib.rs", d).is_empty());
        assert!(historical_rows(&format!("{text}\n{row}"), "lib.rs", d).is_empty());
        assert!(historical_rows("| P | if | check | 935f68319d4f |", "lib.rs", d).is_empty());
    }
    #[test]
    fn exact_source_pairs_keep_references_residuals_and_execution_scope_separate() {
        let f = Fixture::new();
        let a = f.review().unwrap();
        assert_eq!(a, f.review().unwrap());
        assert_eq!(a["compiler_mcdc_measured"], false);
        assert_eq!(a["records"].as_array().unwrap().len(), 2);
        assert_eq!(
            a["source_files"]["lib.rs"]["matching_historical_full_file_pins"],
            json!(["ledger.md"])
        );
        assert_eq!(
            a["controls"][0]["execution"],
            "not inferred from source references; see recorded campaigns"
        );
    }
    #[test]
    fn changed_source_and_reference_bytes_reject() {
        let f = Fixture::new();
        fs::write(f.root.join("lib.rs"), "changed").unwrap();
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("stale file pin")
        );
        let f = Fixture::new();
        fs::write(f.root.join("ledger.md"), "changed").unwrap();
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("stale file pin")
        );
    }
    #[test]
    fn omitted_or_ambiguous_source_dispositions_reject() {
        let mut f = Fixture::new();
        f.manifest["groups"][0]["paths"] = json!(["other.rs"]);
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("exactly one source disposition")
        );
        let mut f = Fixture::new();
        let mut g = f.manifest["groups"][0].clone();
        g["name"] = json!("duplicate-scope");
        f.manifest["groups"].as_array_mut().unwrap().push(g);
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("exactly one source disposition")
        );
    }
    #[test]
    fn missing_independence_pair_and_inconsistent_table_reject() {
        let mut f = Fixture::new();
        f.manifest["controls"][0]["table"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("no independent pair")
        );
        let mut f = Fixture::new();
        f.manifest["controls"][0]["table"][1]["conditions"] = json!([false, false]);
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("duplicate table input")
        );
    }
    #[test]
    fn missing_test_or_mutation_cannot_be_a_control() {
        let mut f = Fixture::new();
        f.manifest["controls"][0]["tests"][0]["name"] = json!("absent");
        assert!(f.review().unwrap_err().to_string().contains("#[test]"));
        let mut f = Fixture::new();
        f.manifest["controls"][0]["mutations"] = json!(["absent"]);
        assert!(f.review().unwrap_err().to_string().contains("mutation"));
    }
    #[test]
    fn changed_record_identity_ranges_and_denominators_reject() {
        let mut f = Fixture::new();
        let d = f.report["decisions"][0].clone();
        f.report["decisions"].as_array_mut().unwrap().push(d);
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("duplicate decision ID")
        );
        let mut f = Fixture::new();
        f.report["decisions"][0]["location"]["start_byte"] = json!(9999);
        assert!(f.review().unwrap_err().to_string().contains("source range"));
        let mut f = Fixture::new();
        f.report["decisions"].as_array_mut().unwrap().pop();
        assert!(f.review().unwrap_err().to_string().contains("denominator"));
    }
    #[test]
    fn renamed_decisions_and_unknown_schemas_reject() {
        let mut f = Fixture::new();
        f.manifest["controls"][0]["selector"]["owner"] = json!("other");
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("missing decision")
        );
        let mut f = Fixture::new();
        f.manifest["schema_version"] = json!(2);
        assert!(
            f.review()
                .unwrap_err()
                .to_string()
                .contains("unknown review schema")
        );
    }
}
