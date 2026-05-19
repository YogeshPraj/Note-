import { MermaidShape } from './flowchartShapes.js';
export const mapBasicShapes = (value) => {
    switch (value) {
        case 'rounded':
        case 'rounded rectangle':
        case 'event':
            return MermaidShape.Rounded;
        case 'process':
        case 'rectangle':
            return MermaidShape.Rectangle;
        case 'diamond':
        case 'decision':
        case 'question':
        case 'diam':
            return MermaidShape.Diamond;
        case 'circle':
        case 'center drag circle':
        case 'circ':
            return MermaidShape.Circle;
        case 'triangle':
        case 'extract':
            return MermaidShape.Triangle;
        case 'hexagon':
        case 'prepare':
            return MermaidShape.Hexagon;
        case 'pill':
        case 'terminal':
        case 'start/end':
            return MermaidShape.Stadium;
        default:
            return null;
    }
};
export const mapDocumentShapes = (value) => {
    switch (value) {
        case 'doc':
        case 'document':
            return MermaidShape.Document;
        case 'lined-document':
            return MermaidShape.LinedDocument;
        case 'documents':
        case 'stacked-document':
            return MermaidShape.MultiDocument;
        case 'tagged-document':
            return MermaidShape.TaggedDocument;
        default:
            return null;
    }
};
export const mapStorageShapes = (value) => {
    switch (value) {
        case 'database':
        case 'cylinder':
        case 'can':
        case 'cyls':
            return MermaidShape.Database;
        case 'horizontal-cylinder':
        case 'das':
        case 'direct access storage':
            return MermaidShape.DirectAccessStorage;
        case 'disk storage':
        case 'linedcylinder':
            return MermaidShape.DiskStorage;
        case 'stored-data':
        case 'bowtie':
        case 'external data':
            return MermaidShape.StoredData;
        case 'internalstorage':
        case 'windowpane':
            return MermaidShape.InternalStorage;
        default:
            return null;
    }
};
export const mapProcessShapes = (value) => {
    switch (value) {
        case 'card':
        case 'custom 2':
            return MermaidShape.NotchedRect;
        case 'lined-process':
        case 'lined-rectangle':
        case 'shaded-rectangle':
        case 'shaded-process':
            return MermaidShape.LinedShadedRect;
        case 'processes':
        case 'stacked process':
            return MermaidShape.MultiRect;
        case 'tagged-process':
            return MermaidShape.TaggedRect;
        case 'subroutine':
        case 'subprocess':
        case 'framedRect':
            return MermaidShape.FramedRect;
        case 'divided-process':
        case 'dividedrectangle':
            return MermaidShape.DividedRect;
        default:
            return null;
    }
};
export const mapInputOutputShapes = (value) => {
    switch (value) {
        case 'in-out':
        case 'lean-right':
        case 'parallelogram':
        case 'data':
            return MermaidShape.DataInputOutput;
        case 'out-in':
        case 'lean-left':
            return MermaidShape.DataOutputInput;
        case 'manual-input':
        case 'slopedrect':
            return MermaidShape.ManualInput;
        case 'display':
        case 'curved-trapezoid':
            return MermaidShape.Display;
        default:
            return null;
    }
};
export const mapSpecializedShapes = (value) => {
    switch (value) {
        case 'delay':
            return MermaidShape.Delay;
        case 'collate':
        case 'hourglass':
            return MermaidShape.Collate;
        case 'priority':
        case 'trapezoid':
            return MermaidShape.Trapezoid;
        case 'manual':
        case 'inv_trapezoid':
            return MermaidShape.FlippedTrapezoid;
        case 'loop-limit':
        case 'trapezoidalpentagon':
            return MermaidShape.LoopLimit;
        case 'flag':
        case 'paper-tape':
            return MermaidShape.PaperTape;
        case 'bolt':
        case 'com-link':
            return MermaidShape.LightningBolt;
        default:
            return null;
    }
};
//# sourceMappingURL=shapeMappers.js.map