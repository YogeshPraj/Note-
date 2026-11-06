// The tree as Excalidraw shapes, and the mouse on them. A tree is the shapes of one group: its frame holds the data, and
// each shape carries the `key` of the part it draws. Shapes are found again by key inside the group, never by a fixed id,
// because copying a tree or dragging it from the library gives every shape a new id.
//
// Undo needs three things of a redraw: the same shape keeps its id with a higher version, a shape that goes is marked
// deleted rather than dropped, and the change is captured. A click kept from Excalidraw must also give it the keyboard
// back, or Ctrl+Z never reaches it.

import { CaptureUpdateAction, convertToExcalidrawElements, viewportCoordsToSceneCoords } from '@excalidraw/excalidraw';

import { fileId, iconFile, iconFiles } from './icons.js';
import { clone, newId, shapes } from './model.js';

const INK = '#1e1e1e';
const LINE = '#888780';
const PAPER = '#ffffff';
const FOLD_SIZE = 10;
/** How far outside a fold box a click still counts, in scene units. */
const FOLD_SLOP = 3;

const groupOf = (e) => e.groupIds?.[e.groupIds.length - 1];
const isTree = (e) => Boolean(e.customData?.notepp_tree);

function describe(s, group) {
  // A line left to itself is drawn right but measures 100 wide, which matters to whatever asks where it is.
  const extent = s.points ? { width: Math.max(...s.points.map((q) => Math.abs(q[0]))), height: Math.max(...s.points.map((q) => Math.abs(q[1]))) } : {};
  const plain = { roughness: 0, strokeWidth: 1, opacity: 100, strokeStyle: 'solid', backgroundColor: 'transparent', fillStyle: 'solid', strokeColor: LINE };
  const customData = { notepp_tree: true, key: s.key, role: s.role, ...(s.node ? { node: s.node } : {}) };
  const common = { x: s.x, y: s.y, groupIds: [group], customData };
  switch (s.kind) {
    case 'frame': return { ...plain, ...common, type: 'rectangle', width: s.width, height: s.height, backgroundColor: PAPER, roundness: { type: 3, value: 4 } };
    case 'line': return { ...plain, ...common, ...extent, type: 'line', points: s.points, strokeStyle: 'dotted' };
    case 'box': return { ...plain, ...common, type: 'rectangle', width: FOLD_SIZE, height: FOLD_SIZE, backgroundColor: PAPER };
    case 'bar': return { ...plain, ...common, ...extent, type: 'line', points: s.points, strokeColor: INK };
    case 'icon': return { ...plain, ...common, type: 'image', width: 16, height: 16, fileId: fileId(s.icon, s.color), status: 'saved', strokeColor: 'transparent' };
    default: return { ...common, type: 'text', text: s.text, fontSize: 14, fontFamily: 2, strokeColor: s.color || INK, roughness: 0, opacity: 100 };
  }
}

/** New Excalidraw shapes for `model` at (x, y), in `group`. For the library: nothing on a scene to match them to. */
export function makeShapes(model, x, y, group) {
  const list = shapes(model, x, y);
  const made = convertToExcalidrawElements(list.map((s) => describe(s, group)));
  made[0].customData = { ...made[0].customData, model };
  return made;
}

/** Everything of the group `group`, deleted shapes included. */
const partsOf = (api, group) => api.getSceneElementsIncludingDeleted().filter((e) => isTree(e) && groupOf(e) === group);

const frameIn = (parts) => parts.find((e) => e.customData.role === 'frame' && !e.isDeleted);

const bump = (e) => ({ version: e.version + 1, versionNonce: Math.floor(Math.random() * 2 ** 31) });

/** Coloured icons are made when they are first needed. */
function addColouredIcons(api, model) {
  const coloured = shapes(model, 0, 0).filter((s) => s.kind === 'icon' && s.color);
  if (coloured.length) api.addFiles(coloured.map((s) => iconFile(s.icon, s.color)));
}

/** Put a new tree on the scene at (x, y), as one undoable step. Returns its group. */
export function insert(api, model, x, y) {
  const group = newId();
  addColouredIcons(api, model);
  const made = makeShapes(model, x, y, group);
  api.updateScene({ elements: [...api.getSceneElementsIncludingDeleted(), ...made], captureUpdate: CaptureUpdateAction.IMMEDIATELY });
  api.scrollToContent(made, { animate: false });
  return group;
}

