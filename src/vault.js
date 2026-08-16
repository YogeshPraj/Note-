'use strict';
// =============================================================================
// vault.js — UI for the isolated vault window.
//
// This is the only renderer that ever holds a plaintext secret, and it does so
// narrowly: one field, fetched on an explicit click, re-masked on a timer. It
// never sees the vault as a whole — `list()` returns summaries with the secret
// fields stripped out in the main process.
// =============================================================================

(function () {
  const $ = id => document.getElementById(id);
  const api = window.vaultAPI;

  const state = {
    entries: [],
    selectedId: null,
    source: 'local',        // 'local' | 'cli'
    search: '',
    editing: false,
    revealed: {},           // field → { value, timer }
    cliProviders: [],
    cliProvider: null,
    cliItems: [],
    totpTimer: null,
  };

  const REVEAL_MS = 20000;  // how long a revealed secret stays on screen

  // ── Chrome ────────────────────────────────────────────────────────────────

  function show(screenId) {
    document.querySelectorAll('.screen').forEach(s => s.classList.remove('active'));
    $('main').classList.remove('active');
    if (screenId === 'main') $('main').classList.add('active');
    else $(screenId).classList.add('active');
    const unlocked = screenId === 'main' || screenId === 'screen-settings';
    $('btn-lock').style.display = unlocked ? '' : 'none';
    $('btn-settings').style.display = unlocked ? '' : 'none';
  }

  function toast(msg, ms) {
    const t = $('toast');
    t.textContent = msg;
    t.classList.add('show');
    clearTimeout(toast._t);
    toast._t = setTimeout(() => t.classList.remove('show'), ms || 2200);
  }

  function esc(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g,
      c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  function setLockState(txt) { $('lock-state').textContent = txt || ''; }

  // ── Boot / routing ────────────────────────────────────────────────────────

  async function route() {
    const st = await api.status();
    if (!st.exists)      { show('screen-create'); $('create-pw').focus(); return; }
    if (st.locked)       { show('screen-unlock'); $('unlock-pw').focus(); return; }
    await enterMain(st);
  }

  async function enterMain(st) {
    show('main');
    setLockState(st && st.autoLockMinutes ? `auto-locks in ${st.autoLockMinutes}m` : '');
    await refreshList();
  }

  function clearSecretsFromMemory() {
    for (const k of Object.keys(state.revealed)) {
      clearTimeout(state.revealed[k].timer);
      delete state.revealed[k];
    }
    if (state.totpTimer) { clearInterval(state.totpTimer); state.totpTimer = null; }
  }

  // ── Create ────────────────────────────────────────────────────────────────

  $('btn-create').addEventListener('click', async () => {
    const pw = $('create-pw').value, pw2 = $('create-pw2').value;
    $('create-err').textContent = '';
    if (pw.length < 8) { $('create-err').textContent = 'Master password must be at least 8 characters.'; return; }
    if (pw !== pw2)    { $('create-err').textContent = 'Passwords do not match.'; return; }
    const r = await api.create(pw);
    if (!r.success) { $('create-err').textContent = r.error || 'Could not create vault'; return; }
    $('create-pw').value = $('create-pw2').value = '';
    $('recovery-key').textContent = r.recoveryKey;
    show('screen-recovery');
  });
  [$('create-pw'), $('create-pw2')].forEach(el =>
    el.addEventListener('keydown', e => { if (e.key === 'Enter') $('btn-create').click(); }));

  $('ack-recovery').addEventListener('change', e => { $('btn-recovery-done').disabled = !e.target.checked; });
  $('btn-copy-recovery').addEventListener('click', async () => {
    await api.copy('__recovery__', $('recovery-key').textContent);
    toast('Recovery key copied — clipboard clears in 30s');
  });
  $('btn-save-recovery').addEventListener('click', async () => {
    const r = await api.copy('__recovery-save__', $('recovery-key').textContent);
    toast(r && r.savedTo ? 'Saved' : 'Save cancelled');
  });
  $('btn-recovery-done').addEventListener('click', async () => {
    $('recovery-key').textContent = '';   // don't leave it in the DOM
    await enterMain(await api.status());
  });

  // ── Unlock ────────────────────────────────────────────────────────────────

  let usingRecovery = false;
  $('btn-use-recovery').addEventListener('click', () => {
    usingRecovery = !usingRecovery;
    $('unlock-label').textContent = usingRecovery ? 'Recovery key' : 'Master password';
    $('unlock-sub').textContent = usingRecovery
      ? 'Paste the recovery key you saved when the vault was created.'
      : 'Enter your vault master password.';
    $('unlock-pw').type = usingRecovery ? 'text' : 'password';
    $('unlock-pw').value = '';
    $('btn-use-recovery').textContent = usingRecovery ? 'Use master password instead' : 'Use recovery key instead';
    $('unlock-pw').focus();
  });

  $('btn-unlock').addEventListener('click', async () => {
    const val = $('unlock-pw').value;
    $('unlock-err').textContent = '';
    if (!val) return;
    $('btn-unlock').disabled = true;
    const r = usingRecovery ? await api.unlockRecovery(val) : await api.unlock(val);
    $('btn-unlock').disabled = false;
    if (!r.success) { $('unlock-err').textContent = r.error || 'Could not unlock'; return; }
    $('unlock-pw').value = '';
    await enterMain(await api.status());
  });
  $('unlock-pw').addEventListener('keydown', e => { if (e.key === 'Enter') $('btn-unlock').click(); });

  // ── Lock / settings / close ───────────────────────────────────────────────

  $('btn-lock').addEventListener('click', async () => {
    clearSecretsFromMemory();
    await api.lock();
    state.entries = []; state.selectedId = null;
    show('screen-unlock'); $('unlock-pw').focus();
  });
  $('btn-close').addEventListener('click', () => api.close());

  $('btn-settings').addEventListener('click', async () => {
    const st = await api.status();
    $('set-autolock').value = String(st.autoLockMinutes != null ? st.autoLockMinutes : 5);
    $('settings-err').textContent = '';
    show('screen-settings');
  });
  $('btn-settings-back').addEventListener('click', async () => { await enterMain(await api.status()); });
  $('set-autolock').addEventListener('change', async e => {
    await api.setAutoLock(parseInt(e.target.value, 10));
    toast('Auto-lock updated');
  });
  $('btn-change-pw').addEventListener('click', async () => {
    const oldPw = $('cp-old').value, newPw = $('cp-new').value;
    $('settings-err').textContent = '';
    const r = await api.changePassword(oldPw, newPw);
    if (!r.success) { $('settings-err').textContent = r.error || 'Could not change password'; return; }
    $('cp-old').value = $('cp-new').value = '';
    toast('Master password changed');
  });

  api.onLocked(reason => {
    clearSecretsFromMemory();
    state.entries = []; state.selectedId = null;
    show('screen-unlock');
    setLockState('');
    if (reason === 'auto') toast('Vault auto-locked', 3000);
  });
  api.onThemeChanged(dark => document.body.classList.toggle('dark', !!dark));

  // ── Source tabs ───────────────────────────────────────────────────────────

  document.querySelectorAll('.source-tab').forEach(btn => {
    btn.addEventListener('click', async () => {
      document.querySelectorAll('.source-tab').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      state.source = btn.dataset.source;
      state.selectedId = null;
      $('btn-new').style.display = state.source === 'local' ? '' : 'none';
      renderDetail();
      if (state.source === 'cli') await refreshCli();
      else await refreshList();
    });
  });

  $('search').addEventListener('input', e => {
    state.search = e.target.value;
    if (state.source === 'local') renderList();
    else renderCliList();
  });

  // ── Local entries ─────────────────────────────────────────────────────────

  async function refreshList() {
    const r = await api.list();
    if (!r.success) {
      if (/locked/i.test(r.error || '')) { show('screen-unlock'); return; }
      toast(r.error || 'Could not list entries'); return;
    }
    state.entries = r.entries || [];
    renderList();
  }

  function filtered() {
    const q = state.search.trim().toLowerCase();
    if (!q) return state.entries;
    return state.entries.filter(e =>
      (e.title + ' ' + e.username + ' ' + e.url + ' ' + (e.tags || []).join(' ')).toLowerCase().includes(q));
  }

  function renderList() {
    const list = $('entry-list');
    const items = filtered();
    if (!items.length) {
      list.innerHTML = `<div class="empty">${state.entries.length
        ? 'Nothing matches that search.'
        : 'No entries yet.<br><br>Click <b>+</b> to add an API key, connection string, or password.'}</div>`;
      return;
    }
    list.innerHTML = items.map(e => `
      <div class="entry${e.id === state.selectedId ? ' selected' : ''}" data-id="${esc(e.id)}">
        <div class="entry-title">${esc(e.title)}</div>
        <div class="entry-sub">
          ${e.username ? `<span>${esc(e.username)}</span>` : ''}
          ${e.hasTotp ? '<span class="badge">TOTP</span>' : ''}
          ${(e.tags || []).slice(0, 2).map(t => `<span class="badge">${esc(t)}</span>`).join('')}
        </div>
      </div>`).join('');
    list.querySelectorAll('.entry').forEach(el =>
      el.addEventListener('click', () => selectEntry(el.dataset.id)));
  }

  function selectEntry(id) {
    clearSecretsFromMemory();
    state.selectedId = id;
    state.editing = false;
    renderList();
    renderDetail();
  }

  // ── Detail ────────────────────────────────────────────────────────────────

  function renderDetail() {
    const d = $('detail');
    if (state.source === 'cli') return renderCliDetail();
    const e = state.entries.find(x => x.id === state.selectedId);
    if (!e) { d.innerHTML = '<div class="empty">Select an entry</div>'; return; }
    if (state.editing) return renderEditForm(e);

    d.innerHTML = `
      <h2>${esc(e.title)}</h2>
      ${e.username ? fieldRow('Username', 'username', esc(e.username), false) : ''}
      ${e.hasPassword ? fieldRow('Password', 'password', '••••••••••••', true) : ''}
      ${e.url ? `<div class="row"><div class="lbl">URL</div><div class="val">${esc(e.url)}</div></div>` : ''}
      ${e.hasNotes ? fieldRow('Notes', 'notes', '••••••••', true) : ''}
      ${e.hasTotp ? `
        <div id="totp-box">
          <svg id="totp-ring" viewBox="0 0 36 36">
            <circle cx="18" cy="18" r="15" fill="none" stroke="var(--border)" stroke-width="4"/>
            <circle id="totp-arc" cx="18" cy="18" r="15" fill="none" stroke="var(--accent)"
                    stroke-width="4" stroke-linecap="round" transform="rotate(-90 18 18)"/>
          </svg>
          <div id="totp-code">------</div>
          <button class="mini" data-act="copy-totp">Copy</button>
          <button class="mini" data-act="insert-totp">Insert</button>
        </div>` : ''}
      ${(e.tags || []).length ? `<div class="row"><div class="lbl">Tags</div><div class="val">${e.tags.map(esc).join(', ')}</div></div>` : ''}
      <div class="detail-actions">
        <button class="mini" data-act="edit">Edit</button>
        <button class="mini" data-act="delete" style="color:var(--danger)">Delete</button>
      </div>`;

    d.querySelectorAll('[data-act]').forEach(b => b.addEventListener('click', () => onDetailAction(b.dataset.act, b.dataset.field)));
    if (e.hasTotp) startTotp(e.id);
  }

  // A field row: masked by default, with Reveal / Copy / Insert.
  function fieldRow(label, field, display, secret) {
    return `
      <div class="row">
        <div class="lbl">${label}</div>
        <div class="val${secret ? ' masked' : ''}" data-val="${field}">${display}</div>
        ${secret ? `<button class="mini" data-act="reveal" data-field="${field}">Reveal</button>` : ''}
        <button class="mini" data-act="copy" data-field="${field}">Copy</button>
        <button class="mini primary" data-act="insert" data-field="${field}">Insert</button>
      </div>`;
  }

  async function onDetailAction(act, field) {
    const id = state.selectedId;
    if (act === 'edit')   { state.editing = true; renderDetail(); return; }
    if (act === 'delete') {
      const e = state.entries.find(x => x.id === id);
      if (!confirm(`Delete "${e.title}"? This cannot be undone.`)) return;
      const r = await api.remove(id);
      if (r.success) { state.selectedId = null; await refreshList(); renderDetail(); toast('Deleted'); }
      return;
    }
    if (act === 'reveal') {
      const cell = document.querySelector(`[data-val="${field}"]`);
      if (state.revealed[field]) {          // toggle back to masked
        clearTimeout(state.revealed[field].timer);
        delete state.revealed[field];
        cell.textContent = field === 'password' ? '••••••••••••' : '••••••••';
        cell.classList.add('masked');
        return;
      }
      const r = await api.reveal(id, field);
      if (!r.success) { toast(r.error || 'Could not reveal'); return; }
      cell.textContent = r.value;
      cell.classList.remove('masked');
      state.revealed[field] = {
        timer: setTimeout(() => {
          delete state.revealed[field];
          const c = document.querySelector(`[data-val="${field}"]`);
          if (c) { c.textContent = field === 'password' ? '••••••••••••' : '••••••••'; c.classList.add('masked'); }
        }, REVEAL_MS),
      };
      return;
    }
    if (act === 'copy' || act === 'copy-totp') {
      const r = await api.copy(id, act === 'copy-totp' ? 'totp' : field);
      if (!r.success) { toast(r.error || 'Copy failed'); return; }
      toast(r.warning ? r.warning : `Copied — clipboard clears in ${Math.round((r.clearMs || 30000) / 1000)}s`, 4200);
      return;
    }
    if (act === 'insert' || act === 'insert-totp') {
      const r = await api.insertAtCursor(id, act === 'insert-totp' ? 'totp' : field);
      toast(r.success ? 'Inserted at cursor' : (r.error || 'Could not insert'));
      return;
    }
  }

  // ── TOTP ticker ───────────────────────────────────────────────────────────

  function startTotp(id) {
    const tick = async () => {
      const r = await api.totp(id);
      const codeEl = $('totp-code'), arc = $('totp-arc');
      if (!codeEl) { clearInterval(state.totpTimer); state.totpTimer = null; return; }
      if (!r || !r.success || !r.totp) { codeEl.textContent = 'invalid'; return; }
      const { code, secondsLeft, period } = r.totp;
      codeEl.textContent = code.replace(/(\d{3})(?=\d)/, '$1 ');
      const C = 2 * Math.PI * 15;
      arc.setAttribute('stroke-dasharray', String(C));
      arc.setAttribute('stroke-dashoffset', String(C * (1 - secondsLeft / period)));
      arc.setAttribute('stroke', secondsLeft <= 5 ? 'var(--danger)' : 'var(--accent)');
    };
    tick();
    if (state.totpTimer) clearInterval(state.totpTimer);
    state.totpTimer = setInterval(tick, 1000);
  }

  // ── Edit form ─────────────────────────────────────────────────────────────

  $('btn-new').addEventListener('click', () => {
    state.selectedId = null;
    state.editing = true;
    renderList();
    renderEditForm(null);
  });

  function renderEditForm(e) {
    const isNew = !e;
    $('detail').innerHTML = `
      <h2>${isNew ? 'New entry' : 'Edit entry'}</h2>
      <label class="field"><span>Title *</span><input type="text" id="f-title" value="${esc(e?.title || '')}"></label>
      <label class="field"><span>Username</span><input type="text" id="f-username" value="${esc(e?.username || '')}"></label>
      <label class="field"><span>Password ${isNew ? '' : '(leave blank to keep current)'}</span>
        <input type="text" id="f-password" autocomplete="off"></label>
      <div class="gen-row">
        <button class="mini" id="btn-gen">Generate</button>
        <label>len <input type="number" id="gen-len" value="20" min="8" max="128" style="width:52px;padding:3px 5px"></label>
        <label><input type="checkbox" id="gen-sym" checked> symbols</label>
      </div>
      <label class="field" style="margin-top:12px"><span>URL</span><input type="text" id="f-url" value="${esc(e?.url || '')}"></label>
      <label class="field"><span>TOTP secret or otpauth:// URI ${isNew ? '' : '(blank keeps current)'}</span>
        <input type="text" id="f-totp" autocomplete="off"></label>
      <label class="field"><span>Tags (comma separated)</span><input type="text" id="f-tags" value="${esc((e?.tags || []).join(', '))}"></label>
      <label class="field"><span>Notes</span><textarea id="f-notes" rows="4"></textarea></label>
      <div class="detail-actions">
        <button class="mini primary" id="btn-save">Save</button>
        <button class="mini" id="btn-cancel">Cancel</button>
      </div>
      <div class="err" id="edit-err"></div>`;

    // Notes may hold sensitive text, so load them only when editing.
    if (e && e.hasNotes) {
      api.reveal(e.id, 'notes').then(r => { if (r.success) $('f-notes').value = r.value; });
    }

    $('btn-gen').addEventListener('click', async () => {
      const r = await api.generate({
        length: parseInt($('gen-len').value, 10) || 20,
        symbols: $('gen-sym').checked,
      });
      if (r.success) { $('f-password').value = r.password; toast('Generated'); }
    });
    $('btn-cancel').addEventListener('click', () => { state.editing = false; renderDetail(); });
    $('btn-save').addEventListener('click', async () => {
      const entry = {
        id: e?.id,
        title: $('f-title').value.trim(),
        username: $('f-username').value,
        password: $('f-password').value,
        url: $('f-url').value,
        totp: $('f-totp').value.trim(),
        notes: $('f-notes').value,
        tags: $('f-tags').value.split(',').map(s => s.trim()).filter(Boolean),
      };
      if (!entry.title) { $('edit-err').textContent = 'Title is required.'; return; }
      const r = await api.upsert(entry);
      if (!r.success) { $('edit-err').textContent = r.error || 'Could not save'; return; }
      state.editing = false;
      state.selectedId = r.entry.id;
      await refreshList();
      renderDetail();
      toast('Saved');
    });
    $('f-title').focus();
  }

  // ── Connected managers (read-only) ────────────────────────────────────────

  async function refreshCli() {
    $('entry-list').innerHTML = '<div class="empty">Looking for installed managers…</div>';
    const r = await api.cliDetect();
    state.cliProviders = (r && r.providers) || [];
    const usable = state.cliProviders.filter(p => p.installed && p.browsable && p.unlocked);
    if (!usable.length) {
      const installed = state.cliProviders.filter(p => p.installed);
      $('entry-list').innerHTML = `<div class="empty">${
        installed.length
          ? installed.map(p => `<b>${esc(p.name)}</b><br><span style="font-size:11px">${esc(p.reason || 'Not available')}</span>`).join('<br><br>')
          : 'No password manager CLI found.<br><br>Install <code>bw</code> (Bitwarden) or <code>op</code> (1Password) and unlock it, then reopen this tab.<br><br>Note++ never stores their credentials — it just reads from an already-unlocked session.'
      }</div>`;
      $('detail').innerHTML = '<div class="empty">Nothing connected</div>';
      return;
    }
    state.cliProvider = usable[0].id;
    const l = await api.cliList(state.cliProvider, '');
    state.cliItems = (l && l.items) || [];
    renderCliList();
  }

  function renderCliList() {
    const q = state.search.trim().toLowerCase();
    const items = q
      ? state.cliItems.filter(i => (i.title + ' ' + i.username).toLowerCase().includes(q))
      : state.cliItems;
    const prov = state.cliProviders.find(p => p.id === state.cliProvider);
    if (!items.length) { $('entry-list').innerHTML = '<div class="empty">No items</div>'; return; }
    $('entry-list').innerHTML =
      `<div style="font-size:10px;color:var(--muted);padding:4px 10px 8px">via ${esc(prov ? prov.name : 'CLI')} — read only</div>` +
      items.map(i => `
        <div class="entry${i.id === state.selectedId ? ' selected' : ''}" data-id="${esc(i.id)}">
          <div class="entry-title">${esc(i.title)}</div>
          <div class="entry-sub">
            ${i.username ? `<span>${esc(i.username)}</span>` : ''}
            ${i.hasTotp ? '<span class="badge">TOTP</span>' : ''}
          </div>
        </div>`).join('');
    $('entry-list').querySelectorAll('.entry').forEach(el =>
      el.addEventListener('click', () => { state.selectedId = el.dataset.id; renderCliList(); renderCliDetail(); }));
  }

  function renderCliDetail() {
    const i = state.cliItems.find(x => x.id === state.selectedId);
    if (!i) { $('detail').innerHTML = '<div class="empty">Select an item</div>'; return; }
    const prov = state.cliProviders.find(p => p.id === state.cliProvider);
    $('detail').innerHTML = `
      <h2>${esc(i.title)}</h2>
      <div class="notice">Read-only, served live by <b>${esc(prov ? prov.name : 'the CLI')}</b>. Note++ stores nothing.</div>
      ${i.username ? `<div class="row"><div class="lbl">Username</div><div class="val">${esc(i.username)}</div>
        <button class="mini" data-cli="copy" data-field="username">Copy</button>
        <button class="mini primary" data-cli="insert" data-field="username">Insert</button></div>` : ''}
      <div class="row"><div class="lbl">Password</div><div class="val masked">••••••••••••</div>
        <button class="mini" data-cli="copy" data-field="password">Copy</button>
        <button class="mini primary" data-cli="insert" data-field="password">Insert</button></div>
      ${i.hasTotp ? `<div class="row"><div class="lbl">TOTP</div><div class="val masked">••••••</div>
        <button class="mini" data-cli="copy" data-field="totp">Copy</button>
        <button class="mini primary" data-cli="insert" data-field="totp">Insert</button></div>` : ''}
      ${i.url ? `<div class="row"><div class="lbl">URL</div><div class="val">${esc(i.url)}</div></div>` : ''}`;

    $('detail').querySelectorAll('[data-cli]').forEach(b => b.addEventListener('click', async () => {
      const fn = b.dataset.cli === 'copy' ? api.cliCopy : api.cliInsert;
      b.disabled = true; b.textContent = '…';
      const r = await fn(state.cliProvider, state.selectedId, b.dataset.field);
      b.disabled = false; b.textContent = b.dataset.cli === 'copy' ? 'Copy' : 'Insert';
      toast(r && r.success
        ? (b.dataset.cli === 'copy' ? 'Copied — clipboard clears shortly' : 'Inserted at cursor')
        : ((r && r.error) || 'Failed'));
    }));
  }

  // ── Go ────────────────────────────────────────────────────────────────────
  document.addEventListener('keydown', e => {
    if (e.key === 'Escape' && $('main').classList.contains('active')) $('btn-lock').click();
  });
  route();
})();
