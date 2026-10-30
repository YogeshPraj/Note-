'use strict';
// =============================================================================
// data-grid.js — table view for CSV / TSV files and JSON arrays
// -----------------------------------------------------------------------------
// Opening a CSV as text is a bad way to read a CSV. This adds a grid view you
// can flip to on the SAME tab (not a separate tab type), so the Monaco model
// stays the single source of truth: cell edits write back through the model,
// which means dirty-marking, Ctrl+Z, Ctrl+S and the external-change watcher all
// keep working with no special-casing.
//
// Sorting and filtering are strictly VIEW operations. The underlying row order
// is never touched, so saving a sorted view doesn't silently reorder the user's
// file — a nasty surprise that's easy to ship by accident.
//
// Loaded as a classic script sharing renderer.js's global scope.
// =============================================================================

const GRID_MAX_ROWS = 5000;      // render cap; larger files show a notice
const GRID_MAX_BYTES = 8 * 1024 * 1024;

const gridState = {
  tabId: null,
  headers: [],
  rows: [],            // always in FILE order
  view: [],            // indices into rows — sort/filter output
  sortCol: -1,
  sortDir: 1,
  filter: '',
  kind: 'csv',         // csv | tsv | json
  delim: ',',
  jsonIndent: 2,
  truncated: false,
};

// ── Parsing ─────────────────────────────────────────────────────────────────

// RFC-4180-ish: handles quoted fields, embedded delimiters/newlines, and ""
// escapes. Written out rather than pulled in — it's 40 lines and a dependency
// in the save path is a dependency that can corrupt someone's file.
function csvParse(text, delim) {
  const rows = [];
  let row = [], field = '', inQuotes = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inQuotes) {
      if (c === '"') {
        if (text[i + 1] === '"') { field += '"'; i++; }
        else inQuotes = false;
      } else field += c;
      continue;
    }
    if (c === '"') { inQuotes = true; continue; }
    if (c === delim) { row.push(field); field = ''; continue; }
    if (c === '\r') continue;
    if (c === '\n') { row.push(field); rows.push(row); row = []; field = ''; continue; }
    field += c;
  }
  if (field.length || row.length) { row.push(field); rows.push(row); }
  // A trailing newline yields one empty row; drop it rather than showing a
  // phantom blank record.
  while (rows.length && rows[rows.length - 1].every(v => v === '')) rows.pop();
  return rows;
}

