'use strict';
// =============================================================================
// term-links.js — make file:line references in terminal output clickable
// -----------------------------------------------------------------------------
// A stack trace, a compiler error, a failing test — every one of them names a
// file and a line, and following it by hand means selecting the path, opening
// it, then scrolling to the line. This turns those references into links:
// Ctrl/Cmd+click (or plain click) opens the file in a tab and jumps to the
// exact line and column.
//
// Registered on the xterm instance via registerLinkProvider, which hands us one
// buffer line at a time and expects back the ranges we want underlined. xterm
// columns are 1-based and inclusive, which is the usual off-by-one trap here.
//
// Loaded as a classic script sharing renderer.js's global scope.
// =============================================================================

// Patterns are ordered most-specific first. Each yields { path, line, col }.
// `g` flag is required — we walk every match on the line.
const TERM_LINK_PATTERNS = [
  // Python: File "script.py", line 42
  {
    re: /File "([^"]+)", line (\d+)/g,
    pick: m => ({ path: m[1], line: +m[2], col: 1 }),
  },
  // MSBuild / TypeScript: src/app.ts(12,5)
  {
    re: /([A-Za-z]:[\\/][^\s()<>"']+|[^\s()<>"':]+\.[A-Za-z0-9]{1,8})\((\d+),(\d+)\)/g,
    pick: m => ({ path: m[1], line: +m[2], col: +m[3] }),
  },
  // The common one: path/to/file.js:12:5  or  C:\path\file.ts:12
  // Requires an extension so we don't linkify "http://host:8080" or "12:30:45".
  {
    re: /((?:[A-Za-z]:)?(?:[.\w@~-]+[\\/])*[.\w@-]+\.[A-Za-z0-9]{1,8}):(\d+)(?::(\d+))?/g,
    pick: m => ({ path: m[1], line: +m[2], col: m[3] ? +m[3] : 1 }),
  },
];

// Things that look like file:line but aren't worth opening.
function termLinkLooksBogus(path) {
  if (!path) return true;
  if (/^https?:$/i.test(path)) return true;      // stray protocol fragment
  if (/^\d+$/.test(path)) return true;           // "12:30" style timestamps
  return false;
}

// Resolve a possibly-relative path against the best root we know. The terminal
// spawns in the workspace folder, so that's the right base for `src/app.js`
// style output; absolute paths pass straight through.
function termLinkResolve(p) {
  if (!p) return null;
  const looksAbsolute = /^([A-Za-z]:[\\/]|[\\/])/.test(p);
  if (looksAbsolute) return p;
  const root =
    (typeof fileTreeRoot === 'string' && fileTreeRoot) ||
    (typeof activeGitRepo === 'string' && activeGitRepo) ||
    (typeof tsResolveWorkspaceRoot === 'function' && tsResolveWorkspaceRoot()) ||
    null;
  if (!root) return null;
  const sep = root.includes('\\') ? '\\' : '/';
  return root.replace(/[\\/]+$/, '') + sep + p.replace(/^[.][\\/]/, '');
}

function termLinkScan(text) {
  const found = [];
  for (const { re, pick } of TERM_LINK_PATTERNS) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(text)) !== null) {
      const hit = pick(m);
      if (termLinkLooksBogus(hit.path)) continue;
      const start = m.index;
      const end = m.index + m[0].length;
      // A URL's host:port is indistinguishable from file.ext:line on its own
      // ("example.com:8443"). What separates them is the "//" in front.
      if (/\/\/$/.test(text.slice(0, start))) continue;
      // Don't let a later, looser pattern re-match text an earlier one claimed.
      if (found.some(f => start < f.end && end > f.start)) continue;
      found.push({ ...hit, start, end, text: m[0] });
    }
  }
  return found.sort((a, b) => a.start - b.start);
}

// Open the file and put the caret on the referenced line/column.
async function termLinkOpen(hit) {
  const resolved = termLinkResolve(hit.path);
  if (!resolved) { showToast(`Can't resolve "${hit.path}" — open its folder first`); return; }

  try {
    const r = await window.electronAPI.validatePath(resolved);
    if (r && r.exists === false) { showToast(`Not found: ${resolved}`); return; }
  } catch { /* if the check itself fails, still try to open */ }

  try {
    const already = tabs.find(t => t.filePath && t.filePath.toLowerCase() === resolved.toLowerCase());
    if (already) activateTab(already.id);
    else await openFile(resolved);

    // openFile is async and swaps the model; let it settle before moving the
    // caret, or the reveal lands on the outgoing document.
    setTimeout(() => {
      try {
        const line = Math.max(1, hit.line || 1);
        const col = Math.max(1, hit.col || 1);
        editor.revealLineInCenter(line);
        editor.setPosition({ lineNumber: line, column: col });
        editor.focus();
      } catch {}
    }, 120);
  } catch (err) {
    showToast('Could not open ' + hit.path);
  }
}

// xterm's provider contract: given a 1-based buffer line, call back with the
// links on it (or undefined). Ranges are 1-based and inclusive on both ends.
function registerTerminalLinks(term) {
  if (!term || typeof term.registerLinkProvider !== 'function') return null;
  return term.registerLinkProvider({
    provideLinks(bufferLineNumber, callback) {
      let text = '';
      try {
        const line = term.buffer.active.getLine(bufferLineNumber - 1);
        if (!line) return callback(undefined);
        text = line.translateToString(true);
      } catch { return callback(undefined); }
      if (!text || text.length < 4) return callback(undefined);

      const hits = termLinkScan(text);
      if (!hits.length) return callback(undefined);

      callback(hits.map(hit => ({
        range: {
          start: { x: hit.start + 1, y: bufferLineNumber },
          end:   { x: hit.end,       y: bufferLineNumber },
        },
        text: hit.text,
        activate: () => termLinkOpen(hit),
      })));
    },
  });
}
