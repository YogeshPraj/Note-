const assert = require('node:assert/strict');
const { test } = require('node:test');

const a = require('./imageagent.js');

const style = { line: '#e53935', fill: null, w: 3, fs: 22 };
const one = (item, steps = 0) => a.toObjects([item], style, steps)[0];

test('a box becomes the page object, with the editor look unless told otherwise', () => {
  const o = one({ type: 'rectangle', x: 10, y: 20, width: 30, height: 40 });
  assert.deepEqual([o.t, o.x1, o.y1, o.x2, o.y2, o.line, o.fill, o.w, o.fs], ['rect', 10, 20, 40, 60, '#e53935', null, 3, 22]);
  const styled = one({ type: 'ellipse', x: 0, y: 0, width: 1, height: 1, color: '#000000', fill: '#ffffff', thickness: 0 });
  assert.deepEqual([styled.t, styled.line, styled.fill, styled.w], ['oval', '#000000', '#ffffff', 0]);
});

test('a line and an arrow go from end to end', () => {
  const o = one({ type: 'arrow', x: 1, y: 2, x2: 30, y2: 40 });
  assert.deepEqual([o.t, o.x1, o.y1, o.x2, o.y2], ['arrow', 1, 2, 30, 40]);
});

test('text, bubbles and steps carry what they say', () => {
  assert.equal(one({ type: 'text', x: 1, y: 2, text: 'Hi' }).text, 'Hi');
  const b = one({ type: 'bubble', x: 100, y: 100, text: 'Here' });
  assert.deepEqual([b.x2, b.y2, b.text], [270, 170, 'Here'], 'the size of a drawn one');
  assert.deepEqual([b.tx, b.ty], [151, 198]);
  assert.deepEqual((({ tx, ty }) => [tx, ty])(one({ type: 'bubble', x: 0, y: 0, text: 'x', tail_x: 5, tail_y: 6 })), [5, 6]);
});

test('steps go on from the ones on the picture, and count up within a call', () => {
  const list = a.toObjects([{ type: 'step', x: 0, y: 0 }, { type: 'rectangle', x: 0, y: 0, width: 1, height: 1 }, { type: 'step', x: 0, y: 0 }], style, 2);
  assert.deepEqual([list[0].n, list[2].n], [3, 4]);
});

test('a highlight is one colour, and hiding has a block size that really hides', () => {
  const h = one({ type: 'highlight', x: 0, y: 0, width: 9, height: 9 });
  assert.deepEqual([h.fill, h.line], ['#ffeb3b', '#ffeb3b']);
  assert.equal(one({ type: 'highlight', x: 0, y: 0, width: 9, height: 9, color: '#00ff00' }).fill, '#00ff00');
  assert.equal(one({ type: 'hide', x: 0, y: 0, width: 9, height: 9 }).w, 6);
  assert.equal(one({ type: 'hide', x: 0, y: 0, width: 9, height: 9, thickness: 1 }).w, 5);
  assert.equal(one({ type: 'hide', x: 0, y: 0, width: 200, height: 80 }).w, 20, 'large blocks over a large area');
});

test('what cannot be drawn is said, and one wrong item stops the whole call', () => {
  const refused = (items) => assert.throws(() => a.toObjects(items, style, 0));
  refused([]);
  refused('rectangle');
  refused([{ type: 'star', x: 0, y: 0 }]);
  refused([{ type: 'rectangle', x: 0, y: 0 }]);
  refused([{ type: 'rectangle', y: 0, width: 1, height: 1 }]);
  refused([{ type: 'rectangle', x: '1', y: 0, width: 1, height: 1 }]);
  refused([{ type: 'rectangle', x: Infinity, y: 0, width: 1, height: 1 }]);
  refused([{ type: 'arrow', x: 0, y: 0 }]);
  refused([{ type: 'text', x: 0, y: 0 }]);
  refused([{ type: 'bubble', x: 0, y: 0 }]);
  refused([{ type: 'rectangle', x: 0, y: 0, width: 1, height: 1, color: 'url(evil)' }]);
  refused([{ type: 'rectangle', x: 0, y: 0, width: 1, height: 1, thickness: -1 }]);
  refused([{ type: 'text', x: 0, y: 0, text: 'x', font_size: 0 }]);
  refused([{ type: 'step', x: 0, y: 0 }, { type: 'star' }]);
  refused(Array.from({ length: 301 }, () => ({ type: 'step', x: 0, y: 0 })));
});

