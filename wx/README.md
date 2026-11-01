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
| Chrome / widgets | `chrome.rs`, `form.rs` | Custom-painted bars; declarative dialog builder. |
| Composition root | `app.rs` (`start`, `wire`) | Builds the window, wires events. |

## Commands (open/closed)

Every menu, toolbar, palette and shortcut action is a numeric id. `CommandBus` offers it, in order,
to each registered `CommandHandler` (chain of responsibility). To add a command group:

1. Implement `CommandHandler` (`name`, `handle`) — return `false` for ids you do not own.
2. Register it in `CommandBus::standard()`.

Workspace-only commands live in `dispatch.rs` and run before the bus.

## Conventions

- The app is `Rc<RefCell<App>>`; event handlers use `try_borrow_mut`. Modal dialogs run with nothing borrowed.
- Windows-only code is isolated in `winmenu.rs`, `extra.rs::sys` and `updates.rs` and must move behind a platform trait before the macOS/Linux builds.

## Build (Windows)

    cd wx
    cargo build

Needs MinGW-w64 (GCC 15, UCRT) on `PATH`, `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER` set to its `gcc.exe`, and `LIBCLANG_PATH` pointing at LLVM.
