// =============================================================================
// Note++ — draw.io integration (main process)
// =============================================================================
// We DON'T bundle draw.io in the installer. Instead, the first time the user
// opens a draw.io tab, this service downloads the official webapp .war (a
// zip) from GitHub releases, extracts it under
//   %AppData%\notepp\drawio-bundle\
// and registers a custom `drawio://` protocol that serves files from that
// directory. The bundle survives Note++ upgrades because userData is
// explicitly preserved by electron-builder / NSIS.
//
// Pinned version model: we ship a single tested DRAWIO_VERSION constant. The
// user can manually trigger an upgrade via "? → Check for draw.io updates",
// which compares this constant to whatever's installed and prompts a refresh
// if newer.
//
// IPC surface (handlers wired in main.js):
//   drawio:status         → { installed, installedVersion, requestedVersion, path }
//   drawio:download       → kicks off download+extract; emits drawio:progress events
//   drawio:uninstall      → removes the bundle (admin-only / debug)
// =============================================================================

const { app, protocol, BrowserWindow, net } = require('electron');
const fs       = require('fs');
const path     = require('path');
const https    = require('https');
const extract  = require('extract-zip');

// ── Pinned version ──────────────────────────────────────────────────────────
// Bump this constant + ship a Note++ release whenever we've tested a newer
// draw.io. Asset URL pattern at jgraph/drawio is stable:
//     https://github.com/jgraph/drawio/releases/download/v{V}/draw.war
const DRAWIO_VERSION = '30.0.2';
const DRAWIO_URL = `https://github.com/jgraph/drawio/releases/download/v${DRAWIO_VERSION}/draw.war`;

// ── Bundle filesystem layout ────────────────────────────────────────────────
function bundleDir() {
  return path.join(app.getPath('userData'), 'drawio-bundle');
}
function versionFilePath() {
  return path.join(bundleDir(), 'version.json');
}
function indexHtmlPath() {
  return path.join(bundleDir(), 'index.html');
}

function readInstalledVersion() {
  try {
    const raw = fs.readFileSync(versionFilePath(), 'utf-8');
    return JSON.parse(raw).version || null;
  } catch { return null; }
}

function isInstalled() {
  return fs.existsSync(indexHtmlPath()) && !!readInstalledVersion();
}

function getStatus() {
  const installedVersion = readInstalledVersion();
  return {
    installed:        isInstalled(),
    installedVersion: installedVersion,
    requestedVersion: DRAWIO_VERSION,
    needsUpdate:      installedVersion && installedVersion !== DRAWIO_VERSION,
    path:             isInstalled() ? indexHtmlPath() : null,
  };
}

// ── Custom protocol: drawio:// → bundleDir() ────────────────────────────────
// Registering this lets the iframe load drawio://index.html?embed=1&… and
// have all its relative asset paths resolve naturally. Avoids file:// origin
// quirks and lets us add safe-path checks.
function registerProtocol() {
  // Privileged registration must happen BEFORE app.ready. main.js calls
  // registerSchemesAsPrivileged before app.whenReady() — see main.js bootstrap.
  protocol.registerFileProtocol('drawio', (request, callback) => {
    try {
      const url = new URL(request.url);
      // host + pathname together form the path under bundleDir. We strip
      // leading slashes and resolve to an absolute path, then guard against
      // traversal that would escape bundleDir.
      let relPath = decodeURIComponent((url.host || '') + url.pathname);
      relPath = relPath.replace(/^\/+/, '');
      if (!relPath || relPath.endsWith('/')) relPath += 'index.html';
      const abs = path.normalize(path.join(bundleDir(), relPath));
      const base = path.normalize(bundleDir() + path.sep);
      if (!abs.startsWith(base)) {
        return callback({ error: -10 /* NET_ERR_ACCESS_DENIED */ });
      }
      callback({ path: abs });
    } catch (err) {
      console.error('[drawio] protocol error:', err);
      callback({ error: -2 /* FAILED */ });
    }
  });
}

// ── Download + extract pipeline ─────────────────────────────────────────────
// Follows GitHub release redirects (302 → S3). Emits progress events to all
// open BrowserWindows via the supplied channel so renderer UIs can show a
// progress bar.
function emitProgress(channel, payload) {
  for (const w of BrowserWindow.getAllWindows()) {
    try { w.webContents.send(channel, payload); } catch {}
  }
}

