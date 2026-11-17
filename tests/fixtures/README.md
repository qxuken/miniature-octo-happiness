# Compatibility fixtures

`pre1-v2.wdb` was archived from the seed committed at `1e883c8`.
SHA-256 is `339be00b9b43f53a3e67adb5f53ff2be3d0eb372a19f66122965d7a75d72db18`.
It uses byte format 2, selected revision 4 and retained revision 2.
Writer and generator are recorded in that commit under `fuzz/src/lib.rs`.
Do not regenerate this fixture with a new encoder.

`tests/compatibility.rs` verifies reading, updating, power-loss reopening,
copying and retained rows. Historical-reader/new-writer directions and
future format transitions each require their own archived reader and test.
