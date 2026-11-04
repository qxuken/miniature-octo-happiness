# Make file-backed commits durable by default

A successful file-backed commit promises survival across process crashes,
operating-system crashes, and power loss under declared filesystem and device
synchronization assumptions. Prefer this default over implicitly trading
acknowledged data for speed; a future weaker mode must expose a separate
contract. The commit and recovery protocol and its failure-injection tests
must be established before claiming that the implementation meets the promise.
