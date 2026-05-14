// =============================================================================
// Note++ — Git service (main process)
// =============================================================================
// Thin wrapper around the system `git` CLI. Designed to be called from IPC
// handlers in main.js — every method returns a Promise that resolves to a
// plain JSON object. Errors include the stderr output for diagnostic UI.
//
// Detection: walks up from a file path looking for `.git` (directory OR file
// in worktrees). Cached at the call site (in renderer's gitState).
//
// Output parsing: uses `--porcelain=v1 -b -z` for stable, NUL-separated
// machine-readable output that handles paths with spaces / quotes / newlines.
// =============================================================================

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

// ── Helpers ────────────────────────────────────────────────────────────────
function runGit(cwd, args, opts = {}) {
  return new Promise((resolve) => {
    const proc = spawn('git', args, {
      cwd,
      windowsHide: true,
      env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' },
      ...opts,
    });
    const stdoutChunks = [];
    const stderrChunks = [];
    proc.stdout.on('data', (d) => stdoutChunks.push(d));
    proc.stderr.on('data', (d) => stderrChunks.push(d));
    proc.on('error', (err) => {
      resolve({ success: false, error: err.message, code: -1, stdout: '', stderr: err.message });
    });
    proc.on('close', (code) => {
      const stdout = Buffer.concat(stdoutChunks).toString('utf-8');
      const stderr = Buffer.concat(stderrChunks).toString('utf-8');
      resolve({
        success: code === 0,
        code,
        stdout,
        stderr,
        error: code === 0 ? null : (stderr.trim() || `git ${args[0]} exited ${code}`),
      });
    });
  });
}

