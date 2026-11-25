# Shared TLS credential-file admission

The same tests load temporary files through both shipping CLI binaries.
The file check preserves a missing path as `NotFound`, rejects a
directory, and returns the exact bytes of a file at the test limit.
A zero-byte file and a file one byte over the limit are rejected.

The Unix permission check allows mode `0644` for a public input. The
same file is rejected as a private key. Changing that file to `0600`
allows the private read. The tests use controlled local files and
the documented trusted-directory assumption.

| Decision | Input conditions | Accept or continue |
| --- | --- | --- |
| Path type | Existing regular file | Yes |
| Path type | Existing directory | No |
| Size | Nonempty at the limit | Yes |
| Size | Empty | No |
| Size | Nonempty one byte over the limit | No |
| Unix permission | Public input, group or other permission bits set | Yes |
| Unix permission | Private input, group and other permission bits clear | Yes |
| Unix permission | Private input, group or other permission bits set | No |

The size rows change each rejection term independently. Both terms
are observed false on the accepted boundary. The Unix permission rows
hold the mode constant when switching the input role, then hold the
private role constant when changing the mode. The same pairs cover
the source short-circuit sites. The manifest lists the explicit pairs
and the six error and guard substitutions for each binary.

This batch covers the first metadata call and the file type, size and
Unix mode guards. The later open, opened-handle metadata, handle type
check and read-failure sites remain open. The `live_month` example
compiles the shared module in a separate tooling context.
