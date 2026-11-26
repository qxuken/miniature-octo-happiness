# Native Windows path entry

The Windows x64 job at `c44c26daf231503b275a5559943439ee879598c0` ran the native path and file
suites in debug and release. The job checked out `5c07f7eee961037ba7c44037582e2b23e2ee1185`.
Its tree matched the pushed head. Both profiles passed the drive and
early path guard test and the native open and reopen regression.
The latter preserves the original stage and OS error while checking
the current and retained roots.

The native step passed 238 tests in debug and
238 in release. This covers the six remaining
runtime path-entry records with actual Windows execution. The compiler,
generated-code and tooling requirements retain their own scope.
[Hosted Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45756615197/job/139696612782)
[Hosted evidence checksums](windows-native-path-entry-c44c26d.sha256)

| Source | SHA-256 |
| --- | --- |
| `src/storage/windows.rs` | `e15440da569ded3c2da401382ca1cc0895f52df7cc83a8a3980e971b7cf52710` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| D | weekend-db::weekend_db | src/storage/windows.rs:63:5– | 1945..1950 | match | drive | A relative path has no drive prefix. Both DOS drive spellings return the same drive letter. UNC and device prefixes reject. |
| D | weekend-db::weekend_db | src/storage/windows.rs:64:44– | 2021..2026 | match | drive | The native prefix matcher accepts both drive spellings and refuses other prefix types. |
| E | weekend-db::weekend_db | src/storage/windows.rs:206:20– | 6601..6602 | try_propagation | open_impl | The factory preserves UnsupportedPlatform for UNC and device prefixes. Fixed local paths reach later admission in the native file integration suite. |
| E | weekend-db::weekend_db | src/storage/windows.rs:207:54– | 6657..6658 | try_propagation | open_impl | Embedded NUL and invalid trailers return Invalid. The original UTF-16 path at the cap returns Limit. |
| E | weekend-db::weekend_db | src/storage/windows.rs:208:58– | 6717..6718 | try_propagation | open_impl | The drive-only C: path has no file name and returns Invalid. Ordinary file names reach the native factory. |
| E | weekend-db::weekend_db | src/storage/windows.rs:209:42– | 6761..6762 | try_propagation | open_impl | A valid-length path with a forbidden leaf character returns Invalid. The file suite exercises accepted leaf names on NTFS. |
