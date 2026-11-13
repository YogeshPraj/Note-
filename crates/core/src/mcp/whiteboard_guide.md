# Drawing on a Note++ whiteboard with draw_elements

`draw_elements` takes a JSON list of elements in the Excalidraw MCP format and draws them on the open whiteboard as one
undo step. Everything stays editable. Nothing leaves the computer. Call whiteboard_image afterwards to check the result.
For a flowchart or sequence diagram from text, add_mermaid is shorter.

## Elements

Every drawn element needs `type`, a unique string `id`, `x`, `y`, `width` and `height`.
Types: rectangle, ellipse, diamond, text, arrow, line.
Defaults, so leave them out: strokeColor "#1e1e1e", backgroundColor "transparent", fillStyle "solid", strokeWidth 2.

- Labelled shape (preferred): `{ "type": "rectangle", "id": "r1", "x": 100, "y": 100, "width": 200, "height": 80, "label": { "text": "Hello", "fontSize": 20 } }`.
  The text is centred and the box grows to fit. Works on rectangle, ellipse and diamond. `roundness: { "type": 3 }` rounds the corners.
- Standalone text, for titles and notes: `{ "type": "text", "id": "t1", "x": 150, "y": 138, "text": "Hello", "fontSize": 20 }`. x is the left edge.
- Arrow: `{ "type": "arrow", "id": "a1", "x": 300, "y": 150, "width": 200, "height": 0, "points": [[0,0],[200,0]], "endArrowhead": "arrow" }`.
  `points` are offsets from x, y. endArrowhead: null, "arrow", "bar", "dot" or "triangle". A `label` goes on an arrow too.
- Joined arrow: add `"startBinding": { "elementId": "r1" }` and `"endBinding": { "elementId": "r2" }`. The arrow is then drawn between
  the two shapes and follows them when they move. They can be shapes from the same call or already on the board (ids from read_whiteboard).

## Two entries that are not drawn

- `{ "type": "delete", "ids": "b2,a1" }` removes shapes by id, from this call or already on the board, and the text inside them.
  Put it after the elements it removes. Do not reuse a deleted id unless the delete comes first.
- `{ "type": "cameraUpdate", "x": 0, "y": 0, "width": 800, "height": 600 }` frames that part of the board. Use a 4:3 size
  (400x300, 800x600, 1200x900). The last one in the list wins. Without one, the board scrolls to what was drawn.
- `restoreCheckpoint` is not needed: the whiteboard itself is what you change. It is ignored.

## Order and size

- List in drawing order: a background, then each shape, its label and its arrows. Later entries are in front.
- Font size at least 16 for text, 20 for titles. Shapes at least 120x60 with 20 to 30 pixels between them.
- A call draws at most 300 elements. Draw a large diagram in several calls.
- Do not use emoji: the font cannot draw them.

## Colours

| Use | Stroke | Fill |
|-----|--------|------|
| Blue, input or primary | #4a9eed | #a5d8ff |
| Green, success or output | #22c55e | #b2f2bb |
| Orange, warning or external | #f59e0b | #ffd8a8 |
| Purple, processing | #8b5cf6 | #d0bfff |
| Red, error | #ef4444 | #ffc9c9 |
| Yellow, notes or decisions | #f59e0b | #fff3bf |
| Teal, storage | #06b6d4 | #c3fae8 |

Text on white must be #757575 or darker. For a zone behind others use a pale fill such as #dbe4ff with `"opacity": 30`.

## Example: two joined boxes

```json
[
  { "type": "cameraUpdate", "x": 50, "y": 50, "width": 800, "height": 600 },
  { "type": "rectangle", "id": "b1", "x": 100, "y": 100, "width": 200, "height": 100, "roundness": { "type": 3 }, "backgroundColor": "#a5d8ff", "label": { "text": "Start", "fontSize": 20 } },
  { "type": "rectangle", "id": "b2", "x": 450, "y": 100, "width": 200, "height": 100, "roundness": { "type": 3 }, "backgroundColor": "#b2f2bb", "label": { "text": "End", "fontSize": 20 } },
  { "type": "arrow", "id": "a1", "x": 300, "y": 150, "width": 150, "height": 0, "points": [[0,0],[150,0]], "endArrowhead": "arrow", "startBinding": { "elementId": "b1" }, "endBinding": { "elementId": "b2" } }
]
```
