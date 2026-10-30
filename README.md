# Note++

A fast, developer-focused text editor for Windows, written in Rust on the Scintilla editing component and
Lexilla lexers. Version 2.0.

## Build (Windows, GNU toolchain — the MSVC install on this machine has no `lib\x64`)

Prerequisites: MinGW-w64 (`winget install BrechtSanders.WinLibs.POSIX.UCRT`), Rust, and Node.js 18+
(only to fetch Mermaid and Excalidraw). The Rust toolchain is pinned to `stable-x86_64-pc-windows-gnu`
in `rust-toolchain.toml`; the WebView2 runtime ships with Windows 10/11.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1   # one time; safe to re-run (-Force rebuilds)
cargo build
cargo run
```

`scripts/setup.ps1` does what the repository deliberately does not store:

1. Downloads **Scintilla 5.5.2** and **Lexilla 5.5.4** from scintilla.org and builds them with MinGW,
   passing `CXXFLAGS="-include cstdint"` (GCC 16 no longer pulls `<cstdint>` in transitively, so the
   sources fail without it).
2. Restores `assets/mermaid.min.js`, `assets/whiteboard/whiteboard.bundle.{js,css}` and the Excalidraw
   fonts from the build in `old-src/` (running `npm ci --ignore-scripts` and
   `build-whiteboard.js` there if they have not been produced yet).

`build.rs` links Lexilla statically, copies `Scintilla.dll` next to the executable, and stops with a
pointer to the setup script if anything above is missing.

Native crashes (access violations, not just panics) are written to `%APPDATA%\notepp-native\crash.log`.

## Release

```powershell
powershell -File scripts\package.ps1    # builds dist\NotePP
powershell -File scripts\release.ps1    # dist\release\NotePP-windows-x64.zip + .sha256
```

Publish a GitHub release tagged `v<version>` (the version in `Cargo.toml`) with both files attached;
the app's updater offers it to older installs. `old-src/` holds the previous version of the project.

## Layout

- `src/core/` — portable: Scintilla messages (`sci`), languages (`lang`, `lexer`), encodings (`doc`),
  tabs/open/save/close/sessions (`workspace`), session file (`session`).
- `src/platform/win32.rs` — the only OS-specific file: window, menu, tab strip, status bar, dialogs.

Gotchas learned the hard way:
- `SCI_SETPROPERTY` (e.g. `fold`) is forwarded to the lexer; set it *after* `SCI_SETILEXER` or it is dropped.
- `SC_MASK_FOLDERS` is `0xFE000000` (as i32), not `-0x2000_0000`.
- Lexilla lexers number their styles independently — each language needs its own palette.
- `SCI_SETCARETLINEBACK` takes the colour in `wParam`.
