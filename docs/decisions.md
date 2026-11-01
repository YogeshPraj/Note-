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
