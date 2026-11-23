//! Disposable instrumentation of allocation and cache work on frozen copies.
use crate::{
    artifact_trace::{copy_tree, inventory},
    assurance::{self, Campaign},
    delivery,
    resource_campaign::{self, Profile},
    resource_oracle::{self, MIB},
    support::{Run, sha},
};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};
const PROFILES: [&str; 3] = ["native", "inspect", "faults"];
const GROWTH_FIELDS: [&str; 6] = ["cap", "peak", "used", "grows", "old", "new"];
const CACHE_FIELDS: [&str; 8] = [
    "value_clones",
    "data_clones",
    "value_rebuild_entries",
    "data_sort_entries",
    "data_compares",
    "value_slots_copied",
    "data_positions_copied",
    "data_shifted",
];
const GROWTH_SITE: &str = "self.charge = Some(charge);";
const GROWTH_SUFFIX: &str = r#"
// Disposable measurement only: fields/layout/allocation policy are unchanged.
static VEC_GROWS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static VEC_OLD_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static VEC_NEW_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
impl Drop for MemoryState {
    fn drop(&mut self) {
        eprintln!("GROWTH_DOMAIN cap={} peak={} used={} grows={} old={} new={}",
            self.limits.max_live_bytes, self.peak.load(Ordering::Relaxed), self.used.load(Ordering::Relaxed),
            VEC_GROWS.load(Ordering::Relaxed), VEC_OLD_BYTES.load(Ordering::Relaxed), VEC_NEW_BYTES.load(Ordering::Relaxed));
    }
}
"#;
const BEFORE: &str = "    cache.extend(old_cache.iter().cloned());";
const AFTER: &str = "        let slots = fork_vec(mem, &self.index.slots, 2 * MAX_OBJECTS)?;";
const DATA: &str = "        let order = fork_vec(mem, &self.index.order, MAX_OBJECTS)?;";
#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Probe {
    Growth,
    Cache,
}
impl Probe {
    fn name(self) -> &'static str {
        match self {
            Self::Growth => "growth",
            Self::Cache => "cache",
        }
    }
}
#[derive(Args)]
pub struct Options {
    pub probe: Probe,
    #[arg(long)]
    pub source: PathBuf,
    #[arg(long)]
    pub reference: PathBuf,
    #[arg(long)]
    pub revision: String,
    #[arg(long)]
    pub output: PathBuf,
}
fn replace(text: &str, needle: &str, replacement: &str) -> Result<String> {
    ensure!(
        text.matches(needle).count() == 1,
        "unique probe site required: {needle}"
    );
    Ok(text.replace(needle, replacement))
}
fn growth_text(text: &str, cache: bool) -> Result<String> {
    ensure!(
        !text.contains("GROWTH_DOMAIN"),
        "already instrumented domain"
    );
    let mut patched = replace(
        text,
        GROWTH_SITE,
        &format!(
            "VEC_GROWS.fetch_add(1, Ordering::Relaxed);\n        VEC_OLD_BYTES.fetch_add(self.charge.as_ref().map_or(0, |old| old.bytes) as u64, Ordering::Relaxed);\n        VEC_NEW_BYTES.fetch_add(charge.bytes as u64, Ordering::Relaxed);\n        {GROWTH_SITE}"
        ),
    )?;
    patched.push_str(GROWTH_SUFFIX);
    if cache {
        let site = "VEC_NEW_BYTES.load(Ordering::Relaxed));";
        patched = replace(
            &patched,
            site,
            &format!("{site}\n        crate::storage::report_cache_work();"),
        )?;
    }
    Ok(patched)
}
fn cache_text(mut text: String) -> Result<String> {
    ensure!(!text.contains("CACHE_WORK"), "already instrumented cache");
    ensure!(
        text.contains(BEFORE) != text.contains(AFTER),
        "exactly one supported cache policy required"
    );
    fn add(
        text: &str,
        site: &str,
        before: &[(&str, &str)],
        after: &[(&str, &str)],
    ) -> Result<String> {
        fn measurements(fields: &[(&str, &str)]) -> String {
            fields.iter().map(|(field,value)|format!("CACHE_{}.fetch_add(({value}) as u64, std::sync::atomic::Ordering::Relaxed);\n",field.to_uppercase())).collect()
        }
        replace(
            text,
            site,
            &format!("{}{site}\n{}", measurements(before), measurements(after)),
        )
    }
    if text.contains(BEFORE) {
        text = add(&text, BEFORE, &[("value_clones", "old_cache.len()")], &[])?;
        text = add(
            &text,
            "    data_cache.extend(old_data.iter().cloned());",
            &[("data_clones", "old_data.len()")],
            &[],
        )?;
    } else {
        text = add(
            &text,
            AFTER,
            &[("value_clones", "entries.len()")],
            &[("value_slots_copied", "slots.len()")],
        )?;
        text = add(
            &text,
            DATA,
            &[("data_clones", "entries.len()")],
            &[("data_positions_copied", "order.len()")],
        )?;
    }
    for (site, field, expr) in [
        (
            "        for (i, old) in cache.iter().enumerate() {",
            "value_rebuild_entries",
            "cache.len()",
        ),
        (
            "        index.order.sort_unstable_by(|a, b| {",
            "data_sort_entries",
            "cache.len()",
        ),
        ("        a.cmp(&b)\n", "data_compares", "1"),
        (
            "        self.order[position..].rotate_right(1);",
            "data_shifted",
            "self.order.len() - position - 1",
        ),
    ] {
        text = add(&text, site, &[(field, expr)], &[])?;
    }
    let mut template = vec![];
    let mut args = vec![];
    for name in CACHE_FIELDS {
        let counter = format!("CACHE_{}", name.to_uppercase());
        writeln!(
            text,
            "\nstatic {counter}: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);"
        )?;
        template.push(format!("{name}={{}}"));
        args.push(format!(
            "{counter}.load(std::sync::atomic::Ordering::Relaxed)"
        ));
    }
    writeln!(
        text,
        "\npub(super) fn report_cache_work() {{ eprintln!(\"CACHE_WORK {}\", {}); }}",
        template.join(" "),
        args.join(", ")
    )?;
    Ok(text)
}
fn instrument(root: &Path, mode: Probe) -> Result<()> {
    let memory = root.join("src/memory.rs");
    let code = growth_text(&fs::read_to_string(&memory)?, mode == Probe::Cache)?;
    if mode == Probe::Cache {
        let codec = root.join("src/storage/codec.rs");
        let codec_text = cache_text(fs::read_to_string(&codec)?)?;
        let module = root.join("src/storage/mod.rs");
        let mut module_text = fs::read_to_string(&module)?;
        ensure!(
            !module_text.contains("fn report_cache_work("),
            "already instrumented storage module"
        );
        module_text
            .push_str("\npub(crate) fn report_cache_work() { codec::report_cache_work(); }\n");
        fs::write(codec, codec_text)?;
        fs::write(module, module_text)?;
    }
    fs::write(memory, code)?;
    Ok(())
}
type Measurement = BTreeMap<&'static str, u64>;
fn measurements(
    text: &str,
    marker: &str,
    fields: &[&'static str],
    monotone: &[&str],
) -> Result<Vec<Measurement>> {
    let mut records: Vec<Measurement> = vec![];
    for line in text.lines().filter(|l| l.starts_with(marker)) {
        let data = line
            .strip_prefix(marker)
            .and_then(|s| s.strip_prefix(' '))
            .context("malformed measurement line")?;
        let tokens: Vec<_> = data.split(' ').collect();
        ensure!(
            tokens.len() == fields.len(),
            "wrong measurement field count"
        );
        let mut record = BTreeMap::new();
        for (field, token) in fields.iter().zip(tokens) {
            let (key, value) = token
                .split_once('=')
                .context("malformed measurement field")?;
            ensure!(
                key == *field && !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
                "malformed measurement field: {field}"
            );
            record.insert(*field, value.parse()?);
        }
        if let Some(previous) = records.last() {
            ensure!(
                monotone.iter().all(|key| record[key] >= previous[key]),
                "nonmonotonic measurement totals"
            );
        }
        records.push(record);
    }
    ensure!(!records.is_empty(), "missing instrumentation records");
    Ok(records)
}
fn growth(text: &str) -> Result<Vec<Measurement>> {
    let records = measurements(
        text,
        "GROWTH_DOMAIN",
        &GROWTH_FIELDS,
        &["grows", "old", "new"],
    )?;
    ensure!(
        records
            .iter()
            .all(|r| r["used"] == 0 && r["peak"] <= r["cap"]),
        "invalid domain ownership/admission"
    );
    Ok(records)
}
fn cache(text: &str) -> Result<Measurement> {
    Ok(
        measurements(text, "CACHE_WORK", &CACHE_FIELDS, &CACHE_FIELDS)?
            .pop()
            .unwrap(),
    )
}
fn validate_reference(
    proof: &Value,
    revision: &str,
    hashes: &BTreeMap<String, String>,
) -> Result<()> {
    ensure!(
        proof["verified"] == true
            && proof["revision"] == revision
            && proof["source_hashes"] == json!(hashes),
        "unverified or different clean reference"
    );
    ensure!(
        proof["runs"]
            .as_object()
            .is_some_and(|o| o.len() == PROFILES.len()),
        "incomplete clean profiles"
    );
    for profile in PROFILES {
        let run = &proof["runs"][profile];
        ensure!(
            run["passed"] == true && run["owned_live_processes_absent"] == true,
            "unclean reference worker"
        );
        resource_oracle::compound(
            run["rows"]
                .as_array()
                .context("clean reference rows missing")?,
            profile,
            revision,
        )?;
    }
    Ok(())
}
fn compare_runs(measured: &Value, reference: &Value) -> Result<()> {
    let measured = measured["rows"].as_array().context("measured rows")?;
    let reference = reference["rows"].as_array().context("reference rows")?;
    ensure!(
        measured.len() == reference.len() && !measured.is_empty(),
        "different phase count"
    );
    for (a, b) in measured[..measured.len() - 1]
        .iter()
        .zip(&reference[..reference.len() - 1])
    {
        for key in [
            "phase",
            "revision",
            "seq",
            "changed",
            "roots",
            "digests",
            "live_bytes",
            "file_bytes",
        ] {
            ensure!(
                a.get(key).is_some() && a[key] == b[key],
                "instrumentation changed state/layout: {key}"
            );
        }
    }
    Ok(())
}
pub fn run(run: &Run, opts: Options) -> Result<()> {
    let output = assurance::fresh_output(&run.root, &opts.output)?;
    let source = dunce::canonicalize(&opts.source)?;
    ensure!(
        !output.starts_with(&source) && !source.starts_with(&output),
        "resource source/output overlap"
    );
    let before = inventory(&source, &[])?;
    let reference = delivery::read_json(&opts.reference)?;
    let reference_hash = sha(&opts.reference)?;
    validate_reference(&reference, &opts.revision, &before)?;
    let mut c = Campaign::new(run, &output, "instrumented work and resource limits only")?;
    c.evidence["verified"] = json!(false);
    c.save()?;
    let result = (|| -> Result<()> {
        let copy = output.join("source");
        copy_tree(&source, &copy)?;
        instrument(&copy, opts.probe)?;
        let revision = format!("{}+{}-probe", opts.revision, opts.probe.name());
        let work = output.join("run");
        resource_campaign::worker(run, &copy, &work, &revision, Profile::Compound)?;
        let evidence = work.join("evidence.json");
        let observed = delivery::read_json(&evidence)?;
        validate_reference(&observed, &revision, &inventory(&copy, &[])?)?;
        let mut profiles = BTreeMap::new();
        for profile in PROFILES {
            compare_runs(&observed["runs"][profile], &reference["runs"][profile])?;
            let stderr = fs::read_to_string(work.join(format!("{profile}.stderr")))?;
            let domains = growth(&stderr)?;
            let last = domains.last().unwrap();
            let peaks = domains
                .iter()
                .filter(|r| r["cap"] == 256 * MIB)
                .map(|r| r["peak"])
                .collect::<Vec<_>>();
            let mut result = json!({"domains":domains,"copy_domain_peaks":peaks,"total_grows":last["grows"],"old_layout_bytes":last["old"],"new_layout_bytes":last["new"]});
            if opts.probe == Probe::Cache {
                result["cache"] = json!(cache(&stderr)?);
            }
            profiles.insert(profile, result);
        }
        ensure!(
            inventory(&source, &[])? == before && sha(&opts.reference)? == reference_hash,
            "reference inputs changed during probe"
        );
        c.evidence["profiles"] = json!(profiles);
        c.evidence["reference_sha256"] = json!(reference_hash);
        c.evidence["run_sha256"] = json!(sha(&evidence)?);
        c.evidence["instrumented_source_hashes"] = observed["source_hashes"].clone();
        c.evidence["binary_sha256"] = observed["binary_sha256"].clone();
        Ok(())
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn growth_requires_unique_patch_and_monotonic_released_domains() {
        let original = include_str!("../../src/memory.rs");
        let patched = growth_text(original, false).unwrap();
        assert_eq!(patched.matches("VEC_GROWS.fetch_add(").count(), 1);
        assert_eq!(patched.matches("impl Drop for MemoryState").count(), 1);
        assert!(growth_text(&patched, false).is_err());
        assert!(growth_text("no matching site", false).is_err());
        assert!(growth_text(&GROWTH_SITE.repeat(2), false).is_err());
        let valid = "GROWTH_DOMAIN cap=10 peak=9 used=0 grows=2 old=4 new=9\n";
        assert_eq!(growth(valid).unwrap()[0]["old"], 4);
        for invalid in [
            String::new(),
            format!("{valid}GROWTH_DOMAIN broken\n"),
            valid.replace("used=0", "used=1"),
            valid.replace("peak=9", "peak=11"),
            format!("{valid}{}", valid.replace("grows=2", "grows=1")),
        ] {
            assert!(growth(&invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn cache_requires_unique_current_or_legacy_sites_and_monotonic_counters() {
        let current = include_str!("../../src/storage/codec.rs");
        let legacy = current
            .replace(AFTER, BEFORE)
            .replace(DATA, "    data_cache.extend(old_data.iter().cloned());");
        for source in [current, &legacy] {
            let instrumented = cache_text(source.into()).unwrap();
            assert_eq!(instrumented.matches("CACHE_WORK").count(), 1);
            assert!(cache_text(instrumented).is_err());
        }
        let memory = growth_text(include_str!("../../src/memory.rs"), true).unwrap();
        assert_eq!(
            memory
                .matches("crate::storage::report_cache_work();")
                .count(),
            1
        );
        assert!(cache_text("no source match".into()).is_err());
        assert!(cache_text(format!("{current}\n{BEFORE}")).is_err());
        let valid = format!(
            "CACHE_WORK {}\n",
            CACHE_FIELDS.map(|f| format!("{f}=2")).join(" ")
        );
        assert_eq!(cache(&valid).unwrap()["value_clones"], 2);
        for invalid in [
            String::new(),
            format!("{valid}CACHE_WORK truncated\n"),
            valid.replacen("=2", "=-1", 1),
            format!(
                "{valid}{}",
                valid.replace("value_clones=2", "value_clones=1")
            ),
        ] {
            assert!(cache(&invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn clean_reference_requires_exact_source_and_every_successful_profile() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/resource-oracles.json")).unwrap();
        let source = fixture["source"].as_str().unwrap();
        let hashes = BTreeMap::from([("src/memory.rs".into(), "sentinel".into())]);
        let runs: BTreeMap<_,_>=PROFILES.into_iter().map(|profile|(profile,json!({"passed":true,"owned_live_processes_absent":true,"rows":fixture["compound"][profile]}))).collect();
        let proof = json!({"verified":true,"revision":source,"source_hashes":hashes,"runs":runs});
        validate_reference(&proof, source, &hashes).unwrap();
        for (key, value) in [
            ("verified", json!(1)),
            ("revision", json!("wrong")),
            ("source_hashes", json!({})),
        ] {
            let mut bad = proof.clone();
            bad[key] = value;
            assert!(validate_reference(&bad, source, &hashes).is_err());
        }
        let mut bad = proof.clone();
        bad["runs"].as_object_mut().unwrap().remove("faults");
        assert!(validate_reference(&bad, source, &hashes).is_err());
        for (profile, key, value) in [
            ("native", "rows", json!([])),
            ("native", "passed", json!(false)),
            ("inspect", "owned_live_processes_absent", json!(false)),
        ] {
            let mut bad = proof.clone();
            bad["runs"][profile][key] = value;
            assert!(validate_reference(&bad, source, &hashes).is_err());
        }
        let run = &proof["runs"]["native"];
        compare_runs(run, run).unwrap();
        let mut bad = run.clone();
        bad["rows"].as_array_mut().unwrap().pop();
        assert!(compare_runs(&bad, run).is_err());
        let mut bad = run.clone();
        bad["rows"][0]["file_bytes"] = json!(0);
        assert!(compare_runs(&bad, run).is_err());
    }
}
