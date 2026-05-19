import { createDefaultStyle } from '../utils/styleUtils.js';
import { createParseError, safeFileRead } from '../utils/errorUtils.js';
export class BaseParser {
    constructor(filePath) {
        this.filePath = filePath;
    }
    async parseDiagram() {
        try {
            const buffer = safeFileRead(this.filePath);
            const shapes = await this.parseContent(buffer);
            return { Shapes: shapes };
        }
        catch (error) {
            console.error(`Error parsing ${this.getParserName()} file:`, error);
            return { Shapes: [] };
        }
    }
    createBaseShape(id, shapeType, label = '') {
        return {
            Id: id,
            ShapeType: shapeType,
            Label: this.sanitizeLabel(label),
            Style: createDefaultStyle(),
            IsEdge: false,
            FromNode: '',
            ToNode: '',
        };
    }
    createEdgeShape(id, fromNode, toNode, label = '') {
        const shape = this.createBaseShape(id, 'line', label);
        shape.IsEdge = true;
        shape.FromNode = fromNode;
        shape.ToNode = toNode;
        return shape;
    }
    sanitizeLabel(label) {
        if (!label)
            return '';
        return label
            .replace(/&lt;/g, '<')
            .replace(/&gt;/g, '>')
            .replace(/&amp;/g, '&')
            .replace(/&quot;/g, '"')
            .replace(/&#39;/g, "'")
            .replace(/<[^>]*>/g, '')
            .trim();
    }
    generateNodeId(originalId) {
        return originalId.replace(/[^a-zA-Z0-9]/g, '_').replace(/^[0-9]/, 'n$&');
    }
    validateFileStructure(data, requiredProps) {
        if (!data || typeof data !== 'object') {
            throw createParseError('Invalid file structure: expected object', this.filePath, this.getParserName());
        }
        const obj = data;
        for (const prop of requiredProps) {
            if (!(prop in obj)) {
                throw createParseError(`Missing required property: ${prop}`, this.filePath, this.getParserName());
            }
        }
    }
}
//# sourceMappingURL=BaseParser.js.map