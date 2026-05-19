export var MermaidShape;
(function (MermaidShape) {
    MermaidShape["Rectangle"] = "rect";
    MermaidShape["Rounded"] = "rounded";
    MermaidShape["Stadium"] = "stadium";
    MermaidShape["FramedRect"] = "fr-rect";
    MermaidShape["MultiRect"] = "st-rect";
    MermaidShape["TaggedRect"] = "tag-rect";
    MermaidShape["LinedShadedRect"] = "lin-rect";
    MermaidShape["DividedRect"] = "div-rect";
    MermaidShape["NotchedRect"] = "notch-rect";
    MermaidShape["Database"] = "cyl";
    MermaidShape["DirectAccessStorage"] = "h-cyl";
    MermaidShape["DiskStorage"] = "lin-cyl";
    MermaidShape["DataInputOutput"] = "lean-r";
    MermaidShape["DataOutputInput"] = "lean-l";
    MermaidShape["Document"] = "doc";
    MermaidShape["LinedDocument"] = "lin-doc";
    MermaidShape["MultiDocument"] = "docs";
    MermaidShape["StoredData"] = "bow-rect";
    MermaidShape["TaggedDocument"] = "tag-doc";
    MermaidShape["Diamond"] = "diam";
    MermaidShape["Circle"] = "circ";
    MermaidShape["DoubleCircle"] = "dbl-circ";
    MermaidShape["SmallCircle"] = "sm-circ";
    MermaidShape["FramedCircle"] = "fr-circ";
    MermaidShape["CrossCircle"] = "cross-circ";
    MermaidShape["FilledCircle"] = "f-circ";
    MermaidShape["Odd"] = "odd";
    MermaidShape["Triangle"] = "tri";
    MermaidShape["RotatedTriangle"] = "flip-tri";
    MermaidShape["Hexagon"] = "hex";
    MermaidShape["Trapezoid"] = "trap-b";
    MermaidShape["FlippedTrapezoid"] = "trap-t";
    MermaidShape["LightningBolt"] = "bolt";
    MermaidShape["TextBlock"] = "text";
    MermaidShape["ForkJoin"] = "fork";
    MermaidShape["Collate"] = "hourglass";
    MermaidShape["Comment"] = "brace";
    MermaidShape["CommentRight"] = "brace-r";
    MermaidShape["Delay"] = "delay";
    MermaidShape["Display"] = "curv-trap";
    MermaidShape["InternalStorage"] = "win-pane";
    MermaidShape["LoopLimit"] = "notch-pent";
    MermaidShape["ManualInput"] = "sl-rect";
    MermaidShape["PaperTape"] = "flag";
})(MermaidShape || (MermaidShape = {}));
import { mapBasicShapes, mapDocumentShapes, mapStorageShapes, mapProcessShapes, mapInputOutputShapes, mapSpecializedShapes, } from './shapeMappers.js';
export const getMermaidShapeByValue = (shape) => {
    const value = shape.toLowerCase();
    const basicShape = mapBasicShapes(value);
    if (basicShape)
        return basicShape;
    const documentShape = mapDocumentShapes(value);
    if (documentShape)
        return documentShape;
    const storageShape = mapStorageShapes(value);
    if (storageShape)
        return storageShape;
    const processShape = mapProcessShapes(value);
    if (processShape)
        return processShape;
    const ioShape = mapInputOutputShapes(value);
    if (ioShape)
        return ioShape;
    const specializedShape = mapSpecializedShapes(value);
    if (specializedShape)
        return specializedShape;
    switch (value) {
        case 'manual-file':
        case 'rotated triangle':
            return MermaidShape.RotatedTriangle;
        case 'doublecircle':
            return MermaidShape.DoubleCircle;
        case 'odd':
            return MermaidShape.Odd;
        case 'small circle':
        case 'start':
        case 'on-page reference':
            return MermaidShape.SmallCircle;
        case 'stop':
        case 'framed-circle':
            return MermaidShape.FramedCircle;
        case 'forkjoin':
            return MermaidShape.ForkJoin;
        case 'comment left':
        case 'left brace':
            return MermaidShape.Comment;
        case 'comment right':
        case 'right brace':
            return MermaidShape.CommentRight;
        case 'junction':
        case 'filledcircle':
            return MermaidShape.FilledCircle;
        case 'summary':
        case 'crossedcircle':
            return MermaidShape.CrossCircle;
    }
    return MermaidShape.Rectangle;
};
export var ArrowType;
(function (ArrowType) {
    ArrowType["None"] = "none";
    ArrowType["ArrowCross"] = "arrow_cross";
    ArrowType["DoubleArrowCross"] = "double_arrow_cross";
    ArrowType["ArrowPoint"] = "arrow_point";
    ArrowType["DoubleArrowPoint"] = "double_arrow_point";
    ArrowType["ArrowCircle"] = "arrow_circle";
    ArrowType["DoubleArrowCircle"] = "double_arrow_circle";
})(ArrowType || (ArrowType = {}));
//# sourceMappingURL=flowchartShapes.js.map