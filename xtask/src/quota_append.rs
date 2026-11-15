//! Independent append oracle shared by real Linux and Windows quota fixtures.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
fn rows(revision: u64) -> Value {
    let new = revision == 4;
    let length = if new { 65536 } else { 512 };
    let (mult, add) = if new { (17, 23) } else { (13, 7) };
    const HEX: &[u8] = b"0123456789abcdef";
    let bytes = (0..length)
        .flat_map(|i| {
            let b = (i * mult + add) % 251;
            [HEX[b / 16], HEX[b % 16]]
        })
        .map(char::from)
        .collect::<String>();
    json!([{"key_hex":"7061796c6f6164","kind":"bytes","bytes_hex":bytes},{"key_hex":"726f77","kind":"integer","integer":if new{18}else{17}}])
}
pub(super) fn expected(source: &str, stage: &str, revision: u64, pid: u32, raw: i32) -> Value {
    let fail = stage == "fail";
    json!({"stage":stage,"source":source,"features":0,"pid":pid,"current_revision":revision,"retained_id":1,"retained_revision":2,"commit_error":if fail{14}else{0},"native_category":if !fail{"None"}else if raw==112{"DiskFull"}else{"Quota"},"native_raw_code":fail.then_some(raw),"native_operation":if fail{"Write"}else{"None"},"terminal_error":if fail{11}else{0},"recovery_required":fail,"blocked_write_error":if fail{16}else{0},"current_rows":rows(revision),"retained_rows":rows(3)})
}
pub fn verify(
    row: &Value,
    source: &str,
    stage: &str,
    revision: u64,
    pid: u32,
    raw: i32,
) -> Result<()> {
    ensure!(
        matches!(stage, "seed" | "fail" | "inspect" | "retry")
            && matches!(revision, 3 | 4)
            && pid > 0
            && matches!(raw, 112 | 122 | 1295),
        "invalid fixture expectation"
    );
    let expected = expected(source, stage, revision, pid, raw);
    ensure!(
        row.as_object()
            .is_some_and(|o| o.keys().eq(expected.as_object().unwrap().keys())),
        "fixture schema mismatch"
    );
    for (key, value) in expected.as_object().unwrap() {
        ensure!(&row[key] == value, "fixture oracle mismatch: {key}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntfs_keeps_the_reported_kernel_code_and_category() {
        let mut row = expected("source", "fail", 3, 42, 112);
        assert_eq!(row["native_category"], "DiskFull");
        assert!(verify(&row, "source", "fail", 3, 42, 112).is_ok());
        assert!(verify(&row, "source", "fail", 3, 42, 1295).is_err());
        row["native_category"] = json!("Quota");
        assert!(verify(&row, "source", "fail", 3, 42, 112).is_err());
        row["native_raw_code"] = json!(1295);
        assert!(verify(&row, "source", "fail", 3, 42, 1295).is_ok());
    }
}
