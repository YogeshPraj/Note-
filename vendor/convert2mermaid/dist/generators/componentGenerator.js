import { shapeToNode, shapeToConnector, sanitizeLabel } from '../utils/labelUtils.js';
export const generateComponentDiagram = (diagram) => {
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
    let mermaidCode = 'block-beta\n';
    mermaidCode += '  columns 3\n';
    for (const node of nodes) {
        const label = sanitizeLabel(node.Shape.Label);
        mermaidCode += `  ${node.ID}["${label}"]\n`;
    }
    for (const edge of edges) {
        const label = edge.Label ? ` : ${sanitizeLabel(edge.Label)}` : '';
        mermaidCode += `  ${edge.FromNode} --> ${edge.ToNode}${label}\n`;
    }
    return mermaidCode;
};
//# sourceMappingURL=componentGenerator.js.map