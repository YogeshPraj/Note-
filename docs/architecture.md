# Architecture

Note++ is a native text editor: Scintilla and Lexilla do the editing and highlighting, a portable Rust core owns the
document logic, and a wxWidgets shell (through the `wxdragon` crate) draws the window. This page is the map; read it
before any non-trivial change.

## Crates

Two Cargo workspaces, because the wxWidgets build needs a different toolchain (MinGW GCC, libclang, the GNU Rust target)
than the portable core.

```
<repo>/Cargo.toml          workspace 1 (plain cargo)
  crates/core              notepp-core   no UI, no OS code
wx/Cargo.toml              workspace 2 (see wx/README.md for the toolchain)
  crates/ui-kit            ui-kit        themed components; no editor or business logic
  crates/app               notepp-wx     the application; composes core and ui-kit
vendor/wxdragon-sys        patched glue (adds Scintilla's direct function and pointer)
```

A `syntax` crate does not exist: Scintilla and Lexilla highlight, and the text lives in Scintilla, so there is no buffer
or token stream to extract. The language registry (`lang.rs`, `lexer.rs`, `functions.rs`, `complete.rs`) is in `core`.

### Dependency rules

| Crate | May depend on | Must not depend on |
|---|---|---|
| `notepp-core` | std, pure libraries (serde, regex, crypto, HTTP client) | any UI toolkit (`wxdragon`, ...), `ui-kit`, the app |
| `ui-kit` | `wxdragon`, `wxdragon-sys`, `serde_json` | `notepp-core`, the app |
| `notepp-wx` (app) | `notepp-core`, `ui-kit`, `wxdragon` | nothing (it is the top) |

`scripts/check-architecture.sh` enforces this from `cargo tree` and a source scan. The kit cannot name the core, so the
app converts between their types (for example `TabInfo` to `ui_kit::tab_strip::Tab`) and gives the kit hooks instead of
imports (`form::PROBLEM_SINK`).

### What lives where

| Area | Crate | Notes |
|---|---|---|
| Workspace (tabs, documents, sessions, recent files), search, edit operations, encodings, crypto, Bitwarden, HTTP client, updater | core | `Workspace<S>` is generic over the editor (`sci::Send`), so it is testable with a fake |
| Command registry, command families, key map, menu providers | core (`registry`, `menu_provider`, `keymap`) | pure data and rules |
| Command dispatch for commands that need only the workspace | core (`dispatch`) | `command_core` |
| Typed errors | core (`error`) | one `Error` enum, `thiserror` |
| Checked numeric conversions | core (`num`), ui-kit (`num`), app (`platform::wx`) | casts are allowed only in the adapters |
| Theme tokens and hub, components, native window services | ui-kit | `theme`, `components/`, `native/` |
| Window, chrome, dialogs, panels, handlers, platform actions | app | `app.rs`, `chrome.rs`, `command.rs`, `handlers.rs`, `platform/` |

## The command system

Every user action is a command: a menu item, a toolbar button, a shortcut, a palette entry.

- **Registry** (`core::registry`): `CommandSpec { id, menu, palette, shortcut, menu_key, enabled }`, one row per fixed
  command. Ids are the numeric Notepad++-compatible ids in `commands.rs`; `Command` gives the frequent ones a typed name.
- **Families** (`Command::OpenRecent(i)`, `EncodeIn`, `ConvertTo`, `SetCharset`, `SetLanguage`): one command per item of a
  list built from state. A `MenuProvider` (or `GroupedMenuProvider`) turns the state into `MenuItemSpec`s; the menu code
  only lays them out.
- **Key map** (`core::keymap`): registry defaults plus `settings.shortcuts` overrides. A clash is resolved (a command on
  its default keeps it; between overrides the lower id wins; the loser keeps its default) and reported.
- **Generation:** `B::cmd(menu, ID)` takes the text and shortcut from the registry and the key map; toolbar tool-tips and
  the palette do the same. The menu bar has a snapshot test (`wx/crates/app/tests/menus.golden`).
- **Execution:** `command::execute(app, id)` checks the registry's enabled rule against `CommandState::of(&ws)` and
  returns `AppError::Disabled` (ignored silently) or `AppError::NotAvailable`; otherwise it tries `core::dispatch`
  first and then the chain of registered handlers (`CommandBus`: view, panels, updates, streams, extra, appearance,
  help). Adding a feature is a handler or a registry row, not an edit of a big `match`.
- **State to UI:** `App::sync_command_state` greys menu items and toolbar buttons whose rule depends on the editor
  (Undo, Redo, Cut, Copy, Delete, Reload), only when the state changed.

## Data flow

```
input (menu, toolbar, key, palette)
  -> command id -> Command / spec
  -> execute(): enabled? -> core::dispatch or a handler
  -> core mutates the Workspace (text changes go through Scintilla's undo stack)
  -> App::refresh(): title, tabs, status bar, menu ticks, command state
  -> components repaint from the theme
```

Text mutations are done through the workspace, which sends Scintilla messages, so every edit is undoable.

## Theme and components

`ui-kit/src/theme.rs` is the one source of colours, spacing, radii and fonts. Components read it (custom-painted ones
through `Look`, native ones through `theme::adopt`), and a test fails the build if a colour or font is written anywhere
else. See `docs/ux.md` for the visual rules and `crates/ui-kit/examples/gallery.rs` for every component on screen.

## Threading

wxWidgets is single-threaded: all UI is on the main thread. Reality today:

- Update download and extract run on a worker thread and report through a shared slot polled by a timer.
- Bitwarden commands run on the main thread inside a `SafeYield` loop so the dialog repaints.
- Find in Files, the file-tree scan and log follow still run on the UI thread (audit finding 18). Moving them off the
  thread with progress and cancel is the target, tracked as UX debt.

## Platform differences

`app::platform` holds OS actions (reveal in the file manager, terminal, open with, recycle, local time) behind a trait
with Windows and Unix implementations. `ui_kit::native` holds window services (dark title bar, RTL, topmost, showing a
popup without taking the focus, a thread-wide key grab). Menus are not native: `ui_kit::popup_menu` draws every popup
(menu-bar menus, tab, editor and status-bar context menus) from the theme tokens, because the operating system's menus
cannot follow the application theme (on current Windows builds the dark popup is half light whatever mode is requested).
The popup never takes the focus: it holds the mouse so a click elsewhere dismisses it, and on Windows a keyboard hook
(`native::grab_keys`) feeds it the keys; other platforms route keys from the frame's char hook. Enabled and ticked state
is read from the native menu bar, which stays attached as the state store and for accelerators. Unsafe code is allowed
only in those modules, `ui-kit::components` controls (a trait method wxdragon makes unsafe) and the Scintilla adapter;
core forbids it.

## Build and quality gates

`scripts/check-all.sh` runs: `cargo fmt --check`, clippy in both workspaces, all tests and the architecture check.
Lints are in each workspace's `[workspace.lints]`; `unwrap_used` and `dbg_macro` are denied. Known lint debt is in
`docs/audit.md`; design decisions taken without asking are in `docs/decisions.md`.
