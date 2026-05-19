import { DiagramDetector } from './DiagramDetector.js';
import { DiagramType } from './types.js';
export class DrawIODetector extends DiagramDetector {
    async analyzeDrawIOFile(buffer) {
        try {
            const xmlAnalysis = await this.analyzeXMLStructure(buffer);
            if (xmlAnalysis.confidence >= 80) {
                return xmlAnalysis;
            }
            return xmlAnalysis;
        }
        catch (error) {
            console.error('Error analyzing DrawIO file:', error);
            return {
                detectedType: DiagramType.UNKNOWN,
                confidence: 0,
                patterns: [],
                metadata: {
                    totalShapes: 0,
                    totalEdges: 0,
                    shapeTypes: [],
                    hasSpecializedShapes: false,
                    hasDirectionalFlow: false,
                    hasHierarchy: false,
                    hasTemporal: false,
                    hasDataModel: false,
                    hasNetworkElements: false,
                },
            };
        }
    }
    async analyzeXMLStructure(buffer) {
        const xmlContent = buffer.toString('utf-8');
        const patterns = {
            sequence: this.analyzeSequencePatterns(xmlContent),
            class: this.analyzeClassPatterns(xmlContent),
            state: this.analyzeStatePatterns(xmlContent),
            component: this.analyzeComponentPatterns(xmlContent),
            entityRelationship: this.analyzeERPatterns(xmlContent),
            network: this.analyzeNetworkPatterns(xmlContent),
            flowchart: this.analyzeFlowchartPatterns(xmlContent),
        };
        let bestMatch = { type: DiagramType.UNKNOWN, confidence: 0 };
        const detectedPatterns = [];
        for (const [patternName, result] of Object.entries(patterns)) {
            if (result.confidence > bestMatch.confidence) {
                bestMatch = {
                    type: result.type,
                    confidence: result.confidence,
                };
            }
            if (result.confidence > 0) {
                detectedPatterns.push({
                    type: result.type,
                    evidence: result.evidence,
                    weight: result.weight,
                    confidence: result.confidence,
                });
            }
        }
        return {
            detectedType: bestMatch.type,
            confidence: bestMatch.confidence,
            patterns: detectedPatterns,
            metadata: this.extractXMLMetadata(xmlContent),
        };
    }
    analyzeSequencePatterns(xml) {
        const evidence = [];
        let confidence = 0;
        if (xml.includes('shape=umlActor') || xml.includes('shape="umlActor"') || xml.includes("shape='umlActor'")) {
            evidence.push('Found UML actor shapes');
            confidence += 30;
        }
        if (xml.includes('targetShapes=umlLifeline') ||
            xml.includes('targetShapes="umlLifeline"') ||
            xml.includes("targetShapes='umlLifeline'") ||
            xml.includes('umlLifeline')) {
            evidence.push('Found UML lifeline shapes');
            confidence += 35;
        }
        const sequenceTerms = ['message', 'call', 'return', 'activate', 'deactivate', 'login', 'validate'];
        for (const term of sequenceTerms) {
            if (xml.toLowerCase().includes(term.toLowerCase())) {
                evidence.push(`Found sequence terminology: ${term}`);
                confidence += 8;
            }
        }
        if (xml.includes('chronologicallyOrdered')) {
            evidence.push('Found chronological ordering');
            confidence += 20;
        }
        if (xml.includes('endArrow=block') || xml.includes('endArrow=open')) {
            evidence.push('Found message arrows');
            confidence += 15;
        }
        if (xml.includes('dashed=1') || xml.includes('dashed="1"')) {
            evidence.push('Found return message patterns');
            confidence += 10;
        }
        return {
            type: DiagramType.SEQUENCE,
            confidence: Math.min(100, confidence),
            evidence,
            weight: confidence,
        };
    }
    analyzeClassPatterns(xml) {
        const evidence = [];
        let confidence = 0;
        if (xml.includes('shape=umlClass') || xml.includes('swimlane') || xml.includes('shape="swimlane"')) {
            evidence.push('Found UML class shapes');
            confidence += 40;
        }
        if (xml.includes('&lt;hr') && xml.includes('margin:0px')) {
            evidence.push('Found HTML-formatted class content');
            confidence += 35;
        }
        if ((xml.includes('|') || xml.includes('&vert;') || xml.includes('&lt;hr')) &&
            (xml.includes('+') || xml.includes('-') || xml.includes('#') || xml.includes('&plus;') || xml.includes('&minus;'))) {
            evidence.push('Found class attribute/method notation');
            confidence += 30;
        }
        if ((xml.includes('()') || xml.includes('&lpar;') || xml.includes('&rpar;')) &&
            (xml.includes(': ') ||
                xml.includes('boolean') ||
                xml.includes('string') ||
                xml.includes('int') ||
                xml.includes('void'))) {
            evidence.push('Found method notation with types');
            confidence += 25;
        }
        const dataTypes = ['int', 'string', 'boolean', 'decimal', 'datetime', 'void'];
        let typeCount = 0;
        for (const type of dataTypes) {
            if (xml.includes(type)) {
                typeCount++;
            }
        }
        if (typeCount >= 2) {
            evidence.push(`Found ${typeCount} data types`);
            confidence += 20;
        }
        if (xml.includes('endArrow=') && (xml.includes('triangle') || xml.includes('diamond') || xml.includes('block'))) {
            evidence.push('Found UML association arrows');
            confidence += 15;
        }
        if (xml.includes('1..*') || xml.includes('0..1') || xml.includes('0..*') || xml.includes('*')) {
            evidence.push('Found multiplicity notation');
            confidence += 15;
        }
        const classTerms = ['class', 'interface', 'abstract', 'extends', 'implements'];
        for (const term of classTerms) {
            if (xml.toLowerCase().includes(term)) {
                evidence.push(`Found class terminology: ${term}`);
                confidence += 5;
            }
        }
        return {
            type: DiagramType.CLASS,
            confidence: Math.min(100, confidence),
            evidence,
            weight: confidence,
        };
    }
    analyzeStatePatterns(xml) {
        const evidence = [];
        let confidence = 0;
        if (xml.includes('shape=startState') || xml.includes('shape=endState')) {
            evidence.push('Found start/end state shapes');
            confidence += 35;
        }
        if (xml.includes('rounded=1') || xml.includes('arcSize=')) {
            evidence.push('Found rounded state shapes');
            confidence += 25;
        }
        if (xml.includes('[') && xml.includes(']') && xml.includes('/')) {
            evidence.push('Found state transition notation');
            confidence += 30;
        }
        const stateTerms = ['idle', 'active', 'waiting', 'processing', 'transition'];
        for (const term of stateTerms) {
            if (xml.toLowerCase().includes(term)) {
                evidence.push(`Found state terminology: ${term}`);
                confidence += 5;
            }
        }
        return {
            type: DiagramType.STATE,
            confidence: Math.min(100, confidence),
            evidence,
            weight: confidence,
        };
    }
    analyzeComponentPatterns(xml) {
        const evidence = [];
        let confidence = 0;
        if (xml.includes('shape=component') || xml.includes('shape=module')) {
            evidence.push('Found component shapes');
            confidence += 40;
        }
        if (xml.includes('shape=ellipse') && xml.includes('interface')) {
            evidence.push('Found interface ellipses');
            confidence += 30;
        }
        if (xml.includes('&lt;&lt;') && xml.includes('&gt;&gt;')) {
            evidence.push('Found stereotype notation');
            confidence += 20;
        }
        if (xml.includes('dashed=1') || xml.includes('strokeDasharray')) {
            evidence.push('Found dependency relationships');
            confidence += 15;
        }
        return {
            type: DiagramType.COMPONENT,
            confidence: Math.min(100, confidence),
            evidence,
            weight: confidence,
        };
    }
    analyzeERPatterns(xml) {
        const evidence = [];
        let confidence = 0;
        const hasRectangles = xml.includes('shape=rectangle') || xml.includes('shape="rectangle"') || xml.includes('shape=table');
        const hasSpecializedShapes = xml.includes('umlActor') || xml.includes('umlClass') || xml.includes('component');
        if (hasRectangles && !hasSpecializedShapes) {
            evidence.push('Found entity rectangles');
            confidence += 35;
        }
        if (xml.includes('shape=rhombus') || xml.includes('shape="rhombus"') || xml.includes('shape=diamond')) {
            evidence.push('Found relationship diamonds');
            confidence += 35;
        }
        if ((xml.includes('shape=ellipse') || xml.includes('shape="ellipse"')) && !xml.includes('interface')) {
            evidence.push('Found attribute ellipses');
            confidence += 25;
        }
        if (xml.includes('1:1') || xml.includes('1:M') || xml.includes('M:N') || xml.includes('1:N')) {
            evidence.push('Found cardinality notation');
            confidence += 30;
        }
        const erTerms = ['entity', 'relationship', 'attribute', 'primary', 'foreign', 'key', 'table'];
        for (const term of erTerms) {
            if (xml.toLowerCase().includes(term)) {
                evidence.push(`Found ER terminology: ${term}`);
                confidence += 8;
            }
        }
        if (xml.includes('umlActor') || xml.includes('targetShapes=umlLifeline')) {
            confidence = Math.max(0, confidence - 30);
        }
        return {
            type: DiagramType.ENTITY_RELATIONSHIP,
            confidence: Math.min(100, confidence),
            evidence,
            weight: confidence,
        };
    }
    analyzeNetworkPatterns(xml) {
        const evidence = [];
        let confidence = 0;
        if (xml.includes('mxgraph.cisco') || xml.includes('cisco')) {
            evidence.push('Found Cisco network shapes');
            confidence += 40;
        }
        const networkDevices = ['router', 'switch', 'firewall', 'server', 'hub'];
        for (const device of networkDevices) {
            if (xml.toLowerCase().includes(device)) {
                evidence.push(`Found network device: ${device}`);
                confidence += 10;
            }
        }
        const ipPattern = /\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b/;
        if (ipPattern.test(xml)) {
            evidence.push('Found IP addresses');
            confidence += 30;
        }
        if (xml.toLowerCase().includes('vlan') || xml.toLowerCase().includes('subnet')) {
            evidence.push('Found VLAN/subnet terminology');
            confidence += 20;
        }
        return {
            type: DiagramType.NETWORK,
            confidence: Math.min(100, confidence),
            evidence,
            weight: confidence,
        };
    }
    analyzeFlowchartPatterns(xml) {
        const evidence = [];
        let confidence = 0;
        if (xml.includes('shape=diamond') || xml.includes('shape=rhombus')) {
            evidence.push('Found decision diamonds');
            confidence += 30;
        }
        if (xml.includes('shape=ellipse')) {
            evidence.push('Found start/end terminals');
            confidence += 25;
        }
        if (xml.includes('shape=rectangle')) {
            evidence.push('Found process rectangles');
            confidence += 20;
        }
        if (xml.includes('endArrow=') || xml.includes('arrow')) {
            evidence.push('Found directional flow');
            confidence += 15;
        }
        const hasSpecializedContent = xml.includes('uml') || xml.includes('cisco') || xml.includes('actor') || xml.includes('lifeline');
        if (hasSpecializedContent) {
            confidence = Math.max(0, confidence - 20);
        }
        return {
            type: DiagramType.FLOWCHART,
            confidence: Math.min(100, confidence),
            evidence,
            weight: confidence,
        };
    }
    extractXMLMetadata(xml) {
        const cellMatches = xml.match(/<mxCell/g) || [];
        const edgeMatches = xml.match(/edge="1"/g) || [];
        const styleMatches = xml.match(/style="[^"]*"/g) || [];
        const shapeTypes = styleMatches
            .map((style) => {
            const shapeMatch = style.match(/shape=([^;]+)/);
            return shapeMatch ? shapeMatch[1] : null;
        })
            .filter((shape) => shape !== null);
        return {
            totalShapes: cellMatches.length - edgeMatches.length,
            totalEdges: edgeMatches.length,
            shapeTypes: [...new Set(shapeTypes)],
            hasSpecializedShapes: xml.includes('uml') || xml.includes('cisco'),
            hasDirectionalFlow: xml.includes('endArrow=') || xml.includes('startArrow='),
            hasHierarchy: xml.includes('parent=') && !xml.includes('parent="1"'),
            hasTemporal: xml.toLowerCase().includes('sequence') || xml.toLowerCase().includes('time'),
            hasDataModel: xml.includes('table') || xml.includes('entity'),
            hasNetworkElements: xml.includes('cisco') || xml.includes('network'),
        };
    }
}
//# sourceMappingURL=DrawIODetector.js.map