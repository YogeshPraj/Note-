'use strict';
// =============================================================================
// tasks-panel.js — Tasks & Schedule UI (renderer)
// -----------------------------------------------------------------------------
// One feature, two presentations, one state object:
//
//   • DOCKED  (~300 px right rail) — compact agenda. "What's next" while you
//                                    code. Grouped Overdue / Today / …
//   • TAB     (full width)         — planning mode. List + month calendar
//                                    side by side with a detail pane.
//
// Docking MOVES the view rather than cloning it, so there is only ever one
// mount point and no dual-render state to keep in sync. `viewMode` is the
// single source of truth and is persisted to settings.json.
//
// Loaded as a classic script after renderer.js — shares the global scope,
// same pattern as whiteboard-helpers.js / drawio-helpers.js.
// =============================================================================

const taskState = {
  tasks: [],
  viewMode: 'hidden',        // 'hidden' | 'docked' | 'tab'
  view: 'list',              // within the tab: 'list' | 'calendar'
  tabId: null,               // id of the tasks tab when viewMode === 'tab'
  selectedId: null,
  calendarAnchor: new Date(),// which month the calendar is showing
  dayFilter: null,           // YYYY-MM-DD when a calendar day is clicked
  filter: { text: '', showDone: false, source: 'all' }, // source: all|manual|code|markdown
  scanStats: null,
  loading: false,
  quickAdd: false,           // inline "new task" row is showing
  lastRoot: null,            // sticky workspace root — see tsResolveWorkspaceRoot
  _dragTaskId: null,
};

const TASK_PRIORITY_LABEL = ['', 'Low', 'Medium', 'High'];
const TASK_PRIORITY_COLOR = ['transparent', '#5aaeea', '#e3a008', '#e5534b'];

// ── Date helpers ────────────────────────────────────────────────────────────

function tsStartOfDay(d) { const x = new Date(d); x.setHours(0, 0, 0, 0); return x; }
function tsDayKey(d) {
  const x = new Date(d);
  if (isNaN(x.getTime())) return null;
  const p = n => String(n).padStart(2, '0');
  return `${x.getFullYear()}-${p(x.getMonth() + 1)}-${p(x.getDate())}`;
}
function tsFmtDue(iso, opts) {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  const today = tsStartOfDay(new Date());
  const target = tsStartOfDay(d);
  const dayDiff = Math.round((target - today) / 86400000);
  const hasTime = d.getHours() !== 0 || d.getMinutes() !== 0;
  const time = hasTime ? d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : '';
  if (dayDiff === 0)  return time ? `Today ${time}` : 'Today';
  if (dayDiff === 1)  return time ? `Tomorrow ${time}` : 'Tomorrow';
  if (dayDiff === -1) return time ? `Yesterday ${time}` : 'Yesterday';
  if (dayDiff < 0 && dayDiff > -7)  return `${-dayDiff}d overdue`;
  if (dayDiff > 0 && dayDiff < 7)   return d.toLocaleDateString([], { weekday: 'short' }) + (time ? ` ${time}` : '');
  const base = d.toLocaleDateString([], { month: 'short', day: 'numeric' });
  return opts && opts.long && time ? `${base} ${time}` : base;
}
function tsIsOverdue(t) {
  return t.status !== 'done' && t.due && new Date(t.due).getTime() < Date.now();
}

// Bucket tasks into the agenda groups both views share.
function tsGroupTasks(tasks) {
  const now = Date.now();
  const today = tsStartOfDay(new Date()).getTime();
  const tomorrow = today + 86400000;
  const weekEnd = today + 7 * 86400000;
  const groups = {
    overdue:  { label: 'Overdue',   items: [] },
    today:    { label: 'Today',     items: [] },
    tomorrow: { label: 'Tomorrow',  items: [] },
    week:     { label: 'This week', items: [] },
    later:    { label: 'Later',     items: [] },
    someday:  { label: 'No date',   items: [] },
    done:     { label: 'Completed', items: [] },
  };
  for (const t of tasks) {
    if (t.status === 'done') { groups.done.items.push(t); continue; }
    if (!t.due) { groups.someday.items.push(t); continue; }
    const due = new Date(t.due).getTime();
    if (isNaN(due))          groups.someday.items.push(t);
    else if (due < now)      groups.overdue.items.push(t);
    else if (due < tomorrow) groups.today.items.push(t);
    else if (due < tomorrow + 86400000) groups.tomorrow.items.push(t);
    else if (due < weekEnd)  groups.week.items.push(t);
    else                     groups.later.items.push(t);
  }
  // Within a group: priority desc, then soonest due, then title.
  const sorter = (a, b) => (b.priority - a.priority)
    || ((a.due ? new Date(a.due) : Infinity) - (b.due ? new Date(b.due) : Infinity))
    || a.title.localeCompare(b.title);
  for (const g of Object.values(groups)) g.items.sort(sorter);
  return groups;
}

