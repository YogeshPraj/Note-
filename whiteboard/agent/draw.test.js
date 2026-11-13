import assert from 'node:assert/strict';
import { test } from 'node:test';

import { plan } from './draw.js';

const box = (id, more = {}) => ({ id, type: 'rectangle', x: 0, y: 0, width: 100, height: 50, ...more });
const rejects = (input, board, pattern) => assert.throws(() => plan(input, board), pattern);

test('a JSON list is read as it is, with labels kept', () => {
  const r = plan(JSON.stringify([{ type: 'rectangle', id: 'a', x: 1, y: 2, width: 3, height: 4, label: { text: 'Hi' } }]), []);
  assert.equal(r.list[0].label.text, 'Hi');
  assert.deepEqual([r.remove, r.borrowed, r.camera], [[], [], undefined]);
});

test('a list given as a list works as well as one given as text', () => {
  assert.equal(plan([box('a')], []).list.length, 1);
});

test('text that is not JSON, or not a list, says so', () => {
  rejects('[{', [], /not valid JSON/);
  rejects('{}', [], /JSON list/);
  rejects('[]', [], /at least one/);
  rejects([3], [], /object/);
});

test('an arrow names its ends as the converter wants, and an end that is nowhere is let go', () => {
  const arrow = { type: 'arrow', id: 'l', x: 0, y: 0, startBinding: { elementId: 'a', fixedPoint: [1, 0.5] }, endBinding: { elementId: 'nowhere' } };
  const { list, borrowed } = plan([box('a'), arrow], []);
  assert.deepEqual(list[1].start, { id: 'a' });
  assert.equal('end' in list[1], false);
  assert.equal('startBinding' in list[1], false);
  assert.deepEqual(borrowed, []);
});

test('an arrow can join a shape that is already on the board, which the converter then has to be handed', () => {
  const { borrowed } = plan([{ type: 'arrow', id: 'l', startBinding: { elementId: 'old' } }], [box('old')]);
  assert.deepEqual(borrowed, ['old']);
});

test('an arrow cannot join another arrow on the board', () => {
  const { list, borrowed } = plan([{ type: 'arrow', id: 'l', startBinding: { elementId: 'arr' } }], [{ id: 'arr', type: 'arrow' }]);
  assert.deepEqual([borrowed, 'start' in list[0]], [[], false]);
});

test('delete removes what is on the board, or what this list drew before it, and takes comma lists', () => {
  const r = plan([box('n1'), { type: 'delete', ids: 'old, n1' }, box('n2')], [box('old')]);
  assert.deepEqual(r.remove, ['old']);
  assert.deepEqual(r.list.map((e) => e.id), ['n2']);
  assert.deepEqual(plan([{ type: 'delete', ids: ['old'] }], [box('old')]).remove, ['old']);
  rejects([{ type: 'delete', ids: 'nope' }], [], /"nope"/);
});

test('an id that is already used is refused, unless the shape that had it is deleted in the same list', () => {
  rejects([box('a')], [box('a')], /already in use/);
  rejects([box('a'), box('a')], [], /already in use/);
  assert.equal(plan([{ type: 'delete', ids: 'a' }, box('a')], [box('a')]).list.length, 1);
});

test('a camera frame is the last one asked for, and must be numbers', () => {
  const r = plan([{ type: 'cameraUpdate', x: 0, y: 0, width: 400, height: 300 }, box('a'), { type: 'cameraUpdate', x: 5, y: 6, width: 800, height: 600 }], []);
  assert.deepEqual(r.camera, { x: 5, y: 6, width: 800, height: 600 });
  rejects([{ type: 'cameraUpdate', x: 0, y: 0, width: 'wide', height: 3 }], [], /width/);
  rejects([{ type: 'cameraUpdate', x: 0, y: 0, width: 0, height: 3 }], [], /above 0/);
});

test('restoreCheckpoint is ignored, and a type that cannot be drawn is refused', () => {
  assert.equal(plan([{ type: 'restoreCheckpoint', id: 'x' }, box('a')], []).list.length, 1);
  rejects([{ type: 'star', id: 'a' }], [], /"star" cannot be drawn/);
});

test('a list of only frames and deletes is allowed', () => {
  const r = plan([{ type: 'cameraUpdate', x: 0, y: 0, width: 4, height: 3 }], []);
  assert.deepEqual(r.list, []);
});
