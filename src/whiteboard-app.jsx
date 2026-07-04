// =============================================================================
// Note++ Whiteboard — Excalidraw-powered React app (runs inside whiteboard.html)
// =============================================================================
// Bridged to the parent renderer via postMessage. Protocol (unchanged from v1):
//
//   parent → iframe : wb-load {content}, wb-theme {dark}, wb-get-data
//   iframe → parent : wb-ready, wb-state {content}, wb-data {content},
//                     wb-save-request
//
// File format envelope (always sent to / received from parent):
//   { __wb__: true, version: 2, source: 'excalidraw',
//     elements: [...],            // Excalidraw scene elements
//     appState: { ... },          // Excalidraw appState subset
//     files:    { ... } }         // Excalidraw binary files (images)
//
// Legacy v1 payloads (`version: 1`, custom canvas shapes) are converted to v2
// on load via `legacyV1ToV2()` so old whiteboard-N.json files keep working.
// =============================================================================

import React, { useEffect, useRef, useState, useCallback } from 'react';
import { createRoot } from 'react-dom/client';
import { Excalidraw, serializeAsJSON, FONT_FAMILY } from '@excalidraw/excalidraw';
import '@excalidraw/excalidraw/index.css';

// Default appState used when a brand-new whiteboard is opened (no saved
// scene, or saved scene didn't specify these). Excalidraw's stock
// defaults are hand-drawn (Virgil font + medium roughness) which suits
// sketching but reads as childish for diagrams in a professional
// context. We default to a clean sans-serif + perfectly straight lines.
// Users can still flip back via the toolbar at any time.
const PROFESSIONAL_DEFAULTS = {
  currentItemFontFamily: FONT_FAMILY.Helvetica, // 2 — Helvetica sans-serif
  currentItemRoughness:  0,                     // architect (no roughness)
  currentItemStrokeWidth: 2,                    // a touch thicker than 1px default
};

