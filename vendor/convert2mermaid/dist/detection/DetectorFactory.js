import { DiagramDetector } from './DiagramDetector.js';
import { DrawIODetector } from './DrawIODetector.js';
import { PlantUMLDetector } from './PlantUMLDetector.js';
import { DiagramType } from './types.js';
import * as path from 'path';
export class DetectorFactory {
    static createDetector(filePath) {
        const extension = path.extname(filePath).toLowerCase();
        switch (extension) {
            case '.drawio':
                return new DrawIODetector();
            case '.puml':
            case '.plantuml':
                return new PlantUMLDetector();
            case '.vsdx':
            case '.excalidraw':
            default:
                return new DiagramDetector();
        }
    }
    static async analyzeFile(filePath, buffer) {
        const extension = path.extname(filePath).toLowerCase();
        try {
            if (!buffer) {
                const fs = await import('fs');
                buffer = fs.readFileSync(filePath);
            }
            switch (extension) {
                case '.drawio':
                    const drawioDetector = new DrawIODetector();
                    return await drawioDetector.analyzeDrawIOFile(buffer);
                case '.puml':
                case '.plantuml':
                    const plantumlDetector = new PlantUMLDetector();
                    return plantumlDetector.analyzePlantUMLFile(buffer);
                case '.vsdx':
                case '.excalidraw':
                default:
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
        catch (error) {
            console.error(`Error analyzing file ${filePath}:`, error);
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
    static analyzeShapes(shapes) {
        const detector = new DiagramDetector();
        return detector.analyze(shapes);
    }
}
//# sourceMappingURL=DetectorFactory.js.map