// The tree on the whiteboard, as data: nodes, what can be done to them, and where each row goes. No Excalidraw and no DOM
// in here, so it can be tested with `node --test`. `scene.js` turns it into shapes.
//
// A node is { id, title, icon, open, children }, with a `color` once one has been chosen. The whole tree is kept in the frame's customData, so it is saved in the
// .excalidraw file, undo brings it back with the shapes, and a copied tree carries its own.

export const ROW = 22;
export const INDENT = 20;
export const PAD = 10;
export const MIN_WIDTH = 200;
export const FONT = 14;
/** Rough width of a character of the row font: the frame only has to clear the longest title. */
const CHAR = 7.2;
/** From the left edge of a row to its title: the fold box, then the icon. */
const TITLE_AT = 30;

let counter = 0;
export const newId = () => `n${Date.now().toString(36)}${(counter += 1).toString(36)}`;

// Longest match first: "tools" before "tool", "sub-agent" before "agent".
const NAMED = [
  ['orchestr', 'sitemap'], ['sub-agent', 'hierarchy'], ['subagent', 'hierarchy'], ['handoff', 'route'], ['agent', 'robot'],
  ['tools', 'tools'], ['tool', 'tool'], ['skill', 'bolt'], ['instruction', 'file-text'], ['prompt', 'message-chatbot'],
  ['llm', 'brain'], ['model', 'brain'], ['memory', 'database'], ['knowledge', 'books'], ['search', 'world'], ['mcp', 'plug-connected'],
  ['api', 'api'], ['human', 'user-check'], ['guardrail', 'shield-check'], ['safety', 'shield-check'], ['evaluat', 'checklist'],
  ['trigger', 'player-play'],
];

/** The icon a title suggests; a plain dot when it suggests none. */
export function iconFor(title) {
  const t = title.toLowerCase();
  const hit = NAMED.find(([word]) => t.includes(word));
  return hit ? hit[1] : 'point';
}

export function node(title, children = [], icon = iconFor(title)) {
  return { id: newId(), title, icon, open: true, children };
}

export const treeTemplate = () => node('Root', [node('Item', [node('Child item')]), node('Item')], 'point');

export const agentTemplate = () =>
  node('Agent name', [
    node('Instruction'),
    node('Tools', [node('Tool name', [], 'tool')]),
    node('Skills', [node('Skill name', [node('Instruction'), node('Tools', [node('Tool name', [], 'tool')])], 'bolt')]),
  ], 'robot');

export const clone = (tree) => JSON.parse(JSON.stringify(tree));

/** `id` in the tree: the node, its parent and its place among the parent's children. */
export function find(tree, id, parent = null, index = 0) {
  if (tree.id === id) return { node: tree, parent, index };
  for (let i = 0; i < tree.children.length; i += 1) {
    const hit = find(tree.children[i], id, tree, i);
    if (hit) return hit;
  }
  return null;
}

/** Add a child to `id`, which opens so it can be seen. Returns the new node. */
export function addChild(tree, id, title = 'New item') {
  const at = find(tree, id);
  if (!at) return null;
  const n = node(title);
  at.node.open = true;
  at.node.children.push(n);
  return n;
}

/** Add a node after `id`. The root has no siblings: returns null. */
export function addSibling(tree, id, title = 'New item') {
  const at = find(tree, id);
  if (!at || !at.parent) return null;
  const n = node(title);
  at.parent.children.splice(at.index + 1, 0, n);
  return n;
}

/** Name a node. A node that still has the dot takes the icon its new name suggests. Empty names are refused. */
export function rename(tree, id, title) {
  const at = find(tree, id);
  const name = title.trim();
  if (!at || !name) return false;
  at.node.title = name;
  if (at.node.icon === 'point') at.node.icon = iconFor(name);
  return true;
}

