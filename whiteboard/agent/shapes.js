// What an AI agent asks for on the whiteboard, as data: its shapes turned into Excalidraw's descriptions, the scene turned
// into something short to read, and what goes with removing a shape. No Excalidraw and no DOM in here, so it can be tested
// with `node --test`. `page.js` does the rest with the real scene.

import { iconFor, newId } from '../tree/model.js';

export const KINDS = ['rectangle', 'ellipse', 'diamond', 'text', 'arrow', 'line'];
const CONTAINERS = ['rectangle', 'ellipse', 'diamond'];
/** The most shapes one call can add: enough for any diagram, and a limit on what a mistake can do. */
export const MAX_SHAPES = 300;
const GAP = 40;
const SIZE = { width: 160, height: 80 };
/** Helvetica: what a new whiteboard uses (see PROFESSIONAL_DEFAULTS in app.jsx). */
const FONT_FAMILY = 2;
const COLOUR = /^(#[0-9a-f]{3,8}|transparent)$/i;
/** How far an arrow stops short of a shape, as a drawn one does. */
const ARROW_GAP = 6;
/** The size assumed for a text before it has been measured, and how long an arrow with one free end is. */
const TEXT_SIZE = { width: 100, height: 30 };
const FREE_LENGTH = 200;

const fail = (message) => { throw new Error(message); };
const live = (elements) => elements.filter((e) => !e.isDeleted);
const isNumber = (n) => typeof n === 'number' && Number.isFinite(n);

function colour(value, name) {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'string' || !COLOUR.test(value)) fail(`"${name}" must be a colour like #1e1e1e, or transparent.`);
  return value;
}

function number(value, name) {
  if (value === undefined || value === null) return undefined;
  if (!isNumber(value)) fail(`"${name}" must be a number.`);
  return value;
}

/** Excalidraw takes an undefined field for a value: only what was given goes in. */
const given = (fields) => Object.fromEntries(Object.entries(fields).filter(([, v]) => v !== undefined));

const centre = (s) => ({ x: s.x + (s.width || 0) / 2, y: s.y + (s.height || 0) / 2 });

/** How far along (dx, dy) from its centre a shape's outline is, as a share of that vector. */
function outlineAt(s, dx, dy) {
  const [rx, ry] = [(s.width || 0) / 2, (s.height || 0) / 2];
  if (!rx || !ry) return 0;
  const [px, py] = [Math.abs(dx) / rx, Math.abs(dy) / ry];
  if (s.type === 'ellipse') return 1 / Math.hypot(px, py);
  if (s.type === 'diamond') return 1 / (px + py);
  return 1 / Math.max(px, py);
}

/**
 * The straight path of an arrow between two shapes, or points given as shapes of no size: from outline to outline along
 * the line between their centres. Excalidraw's converter joins an arrow to its shapes but leaves it where it was put.
 */
export function route(from, to) {
  const [a, b] = [centre(from), centre(to)];
  const [dx, dy] = [b.x - a.x, b.y - a.y];
  const length = Math.hypot(dx, dy) || 1;
  const margin = (s) => (s.width && s.height ? ARROW_GAP / length : 0);
  const near = outlineAt(from, dx, dy) + margin(from);
  const far = 1 - outlineAt(to, dx, dy) - margin(to);
  const x = a.x + dx * near;
  const y = a.y + dy * near;
  const [w, h] = [dx * (far - near), dy * (far - near)];
  return { x, y, width: Math.abs(w), height: Math.abs(h), points: [[0, 0], [w, h]] };
}

/** Where shapes with no position go: below everything on the board. */
export function nextPlace(elements) {
  const shown = live(elements);
  if (!shown.length) return { x: 0, y: 0 };
  return { x: Math.min(...shown.map((e) => e.x)), y: Math.max(...shown.map((e) => e.y + (e.height || 0))) + GAP };
}

function style(s) {
  return given({ strokeColor: colour(s.stroke, 'stroke'), backgroundColor: colour(s.background, 'background'), roughness: 0, strokeWidth: 2 });
}

