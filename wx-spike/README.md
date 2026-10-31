# wx-spike

Feasibility spike for a wxWidgets-based Note++ shell (branch `wx-port`): a native window with a
`wxStyledTextCtrl` (Scintilla + Lexilla bundled in wxWidgets), driven through raw Scintilla
messages the same way `src/core` talks to Scintilla.

It proves: the stack builds and runs on Windows, raw messages work through the direct
function/pointer, documents can be created and swapped per tab, and `CreateLexer` (Lexilla) is
available from the wx libraries.

## Building

wxDragon downloads prebuilt wxWidgets libraries and needs the **same** compiler they were built with
and libclang for its bindings:

```powershell
# WinLibs GCC 15.1.0 UCRT (must match; a newer GCC fails to link), e.g. unpacked to D:\toolchains\mingw64
$env:PATH = "D:\toolchains\mingw64\bin;$env:PATH"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "D:\toolchains\mingw64\bin\gcc.exe"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"      # winget install LLVM.LLVM
cd wx-spike
cargo run
```

`vendor/wxdragon-sys` is a copy of the upstream crate (MIT) with two added functions,
`wxd_StyledTextCtrl_GetDirectFunction` and `wxd_StyledTextCtrl_GetDirectPointer`, which upstream does
not expose. It is wired in through `[patch.crates-io]` in `Cargo.toml`.