/** Delete `id` and everything under it. The root stays. */
export function remove(tree, id) {
  const at = find(tree, id);
  if (!at || !at.parent) return false;
  at.parent.children.splice(at.index, 1);
  return true;
}

/** Can `id` move by `delta` (-1 up, 1 down) among its siblings? */
export function canMove(tree, id, delta) {
  const at = find(tree, id);
  if (!at || !at.parent) return false;
  const to = at.index + delta;
  return to >= 0 && to < at.parent.children.length;
}

export function move(tree, id, delta) {
  if (!canMove(tree, id, delta)) return false;
  const at = find(tree, id);
  const [n] = at.parent.children.splice(at.index, 1);
  at.parent.children.splice(at.index + delta, 0, n);
  return true;
}

export function toggle(tree, id) {
  const at = find(tree, id);
  if (!at || !at.node.children.length) return false;
  at.node.open = !at.node.open;
  return true;
}

export function setIcon(tree, id, icon) {
  const at = find(tree, id);
  if (!at) return false;
  at.node.icon = icon;
  return true;
}

/** Colour a node's text and icon. `null` goes back to the plain ink. */
export function setColor(tree, id, color) {
  const at = find(tree, id);
  if (!at) return false;
  if (color) at.node.color = color;
  else delete at.node.color;
  return true;
}

/** The rows to draw, top to bottom: folded branches have none. `x` and `y` are the left and middle of the row. */
export function rows(tree, x = 0, y = 0) {
  const out = [];
  const walk = (n, depth, parent) => {
    out.push({ node: n, depth, parent, x: x + PAD + 6 + depth * INDENT, y: y + PAD + ROW / 2 + out.length * ROW });
    if (n.open) n.children.forEach((c) => walk(c, depth + 1, n));
  };
  walk(tree, 0, null);
  return out;
}

/** The frame around `tree`. */
export function size(tree) {
  const list = rows(tree);
  const right = Math.max(...list.map((r) => r.x + TITLE_AT + r.node.title.length * CHAR));
  return { width: Math.max(MIN_WIDTH, Math.ceil(right + PAD + 6)), height: list.length * ROW + PAD * 2 };
}

/**
 * What to draw for `tree` with its frame's corner at (x, y): plain descriptions, each with a `key` that names its part
 * for as long as the node lives, so the same part stays the same shape from one redraw to the next.
 */
export function shapes(tree, x, y) {
  const list = rows(tree, x, y);
  const at = new Map(list.map((r) => [r.node.id, r]));
  const { width, height } = size(tree);
  const out = [{ key: 'frame', role: 'frame', kind: 'frame', x, y, width, height }];
  list.forEach((r) => {
    const id = r.node.id;
    if (r.parent) {
      const p = at.get(r.parent.id);
      const hasBox = r.node.children.length > 0;
      out.push({ key: `line:${id}`, role: 'line', kind: 'line', x: p.x, y: p.y + 5, points: [[0, 0], [0, r.y - p.y - 5], [r.x - p.x - (hasBox ? 5 : -7), r.y - p.y - 5]] });
    }
    out.push({ key: `icon:${id}`, role: 'row', node: id, kind: 'icon', x: r.x + 9, y: r.y - 8, icon: r.node.icon, color: r.node.color });
    out.push({ key: `text:${id}`, role: 'row', node: id, kind: 'text', x: r.x + TITLE_AT, y: r.y - FONT * 0.62, text: r.node.title, color: r.node.color });
    if (r.node.children.length) {
      out.push({ key: `box:${id}`, role: 'fold', node: id, kind: 'box', x: r.x - 5, y: r.y - 5 });
      out.push({ key: `minus:${id}`, role: 'mark', kind: 'bar', x: r.x - 3, y: r.y, points: [[0, 0], [6, 0]] });
      if (!r.node.open) out.push({ key: `plus:${id}`, role: 'mark', kind: 'bar', x: r.x, y: r.y - 3, points: [[0, 0], [0, 6]] });
    }
  });
  return out;
}