test('reading gives the names an agent wrote, whichever way the box was dragged', () => {
  const o = one({ type: 'rectangle', x: 10, y: 20, width: 30, height: 40, fill: '#ffffff' });
  o.id = 'p1';
  [o.x1, o.x2] = [o.x2, o.x1];
  assert.deepEqual(a.describe(o), { id: 'p1', type: 'rectangle', x: 10, y: 20, width: 30, height: 40, color: '#e53935', fill: '#ffffff' });
  const arrow = a.describe(one({ type: 'arrow', x: 1, y: 2, x2: 3, y2: 4 }));
  assert.deepEqual([arrow.x, arrow.y, arrow.x2, arrow.y2], [1, 2, 3, 4]);
  assert.equal(a.describe(one({ type: 'step', x: 0, y: 0 }, 4)).number, 5);
  assert.equal(a.describe({ t: 'free', pts: [1, 2, 3], id: 'x' }).points, 3);
  assert.equal('color' in a.describe(one({ type: 'hide', x: 0, y: 0, width: 1, height: 1 })), false);
});

test('changing a box keeps what was not named, and a bubble tail goes with it', () => {
  const o = one({ type: 'bubble', x: 100, y: 100, width: 100, height: 50, text: 'a', tail_x: 120, tail_y: 170 });
  a.patch(o, { x: 200, text: 'b', color: '#000000' });
  assert.deepEqual([o.x1, o.y1, o.x2, o.y2, o.tx, o.ty, o.text, o.line], [200, 100, 300, 150, 220, 170, 'b', '#000000']);
  a.patch(o, { width: 10, height: 20 });
  assert.deepEqual([o.x2, o.y2], [210, 120]);
});

test('changing a line moves the ends named, and text keeps its anchor together', () => {
  const l = one({ type: 'arrow', x: 0, y: 0, x2: 10, y2: 10 });
  a.patch(l, { x2: 50 });
  assert.deepEqual([l.x1, l.x2, l.y2], [0, 50, 10]);
  const t = one({ type: 'text', x: 1, y: 2, text: 'a' });
  a.patch(t, { x: 9 });
  assert.deepEqual([t.x1, t.x2, t.y1, t.y2], [9, 9, 2, 2]);
});

test('a wrong value changes nothing', () => {
  const o = one({ type: 'rectangle', x: 1, y: 2, width: 3, height: 4 });
  const before = { ...o };
  assert.throws(() => a.patch(o, { x: 50, color: 'red' }));
  assert.deepEqual(o, before);
  assert.throws(() => a.patch(o, { x: 50, width: 'wide' }));
  assert.deepEqual(o, before);
  assert.throws(() => a.patch(o, { x: 50, text: 5 }));
  assert.deepEqual(o, before);
  assert.throws(() => a.patch({ t: 'free', pts: [] }, { x: 1 }));
});

test('sizes and crops stay inside what a picture can be', () => {
  assert.deepEqual(a.size({ width: 10.4, height: 20 }), { w: 10, h: 20 });
  assert.throws(() => a.size({ width: 0, height: 5 }));
  assert.throws(() => a.size({ width: 16001, height: 5 }));
  assert.throws(() => a.size({ width: 16000, height: 16000 }), 'too much memory');
  assert.throws(() => a.size({ width: 5 }));
  assert.deepEqual(a.cropBox({ x: 1, y: 2, width: 10, height: 10 }, 20, 20), { x: 1, y: 2, w: 10, h: 10 });
  assert.throws(() => a.cropBox({ x: 15, y: 0, width: 10, height: 10 }, 20, 20));
  assert.throws(() => a.cropBox({ x: -1, y: 0, width: 10, height: 10 }, 20, 20));
  assert.throws(() => a.cropBox({ x: 0, y: 0, width: 1, height: 10 }, 20, 20));
});

test('a picture for an agent is scaled down, never up', () => {
  assert.deepEqual(a.fitted(4000, 2000, 1600), { w: 1600, h: 800 });
  assert.deepEqual(a.fitted(100, 50, 1600), { w: 100, h: 50 });
  assert.deepEqual(a.fitted(10000, 1, 1600), { w: 1600, h: 1 });
});
