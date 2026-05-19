import { shapeToNode, sanitizeLabel } from '../utils/labelUtils.js';
export const generateMindmapDiagram = (diagram) => {
    const nodes = [];
    for (const shape of diagram.Shapes) {
        if (!shape.IsEdge) {
            nodes.push(shapeToNode(shape));
        }
    }
    let mermaidCode = 'mindmap\n';
    const rootNode = nodes[0];
    if (rootNode) {
        const rootLabel = sanitizeLabel(rootNode.Shape.Label);
        mermaidCode += `  root(${rootLabel})\n`;
        for (let i = 1; i < nodes.length; i++) {
            const node = nodes[i];
            const label = sanitizeLabel(node.Shape.Label);
            mermaidCode += `    ${label}\n`;
        }
    }
    return mermaidCode;
};
//# sourceMappingURL=mindmapGenerator.js.map