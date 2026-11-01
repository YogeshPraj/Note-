#!/usr/bin/env bash
# Fast compile check after an edit (the Claude hook runs this). Silent on success; on failure it prints the
# compiler's errors to stderr and exits 2, which feeds them back to the agent.
cd "$(dirname "$0")/.."
if [ -d /d/toolchains/mingw64/bin ]; then export PATH="/d/toolchains/mingw64/bin:$PATH"; fi
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-D:/toolchains/mingw64/bin/gcc.exe}"
export LIBCLANG_PATH="${LIBCLANG_PATH:-C:/Program Files/LLVM/bin}"
out=$(cargo check --workspace --quiet --message-format=short 2>&1) || { echo "$out" 1>&2; exit 2; }
out=$(cd wx && cargo check --workspace --quiet --message-format=short 2>&1) || { echo "$out" 1>&2; exit 2; }
exit 0