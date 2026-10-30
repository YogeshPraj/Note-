'use strict';
// =============================================================================
// vault-panel.js — Passwords tab (renderer)
// -----------------------------------------------------------------------------
// Front-end for the Bitwarden CLI that main downloads on demand. This file
// never sees the vault: it renders metadata (titles, usernames) from
// `bw list`, and secrets only arrive one at a time, for a moment, when the
// user clicks Reveal. Copy and Insert don't route through here at all — main
// fetches the value and either writes the clipboard or pushes the text
// straight into the editor.
//
// Loaded as a classic script after renderer.js, same as tasks-panel.js.
// =============================================================================

const vaultState = {
  status: null,        // last `vault:status` result
  items: [],
  selectedId: null,
  search: '',
  tabId: null,
  installing: false,
  installPct: 0,
  installPhase: '',
  revealTimer: null,
  unlockMode: 'password',   // 'password' | 'token'
  creating: false,          // the "New item" form is showing
  busy: false,
  error: '',
};

const VAULT_REVEAL_MS = 20000;

function vEsc(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g,
    c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// ── Tab lifecycle ───────────────────────────────────────────────────────────

function openVaultTab() {
  if (vaultState.tabId != null) {
    const existing = tabs.find(t => t.id === vaultState.tabId);
    if (existing) { activateTab(existing.id); return existing; }
  }
  tabCounter++;
  const id = tabCounter;
  const tab = {
    id, name: '🔐 Passwords', filePath: null, content: '', dirty: false,
    language: 'plaintext', encoding: 'UTF-8', eol: 'Windows (CR LF)',
    model: null, viewState: null, type: 'vault',
  };
  vaultState.tabId = id;
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}

function closeVaultTabState() {
  vaultState.tabId = null;
  vaultState.items = [];
  vaultState.selectedId = null;
  clearTimeout(vaultState.revealTimer);
}

// Main refuses to hand over a secret unless this tab is in front, so keep it
// informed on every activation.
function notifyVaultTabActive(active) {
  try { window.electronAPI.vault.setTabActive(!!active); } catch {}
}

// ── Data ────────────────────────────────────────────────────────────────────

async function vaultRefreshStatus() {
  try { vaultState.status = await window.electronAPI.vault.status(); }
  catch (err) { vaultState.status = { success: false, error: err.message }; }
}

async function vaultRefreshItems() {
  const r = await window.electronAPI.vault.list(vaultState.search);
  if (r && r.success) { vaultState.items = r.items || []; vaultState.error = ''; }
  else { vaultState.items = []; vaultState.error = (r && r.error) || 'Could not list items'; }
}

async function renderVaultTab() {
  const host = document.getElementById('vault-tab-container');
  if (!host) return;
  if (!vaultState.status) await vaultRefreshStatus();
  const st = vaultState.status || {};

  if (vaultState.installing)                 return renderInstalling(host);
  if (!st.supported)                         return renderUnsupported(host, st);
  if (!st.installed && !st.onPath)           return renderNeedsInstall(host, st);
  if (st.auth && st.auth.state === 'unauthenticated') return renderNeedsSignIn(host, st);
  if (!st.unlocked)                          return renderLocked(host, st);
  return renderUnlocked(host, st);
}

function shell(inner, statusLine) {
  return `
    <div id="vault-toolbar">
      <span class="v-title">🔐 Passwords</span>
      <span class="v-sub">${statusLine || ''}</span>
      <div style="flex:1"></div>
      <div id="vault-toolbar-actions"></div>
    </div>
    <div id="vault-body">${inner}</div>`;
}

// ── Screens ─────────────────────────────────────────────────────────────────

function renderUnsupported(host, st) {
  host.innerHTML = shell(`
    <div class="v-card">
      <h2>Not available on this platform</h2>
      <p>Bitwarden doesn't publish a CLI build for <code>${vEsc(st.platform)}</code>.</p>
    </div>`, '');
}

function renderNeedsInstall(host, st) {
  host.innerHTML = shell(`
    <div class="v-card">
      <h2>Connect Bitwarden</h2>
      <p>Note++ doesn't store passwords itself. It uses the official
         <b>Bitwarden CLI</b> — your vault, your account, Bitwarden's encryption.
         Note++ only ever holds a session token in memory.</p>
      <ul class="v-facts">
        <li>Downloads <b>v${vEsc(st.pinnedVersion)}</b> from Bitwarden's official GitHub release (~40&nbsp;MB)</li>
        <li>Verified against a <b>SHA-256 pinned in Note++'s source</b> before it's allowed to run</li>
        <li>Installed under your user data — survives Note++ upgrades</li>
        <li>Already have <code>bw</code>? Put it on your PATH and reopen this tab.</li>
      </ul>
      <button class="v-btn primary" id="v-install">Download Bitwarden CLI</button>
      <div class="v-err" id="v-err">${vEsc(vaultState.error)}</div>
    </div>`, 'not connected');
  document.getElementById('v-install').addEventListener('click', doInstall);
}

function renderInstalling(host) {
  const pct = Math.max(0, Math.min(100, vaultState.installPct));
  const label = { download: 'Downloading', verify: 'Verifying checksum', extract: 'Extracting', done: 'Done' }[vaultState.installPhase] || 'Working';
  host.innerHTML = shell(`
    <div class="v-card">
      <h2>${vEsc(label)}…</h2>
      <div class="v-bar"><div class="v-bar-fill" style="width:${pct}%"></div></div>
      <p class="v-muted">${pct}%${vaultState.installPhase === 'verify' ? ' — checking the download matches the pinned hash' : ''}</p>
      <div class="v-err" id="v-err">${vEsc(vaultState.error)}</div>
    </div>`, 'installing');
}

function renderNeedsSignIn(host, st) {
  host.innerHTML = shell(`
    <div class="v-card">
      <h2>Sign in to Bitwarden</h2>
      <p class="v-muted">Your credentials go straight to the Bitwarden CLI. Note++ keeps only the session it returns, in memory.</p>
      <label class="v-field"><span>Email</span><input type="text" id="v-email" autocomplete="username"></label>
      <label class="v-field"><span>Master password</span><input type="password" id="v-pw" autocomplete="current-password"></label>
      <button class="v-btn primary" id="v-login">Sign in</button>
      <div class="v-note">Using two-step login? Run <code>bw login</code> in the terminal once, then use
        <b>“I have a session token”</b> on the next screen — your password then never touches Note++ at all.</div>
      <div class="v-err" id="v-err">${vEsc(vaultState.error)}</div>
    </div>`, 'signed out');
  document.getElementById('v-login').addEventListener('click', doLogin);
  document.getElementById('v-pw').addEventListener('keydown', e => { if (e.key === 'Enter') doLogin(); });
}

function renderLocked(host, st) {
  const byToken = vaultState.unlockMode === 'token';
  host.innerHTML = shell(`
    <div class="v-card">
      <h2>Unlock vault</h2>
      ${st.auth && st.auth.email ? `<p class="v-muted">Signed in as <b>${vEsc(st.auth.email)}</b></p>` : ''}
      ${byToken ? `
        <label class="v-field"><span>Session token</span>
          <input type="text" id="v-token" spellcheck="false" placeholder="output of: bw unlock --raw"></label>
        <div class="v-note">Run <code>bw unlock --raw</code> in the integrated terminal
          (<b>Ctrl+\`</b>) and paste the token here. Your master password never
          reaches Note++ this way.</div>
        <button class="v-btn primary" id="v-use-token">Use token</button>
      ` : `
        <label class="v-field"><span>Master password</span>
          <input type="password" id="v-pw" autocomplete="current-password"></label>
        <button class="v-btn primary" id="v-unlock">Unlock</button>
      `}
      <div class="v-alt">
        <button class="v-link" id="v-toggle-mode">${byToken ? 'Use master password instead' : 'I have a session token instead'}</button>
        <button class="v-link" id="v-adopt">Use existing BW_SESSION</button>
      </div>
      <div class="v-err" id="v-err">${vEsc(vaultState.error)}</div>
    </div>`, 'locked');

  document.getElementById('v-toggle-mode').addEventListener('click', () => {
    vaultState.unlockMode = byToken ? 'password' : 'token';
    vaultState.error = '';
    renderVaultTab();
  });
  document.getElementById('v-adopt').addEventListener('click', doAdopt);
  if (byToken) {
    document.getElementById('v-use-token').addEventListener('click', doUseToken);
    document.getElementById('v-token').addEventListener('keydown', e => { if (e.key === 'Enter') doUseToken(); });
  } else {
    document.getElementById('v-unlock').addEventListener('click', doUnlock);
    const pw = document.getElementById('v-pw');
    pw.addEventListener('keydown', e => { if (e.key === 'Enter') doUnlock(); });
    pw.focus();
  }
}

function renderUnlocked(host, st) {
  const q = vaultState.search.trim().toLowerCase();
  const items = q
    ? vaultState.items.filter(i => (i.title + ' ' + i.username).toLowerCase().includes(q))
    : vaultState.items;

  host.innerHTML = shell(`
    <div id="vault-split">
      <div id="vault-list-pane">
        <div id="vault-search-row">
          <input type="text" id="v-search" placeholder="Search vault…" value="${vEsc(vaultState.search)}">
        </div>
        <div id="vault-list">${
          items.length
            ? items.map(i => `
                <div class="v-item${i.id === vaultState.selectedId ? ' selected' : ''}" data-id="${vEsc(i.id)}">
                  <div class="v-item-title">${vEsc(i.title)}</div>
                  <div class="v-item-sub">
                    ${i.username ? `<span>${vEsc(i.username)}</span>` : ''}
                    ${i.hasTotp ? '<span class="v-badge">TOTP</span>' : ''}
                  </div>
                </div>`).join('')
            : `<div class="v-empty">${vaultState.error ? vEsc(vaultState.error) : (vaultState.items.length ? 'Nothing matches.' : 'Vault is empty, or not synced yet — try Sync.')}</div>`
        }</div>
      </div>
      <div id="vault-detail">${vaultDetailHtml()}</div>
    </div>`, `unlocked · ${vaultState.items.length} items${st.autoLockMinutes ? ` · auto-locks in ${st.autoLockMinutes}m` : ''}`);

  const actions = document.getElementById('vault-toolbar-actions');
  actions.innerHTML = `
    <button class="v-btn primary" id="v-new">+ New item</button>
    <button class="v-btn" id="v-sync">Sync</button>
    <button class="v-btn" id="v-lock">Lock</button>`;
  document.getElementById('v-new').addEventListener('click', () => {
    vaultState.creating = true;
    vaultState.selectedId = null;
    renderVaultTab();
  });
  document.getElementById('v-sync').addEventListener('click', doSync);
  document.getElementById('v-lock').addEventListener('click', doLock);

  const search = document.getElementById('v-search');
  let deb;
  search.addEventListener('input', () => {
    clearTimeout(deb);
    deb = setTimeout(() => { vaultState.search = search.value; renderVaultTab(); }, 160);
  });

  document.querySelectorAll('.v-item').forEach(el =>
    el.addEventListener('click', () => {
      clearTimeout(vaultState.revealTimer);
      vaultState.selectedId = el.dataset.id;
      renderVaultTab();
    }));

  wireDetail();
}

function vaultCreateHtml() {
  return `
    <div class="v-detail">
      <h2>New item</h2>
      <div class="v-note">Saved straight into your Bitwarden vault via the CLI.
        Note++ keeps no copy. Editing and deleting stay in the Bitwarden app —
        this only ever adds.</div>
      <label class="v-field"><span>Name *</span><input type="text" id="n-title" spellcheck="false"></label>
      <label class="v-field"><span>Username</span><input type="text" id="n-username" spellcheck="false" autocomplete="off"></label>
      <label class="v-field"><span>Password</span><input type="text" id="n-password" spellcheck="false" autocomplete="off"></label>
      <div class="v-genrow">
        <button class="v-mini" id="n-gen">Generate</button>
        <label>length <input type="number" id="n-len" value="20" min="5" max="128"></label>
        <label><input type="checkbox" id="n-sym" checked> symbols</label>
        <span class="v-genhint">uses <code>bw generate</code></span>
      </div>
      <label class="v-field"><span>URL</span><input type="text" id="n-url" spellcheck="false"></label>
      <label class="v-field"><span>TOTP secret or otpauth:// URI</span><input type="text" id="n-totp" spellcheck="false" autocomplete="off"></label>
      <label class="v-field"><span>Notes</span><input type="text" id="n-notes"></label>
      <div class="v-row" style="margin-top:16px">
        <button class="v-btn primary" id="n-save">Save to Bitwarden</button>
        <button class="v-btn" id="n-cancel">Cancel</button>
      </div>
      <div class="v-err" id="n-err"></div>
    </div>`;
}

function wireCreate() {
  const gen = document.getElementById('n-gen');
  if (!gen) return;
  gen.addEventListener('click', async () => {
    gen.disabled = true; const t = gen.textContent; gen.textContent = '…';
    const r = await window.electronAPI.vault.generate({
      length: parseInt(document.getElementById('n-len').value, 10) || 20,
      symbols: document.getElementById('n-sym').checked,
    });
    gen.disabled = false; gen.textContent = t;
    if (r.success) document.getElementById('n-password').value = r.value;
    else showToast(r.error || 'Generate failed');
  });
  document.getElementById('n-cancel').addEventListener('click', () => {
    vaultState.creating = false; renderVaultTab();
  });
  document.getElementById('n-save').addEventListener('click', doCreate);
  document.getElementById('n-title').focus();
}

async function doCreate() {
  const err = document.getElementById('n-err');
  const btn = document.getElementById('n-save');
  const item = {
    title:    document.getElementById('n-title').value.trim(),
    username: document.getElementById('n-username').value,
    password: document.getElementById('n-password').value,
    url:      document.getElementById('n-url').value.trim(),
    totp:     document.getElementById('n-totp').value.trim(),
    notes:    document.getElementById('n-notes').value,
  };
  if (!item.title) { err.textContent = 'Name is required.'; return; }
  btn.disabled = true; btn.textContent = 'Saving…';
  const r = await window.electronAPI.vault.create(item);
  btn.disabled = false; btn.textContent = 'Save to Bitwarden';
  if (!r.success) { err.textContent = r.error || 'Could not save'; return; }
  vaultState.creating = false;
  await vaultRefreshItems();
  if (r.id) vaultState.selectedId = r.id;
  renderVaultTab();
  showToast('Saved to Bitwarden');
}

function vaultDetailHtml() {
  if (vaultState.creating) return vaultCreateHtml();
  const i = vaultState.items.find(x => x.id === vaultState.selectedId);
  if (!i) return '<div class="v-empty">Select an item, or click <b>+ New item</b></div>';
  const row = (label, field, present) => present ? `
    <div class="v-row">
      <div class="v-lbl">${label}</div>
      <div class="v-val masked" data-val="${field}">••••••••••••</div>
      <button class="v-mini" data-act="reveal" data-field="${field}">Reveal</button>
      <button class="v-mini" data-act="copy" data-field="${field}">Copy</button>
      <button class="v-mini primary" data-act="insert" data-field="${field}">Insert</button>
    </div>` : '';
  return `
    <div class="v-detail">
      <h2>${vEsc(i.title)}</h2>
      <div class="v-note">Served live by Bitwarden. Note++ stores nothing and caches nothing.</div>
      ${i.username ? `
        <div class="v-row">
          <div class="v-lbl">Username</div>
          <div class="v-val">${vEsc(i.username)}</div>
          <button class="v-mini" data-act="copy" data-field="username">Copy</button>
          <button class="v-mini primary" data-act="insert" data-field="username">Insert</button>
        </div>` : ''}
      ${row('Password', 'password', i.hasPassword)}
      ${row('TOTP', 'totp', i.hasTotp)}
      ${i.url ? `<div class="v-row"><div class="v-lbl">URL</div><div class="v-val">${vEsc(i.url)}</div></div>` : ''}
      <div class="v-hint">“Insert” drops the value at your cursor without touching the clipboard — prefer it over Copy.</div>
    </div>`;
}

function wireDetail() {
  if (vaultState.creating) { wireCreate(); return; }
  document.querySelectorAll('#vault-detail [data-act]').forEach(b =>
    b.addEventListener('click', () => onVaultAction(b.dataset.act, b.dataset.field, b)));
}

async function onVaultAction(act, field, btn) {
  const id = vaultState.selectedId;
  if (!id) return;
  const orig = btn.textContent;
  btn.disabled = true; btn.textContent = '…';
  try {
    if (act === 'reveal') {
      const cell = document.querySelector(`[data-val="${field}"]`);
      if (cell && !cell.classList.contains('masked')) {   // toggle back
        cell.textContent = '••••••••••••'; cell.classList.add('masked');
        clearTimeout(vaultState.revealTimer);
        return;
      }
      const r = await window.electronAPI.vault.reveal(id, field);
      if (!r.success) { showToast(r.error || 'Could not reveal'); return; }
      if (cell) {
        cell.textContent = r.value;
        cell.classList.remove('masked');
        clearTimeout(vaultState.revealTimer);
        vaultState.revealTimer = setTimeout(() => {
          const c = document.querySelector(`[data-val="${field}"]`);
          if (c) { c.textContent = '••••••••••••'; c.classList.add('masked'); }
        }, VAULT_REVEAL_MS);
      }
      return;
    }
    if (act === 'copy') {
      const r = await window.electronAPI.vault.copy(id, field);
      showToast(r.success ? (r.warning || `Copied — clears in ${Math.round((r.clearMs || 30000) / 1000)}s`) : (r.error || 'Copy failed'));
      return;
    }
    if (act === 'insert') {
      const r = await window.electronAPI.vault.insert(id, field);
      if (!r.success) showToast(r.error || 'Could not insert');
      return;
    }
  } finally {
    btn.disabled = false; btn.textContent = orig;
  }
}

// ── Actions ─────────────────────────────────────────────────────────────────

async function doInstall() {
  vaultState.installing = true; vaultState.installPct = 0;
  vaultState.installPhase = 'download'; vaultState.error = '';
  renderVaultTab();
  const r = await window.electronAPI.vault.install();
  vaultState.installing = false;
  if (!r.success) {
    vaultState.error = r.error || 'Install failed';
    // A checksum mismatch is worth shouting about rather than burying.
    if (/checksum/i.test(vaultState.error)) {
      showToast('Download failed integrity check — nothing was installed', 6000);
    }
  }
  await vaultRefreshStatus();
  renderVaultTab();
}

async function doLogin() {
  const email = document.getElementById('v-email').value.trim();
  const pw = document.getElementById('v-pw').value;
  if (!email || !pw) return;
  vaultState.error = '';
  const r = await window.electronAPI.vault.login(email, pw);
  document.getElementById('v-pw').value = '';
  if (!r.success) { vaultState.error = r.error || 'Sign in failed'; renderVaultTab(); return; }
  await afterUnlock();
}

async function doUnlock() {
  const el = document.getElementById('v-pw');
  const pw = el.value;
  if (!pw) return;
  vaultState.error = '';
  const r = await window.electronAPI.vault.unlock(pw);
  el.value = '';
  if (!r.success) { vaultState.error = r.error || 'Unlock failed'; renderVaultTab(); return; }
  await afterUnlock();
}

async function doUseToken() {
  const el = document.getElementById('v-token');
  const tok = el.value.trim();
  if (!tok) return;
  vaultState.error = '';
  const r = await window.electronAPI.vault.useToken(tok);
  el.value = '';
  if (!r.success) { vaultState.error = r.error || 'Token rejected'; renderVaultTab(); return; }
  await afterUnlock();
}

async function doAdopt() {
  vaultState.error = '';
  const r = await window.electronAPI.vault.adoptSession();
  if (!r.success) { vaultState.error = r.error || 'No usable BW_SESSION found'; renderVaultTab(); return; }
  await afterUnlock();
}

async function afterUnlock() {
  await vaultRefreshStatus();
  await vaultRefreshItems();
  renderVaultTab();
}

async function doSync() {
  showToast('Syncing…');
  const r = await window.electronAPI.vault.sync();
  if (!r.success) { showToast(r.error || 'Sync failed'); return; }
  await vaultRefreshItems();
  renderVaultTab();
  showToast('Synced');
}

async function doLock() {
  clearTimeout(vaultState.revealTimer);
  await window.electronAPI.vault.lock();
  vaultState.items = []; vaultState.selectedId = null;
  await vaultRefreshStatus();
  renderVaultTab();
}

// ── Boot ────────────────────────────────────────────────────────────────────

function initVaultFeature() {
  try {
    window.electronAPI.vault.onInstallProgress(p => {
      vaultState.installPct = p.percent || 0;
      vaultState.installPhase = p.phase || '';
      if (vaultState.installing) renderVaultTab();
    });
    window.electronAPI.vault.onLocked(reason => {
      vaultState.items = []; vaultState.selectedId = null;
      clearTimeout(vaultState.revealTimer);
      vaultRefreshStatus().then(() => {
        if (getActiveTab()?.type === 'vault') renderVaultTab();
      });
      if (reason === 'auto') showToast('Vault auto-locked');
      if (reason === 'system') showToast('Vault locked — system was locked or slept');
    });
    // The one place a secret reaches this process: a single value, pushed by
    // main because the user clicked Insert.
    window.electronAPI.vault.onInsertText(text => {
      if (typeof text !== 'string' || !text) return;
      const tab = tabs.find(t => t.type === 'editor' && t.id !== vaultState.tabId);
      if (!tab) { showToast('Open a text file to insert into'); return; }
      activateTab(tab.id);
      setTimeout(() => {
        try {
          editor.executeEdits('vault-insert', [{ range: editor.getSelection(), text, forceMoveMarkers: true }]);
          editor.focus();
          showToast('Inserted at cursor');
        } catch { showToast('Could not insert'); }
      }, 60);
    });
  } catch (err) {
    console.warn('[vault] init failed', err);
  }
}
