# Note++ — wxWidgets shell

Native shell for Windows, macOS and Linux on top of the portable `notepp-core` crate.

## Layers

| Layer | Where | Rule |
|---|---|---|
| Domain / services | `crates/core` | No UI toolkit. Search, crypto, Bitwarden, HTTP, compare, settings, `Workspace<S>`. |
| Ports (traits) | `notepp_core::workspace::Ui`, `sci::Send` | The core talks to the shell only through these. |
| Adapters | `src/ui.rs` (`WxUi`), `src/sci.rs` (`WxSci`) | Implement the ports with wx. |
| Commands | `src/command.rs`, `src/handlers.rs` | Command pattern, see below. |
| Features | `findpanel`, `encdialogs`, `pwdialogs`, `streams`, `updates`, `mdbar`, … | One module per feature. |
| Chrome / widgets | `chrome.rs`, `form.rs`, `widgets/` | Custom-painted bars; declarative dialog builder; reusable controls. |
| Composition root | `app.rs` (`start`, `wire`) | Builds the window, wires events. |

## Commands (open/closed)

Every menu, toolbar, palette and shortcut action is a numeric id. `CommandBus` offers it, in order,
to each registered `CommandHandler` (chain of responsibility). To add a command group:

1. Implement `CommandHandler` (`name`, `handle`) — return `false` for ids you do not own.
2. Register it in `CommandBus::standard()`.

Workspace-only commands live in `dispatch.rs` and run before the bus.

## Conventions

- The app is `Rc<RefCell<App>>`; event handlers use `try_borrow_mut`. Modal dialogs run with nothing borrowed.
- State lives in small structs (`state.rs`: whiteboard, preview, streams, search results, clipboard) owned by `App`.
- OS differences go through `platform::current()`; do not add `cfg(windows)` outside `src/platform/`.

## Side list

`side/` is the right-hand list as a strategy: each of the Character table, Clipboard history, Document list and Function list is a `Content` (columns, rows, what activating a row does), and `Side` only tracks which one is shown. It is toolkit-free and tested; `panels.rs` fills the list control from it.

## Dialogs

`form/` builds dialogs from a declarative `Spec` at fixed pixel positions (to match the Win32 build). `form/layout.rs` is pure: `check` finds layout mistakes (duplicate ids, controls off the edge or under OK/Cancel) and runs on every dialog in debug builds; `Flow` places rows of controls for new dialogs.

## Widgets

`widgets/` holds reusable controls. Each is a thin wx panel over a toolkit-free model that is unit tested.

- `tab_strip` - scrollable, reorderable tabs with dirty dot, badge, pin, colour bar, close button and `+`. `model.rs` does the layout and scrolling; scrolling is by tab index (the idea behind wxWidgets' `wxAuiTabContainer`), and a scroll arrow exists, and takes room, only while there is something to scroll to. It reports `TabEvent`s and holds no application state.
- `menu_bar`, `tool_bar`, `status_bar` - the window's bars. Each has a tested layout function and reports typed events; `chrome.rs` only supplies their content and maps events to `Act`.
- `controls` - our own `Button`, `TextBox`, `CheckBox`, `Label`, `ListBox`, `ListView`, `Dialog` ... over the stock widgets. Application code never creates a stock control directly; each wrapper derefs to the stock widget and is adopted by the theme, so styling or behaviour changes happen in one place.
- `quick_pick` - type-to-filter list (Command Palette, Quick Open). Ranking and keyboard navigation are pure and tested.
- `data_list` - the plan for filling a multi-column list (`ListView::set_columns` / `fill` / `selected`). `Role::Editor` lets the workspace tree and side lists take the editor's colours from the theme.
- `web_pane` - a scriptable web page (preview, whiteboard): holds scripts until the page is ready, remembers the theme it was told, and registers the `ipc` message handler. Script building and the ready-queue are pure and tested.
- `dock` - a docked strip (themed background, height, window re-flow) used by the Find panel and the Markdown bar. The Find panel's placement of every control per mode and width is a pure `findpanel/layout.rs`.
- `pointer` - the hover/press/release state machine the bars share.
- `theme` - one `Theme` value and a hub; components subscribe, and a late subscriber is told the current theme at once.
- `geom` / `paint` - `Rect`, colour mixing, the light/dark `Look` and drawing helpers.

## Platforms

`src/platform/` has `windows.rs` (Explorer, cmd, Recycle Bin, regional dates, native menu loop) and `unix.rs` (macOS and Linux). The rest of the shell is OS-independent.

The Win32-only shell that used to live in the repository root was removed on this branch; it remains in git history (`main`).

Packaging, the installer and release CI (`scripts/package.ps1`, `installer/`, `.github/workflows/build.yml`) still describe the old Win32 build and need porting to the wx build.

## Build (Windows)

    cd wx
    cargo build

Needs MinGW-w64 (GCC 15, UCRT) on `PATH`, `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER` set to its `gcc.exe`, and `LIBCLANG_PATH` pointing at LLVM.
