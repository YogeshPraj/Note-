#!/usr/bin/env node
// Fetches the assets the repository does not store, on any OS (Windows, macOS, Linux):
//   assets/mermaid.min.js                       Mermaid, for diagram previews
//   assets/whiteboard/whiteboard.bundle.{js,css} the Excalidraw whiteboard
//   assets/whiteboard/excalidraw-fonts/          its fonts
// They are built from whiteboard/ and old-src/ (exact versions and hashes in its package-lock.json).
// Safe to re-run. Needs Node.js 18+.
//
//   node scripts/setup-assets.js
'use strict';
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const proj = path.resolve(__dirname, '..');
const src = path.join(proj, 'old-src');
const step = (m) => console.log(`==> ${m}`);
const run = (cmd, args) => execFileSync(cmd, args, { cwd: src, stdio: 'inherit', shell: process.platform === 'win32' });

const mermaid = path.join(src, 'node_modules', 'mermaid', 'dist', 'mermaid.min.js');
const bundleJs = path.join(src, 'src', 'whiteboard.bundle.js');
const bundleCss = path.join(src, 'src', 'whiteboard.bundle.css');

if (!fs.existsSync(mermaid) || !fs.existsSync(bundleJs)) {
  if (!fs.existsSync(path.join(src, 'node_modules', 'mermaid'))) {
    step('npm ci (lifecycle scripts skipped)');
    run('npm', ['ci', '--ignore-scripts', '--no-audit', '--no-fund']);
  }
  if (!fs.existsSync(bundleJs)) {
    step('Building the Excalidraw bundle');
    run('node', ['build-whiteboard.js']);
  }
}

const wb = path.join(proj, 'assets', 'whiteboard');
fs.mkdirSync(wb, { recursive: true });
fs.copyFileSync(mermaid, path.join(proj, 'assets', 'mermaid.min.js'));
fs.copyFileSync(bundleJs, path.join(wb, 'whiteboard.bundle.js'));
fs.copyFileSync(bundleCss, path.join(wb, 'whiteboard.bundle.css'));
const fonts = path.join(src, 'src', 'excalidraw-fonts');
if (fs.existsSync(fonts)) {
  fs.rmSync(path.join(wb, 'excalidraw-fonts'), { recursive: true, force: true });
  fs.cpSync(fonts, path.join(wb, 'excalidraw-fonts'), { recursive: true });
}

for (const f of ['assets/mermaid.min.js', 'assets/whiteboard/whiteboard.bundle.js', 'assets/whiteboard/excalidraw-fonts']) {
  if (!fs.existsSync(path.join(proj, f))) {
    console.error(`Expected asset is missing: ${f}`);
    process.exit(1);
  }
}
console.log('Mermaid and whiteboard assets are ready');
