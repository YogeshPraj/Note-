import { sanitizeClassName, sanitizeLabel } from '../utils/labelUtils.js';
import { parseCardinality, parseEntityAttributes } from '../utils/relationshipUtils.js';
export const generateERDiagram = (diagram) => {
    let mermaidSyntax = 'erDiagram\r\n';
    const entities = [];
    const relationships = [];
    for (const shape of diagram.Shapes) {
        if (shape.IsEdge) {
            relationships.push(shape);
        }
        else {
            entities.push(shape);
        }
    }
    for (const entity of entities) {
        const entityName = sanitizeClassName(entity.Id);
        mermaidSyntax += `  ${entityName} {\r\n`;
        const attributes = parseEntityAttributes(entity.Label);
        for (const attr of attributes) {
            mermaidSyntax += `    ${attr}\r\n`;
        }
        mermaidSyntax += `  }\r\n`;
    }
    for (const rel of relationships) {
        const fromEntity = sanitizeClassName(rel.FromNode || '');
        const toEntity = sanitizeClassName(rel.ToNode || '');
        if (fromEntity && toEntity) {
            const cardinality = parseCardinality(rel.Label);
            mermaidSyntax += `  ${fromEntity} ${cardinality} ${toEntity} : "${sanitizeLabel(rel.Label) || 'relationship'}"\r\n`;
        }
    }
    return mermaidSyntax;
};
//# sourceMappingURL=erGenerator.js.map