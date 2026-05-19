import * as fs from 'fs';
import { getMermaidShapeByValue } from '../shapes/flowchartShapes.js';
import { createDefaultStyle, mapArrowTypeToNumber, mapLinePatternToNumber, mapFillPatternToNumber, } from '../utils/styleUtils.js';
export const parseDiagram = async (filePath) => {
    try {
        const buffer = fs.readFileSync(filePath);
        const jsonContent = buffer.toString('utf-8');
        const excalidrawData = JSON.parse(jsonContent);
        const shapes = getShapes(excalidrawData);
        return { Shapes: shapes };
    }
    catch (error) {
        console.error('Error parsing Excalidraw file:', error);
        return { Shapes: [] };
    }
};
export const getShapes = (excalidrawData) => {
    var _a, _b;
    const shapes = [];
    if (!excalidrawData.elements) {
        return shapes;
    }
    const boundTextMap = new Map();
    for (const element of excalidrawData.elements) {
        if (element.type === 'text' && element.containerId && !element.isDeleted) {
            boundTextMap.set(element.containerId, element.text || '');
        }
    }
    for (const element of excalidrawData.elements) {
        if (element.isDeleted) {
            continue;
        }
        if (element.type === 'text') {
            continue;
        }
        const label = boundTextMap.get(element.id) || element.text || '';
        const shape = {
            Id: element.id,
            ShapeType: mapExcalidrawShapeToMermaid(element.type),
            Label: label,
            Style: createStyleFromExcalidrawElement(element),
            IsEdge: isEdgeType(element.type),
            FromNode: '',
            ToNode: '',
        };
        if (shape.IsEdge) {
            if ((_a = element.startBinding) === null || _a === void 0 ? void 0 : _a.elementId) {
                shape.FromNode = element.startBinding.elementId;
            }
            if ((_b = element.endBinding) === null || _b === void 0 ? void 0 : _b.elementId) {
                shape.ToNode = element.endBinding.elementId;
            }
            if (element.startArrowhead || element.endArrowhead) {
                shape.Style.BeginArrow = mapArrowTypeToNumber(element.startArrowhead);
                shape.Style.EndArrow = mapArrowTypeToNumber(element.endArrowhead);
            }
        }
        shapes.push(shape);
    }
    return shapes;
};
const mapExcalidrawShapeToMermaid = (excalidrawType) => {
    const excalidrawToStandardMap = {
        rectangle: 'rectangle',
        diamond: 'diamond',
        ellipse: 'circle',
        triangle: 'triangle',
        text: 'rectangle',
        arrow: 'arrow',
        line: 'line',
        freedraw: 'line',
        image: 'rectangle',
    };
    const standardName = excalidrawToStandardMap[excalidrawType.toLowerCase()] || 'rectangle';
    return getMermaidShapeByValue(standardName);
};
const isEdgeType = (elementType) => {
    const edgeTypes = ['arrow', 'line', 'freedraw'];
    return edgeTypes.includes(elementType.toLowerCase());
};
const createStyleFromExcalidrawElement = (element) => {
    const style = createDefaultStyle();
    if (element.backgroundColor && element.backgroundColor !== 'transparent') {
        style.FillForeground = element.backgroundColor;
    }
    if (element.strokeColor) {
        style.LineColor = element.strokeColor;
    }
    if (element.strokeWidth) {
        style.LineWeight = element.strokeWidth;
    }
    style.LinePattern = mapLinePatternToNumber(element.strokeStyle);
    style.FillPattern = mapFillPatternToNumber(element.fillStyle);
    return style;
};
//# sourceMappingURL=excalidrawParser.js.map