// ── Excalidraw library persistence ──────────────────────────────────────────
// Libraries (Browse Libraries → install) are GLOBAL across whiteboards but
// Excalidraw doesn't persist them on its own — the host app has to:
//   1. read libraryItems on mount and feed them via initialData.libraryItems
//   2. listen to onLibraryChange and save the new list anywhere durable
// We use the iframe's own localStorage, which is scoped to whiteboard.html's
// file:// origin and survives across Note++ launches.
const LIBRARY_STORAGE_KEY = 'notepp.excalidraw.libraryItems.v1';
// One-shot cleanup: earlier builds shipped a converted Azure icon library
// that got auto-loaded into every user's personal library on first launch.
// That feature was reverted; this wipe removes the leftover items so users
// don't keep seeing them. Excalidraw's built-in "Insert image" tool is the
// intended path for using SVG icons on the whiteboard now.
const CLEANUP_MARKER = 'notepp.excalidraw.cleanup.v20260703-revert';
if (typeof window !== 'undefined') {
  try {
    if (!localStorage.getItem(CLEANUP_MARKER)) {
      localStorage.removeItem(LIBRARY_STORAGE_KEY);
      localStorage.setItem(CLEANUP_MARKER, '1');
    }
  } catch {}
}
function readSavedLibraryItems() {
  try {
    const raw = localStorage.getItem(LIBRARY_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch (e) {
    console.warn('[wb] failed to read saved library items:', e);
    return [];
  }
}
function saveLibraryItems(items) {
  try {
    localStorage.setItem(LIBRARY_STORAGE_KEY, JSON.stringify(items || []));
  } catch (e) {
    console.warn('[wb] failed to persist library items:', e);
  }
}

// Self-host fonts under ./excalidraw-fonts/  (avoids any CDN call)
if (typeof window !== 'undefined') {
  window.EXCALIDRAW_ASSET_PATH = './excalidraw-fonts/';
}

// ─── v1 (legacy custom whiteboard) → v2 (Excalidraw) converter ───────────────
// Old format produced by previous whiteboard.js. Best-effort migration so users
// don't lose existing drawings.
function legacyV1ToV2(v1) {
  const out = [];
  let seq = 0;
  const mkId = () => `legacy_${++seq}_${Date.now().toString(36)}`;
  const baseProps = (el) => ({
    id: mkId(),
    seed: Math.floor(Math.random() * 2 ** 31),
    version: 1,
    versionNonce: Math.floor(Math.random() * 2 ** 31),
    isDeleted: false,
    fillStyle: el.fill === 'hatch' ? 'hachure' : (el.fill === 'solid' ? 'solid' : 'hachure'),
    strokeWidth: Math.round(el.sw || 1.5),
    strokeStyle: 'solid',
    roughness: el.ro != null ? el.ro : 1,
    opacity: 100,
    angle: 0,
    strokeColor: el.sc || '#1e1e1e',
    backgroundColor: (el.fc && el.fc !== 'none') ? el.fc : 'transparent',
    groupIds: [],
    frameId: null,
    roundness: null,
    boundElements: null,
    updated: Date.now(),
    link: null,
    locked: false,
  });
  for (const el of (v1.elements || [])) {
    try {
      if (el.type === 'rectangle' || el.type === 'ellipse' || el.type === 'diamond') {
        out.push({ ...baseProps(el), type: el.type, x: el.x, y: el.y, width: el.w || 0, height: el.h || 0 });
      } else if (el.type === 'arrow' || el.type === 'line') {
        const [a, b] = el.pts || [[0,0],[0,0]];
        out.push({
          ...baseProps(el),
          type: el.type,
          x: a[0], y: a[1],
          width: b[0] - a[0], height: b[1] - a[1],
          points: [[0, 0], [b[0] - a[0], b[1] - a[1]]],
          lastCommittedPoint: null, startBinding: null, endBinding: null,
          startArrowhead: null,
          endArrowhead: el.type === 'arrow' ? 'arrow' : null,
        });
      } else if (el.type === 'pencil') {
        const pts = el.pts || [];
        const ox = pts.length ? pts[0][0] : 0;
        const oy = pts.length ? pts[0][1] : 0;
        const local = pts.map(p => [p[0] - ox, p[1] - oy]);
        const xs = local.map(p => p[0]); const ys = local.map(p => p[1]);
        out.push({
          ...baseProps(el),
          type: 'freedraw',
          x: ox, y: oy,
          width: Math.max(...xs, 0) - Math.min(...xs, 0),
          height: Math.max(...ys, 0) - Math.min(...ys, 0),
          points: local,
          pressures: [],
          simulatePressure: true,
          lastCommittedPoint: null,
        });
      } else if (el.type === 'text') {
        out.push({
          ...baseProps(el),
          type: 'text',
          x: el.x, y: el.y,
          width: el.w || 100, height: el.h || (el.fs || 20) * 1.4,
          text: el.text || '',
          fontSize: el.fs || 20,
          fontFamily: 1, // Virgil (Excalidraw's hand-drawn font)
          textAlign: 'left',
          verticalAlign: 'top',
          baseline: (el.fs || 20) * 0.9,
          containerId: null,
          originalText: el.text || '',
          lineHeight: 1.25,
          autoResize: true,
        });
      }
    } catch (e) {
      console.warn('[wb] legacy element skipped:', el, e);
    }
  }
  return { elements: out, appState: { viewBackgroundColor: '#ffffff' }, files: {} };
}

// Robustly parse whatever the parent sent and produce an Excalidraw scene.
function parseIncoming(jsonStr) {
  if (!jsonStr || typeof jsonStr !== 'string') return null;
  let data;
  try { data = JSON.parse(jsonStr); } catch { return null; }
  if (!data || typeof data !== 'object') return null;

  // v2: our Excalidraw-flavoured envelope
  if (data.__wb__ === true && data.version === 2 && Array.isArray(data.elements)) {
    return {
      elements: data.elements,
      appState: data.appState || {},
      files: data.files || {},
    };
  }
  // v1: legacy custom-canvas envelope
  if (data.__wb__ === true && data.version === 1 && Array.isArray(data.elements)) {
    return legacyV1ToV2(data);
  }
  // Raw Excalidraw `.excalidraw` file (no envelope, with `type: "excalidraw"`)
  if (data.type === 'excalidraw' && Array.isArray(data.elements)) {
    return {
      elements: data.elements,
      appState: data.appState || {},
      files: data.files || {},
    };
  }
  return null;
}

// Build the envelope we send back to the parent.
function buildEnvelope(elements, appState, files) {
  // Strip volatile/large fields from appState so the saved file stays small
  // and reproducible across sessions.
  const slim = appState ? {
    viewBackgroundColor: appState.viewBackgroundColor,
    gridSize: appState.gridSize,
    currentItemStrokeColor: appState.currentItemStrokeColor,
    currentItemBackgroundColor: appState.currentItemBackgroundColor,
    currentItemFillStyle: appState.currentItemFillStyle,
    currentItemStrokeWidth: appState.currentItemStrokeWidth,
    currentItemStrokeStyle: appState.currentItemStrokeStyle,
    currentItemRoughness: appState.currentItemRoughness,
    currentItemOpacity: appState.currentItemOpacity,
    currentItemFontFamily: appState.currentItemFontFamily,
    currentItemFontSize: appState.currentItemFontSize,
    currentItemTextAlign: appState.currentItemTextAlign,
    currentItemStartArrowhead: appState.currentItemStartArrowhead,
    currentItemEndArrowhead: appState.currentItemEndArrowhead,
    scrollX: appState.scrollX,
    scrollY: appState.scrollY,
    zoom: appState.zoom,
  } : {};
  return JSON.stringify({
    __wb__: true,
    version: 2,
    source: 'excalidraw',
    elements: elements || [],
    appState: slim,
    files: files || {},
  });
}

function App() {
  const [api, setApi] = useState(null);
  const [theme, setTheme] = useState('light');
  // Read the persisted library once at first render — Excalidraw consumes
  // initialData synchronously on mount, so we can't wait on async I/O.
  // localStorage is sync, so this is safe.
  const [initialLibraryItems] = useState(() => readSavedLibraryItems());
  // Debounce parent-state notifications: Excalidraw onChange fires *very*
  // often. We only need committed snapshots, not every pointer move.
  const stateTimer = useRef(null);
  const lastSerialized = useRef('');
  // Per-load generation counter. Bumped every time the parent sends wb-load
  // (e.g. when the user switches to a different whiteboard tab — the iframe
  // is a single shared instance reused across all whiteboard tabs). Any
  // pending wb-state debounce that was queued for an OLD scene is dropped if
  // the generation has moved on by the time the timer fires. Without this,
  // a tab switch could leak the old tab's drawing into the new tab's
  // `content` (the parent's wb-state handler writes to whichever tab is
  // currently active).
  const loadGen = useRef(0);
  // While loadSuppress is non-zero, onChange events are ignored entirely.
  // updateScene() synchronously fires onChange after applying a scene, and
  // we don't want that load-triggered echo to mark the new tab dirty.
  const loadSuppress = useRef(0);

  const sendState = useCallback((elements, appState, files, gen) => {
    if (gen !== loadGen.current) return;          // stale; new scene loaded since
    const payload = buildEnvelope(elements, appState, files);
    if (payload === lastSerialized.current) return;
    lastSerialized.current = payload;
    window.parent?.postMessage({ type: 'wb-state', content: payload }, '*');
  }, []);

  const scheduleSendState = useCallback((elements, appState, files) => {
    if (loadSuppress.current > 0) return;         // load-triggered onChange — ignore
    clearTimeout(stateTimer.current);
    const gen = loadGen.current;
    stateTimer.current = setTimeout(() => sendState(elements, appState, files, gen), 350);
  }, [sendState]);

  // Wire postMessage listener
  useEffect(() => {
    const onMsg = (e) => {
      const m = e.data;
      if (!m || !m.type) return;

      if (m.type === 'wb-load') {
        const scene = parseIncoming(m.content);
        if (scene && api) {
          // 1. Bump the load generation — any debounced wb-state queued for
          //    the previous scene will be dropped when its timer fires
          //    (sendState compares the captured gen to the current one).
          loadGen.current += 1;
          // 2. Cancel any pending debounce so it can't run at all before
          //    the generation check would catch it.
          clearTimeout(stateTimer.current);
          stateTimer.current = null;
          // 3. Suppress all onChange events while we apply the new scene.
          //    updateScene synchronously fires onChange; without this guard
          //    we'd echo the load back as a "user edit" and mark a freshly
          //    activated tab dirty.
          loadSuppress.current += 1;
          try {
            api.updateScene({
              elements: scene.elements,
              appState: scene.appState,
              commitToHistory: false,
            });
            if (scene.files && Object.keys(scene.files).length) {
              api.addFiles(Object.values(scene.files));
            }
          } finally {
            // Release on the next tick so any synchronous + microtask
            // onChange echoes are covered. (Excalidraw fires onChange
            // synchronously from inside updateScene, so the decrement
            // running before the dispatch chain finishes would be wrong.)
            setTimeout(() => { loadSuppress.current = Math.max(0, loadSuppress.current - 1); }, 0);
          }
          // Prime the dedup guard so the post-load steady-state matches.
          lastSerialized.current = buildEnvelope(scene.elements, scene.appState, scene.files);
        }
        return;
      }

      if (m.type === 'wb-theme') {
        setTheme(m.dark ? 'dark' : 'light');
        return;
      }

      if (m.type === 'wb-get-data') {
        if (!api) return;
        const els = api.getSceneElements();
        const app = api.getAppState();
        const files = api.getFiles();
        window.parent?.postMessage({
          type: 'wb-data',
          content: buildEnvelope(els, app, files),
        }, '*');
        return;
      }

      if (m.type === 'wb-add-icon') {
        // Icon-panel drag-drop: insert an SVG as an image element at
        // the drop coords. clientX/clientY arrive in iframe-pixel
        // space; convert to canvas coords via the current viewport
        // transform (scrollX/scrollY/zoom).
        if (!api || !m.dataURL) return;
        try {
          const st = api.getAppState();
          const zoom = st.zoom?.value || 1;
          const canvasX = (m.clientX - (st.scrollX || 0) * zoom) / zoom - 24;
          const canvasY = (m.clientY - (st.scrollY || 0) * zoom) / zoom - 24;
          const fileId = 'ic-' + Math.random().toString(36).slice(2, 10) + Date.now().toString(36);
          api.addFiles([{
            id: fileId,
            mimeType: 'image/svg+xml',
            dataURL: m.dataURL,
            created: Date.now(),
            lastRetrieved: Date.now(),
          }]);
          const newEl = {
            id: 'el-' + Math.random().toString(36).slice(2, 10),
            type: 'image',
            x: canvasX, y: canvasY,
            width: 48, height: 48,
            angle: 0,
            strokeColor: 'transparent',
            backgroundColor: 'transparent',
            fillStyle: 'solid',
            strokeWidth: 1,
            strokeStyle: 'solid',
            roughness: 0,
            opacity: 100,
            groupIds: [],
            seed: Math.floor(Math.random() * 2 ** 31),
            version: 1,
            versionNonce: Math.floor(Math.random() * 2 ** 31),
            isDeleted: false,
            frameId: null,
            roundness: null,
            boundElements: null,
            updated: Date.now(),
            link: null,
            locked: false,
            crop: null,
            fileId,
            status: 'saved',
            scale: [1, 1],
          };
          const cur = api.getSceneElements();
          api.updateScene({ elements: [...cur, newEl] });
        } catch (err) { console.warn('[wb] add-icon failed', err); }
        return;
      }

    };
    window.addEventListener('message', onMsg);
    if (api) {
      window.parent?.postMessage({ type: 'wb-ready' }, '*');
    }
    return () => window.removeEventListener('message', onMsg);
  }, [api]);

  // Ctrl+S → ask parent to save
  useEffect(() => {
    const onKey = (e) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) {
        e.preventDefault();
        window.parent?.postMessage({ type: 'wb-save-request' }, '*');
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const handleChange = useCallback((elements, appState, files) => {
    scheduleSendState(elements, appState, files);
  }, [scheduleSendState]);

  return (
    <div style={{ position: 'fixed', inset: 0 }}>
      <Excalidraw
        excalidrawAPI={(a) => setApi(a)}
        onChange={handleChange}
        onLibraryChange={saveLibraryItems}
        theme={theme}
        initialData={{
          appState: PROFESSIONAL_DEFAULTS,
          libraryItems: initialLibraryItems,
          scrollToContent: true,
        }}
        UIOptions={{
          canvasActions: {
            // Hide cloud / library / share buttons that don't apply to a
            // desktop-embedded use case.
            loadScene: false,
            saveAsImage: true,
            saveToActiveFile: false,
            export: { saveFileToDisk: false },
          },
        }}
      />
    </div>
  );
}

const root = createRoot(document.getElementById('root'));
root.render(<App />);

// Remove the static "Loading whiteboard…" placeholder now that React is mounted.
// (It sits behind Excalidraw's transparent middle area otherwise.)
const bootMsg = document.getElementById('boot-msg');
if (bootMsg) bootMsg.remove();
