// ══════════════════════════════════════════════════════════════════════════════
//  Note++ Whiteboard  ·  Lazy-loaded canvas whiteboard (Excalidraw-inspired)
//  All rendering via Canvas 2D API with seeded-random hand-drawn line style.
//  Communicates with parent renderer via postMessage.
// ══════════════════════════════════════════════════════════════════════════════

// ── Colour palette ────────────────────────────────────────────────────────────
const PALETTE = [
  '#1e1e1e','#868e96','#ffffff',
  '#e03131','#c2255c','#d9480f',
  '#f08c00','#2f9e44','#0c8599',
  '#1971c2','#6741d9','#845ef7',
];
// Translucent fill variants
const FILL_ALPHA = {};
PALETTE.forEach(c => {
  FILL_ALPHA[c] = c + (c === '#ffffff' ? '20' : '38');
});

// ── App state ─────────────────────────────────────────────────────────────────
let elements   = [];
let selIds     = new Set();
let tool       = 'select';
let strokeClr  = '#1e1e1e';
let fillMode   = 'none';      // 'none' | 'solid' | 'hatch'
let strokeW    = 1;
let isDark     = false;
let camera     = { x:0, y:0, zoom:1 };

// Transient interaction state
let drawing = false, sx = 0, sy = 0;
let panning = false, psx = 0, psy = 0, pcx = 0, pcy = 0, spaceDown = false;
let moving  = false, msx = 0, msy = 0, morig = {};
let editId  = null;

// History
let hist = [], hIdx = -1, idCtr = 0;

// ── DOM refs ──────────────────────────────────────────────────────────────────
const wrap   = document.getElementById('wrap');
const canvas = document.getElementById('canvas');
const ctx    = canvas.getContext('2d');
const tinp   = document.getElementById('tinp');

// ── Seeded RNG (LCG) ──────────────────────────────────────────────────────────
function rng(seed) {
  let s = Math.abs(~~seed) || 1;
  return () => { s = (s*1664525+1013904223)&0xFFFFFFFF; return (s>>>0)/0xFFFFFFFF; };
}
function strHash(str) {
  let h = 0;
  for (let i = 0; i < str.length; i++) h = (h*31 + str.charCodeAt(i)) | 0;
  return Math.abs(h);
}

// ── Coordinate transforms ─────────────────────────────────────────────────────
const s2w = (sx,sy) => ({ x:(sx-camera.x)/camera.zoom, y:(sy-camera.y)/camera.zoom });
const w2s = (wx,wy) => ({ x:wx*camera.zoom+camera.x,   y:wy*camera.zoom+camera.y  });
function evW(e) { const r=canvas.getBoundingClientRect(); return s2w(e.clientX-r.left, e.clientY-r.top); }

// ── Element helpers ───────────────────────────────────────────────────────────
function uid() { return `wb_${++idCtr}_${Date.now().toString(36)}`; }

function bnds(el) {
  if (el.type==='pencil') {
    const xs=el.pts.map(p=>p[0]), ys=el.pts.map(p=>p[1]);
    return { x:Math.min(...xs), y:Math.min(...ys), w:Math.max(...xs)-Math.min(...xs), h:Math.max(...ys)-Math.min(...ys) };
  }
  if (el.type==='arrow'||el.type==='line') {
    const [a,b]=el.pts;
    return { x:Math.min(a[0],b[0]), y:Math.min(a[1],b[1]), w:Math.abs(b[0]-a[0]), h:Math.abs(b[1]-a[1]) };
  }
  return { x:el.x, y:el.y, w:el.w||120, h:el.h||30 };
}

function hit(el, wx, wy) {
  const pad = Math.max(8, 10/camera.zoom);
  if (el.type==='arrow'||el.type==='line') {
    const [a,b]=el.pts, dx=b[0]-a[0], dy=b[1]-a[1], len=Math.hypot(dx,dy);
    if (len<1) return false;
    const t=Math.max(0,Math.min(1,((wx-a[0])*dx+(wy-a[1])*dy)/(len*len)));
    return Math.hypot(wx-(a[0]+t*dx), wy-(a[1]+t*dy)) < pad*2;
  }
  const b=bnds(el);
  return wx>=b.x-pad && wx<=b.x+b.w+pad && wy>=b.y-pad && wy<=b.y+b.h+pad;
}

