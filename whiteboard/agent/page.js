// An AI agent's requests, carried out on the real scene. Each change is one undoable step, like a stroke of the user's.
// Excalidraw's own converter makes the shapes, so they measure text, hold labels and join arrows as drawn ones do.

import { CaptureUpdateAction, convertToExcalidrawElements, exportToBlob } from '@excalidraw/excalidraw';

import { parseMermaidToExcalidraw } from '@excalidraw/mermaid-to-excalidraw';

import { ICONS } from '../tree/icons.js';
import { insert, modelOf, redraw } from '../tree/scene.js';
import { plan } from './draw.js';
import { borrowedCopy, describeScene, fromOutline, isContainer, isTreePart, nextPlace, patched, removal, route, skeletons } from './shapes.js';

/** The longest side of the picture an agent gets: large enough to read, small enough to send. */
const IMAGE_SIZE = 1600;
const FONT_FAMILY = 2;
const MAX_MERMAID_ELEMENTS = 300;
const ICON_NAMES = ICONS.map(([name]) => name);

const fail = (message) => { throw new Error(message); };
const convert = (list) => convertToExcalidrawElements(list, { regenerateIds: false });
const bump = (e) => ({ version: e.version + 1, versionNonce: Math.floor(Math.random() * 2 ** 31), updated: Date.now() });
const commit = (api, elements) => api.updateScene({ elements, captureUpdate: CaptureUpdateAction.IMMEDIATELY });
const everything = (api) => api.getSceneElementsIncludingDeleted();

/** Replace elements by id, and add those that are new. */
function merged(all, changed) {
  const byId = new Map(changed.map((e) => [e.id, e]));
  const kept = all.map((e) => byId.get(e.id) ?? e);
  const known = new Set(all.map((e) => e.id));
  return [...kept, ...changed.filter((e) => !known.has(e.id))];
}

/** The new shapes added to `all`, with the shapes on the board that their arrows join noted as joined. */
function attached(all, list, borrowed) {
  const copies = borrowed.map((id) => borrowedCopy(all.find((e) => e.id === id)));
  const made = convert([...copies, ...list]);
  const fresh = made.filter((e) => !borrowed.includes(e.id));
  // The converter noted the arrows on its copies of the shapes on the board: the real shapes need that too.
  const joined = made.filter((e) => borrowed.includes(e.id)).map((copy) => {
    const real = all.find((e) => e.id === copy.id);
    const arrows = (copy.boundElements || []).filter((b) => b.type === 'arrow');
    return { ...real, boundElements: [...(real.boundElements || []), ...arrows], ...bump(real) };
  });
  return { elements: [...merged(all, joined), ...fresh], fresh };
}

const described = (fresh) => ({ shapes: fresh.filter((e) => !e.containerId).map((e) => ({ id: e.id, type: e.type })) });

function addShapes(api, args) {
  const all = everything(api);
  const { list, borrowed } = skeletons(args.shapes, all);
  const { elements, fresh } = attached(all, list, borrowed);
  commit(api, elements);
  api.scrollToContent(fresh, { animate: false });
  return described(fresh);
}

/** Excalidraw MCP's `create_view`, on the open whiteboard: its element format, with `delete` and `cameraUpdate` entries. */
function drawElements(api, args) {
  const all = everything(api);
  const { list, borrowed, remove, camera } = plan(args.elements, all);
  let base = all;
  if (remove.length) {
    const { gone, changed } = removal(all, remove);
    const out = gone.map((id) => all.find((e) => e.id === id)).map((e) => ({ ...e, isDeleted: true, ...bump(e) }));
    const kept = changed.map(({ id, patch }) => ({ ...all.find((e) => e.id === id), ...patch, ...bump(all.find((e) => e.id === id)) }));
    base = merged(all, [...out, ...kept]);
  }
  const { elements, fresh } = list.length ? attached(base, list, borrowed) : { elements: base, fresh: [] };
  api.updateScene({ elements, captureUpdate: CaptureUpdateAction.IMMEDIATELY });
  if (camera) {
    const { width, height } = api.getAppState();
    const zoom = Math.min(width / camera.width, height / camera.height);
    api.updateScene({
      appState: { zoom: { value: zoom }, scrollX: -camera.x + (width / zoom - camera.width) / 2, scrollY: -camera.y + (height / zoom - camera.height) / 2 },
      captureUpdate: CaptureUpdateAction.NEVER,
    });
  } else if (fresh.length) {
    api.scrollToContent(fresh, { animate: false });
  }
  return { ...described(fresh), removed: remove.length };
}

