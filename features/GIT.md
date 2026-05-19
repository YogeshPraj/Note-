# Note++ — Git Integration Spec

> Status: design locked. Implementation in progress.

## Locked Decisions

| Question | Answer |
|---|---|
| Backend | **`git` CLI** via `child_process.spawn`. No JS git library. Devs already have git installed. |
| v1 scope | Tiers **A + B + C + D**: detect / status bar / source control panel / stage·unstage·commit·discard / push·pull·fetch·sync / branch switch & create. **Defer**: inline diff, blame, history, conflicts (v1.1+) |
| Panel location | New **left side panel** alongside the existing file tree, toggleable via a toolbar `⎇` button |
| Diff viewer | Out of v1 scope. Clicking a changed file just opens it as a normal tab |
| Auto-refresh | On window focus + after save + after every git op. **No** chokidar watching (too costly on big repos) |
| Auto-fetch | Background `git fetch` every 3 min (configurable later) |
| Multi-repo | Show status for **active tab's repo only**. Each tab knows its own repoRoot |
| GPG / signing | Inherit user's `commit.gpgsign` config — no special UI |
| Pull strategy | Single Pull button. Respects `pull.rebase` config |
| Auth | Inherits system git credential manager (no UI) |

## Architecture

```
┌─ Main process (Node) ───────────────────────────────────────────┐
│  src/git-service.js                                             │
│   ├─ findRepoRoot(path)                                         │
│   ├─ status(root)        → { branch, ahead, behind, files[] }   │
│   ├─ stage / unstage / discard / commit                          │
│   ├─ push / pull / fetch / sync                                 │
│   └─ branchList / branchSwitch / branchCreate                   │
└────────────────┬────────────────────────────────────────────────┘
                 │ IPC (electronAPI.git.*)
┌────────────────▼────────────────────────────────────────────────┐
│  Renderer                                                       │
│   ├─ gitState  (cached status per repo, refreshed on events)    │
│   ├─ Status bar git pill (branch · ↑/↓ · ●dirty)                │
│   ├─ Source Control side panel                                  │
│   └─ File tree decorations (M/A/D/U/!) — Phase 4                │
└─────────────────────────────────────────────────────────────────┘
```

### Git status parsing

We use `git status --porcelain=v1 -b -z`:
- `-b` → branch header with ahead/behind tokens
- `-z` → NUL-separated, safe for paths with spaces/newlines
- `--porcelain=v1` → stable, machine-readable format

Two-char status code semantics:
```
First char  = index status (staged)
Second char = worktree status
```
Common combos: ` M`, `M `, `MM`, `A `, `D `, ` D`, `??`, `UU` (conflict).

## UI

### Status bar pill

```
… | INS | ⎇ main ↑2 ↓0 ●5 | …
```
- Click → toggles the Source Control panel.
- `⎇` is `⎇`.

### Source Control panel

```
SOURCE CONTROL                          ⟳ ⋯
────────────────────────────────────────
 Branch: main         [⎇ Switch] [+ New]
 ↑ 2  ↓ 0             [⬆ Push] [⬇ Pull] [↻ Sync]
────────────────────────────────────────
 Commit message
 ┌────────────────────────────────────┐
 │                                    │
 └────────────────────────────────────┘
 [✓ Commit]
────────────────────────────────────────
 ▾ STAGED CHANGES (2)
   M  src/renderer.js              [−][⊟]
   A  src/crypto.js                [−][⊟]
 ▾ CHANGES (3)
   M  src/index.html               [+][↶]
   M  src/style.css                [+][↶]
   ?  ENCRYPTION.md                [+][↶]
```

## Implementation Phases (this session)

1. **git-service.js + IPC + preload bridge** — backend complete, no UI
2. **gitState + detection + status bar pill** — read-only feedback
3. **Source Control panel + toolbar toggle + CSS** — visual shell
4. **Wire panel actions** — stage / unstage / discard / commit / sync / push / pull / fetch
5. **Branch switch + create** — branch picker menu
6. **Auto-refresh** — focus / save / post-op / 3-min auto-fetch
7. **File tree decorations** — M/A/D/U badges (last because it requires reading file tree code carefully)