// ── Rendering ─────────────────────────────────────────────────────────────────
function resize() { canvas.width=wrap.clientWidth; canvas.height=wrap.clientHeight; render(); }
new ResizeObserver(resize).observe(wrap);

function render() {
  const W=canvas.width, H=canvas.height;
  ctx.clearRect(0,0,W,H);
  ctx.fillStyle = isDark ? '#121212' : '#f8f9fa';
  ctx.fillRect(0,0,W,H);
  drawDots(W,H);

  ctx.save();
  ctx.translate(camera.x, camera.y);
  ctx.scale(camera.zoom, camera.zoom);

  elements.forEach(el => drawEl(el));
  if (drawEl_active) drawEl(drawEl_active);

  // Selection outlines
  selIds.forEach(id => {
    const el = elements.find(e=>e.id===id);
    if (el && el.id!==editId) drawSel(el);
  });
  ctx.restore();

  document.getElementById('zoom-val').textContent = Math.round(camera.zoom*100)+'%';
}

// Separate name to avoid collision with the drawEl function
let drawEl_active = null; // current in-progress drawn element

function drawDots(W,H) {
  const sp = 28*camera.zoom;
  if (sp<5) return;
  ctx.fillStyle = isDark ? '#2a2a3a' : '#d0d5db';
  const ox=((camera.x%sp)+sp)%sp, oy=((camera.y%sp)+sp)%sp;
  for (let x=ox;x<W;x+=sp) for (let y=oy;y<H;y+=sp) {
    ctx.beginPath(); ctx.arc(x,y,1.2,0,Math.PI*2); ctx.fill();
  }
}

function drawEl(el) {
  ctx.save();
  ctx.globalAlpha = el.op ?? 1;
  ctx.strokeStyle = el.sc;
  ctx.lineWidth   = el.sw;
  ctx.lineCap='round'; ctx.lineJoin='round';
  const seed = el.id ? strHash(el.id) : 1337;
  switch(el.type) {
    case 'rectangle': drawRect(el,seed); break;
    case 'diamond':   drawDiam(el,seed); break;
    case 'ellipse':   drawEllip(el,seed);break;
    case 'arrow':     drawArrow(el,seed); break;
    case 'line':      drawLine(el,seed);  break;
    case 'pencil':    drawPencil(el);     break;
    case 'text':      drawText(el);       break;
  }
  ctx.restore();
}

function drawSel(el) {
  const b=bnds(el), pad=8/camera.zoom, lw=1.5/camera.zoom, hs=5/camera.zoom;
  ctx.save();
  ctx.strokeStyle='#4263eb'; ctx.lineWidth=lw;
  ctx.setLineDash([5/camera.zoom,3/camera.zoom]);
  ctx.strokeRect(b.x-pad, b.y-pad, b.w+pad*2, b.h+pad*2);
  ctx.setLineDash([]);
  [[b.x-pad,b.y-pad],[b.x+b.w+pad,b.y-pad],
   [b.x-pad,b.y+b.h+pad],[b.x+b.w+pad,b.y+b.h+pad]].forEach(([hx,hy])=>{
    ctx.fillStyle='#fff'; ctx.beginPath(); ctx.rect(hx-hs,hy-hs,hs*2,hs*2); ctx.fill(); ctx.stroke();
  });
  ctx.restore();
}

// ── Sketchy drawing helpers ───────────────────────────────────────────────────
function skLine(x1,y1,x2,y2,r,roughness) {
  const j=()=>(r()-.5)*roughness*3;
  ctx.beginPath();
  ctx.moveTo(x1+j(),y1+j());
  ctx.bezierCurveTo(
    x1+(x2-x1)*.33+j()*2, y1+(y2-y1)*.33+j()*2,
    x1+(x2-x1)*.67+j()*2, y1+(y2-y1)*.67+j()*2,
    x2+j(), y2+j()
  );
  ctx.stroke();
}

