'use strict';
// =============================================================================
// Note++ — Spell checker (main process)
// =============================================================================
// Wraps `nspell` + the en_US Hunspell dictionary. The dictionary loads on
// first IPC call (~50-100 ms), then every subsequent check is in-memory
// and microsecond-fast. The renderer caches results per-word too, so the
// same misspelling crossing into view repeatedly never causes a re-check.
//
// Why main-process: nspell is a Node CommonJS module pulling `is-buffer`
// and Buffer primitives. The renderer runs with `contextIsolation: true`
// + `nodeIntegration: false`, so we'd have to bundle nspell into a
// browser-friendly IIFE. Easier to just run it on the Node side and
// expose check/suggest via IPC. The wire format is plain strings, no
// binary, no Buffers.
// =============================================================================

const fs = require('fs');
const path = require('path');
const { app } = require('electron');

let _spell = null;
let _loadPromise = null;

// Path where user-added words are persisted across restarts. Lives in
// userData so it survives app upgrades. Only created when the first
// custom word gets added.
function _userWordsPath() {
  try { return path.join(app.getPath('userData'), 'spell-custom-words.json'); }
  catch { return null; }
}

function _readUserWords() {
  const p = _userWordsPath();
  if (!p) return [];
  try {
    if (!fs.existsSync(p)) return [];
    const arr = JSON.parse(fs.readFileSync(p, 'utf-8'));
    return Array.isArray(arr) ? arr.filter(w => typeof w === 'string' && w.length > 0) : [];
  } catch { return []; }
}

function _writeUserWords(words) {
  const p = _userWordsPath();
  if (!p) return;
  try {
    fs.writeFileSync(p, JSON.stringify(Array.from(new Set(words)).sort(), null, 2), 'utf-8');
  } catch (e) { console.warn('[spell] persist user words failed:', e?.message || e); }
}

function _dictPaths() {
  // dictionary-en ships .aff + .dic alongside its loader. Modern versions
  // of the package restrict subpath access via the "exports" field, which
  // makes `require.resolve('dictionary-en/package.json')` throw
  // ERR_PACKAGE_PATH_NOT_EXPORTED. Walk node_modules ourselves to find
  // the package directory in a way that works in dev + packaged builds
  // (electron-builder unpacks our app, but `node_modules` stays a
  // standard tree alongside it).
  const candidates = [
    // 1. Sibling node_modules (dev mode + most packaged layouts)
    path.join(__dirname, '..', 'node_modules', 'dictionary-en'),
    // 2. Walk up: useful inside asar or when this file is bundled deep
    path.join(__dirname, '..', '..', 'node_modules', 'dictionary-en'),
    path.join(__dirname, '..', '..', '..', 'node_modules', 'dictionary-en'),
  ];
  for (const dir of candidates) {
    const aff = path.join(dir, 'index.aff');
    const dic = path.join(dir, 'index.dic');
    if (fs.existsSync(aff) && fs.existsSync(dic)) return { aff, dic };
  }
  throw new Error('dictionary-en not found in any of: ' + candidates.join(', '));
}

async function ensureLoaded() {
  if (_spell) return _spell;
  if (_loadPromise) return _loadPromise;
  _loadPromise = (async () => {
    const nspell = require('nspell');
    const { aff, dic } = _dictPaths();
    const affText = fs.readFileSync(aff, 'utf-8');
    const dicText = fs.readFileSync(dic, 'utf-8');
    _spell = nspell({ aff: affText, dic: dicText });

    // Seed with our curated developer wordlist so common tech vocabulary
    // (autoscale, kubectl, webhook, OAuth, …) isn't flagged out of the
    // box. en_US Hunspell doesn't know any of these.
    try {
      const devWords = require('./spell-dev-words');
      for (const w of devWords) {
        try { _spell.add(w); } catch {}
      }
    } catch (e) { console.warn('[spell] dev wordlist load failed:', e?.message || e); }

    // Then replay any words the user has personally added in previous
    // sessions via the "Add to dictionary" right-click action.
    try {
      for (const w of _readUserWords()) {
        try { _spell.add(w); } catch {}
      }
    } catch (e) { console.warn('[spell] user wordlist load failed:', e?.message || e); }

    return _spell;
  })();
  return _loadPromise;
}

// Batch check — used by the renderer's debounced scan. Returns parallel
// arrays so the renderer can correlate input ↔ output by index.
async function checkWords(words) {
  if (!Array.isArray(words) || words.length === 0) return [];
  const sp = await ensureLoaded();
  return words.map(w => sp.correct(w));
}

async function suggest(word, max = 6) {
  if (!word) return [];
  const sp = await ensureLoaded();
  const w = String(word);

  // Pass 1 — Hunspell's built-in suggester. Fast, high quality for
  // single/double edit-distance typos.
  let out = sp.suggest(w);
  if (out.length > 0) return out.slice(0, Math.max(1, Math.min(50, max)));

  // Pass 2 — fallback variants. Hunspell's edit-distance limit gives
  // up on "key held down" typos like "anonymouuus" (3 u's). We
  // generate plausible 1-edit transformations and re-ask the
  // dictionary. The variants we try:
  //   • collapse runs of 3+ same letter to 2
  //   • collapse runs of 2+ same letter to 1
  //   • delete one character at each position
  //   • swap each adjacent pair
  // For each variant we accept it as-is if the dictionary already has
  // it, otherwise we forward to suggest() which will succeed because
  // the variant is closer to a real word than the original.
  const candidates = new Set();
  const seenVariants = new Set();
  const tryVariant = (v) => {
    if (v === w || !v || seenVariants.has(v)) return;
    seenVariants.add(v);
    if (sp.correct(v)) candidates.add(v);
    for (const s of sp.suggest(v)) candidates.add(s);
  };
  tryVariant(w.replace(/([a-z])\1{2,}/gi, '$1$1')); // 3+ same → 2
  tryVariant(w.replace(/([a-z])\1+/gi, '$1'));       // 2+ same → 1
  for (let i = 0; i < w.length && candidates.size < 12; i++) {
    tryVariant(w.slice(0, i) + w.slice(i + 1));
  }
  for (let i = 0; i < w.length - 1 && candidates.size < 12; i++) {
    tryVariant(w.slice(0, i) + w[i + 1] + w[i] + w.slice(i + 2));
  }
  out = Array.from(candidates);
  return out.slice(0, Math.max(1, Math.min(50, max)));
}

// Add a word to the in-memory dictionary AND persist it to userData so
// it survives the next restart. Previously the addition was lost on
// every relaunch.
function addWord(word) {
  if (!_spell || !word) return false;
  try { _spell.add(word); } catch { return false; }
  try {
    const existing = _readUserWords();
    if (!existing.includes(word)) {
      existing.push(word);
      _writeUserWords(existing);
    }
  } catch (e) { console.warn('[spell] persist add failed:', e?.message || e); }
  return true;
}

module.exports = { ensureLoaded, checkWords, suggest, addWord };
