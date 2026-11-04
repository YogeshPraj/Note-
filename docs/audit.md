# Architecture audit (Phase 0)

## Current status (2026-11-03, `main` after v2.0.5) — read this first

The sections below are the historical audit. Measured today:

| Gate | State |
|---|---|
| `bash scripts/check-all.sh` | Passes, including with `STRICT=1` (`-D warnings`). |
| Clippy warnings | **0** in both workspaces. The "Lint debt" tables further down are history. |
| Functions over 60 lines (`too_many_lines`) | **0**, none suppressed. |
| `unwrap` / `expect` / `panic!` in production code | **0** (the five `expect`s left are in test modules). |
| Tests | Core, ui-kit and app suites all pass; `cargo test` gives the counts. |
| Files over 400 lines | **0** (finding 5). `check-architecture.sh` fails on any file over 400. |
| Global mutable state | Only the deliberate globals listed under "Deliberate global state" below (finding 16). |
| Raw native dialogs | **0** outside `ui_kit::ask` (findings 15, 17); enforced by `check-architecture.sh`. |
| Long work on the UI thread | Finding 18 fixed (see architecture.md, Threading). |

"`ARCHITECTURE_SETUP.md`" below is the restructuring brief of 2026-11-01; it was not kept in the repository.

Read-only audit of `crates/core` (the portable core) and `wx/` (the wxWidgets shell). Measured on branch `wx-port`
at `b9634dc`. Test code is excluded from the counts. No code was changed.

## Decisions taken (from the project owner)

| Topic | Decision |
|---|---|
| Scope | Adapt `ARCHITECTURE_SETUP.md` to the real layers rather than follow it literally. |
| `syntax` crate | **Skipped.** Highlighting is done by Scintilla and Lexilla; the text lives in Scintilla. There is no `ropey` buffer and no token/span stream to extract. The language registry (`lang.rs`, `lexer.rs`, `functions.rs`, `complete.rs`) stays in `core`. |
| `unsafe_code` | `forbid` in `core` and `ui-kit`; allowed (and listed) in the FFI / platform code. |

## What the project is

| Item | Reality |
|---|---|
| UI framework | wxWidgets through `wxdragon` 0.9.24 (+ a vendored patched `wxdragon-sys`), Scintilla 5.5 / Lexilla 5.5 for the editor. |
| Crates today | `crates/core` (`notepp-core`, 30 files, 12.0k lines, no UI deps) and `wx` (`notepp-wx`, a **separate workspace**, 60 files, 11.9k lines). Root `Cargo.toml` is a workspace of `core` only. |
| Build | `core` builds with plain cargo. `wx` needs MinGW GCC (UCRT), LLVM `libclang`, and the GNU Rust target; that is why it is its own workspace. |
| Already in the target shape | `wx/src/widgets/` (generic controls, theme hub, tab strip, bars, dock, quick pick, web pane, data list), `command.rs` (command bus), `services/`, `side/`, pure layout modules with tests. 143 unit tests pass. |

## Findings