function applyFill(el,fn) {
  if (!el.fc||el.fc==='none') return;
  ctx.save(); ctx.fillStyle=el.fc; fn(); ctx.fill(); ctx.restore();
}

function drawHatch(x,y,w,h) {
  ctx.save(); ctx.lineWidth=Math.max(.5,ctx.lineWidth*.4); ctx.globalAlpha*=.35;
  ctx.beginPath(); ctx.rect(x,y,w,h); ctx.clip();
  for (let i=x-h;i<x+w+h;i+=9) { ctx.moveTo(i,y); ctx.lineTo(i+h,y+h); }
  ctx.stroke(); ctx.restore();
}

function drawRect(el,seed) {
  const {x,y,w,h}=el, r=rng(seed), ro=el.ro??1;
  applyFill(el, ()=>{ ctx.beginPath(); ctx.rect(x,y,w,h); });
  if (el.fill==='hatch') drawHatch(x,y,w,h);
  skLine(x,y,x+w,y,r,ro); skLine(x+w,y,x+w,y+h,r,ro);
  skLine(x+w,y+h,x,y+h,r,ro); skLine(x,y+h,x,y,r,ro);
}

function drawDiam(el,seed) {
  const {x,y,w,h}=el, cx=x+w/2, cy=y+h/2, r=rng(seed), ro=el.ro??1;
  const pts=[[cx,y],[x+w,cy],[cx,y+h],[x,cy]];
  applyFill(el, ()=>{ ctx.beginPath(); pts.forEach(([px,py],i)=>i?ctx.lineTo(px,py):ctx.moveTo(px,py)); ctx.closePath(); });
  if (el.fill==='hatch') drawHatch(x,y,w,h);
  skLine(...pts[0],...pts[1],r,ro); skLine(...pts[1],...pts[2],r,ro);
  skLine(...pts[2],...pts[3],r,ro); skLine(...pts[3],...pts[0],r,ro);
}

function drawEllip(el,seed) {
  const {x,y,w,h}=el, cx=x+w/2, cy=y+h/2, rx=Math.abs(w/2), ry=Math.abs(h/2);
  applyFill(el, ()=>{ ctx.beginPath(); ctx.ellipse(cx,cy,rx,ry,0,0,Math.PI*2); });
  if (el.fill==='hatch') drawHatch(x,y,w,h);
  const rr=rng(seed), ro=el.ro??1, segs=36;
  ctx.beginPath();
  for (let i=0;i<=segs;i++) {
    const a=i/segs*Math.PI*2, jr=1+(rr()-.5)*ro*.05;
    const px=cx+Math.cos(a)*rx*jr+(rr()-.5)*ro*1.5;
    const py=cy+Math.sin(a)*ry*jr+(rr()-.5)*ro*1.5;
    i ? ctx.lineTo(px,py) : ctx.moveTo(px,py);
  }
  ctx.closePath(); ctx.stroke();
}

function drawArrow(el,seed) {
  const [p0,p1]=el.pts, r=rng(seed), ro=el.ro??1;
  skLine(p0[0],p0[1],p1[0],p1[1],r,ro);
  const ang=Math.atan2(p1[1]-p0[1],p1[0]-p0[0]), len=16, sp=.45;
  ctx.beginPath();
  ctx.moveTo(p1[0],p1[1]); ctx.lineTo(p1[0]-len*Math.cos(ang-sp),p1[1]-len*Math.sin(ang-sp));
  ctx.moveTo(p1[0],p1[1]); ctx.lineTo(p1[0]-len*Math.cos(ang+sp),p1[1]-len*Math.sin(ang+sp));
  ctx.stroke();
}

function drawLine(el,seed) {
  const [p0,p1]=el.pts;
  skLine(p0[0],p0[1],p1[0],p1[1],rng(seed),el.ro??1);
}

