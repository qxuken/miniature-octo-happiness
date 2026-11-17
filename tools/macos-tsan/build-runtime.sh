#!/bin/bash
set -euo pipefail

[[ "$(uname -s)" = Darwin && "$(uname -m)" = arm64 ]]
root="$(cd "$(dirname "$0")" && pwd -P)"
mkdir "${1:?pass a fresh output directory}"
output="$(cd "$1" && pwd -P)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/weekend-tsan-runtime.XXXXXXXX")"
trap 'rm -r "$scratch"' EXIT

llvm=0a5579eb649d3634a9c30ac873300cc2944fa070
tls_fix=4a3ac30a53ba0417941cab04f51c6ce8a8a67feb
input="$scratch/rust-llvm.tar.gz"
tls_patch="$scratch/darwin-tls.patch"
curl -fsSL --retry 2 --max-time 180 --max-filesize 350000000 \
  "https://codeload.github.com/rust-lang/llvm-project/tar.gz/$llvm" -o "$input"
curl -fsSL --retry 2 --max-time 30 --max-filesize 100000 \
  "https://github.com/llvm/llvm-project/commit/$tls_fix.patch" -o "$tls_patch"
printf '%s  %s\n' \
  7091936a040073d4ac7d9a150f558c1e488eadd8ae3565b654fed3b7be17a576 "$input" \
  9e97d82ab249fd11b14b9380bf355804be69e8450fe669b7fe9dab35e90a7c22 "$tls_patch" \
  > "$output/inputs.sha256"
shasum -a 256 -c "$output/inputs.sha256"
shasum -a 256 "$root/errno-shadow.patch" >> "$output/inputs.sha256"

source="$scratch/source"
mkdir "$source"
tar -xzf "$input" -C "$source" --strip-components 1 \
  "llvm-project-$llvm/compiler-rt" "llvm-project-$llvm/cmake" \
  "llvm-project-$llvm/llvm/cmake" "llvm-project-$llvm/llvm/include" \
  "llvm-project-$llvm/llvm/utils/gn" "llvm-project-$llvm/llvm/CMakeLists.txt" \
  "llvm-project-$llvm/third-party" "llvm-project-$llvm/LICENSE.TXT"
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
