'use strict';
// =============================================================================
// sticky.js — renderer for a single sticky-note window.
// Talks only to window.stickyAPI (see sticky-preload.js). Autosaves title and
// notes on a short debounce so a crash never loses a jotted thought.
// =============================================================================

(function () {
  const $ = id => document.getElementById(id);
  const titleEl   = $('sticky-title');
  const notesEl   = $('sticky-notes');
  const gripEl    = $('sticky-grip');
  const dueEl     = $('sticky-due');
  const sourceEl  = $('sticky-source');
  const paletteEl = $('sticky-palette');

  let currentTask = null;
  let saveTimer = null;

  function applyPalette(p) {
    if (!p) return;
    document.documentElement.style.setProperty('--sticky-bg', p.bg);
    document.documentElement.style.setProperty('--sticky-fg', p.fg);
    document.documentElement.style.setProperty('--sticky-accent', p.accent);
  }

  function fmtDue(iso) {
    if (!iso) return '';
    const d = new Date(iso);
    if (isNaN(d.getTime())) return '';
    const now = new Date();
    const sameDay = d.toDateString() === now.toDateString();
    const time = (d.getHours() || d.getMinutes())
      ? d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : '';
    if (sameDay) return time ? `Today ${time}` : 'Today';
    const tomorrow = new Date(now); tomorrow.setDate(now.getDate() + 1);
    if (d.toDateString() === tomorrow.toDateString()) return time ? `Tomorrow ${time}` : 'Tomorrow';
    return d.toLocaleDateString([], { month: 'short', day: 'numeric' }) + (time ? ` ${time}` : '');
  }

  function render(task) {
    currentTask = task;
    titleEl.value = task.title || '';
    notesEl.value = task.notes || '';
    titleEl.classList.toggle('done', task.status === 'done');
    autosizeTitle();

    const label = task.due ? fmtDue(task.due) : '';
    dueEl.textContent = label ? '⏰ ' + label : '';
    dueEl.classList.toggle('overdue',
      !!task.due && task.status !== 'done' && new Date(task.due).getTime() < Date.now());

    if (task.source && task.source.kind !== 'manual' && task.source.filePath) {
      const base = task.source.filePath.split(/[\\/]/).pop();
      sourceEl.textContent = `${base}:${task.source.line}`;
      sourceEl.title = task.source.filePath;
      // Derived tasks live in a file — editing the title here would drift
      // from the source, so keep it read-only and let notes be free-form.
      titleEl.readOnly = true;
      titleEl.title = 'This task comes from your code — edit it in the file';
    } else {
      sourceEl.textContent = '';
      titleEl.readOnly = false;
    }
    gripEl.textContent = task.status === 'done' ? 'Done' : 'Sticky';
  }

  // Title grows to fit up to 4 lines, then scrolls.
  function autosizeTitle() {
    titleEl.style.height = 'auto';
    titleEl.style.height = Math.min(titleEl.scrollHeight, 80) + 'px';
  }

  function scheduleSave() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      if (!currentTask) return;
      await window.stickyAPI.update({
        title: titleEl.readOnly ? undefined : titleEl.value,
        notes: notesEl.value,
      });
    }, 500);
  }

  function buildPalette(colors, activeName) {
    paletteEl.innerHTML = '';
    for (const [name, p] of Object.entries(colors)) {
      const b = document.createElement('button');
      b.className = 'swatch' + (name === activeName ? ' active' : '');
      b.style.background = p.bg;
      b.title = name;
      b.addEventListener('click', async () => {
        const r = await window.stickyAPI.setColor(name);
        if (r && r.palette) applyPalette(r.palette);
        paletteEl.querySelectorAll('.swatch').forEach(s => s.classList.remove('active'));
        b.classList.add('active');
      });
      paletteEl.appendChild(b);
    }
  }

  // ── Wire up ──────────────────────────────────────────────────────────────
  titleEl.addEventListener('input', () => { autosizeTitle(); scheduleSave(); });
  notesEl.addEventListener('input', scheduleSave);

  $('btn-palette').addEventListener('click', () => paletteEl.classList.toggle('open'));

  $('btn-done').addEventListener('click', async () => {
    if (!currentTask) return;
    const done = currentTask.status !== 'done';
    await window.stickyAPI.update({ status: done ? 'done' : 'open' });
    currentTask.status = done ? 'done' : 'open';
    titleEl.classList.toggle('done', done);
    gripEl.textContent = done ? 'Done' : 'Sticky';
  });

  $('btn-close').addEventListener('click', async () => {
    // Flush any pending edit before the window goes away.
    clearTimeout(saveTimer);
    if (currentTask) {
      await window.stickyAPI.update({
        title: titleEl.readOnly ? undefined : titleEl.value,
        notes: notesEl.value,
      });
    }
    window.stickyAPI.close();
  });

  // Esc closes, Ctrl+Enter toggles done — matches the main app's feel.
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') { $('btn-close').click(); return; }
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { $('btn-done').click(); }
  });

  window.stickyAPI.onColorChanged((_name, palette) => applyPalette(palette));

  (async function init() {
    const r = await window.stickyAPI.getTask();
    if (!r || !r.success) {
      titleEl.value = '(task no longer exists)';
      titleEl.readOnly = true;
      return;
    }
    applyPalette(r.palette);
    buildPalette(r.colors, r.colorName);
    render(r.task);
    setTimeout(() => notesEl.focus(), 60);
  })();
})();
