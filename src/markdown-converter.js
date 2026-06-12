'use strict';
// =============================================================================
// Note++ — Binary-to-Markdown converter (main process)
// =============================================================================
// Routes a file path to the right converter based on its extension and
// returns a Markdown string. Inspired by Microsoft's `markitdown` project
// but implemented entirely in Node so users don't need Python installed.
//
//   PDF   → pdf-parse  → page-by-page text extraction
//   DOCX  → mammoth    → HTML internally, then `convertToMarkdown` mode
//
// Progress reporting: each converter takes an `onProgress(percent, stage)`
// callback. Percent is 0–100 (or -1 for indeterminate); stage is a short
// label shown in the UI (e.g. "Reading PDF", "Converting page 3 of 12").
// pdf-parse does not expose page-level progress, so PDFs report just the
// two coarse stages. mammoth is a single async call.
//
// Wiring (main.js):
//   ipcMain.handle('convert-to-markdown:start', async (e, srcPath, jobId) => {
//     return await convert(srcPath, jobId, (pct, stage) => {
//       e.sender.send('convert-to-markdown:progress', { jobId, percent: pct, stage });
//     });
//   });
// =============================================================================

const fs = require('fs');
const path = require('path');

// Lazy requires so the renderer's cold-start path doesn't pay the parse
// cost when the user never opens a PDF/DOCX.
let _PDFParse = null;
let _mammoth  = null;
function getPdfParseClass() {
  // pdf-parse v2 exports a `PDFParse` class instead of v1's plain function.
  if (!_PDFParse) _PDFParse = require('pdf-parse').PDFParse;
  return _PDFParse;
}
function getMammoth() {
  if (!_mammoth) _mammoth = require('mammoth');
  return _mammoth;
}

// Public: list of extensions we know how to convert. Renderer uses this
// to decide whether to intercept openFile().
const SUPPORTED_EXTENSIONS = ['pdf', 'docx'];

function extOf(filePath) {
  return path.extname(filePath || '').replace(/^\./, '').toLowerCase();
}

function canConvert(filePath) {
  return SUPPORTED_EXTENSIONS.includes(extOf(filePath));
}

// ── PDF → Markdown ─────────────────────────────────────────────────────────
async function convertPdf(srcPath, onProgress) {
  onProgress(5, 'Reading PDF');
  // pdf-parse v2's `data` option wants a Uint8Array — Buffer is fine in
  // Node because Buffer extends Uint8Array, but the constructor double-
  // wraps in some platforms. Pass the underlying ArrayBufferView
  // explicitly to avoid surprises.
  const buffer = fs.readFileSync(srcPath);

  onProgress(15, 'Parsing PDF structure');
  const PDFParse = getPdfParseClass();
  const parser = new PDFParse({ data: buffer, verbosity: 0 });

  try {
    let info = {};
    try { info = await parser.getInfo(); } catch { /* metadata is best-effort */ }

    // getText() returns { pages: [{ text, ... }], text, total }
    const data = await parser.getText();
    const pages = Array.isArray(data?.pages) ? data.pages : [];
    const meta = info?.info || {};

    const title = path.basename(srcPath);
    let md  = `# ${escapeMd(title)}\n\n`;
    md += `> Converted from PDF (${pages.length} page${pages.length === 1 ? '' : 's'})\n`;
    if (meta.Title)    md += `> · Title: ${escapeMd(meta.Title)}\n`;
    if (meta.Author)   md += `> · Author: ${escapeMd(meta.Author)}\n`;
    if (meta.Subject)  md += `> · Subject: ${escapeMd(meta.Subject)}\n`;
    if (meta.Producer) md += `> · Producer: ${escapeMd(meta.Producer)}\n`;
    md += '\n---\n\n';

    // Per-page output gives natural section breaks AND lets us report
    // fine-grained progress (rather than one 80→100 jump).
    const PROGRESS_BASE = 20;
    const PROGRESS_RANGE = 78;
    pages.forEach((p, i) => {
      const pageText = (p && typeof p.text === 'string') ? p.text : '';
      md += `## Page ${i + 1}\n\n`;
      md += normalisePdfText(pageText) + '\n\n';
      const pct = PROGRESS_BASE + ((i + 1) / pages.length) * PROGRESS_RANGE;
      onProgress(pct, `Building Markdown (page ${i + 1} of ${pages.length})`);
    });

    onProgress(100, 'Done');
    return md;
  } finally {
    // Release internal pdfjs document handles regardless of success or
    // failure — previously this was only called on the happy path, so
    // a parse error mid-document leaked the underlying handle.
    try { await parser.destroy(); } catch {}
  }
}

// PDF text extraction often produces hard line-wraps and column merges.
// Collapse runs of 2+ newlines into paragraph breaks and trim trailing
// whitespace. Keep it conservative — we don't want to mangle code samples.
function normalisePdfText(t) {
  return t
    .replace(/\r/g, '')
    .replace(/[ \t]+\n/g, '\n')        // strip trailing whitespace on lines
    .replace(/\n{3,}/g, '\n\n')         // collapse multiple blank lines
    .replace(/[ \t]{2,}/g, ' ')         // collapse multiple spaces
    .trim();
}

// ── DOCX → Markdown ────────────────────────────────────────────────────────
async function convertDocx(srcPath, onProgress) {
  onProgress(10, 'Reading DOCX');
  const mammoth = getMammoth();

  onProgress(40, 'Converting to Markdown');
  const result = await mammoth.convertToMarkdown({ path: srcPath });

  onProgress(95, 'Finalising');

  const title = path.basename(srcPath);
  let md = `# ${escapeMd(title)}\n\n`;
  md += `> Converted from DOCX\n\n---\n\n`;
  md += result.value || '';

  if (result.messages && result.messages.length) {
    md += '\n\n---\n\n### Converter notes\n';
    for (const m of result.messages.slice(0, 20)) {
      md += `- _${m.type || 'info'}_: ${m.message}\n`;
    }
  }

  onProgress(100, 'Done');
  return md;
}

// ── Dispatcher ──────────────────────────────────────────────────────────────
async function convert(srcPath, onProgress) {
  const ext = extOf(srcPath);
  onProgress = onProgress || (() => {});
  if (ext === 'pdf')  return convertPdf(srcPath, onProgress);
  if (ext === 'docx') return convertDocx(srcPath, onProgress);
  throw new Error('Unsupported file type: .' + ext);
}

function escapeMd(s) {
  return String(s || '').replace(/([\\*_`~\[\]<>])/g, '\\$1');
}

module.exports = {
  convert,
  canConvert,
  SUPPORTED_EXTENSIONS,
};
