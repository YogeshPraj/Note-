// The whiteboard Library's built-in items. They are made at start-up and never saved with the user's own items.

import { agentTemplate, newId, treeTemplate } from './model.js';
import { makeShapes } from './scene.js';

export const LIBRARY_IDS = ['notepp-tree', 'notepp-agent'];

const item = (id, model) => ({ id, status: 'published', created: 1, name: id === 'notepp-tree' ? 'Tree' : 'Agent', elements: makeShapes(model, 0, 0, newId()) });

export const libraryItems = () => [item('notepp-agent', agentTemplate()), item('notepp-tree', treeTemplate())];