function tsApplyFilter(tasks) {
  const f = taskState.filter;
  const q = (f.text || '').trim().toLowerCase();
  return tasks.filter(t => {
    if (!f.showDone && t.status === 'done') return false;
    if (f.source !== 'all' && (t.source?.kind || 'manual') !== f.source) return false;
    if (taskState.dayFilter) {
      if (!t.due || tsDayKey(t.due) !== taskState.dayFilter) return false;
    }
    if (q) {
      const hay = (t.title + ' ' + (t.tags || []).join(' ') + ' ' + (t.source?.filePath || '')).toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });
}

function tsEsc(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g,
    c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// ── Data plumbing ───────────────────────────────────────────────────────────

// Work out which folder to scan. Deliberately sticky: `activeGitRepo` is
// recomputed per active tab and goes NULL whenever the focused tab has no
// file — including our own Tasks tab. Reading it naively meant that merely
// opening the Tasks tab blanked the scan root, so Scan found nothing. The
// workspace is a property of the session, not of which tab you're looking
// at, so we fall through several signals and remember the last good answer.
function tsResolveWorkspaceRoot() {
  if (typeof fileTreeRootPath !== 'undefined' && fileTreeRootPath) {
    taskState.lastRoot = fileTreeRootPath;
    return fileTreeRootPath;
  }
  if (typeof activeGitRepo !== 'undefined' && activeGitRepo) {
    taskState.lastRoot = activeGitRepo;
    return activeGitRepo;
  }
  // Any open file tab gives us a usable directory.
  const withFile = (typeof tabs !== 'undefined' ? tabs : []).find(t => t.filePath);
  if (withFile) {
    const dir = withFile.filePath.replace(/[\\/][^\\/]*$/, '');
    if (dir) { taskState.lastRoot = dir; return dir; }
  }
  return taskState.lastRoot || null;
}

async function tsRefresh(rescan) {
  taskState.loading = true;
  tsRender();
  try {
    const root = tsResolveWorkspaceRoot();
    await window.electronAPI.tasks.setWorkspace(root);
    const res = rescan
      ? await window.electronAPI.tasks.scan()
      : await window.electronAPI.tasks.list();
    if (res && res.success) {
      taskState.tasks = res.tasks || [];
      if (res.scanStats) taskState.scanStats = res.scanStats;
    }
  } catch (err) {
    console.warn('[tasks] refresh failed', err);
  } finally {
    taskState.loading = false;
    tsRender();
  }
}

async function tsToggleDone(id) {
  const t = taskState.tasks.find(x => x.id === id);
  if (!t) return;
  const res = await window.electronAPI.tasks.toggleDone(id, t.status !== 'done');
  if (res && res.success) { taskState.tasks = res.tasks; tsRender(); }
  else if (res) showToast('Could not update task: ' + (res.error || 'unknown'));
}

async function tsDeleteTask(id) {
  const t = taskState.tasks.find(x => x.id === id);
  if (!t) return;
  if (t.source?.kind !== 'manual') {
    showToast('This task lives in your code — remove the comment to delete it');
    return;
  }
  if (!confirm(`Delete "${t.title}"?`)) return;
  const res = await window.electronAPI.tasks.remove(id);
  if (res && res.success) {
    taskState.tasks = res.tasks;
    if (taskState.selectedId === id) taskState.selectedId = null;
    tsRender();
  }
}

async function tsSetDue(id, dueIso) {
  const res = await window.electronAPI.tasks.setDue(id, dueIso);
  if (res && res.success) { taskState.tasks = res.tasks; tsRender(); }
  else if (res) showToast('Could not set due date: ' + (res.error || 'unknown'));
}

// Jump to the file+line a derived task came from.
async function tsOpenSource(id) {
  const t = taskState.tasks.find(x => x.id === id);
  if (!t || !t.source || !t.source.filePath) return;
  await openFile([t.source.filePath]);
  // openFile is async and may create a tab; give Monaco a tick to mount.
  setTimeout(() => {
    try {
      const line = t.source.line || 1;
      editor.revealLineInCenter(line);
      editor.setPosition({ lineNumber: line, column: 1 });
      editor.focus();
    } catch {}
  }, 220);
}

// ── View-mode transitions ───────────────────────────────────────────────────

function tsSetViewMode(mode) {
  const prev = taskState.viewMode;
  if (prev === mode) return;
  taskState.viewMode = mode;

  const panel  = document.getElementById('tasks-panel');
  const resize = document.getElementById('tasks-panel-resize');

  if (mode === 'docked') {
    // Right rail is single-occupancy — Icons and Tasks can't both be there.
    const icons = document.getElementById('icons-panel');
    if (icons && !icons.classList.contains('hidden')) {
      icons.classList.add('hidden');
      document.getElementById('icons-panel-resize')?.classList.add('hidden');
    }
    panel?.classList.remove('hidden');
    resize?.classList.remove('hidden');
    if (prev === 'tab') tsCloseTasksTab();
  } else {
    panel?.classList.add('hidden');
    resize?.classList.add('hidden');
  }

  if (mode === 'tab') tsOpenTasksTab();
  if (mode === 'hidden' && prev === 'tab') tsCloseTasksTab();

  saveSetting('tasks.viewMode', mode);
  tsUpdateToolbarButton();
  tsRender();
}

// Toolbar button cycles hidden ⇄ docked. The tab is reached from the panel's
// "expand" button or the Tools menu, so a stray click never yanks you into a
// full-screen view you didn't ask for.
function tsToggleTasks() {
  tsSetViewMode(taskState.viewMode === 'hidden' ? 'docked' : 'hidden');
}

function tsOpenTasksTab() {
  // Reuse the existing tab if it's still around.
  if (taskState.tabId != null) {
    const existing = tabs.find(t => t.id === taskState.tabId);
    if (existing) { activateTab(existing.id); return existing; }
  }
  tabCounter++;
  const id = tabCounter;
  const tab = {
    id, name: '✓ Tasks', filePath: null, content: '', dirty: false,
    language: 'plaintext', encoding: 'UTF-8', eol: 'Windows (CR LF)',
    model: null, viewState: null, type: 'tasks',
  };
  taskState.tabId = id;
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}

function tsCloseTasksTab() {
  if (taskState.tabId == null) return;
  const idx = tabs.findIndex(t => t.id === taskState.tabId);
  const id = taskState.tabId;
  taskState.tabId = null;
  if (idx >= 0) {
    tabs.splice(idx, 1);
    if (tabs.length === 0) createTab();
    else if (activeTabId === id) activateTab(tabs[Math.min(idx, tabs.length - 1)].id);
    renderTabs();
  }
  document.getElementById('tasks-tab-container')?.classList.add('hidden');
}

function tsUpdateToolbarButton() {
  const btn = document.getElementById('btn-tasks');
  if (btn) btn.classList.toggle('active', taskState.viewMode !== 'hidden');
}

// ── Rendering ───────────────────────────────────────────────────────────────

function tsRender() {
  if (taskState.viewMode === 'docked') tsRenderPanel();
  else if (taskState.viewMode === 'tab') tsRenderTab();
  tsRenderBadge();
}

function tsRenderBadge() {
  const el = document.getElementById('status-tasks');
  if (!el) return;
  const overdue = taskState.tasks.filter(tsIsOverdue).length;
  const todayKey = tsDayKey(new Date());
  const today = taskState.tasks.filter(t =>
    t.status !== 'done' && t.due && tsDayKey(t.due) === todayKey && !tsIsOverdue(t)).length;
  if (!overdue && !today) { el.classList.add('hidden'); return; }
  el.classList.remove('hidden');
  el.textContent = overdue ? `⏰ ${overdue} overdue` : `✓ ${today} today`;
  el.classList.toggle('tasks-overdue', overdue > 0);
  el.title = `${overdue} overdue · ${today} due today — click to open Tasks`;
}

// One row, shared by panel and tab (tab adds the meta line via `wide`).
function tsRowHtml(t, wide) {
  const overdue = tsIsOverdue(t);
  const done = t.status === 'done';
  const dueLabel = tsFmtDue(t.due, { long: wide });
  const srcBase = t.source?.filePath ? t.source.filePath.split(/[\\/]/).pop() : '';
  const kindIcon = t.source?.kind === 'code' ? '⌗'
    : t.source?.kind === 'markdown' ? '☑' : '•';
  const tags = (t.tags || []).slice(0, wide ? 4 : 2);
  return `
    <div class="task-row${done ? ' done' : ''}${taskState.selectedId === t.id ? ' selected' : ''}"
         data-task-id="${tsEsc(t.id)}" draggable="true">
      <button class="task-check${done ? ' checked' : ''}" data-act="toggle"
              title="${done ? 'Mark not done' : 'Mark done'}">${done ? '✓' : ''}</button>
      <span class="task-prio" style="background:${TASK_PRIORITY_COLOR[t.priority] || 'transparent'}"
            title="${TASK_PRIORITY_LABEL[t.priority] || 'No priority'}"></span>
      <div class="task-main">
        <div class="task-title">${tsEsc(t.title)}</div>
        ${wide || dueLabel || srcBase ? `
        <div class="task-meta">
          ${dueLabel ? `<span class="task-due${overdue ? ' overdue' : ''}">⏰ ${tsEsc(dueLabel)}</span>` : ''}
          ${srcBase ? `<span class="task-src" data-act="source" title="${tsEsc(t.source.filePath)}:${t.source.line}">${kindIcon} ${tsEsc(srcBase)}:${t.source.line}</span>` : ''}
          ${tags.map(tag => `<span class="task-tag">@${tsEsc(tag)}</span>`).join('')}
        </div>` : ''}
      </div>
      <div class="task-actions">
        <button class="task-icon-btn" data-act="sticky" title="Pop out as sticky note">📌</button>
        ${wide ? `<button class="task-icon-btn" data-act="delete" title="Delete">🗑</button>` : ''}
      </div>
    </div>`;
}

function tsGroupsHtml(tasks, wide) {
  const groups = tsGroupTasks(tasks);
  const order = ['overdue', 'today', 'tomorrow', 'week', 'later', 'someday'];
  if (taskState.filter.showDone) order.push('done');
  let html = '';
  let any = false;
  for (const key of order) {
    const g = groups[key];
    if (!g.items.length) continue;
    any = true;
    html += `
      <div class="task-group" data-group="${key}">
        <div class="task-group-header">
          <span class="task-group-label${key === 'overdue' ? ' overdue' : ''}">${g.label}</span>
          <span class="task-group-count">${g.items.length}</span>
        </div>
        ${g.items.map(t => tsRowHtml(t, wide)).join('')}
      </div>`;
  }
  if (!any) {
    html = `<div class="task-empty">
      ${taskState.loading ? 'Loading…' : (taskState.filter.text || taskState.dayFilter
        ? 'No tasks match this filter.'
        // Slashes are / escapes so this line isn't picked up as a task
        // when Note++ scans its own source. (An HTML entity doesn't work —
        // the trailing ";" is itself one of the comment markers we match.)
        : 'No tasks yet.<br><br>Add one with <b>+ New</b>, or write a <code>\u002F\u002F TODO:</code> in your code and hit <b>Scan</b>.')}
    </div>`;
  }
  return html;
}

function tsRenderPanel() {
  const body = document.getElementById('tasks-panel-body');
  if (!body) return;
  const filtered = tsApplyFilter(taskState.tasks);
  body.innerHTML = (taskState.quickAdd ? tsQuickAddHtml() : '') + tsGroupsHtml(filtered, false);
  const countEl = document.getElementById('tasks-panel-count');
  if (countEl) {
    const open = taskState.tasks.filter(t => t.status !== 'done').length;
    countEl.textContent = `${filtered.length}/${open}`;
  }
  tsWireRows(body);
  if (taskState.quickAdd) tsWireQuickAdd(body);
}

function tsRenderTab() {
  const host = document.getElementById('tasks-tab-container');
  if (!host) return;
  const filtered = tsApplyFilter(taskState.tasks);
  const isCal = taskState.view === 'calendar';

  host.innerHTML = `
    <div id="tasks-tab-toolbar">
      <div class="tt-seg">
        <button class="tt-seg-btn${!isCal ? ' active' : ''}" data-view="list">☰ List</button>
        <button class="tt-seg-btn${isCal ? ' active' : ''}" data-view="calendar">▦ Calendar</button>
      </div>
      <input type="text" id="tt-search" placeholder="Filter tasks…" value="${tsEsc(taskState.filter.text)}">
      <select id="tt-source">
        <option value="all"${taskState.filter.source === 'all' ? ' selected' : ''}>All sources</option>
        <option value="manual"${taskState.filter.source === 'manual' ? ' selected' : ''}>Manual only</option>
        <option value="code"${taskState.filter.source === 'code' ? ' selected' : ''}>Code TODOs</option>
        <option value="markdown"${taskState.filter.source === 'markdown' ? ' selected' : ''}>Markdown</option>
      </select>
      <label class="tt-check"><input type="checkbox" id="tt-showdone"${taskState.filter.showDone ? ' checked' : ''}> Show done</label>
      <div style="flex:1"></div>
      ${taskState.dayFilter ? `<button class="tt-btn" id="tt-clear-day">✕ ${tsEsc(taskState.dayFilter)}</button>` : ''}
      <button class="tt-btn" id="tt-new">+ New</button>
      <button class="tt-btn" id="tt-scan" title="Rescan the workspace for TODO / FIXME comments and markdown checkboxes">⟳ Scan</button>
      <button class="tt-btn" id="tt-dock" title="Dock to the right side">⇥ Dock</button>
    </div>
    <div id="tasks-tab-body">
      <div id="tasks-tab-main">${isCal ? tsCalendarHtml(taskState.tasks)
        : (taskState.quickAdd ? tsQuickAddHtml() : '') + tsGroupsHtml(filtered, true)}</div>
      <div id="tasks-tab-detail">${tsDetailHtml()}</div>
    </div>
    <div id="tasks-tab-status">${tsStatusLine()}</div>`;

  // Toolbar wiring
  host.querySelectorAll('.tt-seg-btn').forEach(b =>
    b.addEventListener('click', () => { taskState.view = b.dataset.view; tsRenderTab(); }));
  const search = host.querySelector('#tt-search');
  if (search) {
    let deb;
    search.addEventListener('input', () => {
      clearTimeout(deb);
      deb = setTimeout(() => { taskState.filter.text = search.value; tsRenderTab(); }, 150);
    });
  }
  host.querySelector('#tt-source')?.addEventListener('change', e => {
    taskState.filter.source = e.target.value; tsRenderTab();
  });
  host.querySelector('#tt-showdone')?.addEventListener('change', e => {
    taskState.filter.showDone = e.target.checked; tsRenderTab();
  });
  host.querySelector('#tt-new')?.addEventListener('click', () => tsNewTaskPrompt());
  host.querySelector('#tt-scan')?.addEventListener('click', () => tsRefresh(true));
  host.querySelector('#tt-dock')?.addEventListener('click', () => tsSetViewMode('docked'));
  host.querySelector('#tt-clear-day')?.addEventListener('click', () => {
    taskState.dayFilter = null; tsRenderTab();
  });

  const mainEl = host.querySelector('#tasks-tab-main');
  tsWireRows(mainEl);
  if (isCal) tsWireCalendar(mainEl);
  else if (taskState.quickAdd) tsWireQuickAdd(mainEl);
  tsWireDetail(host.querySelector('#tasks-tab-detail'));
}

function tsStatusLine() {
  const s = taskState.scanStats;
  const open = taskState.tasks.filter(t => t.status !== 'done').length;
  const derived = taskState.tasks.filter(t => t.source?.kind !== 'manual').length;
  let txt = `${open} open · ${derived} from source`;
  if (s) {
    txt += ` · scanned ${s.filesScanned} files in ${s.ms} ms`;
    if (s.truncated) txt += ` (capped at ${s.filesFound})`;
  }
  return tsEsc(txt);
}

// ── Detail pane ─────────────────────────────────────────────────────────────

function tsDetailHtml() {
  const t = taskState.tasks.find(x => x.id === taskState.selectedId);
  if (!t) return `<div class="task-detail-empty">Select a task to see details</div>`;
  const dueVal = t.due ? new Date(t.due) : null;
  const dueInput = dueVal && !isNaN(dueVal)
    ? `${dueVal.getFullYear()}-${String(dueVal.getMonth() + 1).padStart(2, '0')}-${String(dueVal.getDate()).padStart(2, '0')}T${String(dueVal.getHours()).padStart(2, '0')}:${String(dueVal.getMinutes()).padStart(2, '0')}`
    : '';
  const isManual = (t.source?.kind || 'manual') === 'manual';
  return `
    <div class="task-detail">
      <div class="task-detail-title">${tsEsc(t.title)}</div>
      ${!isManual ? `<div class="task-detail-src" data-act="source">
        ${t.source.kind === 'code' ? '⌗ Code TODO' : '☑ Markdown'} —
        ${tsEsc(t.source.filePath.split(/[\\/]/).pop())}:${t.source.line}
        <div class="task-detail-raw">${tsEsc(t.source.raw || '')}</div>
      </div>` : ''}
      <label class="task-detail-field">Due
        <input type="datetime-local" id="td-due" value="${dueInput}">
      </label>
      <label class="task-detail-field">Remind
        <select id="td-remind">
          <option value="">At due time</option>
          <option value="5"${t.remindMinsBefore === 5 ? ' selected' : ''}>5 min before</option>
          <option value="15"${t.remindMinsBefore === 15 ? ' selected' : ''}>15 min before</option>
          <option value="60"${t.remindMinsBefore === 60 ? ' selected' : ''}>1 hour before</option>
          <option value="1440"${t.remindMinsBefore === 1440 ? ' selected' : ''}>1 day before</option>
        </select>
      </label>
      <label class="task-detail-field">Priority
        <select id="td-prio">
          ${[0, 1, 2, 3].map(p => `<option value="${p}"${t.priority === p ? ' selected' : ''}>${TASK_PRIORITY_LABEL[p] || 'None'}</option>`).join('')}
        </select>
      </label>
      <label class="task-detail-field">Notes
        <textarea id="td-notes" rows="5" ${isManual ? '' : ''}>${tsEsc(t.notes)}</textarea>
      </label>
      <div class="task-detail-actions">
        <button class="tt-btn" data-act="sticky">📌 Sticky</button>
        ${isManual ? `<button class="tt-btn" data-act="delete">🗑 Delete</button>` : ''}
      </div>
    </div>`;
}

function tsWireDetail(root) {
  if (!root) return;
  const t = taskState.tasks.find(x => x.id === taskState.selectedId);
  if (!t) return;
  root.querySelector('#td-due')?.addEventListener('change', async (e) => {
    const v = e.target.value;
    await tsSetDue(t.id, v ? new Date(v).toISOString() : null);
  });
  root.querySelector('#td-prio')?.addEventListener('change', async (e) => {
    if ((t.source?.kind || 'manual') !== 'manual') {
      showToast('Priority for code tasks comes from ! marks in the comment');
      return;
    }
    await window.electronAPI.tasks.save({ ...t, priority: parseInt(e.target.value, 10) });
    await tsRefresh(false);
  });
  root.querySelector('#td-remind')?.addEventListener('change', async (e) => {
    const v = e.target.value ? parseInt(e.target.value, 10) : null;
    if ((t.source?.kind || 'manual') === 'manual') {
      await window.electronAPI.tasks.save({ ...t, remindMinsBefore: v });
      await tsRefresh(false);
    } else {
      showToast('Reminder lead time is only editable on manual tasks for now');
    }
  });
  let notesDeb;
  root.querySelector('#td-notes')?.addEventListener('input', (e) => {
    clearTimeout(notesDeb);
    const val = e.target.value;
    notesDeb = setTimeout(async () => {
      if ((t.source?.kind || 'manual') === 'manual') {
        await window.electronAPI.tasks.save({ ...t, notes: val });
      }
    }, 500);
  });
  root.querySelector('[data-act="sticky"]')?.addEventListener('click', () => window.electronAPI.sticky.open(t.id));
  root.querySelector('[data-act="delete"]')?.addEventListener('click', () => tsDeleteTask(t.id));
  root.querySelector('[data-act="source"]')?.addEventListener('click', () => tsOpenSource(t.id));
}

// ── Row wiring (shared) ─────────────────────────────────────────────────────

function tsWireRows(root) {
  if (!root) return;
  root.querySelectorAll('.task-row').forEach(row => {
    const id = row.dataset.taskId;
    row.addEventListener('click', (e) => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'toggle')      { e.stopPropagation(); tsToggleDone(id); return; }
      if (act === 'sticky')      { e.stopPropagation(); window.electronAPI.sticky.open(id); return; }
      if (act === 'delete')      { e.stopPropagation(); tsDeleteTask(id); return; }
      if (act === 'source')      { e.stopPropagation(); tsOpenSource(id); return; }
      taskState.selectedId = id;
      tsRender();
    });
    row.addEventListener('dblclick', () => tsOpenSource(id));
    // Drag onto a calendar day to reschedule.
    row.addEventListener('dragstart', (e) => {
      taskState._dragTaskId = id;
      e.dataTransfer.effectAllowed = 'move';
      try { e.dataTransfer.setData('text/plain', id); } catch {}
    });
    row.addEventListener('dragend', () => { taskState._dragTaskId = null; });
  });
}

