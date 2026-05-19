import { getMermaidShapeByValue } from '../shapes/flowchartShapes.js';
export const sanitizeLabel = (label) => {
    if (!label)
        return '';
    let sanitized = label.replace(/:/g, '');
    sanitized = sanitized.replace(/\r?\n/g, '<br/>');
    sanitized = sanitized.replace(/\s+/g, ' ');
    sanitized = sanitized.trim();
    return sanitized;
};
export const sanitizeEdgeLabel = (label) => {
    if (!label)
        return '';
    const sanitized = sanitizeLabel(label);
    if (sanitized && !sanitized.includes(' ') && !sanitized.includes('<br/>')) {
        return `"${sanitized}"`;
    }
    return sanitized;
};
export const sanitizeClassName = (name) => {
    if (!name)
        return '';
    return name.replace(/[^a-zA-Z0-9_]/g, '').replace(/^[0-9]/, '_$&');
};
export const shapeToNode = (shape) => {
    const nodeId = `n0${shape.Id}`;
    const nodeShape = getMermaidShapeByValue(shape.ShapeType);
    const sanitizedLabel = sanitizeLabel(shape.Label);
    const nodeDef = `${nodeId}@{ shape: ${nodeShape}, label: ${sanitizedLabel} }`;
    return { ID: nodeId, Shape: shape, NodeDef: nodeDef };
};
export const shapeToConnector = (connectorShape) => {
    const edge = connectorShape;
    edge.FromNode = `n0${connectorShape.FromNode}`;
    edge.ToNode = `n0${connectorShape.ToNode}`;
    return edge;
};
//# sourceMappingURL=labelUtils.js.map