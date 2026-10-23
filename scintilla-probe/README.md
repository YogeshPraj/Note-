# Note++ native (Scintilla + Lexilla spike)

## Build (Windows, GNU toolchain — the MSVC install on this machine has no `lib\x64`)

1. MinGW-w64 (winget `BrechtSanders.WinLibs.POSIX.UCRT`) on `PATH`.
2. Build Lexilla and Scintilla with GCC 16 — it needs the extra flag:

   ```
   CXXFLAGS="-include cstdint"
   ```

   (GCC 16 no longer pulls `<cstdint>` in transitively; Scintilla/Lexilla sources fail without it.)
3. `Scintilla.dll` must sit next to `sci_probe.exe`; Lexilla is linked statically from `lexilla/bin/liblexilla.a`.
4. `cargo build` (the toolchain is pinned to `stable-x86_64-pc-windows-gnu` in `rust-toolchain.toml`).

## Layout

- `src/core/` — portable: Scintilla messages (`sci`), languages (`lang`, `lexer`), encodings (`doc`),
  tabs/open/save/close/sessions (`workspace`), session file (`session`).
- `src/platform/win32.rs` — the only OS-specific file: window, menu, tab strip, status bar, dialogs.

Gotchas learned the hard way:
- `SCI_SETPROPERTY` (e.g. `fold`) is forwarded to the lexer; set it *after* `SCI_SETILEXER` or it is dropped.
- `SC_MASK_FOLDERS` is `0xFE000000` (as i32), not `-0x2000_0000`.
- Lexilla lexers number their styles independently — each language needs its own palette.
- `SCI_SETCARETLINEBACK` takes the colour in `wParam`.
