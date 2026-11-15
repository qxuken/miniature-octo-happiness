//! Independent finite operation model, deliberately without engine/FFI imports.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeMap;
pub type Cell = [u32; 4];
pub const MAGIC_V1: &[u8; 8] = b"WDBTRC1\n";
pub const MAGIC_V2: &[u8; 8] = b"WDBTRC2\n";
pub const TRACE_SHA256: &str = "4ad2fe8534dc73bd3032f1dcc41220f5bf919482c1f7df0a0a8716c4c40812c8";
pub const EXPECTED_SHA256: &str =
    "0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    pub version: u32,
    pub cells: Vec<Cell>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    source_sha256: String,
    v1: Vec<Cell>,
    v2: Vec<Cell>,
}
pub fn fixed(version: u32) -> Result<Trace> {
    let data: Fixture = serde_json::from_str(include_str!("../tests/trace-model.json"))?;
    ensure!(data.source_sha256.len() == 64, "model fixture provenance");
    let cells = match version {
        1 => data.v1,
        2 => data.v2,
        _ => anyhow::bail!("unsupported trace version"),
    };
    let t = Trace { version, cells };
    t.validate()?;
    Ok(t)
}
impl Trace {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.version, 1 | 2) && self.cells.len() <= 256,
            "unsupported trace version/size"
        );
        for &[op, slot, key, n] in &self.cells {
            ensure!(
                (1..=13).contains(&op) && slot < 4 && key <= 255 && n <= 254,
                "invalid trace cell"
            );
            ensure!(
                self.version != 2 || op != 1 || n <= 1,
                "invalid BEGIN validation mode"
            );
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = if self.version == 1 {
            MAGIC_V1.to_vec()
        } else {
            MAGIC_V2.to_vec()
        };
        out.extend((self.cells.len() as u32).to_le_bytes());
        for cell in &self.cells {
            for n in cell {
                out.extend(n.to_le_bytes());
            }
        }
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() >= 12, "bad trace magic/size");
        let version = if &bytes[..8] == MAGIC_V1 {
            1
        } else if &bytes[..8] == MAGIC_V2 {
            2
        } else {
            anyhow::bail!("bad trace magic")
        };
        let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        ensure!(
            count <= 256 && bytes.len() == 12 + 16 * count,
            "invalid bounded trace size"
        );
        let cells = bytes[12..]
            .as_chunks::<16>()
            .0
            .iter()
            .map(|chunk| {
                std::array::from_fn(|i| {
                    u32::from_le_bytes(chunk[4 * i..4 * i + 4].try_into().unwrap())
                })
            })
            .collect();
        let t = Self { version, cells };
        t.validate()?;
        Ok(t)
    }
    pub fn expected(&self) -> Result<String> {
        self.validate()?;
        let mut state = BTreeMap::new();
        let mut versions = BTreeMap::new();
        let mut txs: BTreeMap<u32, Transaction> = BTreeMap::new();
        let mut retained: BTreeMap<u32, Snapshot> = BTreeMap::new();
        let mut revision = 1u64;
        let mut collection = 1u64;
        let mut held: Option<Snapshot> = None;
        let mut next_id = 1u32;
        let mut lines = vec![];
        for (i, &[op, slot, key, n]) in self.cells.iter().enumerate() {
            let mut status = 0;
            let mut result = "-".to_owned();
            match op {
                1 => {
                    ensure!(!txs.contains_key(&slot), "duplicate transaction slot");
                    txs.insert(
                        slot,
                        Transaction {
                            state: state.clone(),
                            versions: versions.clone(),
                            mode: if self.version == 2 { n } else { 0 },
                            revision,
                            collection,
                            points: BTreeMap::new(),
                            scan: false,
                            writes: BTreeMap::new(),
                            closed: false,
                        },
                    );
                }
                2..=6 => {
                    let t = txs.get_mut(&slot).context("missing transaction slot")?;
                    if t.closed {
                        status = 11;
                    } else if op == 5 {
                        t.closed = true;
                        if (t.mode == 1 && t.revision != revision)
                            || (t.scan && t.collection != collection)
                            || t.points
                                .iter()
                                .any(|(key, value)| versions.get(key) != Some(value))
                        {
                            status = 10;
                        } else {
                            revision += 1;
                            for (&key, &value) in &t.writes {
                                collection = revision;
                                if let Some(value) = value {
                                    state.insert(key, value);
                                    versions.insert(key, revision);
                                } else {
                                    state.remove(&key);
                                    versions.remove(&key);
                                }
                            }
                            result = revision.to_string();
                        }
                    } else if op == 4 {
                        t.scan = true;
                        let mut overlay = t.state.clone();
                        for (&key, &value) in &t.writes {
                            if let Some(value) = value {
                                overlay.insert(key, value);
                            } else {
                                overlay.remove(&key);
                            }
                        }
                        result = rows(&overlay);
                    } else {
                        if let Some(&version) = t.versions.get(&key) {
                            t.points.insert(key, version);
                        } else {
                            t.scan = true;
                        }
                        if op == 2 {
                            t.writes.insert(key, Some(n));
                        }
                        if op == 6 {
                            t.writes.insert(key, None);
                        }
                        if op == 3 {
                            let value = t
                                .writes
                                .get(&key)
                                .copied()
                                .unwrap_or_else(|| t.state.get(&key).copied());
                            if let Some(value) = value {
                                result = value_text(value);
                            } else {
                                status = 3;
                            }
                        }
                    }
                }
                7 => {
                    ensure!(txs.remove(&slot).is_some(), "release missing transaction");
                }
                8 => {
                    ensure!(held.is_none(), "snapshot slot occupied");
                    held = Some(Snapshot {
                        revision,
                        state: state.clone(),
                    });
                }
                9 => {
                    ensure!(held.take().is_some(), "snapshot slot missing");
                }
                10 => {
                    let snapshot = held.as_ref().context("retain requires held snapshot")?;
                    retained.insert(next_id, snapshot.clone());
                    result = next_id.to_string();
                    next_id += 1;
                    revision += 1;
                }
                11 => {
                    ensure!(held.is_none(), "retained open needs empty snapshot slot");
                    if let Some(snapshot) = retained.get(&key) {
                        held = Some(snapshot.clone());
                    } else {
                        status = 3;
                    }
                }
                12 => {
                    ensure!(
                        txs.is_empty() && held.is_none(),
                        "reopen requires quiescence"
                    );
                }
                13 => {
                    if retained.remove(&key).is_some() {
                        revision += 1;
                    } else {
                        status = 3;
                    }
                }
                _ => unreachable!("validated opcode"),
            }
            let h = held.as_ref().map_or_else(
                || "-".to_owned(),
                |s| format!("{}{}", s.revision, rows(&s.state)),
            );
            let r = if retained.is_empty() {
                "-".to_owned()
            } else {
                retained
                    .keys()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            lines.push(format!(
                "{i} {op} {status} {revision} {} H={h} R={r} X={result}",
                rows(&state)
            ));
        }
        ensure!(
            txs.is_empty() && held.is_none(),
            "trace leaves live handles"
        );
        Ok(lines.join("\n") + "\n")
    }
}
#[derive(Clone)]
struct Snapshot {
    revision: u64,
    state: BTreeMap<u32, u32>,
}
struct Transaction {
    state: BTreeMap<u32, u32>,
    versions: BTreeMap<u32, u64>,
    mode: u32,
    revision: u64,
    collection: u64,
    points: BTreeMap<u32, u64>,
    scan: bool,
    writes: BTreeMap<u32, Option<u32>>,
    closed: bool,
}
fn value_text(n: u32) -> String {
    format!("m{{6e:u{n},76:l[b00{n:02x}ff,u{}]}}", n + 1)
}
fn rows(state: &BTreeMap<u32, u32>) -> String {
    format!(
        "[{}]",
        state
            .iter()
            .map(|(k, v)| format!("{k:02x}={}", value_text(*v)))
            .collect::<Vec<_>>()
            .join(";")
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    fn trace(cells: &[Cell]) -> Trace {
        Trace {
            version: 2,
            cells: cells.to_vec(),
        }
    }
    #[test]
    fn binary_bounds_and_round_trip() {
        let fixed = fixed(2).unwrap();
        let encoded = fixed.encode().unwrap();
        assert_eq!(Trace::decode(&encoded).unwrap(), fixed);
        let mut oversized = MAGIC_V2.to_vec();
        oversized.extend(257u32.to_le_bytes());
        let mut bad_op = encoded.clone();
        bad_op[12..16].copy_from_slice(&14u32.to_le_bytes());
        for bytes in [
            vec![],
            b"BADMAGIC\0\0\0\0".to_vec(),
            oversized,
            [encoded.as_slice(), b"x"].concat(),
            bad_op,
        ] {
            assert!(Trace::decode(&bytes).is_err());
        }
    }
    #[test]
    fn hand_checked_publications_and_missing_delete() {
        let t = trace(&[
            [1, 0, 0, 0],
            [6, 0, 99, 0],
            [3, 0, 99, 0],
            [5, 0, 0, 0],
            [5, 0, 0, 0],
            [7, 0, 0, 0],
        ]);
        assert_eq!(
            t.expected().unwrap(),
            "0 1 0 1 [] H=- R=- X=-\n1 6 0 1 [] H=- R=- X=-\n2 3 3 1 [] H=- R=- X=-\n3 5 0 2 [] H=- R=- X=2\n4 5 11 2 [] H=- R=- X=-\n5 7 0 2 [] H=- R=- X=-\n"
        );
    }
    #[test]
    fn finite_v1_conflicts_and_durable_history() {
        let text = fixed(1).unwrap().expected().unwrap();
        let lines = text.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 59);
        let indices = |status: &str| {
            lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    (line.split_whitespace().nth(2) == Some(status)).then_some(i)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(indices("10"), [22, 33, 42]);
        assert_eq!(indices("11"), [13, 23, 24]);
        assert!(
            lines[55]
                .contains(" H=2[00=m{6e:u1,76:l[b0001ff,u2]};ff=m{6e:u2,76:l[b0002ff,u3]}] R=- ")
        );
        assert_eq!(text, include_str!("../tests/trace-v1.expected"));
    }
    #[test]
    fn v1_bytes_and_records_semantics_are_frozen() {
        let original = fixed(1).unwrap();
        let bytes = original.encode().unwrap();
        assert_eq!(Trace::decode(&bytes).unwrap().encode().unwrap(), bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "78b916121e57e2cb0ce3d010dd126e46ef4d8047ec7ec9beb330f6dd3df33101"
        );
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(original.expected().unwrap().as_bytes())
            ),
            "5997dbaa08eef5bc3c98e6bdcea55f991da09d8d8d2693e5b73f5c5045e295ff"
        );
        let mut old = trace(&[
            [1, 0, 0, 1],
            [1, 1, 0, 0],
            [5, 1, 0, 0],
            [5, 0, 0, 0],
            [7, 0, 0, 0],
            [7, 1, 0, 0],
        ]);
        assert_eq!(
            old.expected().unwrap().lines().nth(3).unwrap(),
            "3 5 10 2 [] H=- R=- X=-"
        );
        old.version = 1;
        assert_eq!(
            old.expected().unwrap().lines().nth(3).unwrap(),
            "3 5 0 3 [] H=- R=- X=3"
        );
    }
    #[test]
    fn hand_checked_whole_database_empty_publication_and_terminal_states() {
        let t = trace(&[
            [1, 0, 0, 1],
            [1, 1, 0, 0],
            [5, 1, 0, 0],
            [5, 0, 0, 0],
            [3, 0, 0, 0],
            [5, 0, 0, 0],
            [7, 0, 0, 0],
            [7, 1, 0, 0],
        ]);
        assert_eq!(
            t.expected().unwrap(),
            "0 1 0 1 [] H=- R=- X=-\n1 1 0 1 [] H=- R=- X=-\n2 5 0 2 [] H=- R=- X=2\n3 5 10 2 [] H=- R=- X=-\n4 3 11 2 [] H=- R=- X=-\n5 5 11 2 [] H=- R=- X=-\n6 7 0 2 [] H=- R=- X=-\n7 7 0 2 [] H=- R=- X=-\n"
        );
        assert_eq!(
            trace(&[[1, 0, 0, 1], [5, 0, 0, 0], [7, 0, 0, 0]])
                .expected()
                .unwrap(),
            "0 1 0 1 [] H=- R=- X=-\n1 5 0 2 [] H=- R=- X=2\n2 7 0 2 [] H=- R=- X=-\n"
        );
    }
    #[test]
    fn hand_checked_retained_publication_validation_and_reopen() {
        let t = trace(&[
            [1, 0, 0, 0],
            [2, 0, 0, 1],
            [5, 0, 0, 0],
            [7, 0, 0, 0],
            [8, 0, 0, 0],
            [1, 0, 0, 1],
            [1, 1, 0, 0],
            [3, 1, 0, 0],
            [10, 0, 0, 0],
            [5, 0, 0, 0],
            [3, 0, 0, 0],
            [5, 0, 0, 0],
            [5, 1, 0, 0],
            [7, 0, 0, 0],
            [7, 1, 0, 0],
            [9, 0, 0, 0],
            [12, 0, 0, 0],
            [11, 0, 1, 0],
            [13, 0, 1, 0],
            [9, 0, 0, 0],
            [12, 0, 0, 0],
            [11, 0, 1, 0],
        ]);
        let row = "[00=m{6e:u1,76:l[b0001ff,u2]}]";
        let value = "m{6e:u1,76:l[b0001ff,u2]}";
        let held = format!("2{row}");
        let h = held.as_str();
        let controls = [
            (0, 1, "[]", "-", "-", "-"),
            (0, 1, "[]", "-", "-", "-"),
            (0, 2, row, "-", "-", "2"),
            (0, 2, row, "-", "-", "-"),
            (0, 2, row, h, "-", "-"),
            (0, 2, row, h, "-", "-"),
            (0, 2, row, h, "-", "-"),
            (0, 2, row, h, "-", value),
            (0, 3, row, h, "1", "1"),
            (10, 3, row, h, "1", "-"),
            (11, 3, row, h, "1", "-"),
            (11, 3, row, h, "1", "-"),
            (0, 4, row, h, "1", "4"),
            (0, 4, row, h, "1", "-"),
            (0, 4, row, h, "1", "-"),
            (0, 4, row, "-", "1", "-"),
            (0, 4, row, "-", "1", "-"),
            (0, 4, row, h, "1", "-"),
            (0, 5, row, h, "-", "-"),
            (0, 5, row, "-", "-", "-"),
            (0, 5, row, "-", "-", "-"),
            (3, 5, row, "-", "-", "-"),
        ];
        let expected = controls
            .iter()
            .enumerate()
            .map(|(i, (status, revision, rows, held, ids, result))| {
                format!(
                    "{i} {} {status} {revision} {rows} H={held} R={ids} X={result}\n",
                    t.cells[i][0]
                )
            })
            .collect::<String>();
        assert_eq!(t.expected().unwrap(), expected);
    }
    #[test]
    fn invalid_v2_validation_and_unknown_version_reject() {
        let mut t = trace(&[[1, 0, 0, 2]]);
        assert!(t.expected().is_err());
        t.version = 1;
        assert_eq!(Trace::decode(&t.encode().unwrap()).unwrap(), t);
        let mut bad = MAGIC_V2.to_vec();
        bad[6] = b'3';
        bad.extend(0u32.to_le_bytes());
        assert!(Trace::decode(&bad).is_err());
    }
    #[test]
    fn v2_validation_matrix_and_v1_prefix_are_frozen() {
        let t = fixed(2).unwrap();
        let bytes = t.encode().unwrap();
        let text = t.expected().unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), TRACE_SHA256);
        assert_eq!(
            format!("{:x}", Sha256::digest(text.as_bytes())),
            EXPECTED_SHA256
        );
        assert_eq!(text, include_str!("../tests/trace-v2.expected"));
        let lines = text.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 134);
        assert_eq!(
            lines[..59].join("\n") + "\n",
            fixed(1).unwrap().expected().unwrap()
        );
        assert_eq!(
            lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| (line.split_whitespace().nth(2) == Some("10")).then_some(i))
                .collect::<Vec<_>>(),
            [22, 33, 42, 75, 92, 114, 127]
        );
    }
}
