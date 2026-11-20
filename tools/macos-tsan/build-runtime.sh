#!/bin/bash
set -euo pipefail

[[ "$(uname -s)" = Darwin && "$(uname -m)" = arm64 ]]
root="$(cd "$(dirname "$0")" && pwd -P)"
mkdir "${1:?pass a fresh output directory}"
output="$(cd "$1" && pwd -P)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/weekend-tsan-runtime.XXXXXXXX")"
trap 'rm -r "$scratch"' EXIT

llvm=0a5579eb649d3634a9c30ac873300cc2944fa070
tls_patch="$root/darwin-tls.patch"
printf '%s  %s\n' 9e97d82ab249fd11b14b9380bf355804be69e8450fe669b7fe9dab35e90a7c22 "$tls_patch" \
  > "$output/inputs.sha256"
shasum -a 256 -c "$output/inputs.sha256"

source="$scratch/source"
llvm_paths=(compiler-rt cmake llvm/cmake llvm/include llvm/utils/gn llvm/CMakeLists.txt third-party LICENSE.TXT)
printf 'Fetching LLVM source at %s\n' "$llvm"
(
  export GIT_TERMINAL_PROMPT=0 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
  mkdir "$source"
  cd "$source"
  git init --quiet --template=
  git config core.autocrlf false
  git config http.lowSpeedLimit 1024
  git config http.lowSpeedTime 40
  git remote add origin https://github.com/rust-lang/llvm-project.git
  printf '/%s\n' "${llvm_paths[@]}" | git sparse-checkout set --no-cone --stdin
  git -c protocol.version=2 fetch --no-tags --depth=1 --filter=blob:none origin "$llvm"
  git checkout --quiet --detach FETCH_HEAD
  [[ "$(git rev-parse HEAD)" = "$llvm" ]]
  printf '%s\n' "$llvm" > "$output/llvm-revision.txt"
  # Match the SHA-256 manifest of the selected files from the pinned archive.
  [[ -z "$(find "${llvm_paths[@]}" ! -type f ! -type d -print -quit)" ]]
  find "${llvm_paths[@]}" -type f -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 \
    > "$output/llvm-source.sha256"
  printf '%s  %s\n' 9458ecabc08a5c66c664b554d8e044f7dd6c17c2930389e1b5dee9707069d231 "$output/llvm-source.sha256" \
    | tee -a "$output/inputs.sha256" | shasum -a 256 -c
)
rm -r "$source/.git"
shasum -a 256 "$root/errno-shadow.patch" >> "$output/inputs.sha256"
(
  cd "$source"
  git apply "$tls_patch"
  git apply "$root/errno-shadow.patch"
)

sdk="$(xcrun --sdk macosx --show-sdk-path)"
{
  sw_vers
  /usr/bin/clang --version
  xcrun --sdk macosx --show-sdk-version
  cmake --version
  ninja --version
} > "$output/build-tools.txt"
cmake -S "$source/compiler-rt" -B "$scratch/build" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_C_COMPILER=/usr/bin/clang -DCMAKE_CXX_COMPILER=/usr/bin/clang++ \
  -DCMAKE_OSX_SYSROOT="$sdk" -DCMAKE_OSX_ARCHITECTURES=arm64 \
  -DCOMPILER_RT_BUILD_BUILTINS=OFF -DCOMPILER_RT_BUILD_SANITIZERS=ON \
  -DCOMPILER_RT_SANITIZERS_TO_BUILD=tsan -DCOMPILER_RT_INCLUDE_TESTS=OFF \
  -DCOMPILER_RT_BUILD_XRAY=OFF -DCOMPILER_RT_BUILD_LIBFUZZER=OFF \
  -DCOMPILER_RT_BUILD_PROFILE=OFF -DCOMPILER_RT_BUILD_CTX_PROFILE=OFF \
  -DCOMPILER_RT_BUILD_MEMPROF=OFF -DCOMPILER_RT_BUILD_ORC=OFF \
  -DCOMPILER_RT_BUILD_GWP_ASAN=OFF > "$output/configure.log" 2>&1
ninja -C "$scratch/build" -j 8 clang_rt.tsan_osx_dynamic > "$output/build.log" 2>&1
lib=libclang_rt.tsan_osx_dynamic.dylib
/usr/bin/lipo "$scratch/build/lib/darwin/$lib" -thin arm64 -output "$output/$lib"
/usr/bin/codesign --verify "$output/$lib"
(cd "$output" && shasum -a 256 "$lib" > runtime.sha256)
cp "$source/compiler-rt/LICENSE.TXT" "$output/LICENSE-LLVM.TXT"
