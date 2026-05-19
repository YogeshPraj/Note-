import { DiagramType } from './detection/types.js';
import { generateFlowchartDiagram } from './generators/flowchartGenerator.js';
import { generateClassDiagram } from './generators/classGenerator.js';
import { generateSequenceDiagram } from './generators/sequenceGenerator.js';
import { generateStateDiagram } from './generators/stateGenerator.js';
import { generateERDiagram } from './generators/erGenerator.js';
import { generateComponentDiagram } from './generators/componentGenerator.js';
import { generateNetworkDiagram } from './generators/networkGenerator.js';
import { generateGanttDiagram } from './generators/ganttGenerator.js';
import { generateMindmapDiagram } from './generators/mindmapGenerator.js';
import { generateTimelineDiagram } from './generators/timelineGenerator.js';
export const generateMermaidCode = (diagram) => {
    var _a;
    const diagramType = ((_a = diagram.Analysis) === null || _a === void 0 ? void 0 : _a.detectedType) || DiagramType.FLOWCHART;
    switch (diagramType) {
        case DiagramType.CLASS:
            return generateClassDiagram(diagram);
        case DiagramType.SEQUENCE:
            return generateSequenceDiagram(diagram);
        case DiagramType.STATE:
            return generateStateDiagram(diagram);
        case DiagramType.ENTITY_RELATIONSHIP:
            return generateERDiagram(diagram);
        case DiagramType.COMPONENT:
            return generateComponentDiagram(diagram);
        case DiagramType.NETWORK:
            return generateNetworkDiagram(diagram);
        case DiagramType.GANTT:
            return generateGanttDiagram(diagram);
        case DiagramType.MINDMAP:
            return generateMindmapDiagram(diagram);
        case DiagramType.TIMELINE:
            return generateTimelineDiagram(diagram);
        case DiagramType.FLOWCHART:
        default:
            return generateFlowchartDiagram(diagram);
    }
};
//# sourceMappingURL=Scribe.js.map