function drawPencil(el) {
  if (!el.pts||el.pts.length<2) return;
  ctx.beginPath(); ctx.moveTo(el.pts[0][0],el.pts[0][1]);
  for (let i=1;i<el.pts.length;i++) {
    const [px,py]=el.pts[i-1],[cx2,cy2]=el.pts[i];
    ctx.quadraticCurveTo(px,py,(px+cx2)/2,(py+cy2)/2);
  }
  ctx.stroke();
}

function drawText(el) {
  if (el.id===editId) return;
  ctx.fillStyle=el.sc;
  ctx.font=`${el.fs}px 'Segoe Print','Comic Sans MS',cursive,sans-serif`;
  ctx.textBaseline='top';
  (el.text||'').split('\n').forEach((line,i)=>ctx.fillText(line, el.x, el.y+i*el.fs*1.4));
}

// ── History / undo-redo ───────────────────────────────────────────────────────
function pushHist() {
  hist = hist.slice(0, hIdx+1);
  hist.push(JSON.stringify(elements));
  if (hist.length>50) hist.shift();
  hIdx = hist.length-1;
  notifyState();
}
function undo() {
  if (hIdx<=0) { if(hIdx===0){elements=[];hIdx=-1;selIds.clear();render();} return; }
  hIdx--; elements=JSON.parse(hist[hIdx]); selIds.clear(); render();
}
function redo() {
  if (hIdx>=hist.length-1) return;
  hIdx++; elements=JSON.parse(hist[hIdx]); selIds.clear(); render();
}

// ── postMessage bridge ────────────────────────────────────────────────────────
function notifyState() {
  window.parent?.postMessage({
    type: 'wb-state',
    content: JSON.stringify({ elements, idCounter:idCtr, camera, version:1 })
  }, '*');
}

window.addEventListener('message', e => {
  const m=e.data; if (!m?.type) return;
  if (m.type==='wb-load') {
    try {
      const d = typeof m.content==='string' ? JSON.parse(m.content||'{}') : (m.content||{});
      elements = d.elements||[];
      if (d.idCounter) idCtr=d.idCounter;
      if (d.camera) Object.assign(camera, d.camera);
      hist=[]; hIdx=-1; selIds.clear(); render();
    } catch(ex){ console.error('wb-load',ex); }
  }
  if (m.type==='wb-theme') setDark(m.dark);
  if (m.type==='wb-get-data') {
    window.parent?.postMessage({ type:'wb-data', content:JSON.stringify({elements,idCounter:idCtr,camera,version:1}) },'*');
  }
});

window.parent?.postMessage({ type:'wb-ready' }, '*');

// ── Dark mode ─────────────────────────────────────────────────────────────────
function setDark(dark) {
  isDark=dark;
  document.body.classList.toggle('dark',dark);
  if (strokeClr==='#1e1e1e'&&dark)   { strokeClr='#ffffff'; refreshSwatches(); }
  if (strokeClr==='#ffffff'&&!dark)  { strokeClr='#1e1e1e'; refreshSwatches(); }
  render();
}
function refreshSwatches() {
  document.querySelectorAll('.swatch').forEach(s=>s.classList.toggle('active',s.dataset.c===strokeClr));
}

// ── Tool buttons ──────────────────────────────────────────────────────────────
function setTool(t) {
  tool=t;
  document.querySelectorAll('button[data-tool]').forEach(b=>b.classList.toggle('active',b.dataset.tool===t));
  canvas.style.cursor = t==='select'?'default':'crosshair';
  commitText();
}
document.querySelectorAll('button[data-tool]').forEach(b=>b.addEventListener('click',()=>setTool(b.dataset.tool)));

// ── Colour swatches ───────────────────────────────────────────────────────────
const palEl = document.getElementById('palette');
PALETTE.forEach(c=>{
  const sw=document.createElement('div');
  sw.className='swatch'+(c===strokeClr?' active':'');
  sw.dataset.c=c; sw.style.background=c; sw.title=c;
  if (c==='#ffffff') sw.classList.add('white-border');
  sw.addEventListener('click',()=>{ strokeClr=c; refreshSwatches(); applyClrToSel(); });
  palEl.appendChild(sw);
});

