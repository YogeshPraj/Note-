# Decisions

One line per decision made without asking, following the "Standing decisions" in `ARCHITECTURE_SETUP.md`.

| Date | Decision | Reason |
|---|---|---|
| 2026-11-01 | Cast lints stay on workspace-wide. Core gets checked conversions in `notepp_core::num` (`try_from`, saturating); the wx workspace gets adapter modules (`ui-kit::px`, `app::platform::{scintilla, wx}`) that alone carry `#![allow(clippy::cast_*)]`. | Standing decision 1; user instruction on 2026-11-01. |
| 2026-11-01 | `notepp_core::num` helpers saturate (negative to 0, overflow to MAX) instead of wrapping. | A wrapped Scintilla position is a silent bug; saturating is safe and testable. |
| 2026-11-01 | Intentional bit truncation (colour channels, hash bytes) is written with masks and `u8::try_from(x & 0xff)`, not `as`. | Keeps the lint strict without changing results. |
| 2026-11-01 | `Result<_, String>` is migrated to `notepp_core::error::Error` (thiserror) in core and `AppError` in the app before the Command registry is built. | Standing decision 2; `execute()` returns typed errors. |
| 2026-11-01 | `cargo clippy --fix` is applied for machine-applicable lints in files being touched. | Standing decision 3. |
| 2026-11-01 | Commands keep their numeric Notepad++ ids; `registry::Command` gives the frequent ones a typed name, and `CommandSpec` rows hold label, palette text, default shortcut and enabled rule. | A 500-variant enum would duplicate the id table; ids are also what menus, sessions and the shortcut mapper store. |
| 2026-11-01 | App-level errors are a hand-written `AppError` (no `anyhow`). | `anyhow` would be a new dependency; `AppError` has three variants and wraps the core's typed `Error`. |
| 2026-11-01 | Commands whose enabled rule says no are ignored silently (no status message) and greyed in the menu and toolbar. | Same as Notepad++. Only Undo, Redo, Cut, Copy, Delete and Reload are gated so far. |
| 2026-11-01 | `ui-kit::num` is a copy of `notepp_core::num`. | The kit may not depend on the core. |
| 2026-11-01 | Toolbar tool-tips come from the registry (name and primary shortcut); a few keep their own text (Print, Encrypt, Passwords, Log Follow). | One place for shortcut text. Tip text of Undo, Redo, Zoom, Word Wrap and Dark Mode changed slightly (capitalisation, shortcut shown). |
| 2026-11-01 | Families (recent files, encodings, charsets, languages) are `Command` variants with an index; `MenuProvider` / `GroupedMenuProvider` in core build their items. Tabs, fold levels, hashes, presets, browsers, search engines and project slots still use id ranges. | Standing decision 4: the named families were converted; the rest have no state-dependent content and can follow the same pattern later. |
| 2026-11-01 | `KeyMap` resolves conflicts: a command on its default keeps it; between overrides the lower id wins; the loser keeps its default. Reported at start-up (trace and status line) and when the mapper is saved. | User instruction. |
| 2026-11-01 | The theme gained colors, spacing, radius, font and scale (see ui-kit/src/theme.rs). Extra tokens beyond the plan: input, bg_active, disabled, ink, accent_line, info, badge. | Existing components needed them; leaving them as literals would break the no-hard-coded-colours rule. |
| 2026-11-01 | tests/design_tokens.rs fails the build when a colour triple, hex colour string or font name appears outside theme.rs and native/. Component layout constants (heights, widths) stay as named constants in their component. | Enforces the rule; pixel metrics of a component are its own design, not shared spacing. |
| 2026-11-01 | ui-kit/src/components/ holds one component per file; the old controls.rs is split into 11 files sharing the internal themed! macro. ui_kit::controls and the old module paths are re-exported. | Phase 4 component rules. |
| 2026-11-01 | The gallery's WebPane renders blank in PrintWindow captures (WebView2 draws on its own surface). | Screenshot limitation only. |

- Allowed `clippy::wildcard_imports` workspace-wide: child modules split out of a large file use `use super::*;` to share the parent's items.
- Replaced the operating system's popup menus with `ui_kit::popup_menu` (themed, non-activating, mouse-capturing). Reason: on Windows 10.0.26300 the native dark popup renders light items on a dark gutter in every preferred-app mode, so theme tokens could not reach it. Removed `native::set_app_dark` and the TrackPopupMenu loop.
- One Settings window (`ui_kit::settings_window`) replaces the Preferences dialog: category navigation is a vertical `TabStrip`, rows apply instantly and are saved, with no OK button. Style Configurator, Shortcut Mapper, the right-click menu editor and Encryption keep their own editors, opened from a button on their page; their Settings-menu entries open the window on that page.
- Settings window editors are inline: `form::embed` builds an existing declarative form into a page (`Control::Custom`) instead of a dialog; the shortcut mapper, right-click menu editor and colour editor apply on every change, and encryption actions are page buttons that rebuild the pages. The modal versions were removed.
