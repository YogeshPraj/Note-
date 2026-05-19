import { shapeToNode, shapeToConnector, sanitizeLabel } from '../utils/labelUtils.js';
export const generateNetworkDiagram = (diagram) => {
    var _a, _b;
    const nodes = [];
    const edges = [];
    for (const shape of diagram.Shapes) {
        if (shape.IsEdge) {
            edges.push(shapeToConnector(shape));
        }
        else {
            nodes.push(shapeToNode(shape));
        }
    }
    let mermaidCode = 'flowchart TD\n';
    for (const node of nodes) {
        const label = sanitizeLabel(node.Shape.Label);
        const shapeStart = ((_a = node.Shape.ShapeType) === null || _a === void 0 ? void 0 : _a.includes('server')) ? '[(' : '[';
        const shapeEnd = ((_b = node.Shape.ShapeType) === null || _b === void 0 ? void 0 : _b.includes('server')) ? ')]' : ']';
        mermaidCode += `  ${node.ID}${shapeStart}"${label}"${shapeEnd}\n`;
    }
    for (const edge of edges) {
        const label = edge.Label ? `|${sanitizeLabel(edge.Label)}|` : '';
        mermaidCode += `  ${edge.FromNode} ${label} --> ${edge.ToNode}\n`;
    }
    return mermaidCode;
};
//# sourceMappingURL=networkGenerator.js.map