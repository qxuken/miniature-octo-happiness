//! Controlled sanitizer environments; inherited overrides cannot disable checks.
use crate::support::path;
use std::{collections::BTreeMap, ffi::OsString, path::Path};
const OVERRIDES: &[&str] = &[
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_ENCODED_RUSTDOCFLAGS",
    "RUSTC",
    "RUSTDOC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "LSAN_OPTIONS",
    "ASAN_OPTIONS",
    "TSAN_OPTIONS",
];
pub fn environment(
    inherited: &BTreeMap<OsString, OsString>,
    build: &Path,
    flags: &[String],
) -> BTreeMap<OsString, OsString> {
    let mut env = inherited.clone();
    for key in OVERRIDES {
        env.remove(&path(key));
    }
    env.insert(path("CARGO_TARGET_DIR"), path(build));
    env.insert(path("CARGO_INCREMENTAL"), path("0"));
    env.insert(path("CARGO_ENCODED_RUSTFLAGS"), path(flags.join("\x1f")));
    env.insert(path("CARGO_ENCODED_RUSTDOCFLAGS"), path(flags.join("\x1f")));
    env
}
pub fn address_environment(
    inherited: &BTreeMap<OsString, OsString>,
    build: &Path,
    flags: &[String],
) -> BTreeMap<OsString, OsString> {
    let mut env = environment(inherited, build, flags);
    env.insert(
        path("ASAN_OPTIONS"),
        path(format!(
            "halt_on_error=1:abort_on_error=0:exitcode=86:detect_leaks={}",
            u8::from(cfg!(target_os = "linux"))
        )),
    );
    env
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_inherited_flags_and_suppressions_are_replaced() {
        let inherited = OVERRIDES
            .iter()
            .map(|s| (path(s), path("suppressions=all")))
            .chain([(path("PATH"), path("keep"))])
            .collect();
        let flags = vec![
            "-Zsanitizer=address".into(),
            "-C".into(),
            "link-arg=a b".into(),
        ];
        let env = environment(&inherited, Path::new("/tmp/build"), &flags);
        for key in OVERRIDES {
            if !matches!(
                *key,
                "CARGO_ENCODED_RUSTFLAGS" | "CARGO_ENCODED_RUSTDOCFLAGS"
            ) {
                assert!(!env.contains_key(&path(key)));
            }
        }
        assert_eq!(
            env[&path("CARGO_ENCODED_RUSTFLAGS")],
            path("-Zsanitizer=address\x1f-C\x1flink-arg=a b")
        );
        assert_eq!(
            env[&path("CARGO_ENCODED_RUSTDOCFLAGS")],
            env[&path("CARGO_ENCODED_RUSTFLAGS")]
        );
        assert_eq!(env[&path("CARGO_INCREMENTAL")], path("0"));
        assert_eq!(env[&path("PATH")], path("keep"));
        let env = address_environment(&inherited, Path::new("/tmp/build"), &flags);
        let asan = env[&path("ASAN_OPTIONS")].to_str().unwrap();
        assert!(asan.contains("halt_on_error=1"));
        assert!(!asan.contains("suppressions"));
    }
}
