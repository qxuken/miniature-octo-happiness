# Build identity and Windows C path decoding

The C ABI probe checks the source identity read from the exact loaded shared
library. A release build with the source stamp unset returns `unknown`.
Setting the stamp to the frozen Git revision returns that exact revision.
Expecting `unknown` from the stamped library fails the probe. The macOS
and Linux proof retains both libraries and verifies the base feature mask.

The native Windows path test passes an ordinary UTF-8 path and a one-byte
invalid sequence to the same converter. The invalid sequence returns Invalid
before opening a database. The native debug and release jobs tested the
same source bytes as the current inventory.

| Source | SHA-256 |
| --- | --- |
| `src/ffi.rs` | `badceb85abf4de706b52d6ee8d03836712055e8e5a3b93ca8b7cca2fbf963e29` |
| `src/version.rs` | `5006fbd7ced8252c579cb6695cd0ecbf1c6ab15491a7fc43f8a999539cb2fcf3` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/ffi.rs:195:70– | 6322..6323 | try_propagation | path | Native Windows debug and release accept the one-byte ASCII path and reject a one-byte invalid UTF-8 sequence. |
| E | weekend-db::weekend_db | src/version.rs:6:29– | 311..316 | match |  | The unset and stamped Cargo builds expose the exact respective strings through the loaded C ABI. |
