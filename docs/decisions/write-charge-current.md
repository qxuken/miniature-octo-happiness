# Transaction write charge bound

A new write adds its key bytes, logical value weight and fixed write
overhead. The subtotal uses saturating addition. The existing draft
admission check then adds previously staged bytes with checked addition
and applies the validated transaction-byte cap.

Let H be isize::MAX and M be usize::MAX. A valid transaction-byte cap is
at most H, and M is greater than H. A subtotal below M is represented
exactly. A subtotal that reaches or exceeds M is represented as M.
Starting from zero staged bytes, the cap rejects M. Starting from positive
staged bytes, checked addition rejects M plus those bytes. In either case
the caller reports Limit before copying a write key or growing the overlay.

The bounded helper therefore returns a subtotal and needs no separate
error propagation site. Observation still precedes admission, and a
completed dependency survives a refused write. Both key and value
admission and the write count check still precede the subtotal.

The numeric controls accept the exact transaction cap, reject one byte
over it and check the subtotal just below, at and above M. They also
exercise saturation of the first addition with an out-of-domain
helper input. The real shared-DAG case retains reachable aggregate
overflow through public assignment within its 1 MiB live-byte cap.
Wrapping each subtotal addition separately causes the numeric fixture
to fail. The transaction-cap configuration control detects removal
of the upper-bound check that makes saturation safe.

## Verification at 988cd04

- Rust 1.100.0 debug and release passed 234 tests per profile on macOS arm64
  and 241 on Docker Linux arm64/ext4. The transaction, allocation, retention
  and durability suites and both golden histories passed in every profile.
- Three focused tests passed under Miri, including the numeric subtotal
  bounds, the shared DAG and the real assignment refusal sweep. Strict
  Clippy passed on macOS and for Windows MSVC. Ten mutation-runner tests passed.
- Both macOS profiles detected all 32 controls. The eight
  clean-source reports verify 278 command retirements. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-assign-commit-
  --case r1-alloc-commit-preparation --case memory-transaction-bytes` and
  repeat with `--release`.

The inventory retires the earlier helper propagation operator and
rebinds the two transaction ledgers. It keeps the existing aggregate
charge propagation control and its real overflowing DAG input.
The inventory reconciles 10,581 decisions. The remaining 9,648 open
records include 2,278 runtime records.