const shownShape = (all, id) => all.find((e) => e.id === id && !e.isDeleted) ?? fail(`No shape has the id "${id}".`);

/** The arrow redrawn to the shapes it is joined to, given where they are now, and its label with it. An end that is joined to nothing stays. */
function rerouted(arrow, all, moved) {
  const shape = (b) => b && (moved.find((e) => e.id === b.elementId) ?? all.find((e) => e.id === b.elementId));
  const last = arrow.points[arrow.points.length - 1];
  const from = shape(arrow.startBinding) ?? { x: arrow.x, y: arrow.y };
  const to = shape(arrow.endBinding) ?? { x: arrow.x + last[0], y: arrow.y + last[1] };
  const line = { ...arrow, ...route(from, to), ...bump(arrow) };
  const label = all.find((e) => e.containerId === arrow.id && !e.isDeleted);
  if (!label) return [line];
  const [mx, my] = [line.x + line.points[1][0] / 2, line.y + line.points[1][1] / 2];
  return [line, { ...label, x: mx - label.width / 2, y: my - label.height / 2, ...bump(label) }];
}

/** A container with a new label, or its old label at the new size: the converter fits the two together. */
function relabelled(next, oldLabel, text) {
  const [box, label] = convert([{ type: next.type, id: next.id, x: next.x, y: next.y, width: next.width, height: next.height, label: { text, fontFamily: FONT_FAMILY } }]);
  const others = (next.boundElements || []).filter((b) => b.type !== 'text');
  const shape = { ...next, width: box.width, height: box.height, boundElements: [...others, { type: 'text', id: label.id }] };
  const colour = oldLabel ? { strokeColor: oldLabel.strokeColor } : {};
  return [shape, { ...label, ...colour }, ...(oldLabel ? [{ ...oldLabel, isDeleted: true, ...bump(oldLabel) }] : [])];
}

function updateShape(api, args) {
  const all = everything(api);
  const was = shownShape(all, args.id);
  if (isTreePart(was)) fail('This shape is part of a tree: use set_tree.');
  if (was.containerId) fail('This is the label of a shape: update the shape.');
  const fields = patched(args);
  if (!Object.keys(fields).length && typeof args.text !== 'string') fail('Nothing to change.');
  let next = { ...was, ...fields, ...bump(was) };
  let changed = [next];
  const oldLabel = all.find((e) => e.containerId === was.id && !e.isDeleted);
  if (was.type === 'text' && typeof args.text === 'string') {
    const [measured] = convert([{ type: 'text', x: next.x, y: next.y, text: args.text, fontSize: was.fontSize, fontFamily: was.fontFamily }]);
    next = { ...next, text: args.text, originalText: args.text, width: measured.width, height: measured.height };
    changed = [next];
  } else if (isContainer(was)) {
    const text = typeof args.text === 'string' ? args.text : oldLabel?.text;
    if (text) changed = relabelled(next, oldLabel, text);
    [next] = changed;
  }
  const arrows = (was.boundElements || []).filter((b) => b.type === 'arrow').map((b) => all.find((e) => e.id === b.id && !e.isDeleted)).filter(Boolean);
  const routes = arrows.flatMap((a) => rerouted(a, all, [next]));
  commit(api, merged(all, [...changed, ...routes]));
  return { id: next.id };
}

