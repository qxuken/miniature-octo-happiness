//! Independent exact-package integrity gate. No engine imports or consumer trust.
use crate::support::sha;
use anyhow::{Context, Result, ensure};
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use std::{collections::BTreeMap, fmt, fs, io::Read, path::Path};
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Number(v.into())))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Number(v.into())))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Number(
                    Number::from_f64(v).ok_or_else(|| E::custom("nonfinite JSON"))?,
                )))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut out = vec![];
                while let Some(v) = seq.next_element::<Unique>()? {
                    out.push(v.0);
                }
                Ok(Unique(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut out = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if out.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate JSON key: {key}")));
                    }
                    out.insert(key, map.next_value::<Unique>()?.0);
                }
                Ok(Unique(Value::Object(out)))
            }
        }
        d.deserialize_any(V)
    }
}
pub fn json(bytes: &[u8]) -> Result<Value> {
    Ok(serde_json::from_slice::<Unique>(bytes)?.0)
}
fn json_bytes(file: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(file)?;
    ensure!(
        meta.is_file() && !reparse(&meta) && meta.len() <= 2 * 1024 * 1024,
        "nonregular or oversized JSON input"
    );
    let mut bytes = vec![];
    fs::File::open(file)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 2 * 1024 * 1024, "JSON grew beyond limit");
    Ok(bytes)
}
pub fn read_json(file: &Path) -> Result<Value> {
    json(&json_bytes(file)?)
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub fn read_json_hashed(file: &Path) -> Result<(Value, String)> {
    use sha2::{Digest, Sha256};
    let bytes = json_bytes(file)?;
    Ok((json(&bytes)?, format!("{:x}", Sha256::digest(bytes))))
}
pub(crate) fn reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}
pub fn inventory(root: &Path) -> Result<BTreeMap<String, String>> {
    fn visit(
        root: &Path,
        dir: &Path,
        out: &mut BTreeMap<String, String>,
        bytes: &mut u64,
        entries: &mut usize,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 64, "package directory depth budget");
        let meta = fs::symlink_metadata(dir)?;
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink() && !reparse(&meta),
            "package directory alias"
        );
        for entry in fs::read_dir(dir)? {
            *entries += 1;
            ensure!(*entries <= 16384, "package entry budget");
            let file = entry?.path();
            let meta = fs::symlink_metadata(&file)?;
            ensure!(
                !meta.file_type().is_symlink() && !reparse(&meta),
                "package symlink/reparse input"
            );
            if meta.is_dir() {
                visit(root, &file, out, bytes, entries, depth + 1)?;
            } else {
                ensure!(
                    meta.is_file() && meta.len() <= 256 * 1024 * 1024,
                    "nonregular or oversized package file"
                );
                *bytes = bytes
                    .checked_add(meta.len())
                    .context("package size overflow")?;
                ensure!(
                    *bytes <= 1024 * 1024 * 1024 && out.len() < 4096,
                    "package inventory budget"
                );
                let name = file
                    .strip_prefix(root)?
                    .to_str()
                    .context("UTF-8 package path")?
                    .replace(std::path::MAIN_SEPARATOR, "/");
                out.insert(name, sha(&file)?);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out, &mut 0, &mut 0, 0)?;
    Ok(out)
}
#[cfg(any(unix, test))]
pub fn verified(root: &Path) -> Result<Value> {
    use std::path::Component;
    let info = read_json(&root.join("manifest.json"))?;
    let source = info["source"].as_str().context("package source missing")?;
    ensure!(
        info["verified"] == true
            && source.len() == 40
            && source
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            && info["source_id"] == source,
        "unverified exact-source package"
    );
    let declared = info["sha256"]
        .as_object()
        .context("package hashes missing")?;
    ensure!(!declared.is_empty(), "empty package inventory");
    let mut expected = BTreeMap::new();
    for (name, digest) in declared {
        ensure!(
            !name.is_empty()
                && !name.contains('\0')
                && Path::new(name)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_))),
            "package hash/path escape"
        );
        let normalized = if cfg!(windows) {
            name.replace('\\', "/")
        } else {
            name.clone()
        };
        ensure!(
            normalized
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != "..")
                && normalized != "manifest.json",
            "ambiguous/self-hashed payload path"
        );
        let hash = digest.as_str().context("payload hash type")?;
        ensure!(
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "invalid payload digest"
        );
        ensure!(
            expected.insert(normalized, hash.to_owned()).is_none(),
            "duplicate payload alias"
        );
    }
    let mut actual = inventory(root)?;
    ensure!(
        actual.remove("manifest.json").is_some(),
        "package manifest missing"
    );
    ensure!(actual == expected, "package inventory or payload mismatch");
    Ok(info)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn package(root: &Path) -> Value {
        fs::write(root.join("payload"), b"synthetic").unwrap();
        let info = serde_json::json!({"verified":true,"source":"a".repeat(40),"source_id":"a".repeat(40),"sha256":{"payload":sha(&root.join("payload")).unwrap()}});
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&info).unwrap(),
        )
        .unwrap();
        info
    }
    #[test]
    fn verified_package_rejects_corruption_extras_symlinks_and_dirty_identity() {
        let dir =
            crate::support::Scratch::new(&std::env::temp_dir(), "delivery-integrity").unwrap();
        let mut info = package(dir.path());
        verified(dir.path()).unwrap();
        fs::write(dir.path().join("payload"), b"changed").unwrap();
        assert!(verified(dir.path()).is_err());
        fs::write(dir.path().join("payload"), b"synthetic").unwrap();
        fs::write(dir.path().join("extra"), b"extra").unwrap();
        assert!(verified(dir.path()).is_err());
        fs::remove_file(dir.path().join("extra")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("payload", dir.path().join("alias")).unwrap();
            assert!(verified(dir.path()).is_err());
            fs::remove_file(dir.path().join("alias")).unwrap();
        }
        info["source_id"] = serde_json::json!("a".repeat(40) + "-dirty");
        fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_vec(&info).unwrap(),
        )
        .unwrap();
        assert!(verified(dir.path()).is_err());
    }
    #[test]
    fn escaped_self_hashed_and_duplicate_json_paths_reject() {
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "delivery-path").unwrap();
        let original = package(dir.path());
        for name in [
            "../escape",
            "/absolute",
            "a/../payload",
            "a//payload",
            "./payload",
            "manifest.json",
        ] {
            let mut info = original.clone();
            info["sha256"][name] = serde_json::json!("0".repeat(64));
            fs::write(
                dir.path().join("manifest.json"),
                serde_json::to_vec(&info).unwrap(),
            )
            .unwrap();
            assert!(verified(dir.path()).is_err(), "{name}");
        }
        for bytes in [
            br#"{"a":1,"a":2}"#.as_slice(),
            br#"{"sha256":{"a":"one","a":"two"}}"#.as_slice(),
        ] {
            assert!(json(bytes).is_err());
        }
        assert_eq!(
            json(br#"{"a":[true,false,null,1,-2,3.5,"text"]}"#).unwrap()["a"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
    }
    #[cfg(unix)]
    #[test]
    fn empty_directory_depth_is_bounded_before_recursion_exhaustion() {
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "delivery-depth").unwrap();
        let mut path = dir.path().to_owned();
        for _ in 0..65 {
            path.push("d");
            fs::create_dir(&path).unwrap();
        }
        assert!(
            inventory(dir.path())
                .unwrap_err()
                .to_string()
                .contains("depth budget")
        );
    }
}