function applyClrToSel() {
  if (!selIds.size) return;
  selIds.forEach(id=>{ const el=elements.find(e=>e.id===id); if(!el) return; el.sc=strokeClr; if(el.fc&&el.fc!=='none') el.fc=getFillClr(); });
  pushHist(); render();
}

// ── Fill ──────────────────────────────────────────────────────────────────────
document.querySelectorAll('button[data-fill]').forEach(b=>b.addEventListener('click',()=>{
  fillMode=b.dataset.fill;
  document.querySelectorAll('button[data-fill]').forEach(x=>x.classList.toggle('active',x.dataset.fill===fillMode));
  applyFillToSel();
}));
function getFillClr() {
  if (fillMode==='none') return 'none';
  return FILL_ALPHA[strokeClr]||strokeClr+'38';
}
function applyFillToSel() {
  if (!selIds.size) return;
  selIds.forEach(id=>{ const el=elements.find(e=>e.id===id); if(!el) return; el.fill=fillMode; el.fc=getFillClr(); });
  pushHist(); render();
}

// ── Stroke width ──────────────────────────────────────────────────────────────
document.querySelectorAll('button[data-width]').forEach(b=>b.addEventListener('click',()=>{
  strokeW=parseFloat(b.dataset.width);
  document.querySelectorAll('button[data-width]').forEach(x=>x.classList.toggle('active',x.dataset.width===b.dataset.width));
  applyWToSel();
}));
function applyWToSel() {
  if (!selIds.size) return;
  selIds.forEach(id=>{ const el=elements.find(e=>e.id===id); if(el) el.sw=strokeW; });
  pushHist(); render();
}

// ── Action buttons ────────────────────────────────────────────────────────────
document.getElementById('btn-undo').addEventListener('click',undo);
document.getElementById('btn-redo').addEventListener('click',redo);
document.getElementById('btn-export').addEventListener('click',()=>{
  const a=document.createElement('a'); a.download='whiteboard.png'; a.href=canvas.toDataURL(); a.click();
});
document.getElementById('btn-clear').addEventListener('click',()=>{
  if (!elements.length||confirm('Clear the entire whiteboard?')) { elements=[]; selIds.clear(); pushHist(); render(); }
});

// ── Zoom ──────────────────────────────────────────────────────────────────────
function zoom(factor,cx=canvas.width/2,cy=canvas.height/2) {
  const nz=Math.max(.08,Math.min(12,camera.zoom*factor));
  camera.x=cx-(cx-camera.x)*(nz/camera.zoom);
  camera.y=cy-(cy-camera.y)*(nz/camera.zoom);
  camera.zoom=nz; render();
}
document.getElementById('z-in' ).addEventListener('click',()=>zoom(1.2));
document.getElementById('z-out').addEventListener('click',()=>zoom(0.83));
document.getElementById('z-rst').addEventListener('click',()=>{ camera={x:0,y:0,zoom:1}; render(); });

// ── Canvas events ─────────────────────────────────────────────────────────────
canvas.addEventListener('mousedown', onDown);
canvas.addEventListener('mousemove', onMove);
canvas.addEventListener('mouseup',   onUp);
canvas.addEventListener('wheel',     onWheel, {passive:false});
canvas.addEventListener('dblclick',  onDbl);

function onDown(e) {
  if (e.button===1)    { startPan(e); return; }
  if (spaceDown)       { startPan(e); return; }
  const w=evW(e);
  if      (tool==='select') selDown(w,e);
  else if (tool==='eraser') eraseAt(w);
  else if (tool==='text')   textDown(w);
  else                      startDraw(w);
}
function onMove(e) {
  if (panning) { camera.x=pcx+(e.clientX-psx); camera.y=pcy+(e.clientY-psy); render(); return; }
  const w=evW(e);
  if (moving)               { moveEls(w); return; }
  if (drawing&&drawEl_active) updateDraw(w);
}
function onUp(e) {
  if (panning) { stopPan(); return; }
  if (moving)  { moving=false; morig={}; pushHist(); return; }
  if (drawing)  stopDraw(evW(e));
}
function onWheel(e) {
  e.preventDefault();
  if (e.ctrlKey||e.metaKey) {
    const r=canvas.getBoundingClientRect();
    zoom(e.deltaY<0?1.1:.91, e.clientX-r.left, e.clientY-r.top);
  } else { camera.x-=e.deltaX; camera.y-=e.deltaY; render(); }
}
function onDbl(e) {
  const w=evW(e);
  for (let i=elements.length-1;i>=0;i--)
    if (elements[i].type==='text'&&hit(elements[i],w.x,w.y)) { startTextEdit(elements[i]); return; }
}