// ── Calendar ────────────────────────────────────────────────────────────────

function tsCalendarHtml(tasks) {
  const anchor = taskState.calendarAnchor;
  const year = anchor.getFullYear();
  const month = anchor.getMonth();
  const first = new Date(year, month, 1);
  const startDow = first.getDay();                 // 0 = Sunday
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const todayKey = tsDayKey(new Date());

  // Bucket undone tasks by day for O(1) cell lookup.
  const byDay = new Map();
  for (const t of tasks) {
    if (!t.due) continue;
    if (!taskState.filter.showDone && t.status === 'done') continue;
    const k = tsDayKey(t.due);
    if (!k) continue;
    if (!byDay.has(k)) byDay.set(k, []);
    byDay.get(k).push(t);
  }

  // 6 rows × 7 cols always, so the grid height doesn't jump between months.
  const cells = [];
  const gridStart = new Date(year, month, 1 - startDow);
  for (let i = 0; i < 42; i++) {
    const d = new Date(gridStart);
    d.setDate(gridStart.getDate() + i);
    const key = tsDayKey(d);
    const inMonth = d.getMonth() === month;
    const items = (byDay.get(key) || []).sort((a, b) => b.priority - a.priority);
    const shown = items.slice(0, 3);
    const extra = items.length - shown.length;
    cells.push(`
      <div class="cal-cell${inMonth ? '' : ' out'}${key === todayKey ? ' today' : ''}${taskState.dayFilter === key ? ' selected' : ''}"
           data-day="${key}">
        <div class="cal-daynum">${d.getDate()}</div>
        ${shown.map(t => `
          <div class="cal-chip${tsIsOverdue(t) ? ' overdue' : ''}${t.status === 'done' ? ' done' : ''}"
               data-task-id="${tsEsc(t.id)}" draggable="true" title="${tsEsc(t.title)}">
            <span class="cal-chip-dot" style="background:${TASK_PRIORITY_COLOR[t.priority] || '#888'}"></span>
            ${tsEsc(t.title)}
          </div>`).join('')}
        ${extra > 0 ? `<div class="cal-more">+${extra} more</div>` : ''}
      </div>`);
  }

  const monthLabel = first.toLocaleDateString([], { month: 'long', year: 'numeric' });
  const dows = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  return `
    <div id="cal-wrap">
      <div id="cal-header">
        <button class="tt-btn" data-cal="prev">‹</button>
        <div id="cal-month">${tsEsc(monthLabel)}</div>
        <button class="tt-btn" data-cal="next">›</button>
        <button class="tt-btn" data-cal="today">Today</button>
        <div style="flex:1"></div>
        <span class="cal-hint">Drag a task onto a day to reschedule</span>
      </div>
      <div id="cal-dow">${dows.map(d => `<div>${d}</div>`).join('')}</div>
      <div id="cal-grid">${cells.join('')}</div>
    </div>`;
}

