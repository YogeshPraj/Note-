# Note++ — Compare (File & Folder Diff) Spec

> Status: design locked, implementation in progress.

Inspired by Diff.Net / WinMerge / Beyond Compare. Two flavours:

1. **File compare** — side-by-side text diff of two files
2. **Folder compare** — side-by-side tree of two directories

Both render as new **tabs** (not modals), so they can coexist with editing.

## Locked decisions

| Question | Answer |
|---|---|
| File-diff engine | **Monaco's built-in `createDiffEditor`** (already in our stack) |
| Folder-diff engine | **`dir-compare` npm package** (~150 k weekly downloads, MIT) |
| Where the result goes | A **new tab** of type `'diff'` (files) or `'folder-diff'` (folders) |
| Default view | File-diff: **side-by-side**, syntax-highlighted. Folder-diff: **side-by-side tree**, colour-coded |
| Entry points | `File → Compare → ...` menu + right-click tab → "Select for Compare" / "Compare with Selected" |
| Skip in folder-diff | `node_modules`, `.git`, `dist`, `build`, `out`, `.next`, `.cache`, `.vscode`, `.idea`, `__pycache__` |
| Compare mode (folder) | Filename + size + content-hash. Equal if all match. |

## File compare UX

```
Tab title:  ⇆  left.js ↔ right.js
─────────────────────────────────────────────────
Toolbar:   [⇆ Swap]  [≡ Inline]  [↑ Prev]  [↓ Next]  [↻ Reload]
─────────────────────────────────────────────────
┌──── left.js ────────────────┬──── right.js ──────────────┐
│ Monaco diff editor          │                            │
│ - removed lines red         │  + added lines green       │
│ - modified lines yellow     │  + modified lines yellow   │
│ - syntax-highlighted        │  - syntax-highlighted      │
│ - line numbers              │  - line numbers            │
│ - overview ruler shows      │                            │
│   change density            │                            │
└─────────────────────────────┴────────────────────────────┘
```

- Shortcuts inside the tab: `F7` next change, `Shift+F7` previous, `Ctrl+Shift+I` toggle inline ↔ side-by-side
- Refresh re-reads both files from disk
- Swap reverses left/right
- Closing the tab disposes the diff models cleanly

## Folder compare UX

```
Tab title:  ⇆  /left/folder ↔ /right/folder
─────────────────────────────────────────────────
Toolbar:   [⇆ Swap]  [☑ Show only changes]  [↻ Reload]
Summary:   12 added · 8 removed · 5 differ · 247 equal
─────────────────────────────────────────────────
┌──── /left/folder ───────────┬──── /right/folder ─────────┐
│  📁 src                      │  📁 src                    │
│    📄 main.js          ✎    │    📄 main.js         ✎   │
│    📄 old.js           ⊖    │    (missing)               │
│    (missing)                │    📄 new.js          ⊕   │
│  📄 package.json            │  📄 package.json           │
└─────────────────────────────┴────────────────────────────┘
```

Status icons + colours:
- 🟢 `⊕` Added — only in RIGHT (left cell empty, right cell green)
- 🔴 `⊖` Removed — only in LEFT (left red, right empty)
- 🟡 `✎` Modified — in both, content differs (both yellow)
- ⚪ Equal — in both, identical (no highlight)

Click a **file** row that's marked Modified → opens that file's diff in a new tab.
Click a **folder** row → expand/collapse.

## Entry points

| Where | Action |
|---|---|
| `File → Compare → Compare Files…` | Two file pickers → opens diff tab |
| `File → Compare → Compare with Saved…` | Compare current tab content vs a picked file |
| `File → Compare → Compare with Clipboard` | Compare current (unsaved) tab content vs clipboard text |
| `File → Compare → Compare Folders…` | Two folder pickers → opens folder-diff tab |
| Right-click a tab → **Select for Compare** | Marks the tab as "left" |
| Right-click another tab → **Compare with Selected** | Opens diff (current as right) |
| Right-click a tab → **Compare with Clipboard** | Diff the live buffer against clipboard text |

## Compare with Clipboard

Fast path for diffing content that isn't saved to disk yet (e.g. two things you
just pasted into separate tabs).

- **Left pane** = the current tab's live editor buffer (works on unsaved tabs).
- **Right pane** = the current OS clipboard text, labelled **Clipboard**.
- The right pane is **editable** — paste or type and the diff updates live.

To compare two open unsaved tabs: in tab A press `Ctrl+A` `Ctrl+C`, switch to
tab B, then **Compare with Clipboard**.

## IPC additions

| Channel | Purpose |
|---|---|
| `compare-folders` | `dir-compare` invocation in main; returns a flat array of entries with status |

(File compare reads via existing `read-file` IPC — no new handler needed.)

## Out of scope (v1)

- Three-way merge view
- Inline editing in the diff view (it's read-only)
- Saving diff result as a patch file
- Compare two arbitrary tabs that are already open via drag-and-drop
- Folder-diff content hashing for very large files (we skip >10 MB binary files)