function one(s, place) {
  if (!s || !KINDS.includes(s.type)) fail(`"type" must be one of ${KINDS.join(', ')}.`);
  const joined = s.type === 'arrow' || s.type === 'line';
  const width = number(s.width, 'width') ?? (s.type === 'text' ? undefined : SIZE.width);
  const height = number(s.height, 'height') ?? (s.type === 'text' ? undefined : joined ? 0 : SIZE.height);
  const x = number(s.x, 'x') ?? place.x;
  const y = number(s.y, 'y') ?? place.y;
  if (number(s.x, 'x') === undefined && number(s.y, 'y') === undefined) place.x += (width ?? SIZE.width) + GAP;
  const base = given({ id: s.id, type: s.type, x, y, width, height, ...style(s) });
  if (s.text !== undefined && s.text !== null && typeof s.text !== 'string') fail('"text" must be text.');
  if (s.type === 'text') {
    if (!s.text) fail('A text shape needs "text".');
    return { ...base, text: s.text, fontFamily: FONT_FAMILY, ...given({ fontSize: number(s.font_size, 'font_size') }) };
  }
  if (s.text && s.type === 'line') fail('A line cannot hold text. Add a text shape beside it.');
  const label = s.text ? { label: { text: s.text, fontFamily: FONT_FAMILY } } : {};
  if (s.type === 'arrow') {
    const ends = given({ start: s.from ? { id: s.from } : undefined, end: s.to ? { id: s.to } : undefined });
    return { ...base, ...label, ...ends };
  }
  return { ...base, ...label, ...(CONTAINERS.includes(s.type) ? { roundness: { type: 3 } } : {}) };
}

/**
 * Excalidraw's descriptions for what an agent asked for, and the shapes already on the board its arrows connect to: those
 * have to be handed to the converter too, which only joins what it is given.
 */
export function skeletons(shapes, elements) {
  if (!Array.isArray(shapes) || !shapes.length) fail('"shapes" must list at least one shape.');
  if (shapes.length > MAX_SHAPES) fail(`One call can add at most ${MAX_SHAPES} shapes.`);
  const onBoard = new Map(live(elements).map((e) => [e.id, e]));
  const own = new Set();
  for (const s of shapes) {
    if (s?.id && (onBoard.has(s.id) || own.has(s.id))) fail(`The id "${s.id}" is already in use.`);
    if (s?.id) own.add(s.id);
  }
  const place = nextPlace(elements);
  const list = shapes.map((s) => one(s, place));
  const borrowed = new Set();
  for (const s of shapes) {
    for (const end of [s.from, s.to].filter(Boolean)) {
      if (s.type !== 'arrow') fail('Only an arrow connects shapes.');
      if (own.has(end)) continue;
      const target = onBoard.get(end);
      if (!target) fail(`No shape has the id "${end}".`);
      if (target.type === 'arrow' || target.type === 'line') fail('An arrow connects shapes, not other arrows.');
      borrowed.add(end);
    }
  }
  const sized = (e) => ({ ...TEXT_SIZE, ...e });
  const known = new Map([...onBoard, ...list.filter((e) => e.id).map((e) => [e.id, sized(e)])]);
  return { list: list.map((e, i) => (e.type === 'arrow' ? { ...e, ...arrowPath(shapes[i], known) } : e)), borrowed: [...borrowed] };
}

/** An arrow's path: between its two shapes, or from the one it has to where it was put. No shape, no change. */
function arrowPath(s, known) {
  const [from, to] = [known.get(s.from), known.get(s.to)];
  if (!from && !to) return {};
  const shape = from ?? to;
  const given = { x: s.x ?? centre(shape).x + FREE_LENGTH, y: s.y ?? centre(shape).y };
  return route(from ?? given, to ?? given);
}

/** A shape on the board, as the converter needs to see it to connect an arrow to it. */
export function borrowedCopy(e) {
  const type = CONTAINERS.includes(e.type) || e.type === 'text' ? e.type : 'rectangle';
  return { type, id: e.id, x: e.x, y: e.y, width: e.width, height: e.height, ...(type === 'text' ? { text: e.text } : {}) };
}

