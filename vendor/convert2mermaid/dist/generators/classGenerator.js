import { extractStereotype, extractClassName, parseClassContent, getShapeLabel, isCardinalityLabel, } from '../utils/classUtils.js';
import { sanitizeClassName, sanitizeLabel } from '../utils/labelUtils.js';
import { determineClassRelationshipType } from '../utils/relationshipUtils.js';
export const generateClassDiagram = (diagram) => {
    let mermaidSyntax = 'classDiagram\r\n';
    const classes = [];
    const relationships = [];
    for (const shape of diagram.Shapes) {
        if (shape.IsEdge) {
            relationships.push(shape);
        }
        else if (!isCardinalityLabel(shape)) {
            classes.push(shape);
        }
    }
    for (const classShape of classes) {
        const className = extractClassName(classShape.Label, sanitizeClassName) || sanitizeClassName(classShape.Id);
        if (!className || /^[_\-\d]+$/.test(className) || className.match(/^[A-Za-z0-9_-]{20,}[0-9]$/)) {
            continue;
        }
        const classContent = parseClassContent(classShape.Label);
        if (!classShape.Label && classContent.length === 0) {
            continue;
        }
        const stereotype = extractStereotype(classShape.Label);
        mermaidSyntax += `  class ${className}`;
        if (stereotype) {
            mermaidSyntax += `\r\n  <<${stereotype}>> ${className}`;
        }
        mermaidSyntax += ` {\r\n`;
        for (const member of classContent) {
            mermaidSyntax += `    ${member}\r\n`;
        }
        mermaidSyntax += `  }\r\n`;
    }
    for (const rel of relationships) {
        let fromClass = extractClassName(getShapeLabel(diagram.Shapes, rel.FromNode), sanitizeClassName) ||
            sanitizeClassName(rel.FromNode || '');
        let toClass = extractClassName(getShapeLabel(diagram.Shapes, rel.ToNode), sanitizeClassName) ||
            sanitizeClassName(rel.ToNode || '');
        if (fromClass && toClass) {
            const relInfo = determineClassRelationshipType(rel);
            const relType = relInfo.type;
            const shouldReverse = relInfo.reverse;
            if (shouldReverse) {
                [fromClass, toClass] = [toClass, fromClass];
            }
            const label = rel.Label ? ` : ${sanitizeLabel(rel.Label)}` : '';
            mermaidSyntax += `  ${fromClass} ${relType} ${toClass}${label}\r\n`;
        }
    }
    return mermaidSyntax;
};
//# sourceMappingURL=classGenerator.js.map