# Plan: Brain, one place for what you learn at work

Status: **approved 2026-11-12, being built.** Encryption is off by default and turned on in Settings. Done so far: parts 1 and 2
(`core::brain`: the note format, entries, tags, links, search, and the store with its lock and encryption). Choices already made by the owner: kept in Note++'s profile; a daily log plus
topic notes; agents may search and add, with permission; the first version has capture, search, the side panel and encryption.

## What it is

A folder of Markdown notes that Note++ fills and searches for you. Nothing is in a database: every note is a file you can open, edit
and copy like any other.

```
%APPDATA%\notepp\brain\
  daily\2026\2026-11-12.md      one note per day; quick captures land here, each under its time
  topics\<name>.md               anything that grows gets a note of its own
```

A day's note:

```markdown
# 2026-11-12 Thursday

## 14:32 #deploy #azure
The staging slot needs WEBSITE_RUN_FROM_PACKAGE=1 or the swap fails.
from: D:\work\infra\deploy.ps1:120

## 16:05
Asked Priya: the nightly job owner is the data team.
```

- An **entry** is a `## HH:MM` heading, its optional `#tags`, and the text under it. `from:` records where a selection came from.
- `[[name]]` links to the topic note of that name (Ctrl+click follows it; a missing one is offered to be created).
- Topic notes are free Markdown; their `#tags` and `[[links]]` are read the same way.

## What the user gets

| Command | Default key | What it does |
|---|---|---|
| Brain: Capture | Ctrl+Alt+B | A small box: type or paste, Enter adds it to today's note under the time. Ctrl+Enter for a new line. |
| Send Selection to Brain | editor right-click | The selection becomes an entry, with the file and line it came from. |
| Brain: Today | | Opens today's note as a tab. |
| Brain: Search | Ctrl+Alt+F | One box over every entry and topic: words, `#tag`, `date:2026-11`, `today`, `week`. Enter opens the note at the entry. |
| Brain: New Topic | | Asks a name, opens the topic note. |
| Brain panel | side panel | Recent days, topics, and tags with their counts; a click opens or searches. |
| Brain: Open Folder / Export | | The folder in Explorer; the whole brain as a zip (the profile is not synced or backed up by itself). |

Settings gets a Brain page: encrypt the brain (on or off), and the shortcuts.

## Where the work goes

`core` decides everything; the shell draws.

1. **`core::brain`** (new, pure, unit tested): the layout, an entry's text, reading entries back, tags and links, the search
   (query parsing, matching, ranking, snippets), the panel's lists.
2. **`core::brain::store`**: reading and adding on disk. Adding is safe against two writers (see below). Encrypted files go
   through the existing `core::crypto` envelope with the profile's key.
3. **Commands**: rows in `core::registry` (palette, menu, shortcuts and the editor's context menu come from them), handled in a new
   app handler.
4. **Shell (wx)**: the capture box and the search list (the kit's quick-pick, extended, not a new component), the side panel's Brain
   mode (the kit's tree), the Settings page rows.
5. **Agents**: two MCP tools in `core::mcp`: `brain_search` (read level) and `brain_add` (edit level, asks like any edit). Both in
   the activity log.
6. **Docs**: `ux.md`, `decisions.md`, the README's feature list.

Each numbered part is a commit with its tests; `STRICT=1 bash scripts/check-all.sh` passes after each.

## What can go wrong, and the answer

- **Two writers at once** (two Note++ windows, or you and an agent): an addition takes a lock file, re-reads the note, adds, and
  writes through a temporary file and a rename. A stale lock (a crash) is taken over after a few seconds.
- **The note is open in a tab** when a capture arrives: the entry is added through the editor (undoable, and unsaved text is kept);
  otherwise on disk. Without this the tab and the file would disagree.
- **Encryption on and the brain locked**: capture and search ask for the password once, then work until Note++ locks again. Nothing
  is ever queued in plain text. A forgotten password is the existing recovery key's job; without both, the brain cannot be read,
  and the Settings page says so before turning encryption on.
- **Turning encryption on or off** rewrites every note; it is done file by file with progress, each through a temporary file, so a
  stop in the middle leaves every note readable in one form or the other.
- **Growth**: years of daily notes are thousands of small files. Search reads them on a worker thread with cancel, and keeps what it
  read in memory keyed by each file's time and size, so only changed notes are read again.
- **The profile is not backed up**: Export writes a zip; Open Folder shows where it is. (A folder of your choosing can come later.)
- **An agent adding junk or reading secrets**: adding asks, like every edit; searching needs agents to be on and is logged; with
  encryption on and the brain locked, agents get "locked", not a prompt on your screen.

## Not in the first version

A capture key that works while Note++ is in the tray or another program has the keyboard; pictures in entries; a graph of links;
syncing; reminders.
