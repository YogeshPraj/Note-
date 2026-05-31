# Note++ — Roadmap

> Drawn from a 2025/26 deep-dive on the note-taking and code-editor landscape.
> See the conversation-of-record (this repo's session notes) for the underlying research and citations.

## Vision

> **Notepad++ for the agent era. Local-first. AI that acts, with permission. No plugin marketplace, no cloud lock-in, no surprises.**

Note++ sits in the gap between Notepad++ (great launcher, no AI) and Cursor / VS Code (great AI, heavy install, cloud-leaning). The bet is that a small, fast, local-first developer notepad with the right slice of futuristic features will keep mind-share that the bigger players can't.

## Three Differentiators (already shipped, lean into these)

1. **AI auto-magic** — click 🤖, Note++ detects → installs (prompts) → starts daemon → downloads `qwen2.5-coder:1.5b` → connects. No other local AI editor does this. Cursor needs an account, Continue needs config, Cline needs a model picker.
2. **Local-first encryption + AI** — no other product in the landscape combines AES-256-GCM per-file encryption with a local AI agent. Cursor is cloud-only AI; Standard Notes has no AI; Obsidian's encryption is paid-plugin.
3. **Lightweight enough to be a notepad, powerful enough to refactor a function** — boots in <2 s, has a real Source Control panel, runs Ollama locally, edits via Monaco diff preview.

---

## Tier 1 — Ship-now (days)

Quick wins that keep Note++ on the standards curve. Each is small and self-contained.

### AI & standards
| Feature | Effort | Why | Status |
|---|---|---|---|
| **`AGENTS.md` support** — read from repo root, inject into AI system prompt | 1 hr | Vendor-neutral standard adopted by Cursor, Codex, Copilot, Jules, Gemini, Windsurf, Zed, Cline (60k+ repos). Free compatibility with the entire agentic ecosystem. | 🟡 In progress |
| **Project memory at `.notepp/memory.md`** — auto-loaded into agent context every turn | 2 hrs | Match Claude Code / VS Code Agent Skills pattern. Tiny but important — gives users a place to write project-specific instructions the AI will always see. | 🟡 In progress |
| **Quick-action buttons** in AI panel — `✨ Polish` / `🔧 Refactor` / `📝 Add comments` / `🧪 Generate tests` | 2 hrs | Granola's $1.5B wedge is "polish my mess." Pre-canned Agent-mode prompts that Just Work. Reuses existing diff modal. | 🟡 In progress |
| **Multi-model picker** — separate models for chat vs agent vs quick-actions | 3 hrs | Speed/quality tradeoff per task; Continue & Cursor both do this. E.g., `qwen2.5-coder:1.5b` for chat speed, `deepseek-coder:6.7b` for agent quality. | 🟡 In progress |
| **Voice prompt input** in AI panel | — | Tried: Web Speech API (no Google API key in Electron), `@xenova/transformers` whisper-tiny in renderer (renderer killed on inference, both main-thread and proxy modes), the same in main process via `onnxruntime-node` + `AudioWorklet` PCM capture (worked end-to-end but had assorted rough edges). All voice/dictation code removed in v1.3 cleanup. Reintroduce via native `whisper.cpp` bindings when the broader Tier 3 voice work happens. | 🗑 Removed (deferred) |

### Power-user parity (Notepad++ / Sublime gaps — community research May 2026)
| Feature | Effort | Why | Status |
|---|---|---|---|
| **Distraction-free / Zen mode** — F11 hides tabs, toolbar, statusbar; pure full-screen editor | 2 hrs | Top Sublime Text praised feature. Writers + focus sessions. Pure CSS + toggle; zero dependencies. | ✅ Shipped |
| **Auto-save to disk** — configurable interval (e.g. every 30 s) writes dirty files to their actual path | 3 hrs | #3 most-downloaded Notepad++ plugin (AutoSave). Different from session save — eliminates crash data loss for saved files. Opt-in via Settings. | ✅ Shipped |
| **Spell check** — toggle in status bar / View menu; highlights misspellings in comments, strings, markdown, plain text | 1 day | #1 or #2 most-downloaded Notepad++ plugin (DSpellCheck). Use Electron's built-in `webContents.session.spellCheckerLanguages` + Monaco squiggles via custom marker provider. Works offline, no external service. | ⬜ Planned |
| **Tab color-coding** — right-click tab → pick from 8 accent colours; persists in session | 3 hrs | Notepad++ built-in, praised for large sessions. Simple DOM + session JSON change. | ✅ Shipped |
| **Large-file safe mode** — files > 10 MB auto-disable IntelliSense, minimap, diff gutter, syntax highlight | 4 hrs | Notepad++ handles multi-GB logs; Monaco freezes. Warn user + offer lite mode. Unblocks a real sysadmin/devops use-case. | ✅ Shipped |

## Tier 2 — Ship-soon (1–4 weeks)

The leverage moves. Each one moves Note++'s capability ceiling significantly.

### AI & standards
| Feature | Effort | Why |
|---|---|---|
| **MCP client support** — connect to community MCP servers (filesystem, GitHub, Brave Search, Postgres, etc.) | 1–2 wks | MCP became the universal tool protocol in ~12 months (~97M monthly SDK downloads). Implementing MCP client multiplies capability instantly. Anthropic's protocol; OpenAI / LF agentic-AI foundation backed it. |
| **Inline AI completion** (Cursor Tab pattern) via Ollama FIM | 1 wk | Predictive ghost-text edits as you type. Cursor's Sept 2025 numbers: −21% suggestions, +28% acceptance once they predicted *where* the cursor jumps. Monaco supports this via `registerInlineCompletionsProvider`. |
| **Repo-wide chat** — when a workspace folder is open, AI can read/search across files | 4–5 days | Closes the gap with Cursor's repo chat. Uses the file-tree code we already have. |
| **Inline git diff gutter** (added/modified/deleted markers in editor) | 3–4 days | Already on `GIT.md`'s deferred list. Showed up in both research streams as the universal expectation. Cheap with Monaco's decoration API. |
| **Local semantic search** via `nomic-embed-text` over open workspace files | 1 wk | "Find similar code" / "find file about X" without uploading anywhere. Karpathy's LLM-Wiki pattern + a small embedding model. Works offline. |

### Power-user parity (Notepad++ / Sublime gaps — community research May 2026)
| Feature | Effort | Why |
|---|---|---|
| **User snippets** — define abbreviations (`iife`, `cls`, `log`) that expand to multi-line templates; managed via a JSON file + UI | 1 wk | Monaco has the `registerCompletionItemProvider` + `SnippetString` backend; just needs the management UI (add/edit/delete rows, assign language scope). Sublime Text's most-loved productivity feature; Notepad++'s Snippets plugin is widely installed. |
| **NppExec-style command runner** — `Tools → Run…` dialog: enter any shell command with `$(FILE)` / `$(DIR)` / `$(SEL)` variables; output streams into a new Results tab | 1 wk | More flexible than the current F5 run-in-terminal. Power users use this for compilers, linters, custom scripts. Keeps terminal clean; output is capturable/searchable. |
| **Macro recording & playback** — record a sequence of editor actions, save as named macro, replay with a shortcut or N times | 2–3 wks | Notepad++'s single biggest differentiator over VS Code / Cursor. No Monaco-based editor ships this. Intercept Monaco commands via `editor.onDidType` + action dispatch; serialize to JSON; add a Macros menu. Massive power-user differentiator. |

## Tier 3 — Aspirational (months, transformative)

Each one would be a marketing line on its own.

### AI & transformative
| Feature | Why it's worth it |
|---|---|
| **Encrypted Vault** (multi-note encrypted workspace) | Expand encrypted-pad to a vault model, 1Password-style. Quick toggle between plaintext workspace and encrypted vault. Differentiated against everyone except Standard Notes (and they have no AI). |
| **Background agents on git worktrees** | Cursor 2.0's parallel-agent fan-out, but local. "Try 3 approaches to this refactor in parallel" — Note++ already has git integration; `git worktree add` is the missing piece. |
| **Voice-to-code** (full inline) — Whisper.cpp + Cursor-style voice mode | Real differentiator; Whisper runs offline; opens accessibility. |
| **Vision input** — paste screenshot of a hand-drawn diagram → AI converts to Mermaid | Plays to Note++'s Mermaid + Whiteboard strengths. LLaVA-equivalent runs in Ollama. |
| **Sandbox / permission boundaries** for Agent mode | Confirm-before-shell, confirm-before-multi-file-write, deny-list for sensitive paths. Pitch: "the AI editor that won't `rm -rf` your home folder" — competitive positioning amid the 30+ prompt-injection RCE bugs disclosed across Cursor, Windsurf, Kiro, Copilot, Zed, Roo, Cline, Junie in Dec 2025. |
| **Mobile capture companion** (PWA) | The universal mobile-desktop-parity complaint. Simple PWA captures voice/text → syncs to a Note++ inbox file via cloud-sync. |
| **Karpathy LLM-Wiki view** for `.md` workspaces | "70× more efficient than RAG for personal scale." Curated markdown handed wholesale to long-context model. Good fit for users with a Markdown notes folder. |

### Power-user parity (Notepad++ / Sublime gaps — community research May 2026)
| Feature | Why it's worth it |
|---|---|
| **Hex viewer / editor** — open any binary file in a hex+ASCII split view; edit bytes in place | Popular Notepad++ plugin (HEX-Editor). Niche but high-signal: sysadmins, reverse engineers, firmware devs. Implement via a dedicated tab type backed by a `Uint8Array` model + custom Monaco-free renderer, or find a WASM hex-editor component. |

---

## What we **won't** do (research-backed)

| ❌ Don't | Why |
|---|---|
| **Plugin marketplace** | #1 unspoken Obsidian complaint is plugin supply-chain risk. Skiff (Notion-acquired then killed Feb 2024) showed the danger of becoming dependent on one platform's ecosystem. |
| **Graph view** | "Universally treated as eye-candy, not a workflow." Tana's typed-node supertags > Roam-style backlinks. Graph view is dead as a feature; schema-on-read is alive. |
| **Cloud-native sync of notes** | Notion fatigue is real (HN/Trustpilot complaints cluster around lock-in, surprise billing). Note++ wins by *not* being that. Cloud session sync (already shipped) is fine; cloud-host of notes themselves is not. |
| **AR/VR notes** | IDC: ~14M units by end-2025 vs ~300M PCs sold/year. No breakout note app on Vision Pro. Treat as long-tail bet, not a 1–3yr play. |
| **Compete with Cursor/Windsurf on parallel worktrees, custom foundation models** | Different product category, they raised $10B+. Compete on **focused, lightweight, local-first**. |
| **Proprietary "Note++ rules" file format** | AGENTS.md won the standards war in 12 months. Don't fragment further. |
| **Brain-dead RAG over notes** | Karpathy's LLM-Wiki pattern (curated markdown → long-context LLM) is "70× more efficient" for personal scale. Skip embeddings for notebooks under ~10k notes. |

---

## Notepad++ / Sublime parity research summary (May 2026)

Ten most impactful community-validated gaps vs Notepad++ and Sublime Text, in priority order:

| # | Feature | Signal source | Note++ status |
|---|---|---|---|
| 1 | Macro recording & playback | Notepad++'s single most-cited differentiator vs VS Code; no Monaco editor ships it | Tier 2 |
| 2 | Spell check | #1–2 most-downloaded Notepad++ plugin (DSpellCheck); praised in every user review | Tier 1 |
| 3 | Auto-save to disk | Top plugin (AutoSave); universal request; distinct from session save | Tier 1 |
| 4 | Distraction-free / Zen mode | Top Sublime Text praised feature; writers + focus use-case | Tier 1 |
| 5 | User snippets | Sublime's most-loved productivity feature; Monaco backend already exists | Tier 2 |
| 6 | Tab color-coding | Notepad++ built-in; praised for large multi-project sessions | Tier 1 |
| 7 | Hex viewer | Popular HEX-Editor plugin; niche but high-signal for target users | Tier 3 |
| 8 | Large-file safe mode | Notepad++ handles GB+ logs; Monaco struggles; devops use-case | Tier 1 |
| 9 | NppExec-style command runner | Power-user staple; more flexible than F5 terminal runner | Tier 2 |

Research sources: Notepad++ community forum, GitHub issues, TrustRadius/G2 reviews, Hacker News Sublime Thread (Jan 2026), plugin download rankings, developer surveys.

---

## Underlying research signals (for posterity)

- **Cursor 2.0** (Oct 2025): own foundation model "Composer", parallel agents on git worktrees, voice control
- **Windsurf** (Cognition): Cascade planner+executor split
- **Zed**: GPL open-source, $32M Sequoia, deep Anthropic integration
- **VS Code + Copilot**: Agent Mode + MCP GA, Next Edit Suggestions, Prompt Files, Agent Skills, Agents Window
- **Cline** (~58k stars), **Roo Code** (~22k stars), **Continue** (~32k stars) — open-source agents winning
- **MCP**: universal in ~12 months; ~97M monthly SDK downloads
- **AGENTS.md**: 60k+ repos in a year
- **Granola**: $43M Series B → $125M Series C ($1.5B val) in months
- **Notion 3.0** (Sept 2025): AI Agents → **Custom Agents** (Feb 2026)
- **Standard Notes** acquired by Proton AG (Apr 2024); **Skiff** acquired and shut by Notion (Feb 2024)
- **Tana**: supertags = the genuine Roam-evolution
- **30+ prompt-injection RCE bugs** in AI IDEs (Dec 2025 disclosure)
- **Local AI**: Qwen2.5-Coder 32B matches GPT-4o on HumanEval; Ollama is the dominant runner

---

## What "done" looks like for v1.3

Tier 1 + a chunk of Tier 2 (MCP + inline completion). Then Note++ has:

- Standards-compliant AI hosting (`AGENTS.md`, `.notepp/memory.md`, MCP)
- Multi-model + quick-action UX
- Voice capture (where supported)
- Inline ghost-text completions
- Repo-wide chat
- Plus everything already shipped (Git, encryption, whiteboard, agent diff)

That stack alone moves Note++ from "good 2024 editor with AI" to "competitive 2026 editor with a clear differentiation story" — without taking on any of the risks (plugin ecosystem, cloud sync, AR/VR) the research warns against.
