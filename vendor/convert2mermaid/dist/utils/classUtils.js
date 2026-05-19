export const extractStereotype = (label) => {
    if (!label)
        return '';
    let content = label.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
    const stereotypeMatch = content.match(/<<([^>]+)>>/);
    if (stereotypeMatch) {
        return stereotypeMatch[1].trim();
    }
    return '';
};
export const extractClassName = (label, sanitizeClassName) => {
    if (!label)
        return '';
    const boldMatch = label.match(/<b>([^<]+)<\/b>/);
    if (boldMatch) {
        return sanitizeClassName(boldMatch[1]);
    }
    let content = label.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
    content = content.replace(/<[^>]*>/g, '');
    if (content.includes('---')) {
        const parts = content.split('---');
        if (parts.length > 0) {
            const className = parts[0].trim();
            if (className) {
                return sanitizeClassName(className);
            }
        }
    }
    if (content.includes('<<') && content.includes('>>')) {
        const afterStereotype = content.split('>>')[1];
        if (afterStereotype) {
            const words = afterStereotype.trim().split(/[\r\n]+/);
            if (words.length > 0) {
                return sanitizeClassName(words[0].trim());
            }
        }
    }
    const lines = content.split(/[\r\n]+/).filter((line) => line.trim().length > 0);
    if (lines.length > 0) {
        const firstLine = lines[0].trim();
        if (!firstLine.match(/^[+\-#~]\s/)) {
            return sanitizeClassName(firstLine);
        }
    }
    const words = content.split(/\s+/).filter((word) => word.length > 0);
    if (words.length > 0) {
        return sanitizeClassName(words[0]);
    }
    return '';
};
export const parseClassContent = (label) => {
    if (!label)
        return [];
    let content = label
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&amp;/g, '&')
        .replace(/&quot;/g, '"')
        .replace(/&nbsp;/g, ' ');
    const members = [];
    let lines = [];
    if (content.includes('<hr')) {
        const sections = content.split(/<hr[^>]*>/);
        for (let i = 1; i < sections.length; i++) {
            const section = sections[i];
            let cleanSection = section.replace(/<\/p>/g, '').replace(/<p[^>]*>/g, '');
            const sectionLines = cleanSection
                .split(/<br[^>]*>/g)
                .map((line) => line.replace(/<[^>]*>/g, '').trim())
                .filter((line) => line.length > 0);
            lines.push(...sectionLines);
        }
    }
    else if (content.includes('---')) {
        const parts = content.split('---');
        if (parts.length > 1) {
            for (let i = 1; i < parts.length; i++) {
                const sectionLines = parts[i]
                    .split(/\r?\n/)
                    .map((line) => line.trim())
                    .filter((line) => line.length > 0);
                lines.push(...sectionLines);
            }
        }
    }
    else {
        lines = content
            .split(/\r?\n/)
            .map((line) => line.trim())
            .filter((line) => line.length > 0)
            .slice(1);
    }
    for (const line of lines) {
        const cleanLine = line.trim();
        if (!cleanLine)
            continue;
        if (cleanLine === '---' || cleanLine.match(/^-+$/))
            continue;
        if (cleanLine.includes('(') && cleanLine.includes(')')) {
            const visibility = cleanLine.startsWith('+')
                ? '+'
                : cleanLine.startsWith('-')
                    ? '-'
                    : cleanLine.startsWith('#')
                        ? '#'
                        : cleanLine.startsWith('~')
                            ? '~'
                            : '+';
            let method = cleanLine.replace(/^[+\-#~]\s*/, '').trim();
            method = method.replace(/\s*:\s*:\s*/g, ': ');
            if (method) {
                members.push(`${visibility}${method}`);
            }
        }
        else if (cleanLine.startsWith('+') ||
            cleanLine.startsWith('-') ||
            cleanLine.startsWith('#') ||
            cleanLine.startsWith('~')) {
            const visibility = cleanLine.startsWith('+')
                ? '+'
                : cleanLine.startsWith('-')
                    ? '-'
                    : cleanLine.startsWith('#')
                        ? '#'
                        : cleanLine.startsWith('~')
                            ? '~'
                            : '-';
            const attribute = cleanLine.replace(/^[+\-#~]\s*/, '').trim();
            if (attribute) {
                members.push(`${visibility}${attribute}`);
            }
        }
    }
    return members;
};
export const getShapeLabel = (shapes, shapeId) => {
    if (!shapeId)
        return '';
    const shape = shapes.find((s) => s.Id === shapeId);
    return (shape === null || shape === void 0 ? void 0 : shape.Label) || '';
};
export const isCardinalityLabel = (shape) => {
    var _a;
    const label = ((_a = shape.Label) === null || _a === void 0 ? void 0 : _a.trim()) || '';
    return (/^(\d+|\*|0\.\.1|1\.\.\*|0\.\.\*|\d+\.\.\d+|[mn])$/.test(label) ||
        (label === '' && shape.Id.match(/^[_\-]?\d+$/) !== null));
};
//# sourceMappingURL=classUtils.js.map