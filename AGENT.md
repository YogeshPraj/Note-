# Note++ — AI Agent Mode Spec

> Status: design locked, implementation in progress.

## Summary

A toggle in the AI panel that switches the chat from **suggest-and-apply** mode (current) to **agent mode**. In agent mode, the AI's response is treated as the new content for the file/selection, and the user sees a **Monaco diff editor** modal with Apply / Reject buttons before any change lands.

Multi-turn: each follow-up message rebuilds the system prompt from the **current** editor content, so iterative refinement works ("actually only do public functions" → AI starts from the current file state, not the original).

## Locked decisions

| Question | Answer |
|---|---|
| Tier | **B — Diff preview** (Monaco diff editor modal) |
| Multi-turn | **Yes**. Each turn rebuilds the system prompt from current editor content |
| Scope | Selection if present, else whole file (same as current chat) |
| Dangerous edits | **No special confirmation.** Diff preview already provides review; Ctrl+Z handles rollback if applied in error |
| What shows in chat thread | A summary line ("📝 Proposed change — review diff"), not the raw file content (which is huge) |
| Default mode on panel open | **Chat mode** (current behaviour). User must explicitly toggle to Agent |
| Toggle persistence | Per-session only — does not persist across app restarts (safer default) |

## Architecture

```
Agent mode ON:
  user types prompt
    → build NEW system prompt with current file content
    → REPLACE aiMessages[0] (system) with the new one
    → append user message
    → call /api/chat as before (keeps multi-turn history)
    → on token: stream into a hidden buffer, NOT the chat thread
    → on done:
        - chat thread shows only: "📝 Proposed change · review diff →"
        - open diff modal: original (left) vs AI output (right)
        - Apply → editor.executeEdits(...) replaces selection or full file
        - Reject → close modal, no editor change
```

## UI

### Agent toggle (AI panel header)
- Small badge button next to "+ New chat":  `🤖 Chat` ↔ `⚡ Agent`
- Visually distinct when in agent mode (orange/red accent)
- Tooltip: "Toggle Agent mode — AI's reply replaces editor content via diff preview"

### Diff modal (`#ai-diff-dialog`)
```
┌─ Review AI change ────────────────────────────── × ┐
│  [Apply ✓]  [Reject ✕]                            │
├────────────────────────────────────────────────────┤
│  Original                  │  AI proposal          │
│  ─────────────────────────────────────────────────│
│  function foo(x) {         │  /** Adds 1 to x */  │
│    return x + 1;           │  function foo(x) {   │
│  }                         │    return x + 1;     │
│                            │  }                    │
│  ─────────────────────────────────────────────────│
│  ↑ red = removed        ↓ green = added           │
└────────────────────────────────────────────────────┘
```

- Monaco's built-in `monaco.editor.createDiffEditor` does the heavy lifting (gutter markers, hunk navigation, syntax highlighting carries over from the file's language)
- Modal closes on Apply, Reject, Esc, or × button
- Reject preserves the chat history → user can ask "actually only public functions" and try again

### Chat-thread render in agent mode

Instead of dumping the (huge) file content as the assistant's message, we show:
```
🤖 Assistant
📝 Proposed change — 24 lines added, 3 removed.   [Review diff →]
```

The "Review diff →" link reopens the diff modal even after closing it.

## System prompt (agent mode)

```
You are an expert coding assistant in agent mode.

Output ONLY the complete updated content of the file/selection. No explanations,
no markdown fences, no preamble. Your response IS the new content — it will
directly replace what's currently there.

Preserve everything you weren't asked to change. Match indentation, style,
language conventions exactly.

Current scope: <selection or whole file>
Current content (this is what you must output an updated version of):
\`\`\`
<content>
\`\`\`
```

## Implementation Phases

1. **HTML/CSS** — agent toggle button + diff modal shell
2. **Toggle state** — `aiAgentMode` boolean + `toggleAiAgentMode()` + visual state
3. **Capture mode** — when `aiAgentMode` is on, route streaming tokens to a hidden buffer instead of the chat thread; replace assistant chat message with the summary line
4. **System-prompt rebuild** — on each turn in agent mode, rebuild `aiMessages[0]` from the **current** editor content
5. **Diff modal** — Monaco diff editor inside the modal, Apply/Reject wiring, Esc dismiss, "Review diff →" link to reopen
6. **Apply** — `editor.executeEdits` on selection or whole-file range; close modal; set tab dirty
7. **Reject** — just close modal; chat history retained for follow-up

## Out of Scope (v1)

- Hunk-by-hunk Apply / Reject (just whole-diff for now)
- Multi-file edits (one file per turn — future)
- Tool use / terminal access (future "Composer" tier)
- Auto-commit after Apply (defer to manual save)