function tsWireCalendar(root) {
  if (!root) return;
  root.querySelectorAll('[data-cal]').forEach(b => b.addEventListener('click', () => {
    const dir = b.dataset.cal;
    const a = new Date(taskState.calendarAnchor);
    if (dir === 'prev')  a.setMonth(a.getMonth() - 1);
    if (dir === 'next')  a.setMonth(a.getMonth() + 1);
    if (dir === 'today') { taskState.calendarAnchor = new Date(); tsRenderTab(); return; }
    taskState.calendarAnchor = a;
    tsRenderTab();
  }));

  root.querySelectorAll('.cal-chip').forEach(chip => {
    chip.addEventListener('click', (e) => {
      e.stopPropagation();
      taskState.selectedId = chip.dataset.taskId;
      tsRenderTab();
    });
    chip.addEventListener('dblclick', (e) => { e.stopPropagation(); tsOpenSource(chip.dataset.taskId); });
    chip.addEventListener('dragstart', (e) => {
      taskState._dragTaskId = chip.dataset.taskId;
      e.dataTransfer.effectAllowed = 'move';
      try { e.dataTransfer.setData('text/plain', chip.dataset.taskId); } catch {}
      e.stopPropagation();
    });
    chip.addEventListener('dragend', () => { taskState._dragTaskId = null; });
  });

  root.querySelectorAll('.cal-cell').forEach(cell => {
    cell.addEventListener('click', () => {
      const k = cell.dataset.day;
      taskState.dayFilter = taskState.dayFilter === k ? null : k;
      taskState.view = 'list';
      tsRenderTab();
    });
    cell.addEventListener('dragover', (e) => {
      if (!taskState._dragTaskId) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      cell.classList.add('drop-target');
    });
    cell.addEventListener('dragleave', () => cell.classList.remove('drop-target'));
    cell.addEventListener('drop', async (e) => {
      e.preventDefault();
      cell.classList.remove('drop-target');
      const id = taskState._dragTaskId || e.dataTransfer.getData('text/plain');
      taskState._dragTaskId = null;
      if (!id) return;
      const t = taskState.tasks.find(x => x.id === id);
      if (!t) return;
      // Preserve the existing time-of-day; only move the date.
      const [Y, M, D] = cell.dataset.day.split('-').map(n => parseInt(n, 10));
      const old = t.due ? new Date(t.due) : null;
      const next = new Date(Y, M - 1, D,
        old && !isNaN(old) ? old.getHours() : 9,
        old && !isNaN(old) ? old.getMinutes() : 0);
      await tsSetDue(id, next.toISOString());
    });
  });
}

