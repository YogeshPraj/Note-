#!/usr/bin/env bash
# Everything a change must pass, in both workspaces. STRICT=1 also denies warnings (both workspaces are at zero).
set -uo pipefail
cd "$(dirname "$0")/.."

# The wx workspace needs the GNU toolchain; fall back to this machine's locations when they are not already set.
if [ -d /d/toolchains/mingw64/bin ]; then export PATH="/d/toolchains/mingw64/bin:$PATH"; fi
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-D:/toolchains/mingw64/bin/gcc.exe}"
export LIBCLANG_PATH="${LIBCLANG_PATH:-C:/Program Files/LLVM/bin}"

deny=""
if [ "${STRICT:-0}" = "1" ]; then deny="-- -D warnings"; fi
fail=0
step() { echo "== $*"; }

for ws in . wx; do
  step "fmt ($ws)";    (cd "$ws" && cargo fmt --all -- --check) || fail=1
  step "clippy ($ws)"; (cd "$ws" && cargo clippy --all-targets $deny) || fail=1
  step "test ($ws)";   (cd "$ws" && cargo test) || fail=1
done
step "whiteboard"; (cd whiteboard && node --test tree/model.test.js) || fail=1
step "architecture"; bash scripts/check-architecture.sh || fail=1

[ "$fail" -eq 0 ] && echo "ALL CHECKS PASSED" || echo "SOME CHECKS FAILED"
exit $fail
