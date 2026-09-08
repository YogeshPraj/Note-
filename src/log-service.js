'use strict';
// =============================================================================
// log-service.js — tail -f for the editor
// -----------------------------------------------------------------------------
// Large-file safe mode (v1.7) stopped big logs from freezing Monaco, but that
// only made them survivable, not useful. This streams a log as it grows.
//
// Reads by OFFSET rather than re-reading the file: on each tick we stat the
// file and, if it grew, read only the new bytes. A 3 GB log costs one stat per
// poll, not a 3 GB read.
//
// Polling rather than fs.watch: appends to a file that is held open by another
// process are exactly the case fs.watch is least reliable about across
// platforms, and a 700 ms stat is cheap. It also makes rotation detection
// trivial — if the file suddenly got smaller, it was rotated or truncated, so
// we start over from the top rather than reading garbage from a stale offset.
// =============================================================================

const fs = require('fs');
const POLL_MS = 700;
const INITIAL_TAIL_BYTES = 256 * 1024;   // context shown when you start following
const MAX_CHUNK_BYTES = 4 * 1024 * 1024; // cap a single catch-up read

const tails = new Map();   // id -> { filePath, offset, timer, partial }

function readRange(filePath, from, to) {
  const len = to - from;
  if (len <= 0) return '';
  const fd = fs.openSync(filePath, 'r');
  try {
    const buf = Buffer.alloc(Math.min(len, MAX_CHUNK_BYTES));
    const read = fs.readSync(fd, buf, 0, buf.length, from);
    return buf.slice(0, read).toString('utf8');
  } finally {
    try { fs.closeSync(fd); } catch {}
  }
}

// Emit whole lines only; a partial trailing line is held until its newline
// arrives, so a log written in mid-line chunks never renders a torn line.
function emitText(entry, text, send) {
  const combined = entry.partial + text;
  const lastNl = combined.lastIndexOf('\n');
  if (lastNl === -1) { entry.partial = combined; return; }
  entry.partial = combined.slice(lastNl + 1);
  const whole = combined.slice(0, lastNl);
  if (whole) send({ id: entry.id, lines: whole.split(/\r?\n/) });
}

function start(id, filePath, send) {
  stop(id);
  let stat;
  try { stat = fs.statSync(filePath); }
  catch (err) { return { ok: false, error: err.message }; }

  const entry = { id, filePath, offset: 0, timer: null, partial: '' };
  tails.set(id, entry);

  // Seed with the tail end of the file so you're not staring at nothing while
  // waiting for the next write.
  const seedFrom = Math.max(0, stat.size - INITIAL_TAIL_BYTES);
  const seed = readRange(filePath, seedFrom, stat.size);
  entry.offset = stat.size;
  // A mid-line start would render a fragment as if it were a record; drop it.
  const firstNl = seedFrom > 0 ? seed.indexOf('\n') : -1;
  const seedText = firstNl >= 0 ? seed.slice(firstNl + 1) : seed;
  if (seedText) send({ id, lines: seedText.split(/\r?\n/), seed: true, truncatedStart: seedFrom > 0 });

  entry.timer = setInterval(() => {
    let s;
    try { s = fs.statSync(filePath); }
    catch { send({ id, gone: true }); stop(id); return; }

    if (s.size < entry.offset) {
      // Rotated or truncated — a stale offset would read garbage.
      entry.offset = 0;
      entry.partial = '';
      send({ id, rotated: true });
    }
    if (s.size > entry.offset) {
      const text = readRange(filePath, entry.offset, s.size);
      entry.offset = Math.min(s.size, entry.offset + MAX_CHUNK_BYTES);
      emitText(entry, text, send);
    }
  }, POLL_MS);

  return { ok: true, size: stat.size };
}

function stop(id) {
  const e = tails.get(id);
  if (!e) return { ok: true };
  if (e.timer) clearInterval(e.timer);
  tails.delete(id);
  return { ok: true };
}

function stopAll() {
  for (const id of [...tails.keys()]) stop(id);
}

module.exports = { start, stop, stopAll, POLL_MS, INITIAL_TAIL_BYTES };
