// =============================================================================
// Note++ — draw.io tab + download helpers
// =============================================================================
// Extracted from renderer.js. Six functions that participate in tab
// management for the draw.io tab type. They read/write the shared globals
// declared with `let` at the top of renderer.js — `tabs`, `tabCounter`,
// `drawioReady`, `drawioPendingContent` — which classic scripts in the
// same global scope all share.
//
// Pairs with src/drawio-templates.js (pure data + thin shim) and
// src/drawio.html (the iframe shell). Functions NOT extracted because
// they're entangled with the big switch in renderer.js:
//   - activateTab's drawio branch (the iframe load + dw-load orchestration)
//   - closeTab's drawio branch
//   - the dw-ready / dw-state postMessage handlers
//   - saveTabFile's drawio branch
// =============================================================================

// Lowest unused integer N for a new "drawing-N" tab name.
function nextDrawioTabNumber() {
  const used = new Set(
    tabs
      .filter(t => t.type === 'drawio')
      .map(t => { const mm = t.name.match(/^drawing-(\d+)(?:\.drawio)?$/); return mm ? parseInt(mm[1], 10) : null; })
      .filter(n => n !== null)
  );
  for (let n = 1; ; n++) if (!used.has(n)) return n;
}

// Post a message to the (shared) drawio iframe shell. Target is
// #drawio-frame-host (renderer-side iframe); the inner #drawio-frame
// element lives only inside drawio.html.
function sendToDrawio(msg) {
  const frame = document.getElementById('drawio-frame-host');
  if (frame && frame.contentWindow) frame.contentWindow.postMessage(msg, '*');
}

// Show a modal that downloads the drawio bundle to userData.
// Returns true on success, false on cancel / network failure. Resolves
// only after the download is fully done (or the user dismissed it).
async function showDrawioDownloadModal() {
  // Lazy-create the modal markup on first use
  let modal = document.getElementById('drawio-download-modal');
  if (!modal) {
    modal = document.createElement('div');
    modal.id = 'drawio-download-modal';
    modal.className = 'modal-overlay hidden';
    modal.innerHTML = `
      <div class="modal-box" style="max-width:460px">
        <div class="modal-header">
          <span>Download draw.io</span>
        </div>
        <div class="modal-body" style="padding:24px;text-align:center">
          <div style="font-size:13px;color:var(--fg-secondary);margin-bottom:14px">
            draw.io isn't bundled with Note++. We'll download it once
            (≈40 MB) and reuse it across upgrades.
          </div>
          <div id="dw-dl-phase" style="font-size:13px;font-weight:600;margin-bottom:8px">Starting…</div>
          <div style="width:100%;height:6px;background:#374151;border-radius:3px;overflow:hidden">
            <div id="dw-dl-bar" style="width:0%;height:100%;background:linear-gradient(90deg,#3b82f6,#60a5fa);transition:width 0.15s linear"></div>
          </div>
          <div id="dw-dl-detail" style="font-size:11px;color:var(--fg-secondary);margin-top:6px">&nbsp;</div>
          <div id="dw-dl-error" style="font-size:12px;color:#dc2626;margin-top:10px;display:none"></div>
        </div>
        <div class="modal-footer">
          <button class="modal-btn" id="dw-dl-cancel">Cancel</button>
          <button class="modal-btn primary hidden" id="dw-dl-retry">Retry</button>
        </div>
      </div>`;
    document.body.appendChild(modal);
  }

  const phaseEl  = modal.querySelector('#dw-dl-phase');
  const barEl    = modal.querySelector('#dw-dl-bar');
  const detailEl = modal.querySelector('#dw-dl-detail');
  const errEl    = modal.querySelector('#dw-dl-error');
  const cancelEl = modal.querySelector('#dw-dl-cancel');
  const retryEl  = modal.querySelector('#dw-dl-retry');

  modal.classList.remove('hidden');
  errEl.style.display = 'none';
  retryEl.classList.add('hidden');
  barEl.style.width = '0%';
  detailEl.textContent = '';

  return new Promise((resolve) => {
    let settled = false;
    function finish(result) {
      if (settled) return;
      settled = true;
      window.electronAPI.drawio.removeProgressListener();
      modal.classList.add('hidden');
      resolve(result);
    }

    cancelEl.onclick = () => finish(false);

    window.electronAPI.drawio.onProgress((p) => {
      if (p.phase === 'download') {
        phaseEl.textContent = 'Downloading draw.io…';
        barEl.style.width = (p.percent || 0) + '%';
        if (p.totalBytes) {
          const mb = (b) => (b / (1024 * 1024)).toFixed(1);
          detailEl.textContent = `${mb(p.bytes)} / ${mb(p.totalBytes)} MB`;
        }
      } else if (p.phase === 'extract') {
        phaseEl.textContent = 'Extracting…'; barEl.style.width = '95%'; detailEl.textContent = '';
      } else if (p.phase === 'install') {
        phaseEl.textContent = 'Installing…'; barEl.style.width = '98%';
      } else if (p.phase === 'done') {
        phaseEl.textContent = `draw.io ${p.version} installed`; barEl.style.width = '100%';
      }
    });

    async function attempt() {
      errEl.style.display = 'none';
      retryEl.classList.add('hidden');
      const res = await window.electronAPI.drawio.download();
      if (res?.success) {
        // Brief pause so the user sees "installed" before the modal closes
        setTimeout(() => finish(true), 700);
      } else {
        errEl.textContent = 'Download failed: ' + (res?.error || 'unknown error');
        errEl.style.display = 'block';
        retryEl.classList.remove('hidden');
        retryEl.onclick = attempt;
      }
    }
    attempt();
  });
}