| # | Problem | Where | Proposed fix | Target |
|---|---|---|---|---|
| 1 | `ui-kit`-style widgets depend on `notepp_core` | `widgets/tab_strip/mod.rs` (`TabInfo`), `widgets/theme.rs` calls `crate::platform` | Give the kit its own `Tab` description type (app converts); split `platform` into native-window styling (kit) and OS actions (app). | ui-kit / app |
| 2 | Generic pure logic sits in the shell crate | `services/schedule.rs`, `services/clipboard.rs` (`SecretGuard`, `Fingerprint`), `form/layout.rs`, `widgets/data_list.rs`, `widgets/web_pane/{queue,script}.rs`, `widgets/quick_pick/filter.rs` | Pure parts to `core` (schedule, secret guard) or `ui-kit` (UI-generic ones). Keep wx adapters in the app. | core / ui-kit |
| 3 | Command execution split across three places | `wx/src/dispatch.rs:25` (`command_core`, 341 lines), `command.rs` bus, `extra.rs:404` (`handle`, 301 lines), `app.rs` | One `Command` registry (spec: id, label, shortcut, enabled) in the app; workspace-only commands execute in `core`. Menus, toolbar, palette and shortcuts generated from it. | app + core |
| 4 | Menu content is imperative and huge | `menus.rs:189` `build_menus` (410 lines), `menus.rs` 600 lines | Generate menus from the command registry through `MenuSpec`. | app |
| 5 | Files over 400 lines | core: `workspace.rs` 2279, `edit.rs` 1134, `search.rs` 951, `bitwarden.rs` 653, `crypto.rs` 552, `http.rs` 541, `mdformat.rs` 535, `commands.rs` 513, `session.rs` 453, `lang.rs` 428, `sci.rs` 406. wx: `app.rs` 891, `extra.rs` 706, `menus.rs` 600, `pwdialogs.rs` 487, `form/mod.rs` 469, `dialogs.rs` 464, `encdialogs.rs` 417, `widgets/tab_strip/mod.rs` 412 | Split by responsibility (e.g. `workspace` into tabs / files / edit ops / sessions). | core / app / ui-kit |
| 6 | Functions over 60 lines | core: `workspace.rs:2056` `restore_tabs` 92, `:805` `line_op` 71, `:1942` `snapshot` 61, `edit.rs:81` `natural_cmp` 84, `lang.rs:13` `palette` 108, `lexer.rs:21` 66, `http.rs:316` `send` 62. wx: `menus.rs:189` 410, `dispatch.rs:25` 341, `extra.rs:404` 301, `form/mod.rs:286` `run` 183, `app.rs:82` `start` 170, `app.rs:320` `wire` 144, `settings_ui.rs:34` 139, `tab_strip/mod.rs:131` `wire` 119, `encdialogs.rs:311` 106, `findpanel/mod.rs:242` 98, `app.rs:499` 96, `quick_pick/mod.rs:33` 80, `dialogs.rs:297` 74, `extra.rs:224` 72, `extra.rs:154` 68, `findpanel/layout.rs:88` 61 | Extract steps into named functions or types. `fuzz.rs:42` is a test-style harness (125 lines) and moves under `#[cfg(test)]`. | all |
| 7 | `app.rs` is a god object | `app.rs` `App` (about 45 fields), `start`, `wire`, `command` | Continue splitting into services and handlers; `start`/`wire` into per-area setup functions. | app |
| 8 | Hard-coded colours outside a theme | core: `sci.rs` 34, `workspace.rs` 10, `bookmarks.rs` 6, `lang.rs` 5, `search.rs` 5. wx: `widgets/paint.rs` 9, `widgets/theme.rs` 7, `tab_strip/mod.rs` 4, `updates.rs` 3, `status_bar.rs`, `form/mod.rs` | Move to named tokens in the theme (UI colours) and to a palette table in `core` for the editor palette. `Look` (paint.rs) and `Theme` (theme.rs) merge into one token set. | ui-kit / core |
| 9 | Two theming mechanisms | `Look` (custom-painted controls) and `Theme`/`Role` (native controls) | One `Theme` with tokens for both. | ui-kit |
| 10 | No spacing / radius / font tokens | pixel literals throughout `widgets/*`, `chrome.rs`, `findpanel/layout.rs`, `form` | Add `Spacing`, `Radius`, `Fonts` to the theme; widgets use them. (Dialog layouts that must match Win32 pixel for pixel are exempt, and listed.) | ui-kit |
| 11 | `unwrap`/`expect`/`panic` in production code | `core/complete.rs:77` (`base.unwrap()`), `core/crypto.rs:37` (`expect`, fixed key length), `core/search.rs` 1, `lang.rs` 1; wx: `app.rs` 1 `expect`, `pwdialogs.rs` 2, `preview.rs` 2, `whiteboard.rs` 1 (about 14 sites; no `panic!`, `todo!` or `dbg!`) | Replace with `?` / `let else` and typed errors. | core / app |
| 12 | No typed errors | `core` returns `String` errors widely (e.g. `Result<_, String>`) | One `Error` enum per library crate (`thiserror`). New dependency, needs approval. | core |
| 13 | `println!` / `eprintln!` | `wx/form/mod.rs` (debug layout check) | Route through the existing trace helper. | app |
| 14 | `unsafe` in FFI / platform code | wx: `platform/windows.rs` 18, `platform/winmenu.rs` 11, `sci.rs` 6, `widgets/controls.rs` 2 (trait impl delegation), `pwdialogs.rs` 1 (`wxd_Note_SafeYield`); `core` has **none** (matches in `compare.rs`, `functions.rs`, `lang.rs`, `update.rs` are strings) | `forbid` in `core` and `ui-kit`. `controls.rs` unsafe moves out of the kit or is allowed per-module with a comment. | core / ui-kit / app |
| 15 | Duplicated native-dialog code | `MessageDialog::builder` 8 sites, `FileDialog::builder` 4, `DirDialog::builder` 2, `info()` helper in `extra.rs` | One dialog helper set in the kit (message, confirm, pick file, pick folder). | ui-kit |
| 16 | Global mutable state | 15 `thread_local!` statics in the shell (`findpanel`, `panels` ROOT, `incr`, `dialogs`, `encdialogs`, `pwdialogs`, `settings_ui`, `preview`, `menus` arena, `form` hook, theme hub) | Replace with owned state structs where practical; the theme hub and menu arena are deliberate and documented. | app / ui-kit |
| 17 | Remaining raw widget use | `dialogs.rs` (`SingleChoiceDialog`), `settings_ui.rs` (`FontDialog`, `ColourDialog`), `ui.rs` (`TextEntryDialog`), `app.rs` | Wrap in the kit's dialog helpers. | ui-kit |
| 18 | Long-running work on the UI thread | Find in Files (`app.rs` `panel_command`), file tree scan, Bitwarden calls (use a `wxYield` loop), log follow | Background worker with progress and cancel. (Behaviour change: out of scope until the structure is done; listed as UX debt.) | app |
| 19 | No lint configuration | no `[workspace.lints]`, no `clippy.toml`, no `docs/ux.md`, no `docs/architecture.md`; `CLAUDE.md` exists but describes the old Electron project | Add per Phases 2 and 5. | repo |
| 20 | Platform code mixed with UI | `platform/` contains both OS actions (reveal, terminal, recycle, local time) and window styling | Split (see 1). | app / ui-kit |

