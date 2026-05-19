export const createDefaultStyle = () => ({
    FillForeground: '',
    FillBackground: '',
    TextColor: '',
    LineWeight: 1,
    LineColor: '',
    LinePattern: 0,
    Rounding: 0,
    BeginArrow: 0,
    BeginArrowSize: 0,
    EndArrow: 0,
    EndArrowSize: 0,
    LineCap: 0,
    FillPattern: 0,
});
export const mapArrowTypeToNumber = (arrowType) => {
    if (!arrowType)
        return 0;
    const normalizedType = arrowType.toLowerCase();
    switch (normalizedType) {
        case 'arrow':
        case 'triangle':
        case 'classic':
            return 1;
        case 'triangle_outline':
        case 'open':
        case 'block':
            return 2;
        case 'blockthin':
            return 3;
        case 'openthin':
            return 4;
        case 'dash':
        case 'dashedopen':
            return 5;
        case 'circle':
        case 'oval':
            return 6;
        case 'circle_outline':
        case 'circlePlus':
            return 7;
        case 'diamond':
        case 'diamondthin':
            return 8;
        case 'diamond_outline':
            return 9;
        case 'cross':
            return 10;
        case 'none':
        default:
            return 0;
    }
};
export const mapLinePatternToNumber = (pattern) => {
    if (!pattern)
        return 0;
    const normalizedPattern = pattern.toLowerCase();
    switch (normalizedPattern) {
        case 'solid':
            return 0;
        case 'dashed':
        case 'dash':
            return 1;
        case 'dotted':
        case 'dot':
            return 2;
        case 'dashdot':
        case 'dash-dot':
            return 3;
        default:
            return 0;
    }
};
export const mapFillPatternToNumber = (pattern) => {
    if (!pattern)
        return 0;
    const normalizedPattern = pattern.toLowerCase();
    switch (normalizedPattern) {
        case 'none':
        case 'transparent':
            return 0;
        case 'solid':
            return 1;
        case 'hachure':
        case 'diagonal':
            return 2;
        case 'cross-hatch':
        case 'cross':
            return 6;
        default:
            return 0;
    }
};
export const getStyleStatement = (style) => {
    let styleStatement = '';
    if (style.FillForeground && style.FillBackground && style.FillPattern !== undefined) {
        switch (style.FillPattern) {
            case 0:
                styleStatement += `fill: none,`;
                break;
            case 1:
                styleStatement += `fill: ${style.FillForeground},`;
                break;
            case 2:
                styleStatement += `background: repeating-linear-gradient(0deg, ${style.FillForeground}, ${style.FillForeground} 10px, ${style.FillBackground} 10px, ${style.FillBackground} 20px),`;
                break;
            case 6:
                styleStatement += `background: repeating-linear-gradient(45deg, ${style.FillForeground}, ${style.FillForeground} 10px, ${style.FillBackground} 10px, ${style.FillBackground} 20px),`;
                break;
            default:
                styleStatement += `fill: ${style.FillForeground},`;
        }
    }
    if (style.LineWeight && style.LineWeight > 2) {
        styleStatement += `stroke-width: ${Math.round(style.LineWeight)},`;
    }
    if (style.LineColor) {
        styleStatement += `stroke: ${style.LineColor},`;
    }
    if (style.LinePattern) {
        switch (style.LinePattern) {
            case 1:
                styleStatement += `stroke-dasharray: 5, 5,`;
                break;
            case 2:
                styleStatement += `stroke-dasharray: 1, 5,`;
                break;
            case 3:
                styleStatement += `stroke-dasharray: 5, 5, 1, 5,`;
                break;
        }
    }
    if (style.Rounding && style.Rounding > 0) {
        styleStatement += `border-radius: ${style.Rounding}px,`;
    }
    if (style.LineCap) {
        switch (style.LineCap) {
            case 0:
                styleStatement += `stroke-linecap: butt,`;
                break;
            case 1:
                styleStatement += `stroke-linecap: round,`;
                break;
            case 2:
                styleStatement += `stroke-linecap: square,`;
                break;
        }
    }
    if (style.FillForeground && styleStatement.indexOf('fill') === -1) {
        styleStatement += `fill: ${style.FillForeground},`;
    }
    return styleStatement.trim().replace(/,$/, '');
};
export const buildEdgeStatement = (start, end, style, text, sanitizeEdgeLabelFn) => {
    let startArrow = getArrow(style.BeginArrow);
    let endArrow = getArrow(style.EndArrow);
    const sanitizedText = sanitizeEdgeLabelFn(text);
    switch (startArrow) {
        case '>':
            startArrow = '<';
            break;
        case '&':
            startArrow = '';
    }
    if (endArrow === '&') {
        endArrow = '>';
    }
    let { startStroke, endStroke } = getStroke(style.LinePattern);
    if (startArrow === '' && sanitizedText === '') {
        startStroke = '';
    }
    if (startArrow === '<' && endArrow === '') {
        return `${end} ${endStroke}${sanitizedText}${startStroke}> ${start}`;
    }
    return `${start} ${startArrow}${startStroke}${sanitizedText}${endStroke}${endArrow} ${end}`;
};
export const getStroke = (linePattern) => {
    let startStroke = '--';
    let endStroke = '--';
    if (linePattern) {
        switch (linePattern) {
            case 2:
            case 3:
                startStroke = '-.';
                endStroke = '.-';
                break;
        }
    }
    return { startStroke, endStroke };
};
export function getArrow(arrow) {
    if (isNaN(arrow)) {
        return '&';
    }
    switch (arrow) {
        case 0:
            return '';
        case 1:
            return '>';
        case 2:
            return 'x';
        case 3:
            return 'o';
        case 4:
            return 'o';
        case 5:
            return '>';
        case 6:
            return 'o';
        case 7:
            return 'o';
        case 8:
            return '>';
        case 9:
            return '>';
        case 10:
            return '>';
        case 11:
            return 'o';
        case 12:
            return 'o';
        default:
            return '>';
    }
}
//# sourceMappingURL=styleUtils.js.map