/** Draw `model` over the tree of `group`, as one undoable step. */
export function redraw(api, group, model) {
  const old = partsOf(api, group);
  const frame = frameIn(old);
  if (!frame) return;
  addColouredIcons(api, model);
  const byKey = new Map(old.map((e) => [e.customData.key, e]));
  const made = makeShapes(model, frame.x, frame.y, group).map((e) => {
    const was = byKey.get(e.customData.key);
    if (!was) return e;
    // The same part: a new look on the shape it already has. Its place in the frame comes from the model.
    return { ...e, id: was.id, groupIds: was.groupIds, seed: was.seed, ...bump(was), isDeleted: false };
  });
  const keep = new Set(made.map((e) => e.id));
  const gone = old.filter((e) => !keep.has(e.id) && !e.isDeleted).map((e) => ({ ...e, ...bump(e), isDeleted: true }));
  const others = api.getSceneElementsIncludingDeleted().filter((e) => !keep.has(e.id) && !gone.some((g) => g.id === e.id));
  api.updateScene({ elements: [...others, ...made, ...gone], captureUpdate: CaptureUpdateAction.IMMEDIATELY });
}

/** The data of the tree of `group`, to change and hand back to `redraw`. */
export function modelOf(api, group) {
  const frame = frameIn(partsOf(api, group));
  return frame ? clone(frame.customData.model) : null;
}

/** The tree shape under a mouse event: a fold box before a row, since a row's rectangle does not cover it. */
function hit(api, ev) {
  const p = viewportCoordsToSceneCoords({ clientX: ev.clientX, clientY: ev.clientY }, api.getAppState());
  const near = api.getSceneElements().filter(isTree).filter((e) => {
    const slop = e.customData.role === 'fold' ? FOLD_SLOP : 0;
    return p.x >= e.x - slop && p.x <= e.x + e.width + slop && p.y >= e.y - slop && p.y <= e.y + e.height + slop;
  });
  return near.find((e) => e.customData.role === 'fold') || near.find((e) => e.customData.role === 'row') || null;
}

const onCanvas = (ev) => ev.target?.tagName === 'CANVAS';
const editing = (api) => {
  const s = api.getAppState();
  return s.viewModeEnabled || s.activeTool?.type !== 'selection' || s.editingTextElement;
};

/** Stop Excalidraw seeing this event, and give it the keyboard, which it would have taken from the click. */
function keep(ev) {
  ev.preventDefault();
  ev.stopPropagation();
  document.querySelector('.excalidraw-container')?.focus();
}

/** Listen for the mouse on trees. `onFold(group, node)`, `onMenu(group, node, x, y)`, `onRename` likewise. Returns the way to stop. */
export function listen(api, { onFold, onMenu, onRename }) {
  const down = (ev) => {
    if (ev.button !== 0 || ev.shiftKey || ev.ctrlKey || !onCanvas(ev) || editing(api)) return;
    const e = hit(api, ev);
    if (e?.customData.role === 'fold') {
      keep(ev);
      onFold(groupOf(e), e.customData.node);
    }
  };
  const context = (ev) => {
    if (!onCanvas(ev) || editing(api)) return;
    const e = hit(api, ev);
    if (e?.customData.role === 'row') {
      keep(ev);
      ev.stopImmediatePropagation();
      onMenu(groupOf(e), e.customData.node, ev.clientX, ev.clientY);
    }
  };
  const double = (ev) => {
    if (!onCanvas(ev) || editing(api)) return;
    const e = hit(api, ev);
    if (e?.customData.role === 'row') {
      keep(ev);
      onRename(groupOf(e), e.customData.node, ev.clientX, ev.clientY);
    }
  };
  window.addEventListener('pointerdown', down, true);
  window.addEventListener('contextmenu', context, true);
  window.addEventListener('dblclick', double, true);
  return () => {
    window.removeEventListener('pointerdown', down, true);
    window.removeEventListener('contextmenu', context, true);
    window.removeEventListener('dblclick', double, true);
  };
}

/** The icons have to be in every page: a scene or a library item only names them. */
export const addIcons = (api) => api.addFiles(iconFiles());