function httpsGetFollowingRedirects(url, onResponse, onError) {
  const req = https.get(url, { headers: { 'User-Agent': 'NotePP-Updater' } }, (res) => {
    if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
      res.resume(); // drain
      return httpsGetFollowingRedirects(res.headers.location, onResponse, onError);
    }
    if (res.statusCode !== 200) {
      onError(new Error(`HTTP ${res.statusCode} fetching ${url}`));
      res.resume();
      return;
    }
    onResponse(res);
  });
  req.on('error', onError);
  req.setTimeout(60_000, () => req.destroy(new Error('Timeout fetching ' + url)));
}

async function download() {
  if (isInstalled() && readInstalledVersion() === DRAWIO_VERSION) {
    return { success: true, alreadyInstalled: true };
  }

  const tmpZip = path.join(app.getPath('temp'), `drawio-${DRAWIO_VERSION}-${Date.now()}.war`);
  const tmpExtractDir = path.join(app.getPath('temp'), `drawio-${DRAWIO_VERSION}-${Date.now()}-extract`);

  // 1. Download .war to temp ------------------------------------------------
  emitProgress('drawio:progress', { phase: 'download', percent: 0, version: DRAWIO_VERSION });
  await new Promise((resolve, reject) => {
    httpsGetFollowingRedirects(
      DRAWIO_URL,
      (res) => {
        const total = parseInt(res.headers['content-length'] || '0', 10) || 0;
        let got = 0;
        const sink = fs.createWriteStream(tmpZip);
        res.on('data', (chunk) => {
          got += chunk.length;
          if (total > 0) {
            emitProgress('drawio:progress', {
              phase: 'download',
              percent: Math.round((got / total) * 100),
              bytes: got,
              totalBytes: total,
              version: DRAWIO_VERSION,
            });
          }
        });
        res.pipe(sink);
        sink.on('finish', () => sink.close(resolve));
        sink.on('error', reject);
        res.on('error', reject);
      },
      reject
    );
  });

  // 2. Extract --------------------------------------------------------------
  emitProgress('drawio:progress', { phase: 'extract', percent: 0, version: DRAWIO_VERSION });
  await extract(tmpZip, { dir: tmpExtractDir });

  // 3. Validate that index.html is present -----------------------------------
  if (!fs.existsSync(path.join(tmpExtractDir, 'index.html'))) {
    // .war files sometimes nest under WEB-INF — check one level deep
    const entries = fs.readdirSync(tmpExtractDir);
    const nest = entries.find(e => fs.existsSync(path.join(tmpExtractDir, e, 'index.html')));
    if (nest) {
      // Move nested contents up
      const nestedDir = path.join(tmpExtractDir, nest);
      for (const f of fs.readdirSync(nestedDir)) {
        fs.renameSync(path.join(nestedDir, f), path.join(tmpExtractDir, f));
      }
      fs.rmdirSync(nestedDir);
    } else {
      throw new Error('Extracted draw.war is missing index.html');
    }
  }

  // 4. Swap into the final bundle dir atomically -----------------------------
  emitProgress('drawio:progress', { phase: 'install', percent: 0, version: DRAWIO_VERSION });
  const finalDir = bundleDir();
  // Replace existing bundle (if upgrading) by removing the old one first.
  if (fs.existsSync(finalDir)) {
    try { fs.rmSync(finalDir, { recursive: true, force: true }); } catch {}
  }
  fs.mkdirSync(path.dirname(finalDir), { recursive: true });
  fs.renameSync(tmpExtractDir, finalDir);

  // 5. Write version marker --------------------------------------------------
  fs.writeFileSync(versionFilePath(), JSON.stringify({
    version: DRAWIO_VERSION,
    downloadedAt: new Date().toISOString(),
  }, null, 2));

  // 6. Cleanup ---------------------------------------------------------------
  try { fs.unlinkSync(tmpZip); } catch {}

  emitProgress('drawio:progress', { phase: 'done', percent: 100, version: DRAWIO_VERSION });
  return { success: true, version: DRAWIO_VERSION, path: indexHtmlPath() };
}

function uninstall() {
  const d = bundleDir();
  if (fs.existsSync(d)) fs.rmSync(d, { recursive: true, force: true });
  return { success: true };
}

module.exports = {
  DRAWIO_VERSION,
  bundleDir,
  indexHtmlPath,
  isInstalled,
  readInstalledVersion,
  getStatus,
  registerProtocol,
  download,
  uninstall,
};
