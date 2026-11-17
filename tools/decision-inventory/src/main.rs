use proc_macro2::Span;
use quote::ToTokens;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use syn::{
    Attribute, Expr, Item, Meta, Token,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// Three-valued cfg evaluation. Only test is configured. Platform/features
// remain unknown so the inventory includes every possible production variant.
fn cfg_value(meta: &Meta, include_tests: bool) -> Option<bool> {
    match meta {
        Meta::Path(path) if path.is_ident("test") => {
            if include_tests {
                None
            } else {
                Some(false)
            }
        }
        Meta::List(list) => {
            let args = list
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .ok()?;
            let values: Vec<_> = args
                .iter()
                .map(|arg| cfg_value(arg, include_tests))
                .collect();
            if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|v| *v == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|v| *v == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else if list.path.is_ident("not") && values.len() == 1 {
                values[0].map(|v| !v)
            } else {
                None
            }
        }
        _ => None,
    }
}
fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(x) => &x.attrs,
        Item::Enum(x) => &x.attrs,
        Item::ExternCrate(x) => &x.attrs,
        Item::Fn(x) => &x.attrs,
        Item::ForeignMod(x) => &x.attrs,
        Item::Impl(x) => &x.attrs,
        Item::Macro(x) => &x.attrs,
        Item::Mod(x) => &x.attrs,
        Item::Static(x) => &x.attrs,
        Item::Struct(x) => &x.attrs,
        Item::Trait(x) => &x.attrs,
        Item::TraitAlias(x) => &x.attrs,
        Item::Type(x) => &x.attrs,
        Item::Union(x) => &x.attrs,
        Item::Use(x) => &x.attrs,
        _ => &[],
    }
}
fn expr_attrs(expr: &Expr) -> &[Attribute] {
    match expr {
        Expr::Array(x) => &x.attrs,
        Expr::Assign(x) => &x.attrs,
        Expr::Async(x) => &x.attrs,
        Expr::Await(x) => &x.attrs,
        Expr::Binary(x) => &x.attrs,
        Expr::Block(x) => &x.attrs,
        Expr::Break(x) => &x.attrs,
        Expr::Call(x) => &x.attrs,
        Expr::Cast(x) => &x.attrs,
        Expr::Closure(x) => &x.attrs,
        Expr::Const(x) => &x.attrs,
        Expr::Continue(x) => &x.attrs,
        Expr::Field(x) => &x.attrs,
        Expr::ForLoop(x) => &x.attrs,
        Expr::Group(x) => &x.attrs,
        Expr::If(x) => &x.attrs,
        Expr::Index(x) => &x.attrs,
        Expr::Infer(x) => &x.attrs,
        Expr::Let(x) => &x.attrs,
        Expr::Lit(x) => &x.attrs,
        Expr::Loop(x) => &x.attrs,
        Expr::Macro(x) => &x.attrs,
        Expr::Match(x) => &x.attrs,
        Expr::MethodCall(x) => &x.attrs,
        Expr::Paren(x) => &x.attrs,
        Expr::Path(x) => &x.attrs,
        Expr::Range(x) => &x.attrs,
        Expr::RawAddr(x) => &x.attrs,
        Expr::Reference(x) => &x.attrs,
        Expr::Repeat(x) => &x.attrs,
        Expr::Return(x) => &x.attrs,
        Expr::Struct(x) => &x.attrs,
        Expr::Try(x) => &x.attrs,
        Expr::TryBlock(x) => &x.attrs,
        Expr::Tuple(x) => &x.attrs,
        Expr::Unary(x) => &x.attrs,
        Expr::Unsafe(x) => &x.attrs,
        Expr::While(x) => &x.attrs,
        Expr::Yield(x) => &x.attrs,
        _ => &[],
    }
}
fn atoms(expr: &Expr, out: &mut Vec<Span>) {
    match expr {
        Expr::Binary(x) if matches!(x.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) => {
            atoms(&x.left, out);
            atoms(&x.right, out);
        }
        Expr::Paren(x) => atoms(&x.expr, out),
        Expr::Group(x) => atoms(&x.expr, out),
        _ => out.push(expr.span()),
    }
}
#[derive(Default)]
struct Audit {
    root: PathBuf,
    include_tests: bool,
    decisions: Vec<Value>,
    exclusions: Vec<Value>,
    blind_spots: Vec<Value>,
    files: BTreeMap<String, String>,
    active: Vec<PathBuf>,
    failures: Vec<String>,
}
impl Audit {
    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
    fn file(&mut self, path: &Path, module_dir: PathBuf, cfg: Vec<String>, target: &str) {
        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                self.failures.push(format!("{}: {e}", path.display()));
                return;
            }
        };
        if !path.starts_with(&self.root) {
            self.failures
                .push(format!("module outside source root: {}", path.display()));
            return;
        }
        if self.active.contains(&path) {
            self.failures
                .push(format!("recursive module inclusion: {}", path.display()));
            return;
        }
        let source = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                self.failures.push(e.to_string());
                return;
            }
        };
        let file = match syn::parse_file(&source) {
            Ok(f) => f,
            Err(e) => {
                self.failures.push(format!("{}: {e}", path.display()));
                return;
            }
        };
        self.files
            .insert(self.relative(&path), hash(source.as_bytes()));
        self.active.push(path.clone());
        let mut visitor = Scanner {
            audit: self,
            source: &source,
            path: &path,
            module_dir,
            cfg,
            target,
            owner: Vec::new(),
        };
        if visitor.enter(&file.attrs) {
            visit::visit_file(&mut visitor, &file);
        }
        self.active.pop();
    }
}
struct Scanner<'a> {
    audit: &'a mut Audit,
    source: &'a str,
    path: &'a Path,
    module_dir: PathBuf,
    cfg: Vec<String>,
    target: &'a str,
    owner: Vec<String>,
}
impl Scanner<'_> {
    fn location(&self, span: Span) -> Value {
        let range = span.byte_range();
        let start = span.start();
        let end = span.end();
        let text = self.source.get(range.clone()).unwrap_or("");
        json!({"file":self.audit.relative(self.path),"start_byte":range.start,"end_byte":range.end,
            "line":start.line,"column":start.column+1,"end_line":end.line,"end_column":end.column+1,
            "source":text,"sha256":hash(text.as_bytes())})
    }
    fn enter(&mut self, attrs: &[Attribute]) -> bool {
        for attr in attrs {
            if attr.path().is_ident("derive") {
                self.blind(
                    "unexpanded_derive",
                    attr.span(),
                    "derive output is not expanded",
                );
            }
            let test = attr.path().is_ident("test") && !self.audit.include_tests;
            let cfg = attr.path().is_ident("cfg");
            if cfg || attr.path().is_ident("cfg_attr") {
                self.cfg.push(attr.meta.to_token_stream().to_string());
            }
            if attr.path().is_ident("cfg_attr") {
                self.blind(
                    "conditional_attribute",
                    attr.span(),
                    "cfg_attr is recorded without expansion",
                );
            }
            if test
                || (cfg
                    && attr
                        .parse_args::<Meta>()
                        .ok()
                        .and_then(|m| cfg_value(&m, self.audit.include_tests))
                        == Some(false))
            {
                self.audit.exclusions.push(json!({"target":self.target,"location":self.location(attr.span()),"cfg":self.cfg,"reason":"excluded when test=false"}));
                return false;
            }
        }
        true
    }
    fn blind(&mut self, kind: &str, span: Span, reason: &str) {
        self.audit.blind_spots.push(json!({"kind":kind,"target":self.target,"location":self.location(span),"cfg":self.cfg,"reason":reason}));
    }
    fn decision(
        &mut self,
        kind: &str,
        span: Span,
        condition: Option<&Expr>,
        alternatives: Vec<String>,
    ) {
        let range = span.byte_range();
        let mut conditions = Vec::new();
        if let Some(expr) = condition {
            atoms(expr, &mut conditions);
        }
        let location = self.location(span);
        let id = format!(
            "{}:{}:{}:{}:{}",
            self.target,
            self.audit.relative(self.path),
            range.start,
            range.end,
            kind
        );
        self.audit.decisions.push(json!({"id":id,"kind":kind,"target":self.target,"owner":self.owner.join("::"),
            "location":location,"cfg":self.cfg,"conditions":conditions.into_iter().map(|s|self.location(s)).collect::<Vec<_>>(),
            "alternatives":alternatives,"review":"unreviewed","ledger":ledger(&self.audit.relative(self.path))}));
    }
}
impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item(&mut self, node: &'ast Item) {
        let saved = self.cfg.len();
        if self.enter(item_attrs(node)) {
            visit::visit_item(self, node);
        }
        self.cfg.truncate(saved);
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.owner.push(node.sig.ident.to_string());
        visit::visit_item_fn(self, node);
        self.owner.pop();
    }
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let saved = self.cfg.len();
        if self.enter(&node.attrs) {
            self.owner.push(node.sig.ident.to_string());
            visit::visit_impl_item_fn(self, node);
            self.owner.pop();
        }
        self.cfg.truncate(saved);
    }
    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let saved = self.cfg.len();
        if self.enter(&node.attrs) {
            self.owner.push(node.sig.ident.to_string());
            visit::visit_trait_item_fn(self, node);
            self.owner.pop();
        }
        self.cfg.truncate(saved);
    }
    fn visit_expr(&mut self, node: &'ast Expr) {
        let saved = self.cfg.len();
        if !self.enter(expr_attrs(node)) {
            self.cfg.truncate(saved);
            return;
        }
        match node {
            Expr::If(x) => self.decision(
                "if",
                x.if_token.span,
                Some(&x.cond),
                vec!["then".into(), "else_or_continue".into()],
            ),
            Expr::While(x) => self.decision(
                "while",
                x.while_token.span,
                Some(&x.cond),
                vec!["body".into(), "exit".into()],
            ),
            Expr::ForLoop(x) => self.decision(
                "for_iteration",
                x.for_token.span,
                Some(&x.expr),
                vec!["next_item".into(), "finished".into()],
            ),
            Expr::Match(x) => self.decision(
                "match",
                x.match_token.span,
                Some(&x.expr),
                x.arms
                    .iter()
                    .map(|a| a.pat.to_token_stream().to_string())
                    .collect(),
            ),
            Expr::Binary(x) if matches!(x.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) => self
                .decision(
                    if matches!(x.op, syn::BinOp::And(_)) {
                        "short_circuit_and"
                    } else {
                        "short_circuit_or"
                    },
                    x.span(),
                    Some(node),
                    vec!["evaluate_rhs".into(), "skip_rhs".into()],
                ),
            Expr::Try(x) => self.decision(
                "try_propagation",
                x.question_token.span,
                Some(&x.expr),
                vec!["continue".into(), "propagate_residual".into()],
            ),
            Expr::Let(x) => self.decision(
                "pattern_test",
                x.let_token.span,
                Some(&x.expr),
                vec!["matches".into(), "does_not_match".into()],
            ),
            Expr::Verbatim(x) => {
                self.blind("verbatim_expression", x.span(), "unparsed token expression")
            }
            _ => {}
        }
        visit::visit_expr(self, node);
        self.cfg.truncate(saved);
    }
    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        let saved = self.cfg.len();
        if self.enter(&node.attrs) {
            if let Some((_, guard)) = &node.guard {
                self.decision(
                    "match_guard",
                    guard.span(),
                    Some(guard),
                    vec!["accept_arm".into(), "try_next_arm".into()],
                );
            }
            visit::visit_arm(self, node);
        }
        self.cfg.truncate(saved);
    }
    fn visit_local(&mut self, node: &'ast syn::Local) {
        let saved = self.cfg.len();
        if self.enter(&node.attrs) {
            if let Some(init) = &node.init
                && init.diverge.is_some()
            {
                self.decision(
                    "let_else",
                    node.let_token.span,
                    Some(&init.expr),
                    vec!["matches".into(), "else".into()],
                );
            }
            visit::visit_local(self, node);
        }
        self.cfg.truncate(saved);
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.blind(
            "unexpanded_macro",
            node.span(),
            &format!("{} token body not expanded", node.path.to_token_stream()),
        );
    }
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if let Some((_, items)) = &node.content {
            let old = self.module_dir.clone();
            self.module_dir.push(node.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.module_dir = old;
            return;
        }
        let explicit = node
            .attrs
            .iter()
            .find(|a| a.path().is_ident("path"))
            .and_then(|attr| match &attr.meta {
                Meta::NameValue(value) => match &value.value {
                    Expr::Lit(lit) => match &lit.lit {
                        syn::Lit::Str(s) => Some(s.value()),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            });
        let path = if let Some(path) = explicit {
            self.path.parent().unwrap().join(path)
        } else {
            let file = self.module_dir.join(format!("{}.rs", node.ident));
            let directory = self.module_dir.join(node.ident.to_string()).join("mod.rs");
            match (file.exists(), directory.exists()) {
                (true, false) => file,
                (false, true) => directory,
                _ => {
                    self.blind(
                        "unresolved_module",
                        node.span(),
                        "missing or ambiguous external module path",
                    );
                    return;
                }
            }
        };
        let directory = if path.file_name().is_some_and(|n| n == "mod.rs") {
            path.parent().unwrap().to_path_buf()
        } else {
            path.with_extension("")
        };
        self.audit
            .file(&path, directory, self.cfg.clone(), self.target);
    }
}
fn ledger(file: &str) -> Vec<&'static str> {
    let anchors: &[&str] = match file {
        "src/engine.rs" => &[
            "tx-v1-whole-database-revision-check",
            "tx-v2-collection-and-record-observations",
            "pub-v1-local-commit-barriers",
        ],
        "src/storage/codec.rs" => &[
            "rec-v1-witness-agreement-with-the-selected-root",
            "rec-v3-root-slot-selection-and-adjacency",
            "rec-v4-verify-the-selected-graph-before-opening",
            "rec-v5-verify-the-retained-catalog",
            "rec-v6-validate-metadata-before-choosing-a-root",
            "rec-v7-traversal-aliases-and-stable-object-reads",
        ],
        "src/memory.rs" => &[
            "mem-v1-independent-configuration-bounds",
            "mem-v2-allocation-budgets-reservation-and-ownership",
            "mem-v3-refcount-threshold-modeled-ordering-and-pointer-checks",
        ],
        "src/ffi.rs" => &[
            "ffi-v1-input-output-and-ownership-boundaries",
            "ffi-v2-path-validation-and-panic-containment",
        ],
        "src/tree.rs" => &["tree-v1-capacity-hash-and-key-controls"],
        "src/value.rs" => &["val-v1-weight-and-child-depth-limits-before-editing"],
        "src/storage/mod.rs" => &[
            "io-v1-exact-byte-transfer-progress",
            "pub-v1-local-commit-barriers",
            "rec-v2-stabilization-before-a-writable-reopen",
        ],
        "src/storage/simulator.rs" => &["sim-v1-lease-and-persistence-controls"],
        "src/storage/replacement.rs" => &["replace-v1-ordered-namespace-error-propagation"],
        "src/replication/follower.rs" => &[
            "fol-v1-durable-installation-and-progress",
            "fol-v2-image-verification",
            "fol-v3-older-and-duplicate-offers",
        ],
        "src/replication/mod.rs" => &[
            "pri-v1-accepting-follower-progress",
            "pri-v2-acknowledgment-before-and-after-upload",
        ],
        _ => &[],
    };
    anchors.to_vec()
}
fn markdown(report: &Value) -> String {
    let mut counts = report["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(|file| (file.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    let mut kinds = BTreeMap::<String, usize>::new();
    for row in report["decisions"].as_array().unwrap() {
        *counts
            .entry(row["location"]["file"].as_str().unwrap().into())
            .or_default() += 1;
        *kinds
            .entry(row["kind"].as_str().unwrap().into())
            .or_default() += 1;
    }
    let mut out = format!(
        "# Source decision inventory\n\n{} decision records across {} source files. Every record remains unreviewed.\nThis is an AST inventory across configurations, with no measured MC/DC claim.\n\n| Kind | Records |\n|---|---:|\n",
        report["decisions"].as_array().unwrap().len(),
        report["files"].as_object().unwrap().len()
    );
    for (kind, count) in kinds {
        out.push_str(&format!("| {kind} | {count} |\n"));
    }
    out.push_str("\n| Module | Unreviewed records | Existing module ledger, not condition coverage |\n|---|---:|---|\n");
    for (file, count) in counts {
        let links = ledger(&file)
            .iter()
            .map(|anchor| {
                format!(
                    "[{}](DESIGN.md#{anchor})",
                    anchor
                        .split('-')
                        .take(2)
                        .collect::<Vec<_>>()
                        .join("-")
                        .to_uppercase()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("| {file} | {count} | {links} |\n"));
    }
    out.push_str(&format!("\n{} excluded cfg/test items, {} macro/conditional-path blind spots, {} parse/resolution failures.\nFull source ranges, conditions, provenance and hashes are in inventory.json.\n",report["exclusions"].as_array().unwrap().len(),report["blind_spots"].as_array().unwrap().len(),report["failures"].as_array().unwrap().len()));
    out
}
fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or("usage: decision-inventory ROOT OUTPUT [--include-tests]")?,
    )
    .canonicalize()?;
    let output = PathBuf::from(args.next().ok_or("output directory required")?);
    let include_tests = match args.next().as_deref() {
        None => false,
        Some("--include-tests") => true,
        _ => return Err("unknown option".into()),
    };
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let metadata = Command::new("cargo")
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .current_dir(&root)
        .output()?;
    if !metadata.status.success() {
        return Err(String::from_utf8_lossy(&metadata.stderr)
            .into_owned()
            .into());
    }
    let metadata: Value = serde_json::from_slice(&metadata.stdout)?;
    let mut audit = Audit {
        root: root.clone(),
        include_tests,
        ..Default::default()
    };
    let mut targets = Vec::new();
    for package in metadata["packages"]
        .as_array()
        .ok_or("missing metadata packages")?
    {
        if !metadata["workspace_members"]
            .as_array()
            .unwrap()
            .contains(&package["id"])
        {
            continue;
        }
        for target in package["targets"].as_array().unwrap() {
            let kinds = target["kind"].as_array().unwrap();
            if !kinds.iter().any(|kind| {
                matches!(
                    kind.as_str(),
                    Some("lib" | "rlib" | "cdylib" | "staticlib" | "dylib" | "proc-macro" | "bin")
                )
            }) {
                continue;
            }
            let name = format!(
                "{}::{}",
                package["name"].as_str().unwrap(),
                target["name"].as_str().unwrap()
            );
            let path = Path::new(target["src_path"].as_str().unwrap());
            targets.push(json!({"target":name,"kinds":kinds,"required_features":target["required-features"],"entry":audit.relative(path)}));
            audit.file(
                path,
                path.parent().unwrap().to_path_buf(),
                Vec::new(),
                &name,
            );
        }
    }
    audit
        .decisions
        .sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let report = json!({"schema_version":1,"tool_version":env!("CARGO_PKG_VERSION"),"tool_source_sha256":hash(include_bytes!("main.rs")),"source_set_sha256":hash(&serde_json::to_vec(&audit.files)?),"scope":"source AST across feature/platform variants","include_tests":include_tests,"files":audit.files,"targets":targets,"decisions":audit.decisions,"exclusions":audit.exclusions,"blind_spots":audit.blind_spots,"failures":audit.failures});
    fs::create_dir_all(&output)?;
    fs::write(
        output.join("inventory.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::write(output.join("inventory.md"), markdown(&report))?;
    println!(
        "{} decisions, {} files, {} blind spots, {} failures",
        report["decisions"].as_array().unwrap().len(),
        report["files"].as_object().unwrap().len(),
        report["blind_spots"].as_array().unwrap().len(),
        report["failures"].as_array().unwrap().len()
    );
    if !report["failures"].as_array().unwrap().is_empty() {
        return Err("source inventory incomplete; inspect failures".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeSet,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "decision-inventory-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn audit(&self, include_tests: bool) -> Audit {
            let root = self.0.canonicalize().unwrap();
            let mut audit = Audit {
                root: root.clone(),
                include_tests,
                ..Default::default()
            };
            audit.file(&root.join("lib.rs"), root, Vec::new(), "sample::lib");
            audit
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn compound_conditions_cfg_exclusions_and_source_identity_are_explicit() {
        let fixture = Fixture::new();
        let source = r#"
#[cfg(feature="sample")]
fn check(a: bool,b: bool,c: bool)->Result<(),()> {
    if a && (b || c) { work()?; }
    while a || b { break; }
    for x in items { if let Some(y)=x { work()?; } }
    match value { Some(x) if b && c => work()?, _ => () }
    let Some(x)=value else { return Err(()); };
    assert!(a);
    Ok(())
}
#[cfg(all(feature="sample", test))]
mod tests { fn excluded() { if false {} } }
#[cfg(any(test,feature="sample"))]
fn retained_variant() { if true {} }
#[cfg(not(test))]
fn production() { if true {} }
"#;
        fs::write(fixture.0.join("lib.rs"), source).unwrap();
        let audit = fixture.audit(false);
        assert!(audit.failures.is_empty());
        assert_eq!(audit.exclusions.len(), 1);
        let first = audit
            .decisions
            .iter()
            .find(|r| r["kind"] == "if" && r["owner"] == "check")
            .unwrap();
        assert_eq!(
            first["conditions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["source"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(first["location"]["source"], "if");
        assert_eq!(first["cfg"].as_array().unwrap().len(), 1);
        for kind in [
            "if",
            "while",
            "for_iteration",
            "pattern_test",
            "match",
            "match_guard",
            "try_propagation",
            "let_else",
            "short_circuit_and",
            "short_circuit_or",
        ] {
            assert!(
                audit.decisions.iter().any(|row| row["kind"] == kind),
                "{kind}"
            );
        }
        assert!(!audit.decisions.iter().any(|r| r["owner"] == "excluded"));
        assert!(
            audit
                .decisions
                .iter()
                .any(|r| r["owner"] == "retained_variant")
        );
        assert!(audit.decisions.iter().any(|r| r["owner"] == "production"));
        assert_eq!(audit.blind_spots.len(), 1);
        assert_eq!(audit.files["lib.rs"], hash(source.as_bytes()));
        let ids = audit
            .decisions
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), audit.decisions.len());
        let second = fixture.audit(false);
        assert_eq!(audit.decisions, second.decisions);
        let with_tests = fixture.audit(true);
        assert!(
            with_tests
                .decisions
                .iter()
                .any(|r| r["owner"] == "excluded")
        );
    }
    #[test]
    fn module_resolution_carries_cfg_and_external_paths() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("nested")).unwrap();
        fs::write(fixture.0.join("lib.rs"),"#[cfg(feature=\"wire\")] mod nested; #[path=\"shared.rs\"] mod alias; #[cfg(test)] mod absent;").unwrap();
        fs::write(
            fixture.0.join("nested.rs"),
            "mod child; fn outer(){if true{}}",
        )
        .unwrap();
        fs::write(fixture.0.join("nested/child.rs"), "fn inner(){if false{}}").unwrap();
        fs::write(fixture.0.join("shared.rs"), "fn shared(){while false{}}").unwrap();
        let audit = fixture.audit(false);
        assert!(audit.failures.is_empty());
        assert!(audit.blind_spots.is_empty());
        assert_eq!(audit.files.len(), 4);
        assert_eq!(audit.decisions.len(), 3);
        let inner = audit
            .decisions
            .iter()
            .find(|r| r["owner"] == "inner")
            .unwrap();
        assert_eq!(inner["cfg"].as_array().unwrap().len(), 1);
        assert_eq!(inner["location"]["file"], "nested/child.rs");
        let shared = audit
            .decisions
            .iter()
            .find(|r| r["owner"] == "shared")
            .unwrap();
        assert!(shared["cfg"].as_array().unwrap().is_empty());
    }
    #[test]
    fn cfg_three_valued_evaluation_does_not_guess_features() {
        for (source, expected) in [
            ("test", Some(false)),
            ("not(test)", Some(true)),
            ("all(test, feature=\"x\")", Some(false)),
            ("any(test, feature=\"x\")", None),
            ("any()", Some(false)),
            ("all()", Some(true)),
        ] {
            assert_eq!(
                cfg_value(&syn::parse_str::<Meta>(source).unwrap(), false),
                expected,
                "{source}"
            );
        }
    }
    #[test]
    fn generated_code_and_unresolved_modules_are_visible() {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("lib.rs"),
            "#[derive(Clone)] struct É; #[cfg_attr(feature=\"x\", inline)] fn f(){ if true {} } mod missing;",
        )
        .unwrap();
        let audit = fixture.audit(false);
        for kind in [
            "unexpanded_derive",
            "conditional_attribute",
            "unresolved_module",
        ] {
            assert!(audit.blind_spots.iter().any(|row| row["kind"] == kind));
        }
        assert_eq!(audit.decisions[0]["location"]["source"], "if");
        assert_eq!(audit.decisions[0]["cfg"].as_array().unwrap().len(), 1);
        fs::write(fixture.0.join("lib.rs"), "fn broken(").unwrap();
        let audit = fixture.audit(false);
        assert_eq!(audit.failures.len(), 1);
        assert!(audit.decisions.is_empty());
    }
}