## Proposed crate mapping (adapted)

Two Cargo workspaces stay, because the wxWidgets build needs a different toolchain. This is the one structural
deviation from `ARCHITECTURE_SETUP.md` and needs your decision at the STOP below.

```
<repo>/
  Cargo.toml                       # workspace: crates/core   (builds with plain cargo)
  crates/core/                     # notepp-core: unchanged role; gains the pure services below
  wx/
    Cargo.toml                     # workspace: crates/ui-kit, crates/app   (needs the MinGW/LLVM environment)
    crates/ui-kit/                 # wx widgets + theme; depends on wxdragon only
    crates/app/                    # notepp-wx binary
  vendor/wxdragon-sys/             # unchanged
```

| Today | Goes to | Notes |
|---|---|---|
| `core/*` (30 modules) | `core` | Split the over-long files (finding 5). `commands.rs` ids become the registry's `Command` ids. |
| `widgets/theme.rs`, `paint.rs`, `geom.rs`, `pointer.rs` | `ui-kit` | One token set (findings 8-10). |
| `widgets/controls.rs`, `dock.rs`, `data_list.rs`, `menu_bar.rs`, `tool_bar.rs`, `status_bar.rs`, `tab_strip/*`, `quick_pick/*`, `web_pane/*` | `ui-kit` | `tab_strip` gets its own `Tab` type (finding 1). Native-dialog helpers added (15, 17). |
| `form/*` | `ui-kit` (generic dialog builder, `layout.rs`) | App dialog definitions stay in `app`. |
| `platform/windows.rs`, `winmenu.rs` (window styling, native menu loop) | `ui-kit` (native module) | The only `unsafe` the kit would keep; needs a per-module allowance or its own `ui-kit-native` crate. |
| `platform/*` OS actions (reveal, terminal, recycle, time) | `app` | |
| `services/schedule.rs`, `services/clipboard.rs` (pure guard) | `core` | wx adapter (`copy`, `paste`) stays in `app`. |
| `dispatch.rs`, `tables.rs`, `whiteboard.rs` (protocol parse) | `core` | Workspace-only command execution and tables. |
| `command.rs`, `handlers.rs`, `app.rs`, `extra.rs`, `menus.rs`, `menu_spec.rs`, `popups.rs`, `panels.rs`, `side/*`, `findpanel/*`, `mdbar.rs`, `incr.rs`, `streams.rs`, `updates.rs`, `settings_ui.rs`, `dialogs.rs`, `encdialogs.rs`, `pwdialogs.rs`, `palette.rs`, `preview.rs`, `whiteboard` glue, `chrome.rs`, `ui.rs`, `sci.rs` (wx Scintilla adapter), `state.rs`, `main.rs` | `app` | `menu_spec` stays app-side because it uses `notepp_core::shortcuts`. |

