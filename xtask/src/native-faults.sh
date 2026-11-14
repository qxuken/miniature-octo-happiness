set -eu
uname -a
rustc -Vv
findmnt -n -o FSTYPE,SIZE,OPTIONS /faults
test "$(findmnt -n -o FSTYPE /faults)" = tmpfs
cargo build --offline --locked --release --manifest-path /source/Cargo.toml --example native_faults --target-dir /build
sha256sum /build/release/examples/native_faults
mkdir /faults/db /faults/denied
chmod 700 /faults/denied
setpriv --reuid=10001 --regid=10001 --clear-groups /build/release/examples/native_faults permission /faults/denied
test ! -e /faults/denied/database.wdb
WEEKEND_PRIVATE_TMPFS=1 /build/release/examples/native_faults diskfull /faults/db
printf 'NATIVE_FAULTS_COMPLETED\n'
