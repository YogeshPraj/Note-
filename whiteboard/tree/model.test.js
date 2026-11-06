import assert from 'node:assert/strict';
import { test } from 'node:test';

import * as m from './model.js';

const sample = () => m.node('Peridot Agent', [m.node('Instruction'), m.node('Tools', [m.node('KA Answers')]), m.node('Skills')], 'robot');
const titled = (tree, title) => {
  let hit = null;
  const walk = (n) => { if (n.title === title) hit = n; n.children.forEach(walk); };
  walk(tree);
  return hit;
};
const titles = (tree) => m.rows(tree).map((r) => r.node.title);

test('a title suggests its icon, the longer word first', () => {
  assert.equal(m.iconFor('Peridot Agent'), 'robot');
  assert.equal(m.iconFor('Sub-agent'), 'hierarchy');
  assert.equal(m.iconFor('Tools'), 'tools');
  assert.equal(m.iconFor('Search tool'), 'tool');
  assert.equal(m.iconFor('Session Memory'), 'database');
  assert.equal(m.iconFor('Something else'), 'point');
});

test('the agent comes with an instruction, a tool and a sample skill', () => {
  const a = m.agentTemplate();
  assert.equal(a.icon, 'robot');
  assert.deepEqual(a.children.map((c) => c.title), ['Instruction', 'Tools', 'Skills']);
  assert.equal(titled(a, 'Skill name').icon, 'bolt');
  assert.equal(titled(a, 'Tool name').icon, 'tool');
});

test('a child goes last under its parent and opens the parent', () => {
  const t = sample();
  const tools = titled(t, 'Tools');
  tools.open = false;
  const n = m.addChild(t, tools.id, 'New tool');
  assert.equal(tools.open, true);
  assert.deepEqual(tools.children.map((c) => c.title), ['KA Answers', 'New tool']);
  assert.equal(n.icon, 'tool');
  assert.equal(m.addChild(t, 'nobody'), null);
});

test('a sibling goes right after the node, and the root has none', () => {
  const t = sample();
  m.addSibling(t, titled(t, 'Instruction').id, 'Memory');
  assert.deepEqual(t.children.map((c) => c.title), ['Instruction', 'Memory', 'Tools', 'Skills']);
  assert.equal(m.addSibling(t, t.id), null);
});

test('renaming refuses an empty name and gives a dot its icon', () => {
  const t = sample();
  const n = m.addChild(t, t.id);
  assert.equal(n.icon, 'point');
  assert.equal(m.rename(t, n.id, '   '), false);
  assert.equal(n.title, 'New item');
  assert.equal(m.rename(t, n.id, ' Guardrail '), true);
  assert.equal(n.title, 'Guardrail');
  assert.equal(n.icon, 'shield-check');
});

test('renaming keeps an icon that was chosen', () => {
  const t = sample();
  const n = titled(t, 'Instruction');
  m.setIcon(t, n.id, 'brain');
  m.rename(t, n.id, 'Memory');
  assert.equal(n.icon, 'brain');
});

test('deleting takes the branch with it, and the root stays', () => {
  const t = sample();
  assert.equal(m.remove(t, titled(t, 'Tools').id), true);
  assert.equal(titled(t, 'KA Answers'), null);
  assert.equal(m.remove(t, t.id), false);
});

test('moving stays among the siblings', () => {
  const t = sample();
  const first = titled(t, 'Instruction');
  assert.equal(m.canMove(t, first.id, -1), false);
  assert.equal(m.move(t, first.id, 1), true);
  assert.deepEqual(t.children.map((c) => c.title), ['Tools', 'Instruction', 'Skills']);
  const last = titled(t, 'Skills');
  assert.equal(m.canMove(t, last.id, 1), false);
  assert.equal(m.canMove(t, t.id, 1), false);
});

test('a folded branch has no rows and a leaf cannot fold', () => {
  const t = sample();
  const tools = titled(t, 'Tools');
  assert.equal(m.toggle(t, tools.id), true);
  assert.deepEqual(titles(t), ['Peridot Agent', 'Instruction', 'Tools', 'Skills']);
  assert.equal(m.toggle(t, tools.id), true);
  assert.equal(titles(t).includes('KA Answers'), true);
  assert.equal(m.toggle(t, titled(t, 'Skills').id), false);
});

test('rows step down by one and in by their depth', () => {
  const t = sample();
  const r = m.rows(t, 100, 50);
  assert.equal(r[1].y - r[0].y, m.ROW);
  assert.equal(r[3].x - r[2].x, m.INDENT);
  assert.equal(r[1].parent, t);
});

test('the frame grows with the rows and the longest title', () => {
  const t = sample();
  const before = m.size(t);
  m.addChild(t, t.id, 'x'.repeat(60));
  const after = m.size(t);
  assert.equal(after.height - before.height, m.ROW);
  assert.ok(after.width > before.width);
  assert.ok(m.size(m.node('a')).width >= m.MIN_WIDTH);
});

test('every part of a tree has its own key, the same from one drawing to the next', () => {
  const t = sample();
  const keys = m.shapes(t, 0, 0).map((s) => s.key);
  assert.equal(new Set(keys).size, keys.length);
  assert.deepEqual(m.shapes(t, 40, 90).map((s) => s.key), keys);
});

test('a fold box shows minus when open and plus when folded', () => {
  const t = sample();
  const tools = titled(t, 'Tools');
  const has = (key) => m.shapes(t, 0, 0).some((s) => s.key === key);
  assert.equal(has(`minus:${tools.id}`), true);
  assert.equal(has(`plus:${tools.id}`), false);
  m.toggle(t, tools.id);
  assert.equal(has(`plus:${tools.id}`), true);
  assert.equal(has(`box:${titled(t, 'Instruction').id}`), false, 'a leaf has no box');
});

test('only the box of a fold can be clicked, and lines are as long as they are drawn', () => {
  const t = sample();
  const parts = m.shapes(t, 0, 0);
  assert.equal(parts.filter((s) => s.role === 'fold').every((s) => s.kind === 'box'), true);
  assert.equal(parts.filter((s) => s.kind === 'bar').every((s) => s.role === 'mark'), true);
});

test('a tree survives being saved', () => {
  const t = sample();
  assert.deepEqual(m.clone(t), t);
  assert.notEqual(m.clone(t).children, t.children);
});