function deleteShapes(api, args) {
  const all = everything(api);
  const { gone, changed } = removal(all, args.ids);
  const out = gone.map((id) => all.find((e) => e.id === id)).map((e) => ({ ...e, isDeleted: true, ...bump(e) }));
  const kept = changed.map(({ id, patch }) => all.find((e) => e.id === id)).map((e, i) => ({ ...e, ...changed[i].patch, ...bump(e) }));
  commit(api, merged(all, [...out, ...kept]));
  return { deleted: args.ids.length };
}

function addTree(api, args) {
  const model = fromOutline(args.tree, ICON_NAMES);
  const place = nextPlace(api.getSceneElements());
  return { id: insert(api, model, args.x ?? place.x, args.y ?? place.y) };
}

function setTree(api, args) {
  if (!modelOf(api, args.id)) fail(`No tree has the id "${args.id}".`);
  redraw(api, args.id, fromOutline(args.tree, ICON_NAMES));
  return { id: args.id };
}

/** A flowchart from Mermaid text, drawn offline as ordinary shapes and arrows the user can edit. */
async function addMermaid(api, args) {
  const source = String(args.mermaid ?? '').trim();
  if (!source) fail('"mermaid" is empty: give the diagram\'s Mermaid text.');
  let parsed;
  try {
    parsed = await parseMermaidToExcalidraw(source, { themeVariables: { fontSize: '20px' } });
  } catch (e) {
    fail(`Mermaid could not read the diagram: ${e?.message ?? e}`);
  }
  if (!parsed.elements.length) fail('The diagram has nothing to draw. Flowcharts ("flowchart TD" or "graph LR") are supported.');
  if (parsed.elements.length > MAX_MERMAID_ELEMENTS) fail(`The diagram is too big: up to ${MAX_MERMAID_ELEMENTS} parts.`);
  const made = convertToExcalidrawElements(parsed.elements, { regenerateIds: true });
  const all = everything(api);
  const place = nextPlace(api.getSceneElements());
  const dx = (args.x ?? place.x) - Math.min(...made.map((e) => e.x));
  const dy = (args.y ?? place.y) - Math.min(...made.map((e) => e.y));
  const fresh = made.map((e) => ({ ...e, x: e.x + dx, y: e.y + dy }));
  const files = Object.values(parsed.files ?? {});
  if (files.length) api.addFiles(files);
  commit(api, [...all, ...fresh]);
  api.scrollToContent(fresh, { animate: false });
  return { shapes: fresh.filter((e) => !e.containerId).map((e) => ({ id: e.id, type: e.type })) };
}
async function picture(api) {
  const elements = api.getSceneElements();
  if (!elements.length) fail('The whiteboard is empty.');
  const blob = await exportToBlob({
    elements,
    files: api.getFiles(),
    mimeType: 'image/png',
    exportPadding: 20,
    maxWidthOrHeight: IMAGE_SIZE,
    appState: { ...api.getAppState(), exportBackground: true, exportWithDarkMode: false },
  });
  const url = await new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = () => reject(new Error('The picture could not be made.'));
    reader.readAsDataURL(blob);
  });
  return { image_png: String(url).split(',')[1] };
}

const OPS = {
  read_whiteboard: (api) => ({ shapes: describeScene(api.getSceneElements()) }),
  whiteboard_image: picture,
  add_shapes: addShapes,
  draw_elements: drawElements,
  update_shape: updateShape,
  delete_shapes: deleteShapes,
  add_mermaid: addMermaid,
  add_tree: addTree,
  set_tree: setTree,
  // Only so that the app gets the latest scene: saving needs it.
  sync: () => ({}),
};

/** Carry out one request. Rejects with what to tell the agent. */
export async function runOp(api, op) {
  const run = OPS[op?.tool] ?? fail('Unknown request.');
  return run(api, op.args ?? {});
}