function csvQuote(v, delim) {
  const s = v == null ? '' : String(v);
  return (s.includes(delim) || s.includes('"') || s.includes('\n') || s.includes('\r'))
    ? '"' + s.replace(/"/g, '""') + '"'
    : s;
}

function csvStringify(headers, rows, delim, eol) {
  const out = [headers.map(h => csvQuote(h, delim)).join(delim)];
  for (const r of rows) out.push(headers.map((_, i) => csvQuote(r[i], delim)).join(delim));
  return out.join(eol || '\n');
}

// Only a top-level array of objects becomes a table. Anything else (nested
// objects, arrays of arrays, a bare object) has no honest tabular shape, and
// guessing one would misrepresent the data.
function jsonToTable(text) {
  let data;
  try { data = JSON.parse(text); } catch { return null; }
  if (!Array.isArray(data) || !data.length) return null;
  if (!data.every(r => r && typeof r === 'object' && !Array.isArray(r))) return null;
  const headers = [];
  for (const rec of data) for (const k of Object.keys(rec)) if (!headers.includes(k)) headers.push(k);
  const rows = data.map(rec => headers.map(h => {
    const v = rec[h];
    if (v == null) return '';
    return typeof v === 'object' ? JSON.stringify(v) : String(v);
  }));
  return { headers, rows };
}

// Round-trip back to JSON. Values that parse as JSON scalars are restored as
// such, so a numeric column doesn't come back as strings.
function tableToJson(headers, rows, indent) {
  const out = rows.map(r => {
    const o = {};
    headers.forEach((h, i) => {
      const raw = r[i] == null ? '' : String(r[i]);
      if (raw === '') { o[h] = ''; return; }
      if (/^-?\d+(\.\d+)?$/.test(raw)) { o[h] = Number(raw); return; }
      if (raw === 'true' || raw === 'false') { o[h] = raw === 'true'; return; }
      if (raw === 'null') { o[h] = null; return; }
      if (/^[[{]/.test(raw)) { try { o[h] = JSON.parse(raw); return; } catch {} }
      o[h] = raw;
    });
    return o;
  });
  return JSON.stringify(out, null, indent);
}

// ── Eligibility ─────────────────────────────────────────────────────────────

function gridDetectKind(tab) {
  const name = (tab.name || '').toLowerCase();
  if (name.endsWith('.csv')) return 'csv';
  if (name.endsWith('.tsv')) return 'tsv';
  if (name.endsWith('.json') || name.endsWith('.jsonc')) return 'json';
  return null;
}

function gridCanShow(tab) {
  if (!tab || tab.type !== 'editor' || !tab.model) return false;
  const kind = gridDetectKind(tab);
  if (!kind) return false;
  if (tab.model.getValueLength() > GRID_MAX_BYTES) return false;
  if (kind === 'json') return jsonToTable(tab.model.getValue()) !== null;
  return true;
}

function gridIsOpen() { return gridState.tabId != null; }

// ── Load / save ─────────────────────────────────────────────────────────────

function gridLoadFromTab(tab) {
  const kind = gridDetectKind(tab);
  const text = tab.model.getValue();
  gridState.kind = kind;
  gridState.truncated = false;

  if (kind === 'json') {
    const t = jsonToTable(text);
    if (!t) return false;
    gridState.headers = t.headers;
    gridState.rows = t.rows;
    const m = text.match(/^\[\s*\n(\s+)/);
    gridState.jsonIndent = m ? m[1].length : 2;
  } else {
    gridState.delim = kind === 'tsv' ? '\t' : ',';
    const all = csvParse(text, gridState.delim);
    if (!all.length) return false;
    gridState.headers = all[0].map((h, i) => h || `col${i + 1}`);
    gridState.rows = all.slice(1);
  }
  if (gridState.rows.length > GRID_MAX_ROWS) gridState.truncated = true;
  gridRebuildView();
  return true;
}

// Push the table back through the Monaco model so the edit joins the normal
// undo stack and dirty/save path instead of bypassing them.
function gridWriteBack() {
  const tab = tabs.find(t => t.id === gridState.tabId);
  if (!tab || !tab.model) return;
  const eol = tab.model.getEOL();
  const text = gridState.kind === 'json'
    ? tableToJson(gridState.headers, gridState.rows, gridState.jsonIndent)
    : csvStringify(gridState.headers, gridState.rows, gridState.delim, eol);
  const full = tab.model.getFullModelRange();
  tab.model.pushEditOperations([], [{ range: full, text }], () => null);
}

// ── View (sort / filter) ────────────────────────────────────────────────────

function gridRebuildView() {
  const f = gridState.filter.trim().toLowerCase();
  let idx = gridState.rows.map((_, i) => i);
  if (f) idx = idx.filter(i => gridState.rows[i].some(v => String(v).toLowerCase().includes(f)));
  if (gridState.sortCol >= 0) {
    const c = gridState.sortCol, dir = gridState.sortDir;
    idx.sort((a, b) => {
      const x = gridState.rows[a][c] ?? '', y = gridState.rows[b][c] ?? '';
      const nx = parseFloat(x), ny = parseFloat(y);
      const bothNum = !isNaN(nx) && !isNaN(ny) && String(nx) === String(x).trim() && String(ny) === String(y).trim();
      if (bothNum) return (nx - ny) * dir;
      return String(x).localeCompare(String(y), undefined, { numeric: true }) * dir;
    });
  }
  gridState.view = idx;
}

// ── Render ──────────────────────────────────────────────────────────────────

function gEsc(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g,
    c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function gridRender() {
  const host = document.getElementById('grid-container');
  if (!host) return;
  const shown = gridState.view.slice(0, GRID_MAX_ROWS);
  const arrow = c => gridState.sortCol !== c ? '' : (gridState.sortDir === 1 ? ' ▲' : ' ▼');

  host.innerHTML = `
    <div id="grid-toolbar">
      <span class="g-kind">${gEsc(gridState.kind.toUpperCase())}</span>
      <input id="g-filter" type="text" placeholder="Filter rows…" value="${gEsc(gridState.filter)}">
      <span class="g-count">${gridState.view.length} of ${gridState.rows.length} rows · ${gridState.headers.length} cols</span>
      <div style="flex:1"></div>
      <button class="g-btn" id="g-addrow">+ Row</button>
      <button class="g-btn" id="g-text">Text view</button>
    </div>
    ${gridState.truncated ? `<div class="g-warn">Showing the first ${GRID_MAX_ROWS.toLocaleString()} rows. Editing still writes the whole file.</div>` : ''}
    <div id="grid-scroll">
      <table id="grid-table">
        <thead><tr><th class="g-rownum"></th>${
          gridState.headers.map((h, i) => `<th data-col="${i}" title="Sort by ${gEsc(h)}">${gEsc(h)}${arrow(i)}</th>`).join('')
        }</tr></thead>
        <tbody>${
          shown.map(ri => `<tr data-row="${ri}">
            <td class="g-rownum">${ri + 2}</td>${
            gridState.headers.map((_, ci) =>
              `<td data-row="${ri}" data-col="${ci}" tabindex="0">${gEsc(gridState.rows[ri][ci])}</td>`).join('')
          }</tr>`).join('')
        }</tbody>
      </table>
    </div>`;

  const filt = document.getElementById('g-filter');
  let deb;
  filt.addEventListener('input', () => {
    clearTimeout(deb);
    deb = setTimeout(() => {
      gridState.filter = filt.value;
      gridRebuildView();
      gridRender();
      const f2 = document.getElementById('g-filter');
      if (f2) { f2.focus(); f2.setSelectionRange(f2.value.length, f2.value.length); }
    }, 180);
  });

  host.querySelectorAll('#grid-table thead th[data-col]').forEach(th =>
    th.addEventListener('click', () => {
      const c = +th.dataset.col;
      if (gridState.sortCol === c) gridState.sortDir *= -1;
      else { gridState.sortCol = c; gridState.sortDir = 1; }
      gridRebuildView();
      gridRender();
    }));

  host.querySelectorAll('#grid-table tbody td[data-col]').forEach(td =>
    td.addEventListener('dblclick', () => gridEditCell(td)));

  document.getElementById('g-text').addEventListener('click', () => closeGridView());
  document.getElementById('g-addrow').addEventListener('click', () => {
    gridState.rows.push(gridState.headers.map(() => ''));
    gridWriteBack();
    gridRebuildView();
    gridRender();
  });
}

function gridEditCell(td) {
  const ri = +td.dataset.row, ci = +td.dataset.col;
  const old = gridState.rows[ri][ci] ?? '';
  const input = document.createElement('input');
  input.className = 'g-edit';
  input.value = old;
  td.textContent = '';
  td.appendChild(input);
  input.focus();
  input.select();

  const commit = (save) => {
    if (save && input.value !== old) {
      gridState.rows[ri][ci] = input.value;
      gridWriteBack();
    }
    td.textContent = gridState.rows[ri][ci] ?? '';
  };
  input.addEventListener('blur', () => commit(true));
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); commit(true); td.focus(); }
    else if (e.key === 'Escape') { e.preventDefault(); commit(false); td.focus(); }
  });
}

// ── Open / close ────────────────────────────────────────────────────────────

function openGridView() {
  const tab = getActiveTab();
  if (!gridCanShow(tab)) {
    const why = !tab || tab.type !== 'editor' ? 'Not a text tab'
      : !gridDetectKind(tab) ? 'Table view works on .csv, .tsv and .json files'
      : tab.model.getValueLength() > GRID_MAX_BYTES ? 'File is too large for table view'
      : 'This JSON is not an array of objects, so it has no table shape';
    showToast(why);
    return;
  }
  if (!gridLoadFromTab(tab)) { showToast('Could not read that file as a table'); return; }
  gridState.tabId = tab.id;
  gridState.filter = ''; gridState.sortCol = -1; gridState.sortDir = 1;
  gridRebuildView();
  document.getElementById('monaco-editor').style.display = 'none';
  document.getElementById('grid-container').classList.remove('hidden');
  gridRender();
  updateGridButton();
}

function closeGridView() {
  gridState.tabId = null;
  const c = document.getElementById('grid-container');
  if (c) { c.classList.add('hidden'); c.innerHTML = ''; }
  const m = document.getElementById('monaco-editor');
  if (m && getActiveTab() && getActiveTab().type === 'editor') m.style.display = '';
  try { if (editor) editor.layout(); } catch {}
  updateGridButton();
}

function toggleGridView() {
  if (gridIsOpen()) closeGridView(); else openGridView();
}

// Called from activateTab — the grid belongs to one tab, so switching away
// must put the editor back rather than leaving a stale table on screen.
function gridOnTabChanged(tab) {
  if (gridIsOpen() && (!tab || tab.id !== gridState.tabId)) closeGridView();
  updateGridButton();
}

function updateGridButton() {
  const btn = document.getElementById('btn-grid');
  if (!btn) return;
  const tab = getActiveTab();
  const eligible = gridCanShow(tab);
  btn.classList.toggle('hidden', !eligible && !gridIsOpen());
  btn.classList.toggle('active', gridIsOpen());
}
