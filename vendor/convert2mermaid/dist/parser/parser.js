import { parseVisioFile } from 'vsdx-js';
import * as drawioParser from './drawioParser.js';
import * as excalidrawParser from './excalidrawParser.js';
import * as plantumlParser from './plantumlParser.js';
import { DetectorFactory } from '../detection/DetectorFactory.js';
const convertVisioShapeToShape = (visioShape) => {
    return {
        Id: visioShape.Id,
        ShapeType: visioShape.Type,
        Label: visioShape.Label,
        Style: {
            FillForeground: visioShape.Style.FillForeground,
            FillBackground: visioShape.Style.FillBackground,
            TextColor: visioShape.Style.TextColor,
            LineWeight: visioShape.Style.LineWeight,
            LineColor: visioShape.Style.LineColor,
            LinePattern: visioShape.Style.LinePattern,
            Rounding: visioShape.Style.Rounding,
            BeginArrow: visioShape.Style.BeginArrow,
            BeginArrowSize: visioShape.Style.BeginArrowSize,
            EndArrow: visioShape.Style.EndArrow,
            EndArrowSize: visioShape.Style.EndArrowSize,
            LineCap: visioShape.Style.LineCap,
            FillPattern: visioShape.Style.FillPattern,
        },
        IsEdge: visioShape.IsEdge,
        FromNode: visioShape.FromNode,
        ToNode: visioShape.ToNode,
    };
};
export async function parseData(filepath) {
    let diagram = undefined;
    try {
        const extension = filepath.split('.').pop();
        switch (extension) {
            case 'vsdx': {
                const visioFile = await parseVisioFile(filepath);
                if (visioFile.Pages && visioFile.Pages.length > 0) {
                    const firstPage = visioFile.Pages[0];
                    const convertedShapes = firstPage.Shapes.map(convertVisioShapeToShape);
                    diagram = {
                        Shapes: convertedShapes,
                    };
                }
                break;
            }
            case 'drawio': {
                diagram = await drawioParser.parseDiagram(filepath);
                break;
            }
            case 'excalidraw': {
                diagram = await excalidrawParser.parseDiagram(filepath);
                break;
            }
            case 'puml':
            case 'plantuml': {
                diagram = await plantumlParser.parseDiagram(filepath);
                break;
            }
            default: {
                console.log(`Failed to find parser for ${filepath}`);
            }
        }
        if (diagram && diagram.Shapes.length > 0) {
            try {
                diagram.Analysis = await DetectorFactory.analyzeFile(filepath);
                console.log(`Detected diagram type: ${diagram.Analysis.detectedType} (confidence: ${diagram.Analysis.confidence}%)`);
            }
            catch (detectionError) {
                console.warn('Could not analyze diagram type:', detectionError);
                diagram.Analysis = DetectorFactory.analyzeShapes(diagram.Shapes);
            }
        }
    }
    catch (error) {
        console.error(`Error parsing file: ${error}`);
    }
    return diagram;
}
//# sourceMappingURL=parser.js.map