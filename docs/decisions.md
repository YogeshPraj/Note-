# Decisions

One line per decision made without asking, following the "Standing decisions" in `ARCHITECTURE_SETUP.md`.

| Date | Decision | Reason |
|---|---|---|
| 2026-11-01 | Cast lints stay on workspace-wide. Core gets checked conversions in `notepp_core::num` (`try_from`, saturating); the wx workspace gets adapter modules (`ui-kit::px`, `app::platform::{scintilla, wx}`) that alone carry `#![allow(clippy::cast_*)]`. | Standing decision 1; user instruction on 2026-11-01. |
| 2026-11-01 | `notepp_core::num` helpers saturate (negative to 0, overflow to MAX) instead of wrapping. | A wrapped Scintilla position is a silent bug; saturating is safe and testable. |
| 2026-11-01 | Intentional bit truncation (colour channels, hash bytes) is written with masks and `u8::try_from(x & 0xff)`, not `as`. | Keeps the lint strict without changing results. |
| 2026-11-01 | `Result<_, String>` is migrated to `notepp_core::error::Error` (thiserror) in core and `AppError` in the app before the Command registry is built. | Standing decision 2; `execute()` returns typed errors. |
| 2026-11-01 | `cargo clippy --fix` is applied for machine-applicable lints in files being touched. | Standing decision 3. |