// ── Pan ───────────────────────────────────────────────────────────────────────
function startPan(e) {
  panning=true; psx=e.clientX; psy=e.clientY; pcx=camera.x; pcy=camera.y;
  canvas.style.cursor='grabbing';
}
function stopPan() { panning=false; canvas.style.cursor=tool==='select'?'default':'crosshair'; }

// ── Selection / move ──────────────────────────────────────────────────────────
function selDown(w,e) {
  let clicked=null;
  for (let i=elements.length-1;i>=0;i--) if(hit(elements[i],w.x,w.y)){clicked=elements[i];break;}
  if (!clicked) { selIds.clear(); render(); return; }
  if (!e.shiftKey&&!selIds.has(clicked.id)) selIds.clear();
  selIds.add(clicked.id); render();
  moving=true; msx=w.x; msy=w.y; morig={};
  selIds.forEach(id=>{
    const el=elements.find(e=>e.id===id); if(!el) return;
    if (el.type==='pencil'||el.type==='arrow'||el.type==='line') morig[id]={pts:el.pts.map(p=>[...p])};
    else morig[id]={x:el.x,y:el.y};
  });
}
function moveEls(w) {
  const dx=w.x-msx, dy=w.y-msy;
  selIds.forEach(id=>{
    const el=elements.find(e=>e.id===id); if(!el) return; const o=morig[id]; if(!o) return;
    if (el.type==='pencil'||el.type==='arrow'||el.type==='line') el.pts=o.pts.map(p=>[p[0]+dx,p[1]+dy]);
    else { el.x=o.x+dx; el.y=o.y+dy; }
  });
  render();
}

// ── Eraser ────────────────────────────────────────────────────────────────────
function eraseAt(w) {
  for (let i=elements.length-1;i>=0;i--)
    if(hit(elements[i],w.x,w.y)){elements.splice(i,1);pushHist();render();return;}
}

// ── Drawing ───────────────────────────────────────────────────────────────────
function mkEl(type,extra={}) {
  return { id:uid(), type, sc:strokeClr, fc:getFillClr(), sw:strokeW, fill:fillMode, ro:1, op:1, ...extra };
}
function startDraw(w) {
  drawing=true; sx=w.x; sy=w.y;
  if      (tool==='pencil')              drawEl_active=mkEl('pencil',{pts:[[w.x,w.y]]});
  else if (tool==='arrow'||tool==='line') drawEl_active=mkEl(tool,{pts:[[w.x,w.y],[w.x,w.y]]});
  else                                   drawEl_active=mkEl(tool,{x:w.x,y:w.y,w:0,h:0});
}
function updateDraw(w) {
  if (!drawEl_active) return;
  if (drawEl_active.type==='pencil') drawEl_active.pts.push([w.x,w.y]);
  else if (drawEl_active.type==='arrow'||drawEl_active.type==='line') drawEl_active.pts=[[sx,sy],[w.x,w.y]];
  else { drawEl_active.x=Math.min(sx,w.x); drawEl_active.y=Math.min(sy,w.y); drawEl_active.w=Math.abs(w.x-sx); drawEl_active.h=Math.abs(w.y-sy); }
  render();
}
function stopDraw(w) {
  if (!drawEl_active){drawing=false;return;}
  updateDraw(w);
  const b=bnds(drawEl_active);
  if (b.w>=4||b.h>=4||drawEl_active.type==='pencil') {
    elements.push(drawEl_active); selIds.clear(); selIds.add(drawEl_active.id); pushHist();
  }
  drawEl_active=null; drawing=false; render();
}

