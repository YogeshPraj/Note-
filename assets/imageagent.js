// What an AI agent asks for on a picture, as data: its annotations turned into the page's objects and back. No DOM in here,
// so it can be tested with `node --test`. It is put into imageeditor.html, whose script carries the requests out.
var imageAgent = (function () {
  var KINDS = { rectangle: 'rect', ellipse: 'oval', line: 'line', arrow: 'arrow', text: 'text', bubble: 'bubble', step: 'step', highlight: 'highlight', hide: 'hide' };
  var NAMES = { rect: 'rectangle', oval: 'ellipse', free: 'freehand' };
  var COLOUR = /^#[0-9a-f]{3,8}$/i;
  // The most one call can add, and the largest picture: the same limit as the Resize dialog, and an area a page can hold.
  var MAX_ITEMS = 300, MAX_SIDE = 16000, MAX_AREA = 64000000;

  function fail(message) { throw new Error(message); }
  function isNumber(n) { return typeof n === 'number' && isFinite(n); }

  function number(args, name, otherwise) {
    var v = args[name];
    if (v === undefined || v === null) return otherwise;
    if (!isNumber(v)) fail('"' + name + '" must be a number.');
    return v;
  }

  function colour(args, name, otherwise) {
    var v = args[name];
    if (v === undefined || v === null) return otherwise;
    if (typeof v !== 'string' || !COLOUR.test(v)) fail('"' + name + '" must be a colour like #e53935.');
    return v;
  }

  function required(args, name) {
    return number(args, name, undefined) === undefined ? fail('"' + name + '" is required.') : args[name];
  }

  function words(args) {
    if (typeof args.text !== 'string' || !args.text) fail('A ' + args.type + ' needs "text".');
    return args.text;
  }

  function isBox(t) { return t === 'rect' || t === 'oval' || t === 'bubble' || t === 'highlight' || t === 'hide'; }
  function isLine(t) { return t === 'line' || t === 'arrow'; }

  // The two corners of a box, or the two ends of a line.
  function corners(o, args, t) {
    var x = required(args, 'x'), y = required(args, 'y');
    o.x1 = x; o.y1 = y; o.x2 = x; o.y2 = y;
    if (isLine(t)) { o.x2 = required(args, 'x2'); o.y2 = required(args, 'y2'); }
    if (isBox(t)) {
      var defaultW = t === 'bubble' ? 170 : undefined, defaultH = t === 'bubble' ? 70 : undefined;
      o.x2 = x + (number(args, 'width', defaultW) === undefined ? fail('A ' + args.type + ' needs "width" and "height".') : number(args, 'width', defaultW));
      o.y2 = y + (number(args, 'height', defaultH) === undefined ? fail('A ' + args.type + ' needs "width" and "height".') : number(args, 'height', defaultH));
    }
  }

  // The page's object for one annotation. `style` is the editor's current look; `steps` how many step labels there are.
  function toObject(args, style, steps) {
    var t = args && KINDS[args.type];
    if (!t) fail('"type" must be one of ' + Object.keys(KINDS).join(', ') + '.');
    var w = number(args, 'thickness', style.w);
    if (w < 0) fail('"thickness" must not be negative.');
    var fs = number(args, 'font_size', style.fs);
    if (fs <= 0) fail('"font_size" must be above 0.');
    var o = { t: t, line: colour(args, 'color', style.line), fill: null, w: w, fs: fs };
    corners(o, args, t);
    if (t === 'rect' || t === 'oval' || t === 'bubble' || t === 'step') o.fill = colour(args, 'fill', null);
    if (t === 'highlight') { o.fill = colour(args, 'fill', colour(args, 'color', '#ffeb3b')); o.line = o.fill; }
    // Blocks in proportion to the area, or something small keeps its shape and colour. The page doubles this.
    if (t === 'hide') o.w = Math.max(5, number(args, 'thickness', Math.max(style.w * 2, Math.ceil(Math.min(Math.abs(o.x2 - o.x1), Math.abs(o.y2 - o.y1)) / 4))));
    if (t === 'text') o.text = words(args);
    if (t === 'step') o.n = steps + 1;
    if (t === 'bubble') {
      o.text = words(args);
      o.tx = number(args, 'tail_x', o.x1 + (o.x2 - o.x1) * 0.3);
      o.ty = number(args, 'tail_y', o.y2 + 28);
    }
    return o;
  }

  // Objects for a list of annotations. Nothing is made if any of them is wrong, so a call adds all it was given or nothing.
  function toObjects(items, style, steps) {
    if (!Array.isArray(items) || !items.length) fail('"items" must list at least one annotation.');
    if (items.length > MAX_ITEMS) fail('One call can add at most ' + MAX_ITEMS + ' annotations.');
    return items.map(function (item) {
      var o = toObject(item, style, steps);
      if (o.t === 'step') steps += 1;
      return o;
    });
  }

  function describe(o) {
    var x = Math.min(o.x1, o.x2), y = Math.min(o.y1, o.y2), out = { id: o.id, type: NAMES[o.t] || o.t };
    if (o.t === 'free') { out.points = o.pts.length; return out; }
    out.x = Math.round(isLine(o.t) ? o.x1 : x); out.y = Math.round(isLine(o.t) ? o.y1 : y);
    if (isLine(o.t)) { out.x2 = Math.round(o.x2); out.y2 = Math.round(o.y2); }
    if (isBox(o.t)) { out.width = Math.round(Math.abs(o.x2 - o.x1)); out.height = Math.round(Math.abs(o.y2 - o.y1)); }
    if (o.text) out.text = o.text;
    if (o.t === 'step') out.number = o.n;
    if (o.t !== 'hide') out.color = o.line;
    if (o.fill) out.fill = o.fill;
    return out;
  }

  // Change an object in place, after everything has been checked, so that a wrong value changes nothing.
  function patch(o, args) {
    if (o.t === 'free') fail('Freehand strokes cannot be changed by an agent: delete it instead.');
    var line = colour(args, 'color', undefined), fill = colour(args, 'fill', undefined);
    var thickness = number(args, 'thickness', undefined), size = number(args, 'font_size', undefined);
    var x = number(args, 'x', undefined), y = number(args, 'y', undefined), x2 = number(args, 'x2', undefined), y2 = number(args, 'y2', undefined);
    var width = number(args, 'width', undefined), height = number(args, 'height', undefined);
    if (args.text !== undefined && typeof args.text !== 'string') fail('"text" must be text.');
    var left = Math.min(o.x1, o.x2), top = Math.min(o.y1, o.y2);
    var boxW = Math.abs(o.x2 - o.x1), boxH = Math.abs(o.y2 - o.y1);
    if (isBox(o.t)) {
      var nx = x === undefined ? left : x, ny = y === undefined ? top : y;
      if (o.t === 'bubble') { o.tx += nx - left; o.ty += ny - top; }
      o.x1 = nx; o.y1 = ny;
      o.x2 = nx + (width === undefined ? boxW : width); o.y2 = ny + (height === undefined ? boxH : height);
    } else {
      if (x !== undefined) o.x1 = x;
      if (y !== undefined) o.y1 = y;
      if (isLine(o.t)) { if (x2 !== undefined) o.x2 = x2; if (y2 !== undefined) o.y2 = y2; } else { o.x2 = o.x1; o.y2 = o.y1; }
    }
    if (line !== undefined) { o.line = line; if (o.t === 'highlight') o.fill = line; }
    if (fill !== undefined) { o.fill = fill; if (o.t === 'highlight') o.line = fill; }
    if (thickness !== undefined) o.w = thickness;
    if (size !== undefined) o.fs = size;
    if (args.text !== undefined && (o.t === 'text' || o.t === 'bubble')) o.text = args.text;
  }

  // A size a page can hold, in whole pixels.
  function size(args) {
    var w = Math.round(required(args, 'width')), h = Math.round(required(args, 'height'));
    if (w < 1 || h < 1 || w > MAX_SIDE || h > MAX_SIDE || w * h > MAX_AREA) fail('That size is more than a picture can be.');
    return { w: w, h: h };
  }

  // The part of a picture of `width` by `height` to keep.
  function cropBox(args, width, height) {
    var s = size(args), x = Math.round(required(args, 'x')), y = Math.round(required(args, 'y'));
    if (x < 0 || y < 0 || x + s.w > width || y + s.h > height) fail('The crop must be inside the picture, which is ' + width + ' by ' + height + '.');
    if (s.w < 2 || s.h < 2) fail('The crop is too small.');
    return { x: x, y: y, w: s.w, h: s.h };
  }

  // The size of a picture scaled down to fit `longest`, never up.
  function fitted(width, height, longest) {
    var scale = Math.min(1, longest / Math.max(width, height));
    return { w: Math.max(1, Math.round(width * scale)), h: Math.max(1, Math.round(height * scale)) };
  }

  return { toObjects: toObjects, describe: describe, patch: patch, size: size, cropBox: cropBox, fitted: fitted };
})();
if (typeof module !== 'undefined') module.exports = imageAgent;