// ── New-task flow ───────────────────────────────────────────────────────────
// A single inline text row with the same grammar as the code comments, so
// there's exactly one syntax to learn:
//   Ship the release due:2026-08-20 09:00 !! @release
//
// NOTE: this deliberately does NOT use window.prompt() — Electron does not
// implement it (the call silently no-ops), which is a genuinely easy trap.
// Built per-render so it can show which day a new task will land on when the
// user has a calendar day selected.
function tsQuickAddHtml() {
  const day = taskState.dayFilter;
  const dayLabel = day ? tsFmtDue(new Date(day + 'T09:00').toISOString()) : null;
  return `
  <div class="task-quickadd">
    <input type="text" class="task-quickadd-input"
           placeholder="${day ? 'Task title…  (lands on ' + tsEsc(dayLabel) + ')' : 'Task title…  due:2026-08-20 09:00  !!  @tag'}"
           spellcheck="false">
    <div class="task-quickadd-hint">
      ${day ? `<b>Due ${tsEsc(dayLabel)}</b> — because that day is selected. Type <code>due:</code> to override. · ` : ''}
      <b>Enter</b> to add · <b>Esc</b> to cancel ·
      <code>due:</code> date · <code>!</code>–<code>!!!</code> priority · <code>@</code> tag
    </div>
  </div>`;
}

