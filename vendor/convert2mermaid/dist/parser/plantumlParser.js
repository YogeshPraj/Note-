import * as fs from 'fs';
import { getMermaidShapeByValue } from '../shapes/flowchartShapes.js';
const generateNodeId = (originalId) => {
    return originalId.replace(/[^a-zA-Z0-9]/g, '_').replace(/^[0-9]/, 'n$&');
};
const parseArrowType = (arrow) => {
    if (arrow.includes('..>') || arrow.includes('..|>'))
        return { style: 'dotted', arrowType: 1 };
    if (arrow.includes('-->') || arrow.includes('--|>'))
        return { style: 'solid', arrowType: 1 };
    if (arrow.includes('->'))
        return { style: 'solid', arrowType: 1 };
    if (arrow.includes('<->'))
        return { style: 'solid', arrowType: 1 };
    if (arrow.includes('<--'))
        return { style: 'solid', arrowType: 1 };
    return { style: 'solid', arrowType: 1 };
};
function createDefaultStyle() {
    return {
        FillForeground: '#ffffff',
        FillBackground: '#ffffff',
        TextColor: '#000000',
        LineWeight: 1,
        LineColor: '#000000',
        LinePattern: 1,
        Rounding: 0,
        BeginArrow: 0,
        BeginArrowSize: 1,
        EndArrow: 0,
        EndArrowSize: 1,
        LineCap: 0,
        FillPattern: 1,
    };
}
function parseClassDiagram(lines) {
    var _a;
    const diagram = {
        elements: [],
        connections: [],
        classes: [],
        components: [],
        states: [],
        usecases: [],
        ganttTasks: [],
        relationships: [],
        diagramType: 'class',
    };
    let currentClass = null;
    let inClassBody = false;
    for (const line of lines) {
        if (line.startsWith('title ')) {
            diagram.title = line.substring(6).trim();
            continue;
        }
        const classMatch = line.match(/^(class|interface|abstract\s+class|enum)\s+(\w+)\s*\{?/);
        if (classMatch) {
            const stereotype = classMatch[1].includes('interface')
                ? 'interface'
                : classMatch[1].includes('abstract')
                    ? 'abstract'
                    : classMatch[1].includes('enum')
                        ? 'enumeration'
                        : undefined;
            currentClass = {
                name: classMatch[2],
                stereotype: stereotype,
                attributes: [],
                methods: [],
            };
            if (line.includes('{')) {
                inClassBody = true;
            }
            continue;
        }
        if (inClassBody && currentClass) {
            if (line === '}') {
                diagram.classes.push(currentClass);
                currentClass = null;
                inClassBody = false;
                continue;
            }
            if (line === '--' || line === '==') {
                continue;
            }
            if (line.includes('(') && line.includes(')')) {
                currentClass.methods.push(line);
            }
            else if (line.trim()) {
                currentClass.attributes.push(line);
            }
            continue;
        }
        const relationMatch = line.match(/^(\w+)\s+(--|>|\.\.|\|>|<\||\*--|o--)\s*(\w+)(?:\s*:\s*(.+))?/);
        if (relationMatch) {
            diagram.relationships.push({
                from: relationMatch[1],
                to: relationMatch[3],
                type: relationMatch[2],
                label: (_a = relationMatch[4]) === null || _a === void 0 ? void 0 : _a.trim(),
            });
            continue;
        }
        const cardinalityMatch = line.match(/^(\w+)\s+"([^"]+)"\s+(--|\.\.|--\*|\*--)\s+"([^"]+)"\s+(\w+)(?:\s*:\s*(.+))?/);
        if (cardinalityMatch) {
            diagram.relationships.push({
                from: cardinalityMatch[1],
                to: cardinalityMatch[5],
                type: cardinalityMatch[3],
                label: `${cardinalityMatch[2]}:${cardinalityMatch[4]}${cardinalityMatch[6] ? ' ' + cardinalityMatch[6] : ''}`,
            });
        }
    }
    return diagram;
}
function parseComponentDiagram(lines) {
    var _a;
    const diagram = {
        elements: [],
        connections: [],
        classes: [],
        components: [],
        states: [],
        usecases: [],
        ganttTasks: [],
        relationships: [],
        diagramType: 'component',
    };
    let currentPackage = null;
    for (const line of lines) {
        if (line.startsWith('title ')) {
            diagram.title = line.substring(6).trim();
            continue;
        }
        const packageMatch = line.match(/^package\s+"([^"]+)"\s*\{/);
        if (packageMatch) {
            currentPackage = packageMatch[1];
            diagram.components.push({
                name: packageMatch[1],
                type: 'package',
            });
            continue;
        }
        if (line === '}' && currentPackage) {
            currentPackage = null;
            continue;
        }
        const componentMatch = line.match(/^\[([^\]]+)\](?:\s+as\s+(\w+))?/);
        if (componentMatch) {
            diagram.components.push({
                name: componentMatch[1],
                alias: componentMatch[2] || componentMatch[1],
                type: 'component',
            });
            continue;
        }
        const databaseMatch = line.match(/^database\s+"([^"]+)"\s*\{?/);
        if (databaseMatch) {
            diagram.components.push({
                name: databaseMatch[1],
                type: 'database',
            });
            continue;
        }
        const connectionMatch = line.match(/^(\w+)\s+(-->|->|\.\.>)\s+(\w+)(?:\s*:\s*(.+))?/);
        if (connectionMatch) {
            diagram.relationships.push({
                from: connectionMatch[1],
                to: connectionMatch[3],
                type: connectionMatch[2],
                label: (_a = connectionMatch[4]) === null || _a === void 0 ? void 0 : _a.trim(),
            });
        }
    }
    return diagram;
}
function parseStateDiagram(lines) {
    var _a;
    const diagram = {
        elements: [],
        connections: [],
        classes: [],
        components: [],
        states: [],
        usecases: [],
        ganttTasks: [],
        relationships: [],
        diagramType: 'state',
    };
    for (const line of lines) {
        if (line.startsWith('title ')) {
            diagram.title = line.substring(6).trim();
            continue;
        }
        const stateDescMatch = line.match(/^(\w+)\s*:\s*(.+)/);
        if (stateDescMatch && !line.includes('-->') && !line.includes('[*]')) {
            const existingState = diagram.states.find((s) => s.name === stateDescMatch[1]);
            if (existingState) {
                existingState.description = stateDescMatch[2];
            }
            else {
                diagram.states.push({
                    name: stateDescMatch[1],
                    description: stateDescMatch[2],
                    isInitial: false,
                    isFinal: false,
                });
            }
            continue;
        }
        const transitionMatch = line.match(/^(\[?\*?\]?|\w+)\s+-->\s+(\[?\*?\]?|\w+)(?:\s*:\s*(.+))?/);
        if (transitionMatch) {
            const from = transitionMatch[1];
            const to = transitionMatch[2];
            const label = (_a = transitionMatch[3]) === null || _a === void 0 ? void 0 : _a.trim();
            if (from === '[*]') {
                const state = diagram.states.find((s) => s.name === to);
                if (state) {
                    state.isInitial = true;
                }
                else {
                    diagram.states.push({
                        name: to,
                        isInitial: true,
                        isFinal: false,
                    });
                }
            }
            if (to === '[*]') {
                const state = diagram.states.find((s) => s.name === from);
                if (state) {
                    state.isFinal = true;
                }
            }
            if (from !== '[*]' && to !== '[*]') {
                if (!diagram.states.find((s) => s.name === from)) {
                    diagram.states.push({ name: from, isInitial: false, isFinal: false });
                }
                if (!diagram.states.find((s) => s.name === to)) {
                    diagram.states.push({ name: to, isInitial: false, isFinal: false });
                }
            }
            diagram.relationships.push({
                from: from,
                to: to,
                type: '-->',
                label: label,
            });
        }
    }
    return diagram;
}
function parseUseCaseDiagram(lines) {
    var _a;
    const diagram = {
        elements: [],
        connections: [],
        classes: [],
        components: [],
        states: [],
        usecases: [],
        ganttTasks: [],
        relationships: [],
        diagramType: 'usecase',
    };
    for (const line of lines) {
        if (line.startsWith('title ')) {
            diagram.title = line.substring(6).trim();
            continue;
        }
        const actorMatch = line.match(/^actor\s+"([^"]+)"\s+as\s+(\w+)/);
        if (actorMatch) {
            diagram.usecases.push({
                name: actorMatch[1],
                alias: actorMatch[2],
                type: 'actor',
            });
            continue;
        }
        if (line.match(/^rectangle\s+"([^"]+)"\s*\{/)) {
            continue;
        }
        if (line === '}') {
            continue;
        }
        const usecaseMatch = line.match(/^usecase\s+"([^"]+)"\s+as\s+(\w+)/);
        if (usecaseMatch) {
            diagram.usecases.push({
                name: usecaseMatch[1],
                alias: usecaseMatch[2],
                type: 'usecase',
            });
            continue;
        }
        const relationMatch = line.match(/^(\w+)\s+(-->|\.\.>)\s+(\w+)(?:\s*:\s*(.+))?/);
        if (relationMatch) {
            diagram.relationships.push({
                from: relationMatch[1],
                to: relationMatch[3],
                type: relationMatch[2],
                label: (_a = relationMatch[4]) === null || _a === void 0 ? void 0 : _a.trim(),
            });
        }
    }
    return diagram;
}
function parseGanttDiagram(lines) {
    const diagram = {
        elements: [],
        connections: [],
        classes: [],
        components: [],
        states: [],
        usecases: [],
        ganttTasks: [],
        relationships: [],
        diagramType: 'gantt',
    };
    for (const line of lines) {
        if (line.startsWith('title ') || line.startsWith('Project starts ')) {
            diagram.title = line.replace('title ', '').replace('Project starts ', '').trim();
            continue;
        }
        const taskMatch = line.match(/^\[([^\]]+)\](?:\s+lasts\s+(\d+)\s+days?)?/);
        if (taskMatch) {
            diagram.ganttTasks.push({
                name: taskMatch[1],
                duration: taskMatch[2] ? `${taskMatch[2]}d` : '1d',
                milestone: false,
            });
            continue;
        }
        const milestoneMatch = line.match(/^\[([^\]]+)\]\s+happens/);
        if (milestoneMatch) {
            diagram.ganttTasks.push({
                name: milestoneMatch[1],
                milestone: true,
            });
        }
    }
    return diagram;
}
function parseSequenceOrActivityDiagram(lines) {
    var _a, _b;
    const diagram = {
        elements: [],
        connections: [],
        classes: [],
        components: [],
        states: [],
        usecases: [],
        ganttTasks: [],
        relationships: [],
        diagramType: 'flowchart',
    };
    let lastActivityId = null;
    let decisionStack = [];
    for (const line of lines) {
        if (line.startsWith('title ')) {
            diagram.title = line.substring(6).trim();
            continue;
        }
        if (line.includes('actor') || line.includes('participant') || line.includes('database')) {
            diagram.diagramType = 'sequence';
        }
        else if (line.includes('start') || line.includes('stop') || line.includes('if (') || line.includes('endif')) {
            diagram.diagramType = 'activity';
        }
        if (line.startsWith('actor ')) {
            const parts = line.substring(6).trim().split(' as ');
            const id = parts.length > 1 ? parts[1].replace(/"/g, '') : parts[0];
            const label = parts[0].replace(/"/g, '');
            diagram.elements.push({
                type: 'actor',
                id: id,
                label: label,
                alias: parts.length > 1 ? parts[1] : undefined,
            });
            continue;
        }
        if (line.startsWith('participant ')) {
            const parts = line.substring(12).trim().split(' as ');
            const label = parts[0].replace(/"/g, '');
            const id = parts.length > 1 ? parts[1].replace(/"/g, '') : label;
            diagram.elements.push({
                type: 'participant',
                id: id,
                label: label,
                alias: parts.length > 1 ? parts[1] : undefined,
            });
            continue;
        }
        if (line.startsWith('database ')) {
            const parts = line.substring(9).trim().split(' as ');
            const label = parts[0].replace(/"/g, '');
            const id = parts.length > 1 ? parts[1].replace(/"/g, '') : label;
            diagram.elements.push({
                type: 'database',
                id: id,
                label: label,
                alias: parts.length > 1 ? parts[1] : undefined,
            });
            continue;
        }
        if (line === 'start') {
            diagram.elements.push({ type: 'start', id: 'start', label: 'Start' });
            lastActivityId = 'start';
            continue;
        }
        if (line === 'stop') {
            diagram.elements.push({ type: 'stop', id: 'stop', label: 'Stop' });
            if (lastActivityId) {
                diagram.connections.push({
                    from: lastActivityId,
                    to: 'stop',
                    arrow: '->',
                    style: 'solid',
                });
            }
            continue;
        }
        if (line.startsWith(':') && line.endsWith(';')) {
            const label = line.substring(1, line.length - 1);
            const id = `activity_${diagram.elements.filter((e) => e.type === 'activity').length}`;
            diagram.elements.push({ type: 'activity', id: id, label: label });
            if (lastActivityId) {
                diagram.connections.push({
                    from: lastActivityId,
                    to: id,
                    arrow: '->',
                    style: 'solid',
                });
            }
            lastActivityId = id;
            continue;
        }
        if (line.startsWith('if (') && line.includes(') then')) {
            const condition = ((_a = line.match(/if \(([^)]+)\)/)) === null || _a === void 0 ? void 0 : _a[1]) || 'condition';
            const id = `decision_${diagram.elements.filter((e) => e.type === 'decision').length}`;
            diagram.elements.push({ type: 'decision', id: id, label: condition });
            if (lastActivityId) {
                diagram.connections.push({
                    from: lastActivityId,
                    to: id,
                    arrow: '->',
                    style: 'solid',
                });
            }
            decisionStack.push(id);
            lastActivityId = id;
            continue;
        }
        if (line.startsWith('else (')) {
            continue;
        }
        if (line === 'endif') {
            if (decisionStack.length > 0) {
                decisionStack.pop();
            }
            continue;
        }
        const arrowPatterns = [
            /^(.+?)\s*(-->|->|<->|<--|\.\.>)\s*(.+?):\s*(.+)$/,
            /^(.+?)\s*(-->|->|<->|<--|\.\.>)\s*(.+?)$/,
        ];
        for (const pattern of arrowPatterns) {
            const match = line.match(pattern);
            if (match) {
                const from = match[1].trim();
                const arrow = match[2];
                const to = match[3].trim();
                const label = ((_b = match[4]) === null || _b === void 0 ? void 0 : _b.trim()) || '';
                const arrowInfo = parseArrowType(arrow);
                diagram.connections.push({
                    from: from,
                    to: to,
                    label: label,
                    arrow: arrow,
                    style: arrowInfo.style,
                });
                break;
            }
        }
    }
    return diagram;
}
function detectDiagramType(lines) {
    const content = lines.join('\n').toLowerCase();
    if (content.includes('@startgantt'))
        return 'gantt';
    if (content.includes('@startuml') && content.includes('class '))
        return 'class';
    if (content.includes('usecase'))
        return 'usecase';
    if (content.includes('@startcomponent'))
        return 'component';
    if (content.includes('state ') || content.includes('[*] -->'))
        return 'state';
    if (content.includes('participant '))
        return 'sequence';
    if (content.includes('start') && content.includes('stop'))
        return 'activity';
    if (content.includes('package ') || (content.includes('[') && content.includes('] -->')))
        return 'component';
    if (content.includes('actor ') && content.includes('->'))
        return 'sequence';
    return 'flowchart';
}
function buildClassLabel(cls) {
    let label = '';
    if (cls.stereotype) {
        label += `<<${cls.stereotype}>>\n`;
    }
    label += cls.name;
    if (cls.attributes.length > 0) {
        label += '\n---\n' + cls.attributes.join('\n');
    }
    if (cls.methods.length > 0) {
        label += '\n---\n' + cls.methods.join('\n');
    }
    return label;
}
function convertClassDiagramToShapes(diagram) {
    const shapes = [];
    diagram.classes.forEach((cls) => {
        const label = buildClassLabel(cls);
        shapes.push({
            Id: generateNodeId(cls.name),
            ShapeType: getMermaidShapeByValue('rectangle'),
            Label: label,
            Style: createDefaultStyle(),
            IsEdge: false,
        });
    });
    diagram.relationships.forEach((rel) => {
        const arrowInfo = parseArrowType(rel.type);
        shapes.push({
            Id: `${generateNodeId(rel.from)}_to_${generateNodeId(rel.to)}`,
            ShapeType: 'line',
            Label: rel.label || '',
            Style: {
                ...createDefaultStyle(),
                LinePattern: arrowInfo.style === 'dotted' ? 3 : 1,
                EndArrow: 1,
            },
            IsEdge: true,
            FromNode: generateNodeId(rel.from),
            ToNode: generateNodeId(rel.to),
        });
    });
    return shapes;
}
function convertComponentDiagramToShapes(diagram) {
    const shapes = [];
    diagram.components.forEach((comp) => {
        const shapeType = comp.type === 'database'
            ? getMermaidShapeByValue('database')
            : comp.type === 'package'
                ? getMermaidShapeByValue('rectangle')
                : getMermaidShapeByValue('rectangle');
        shapes.push({
            Id: generateNodeId(comp.alias || comp.name),
            ShapeType: shapeType,
            Label: comp.name,
            Style: createDefaultStyle(),
            IsEdge: false,
        });
    });
    diagram.relationships.forEach((rel) => {
        const arrowInfo = parseArrowType(rel.type);
        shapes.push({
            Id: `${generateNodeId(rel.from)}_to_${generateNodeId(rel.to)}`,
            ShapeType: 'line',
            Label: rel.label || '',
            Style: {
                ...createDefaultStyle(),
                LinePattern: arrowInfo.style === 'dotted' ? 3 : 1,
                EndArrow: 1,
            },
            IsEdge: true,
            FromNode: generateNodeId(rel.from),
            ToNode: generateNodeId(rel.to),
        });
    });
    return shapes;
}
function convertStateDiagramToShapes(diagram) {
    const shapes = [];
    diagram.states.forEach((state) => {
        const shapeType = state.isInitial || state.isFinal ? getMermaidShapeByValue('circle') : getMermaidShapeByValue('rectangle');
        const label = state.description ? `${state.name}\\n${state.description}` : state.name;
        shapes.push({
            Id: generateNodeId(state.name),
            ShapeType: shapeType,
            Label: label,
            Style: createDefaultStyle(),
            IsEdge: false,
        });
    });
    diagram.relationships.forEach((rel) => {
        if (rel.from === '[*]' || rel.to === '[*]')
            return;
        shapes.push({
            Id: `${generateNodeId(rel.from)}_to_${generateNodeId(rel.to)}`,
            ShapeType: 'line',
            Label: rel.label || '',
            Style: createDefaultStyle(),
            IsEdge: true,
            FromNode: generateNodeId(rel.from),
            ToNode: generateNodeId(rel.to),
        });
    });
    return shapes;
}
function convertUseCaseDiagramToShapes(diagram) {
    const shapes = [];
    diagram.usecases.forEach((uc) => {
        const shapeType = uc.type === 'actor' ? getMermaidShapeByValue('rectangle') : getMermaidShapeByValue('ellipse');
        shapes.push({
            Id: generateNodeId(uc.alias || uc.name),
            ShapeType: shapeType,
            Label: uc.name,
            Style: createDefaultStyle(),
            IsEdge: false,
        });
    });
    diagram.relationships.forEach((rel) => {
        const arrowInfo = parseArrowType(rel.type);
        shapes.push({
            Id: `${generateNodeId(rel.from)}_to_${generateNodeId(rel.to)}`,
            ShapeType: 'line',
            Label: rel.label || '',
            Style: {
                ...createDefaultStyle(),
                LinePattern: arrowInfo.style === 'dotted' ? 3 : 1,
                EndArrow: 1,
            },
            IsEdge: true,
            FromNode: generateNodeId(rel.from),
            ToNode: generateNodeId(rel.to),
        });
    });
    return shapes;
}
function convertGanttDiagramToShapes(diagram) {
    const shapes = [];
    diagram.ganttTasks.forEach((task, index) => {
        const shapeType = task.milestone ? getMermaidShapeByValue('diamond') : getMermaidShapeByValue('rectangle');
        shapes.push({
            Id: generateNodeId(`task_${index}`),
            ShapeType: shapeType,
            Label: task.name,
            Style: createDefaultStyle(),
            IsEdge: false,
        });
    });
    return shapes;
}
function convertSequenceActivityToShapes(diagram) {
    const shapes = [];
    diagram.elements.forEach((element) => {
        let shapeType = getMermaidShapeByValue('rectangle');
        switch (element.type) {
            case 'actor':
            case 'participant':
            case 'entity':
            case 'boundary':
                shapeType = getMermaidShapeByValue('rectangle');
                break;
            case 'database':
                shapeType = getMermaidShapeByValue('database');
                break;
            case 'control':
            case 'start':
            case 'stop':
                shapeType = getMermaidShapeByValue('circle');
                break;
            case 'decision':
                shapeType = getMermaidShapeByValue('diamond');
                break;
            default:
                shapeType = getMermaidShapeByValue('rectangle');
        }
        shapes.push({
            Id: generateNodeId(element.id),
            ShapeType: shapeType,
            Label: element.label,
            Style: createDefaultStyle(),
            IsEdge: false,
        });
    });
    diagram.connections.forEach((conn) => {
        const arrowInfo = parseArrowType(conn.arrow);
        shapes.push({
            Id: `${generateNodeId(conn.from)}_to_${generateNodeId(conn.to)}`,
            ShapeType: 'line',
            Label: conn.label || '',
            Style: {
                ...createDefaultStyle(),
                LinePattern: conn.style === 'dashed' ? 2 : conn.style === 'dotted' ? 3 : 1,
                EndArrow: arrowInfo.arrowType,
            },
            IsEdge: true,
            FromNode: generateNodeId(conn.from),
            ToNode: generateNodeId(conn.to),
        });
    });
    return shapes;
}
function convertToShapes(diagram) {
    switch (diagram.diagramType) {
        case 'class':
            return convertClassDiagramToShapes(diagram);
        case 'component':
            return convertComponentDiagramToShapes(diagram);
        case 'state':
            return convertStateDiagramToShapes(diagram);
        case 'usecase':
            return convertUseCaseDiagramToShapes(diagram);
        case 'gantt':
            return convertGanttDiagramToShapes(diagram);
        case 'sequence':
        case 'activity':
        case 'flowchart':
        default:
            return convertSequenceActivityToShapes(diagram);
    }
}
function parsePlantUMLContent(content) {
    const lines = content
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.length > 0 && !line.startsWith("'") && !line.startsWith('!'));
    const filteredLines = lines.filter((line) => !line.startsWith('@start') && !line.startsWith('@end'));
    const diagramType = detectDiagramType(lines);
    let diagram;
    switch (diagramType) {
        case 'class':
            diagram = parseClassDiagram(filteredLines);
            break;
        case 'component':
            diagram = parseComponentDiagram(filteredLines);
            break;
        case 'state':
            diagram = parseStateDiagram(filteredLines);
            break;
        case 'usecase':
            diagram = parseUseCaseDiagram(filteredLines);
            break;
        case 'gantt':
            diagram = parseGanttDiagram(filteredLines);
            break;
        case 'sequence':
        case 'activity':
        case 'flowchart':
        default:
            diagram = parseSequenceOrActivityDiagram(filteredLines);
            break;
    }
    return diagram;
}
export async function parseDiagram(filepath) {
    try {
        const content = fs.readFileSync(filepath, 'utf-8');
        const plantUMLDiagram = parsePlantUMLContent(content);
        const shapes = convertToShapes(plantUMLDiagram);
        return {
            Shapes: shapes,
            Settings: plantUMLDiagram.title,
        };
    }
    catch (error) {
        console.error(`Error parsing PlantUML file: ${error}`);
        return undefined;
    }
}
//# sourceMappingURL=plantumlParser.js.map