/** The fields of a shape that `update_shape` sets directly; the text goes through the editor's measuring. */
export function patched(args) {
  return given({
    x: number(args.x, 'x'),
    y: number(args.y, 'y'),
    width: number(args.width, 'width'),
    height: number(args.height, 'height'),
    strokeColor: colour(args.stroke, 'stroke'),
    backgroundColor: colour(args.background, 'background'),
  });
}

export const isTreePart = (e) => Boolean(e.customData?.notepp_tree);
export const groupOf = (e) => e.groupIds?.[e.groupIds.length - 1];
export const isContainer = (e) => CONTAINERS.includes(e.type);

/** A tree as an agent reads and writes it. */
export function outline(n) {
  return { title: n.title, icon: n.icon, ...(n.color ? { color: n.color } : {}), ...(n.open ? {} : { open: false }), children: n.children.map(outline) };
}

/** The tree's data from what an agent wrote. An icon that is not known is chosen from the title. */
export function fromOutline(spec, icons, depth = 0) {
  if (!spec || typeof spec.title !== 'string' || !spec.title) fail('Every node of a tree needs a "title".');
  if (depth > 32) fail('The tree is too deep.');
  if (spec.children !== undefined && !Array.isArray(spec.children)) fail('"children" must be a list.');
  return {
    id: newId(),
    title: spec.title,
    icon: icons.includes(spec.icon) ? spec.icon : iconFor(spec.title),
    open: spec.open !== false,
    ...(spec.color ? { color: colour(spec.color, 'color') } : {}),
    children: (spec.children || []).map((c) => fromOutline(c, icons, depth + 1)),
  };
}

/** A text as it was written: `text` has the line breaks of its wrapping. */
const written = (e) => e.originalText || e.text;

function readOne(e, labels) {
  return given({
    id: e.id,
    type: e.type,
    x: Math.round(e.x),
    y: Math.round(e.y),
    width: Math.round(e.width),
    height: Math.round(e.height),
    text: e.type === 'text' ? written(e) : labels.get(e.id),
    stroke: e.strokeColor,
    background: e.backgroundColor === 'transparent' ? undefined : e.backgroundColor,
    from: e.startBinding?.elementId,
    to: e.endBinding?.elementId,
  });
}

/** The board in few words: a label belongs to its shape, and a tree is one thing however many shapes draw it. */
export function describeScene(elements) {
  const shown = live(elements);
  const labels = new Map(shown.filter((e) => e.containerId).map((e) => [e.containerId, written(e)]));
  const out = [];
  for (const e of shown) {
    if (isTreePart(e)) {
      if (e.customData.role !== 'frame') continue;
      out.push({ id: groupOf(e), type: 'tree', x: Math.round(e.x), y: Math.round(e.y), width: Math.round(e.width), height: Math.round(e.height), tree: outline(e.customData.model) });
    } else if (!e.containerId) {
      out.push(readOne(e, labels));
    }
  }
  return out;
}

/**
 * What removing `ids` means for the scene: the shapes, their labels, and every part of a tree named by its id. Arrows that
 * were joined to them stay, let go at that end, and arrows that go leave the shapes they were joined to.
 */
export function removal(elements, ids) {
  if (!Array.isArray(ids) || !ids.length) fail('"ids" must list at least one shape.');
  const shown = live(elements);
  const named = new Set(ids);
  const known = new Set(shown.flatMap((e) => (isTreePart(e) ? [groupOf(e)] : [e.id])));
  const missing = ids.find((id) => !known.has(id));
  if (missing) fail(`No shape has the id "${missing}".`);
  const gone = new Set(shown.filter((e) => named.has(e.id) || named.has(e.containerId) || (isTreePart(e) && named.has(groupOf(e)))).map((e) => e.id));
  const changed = [];
  for (const e of shown.filter((x) => !gone.has(x.id))) {
    const patch = {};
    if (gone.has(e.startBinding?.elementId)) patch.startBinding = null;
    if (gone.has(e.endBinding?.elementId)) patch.endBinding = null;
    if (e.boundElements?.some((b) => gone.has(b.id))) patch.boundElements = e.boundElements.filter((b) => !gone.has(b.id));
    if (Object.keys(patch).length) changed.push({ id: e.id, patch });
  }
  return { gone: [...gone], changed };
}