// ── Text tool ─────────────────────────────────────────────────────────────────
function textDown(w) {
  for (let i=elements.length-1;i>=0;i--)
    if(elements[i].type==='text'&&hit(elements[i],w.x,w.y)){startTextEdit(elements[i]);return;}
  const el=mkEl('text',{x:w.x,y:w.y,text:'',fs:20,w:0,h:0,ro:0});
  elements.push(el); selIds.clear(); selIds.add(el.id); startTextEdit(el);
}
function startTextEdit(el) {
  commitText();
  editId=el.id;
  const s=w2s(el.x,el.y), wr=wrap.getBoundingClientRect();
  tinp.style.left=s.x+'px'; tinp.style.top=s.y+'px';
  tinp.style.fontSize=(el.fs*camera.zoom)+'px'; tinp.style.color=el.sc;
  tinp.value=el.text||''; tinp.classList.remove('hidden'); tinp.focus(); tinp.select(); render();
}
function commitText() {
  if (!editId) return;
  const el=elements.find(e=>e.id===editId);
  if (el) {
    el.text=tinp.value;
    ctx.font=`${el.fs}px 'Segoe Print','Comic Sans MS',cursive,sans-serif`;
    const lines=(el.text||'').split('\n');
    el.w=Math.max(80,...lines.map(l=>ctx.measureText(l).width));
    el.h=lines.length*el.fs*1.4;
    if (!el.text.trim()) { const i=elements.indexOf(el); if(i>=0) elements.splice(i,1); }
    else pushHist();
  }
  editId=null; tinp.classList.add('hidden'); tinp.value=''; render();
}
tinp.addEventListener('blur', commitText);
tinp.addEventListener('keydown', e=>{
  if (e.key==='Escape'){ const el=elements.find(x=>x.id===editId); if(el&&!el.text.trim())elements.splice(elements.indexOf(el),1); editId=null; tinp.classList.add('hidden'); tinp.value=''; render(); return; }
  if (e.key==='Enter'&&!e.shiftKey){ e.preventDefault(); commitText(); }
});

// ── Keyboard shortcuts ────────────────────────────────────────────────────────
document.addEventListener('keydown', e=>{
  if (editId||e.target===tinp) return;
  if (e.ctrlKey||e.metaKey) {
    if(e.key==='z'){e.preventDefault();undo();}
    else if(e.key==='y'||e.key==='Z'){e.preventDefault();redo();}
    else if(e.key==='a'){e.preventDefault();selIds=new Set(elements.map(x=>x.id));render();}
    else if(e.key==='s'){e.preventDefault();window.parent?.postMessage({type:'wb-save-request'},'*');}
    return;
  }
  const k=e.key.toLowerCase();
  if(k===' '){e.preventDefault();spaceDown=true;canvas.style.cursor='grab';}
  else if(k==='v')setTool('select');
  else if(k==='r')setTool('rectangle');
  else if(k==='d')setTool('diamond');
  else if(k==='e')setTool('ellipse');
  else if(k==='a')setTool('arrow');
  else if(k==='l')setTool('line');
  else if(k==='p')setTool('pencil');
  else if(k==='t')setTool('text');
  else if(k==='x')setTool('eraser');
  else if(k==='delete'||k==='backspace'){
    if(selIds.size){elements=elements.filter(el=>!selIds.has(el.id));selIds.clear();pushHist();render();}
  }
  else if(k==='escape'){selIds.clear();drawEl_active=null;drawing=false;render();}
  else if(k==='+'||k==='=')zoom(1.2);
  else if(k==='-')zoom(.83);
  else if(k==='0'){camera={x:0,y:0,zoom:1};render();}
});
document.addEventListener('keyup', e=>{
  if(e.key===' '){spaceDown=false;canvas.style.cursor=tool==='select'?'default':'crosshair';}
});

// ── Init ──────────────────────────────────────────────────────────────────────
resize();
