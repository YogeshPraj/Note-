// =============================================================================
// Note++ — draw.io starter templates
// =============================================================================
// Extracted from renderer.js. Pure data + a one-line shim — zero global
// state, zero DOM dependencies. Loaded as a classic <script> before
// renderer.js so DRAWIO_TEMPLATES + createDrawioTabFromTemplate end up on
// the global scope and the menu wiring in renderer.js needs no change.
//
// Each template is a minimal but well-formed mxfile XML so drawio renders
// the scaffold immediately. Kept intentionally small (≤ ~8 shapes per
// template) — the goal is a head-start, not a finished diagram. Users
// looking for a richer gallery can use draw.io's own File → New menu
// inside the iframe (100+ templates across BPMN, AWS, Azure, etc.).
// =============================================================================

const DRAWIO_TEMPLATES = {
  flowchart:
    '<mxfile host="notepp" version="1"><diagram id="flow" name="Flowchart">' +
      '<mxGraphModel dx="900" dy="700" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">' +
        '<root>' +
          '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
          '<mxCell id="s" value="Start" style="ellipse;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;" vertex="1" parent="1"><mxGeometry x="360" y="40" width="120" height="50" as="geometry"/></mxCell>' +
          '<mxCell id="p" value="Process step" style="rounded=0;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1"><mxGeometry x="360" y="140" width="120" height="50" as="geometry"/></mxCell>' +
          '<mxCell id="d" value="Decision?" style="rhombus;whiteSpace=wrap;html=1;fillColor=#fff2cc;strokeColor=#d6b656;" vertex="1" parent="1"><mxGeometry x="360" y="240" width="120" height="80" as="geometry"/></mxCell>' +
          '<mxCell id="e" value="End" style="ellipse;whiteSpace=wrap;html=1;fillColor=#f8cecc;strokeColor=#b85450;" vertex="1" parent="1"><mxGeometry x="360" y="380" width="120" height="50" as="geometry"/></mxCell>' +
          '<mxCell id="e1" style="edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;exitX=0.5;exitY=1;entryX=0.5;entryY=0;" edge="1" parent="1" source="s" target="p"><mxGeometry relative="1" as="geometry"/></mxCell>' +
          '<mxCell id="e2" style="edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;exitX=0.5;exitY=1;entryX=0.5;entryY=0;" edge="1" parent="1" source="p" target="d"><mxGeometry relative="1" as="geometry"/></mxCell>' +
          '<mxCell id="e3" value="Yes" style="edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;exitX=0.5;exitY=1;entryX=0.5;entryY=0;" edge="1" parent="1" source="d" target="e"><mxGeometry relative="1" as="geometry"/></mxCell>' +
        '</root>' +
      '</mxGraphModel>' +
    '</diagram></mxfile>',

  sequence:
    '<mxfile host="notepp" version="1"><diagram id="seq" name="Sequence">' +
      '<mxGraphModel dx="900" dy="700" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">' +
        '<root>' +
          '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
          '<mxCell id="a" value="Actor A" style="shape=umlLifeline;perimeter=lifelinePerimeter;whiteSpace=wrap;html=1;container=1;collapsible=0;recursiveResize=0;outlineConnect=0;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1"><mxGeometry x="120" y="40" width="120" height="360" as="geometry"/></mxCell>' +
          '<mxCell id="b" value="Actor B" style="shape=umlLifeline;perimeter=lifelinePerimeter;whiteSpace=wrap;html=1;container=1;collapsible=0;recursiveResize=0;outlineConnect=0;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1"><mxGeometry x="400" y="40" width="120" height="360" as="geometry"/></mxCell>' +
          '<mxCell id="m1" value="request()" style="html=1;verticalAlign=bottom;startSize=8;endSize=8;" edge="1" parent="1" source="a" target="b"><mxGeometry relative="1" as="geometry"><mxPoint x="180" y="140" as="sourcePoint"/><mxPoint x="460" y="140" as="targetPoint"/></mxGeometry></mxCell>' +
          '<mxCell id="m2" value="response" style="html=1;verticalAlign=bottom;startSize=8;endSize=8;endArrow=open;dashed=1;" edge="1" parent="1" source="b" target="a"><mxGeometry relative="1" as="geometry"><mxPoint x="460" y="240" as="sourcePoint"/><mxPoint x="180" y="240" as="targetPoint"/></mxGeometry></mxCell>' +
        '</root>' +
      '</mxGraphModel>' +
    '</diagram></mxfile>',

  classDiagram:
    '<mxfile host="notepp" version="1"><diagram id="cls" name="Class">' +
      '<mxGraphModel dx="900" dy="700" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">' +
        '<root>' +
          '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
          '<mxCell id="c" value="ClassName" style="swimlane;fontStyle=1;align=center;verticalAlign=top;childLayout=stackLayout;horizontal=1;startSize=26;horizontalStack=0;resizeParent=1;resizeParentMax=0;collapsible=0;marginBottom=0;fillColor=#dae8fc;strokeColor=#6c8ebf;swimlaneFillColor=#ffffff;" vertex="1" parent="1"><mxGeometry x="200" y="80" width="240" height="170" as="geometry"/></mxCell>' +
          '<mxCell id="a1" value="+ name: String" style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;" vertex="1" parent="c"><mxGeometry y="26" width="240" height="22" as="geometry"/></mxCell>' +
          '<mxCell id="a2" value="+ age: int" style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;" vertex="1" parent="c"><mxGeometry y="48" width="240" height="22" as="geometry"/></mxCell>' +
          '<mxCell id="sep" value="" style="line;strokeWidth=1;fillColor=none;align=left;verticalAlign=middle;spacingTop=-1;spacingLeft=3;spacingRight=3;rotatable=0;labelPosition=right;points=[];portConstraint=eastwest;" vertex="1" parent="c"><mxGeometry y="70" width="240" height="8" as="geometry"/></mxCell>' +
          '<mxCell id="m1" value="+ getName(): String" style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;" vertex="1" parent="c"><mxGeometry y="78" width="240" height="22" as="geometry"/></mxCell>' +
          '<mxCell id="m2" value="+ setAge(age: int): void" style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;" vertex="1" parent="c"><mxGeometry y="100" width="240" height="22" as="geometry"/></mxCell>' +
        '</root>' +
      '</mxGraphModel>' +
    '</diagram></mxfile>',

  erDiagram:
    '<mxfile host="notepp" version="1"><diagram id="er" name="ER">' +
      '<mxGraphModel dx="900" dy="700" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">' +
        '<root>' +
          '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
          // Customer entity
          '<mxCell id="ec" value="Customer" style="rounded=0;whiteSpace=wrap;html=1;verticalAlign=top;fontStyle=1;fillColor=#dae8fc;strokeColor=#6c8ebf;align=center;" vertex="1" parent="1"><mxGeometry x="80" y="120" width="200" height="120" as="geometry"/></mxCell>' +
          '<mxCell id="ec1" value="🔑 id: int" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="ec"><mxGeometry y="30" width="200" height="24" as="geometry"/></mxCell>' +
          '<mxCell id="ec2" value="name: string" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="ec"><mxGeometry y="58" width="200" height="24" as="geometry"/></mxCell>' +
          '<mxCell id="ec3" value="email: string" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="ec"><mxGeometry y="86" width="200" height="24" as="geometry"/></mxCell>' +
          // Order entity
          '<mxCell id="eo" value="Order" style="rounded=0;whiteSpace=wrap;html=1;verticalAlign=top;fontStyle=1;fillColor=#d5e8d4;strokeColor=#82b366;align=center;" vertex="1" parent="1"><mxGeometry x="440" y="120" width="200" height="120" as="geometry"/></mxCell>' +
          '<mxCell id="eo1" value="🔑 id: int" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="eo"><mxGeometry y="30" width="200" height="24" as="geometry"/></mxCell>' +
          '<mxCell id="eo2" value="🔗 customer_id: int" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="eo"><mxGeometry y="58" width="200" height="24" as="geometry"/></mxCell>' +
          '<mxCell id="eo3" value="total: decimal" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="eo"><mxGeometry y="86" width="200" height="24" as="geometry"/></mxCell>' +
          // Relationship with cardinality
          '<mxCell id="rel" value="places" style="edgeStyle=entityRelationEdgeStyle;fontSize=12;html=1;endArrow=ERmany;startArrow=ERone;rounded=0;exitX=1;exitY=0.5;entryX=0;entryY=0.5;" edge="1" parent="1" source="ec" target="eo"><mxGeometry relative="1" as="geometry"/></mxCell>' +
        '</root>' +
      '</mxGraphModel>' +
    '</diagram></mxfile>',
};

// Renderer.js still owns createDrawioTab (it's tied to tab management).
// This is just a thin shim that picks the template + delegates.
function createDrawioTabFromTemplate(name) {
  const xml = DRAWIO_TEMPLATES[name];
  if (!xml) {
    if (typeof showToast === 'function') showToast('Unknown template: ' + name);
    return;
  }
  if (typeof createDrawioTab !== 'function') {
    console.error('[drawio-templates] createDrawioTab not yet defined — load order issue?');
    return;
  }
  return createDrawioTab(null, xml);
}
