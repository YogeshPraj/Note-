import assert from 'node:assert/strict';
import { test } from 'node:test';

import * as s from './shapes.js';

const box = (id, x = 0, y = 0, more = {}) => ({ id, type: 'rectangle', x, y, width: 100, height: 50, strokeColor: '#000000', backgroundColor: 'transparent', ...more });

test('shapes become descriptions, with the look of a new whiteboard', () => {
  const { list } = s.skeletons([{ type: 'rectangle', id: 'a', x: 10, y: 20, width: 30, height: 40, text: 'Hi', stroke: '#ff0000', background: '#00ff00' }], []);
  const r = list[0];
  assert.deepEqual([r.id, r.x, r.y, r.width, r.height, r.strokeColor, r.backgroundColor, r.roughness], ['a', 10, 20, 30, 40, '#ff0000', '#00ff00', 0]);
  assert.equal(r.label.text, 'Hi');
});

test('fields that were not given stay out, since Excalidraw takes undefined for a value', () => {
  const { list } = s.skeletons([{ type: 'ellipse' }], []);
  assert.equal('id' in list[0], false);
  assert.equal('strokeColor' in list[0], false);
  assert.equal('backgroundColor' in list[0], false);
});

test('shapes with no position go below the board, side by side', () => {
  const { list } = s.skeletons([{ type: 'rectangle' }, { type: 'rectangle' }], [box('a', 5, 10)]);
  assert.deepEqual([list[0].x, list[0].y], [5, 100]);
  assert.equal(list[1].y, 100);
  assert.ok(list[1].x > list[0].x + list[0].width);
});

test('a deleted shape neither takes space nor holds its id', () => {
  const gone = box('a', 0, 1000, { isDeleted: true });
  const { list } = s.skeletons([{ type: 'rectangle', id: 'a' }], [gone]);
  assert.equal(list[0].y, 0);
});

test('an arrow connects shapes in the same call or on the board', () => {
  const { list, borrowed } = s.skeletons([{ type: 'rectangle', id: 'new' }, { type: 'arrow', from: 'new', to: 'old', text: 'calls' }], [box('old')]);
  assert.deepEqual(list[1].start, { id: 'new' });
  assert.deepEqual(list[1].end, { id: 'old' });
  assert.equal(list[1].label.text, 'calls');
  assert.deepEqual(borrowed, ['old']);
});

const near = (a, b) => assert.ok(Math.abs(a - b) < 0.01, `${a} is not ${b}`);

test('an arrow runs from outline to outline, stopping short of both', () => {
  const r = s.route({ type: 'rectangle', x: 0, y: 0, width: 100, height: 50 }, { type: 'rectangle', x: 300, y: 0, width: 100, height: 50 });
  near(r.x, 106);
  near(r.y, 25);
  near(r.width, 188);
  near(r.points[1][0], 188);
  near(r.points[1][1], 0);
});

test('an arrow meets the outline of an ellipse and of a diamond, not their boxes', () => {
  const to = (type) => s.route({ x: 0, y: 0, width: 0, height: 0 }, { type, x: 100, y: 100, width: 100, height: 100 });
  const reach = (r) => Math.hypot(r.points[1][0], r.points[1][1]);
  const [box, ellipse, diamond] = [to('rectangle'), to('ellipse'), to('diamond')];
  assert.ok(reach(box) < reach(ellipse) && reach(ellipse) < reach(diamond), 'the corner of a box comes first, then a curve, then a slope');
});

test('an arrow going up or left keeps its direction, and its size stays positive', () => {
  const r = s.route({ x: 500, y: 500, width: 100, height: 100, type: 'rectangle' }, { x: 0, y: 0, width: 100, height: 100, type: 'rectangle' });
  assert.ok(r.points[1][0] < 0 && r.points[1][1] < 0);
  assert.ok(r.width > 0 && r.height > 0);
});

test('an arrow is placed between its shapes when it is drawn', () => {
  const { list } = s.skeletons([{ type: 'rectangle', id: 'a', x: 0, y: 0, width: 100, height: 50 }, { type: 'arrow', from: 'a', to: 'old' }], [box('old', 300, 0)]);
  near(list[1].x, 106);
  near(list[1].points[1][0], 188);
});

test('an arrow with one shape runs from it to where it was put, and text has a size before it is measured', () => {
  const { list } = s.skeletons([{ type: 'text', id: 't', x: 0, y: 0, text: 'x' }, { type: 'arrow', from: 't', x: 300, y: 15 }], []);
  assert.ok(list[1].x > 50 && list[1].width > 100);
  near(list[1].x + list[1].points[1][0], 300);
});

test('what cannot be drawn is said', () => {
  const refused = (shapes, on = []) => assert.throws(() => s.skeletons(shapes, on));
  refused([]);
  refused('rectangle');
  refused([{ type: 'star' }]);
  refused([{ type: 'text' }]);
  refused([{ type: 'line', text: 'x' }]);
  refused([{ type: 'rectangle', x: '1' }]);
  refused([{ type: 'rectangle', stroke: 'url(evil)' }]);
  refused([{ type: 'arrow', to: 'nobody' }]);
  refused([{ type: 'rectangle', from: 'a' }], [box('a')]);
  refused([{ type: 'rectangle', id: 'a' }], [box('a')]);
  refused([{ type: 'rectangle', id: 'b' }, { type: 'ellipse', id: 'b' }]);
  refused([{ type: 'arrow', to: 'l' }], [{ ...box('l'), type: 'line' }]);
  refused(Array.from({ length: s.MAX_SHAPES + 1 }, () => ({ type: 'rectangle' })));
});