function tsWireQuickAdd(root) {
  const input = root?.querySelector('.task-quickadd-input');
  if (!input) return;
  input.focus();
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      const raw = input.value.trim();
      if (!raw) { taskState.quickAdd = false; tsRender(); return; }
      input.value = '';
      tsCreateFromText(raw);      // stays open so you can add several in a row
    } else if (e.key === 'Escape') {
      e.preventDefault();
      taskState.quickAdd = false;
      tsRender();
    }
  });
  // Clicking away closes it, but only if nothing was typed.
  input.addEventListener('blur', () => {
    setTimeout(() => {
      if (!input.isConnected) return;
      if (!input.value.trim()) { taskState.quickAdd = false; tsRender(); }
    }, 120);
  });
}

function tsNewTaskPrompt() {
  taskState.quickAdd = true;
  tsRender();
}

async function tsCreateFromText(raw) {
  // Parse client-side using the same grammar the scanner uses. We re-implement
  // the tiny bit we need rather than round-tripping to main for a preview.
  const dueM = /\bdue:\s*(\d{4}-\d{2}-\d{2})(?:[T ](\d{1,2}):(\d{2}))?/i.exec(raw);
  let due = null;
  if (dueM) {
    const [, date, hh, mm] = dueM;
    const [Y, Mo, D] = date.split('-').map(n => parseInt(n, 10));
    const d = new Date(Y, Mo - 1, D, hh !== undefined ? parseInt(hh, 10) : 9,
      mm !== undefined ? parseInt(mm, 10) : 0);
    if (!isNaN(d.getTime())) due = d.toISOString();
  }
  // No explicit due: in the text? If the user has a calendar day selected,
  // that's the day they're looking at — "add a task" here means "add it to
  // this day". An explicit due: in the text still wins.
  if (!due && taskState.dayFilter) {
    const [Y, Mo, D] = taskState.dayFilter.split('-').map(n => parseInt(n, 10));
    const d = new Date(Y, Mo - 1, D, 9, 0);
    if (!isNaN(d.getTime())) due = d.toISOString();
  }

  const bangM = /(?:^|\s)(!{1,3})(?=\s|$)/.exec(raw);
  const priority = bangM ? bangM[1].length : 0;
  const tags = [];
  const tagRe = /(?:^|\s)@([A-Za-z][\w-]{0,31})/g;
  let tm;
  while ((tm = tagRe.exec(raw)) !== null) if (!tags.includes(tm[1])) tags.push(tm[1]);
  const title = raw
    .replace(/\bdue:\s*\d{4}-\d{2}-\d{2}(?:[T ]\d{1,2}:\d{2})?/i, '')
    .replace(/(?:^|\s)(!{1,3})(?=\s|$)/, ' ')
    .replace(/(?:^|\s)@([A-Za-z][\w-]{0,31})/g, ' ')
    .replace(/\s{2,}/g, ' ').trim();
  if (!title) { showToast('Task needs a title'); return; }

  const res = await window.electronAPI.tasks.save({ title, due, priority, tags });
  if (res && res.success) {
    taskState.tasks = res.tasks;
    taskState.selectedId = res.task.id;
    tsRender();
    showToast('Task added');
  } else {
    showToast('Could not add task: ' + (res?.error || 'unknown'));
  }
}

