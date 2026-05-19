import * as fs from 'fs';
import { parseStringPromise } from 'xml2js';
import { MermaidShape, getMermaidShapeByValue } from '../shapes/flowchartShapes.js';
import { createDefaultStyle, mapArrowTypeToNumber } from '../utils/styleUtils.js';
export const parseDiagram = async (filePath) => {
    const buffer = fs.readFileSync(filePath);
    const xmlContent = buffer.toString('utf-8');
    const jsonObj = await parseStringPromise(xmlContent);
    const shapes = getShapes(jsonObj);
    return { Shapes: shapes };
};
export const getShapes = (jsonObj) => {
    var _a;
    const shapes = [];
    try {
        const diagram = jsonObj['mxfile']['diagram'];
        if (!diagram || !diagram[0]) {
            return shapes;
        }
        const mxGraphModel = diagram[0]['mxGraphModel'];
        if (!mxGraphModel || !mxGraphModel[0]) {
            return shapes;
        }
        const root = mxGraphModel[0]['root'];
        if (!root || !root[0]) {
            return shapes;
        }
        const cells = root[0]['mxCell'];
        if (!cells) {
            return shapes;
        }
        const cellMap = new Map();
        const edgeLabels = new Map();
        const swimlanes = new Map();
        const swimlaneChildren = new Map();
        for (const cell of cells) {
            const cellData = cell['$'];
            cellMap.set(cellData.id, cellData);
            if (cellData.style && cellData.style.includes('swimlane')) {
                swimlanes.set(cellData.id, cellData);
                swimlaneChildren.set(cellData.id, []);
            }
            if (cellData.connectable === '0' && cellData.vertex === '1' && cellData.parent !== '1') {
                const parentId = cellData.parent;
                if (parentId && cellData.value) {
                    if (swimlanes.has(parentId)) {
                        (_a = swimlaneChildren.get(parentId)) === null || _a === void 0 ? void 0 : _a.push(cellData);
                    }
                    else {
                        edgeLabels.set(parentId, cellData.value);
                    }
                }
            }
            if (cellData.parent && cellData.parent !== '0' && cellData.parent !== '1') {
                if (swimlanes.has(cellData.parent) && cellData.vertex === '1' && cellData.value) {
                    const children = swimlaneChildren.get(cellData.parent);
                    if (children && !children.includes(cellData)) {
                        children.push(cellData);
                    }
                }
            }
        }
        for (const [id, cellData] of cellMap.entries()) {
            if (id === '0' || id === '1') {
                continue;
            }
            if (cellData.connectable === '0' &&
                cellData.vertex === '1' &&
                cellData.parent !== '1' &&
                !swimlanes.has(cellData.parent || '')) {
                continue;
            }
            if (cellData.parent && swimlanes.has(cellData.parent) && cellData.vertex === '1') {
                continue;
            }
            let label = cellData.value || '';
            if (swimlanes.has(id)) {
                const children = swimlaneChildren.get(id) || [];
                if (children.length > 0) {
                    const childLabels = children
                        .filter((child) => child.value && child.value.trim().length > 0)
                        .map((child) => child.value)
                        .join('\n');
                    if (childLabels) {
                        label = `${label}\n---\n${childLabels}`;
                    }
                }
            }
            const shape = {
                Id: id,
                ShapeType: getMermaidShapeByValue('rectangle'),
                Label: label,
                Style: createDefaultStyle(),
                IsEdge: cellData.edge === '1',
                FromNode: cellData.source || '',
                ToNode: cellData.target || '',
            };
            if (cellData.style) {
                parseDrawIOStyle(cellData.style, shape);
            }
            if (shape.IsEdge && edgeLabels.has(id)) {
                shape.Label = edgeLabels.get(id) || '';
            }
            if (shape.Label) {
                let decodedLabel = shape.Label.replace(/&lt;/g, '<')
                    .replace(/&gt;/g, '>')
                    .replace(/&amp;/g, '&')
                    .replace(/&#xa;/g, '\n')
                    .replace(/&#xA;/g, '\n');
                const stereotypes = [];
                decodedLabel = decodedLabel.replace(/<<([^>]+)>>/g, (match) => {
                    stereotypes.push(match);
                    return `___STEREOTYPE_${stereotypes.length - 1}___`;
                });
                decodedLabel = decodedLabel.replace(/<[^>]*>/g, '');
                stereotypes.forEach((stereotype, index) => {
                    decodedLabel = decodedLabel.replace(`___STEREOTYPE_${index}___`, stereotype);
                });
                shape.Label = decodedLabel.trim();
            }
            shapes.push(shape);
        }
    }
    catch (e) {
        console.error('Error parsing DrawIO file:', e);
    }
    return shapes;
};
const parseDrawIOStyle = (styleString, shape) => {
    var _a;
    const stylePairs = styleString.split(';');
    if (stylePairs.length > 0) {
        const firstElement = stylePairs[0].trim();
        if (!firstElement.includes('=')) {
            const mappedShape = mapDrawIOShapeToMermaid(firstElement);
            if (mappedShape !== MermaidShape.Rectangle) {
                shape.ShapeType = mappedShape;
            }
        }
    }
    for (const pair of stylePairs) {
        const [key, value] = pair.split('=');
        if (!key || !value)
            continue;
        switch (key.toLowerCase()) {
            case 'shape':
                shape.ShapeType = mapDrawIOShapeToMermaid(value);
                if (value === 'umlLifeline') {
                    shape.ParticipantType = 'participant';
                }
                else if (value === 'umlFrame') {
                    shape.ParticipantType = 'frame';
                    const label = (_a = shape.Label) === null || _a === void 0 ? void 0 : _a.trim().toLowerCase();
                    if (label === 'par' || label === 'alt' || label === 'loop' || label === 'opt') {
                        shape.FrameType = label;
                    }
                }
                else if (value === 'note') {
                    shape.ParticipantType = 'note';
                }
                break;
            case 'participant':
                if (value === 'umlActor') {
                    shape.ParticipantType = 'actor';
                }
                break;
            case 'points':
                if (styleString.includes('perimeter=orthogonalPerimeter')) {
                    shape.ParticipantType = 'activation';
                }
                break;
            case 'fillcolor':
                shape.Style.FillForeground = value;
                if (value.toLowerCase() === '#ffff88' || value.toLowerCase() === 'yellow') {
                    shape.ParticipantType = 'note';
                }
                break;
            case 'fontcolor':
                shape.Style.TextColor = value;
                break;
            case 'strokecolor':
                shape.Style.LineColor = value;
                break;
            case 'strokewidth':
                shape.Style.LineWeight = parseFloat(value) || 1;
                break;
            case 'rounded':
                if (value === '1') {
                    shape.ShapeType = getMermaidShapeByValue('rounded rectangle');
                }
                break;
            case 'dashed':
                if (value === '1') {
                    shape.Style.LinePattern = 2;
                }
                break;
            case 'dashpattern':
                shape.Style.LinePattern = 2;
                break;
            case 'endarrow':
                shape.Style.EndArrow = mapArrowTypeToNumber(value);
                break;
            case 'startarrow':
                shape.Style.BeginArrow = mapArrowTypeToNumber(value);
                break;
            case 'endfill':
                shape.Style.EndArrowSize = value === '1' ? 1 : 0;
                break;
            case 'startfill':
                shape.Style.BeginArrowSize = value === '1' ? 1 : 0;
                break;
        }
    }
    const combinedStyle = stylePairs.join(';').toLowerCase();
    if (combinedStyle.includes('ellipse') && combinedStyle.includes('shape=cloud')) {
        shape.ShapeType = getMermaidShapeByValue('rectangle');
    }
    else if (combinedStyle.includes('ellipse') && combinedStyle.includes('aspect=fixed')) {
        shape.ShapeType = getMermaidShapeByValue('circle');
    }
    else if (combinedStyle.includes('ellipse') && !combinedStyle.includes('shape=')) {
        shape.ShapeType = getMermaidShapeByValue('circle');
    }
};
const mapDrawIOShapeToMermaid = (drawioShape) => {
    const drawioToStandardMap = {
        rectangle: 'rectangle',
        ellipse: 'circle',
        rhombus: 'diamond',
        triangle: 'triangle',
        hexagon: 'hexagon',
        cylinder: 'cylinder',
        cylinder3: 'cylinder',
        process: 'process',
        decision: 'diamond',
        document: 'document',
        parallelogram: 'parallelogram',
        trapezoid: 'trapezoid',
        step: 'rectangle',
        tape: 'paper-tape',
        card: 'card',
        dataStorage: 'stored-data',
        datastore: 'database',
        internalStorage: 'internalstorage',
        cloud: 'rectangle',
        delay: 'delay',
        display: 'display',
        collate: 'collate',
        manualInput: 'manual-input',
        loopLimit: 'loop-limit',
        offPageConnector: 'rectangle',
        orEllipse: 'circle',
        sumEllipse: 'circle',
        sortShape: 'diamond',
        'mxgraph.flowchart.database': 'database',
        'mxgraph.flowchart.decision': 'diamond',
        'mxgraph.flowchart.collate': 'collate',
        'mxgraph.flowchart.delay': 'delay',
        'mxgraph.flowchart.display': 'display',
        'mxgraph.flowchart.document': 'document',
        'mxgraph.flowchart.extract': 'extract',
        'mxgraph.flowchart.extract_or_measurement': 'extract',
        'mxgraph.flowchart.internal_storage': 'internalstorage',
        'mxgraph.flowchart.loop_limit': 'loop-limit',
        'mxgraph.flowchart.manual_input': 'manual-input',
        'mxgraph.flowchart.manual_operation': 'manual',
        'mxgraph.flowchart.merge': 'triangle',
        'mxgraph.flowchart.merge_or_storage': 'triangle',
        'mxgraph.flowchart.multi-document': 'documents',
        'mxgraph.flowchart.off_page_connector': 'rectangle',
        'mxgraph.flowchart.on_page_connector': 'small circle',
        'mxgraph.flowchart.on-page_reference': 'small circle',
        'mxgraph.flowchart.or': 'circle',
        'mxgraph.flowchart.predefined_process': 'subroutine',
        'mxgraph.flowchart.preparation': 'hexagon',
        'mxgraph.flowchart.sequential_data': 'stored-data',
        'mxgraph.flowchart.direct_data': 'stored-data',
        'mxgraph.flowchart.sort': 'diamond',
        'mxgraph.flowchart.start_1': 'terminal',
        'mxgraph.flowchart.start_2': 'circle',
        'mxgraph.flowchart.stored_data': 'stored-data',
        'mxgraph.flowchart.summing_function': 'circle',
        'mxgraph.flowchart.terminator': 'terminal',
    };
    const standardName = drawioToStandardMap[drawioShape.toLowerCase()] || drawioShape;
    return getMermaidShapeByValue(standardName);
};
//# sourceMappingURL=drawioParser.js.map