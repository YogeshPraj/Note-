// =============================================================================
// Note++ — Whiteboard (Excalidraw) helpers
// =============================================================================
// Extracted from renderer.js. Three pure functions that participate in tab
// management — they read/write the shared `tabs` / `tabCounter` globals
// (declared with `let` at the top of renderer.js, which makes them visible
// to all classic scripts in the same global scope).
//
// Functions not extracted (intentionally — too entangled):
//   - activateTab           (the big switch on tab.type — touches every type)
//   - closeTab              (whiteboard close has legacy-AppData migration)
//   - the wb-ready/wb-state postMessage bridge in renderer.js
// =============================================================================

// Lowest integer N ≥ 1 not used by an existing whiteboard tab name.
// Accepts both the new "whiteboard-N" form and the legacy
// "whiteboard-N.json" form so a session restored from before v1.5.x
// doesn't reuse numbers.
function nextWbTabNumber() {
  const used = new Set(
    tabs
      .filter(t => t.type === 'whiteboard')
      .map(t => {
        const m = t.name.match(/^whiteboard-(\d+)(?:\.json)?$/);
        return m ? parseInt(m[1], 10) : null;
      })
      .filter(n => n !== null)
  );
  for (let n = 1; ; n++) if (!used.has(n)) return n;
}

// Post a message to the (shared) whiteboard iframe. No-op until the iframe
// is mounted in activateTab.
function sendToWhiteboard(msg) {
  const frame = document.getElementById('whiteboard-frame');
  if (frame && frame.contentWindow) {
    frame.contentWindow.postMessage(msg, '*');
  }
}

// Create a new whiteboard tab (or activate an existing one if filePath is
// already open). Mirrors text-tab UX — no auto-saved AppData backing file;
// red dot only appears on first edit; Ctrl+S → Save As for new tabs.
function createWhiteboardTab(filePath, content) {
  if (filePath) {
    const existing = tabs.find(t => t.filePath === filePath && t.type === 'whiteboard');
    if (existing) { activateTab(existing.id); return existing; }
  }
  tabCounter++;
  const id = tabCounter;
  const name = filePath
    ? filePath.split(/[\\/]/).pop()
    : `whiteboard-${nextWbTabNumber()}`;
  const tab = {
    id, name,
    filePath: filePath || null,
    content: content || '',
    dirty: false,
    language: 'whiteboard',
    encoding: 'UTF-8',
    eol: 'Windows (CR LF)',
    model: null, viewState: null,
    type: 'whiteboard'
  };
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}
