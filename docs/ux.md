# UX Guidelines

## Reference
Behavior follows long-established editor conventions; visuals follow Windows Fluent guidelines (clean, flat, generous spacing).

## Principles
1. Keyboard first: every action has a command, every command can be bound to a key. Familiar default shortcuts.
2. Never block the UI: anything that can take >100 ms runs in the background with a progress indicator and is cancelable.
3. No data loss: unsaved changes are always confirmed; crash recovery / session restore for open tabs.
4. Predictable: same action looks and behaves the same everywhere (one component per pattern).
5. Accessible: visible focus ring, full keyboard navigation in dialogs, minimum contrast 4.5:1 for text.
6. Scales: correct at 100%-200% DPI; layout never relies on fixed pixel sizes outside theme tokens.

## Layout
- Spacing only from the scale: 2 / 4 / 8 / 12 / 16 / 24 (`Theme::spacing`, scaled by `Theme::px`).
- Dialogs: title, content, buttons right-aligned; primary action rightmost; Esc cancels, Enter confirms.
- Status bar: line/col, selection length, encoding, line endings, language, zoom (clickable to change).

## Feedback
- Errors: non-blocking toast for recoverable errors; dialog only when user decision is required.
- Destructive actions (close unsaved, replace all): confirmation, and undoable where possible.
- Empty states: explain what to do next (e.g. "No results for 'foo'. Try regex mode?").

## Theming
- Light and dark themes required for every screen and component.
- Syntax colors come from the theme palette, never hard-coded in language definitions.

## Where the code stands against these rules

These are the target. Known gaps (see `docs/audit.md` for the full list):

| Rule | State |
|---|---|
| Keyboard first | Commands are in one registry with default shortcuts; the shortcut mapper edits them. Alt+letter and F10 open the menus, also while a web page has the keyboard. |
| Never block the UI | Find in Files, the Quick Open listing, tree folder reads, the session write, hashing and large file reads run off the UI thread, with progress and Stop/Cancel where they can take long. Inserting text into Scintilla stays on it. |
| Accessible | Contrast of text on every theme background is tested; focus rings and screen-reader names of the custom-painted bars are not done. |
| Scales | `Theme::px` exists, but components still use their own pixel constants; the Win32-parity dialogs are pixel-positioned on purpose. |
| Toasts | `ui_kit::toast`: errors that need no answer (cannot save, open, read, check for updates) and the "latest version" notice show as toasts bottom-right, never taking the keyboard; up to three stack, each goes after 5 s (info) or 10 s (error) or on a click. Before the window is up they fall back to a message box. Errors inside a modal flow (Passwords, Encryption) stay in that flow's dialogs. Screen readers are not told when a toast appears. |
| Dark mode | Custom components and stock controls follow the theme, and so do messages and questions (`ui_kit::message_box`). The system pickers (open/save, folder, colour, font) and the progress window stay light. |
| Editor syntax colours | The editor's palette lives in `core` (`sci.rs`, `settings.rs`); the theme's `Syntax` tokens are for the kit's own surfaces. |