test('a board shape keeps its kind when an arrow borrows it, and an image is joined as a box', () => {
  assert.equal(s.borrowedCopy(box('a')).type, 'rectangle');
  assert.equal(s.borrowedCopy({ ...box('a'), type: 'diamond' }).type, 'diamond');
  assert.equal(s.borrowedCopy({ ...box('a'), type: 'image' }).type, 'rectangle');
  assert.equal(s.borrowedCopy({ ...box('a'), type: 'text', text: 'x' }).text, 'x');
});

test('an update sets only what was given', () => {
  assert.deepEqual(s.patched({ x: 3, stroke: '#123456', id: 'a', text: 'no' }), { x: 3, strokeColor: '#123456' });
  assert.throws(() => s.patched({ width: 'wide' }));
});

test('reading gives labels with their shape, and joined arrows by id', () => {
  const scene = [
    box('a', 1.4, 2.6),
    { id: 't', type: 'text', containerId: 'a', text: 'La\nbel', originalText: 'Label', x: 0, y: 0, width: 1, height: 1 },
    { id: 'n', type: 'text', text: 'Note', x: 0, y: 0, width: 1, height: 1 },
    { ...box('b'), isDeleted: true },
    { ...box('c'), type: 'arrow', startBinding: { elementId: 'a' }, endBinding: null },
  ];
  const read = s.describeScene(scene);
  assert.deepEqual(read.map((e) => e.id), ['a', 'n', 'c']);
  assert.deepEqual([read[0].text, read[0].x, read[0].y], ['Label', 1, 3]);
  assert.equal(read[1].text, 'Note');
  assert.equal(read[2].from, 'a');
});

test('a tree reads as one thing, by its group', () => {
  const model = s.fromOutline({ title: 'Agent', children: [{ title: 'Tools' }] }, ['robot', 'tools']);
  const part = (role, more = {}) => ({ ...box(role), groupIds: ['g'], customData: { notepp_tree: true, role, ...more } });
  const read = s.describeScene([part('frame', { model }), part('row'), part('line')]);
  assert.equal(read.length, 1);
  assert.deepEqual([read[0].id, read[0].type, read[0].tree.title, read[0].tree.children[0].title], ['g', 'tree', 'Agent', 'Tools']);
});

test('a tree comes from what an agent wrote, and an unknown icon is chosen from the title', () => {
  const t = s.fromOutline({ title: 'Memory', icon: 'nonsense', children: [{ title: 'x', icon: 'robot', color: '#112233', open: false }] }, ['robot']);
  assert.equal(t.icon, 'database');
  assert.deepEqual([t.children[0].icon, t.children[0].color, t.children[0].open], ['robot', '#112233', false]);
  assert.throws(() => s.fromOutline({ children: [] }, []));
  assert.throws(() => s.fromOutline({ title: 'a', children: 'b' }, []));
  assert.throws(() => s.fromOutline({ title: 'a', color: 'red' }, []));
  let deep = { title: 'x' };
  for (let i = 0; i < 40; i += 1) deep = { title: 'x', children: [deep] };
  assert.throws(() => s.fromOutline(deep, []));
});

test('reading a tree back and writing it keeps it the same', () => {
  const t = s.fromOutline({ title: 'A', icon: 'robot', children: [{ title: 'B', icon: 'tools', color: '#112233' }] }, ['robot', 'tools']);
  assert.deepEqual(s.outline(s.fromOutline(s.outline(t), ['robot', 'tools'])), s.outline(t));
});

test('a shape goes with its label, and arrows let go of it', () => {
  const scene = [
    box('a', 0, 0, { boundElements: [{ type: 'text', id: 't' }, { type: 'arrow', id: 'arrow' }] }),
    { id: 't', type: 'text', containerId: 'a', text: 'x', x: 0, y: 0 },
    box('b', 0, 0, { boundElements: [{ type: 'arrow', id: 'arrow' }] }),
    { ...box('arrow'), type: 'arrow', startBinding: { elementId: 'a' }, endBinding: { elementId: 'b' } },
  ];
  const r = s.removal(scene, ['a']);
  assert.deepEqual(r.gone.sort(), ['a', 't']);
  assert.deepEqual(r.changed, [{ id: 'arrow', patch: { startBinding: null } }]);
  const noArrow = s.removal(scene, ['arrow']);
  assert.deepEqual(noArrow.changed.map((c) => c.id).sort(), ['a', 'b']);
  assert.deepEqual(noArrow.changed.find((c) => c.id === 'b').patch.boundElements, []);
});

test('a tree goes as a whole, and a shape that is not there is said', () => {
  const part = (id) => ({ ...box(id), groupIds: ['g'], customData: { notepp_tree: true, role: 'row' } });
  assert.deepEqual(s.removal([part('p'), part('q'), box('x')], ['g']).gone.sort(), ['p', 'q']);
  assert.throws(() => s.removal([box('x')], ['nobody']));
  assert.throws(() => s.removal([box('x')], []));
  assert.throws(() => s.removal([{ ...box('x'), isDeleted: true }], ['x']));
});
