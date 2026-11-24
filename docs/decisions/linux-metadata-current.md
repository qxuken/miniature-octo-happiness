# Linux attribute validation and metadata error propagation

Source and destination attribute lists are validated before attribute
removal or writes. An empty list is valid. Each nonempty list needs its
final terminator and a nonempty name for every entry. The validated
buffer supplies borrowed C strings to the native operations. Names keep
the selected allocator and borrowing them adds no allocation.

The adapter reads source and destination attributes, changes ownership if
needed, applies permissions, copies attributes, then verifies the result.

| Check | Controlled cases | Expected outcome |
| --- | --- | --- |
| Attribute list | Empty, terminated names, missing final terminator, leading or interior empty name | Accept valid lists and reject malformed lists with Io |
| Initial ownership | Matching, UID mismatch alone, GID mismatch alone | Skip chown for a match. Either mismatch requests the source owner once. |
| Metadata read | EIO on source, destination or final read | Stop at that read and preserve CopyMetadata and EIO |
| Ownership callback | Refusal after either initial mismatch | Preserve the refusal and stop before permissions or xattrs |
| Final verification | Full match, UID mismatch alone, GID mismatch alone, mode mismatch alone | Accept the match and reject each mismatch with Io |

The read and comparison fixtures control reported attributes around the
production sequence. They vary one field at a time. Source or destination
read refusal occurs before changing the candidate. A final read can fail
after metadata has changed. Compaction must complete this sequence and
its existing verification before exchange.

The native fixtures call chown, chmod, attribute removal and attribute
write using valid path-only descriptors and verify raw EBADF. A regular
file accepts ownership set to its existing UID and GID. Existing ext4
copy fixtures exercise successful chmod and attribute operations, along
with selected allocation refusal and retry. The pathname architecture
guard and deployed-device evidence retain their separate scope.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/linux.rs` | `512931ec540b580a4cf83706878f73aeae4358e74fefd1e0b15fd74a3fcf0220` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P | weekend-db::weekend_db | src/storage/linux.rs:185:9– | 6370..6372 | if | new | Allow the empty list and terminated nonempty lists. A missing final terminator rejects. |
| C | weekend-db::weekend_db | src/storage/linux.rs:185:12– | 6373..6418 | short_circuit_and | new | Shares the empty and terminator conditions with the enclosing if. |
| P | weekend-db::weekend_db | src/storage/linux.rs:188:9– | 6474..6476 | if | new | Reject leading and interior empty names before native xattr arguments exist. |
| E | weekend-db::weekend_db | src/storage/linux.rs:245:73– | 8186..8187 | try_propagation | copy_metadata_with | Preserve EIO and the CopyMetadata diagnostic on the source read. |
| E | weekend-db::weekend_db | src/storage/linux.rs:246:69– | 8257..8258 | try_propagation | copy_metadata_with | Stop at an EIO on the destination read before making changes. |
| P | weekend-db::weekend_db | src/storage/linux.rs:247:5– | 8264..8266 | if | copy_metadata_with | Either UID or GID mismatch requests the source owner, and matching ownership skips it. |
| C | weekend-db::weekend_db | src/storage/linux.rs:247:8– | 8267..8323 | short_circuit_or | copy_metadata_with | Shares the UID and GID pair with the enclosing if. |
| E | weekend-db::weekend_db | src/storage/linux.rs:248:32– | 8357..8358 | try_propagation | copy_metadata_with | Preserve the configured ownership refusal. The native wrapper reports EBADF on a path-only descriptor. |
| E | weekend-db::weekend_db | src/storage/linux.rs:254:7– | 8648..8649 | try_propagation | copy_metadata_with | A real chmod failure on a path-only descriptor stops before attributes. |
| E | weekend-db::weekend_db | src/storage/linux.rs:257:61– | 8793..8794 | try_propagation | copy_metadata_with | An allocation failure while preparing a source value aborts metadata copy. |
| E | weekend-db::weekend_db | src/storage/linux.rs:258:69– | 8864..8865 | try_propagation | copy_metadata_with | Propagate EIO from the final metadata read after applying the new mode. |
| P | weekend-db::weekend_db | src/storage/linux.rs:259:5– | 8871..8873 | if | copy_metadata_with | UID, GID and mode each independently block acceptance of copied metadata. |
| C | weekend-db::weekend_db | src/storage/linux.rs:259:8– | 8874..8930 | short_circuit_or | copy_metadata_with | The UID and GID prefix is exercised by the final comparison pairs. |
| C | weekend-db::weekend_db | src/storage/linux.rs:259:8– | 8874..8962 | short_circuit_or | copy_metadata_with | Shares all three conditions with the final comparison. |
| E | weekend-db::weekend_db | src/storage/linux.rs:276:15– | 9410..9411 | try_propagation | copy_xattrs | A real xattr removal failure propagates with its CopyMetadata diagnostic. |
| E | weekend-db::weekend_db | src/storage/linux.rs:293:11– | 9923..9924 | try_propagation | copy_xattrs | A real xattr write failure propagates with its CopyMetadata diagnostic. |

## Verification at 6d3d308

- Rust 1.100.0 passed 276 tests per profile on Docker Linux arm64/ext4
  and 265 per profile on macOS arm64, including both golden histories.
  The frozen source was `6d3d30881575bbe59676df2d0bfb44dbc24be203`.
- Both Linux profiles detected all 24 selected controls. The command
  below selects the 17 new controls and the seven moved controls.
  Repeat with `--release` for the second profile.
- The eight Linux decision tests also passed under UID 1000 on ext4 in
  both profiles. Strict Clippy passed on Linux, macOS and for Windows
  MSVC. Eleven mutation-runner tests passed on Linux.
- The inventory maps 16 additional records and rebinds the prior 24.
  It reports 10,583 decisions and 9,474 open records, including 2,105
  runtime. The Linux adapter has 40 of 41 records mapped. Its compile-time
  unsupported-architecture branch still requires its own disposition.

```sh
cargo xtask mutations --toolchain 1.100.0 \
  --case r1-linux-metadata- \
  --case r1-linux-primitive-names-read \
  --case r1-linux-primitive-copy- \
  --case r1-linux-primitive-target-name-loop \
  --case r1-linux-primitive-retain-shared-name \
  --case r1-linux-primitive-source-name-loop
```