## Lint debt (expected once Phase 2 lints are on)

Not measured yet; to be filled in after Phase 2. Likely largest groups: `too_many_lines` (the 24 functions above),
`pedantic` casts (`as i32` / `as usize` in layout code), `unwrap_used` (finding 11), `print_stdout` (13), and
`missing_debug_implementations` on the wx wrapper types.

## Risks to settle before Phase 1

1. **Moving the widgets into a second crate** touches most of `wx/src` (imports) and the build scripts (`build.rs`,
   `app.rc`, MinGW linker settings), and must keep the tests and the 264-command sweep green.
2. **`ui-kit` without `core`** requires the tab and theme types listed in finding 1; menus and the status bar are
   already free of `core` types.
3. **Typed errors** add `thiserror` (a new dependency) and change many signatures in `core`.
4. **Packaging and CI** for the wx build do not exist yet; the crate move should land before they are written.


## Lint debt (Phase 2)

Gates now in force: `unsafe_code` is `forbid` in the root workspace (`notepp-core`) and `deny` in the `wx` workspace, where only
the FFI modules opt out with `#[allow(unsafe_code)]` (`ui-kit::native`, `ui-kit::controls`, app `platform`, `sci`, one call in
`pwdialogs`). `unwrap_used` and `dbg_macro` are `deny`: **0 errors** in both workspaces (11 production `unwrap`s fixed,
`build.rs` exempt by design). `clippy` pedantic warnings remain, to be burned down:

