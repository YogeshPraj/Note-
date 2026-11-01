# Project: Note++ (Rust, wxWidgets)

A native Notepad++-style editor: Scintilla and Lexilla for editing, a portable Rust core, a wxWidgets shell.
Branch `wx-port` is the cross-platform shell; `main` shipped the earlier Win32 build.

Read `docs/architecture.md` and `docs/ux.md` before any non-trivial change. Design decisions made without asking are in
`docs/decisions.md`; known lint debt and the audit status are in `docs/audit.md`.

## Layout
- `crates/core` (`notepp-core`): portable logic. Root workspace; `cargo test -p notepp-core`.
- `wx/crates/ui-kit`: themed components, theme tokens, native window services. `wx/crates/app` (`notepp-wx`): the application.
- `wx/` is a separate workspace that needs MinGW GCC, LLVM and the GNU Rust target (see `wx/README.md`).
  On this machine: `PATH=D:\toolchains\mingw64\bin;%USERPROFILE%\.cargo\bin;...`,
  `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=D:\toolchains\mingw64\bin\gcc.exe`, `LIBCLANG_PATH=C:\Program Files\LLVM\bin`.
- `old-src/`, `rust-port*/`, `wx-spike/`, `features/` are legacy or scratch; do not extend them.

## Architecture rules (non-negotiable)
- Direction: `core` <- `app` -> `ui-kit`. `core` imports no UI toolkit; `ui-kit` never imports `core` or the app.
  Run `bash scripts/check-architecture.sh` (Git Bash on Windows; the WSL `bash` stub does not work).
- There is no `syntax` crate: Scintilla and Lexilla highlight; the language registry is in `core`.
- UI components live in `wx/crates/ui-kit/src/components`, one per file. Search the kit before writing a component; extend, do not duplicate.
- Every user action is a command in `core::registry` (id, label, shortcut, enabled rule). Menus, toolbar tool-tips,
  the palette and keybindings are generated from it. Families (recent files, encodings, languages) are `Command`
  variants with a `MenuProvider`. Shortcuts come from `KeyMap` (defaults plus `settings.shortcuts`).
- All text mutations go through the workspace and Scintilla, so they are undoable.

## Code rules
- SOLID: one responsibility per module; extend with a registry row, a handler or a trait impl instead of editing a big `match`.
- No `unwrap()`, `expect()` or `panic!` outside tests (`unwrap_used` is denied). `core` and `ui-kit` use typed errors
  (`notepp_core::error::Error`, `thiserror`); the app uses `AppError`. Never add a `Result<_, String>`.
- No `as` numeric casts outside the adapters: use `notepp_core::num::to_*` (core), `ui_kit::num` (kit),
  `crate::platform::wx` (app). Casts are allowed only in `platform/scintilla.rs`, `platform/wx.rs`,
  `platform/windows.rs` and `ui-kit/native/`.
- `unsafe` only in FFI modules marked `#[allow(unsafe_code)]`; `core` forbids it.
- No hard-coded colours, fonts or text styles: use `ui_kit::theme` tokens (`tests/design_tokens.rs` enforces it).
- Functions at most 60 lines, files at most 400 lines (existing violations are listed in `docs/audit.md`; do not add more).
- Derive or implement `Debug` for every public type. Public functions returning `Result` get a `# Errors` section.
- Long-running work goes off the UI thread with progress and cancel (target; see `docs/architecture.md`, Threading).
- New logic in `core` and pure UI models come with unit tests.

## Workflow
- Changes touching more than 2 files: write a short plan first and wait for approval.
- No new dependencies without asking.
- Decisions the owner delegated (naming, placement, small refactors) go in `docs/decisions.md`, one line each.
- Stop at a numbered checkpoint only when the plan has one; otherwise keep going.
- Testing the UI: use a separate virtual desktop, never steal focus or mouse, never touch the real `%APPDATA%\notepp`
  profile (use a copy via `APPDATA=...`), and close what you open.
- Before declaring done, run `bash scripts/check-all.sh`. It runs, in both workspaces: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets` (errors fail; `STRICT=1` adds `-D warnings`, which does not pass yet because of the lint
  debt), `cargo test`, and `scripts/check-architecture.sh`. New code must not add warnings.

## Pitfalls
- Kill `notepp-wx.exe` before rebuilding (the exe is locked); a running instance blocks test launches (use `--new-instance`).
- PowerShell: backticks inside double-quoted strings are escapes; use the edit tool for code with backticks. Several
  source files have mixed CRLF/LF endings; edit with the edit tool or match both.
- `rustfmt.toml` sets `max_width = 160`; run `cargo fmt --all` in both workspaces.
