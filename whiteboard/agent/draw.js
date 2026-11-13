// Excalidraw MCP's drawing format (what its `create_view` takes), read for a whiteboard that is already open: a JSON list of
// elements, where a shape may carry a `label`, an arrow names its ends with `startBinding`/`endBinding`, and two entries that
// are not drawn say what else to do: `delete` removes shapes by id and `cameraUpdate` frames part of the board.

import { KINDS, MAX_SHAPES } from './shapes.js';

const fail = (message) => { throw new Error(message); };
/** The longest list an agent may send, in characters (as Excalidraw MCP keeps to). */
export const MAX_INPUT = 5 * 1024 * 1024;
const CAMERA_FIELDS = ['x', 'y', 'width', 'height'];

const idList = (entry) => {
  const ids = entry.ids ?? entry.id;
  const list = Array.isArray(ids) ? ids : String(ids ?? '').split(',');
  return list.map((id) => String(id).trim()).filter(Boolean);
};

function frame(entry) {
  for (const field of CAMERA_FIELDS) {
    if (typeof entry[field] !== 'number' || !Number.isFinite(entry[field])) fail(`A cameraUpdate needs a number for "${field}".`);
  }
  if (entry.width <= 0 || entry.height <= 0) fail('A cameraUpdate needs a width and a height above 0.');
  return { x: entry.x, y: entry.y, width: entry.width, height: entry.height };
}

/** An arrow that names its ends the way Excalidraw's own files do, as the converter wants them: `start` and `end`. */
function ends(entry, known) {
  const { startBinding, endBinding, ...rest } = entry;
  const side = (binding) => (binding?.elementId && known.has(binding.elementId) ? { id: binding.elementId } : undefined);
  const [start, end] = [side(startBinding), side(endBinding)];
  return { ...rest, ...(start ? { start } : {}), ...(end ? { end } : {}) };
}

/**
 * What an agent's list asks for. `input` is the JSON text, or the list itself; `board` is every element of the scene.
 * Gives the descriptions to draw, the shapes on the board that its arrows join (the converter has to be handed those too),
 * the ids to remove from the board and the last frame asked for.
 */
export function plan(input, board) {
  let entries = input;
  if (typeof input === 'string') {
    if (input.length > MAX_INPUT) fail('"elements" is too long. Draw in several calls.');
    try {
      entries = JSON.parse(input);
    } catch (e) {
      fail(`"elements" is not valid JSON: ${e.message}. Use no comments and no trailing commas.`);
    }
  }
  if (!Array.isArray(entries) || !entries.length) fail('"elements" must be a JSON list with at least one element.');
  if (entries.some((e) => !e || typeof e !== 'object')) fail('Every element must be an object.');

  const onBoard = new Map(board.filter((e) => !e.isDeleted).map((e) => [e.id, e]));
  const drawn = [];
  const remove = [];
  let camera;
  for (const entry of entries) {
    if (entry.type === 'cameraUpdate') {
      camera = frame(entry);
    } else if (entry.type === 'restoreCheckpoint') {
      // Excalidraw MCP keeps pictures to come back to; here the picture is the open whiteboard, so there is nothing to restore.
    } else if (entry.type === 'delete') {
      for (const id of idList(entry)) {
        const at = drawn.findIndex((d) => d.id === id);
        if (at >= 0) drawn.splice(at, 1);
        else if (onBoard.has(id)) remove.push(id);
        else fail(`No shape has the id "${id}" to delete.`);
      }
    } else if (KINDS.includes(entry.type)) {
      if (entry.id !== undefined && typeof entry.id !== 'string') fail('An id must be a string.');
      drawn.push(entry);
    } else {
      fail(`"${entry.type}" cannot be drawn. The types are ${KINDS.join(', ')}, with label on a shape and arrow for connections.`);
    }
  }
  if (drawn.length > MAX_SHAPES) fail(`One call can draw at most ${MAX_SHAPES} elements.`);

  const own = new Set();
  for (const d of drawn) {
    if (!d.id) continue;
    if (own.has(d.id) || (onBoard.has(d.id) && !remove.includes(d.id))) fail(`The id "${d.id}" is already in use. Give each element its own id.`);
    own.add(d.id);
  }
  const gone = new Set(remove);
  const borrowed = new Set();
  const known = new Set(own);
  for (const [id, e] of onBoard) if (!gone.has(id) && e.type !== 'arrow' && e.type !== 'line') known.add(id);
  const list = drawn.map((d) => {
    const arrow = d.type === 'arrow' ? ends(d, known) : d;
    for (const end of [arrow.start?.id, arrow.end?.id]) if (end && !own.has(end)) borrowed.add(end);
    return arrow;
  });
  return { list, borrowed: [...borrowed], remove, camera };
}
