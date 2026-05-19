export const determineClassRelationshipType = (rel) => {
    var _a;
    const startArrow = rel.Style.BeginArrow;
    const endArrow = rel.Style.EndArrow;
    const startFill = rel.Style.BeginArrowSize;
    const endFill = rel.Style.EndArrowSize;
    const linePattern = rel.Style.LinePattern;
    if (linePattern === 2) {
        if (endArrow === 4 || (endArrow === 2 && endFill === 0)) {
            return { type: '..|>', reverse: false };
        }
        if (startArrow === 4 || (startArrow === 2 && startFill === 0)) {
            return { type: '..|>', reverse: true };
        }
    }
    if (endArrow === 3 || (endArrow === 2 && endFill === 0 && linePattern !== 2)) {
        return { type: '--|>', reverse: false };
    }
    if (startArrow === 3 || (startArrow === 2 && startFill === 0 && linePattern !== 2)) {
        return { type: '--|>', reverse: true };
    }
    if (startArrow === 8 && startFill === 1) {
        return { type: '*--', reverse: false };
    }
    if (endArrow === 8 && endFill === 1) {
        return { type: '--*', reverse: false };
    }
    if (startArrow === 8 && startFill === 0) {
        return { type: 'o--', reverse: false };
    }
    if (endArrow === 8 && endFill === 0) {
        return { type: '--o', reverse: false };
    }
    if (linePattern === 2) {
        return { type: '..>', reverse: false };
    }
    const label = ((_a = rel.Label) === null || _a === void 0 ? void 0 : _a.toLowerCase()) || '';
    if (label.includes('inherit') || label.includes('extends')) {
        return { type: '--|>', reverse: false };
    }
    else if (label.includes('implement') || label.includes('interface')) {
        return { type: '..|>', reverse: false };
    }
    else if (label.includes('composition')) {
        return { type: '*--', reverse: false };
    }
    else if (label.includes('aggregation')) {
        return { type: 'o--', reverse: false };
    }
    else if (label.includes('dependency') || label.includes('depends')) {
        return { type: '..>', reverse: false };
    }
    return { type: '-->', reverse: false };
};
export const parseCardinality = (label) => {
    if (!label)
        return '||--||';
    const cardinality = label.toLowerCase();
    if (cardinality.includes('1:1') || cardinality.includes('one to one')) {
        return '||--||';
    }
    else if (cardinality.includes('1:m') || cardinality.includes('1:n') || cardinality.includes('one to many')) {
        return '||--o{';
    }
    else if (cardinality.includes('m:n') || cardinality.includes('many to many')) {
        return '}o--o{';
    }
    else {
        return '||--||';
    }
};
export const parseEntityAttributes = (label) => {
    if (!label)
        return [];
    const lines = label
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter((line) => line.length > 0);
    const attributes = [];
    for (const line of lines) {
        if (line.includes(':') || line.includes(' ')) {
            const parts = line.split(/[:\s]+/);
            if (parts.length >= 2) {
                const attrName = parts[0];
                const attrType = parts[1] || 'string';
                attributes.push(`${attrType} ${attrName}`);
            }
        }
    }
    return attributes.length > 0 ? attributes : ['string name'];
};
//# sourceMappingURL=relationshipUtils.js.map