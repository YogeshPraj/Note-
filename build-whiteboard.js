// =============================================================================
// build-whiteboard.js — bundles src/whiteboard-app.jsx into src/whiteboard.bundle.{js,css}
//                       and copies Excalidraw fonts to src/excalidraw-fonts/
// =============================================================================
// Run automatically via npm `postinstall` and `prestart`, or manually with:
//   node build-whiteboard.js
//   node build-whiteboard.js --watch   (rebuild on change during dev)
// =============================================================================
const esbuild = require('esbuild');
const fs      = require('fs');
const path    = require('path');

const ROOT = __dirname;
const ENTRY  = path.join(ROOT, 'src', 'whiteboard-app.jsx');
const OUTDIR = path.join(ROOT, 'src');

const FONT_SRC = path.join(ROOT, 'node_modules', '@excalidraw', 'excalidraw', 'dist', 'prod', 'fonts');
const FONT_DST = path.join(ROOT, 'src', 'excalidraw-fonts');

// ─── 1. Self-host the Excalidraw font assets ─────────────────────────────────
function copyDirRecursive(src, dst) {
  if (!fs.existsSync(src)) {
    console.warn(`[wb-build] font source not found: ${src} — skipping copy`);
    return;
  }
  fs.mkdirSync(dst, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const s = path.join(src, entry.name);
    const d = path.join(dst, entry.name);
    if (entry.isDirectory()) copyDirRecursive(s, d);
    else fs.copyFileSync(s, d);
  }
}

console.log('[wb-build] copying Excalidraw fonts → src/excalidraw-fonts/');
copyDirRecursive(FONT_SRC, FONT_DST);

// ─── 2. Bundle the React app ─────────────────────────────────────────────────
const buildOptions = {
  entryPoints: [ENTRY],
  bundle: true,
  outdir: OUTDIR,
  entryNames: 'whiteboard.bundle',
  assetNames: 'whiteboard-assets/[name]-[hash]',
  format: 'iife',
  platform: 'browser',
  target: ['chrome120'],   // Electron 28 ships Chromium 120
  jsx: 'automatic',
  loader: {
    '.woff':   'file',
    '.woff2':  'file',
    '.ttf':    'file',
    '.png':    'file',
    '.svg':    'file',
    '.jpg':    'file',
  },
  // Excalidraw expects `process.env.NODE_ENV` to be defined and many React
  // libs check it explicitly. Pin to production for smaller, dead-code-free
  // output.
  define: {
    'process.env.NODE_ENV': '"production"',
    'process.env.IS_PREACT': '"false"',
  },
  // Excalidraw uses Node-style export conditions (`development` / `production`)
  // rather than `default`. Tell esbuild to pick the production bundle.
  conditions: ['production', 'browser', 'import', 'module', 'default'],
  minify: true,
  sourcemap: false,
  legalComments: 'none',
  logLevel: 'info',
};

const watchMode = process.argv.includes('--watch');

(async () => {
  try {
    if (watchMode) {
      const ctx = await esbuild.context(buildOptions);
      await ctx.watch();
      console.log('[wb-build] watching for changes…');
    } else {
      await esbuild.build(buildOptions);
      console.log('[wb-build] done.');
    }
  } catch (err) {
    console.error('[wb-build] failed:', err);
    process.exit(1);
  }
})();
