<div align="center">

# Note++

### A fast, developer-focused text editor for Windows

*Written in Rust on the Scintilla editing engine. Familiar Notepad++ editing, with live previews, a whiteboard, encrypted files, an HTTP scratchpad and a Bitwarden-backed password inserter built in.*

[![Latest release](https://img.shields.io/github/v/release/YogeshPraj/Note-?label=latest&color=2563eb)](https://github.com/YogeshPraj/Note-/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/YogeshPraj/Note-/total?label=downloads&color=16a34a)](https://github.com/YogeshPraj/Note-/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/YogeshPraj/Note-/build.yml?label=build)](https://github.com/YogeshPraj/Note-/actions/workflows/build.yml)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](#license)

[**Download**](https://github.com/YogeshPraj/Note-/releases/latest) • [Features](#features) • [Build from source](#build-from-source) • [Releases](#releases) • [Report a bug](https://github.com/YogeshPraj/Note-/issues/new?labels=bug&title=%5BBug%5D%20) • [Request a feature](https://github.com/YogeshPraj/Note-/issues/new?labels=enhancement&title=%5BFeature%5D%20)

</div>

---

## About

**Note++ 2.0** is a native Windows editor: quick to start, light on memory, and comfortable for anyone who knows Notepad++. The editing core is [Scintilla](https://www.scintilla.org/) with [Lexilla](https://www.scintilla.org/Lexilla.html) lexers; the window, menus, tabs and dialogs are native Win32. Previews, the whiteboard and the compare view use the system's WebView2.

Version 2.0 replaces the earlier 1.x line. It keeps the same data folder (`%APPDATA%\notepp`), so open tabs, unsaved text, settings and the encryption profile carry over on the first start.

## Features

### Editing
- Multi-tab editing with syntax highlighting for dozens of languages (Language menu), code folding, bookmarks, word wrap, minimap, indent guides and whitespace/EOL symbols.
- Find, Replace, Mark and Find in Files with a results panel; incremental search; regular expressions.
- Encodings the Notepad++ way: *Encode in* / *Convert to*, ANSI code pages, UTF-8 (with or without BOM), UTF-16; line endings (CRLF / LF / CR).
- Sessions: tabs and unsaved text survive restarts and crashes; Load / Save Session; Recent Files.
- Notepad++ default shortcuts, a **Shortcut Mapper**, a **Style Configurator** with built-in themes (Monokai, Dracula, Solarized and more), light and dark mode, and an editor for the right-click menu.
- Command Palette (`Ctrl+Shift+P`), Quick Open (`Ctrl+P`), Print Preview, Page Setup, distraction-free mode.
- Tabs you can pin and colour, with Close All / Close Others / Reveal in File Tree / Copy Full Path from the right-click menu.
- Folder as Workspace tree with a one-click close button.

### Previews
- **Markdown** and **HTML** live preview (`Ctrl+Shift+V`), with Mermaid diagrams rendered inside Markdown.
- **JSON** tree view and editor; **Mermaid** (`.mmd`) diagram preview.
- **Log Follow** mode for `.log` / `.out` / `.err`: live tail with level filters (ERROR / WARN / INFO / DEBUG), text or regex search, stack traces kept with their line.

### Tools
- **Compare**: two files, or a file against the clipboard, with a side-by-side diff.
- **Whiteboard**: a hand-drawn canvas (Excalidraw) in its own tab; saved as JSON.
- **Encrypted pad**: per-file AES-256-GCM encryption with a password and a recovery key.
- **HTTP scratchpad**: write requests in a `.http` file and send them with `Ctrl+Enter`.
- **Passwords**: insert secrets from your Bitwarden vault (through the official Bitwarden CLI) without copying them to the clipboard.
- Text tools: hashes (MD5, SHA-1, SHA-256, ...), Base64, case conversion, sort, line operations.

### Updates
- Checks GitHub for a newer release and downloads it in the background. A toolbar notice turns green when it is ready; one click installs it and restarts. Downloads are verified against a published SHA-256 before they are used.

## Install

Download the latest `NotePP-windows-x64.zip` from [Releases](https://github.com/YogeshPraj/Note-/releases/latest), unzip it anywhere and run `sci_probe.exe`. Requires Windows 10 or 11 (64-bit) with the WebView2 runtime, which ships with Windows 11 and current Windows 10.

Data is kept in `%APPDATA%\notepp`.

## Build from source

Prerequisites: Windows, [Rust](https://rustup.rs/), MinGW-w64 with the UCRT runtime (`winget install BrechtSanders.WinLibs.POSIX.UCRT`) and Node.js 18+ (only to fetch Mermaid and Excalidraw). The Rust toolchain is pinned to `stable-x86_64-pc-windows-gnu` in `rust-toolchain.toml`.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1   # one time; safe to re-run (-Force rebuilds)
cargo run                                                    # or: cargo test
```

`scripts/setup.ps1` does what the repository deliberately does not store:

1. Downloads **Scintilla 5.5.2** and **Lexilla 5.5.4** (checked against pinned SHA-256 hashes) and builds them with MinGW.
2. Restores `assets/mermaid.min.js` and the Excalidraw whiteboard bundle and fonts, built from the project in `old-src/` (`npm ci` and `build-whiteboard.js`).

`build.rs` links Lexilla statically and stops with a pointer to the setup script if anything is missing. Native crashes are written to `%APPDATA%\notepp\crash.log`.

## Releases

Every push and pull request is built and tested by GitHub Actions ([`build.yml`](.github/workflows/build.yml)); the packaged zip is attached to each run as an artifact. Pushing a tag `vX.Y.Z` that matches the version in `Cargo.toml` also publishes a GitHub release with:

- `NotePP-windows-x64.zip`: the app folder (`sci_probe.exe`, `Scintilla.dll`, `WebView2Loader.dll`, `whiteboard/`)
- `NotePP-windows-x64.zip.sha256`: its SHA-256, which the in-app updater checks

To build the same files locally:

```powershell
powershell -File scripts\package.ps1    # builds dist\NotePP
powershell -File scripts\release.ps1    # dist\release\NotePP-windows-x64.zip + .sha256
```

## Project layout

```
src/core/        portable logic: editor core, tabs, sessions, encodings, search, crypto, updater
src/platform/    the Win32 shell: window, menus, tab strip, dialogs, WebView2 panes
assets/          icons, preview pages, bundled Mermaid / jsoneditor / whiteboard
scripts/         setup, package and release scripts
tests/           fixtures
old-src/         the previous 1.x project, kept for reference and as the source of the bundled web assets
```

Gotchas worth knowing when working on the editor core:
- `SCI_SETPROPERTY` (e.g. `fold`) is forwarded to the lexer; set it *after* `SCI_SETILEXER` or it is dropped.
- Lexilla lexers number their styles independently, so each language needs its own palette.

## Security

- The executable only loads DLLs from `System32` and its own folder.
- Document previews do not fetch images from the web unless you turn that on in the settings.
- Updates are downloaded over HTTPS and verified against a SHA-256 checksum before they are unpacked. The checksum catches damaged or swapped files; it is not a code signature, because releases are not signed yet.
- Encrypted files use AES-256-GCM with keys derived from your password (PBKDF2); unsaved text of an encrypted tab is never written to the session file as plain text.

If you find a vulnerability, please open a private security advisory on GitHub rather than a public issue.

## License

MIT. Scintilla and Lexilla are by Neil Hodgson and are used under the HPND license.

---

<div align="center">

Created by [**Yogesh Prajapati**](https://github.com/YogeshPraj)

[Report a bug](https://github.com/YogeshPraj/Note-/issues) • [Request a feature](https://github.com/YogeshPraj/Note-/issues) • [Releases](https://github.com/YogeshPraj/Note-/releases)

</div>