// Ensures the drawio bundle is installed. Returns true if available, false
// if the user cancelled or the download failed.
async function ensureDrawioInstalled() {
  const s = await window.electronAPI.drawio.status();
  if (s.installed && !s.needsUpdate) return true;
  return await showDrawioDownloadModal();
}

// Manual update check — invoked from "? → Diagram (draw.io) → Check for updates"
async function checkDrawioForUpdates() {
  const s = await window.electronAPI.drawio.status();
  if (!s.installed) {
    showToast('draw.io isn\'t installed yet — open a draw.io tab to download it.');
    return;
  }
  if (s.installedVersion === s.requestedVersion) {
    showToast(`draw.io is up to date (v${s.installedVersion}).`);
    return;
  }
  const r = await window.electronAPI.messageDialog({
    type: 'question', title: 'Update draw.io',
    message: `draw.io update available: v${s.installedVersion} → v${s.requestedVersion}. Download now?`,
    buttons: ['Update', 'Later'], defaultId: 0, cancelId: 1,
  });
  if (r.response === 0) await showDrawioDownloadModal();
}

// Create (or re-activate) a draw.io tab. Triggers the download modal on
// first use; closes the placeholder tab cleanly if the user cancels.
function createDrawioTab(filePath, content) {
  if (filePath) {
    const existing = tabs.find(t => t.filePath === filePath && t.type === 'drawio');
    if (existing) { activateTab(existing.id); return existing; }
  }
  tabCounter++;
  const id = tabCounter;
  const name = filePath
    ? filePath.split(/[\\/]/).pop()
    : `drawing-${nextDrawioTabNumber()}`;
  const tab = {
    id, name,
    filePath: filePath || null,
    content: content || '',
    dirty: false,
    language: 'drawio',
    encoding: 'UTF-8',
    eol: 'Windows (CR LF)',
    model: null, viewState: null,
    type: 'drawio',
  };
  tabs.push(tab);
  // Activate after install check so we don't show a blank iframe.
  ensureDrawioInstalled().then(ok => {
    if (!ok) {
      // User cancelled or download failed — close the placeholder tab.
      const idx = tabs.findIndex(t => t.id === id);
      if (idx >= 0) tabs.splice(idx, 1);
      renderTabs();
      // Re-activate something useful
      if (tabs.length === 0) createTab();
      else activateTab(tabs[tabs.length - 1].id);
      return;
    }
    activateTab(id);
    renderTabs();
  });
  return tab;
}
