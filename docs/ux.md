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
| Docked tab | Tab menu > Dock > Left, Right, Top or Bottom shows that tab, whatever its kind in its own editor pane, with a draggable splitter; Undock puts it back. One tab at a time; a whiteboard cannot be docked. While docked, each pane has a header with its file name: the current pane has an accent line and the other is dimmed, the docked one has a `dock` badge and a close mark that undocks, and a line under a tab in the strip marks the tab in the other pane. F8 or a click on a pane or its header moves between them. The pane, its side and the split survive a restart. |
| Floating tab | Tab menu > Dock > Undock makes the tab a window of its own, whatever its kind, with the usual title-bar buttons and a Dock Back button in the bar under them. Several can float. Closing the window closes the document, asking first if it has unsaved changes. Dock Back puts it last in the tab strip. The Window menu lists floating documents and brings them forward. Find, the status bar and the preview stay in the main window. |
| Tree on the whiteboard | The whiteboard's Library has Agent and Tree. Each is a compact tree of real shapes: right-click a row for Add child, Add sibling, Change icon, Rename, Move up, Move down and Delete; double-click renames; the box beside a row folds it. New rows take an icon from their name. Right-clicking anywhere else gives the usual menu. |
| Keyboard first | Commands are in one registry with default shortcuts; the shortcut mapper edits them. Alt+letter and F10 open the menus, also while a web page has the keyboard. F6 and Shift+F6 move through the editor, toolbar, tabs and status bar (Tab types in the editor, so it cannot leave it). Inside a bar: Left, Right, Home, End move, Enter or Space presses, Tab goes on, Esc returns to the editor. |
| Never block the UI | Find in Files, the Quick Open listing, tree folder reads, the session write, hashing and large file reads run off the UI thread, with progress and Stop/Cancel where they can take long. Inserting text into Scintilla stays on it. |
| Accessible | Contrast of text on every theme background is tested. The custom-painted bars (menu, tool, tab, status) show their items to screen readers and can be pressed through them (`ui_kit::a11y`, MSAA, Windows only). The Settings page list takes the keyboard: Up, Down, Home, End, with a focus ring. A ring shows where the keyboard is in a bar, only while it is there. |
| Scales | The program is system-DPI aware: `Theme::scale` comes from the system at start, and components, form layouts (written at 100%, placed scaled), toolbar icons and editor margins go through `Theme::px`. Sharp at the scale in force at start; on a monitor with another scale Windows stretches the window. `NOTEPP_UI_SCALE` forces the layout scale for testing (fonts do not follow it). |
| Toasts | `ui_kit::toast`: errors that need no answer (cannot save, open, read, check for updates) and the "latest version" notice show as toasts bottom-right, never taking the keyboard; up to three stack, each goes after 5 s (info) or 10 s (error) or on a click. Before the window is up they fall back to a message box. Errors inside a modal flow (Passwords, Encryption) stay in that flow's dialogs. Screen readers are not told when a toast appears. |
| Dark mode | Custom components and stock controls follow the theme, and so do messages and questions (`ui_kit::message_box`). The system pickers (open/save, folder, colour, font) and the progress window stay light. |
| Editor syntax colours | The editor's palette lives in `core` (`sci.rs`, `settings.rs`); the theme's `Syntax` tokens are for the kit's own surfaces. |