// Walk up from `startPath` (a file or folder) looking for a `.git` entry.
// Returns the repo root (folder containing `.git`) or null.
function findRepoRoot(startPath) {
  if (!startPath) return null;
  let dir = startPath;
  try {
    const stat = fs.statSync(dir);
    if (stat.isFile()) dir = path.dirname(dir);
  } catch { return null; }
  while (true) {
    const dotGit = path.join(dir, '.git');
    if (fs.existsSync(dotGit)) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

// ── Status (porcelain v1, NUL-separated) ──────────────────────────────────
// Returns { success, branch, upstream, ahead, behind, files: [{x,y,path,renameFrom}] }
async function status(repoRoot) {
  const r = await runGit(repoRoot, ['status', '--porcelain=v1', '-b', '-z']);
  if (!r.success) return { success: false, error: r.error, files: [] };

  const out = { success: true, branch: null, upstream: null, ahead: 0, behind: 0, files: [] };
  // -z separates entries by NUL. Rename entries (`R `) span TWO NUL-separated
  // tokens: "R  newpath\0oldpath".
  const tokens = r.stdout.split('\0');
  let i = 0;
  while (i < tokens.length) {
    const line = tokens[i];
    if (!line) { i++; continue; }
    if (line.startsWith('## ')) {
      // Branch header: "## main...origin/main [ahead 1, behind 0]"
      // or for detached: "## HEAD (no branch)"
      const body = line.slice(3);
      const ahead = body.match(/ahead (\d+)/);
      const behind = body.match(/behind (\d+)/);
      out.ahead = ahead ? parseInt(ahead[1]) : 0;
      out.behind = behind ? parseInt(behind[1]) : 0;
      // Branch name is everything before "..." or "(no branch)" or " [..."
      let head = body;
      const dotsIdx = head.indexOf('...');
      if (dotsIdx >= 0) {
        out.branch = head.slice(0, dotsIdx);
        const rest = head.slice(dotsIdx + 3);
        const upMatch = rest.match(/^([^\s\[]+)/);
        out.upstream = upMatch ? upMatch[1] : null;
      } else {
        const cleanMatch = head.match(/^([^\s\[]+)/);
        out.branch = cleanMatch ? cleanMatch[1] : head.trim();
      }
      i++;
      continue;
    }
    // File line: "XY path"
    const x = line[0];
    const y = line[1];
    const filePath = line.slice(3);
    if (x === 'R' || x === 'C') {
      // Renames: next token is the original path
      const renameFrom = tokens[i + 1] || '';
      out.files.push({ x, y, path: filePath, renameFrom });
      i += 2;
    } else {
      out.files.push({ x, y, path: filePath, renameFrom: null });
      i++;
    }
  }
  return out;
}

// ── Stage / unstage / discard / commit ─────────────────────────────────────
async function stage(repoRoot, paths) {
  if (!paths || !paths.length) return { success: true };
  return await runGit(repoRoot, ['add', '--', ...paths]);
}
async function unstage(repoRoot, paths) {
  if (!paths || !paths.length) return { success: true };
  // git restore --staged is the modern equivalent of `git reset HEAD --`
  return await runGit(repoRoot, ['restore', '--staged', '--', ...paths]);
}
async function discard(repoRoot, paths) {
  if (!paths || !paths.length) return { success: true };
  // Discard worktree changes. For untracked files this is a no-op (use clean).
  return await runGit(repoRoot, ['restore', '--', ...paths]);
}
async function cleanUntracked(repoRoot, paths) {
  if (!paths || !paths.length) return { success: true };
  return await runGit(repoRoot, ['clean', '-f', '--', ...paths]);
}
async function commit(repoRoot, message) {
  if (!message || !message.trim()) {
    return { success: false, error: 'Commit message is empty' };
  }
  return await runGit(repoRoot, ['commit', '-m', message]);
}
// Amend the last commit. If message is empty, keep the existing message.
async function commitAmend(repoRoot, message) {
  if (message && message.trim()) {
    return await runGit(repoRoot, ['commit', '--amend', '-m', message]);
  }
  return await runGit(repoRoot, ['commit', '--amend', '--no-edit']);
}

// ── Remote ops ─────────────────────────────────────────────────────────────
async function fetch(repoRoot)  { return await runGit(repoRoot, ['fetch', '--all', '--prune']); }
async function pull(repoRoot)   { return await runGit(repoRoot, ['pull']); }
async function push(repoRoot)   { return await runGit(repoRoot, ['push']); }
async function pushSetUpstream(repoRoot, remote, branch) {
  return await runGit(repoRoot, ['push', '--set-upstream', remote || 'origin', branch]);
}
// Sync = pull then push (uses the user's pull.rebase config naturally).
async function sync(repoRoot) {
  const p = await pull(repoRoot);
  if (!p.success) return p;
  return await push(repoRoot);
}

// ── Branches ───────────────────────────────────────────────────────────────
// Returns { success, current, locals: [string], remotes: [string] }
async function branchList(repoRoot) {
  const r = await runGit(repoRoot, ['branch', '-a', '--format=%(refname:short)\t%(HEAD)']);
  if (!r.success) return { success: false, error: r.error, locals: [], remotes: [], current: null };
  const out = { success: true, locals: [], remotes: [], current: null };
  for (const line of r.stdout.split('\n')) {
    if (!line) continue;
    const [name, head] = line.split('\t');
    if (head && head.trim() === '*') out.current = name;
    if (name.startsWith('origin/') || name.includes('/')) out.remotes.push(name);
    else out.locals.push(name);
  }
  return out;
}
async function branchSwitch(repoRoot, name) {
  return await runGit(repoRoot, ['switch', name]);
}
async function branchCreate(repoRoot, name, fromRef) {
  if (fromRef) return await runGit(repoRoot, ['switch', '-c', name, fromRef]);
  return await runGit(repoRoot, ['switch', '-c', name]);
}

// ── Misc utilities for the renderer ────────────────────────────────────────
async function getCurrentBranch(repoRoot) {
  const r = await runGit(repoRoot, ['rev-parse', '--abbrev-ref', 'HEAD']);
  if (!r.success) return null;
  return r.stdout.trim();
}

// Quick liveness check — used at startup to detect if git is even installed.
async function isGitInstalled() {
  const r = await runGit(process.cwd(), ['--version']);
  return r.success;
}

module.exports = {
  runGit, findRepoRoot,
  status,
  stage, unstage, discard, cleanUntracked, commit, commitAmend,
  fetch, pull, push, pushSetUpstream, sync,
  branchList, branchSwitch, branchCreate, getCurrentBranch,
  isGitInstalled,
};
