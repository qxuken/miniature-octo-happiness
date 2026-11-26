# Follower snapshot decoder error propagation

The follower opens its private database before reading the selected image.
The fixture first installs a snapshot whose current value is 2 and whose
retained revision 2 holds value 1. It records the durable receipt and both
files, closes the follower, then arms the selected image's first I/O to fail.

| Decode limits | First image I/O | Caller sees |
| --- | --- | --- |
| Zero maximum depth | Never reached | Invalid configuration |
| Zero live bytes | Never reached | Resource limit |
| Default | Fails on the first operation | I/O error |
| Default, fault cleared | Read-only recovery succeeds | Original durable receipt, current value 2 and retained value 1 |

The fixture checks that the selected image stays byte-identical after every
refusal. After retry it checks the manifest bytes, selected image bytes,
receipt, current value and retained view. Each mutation replaces one
propagated error at the decoder boundary. These checks exercise the two
error returns in `Database::decode_replica`.