// ── Reminder toast ──────────────────────────────────────────────────────────
// The native OS notification is the primary surface (it works when the window
// is hidden). This in-app banner is the secondary one for when you're already
// looking at the app — it carries the snooze/done actions.
function tsShowReminderBanner(task, meta) {
  const host = document.getElementById('task-reminder-host');
  if (!host) return;
  const el = document.createElement('div');
  el.className = 'task-reminder';
  el.innerHTML = `
    <div class="tr-icon">⏰</div>
    <div class="tr-body">
      <div class="tr-title">${meta && meta.overdue ? 'Overdue' : 'Due now'}</div>
      <div class="tr-task">${tsEsc(task.title)}</div>
      ${task.source?.filePath ? `<div class="tr-src">${tsEsc(task.source.filePath.split(/[\\/]/).pop())}:${task.source.line}</div>` : ''}
    </div>
    <div class="tr-actions">
      <button class="tr-btn" data-a="done">Done</button>
      <button class="tr-btn" data-a="snooze">Snooze 10m</button>
      <button class="tr-btn tr-x" data-a="dismiss">✕</button>
    </div>`;
  host.appendChild(el);

  const close = () => { el.classList.add('leaving'); setTimeout(() => el.remove(), 200); };
  el.querySelector('[data-a="done"]').addEventListener('click', async () => { await tsToggleDone(task.id); close(); });
  el.querySelector('[data-a="snooze"]').addEventListener('click', async () => {
    const r = await window.electronAPI.tasks.snooze(task.id, 10);
    if (r?.success) taskState.tasks = r.tasks;
    tsRender(); close();
  });
  el.querySelector('[data-a="dismiss"]').addEventListener('click', close);
  el.querySelector('.tr-task').addEventListener('click', () => { tsOpenSource(task.id); close(); });

  // Deliberately NO auto-dismiss. A reminder that self-destructs is one you
  // miss the moment you step away from the desk — which is exactly when you
  // needed it. It stays until you hit Done, Snooze, or ✕. To stop a long
  // absence producing an unbounded stack, we cap how many are on screen and
  // drop the oldest.
  const MAX_BANNERS = 4;
  while (host.children.length > MAX_BANNERS) host.firstElementChild.remove();
}

