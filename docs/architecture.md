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
  command. Ids are the numeric ids in `commands.rs`; `Command` gives the frequent ones a typed name.
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

Message boxes and the system pickers (files, folder, one of a list, colour, font) are built only by `ui_kit::ask`.
A dialog that needs state between its callbacks keeps it in its form (`Form::set_state`), not in a static; the few
deliberate globals are listed in `docs/audit.md`.

## Floating tabs

Any tab can leave the strip for a window of its own, with its own Scintilla view or its own whiteboard or picture page (`workspace/floats.rs`, `app/float_pane.rs`),
any number of them. Floating tabs stay in `Workspace::tabs`, always at the end: the strip draws the first `main_count()`, so its
indices are still `tabs` indices and Save All, the session and closing see every tab without knowing. As with the dock, `sci` and
`active` mean where the user is: going into a window swaps its view into `sci`, leaves the main view in the window's slot, and
`in_float` keeps the tab the main window shows (`main_index()`). The windows have no parent, so they minimise on their own, and no
menu bar, so `wire_keys` turns keys into commands through `KeyMap`. `sync_floats` (from `refresh`) closes windows whose tab has
gone. A window is only ever destroyed after the workspace has let go of its view: the close event is vetoed, and `sync_floats`
does it.
A floating whiteboard has no view in the workspace, so `sci` stays on the main view and `is_page()` is true while the main window
shows text: the main window goes by `main_is_page()`, and commands by `is_page()`. Its last strokes are in the page, so undocking,
Dock Back and closing ask for the scene first (`wb-get-data`) and act when it arrives. Scenes are applied to a tab by id, never
to the active tab, since the page sending one need not be where the keyboard is.

## Whiteboard page

The Excalidraw page is built from `whiteboard/` by `old-src/build-whiteboard.js` (its packages are still in `old-src`). A tree on the
whiteboard (`whiteboard/tree/`) is real Excalidraw shapes in one group, with the tree itself in the frame's `customData`:
`model.js` is the data and has no toolkit in it (`node --test`), `scene.js` draws it and watches the mouse, `menu.jsx` is the
right-click menu, `library.js` the built-in Tree and Agent items, which are never saved with the user's library.

Shapes are found by the `key` in their `customData` inside the group, never by id: copying a tree or dragging it from the library
gives every shape a new id. For undo, a redraw keeps each shape's id with a higher version and a new nonce, marks what goes as
deleted rather than dropping it, and passes `CaptureUpdateAction.IMMEDIATELY`. A click kept from Excalidraw, and the menu closing,
must give the canvas the keyboard back, or Ctrl+Z never reaches it. A line must be given its width and height: left alone it
draws correctly but measures 100 wide.

## Docked tab

One tab can sit in a second Scintilla view beside the main one, both editable (`workspace/dock.rs`, `app/dock_pane.rs`).
`Workspace::sci` and `active` always mean the view the user is in and its tab, so commands, the status bar and the
preview follow the keyboard without knowing about docking: moving between views swaps them, and the other view waits in
`second`. The docked tab is fixed: any other tab opens in the main view, new tabs included. The app reads the workspace
in `sync_dock` (called from `refresh`) to show, move or hide the pane; `App::stc()` is the view the user is in. A
whiteboard in the main view hides both editors.
A docked whiteboard or picture has its own page (`app/dock_board.rs`, one of each kind), made the first time one docks; `second` stays on its empty document and
nothing is swapped, so `active` alone says which side the user is on. The toolkit hears nothing of a click in a page, so the page
reports it (`wb-focus`, `focus`).

## MCP server

An AI agent works in the running app through MCP instead of sending it keystrokes. The server is part of `notepp.exe` and is off
until the user turns it on under *Settings > AI Agents*. It listens on 127.0.0.1, on a fixed port the user can change, and speaks
Streamable HTTP with plain JSON replies; there are no event streams, and GET answers 405, which the protocol allows.

- `core::mcp`: `http` (one request, one reply, and the `gate`: any `Origin` is refused, `Host` must be local, the token must match),
  `protocol` (JSON-RPC), `tools` (the table: name, description, input, level), `server` (the listener and the `Queue`), `access`
  (levels, the token, which paths can be reached), `text` (line and match work with no editor in it).
- `Workspace::agent_call` (`workspace/agent.rs`, `agent_edit.rs`) carries out a tool. It reaches tabs through `with_doc`, so the
  user is not moved off the tab in front; a tab that is not in a view gets its modified mark set by hand, as the hidden view tells
  nobody. Each change is one undo step. `Headless` stands in for `Ui`: nothing an agent does can open a dialog, and what would
  have been a question is a refusal.
- `app/mcp.rs` joins them. Connection threads never touch a document: a tool call waits on the `Queue`, and a 50 ms timer, which
  runs only while the server is on, carries it out on the UI thread. When a dialog holds the app the call waits, up to 15 seconds;
  a call nobody is waiting for any more is dropped rather than run, so an edit reported as failed cannot happen later.

To add a tool: a row in `mcp::tools::TOOLS` and a handler in `agent_call`. A test fails if a row has no handler.

The token is in a file of its own in the profile (`mcp-token`), not in `settings.json`, which may be shared or backed up. Agents reach
only tabs and the workspace folder, never the profile; network paths are refused before the disk is touched, since looking at one
sends the user's credentials to its host.

## Threading

wxWidgets is single-threaded: all UI is on the main thread. Work that can take long runs on a worker thread and hands
its result back through a shared slot that the half-second timer (`wire_timers`, the Stream job) polls. Two patterns:

- **Modeless, polled:** the worker reports progress through atomics and leaves its outcome in an `Arc<Mutex<Option<_>>>`.
  Find in Files (`in_files.rs`, `search::SearchProgress`, its button becomes Stop), the Quick Open file listing
  (`files::FileIndex`), folder reads of the workspace tree (`tree_reader.rs`), the session autosave write
  (`Session::save_in_order`, tickets keep writes in order), the update check and download, HTTP requests.
- **Modal with progress:** `ui_kit::progress::run` runs a job on a worker and, after 300 ms, shows a progress window
  whose Cancel the job sees (`Progress::cancelled`). Hashing files, reading files over 16 MB (`Ui::read_file`).
  The Passwords dialog's `work` loop is the same idea inside its own form; Cancel ends a `bw` call or download
  (`bitwarden::cancel`).

Still on the UI thread: putting text into Scintilla (it must be), Find All in the current document (its result list is
virtual, so 20,000 hits list in milliseconds), and Log Follow polls, which read at most 512 KB each.

The Settings window is data-driven: the app builds `Page`/`Row` values from `Settings` (`settings_window.rs`) and gets each change back as a key and a `Value`, which a pure `apply_value` writes into `Settings`.

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

`scripts/check-all.sh` runs: `cargo fmt --check`, clippy in both workspaces, all tests and the architecture check
(layer dependencies, native dialogs only in `ui_kit::ask`, no file over 400 lines).
Lints are in each workspace's `[workspace.lints]`; `unwrap_used` and `dbg_macro` are denied. Known lint debt is in
`docs/audit.md`; design decisions taken without asking are in `docs/decisions.md`.