| Workspace | Warnings | Largest groups |
|---|---|---|
| `notepp-core` | 461 | 179 `isize`/`usize` sign casts (Scintilla's pointer-sized API), 48 missing `# Errors` docs, 18 missing `Debug` |
| `wx` (ui-kit + app) | 391 | 47 `Debug` missing, 50 `usize`→`i32` casts (wx pixel coordinates), 24 redundant closures, 22 collapsible `if` |

Decision needed later: the cast lints are inherent to the Scintilla and wx APIs; either allow them in the two manifests or wrap
them in checked helpers. Typed errors: `notepp_core::error::Error` (thiserror) now covers `crypto`; the other `Result<_, String>`
sites (bitwarden 12, search 6, textools 4, update 3, http 2, …) are still to migrate. `scripts/check-architecture.sh` enforces
the crate boundaries (needs Git Bash on Windows).


## Status at the end of the restructuring (Phase 7)

Verification: `cargo fmt --all -- --check`, `cargo clippy --all-targets`, `cargo test` in both workspaces and
`scripts/check-architecture.sh` all pass (`bash scripts/check-all.sh`: ALL CHECKS PASSED). `cargo clippy ... -D warnings`
does **not** pass yet: 143 warnings in `core` and 141 in `wx` remain (see Lint debt). Tests: core 202, ui-kit 57 + 1 token
test, app 82.

| # | Finding | Status |
|---|---|---|
| 1 | Kit depends on core | **Fixed.** `ui-kit` has its own `Tab`; `platform` is split into `ui_kit::native` and `app::platform`; enforced by `check-architecture.sh`. |
| 2 | Pure logic in the shell | **Fixed.** `dispatch`, `tables`, `schedule` and the secret guard are in core; `form`, `data_list`, `quick_pick`, `web_pane` models are in the kit. |
| 3 | Command execution split three ways | **Fixed in structure, partly in content.** One registry and `execute()`; `command_core` and the handler chain remain as the two executors behind it. `extra::handle` (301 lines) is not split yet. |
| 4 | Imperative menus | **Fixed.** 212 items generated from the registry, 5 families from providers, snapshot-tested. `build_menus` is still a long function (it lays out the structure). |
| 5 | Files over 400 lines | **Fixed** (2026-11-03). Split by responsibility into child modules, tests into `foo/tests.rs`; enforced by `check-architecture.sh`. |
| 6 | Functions over 60 lines | **Remaining.** 8 in core and 15 in the shell (clippy `too_many_lines`). |
| 7 | `app.rs` god object | **Remaining.** `App` has about 46 fields; command state and key map moved onto it as fields rather than out of it. |
| 8 | Hard-coded colours | **Fixed in the kit and the shell** (token test). Remaining: the editor's own palette in `core` (`sci.rs`, `settings.rs`), which is data of the editor colour scheme, not UI chrome. |
| 9 | Two theming mechanisms | **Fixed.** `Look` is derived from `Theme`. |
| 10 | No spacing, radius, font tokens | **Partly fixed.** Tokens and `Theme::px` exist and fonts are used; component pixel constants and the Win32-parity dialog positions are still literals by design. |
| 11 | `unwrap`/`expect` in production | **Fixed** for `unwrap` (denied). `expect` remains in three places (`crypto.rs` fixed key length, the menu bar lookup at app start-up, one in `pwdialogs.rs`), each a warning. |
| 12 | No typed errors | **Fixed.** `notepp_core::error::Error` (`thiserror`) replaces every `Result<_, String>`; the app has `AppError`. |
| 13 | `println!` in the shell | **Fixed.** Routed through `PROBLEM_SINK` and the trace helper. |
| 14 | `unsafe` | **Fixed.** `core` forbids it; the workspace denies it and only FFI modules allow it, each with a comment. `ui-kit::components` controls allow it for one trait method wxdragon makes unsafe. |
| 15 | Duplicated native-dialog code | **Fixed** (2026-11-03). `ui_kit::ask` (tell, yes_no, yes_no_cancel, open_files, save_file, folder, choice, colour_of, font); no builder elsewhere. |
| 16 | Global mutable state | **Fixed** (2026-11-03). Dialog state lives in its form (`Form::set_state`), Incremental Search and worker slots on `App`; what remains is deliberate and listed below. |
| 17 | Raw widget use | **Fixed** (2026-11-03) with finding 15. |
| 18 | Long work on the UI thread | **Fixed** (2026-11-03): worker threads with progress and cancel; see architecture.md, Threading. |
| 19 | No lint config or docs | **Fixed.** `[workspace.lints]` in both workspaces, `clippy.toml`, `rustfmt.toml`, `docs/architecture.md`, `docs/ux.md`, new `CLAUDE.md`, `scripts/check-all.sh`, the Claude hook. |
| 20 | Platform code mixed with UI | **Fixed.** |

### Deliberate global state

Each of these is global because its owner cannot hold it: a C callback with no user data, a callback that runs while
`App` is borrowed, or one object per process.

| State | Where | Why it is global |
|---|---|---|
| `KEYS`, `HOOK`, `CLICKS`, `MOUSE_HOOK`, `EATEN` | `ui-kit/native/windows.rs` | Win32 hook procedures are plain C functions with no user-data pointer. |
| `HUB` (theme and subscribers) | `ui-kit/theme.rs` | One theme per process; every component reads it while painting. |
| `QUEUE`, `TIMER` | `ui-kit/defer.rs` | Work run after the current event; callers have no shared owner. |
| `CURRENT` | `ui-kit/popup_menu` | The one open popup; the keyboard and mouse hooks route to it. |
| `EDIT_HOOK`, `PROBLEM_SINK` | `ui-kit/form` | Form callbacks are plain `fn` pointers; the hook is handed to the next form and taken when it is built. The sink is set once at start. |
| `ARENA`, `OVERRIDES` | `app/menus.rs` | Native menu ids refer to entries for the frame's life; menus are rebuilt from many places. |
| `OPEN` | `app/popups.rs` | Which bar menu is showing; a stale menu closing must not clear a newer one. |
| `PANEL`, `DOCK`, `MODE`, `SHOWN`, `RESULTS` | `app/findpanel` | One docked Find panel per process, read by handlers that already hold `App` borrowed. |
| `ROOT` | `app/panels.rs` | Tree events (expanding) fire while `open_workspace` holds `App` borrowed. |
| `PENDING` | `app/tree_reader.rs` | Folders being read in the background: added by the tree's expand event (which can fire while `App` is borrowed), filled in by the timer. |
| `BUSY` | `app/pwdialogs.rs` | A Bitwarden job is running; one flag across the flow's four screens (install, unlock, vault, add), each its own form. |
| `EDITS` | `app/app.rs` | Bumped by Scintilla's change notification, which fires during commands that hold `App` borrowed. |

### Controls with nothing behind them (checked 2026-11-04)

Swept: every command against its handlers, form buttons, native buttons, Settings rows, find options, preview page messages, whiteboard events, tab and status bar events.

Fixed: the folder panel's close button, the JSON preview's `json:`, `jsonfix:` and `jpath:` messages, and the status bar's INS/OVR.

Removed at the owner's request: View > Move to Other View, Clone to Other View, Focus on Another View, Synchronize Vertical and Horizontal Scrolling, and Project Panel 1 to 3 (always greyed, no handler; Focus on Other Pane came back with the docked tab), and the unread settings `md_preview` and `whats_new_seen`. The Other View submenu is now New Instance.

### Lint debt after Phase 7

| Workspace | Warnings | Largest groups |
|---|---|---|
| `notepp-core` | 143 | 48 `missing_errors_doc`, 11 `assert!(x.is_empty())` style, 8 `too_many_lines`, 6 identical match arms, 6 structs with more than 3 bools |
| `wx` | 141 | 11 `assert!` on emptiness, 10 `format!` appended to a `String`, 9 by-reference small arguments, 9 items after statements, 15 `too_many_lines`, dead code (`mdbar::wire`, `panels::wire`, `Flow`) |

Cast, `Debug` and `unwrap` lints are at zero.

## Behaviour differences introduced

- Undo, Redo, Cut, Copy, Delete and Reload are greyed (menu and toolbar) when they cannot run.
- Toolbar tool-tips for Undo, Redo, Zoom In/Out, Word Wrap and Dark Mode read slightly differently (derived from the registry).
- A shortcut override that clashes with another command is now refused at start-up (the command keeps its default) and
  reported on the status line; before, both bound the same chord.
- Update-pill colours changed slightly (`info`, `warning`, `success` tokens); the tab badge and dirty dot use tokens with
  the same values as before.