// ── Boot ────────────────────────────────────────────────────────────────────

async function initTasksFeature() {
  // Panel chrome
  document.getElementById('tasks-close-btn')?.addEventListener('click', () => tsSetViewMode('hidden'));
  document.getElementById('tasks-expand-btn')?.addEventListener('click', () => tsSetViewMode('tab'));
  document.getElementById('tasks-scan-btn')?.addEventListener('click', () => tsRefresh(true));
  document.getElementById('tasks-new-btn')?.addEventListener('click', () => tsNewTaskPrompt());
  document.getElementById('status-tasks')?.addEventListener('click', () => {
    if (taskState.viewMode === 'hidden') tsSetViewMode('docked');
    else if (taskState.viewMode === 'docked') tsSetViewMode('tab');
  });

  const search = document.getElementById('tasks-panel-search');
  if (search) {
    let deb;
    search.addEventListener('input', () => {
      clearTimeout(deb);
      deb = setTimeout(() => { taskState.filter.text = search.value; tsRenderPanel(); }, 150);
    });
  }

  // Main → renderer events
  window.electronAPI.tasks.onReminder(({ task, meta }) => tsShowReminderBanner(task, meta));
  window.electronAPI.tasks.onNotifClick((id) => {
    taskState.selectedId = id;
    if (taskState.viewMode === 'hidden') tsSetViewMode('docked');
    else tsRender();
  });
  window.electronAPI.tasks.onChanged(() => tsRefresh(false));
  // A source rewrite (checkbox ticked, due date moved) means the file on disk
  // changed underneath any open tab — let the existing watcher reconcile it.
  window.electronAPI.tasks.onSourceChanged((fp) => {
    const tab = tabs.find(t => t.filePath && t.filePath === fp);
    if (tab && !tab.dirty) {
      window.electronAPI.readFile(fp).then(r => {
        if (r?.success && tab.model && r.content !== tab.model.getValue()) {
          tab.model.setValue(r.content);
          tab.content = r.content;
        }
      }).catch(() => {});
    }
  });

  // Restore persisted view mode + any stickies that were open last session.
  try {
    const s = await window.electronAPI.readSettings();
    const mode = s?.tasks?.viewMode;
    await tsRefresh(false);
    if (mode === 'docked' || mode === 'tab') tsSetViewMode(mode);
    // Kick a scan in the background so the panel is populated by the time
    // the user looks at it, without blocking boot.
    setTimeout(() => tsRefresh(true), 2500);
    setTimeout(() => window.electronAPI.sticky.restoreAll(), 1200);
  } catch (err) {
    console.warn('[tasks] init failed', err);
  }
}
