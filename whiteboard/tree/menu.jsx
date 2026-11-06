// The right-click menu of a tree row: add, rename, move, delete and the icon grid.

import React, { useEffect, useRef, useState } from 'react';

import { COLORS, ICONS } from './icons.js';
import { addChild, addSibling, canMove, find, move, remove, rename, setColor, setIcon } from './model.js';
import { modelOf, redraw } from './scene.js';

// This sits outside the Excalidraw component, so none of its colour variables reach here.
const COLOURS = {
  light: { paper: '#ffffff', ink: '#1e1e1e', line: '#d6d6d6', hot: '#ececf4', wrong: '#c92a2a' },
  dark: { paper: '#232329', ink: '#e3e3e8', line: '#4a4a55', hot: '#31303b', wrong: '#ff8787' },
};
const panel = (c) => ({
  position: 'fixed', zIndex: 100, width: 196, padding: 6, borderRadius: 8, fontSize: 13, fontFamily: 'Segoe UI, sans-serif',
  background: c.paper, color: c.ink, border: `1px solid ${c.line}`, boxShadow: '0 4px 14px rgba(0,0,0,.25)',
});
const item = {
  display: 'block', width: '100%', textAlign: 'left', padding: '5px 8px', border: 0, borderRadius: 4, fontSize: 13,
  background: 'transparent', color: 'inherit', cursor: 'pointer',
};

function Item({ label, onClick, off, c }) {
  const [hot, setHot] = useState(false);
  const style = { ...item, opacity: off ? 0.4 : 1, cursor: off ? 'default' : 'pointer', background: hot && !off ? c.hot : 'transparent' };
  return <button type="button" style={style} disabled={off} onClick={onClick} onMouseEnter={() => setHot(true)} onMouseLeave={() => setHot(false)}>{label}</button>;
}

function Rename({ title, onSave, c }) {
  const [text, setText] = useState(title);
  const [wrong, setWrong] = useState(false);
  const box = useRef(null);
  useEffect(() => { box.current?.focus(); box.current?.select(); }, []);
  const save = () => { if (text.trim()) onSave(text); else setWrong(true); };
  return (
    <div>
      <input
        ref={box} value={text} aria-label="Name"
        style={{ width: '100%', boxSizing: 'border-box', padding: '4px 6px', fontSize: 13, background: 'transparent', color: 'inherit', border: `1px solid ${c.line}`, borderRadius: 4 }}
        onChange={(e) => { setText(e.target.value); setWrong(false); }}
        onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Enter') save(); }}
      />
      {wrong && <div style={{ color: c.wrong, fontSize: 12, padding: '3px 2px' }}>Enter a name</div>}
    </div>
  );
}

function Icons({ onPick, c }) {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(5, 1fr)', gap: 3 }}>
      {ICONS.map(([name, label, svg]) => (
        <button
          key={name} type="button" title={label} aria-label={label} onClick={() => onPick(name)}
          style={{ padding: 3, height: 30, background: 'transparent', border: `1px solid ${c.line}`, borderRadius: 4, cursor: 'pointer', color: 'inherit' }}
          dangerouslySetInnerHTML={{ __html: svg.replace('width="48" height="48"', 'width="20" height="20"') }}
        />
      ))}
    </div>
  );
}

function Colours({ onPick, c, current }) {
  const swatch = (selected) => ({ width: 26, height: 26, padding: 0, borderRadius: 6, cursor: 'pointer', border: selected ? `2px solid ${c.ink}` : `1px solid ${c.line}` });
  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 6, justifyItems: 'center', padding: 2 }}>
      <button type="button" title="Default" aria-label="Default" onClick={() => onPick(null)} style={{ ...swatch(!current), background: 'transparent', color: c.ink, fontSize: 12 }}>A</button>
      {COLORS.map(([label, hex]) => (
        <button key={hex} type="button" title={label} aria-label={label} onClick={() => onPick(hex)} style={{ ...swatch(current === hex), background: hex }} />
      ))}
    </div>
  );
}

/** `at` is { group, node, x, y, view }: `view` opens it on 'rename' instead of the list. */
export function TreeMenu({ api, at, theme, onClose }) {
  const c = COLOURS[theme] || COLOURS.light;
  const [view, setView] = useState(at.view || 'list');
  const [node, setNode] = useState(at.node);
  const me = useRef(null);

  useEffect(() => {
    setView(at.view || 'list');
    setNode(at.node);
  }, [at]);

  useEffect(() => {
    const outside = (e) => { if (!me.current?.contains(e.target)) onClose(); };
    const key = (e) => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } };
    window.addEventListener('pointerdown', outside, true);
    window.addEventListener('keydown', key, true);
    return () => { window.removeEventListener('pointerdown', outside, true); window.removeEventListener('keydown', key, true); };
  }, [onClose]);

  const tree = modelOf(api, at.group);
  const place = tree && find(tree, node);
  if (!place) return null;
  /** Change a copy of the tree and draw it; `then` says what happens to the menu. */
  const apply = (change, then = onClose) => {
    const t = modelOf(api, at.group);
    if (!t) return;
    const out = change(t);
    redraw(api, at.group, t);
    then(out);
  };
  const added = (n) => { if (n) { setNode(n.id); setView('rename'); } };
  const left = Math.min(at.x, window.innerWidth - 206);
  const top = Math.min(at.y, window.innerHeight - (view === 'icons' ? 230 : 240));

  return (
    <div ref={me} style={{ ...panel(c), left, top }} role="menu" aria-label="Tree node">
      {view === 'rename' && <Rename key={node} c={c} title={place.node.title} onSave={(text) => apply((t) => rename(t, node, text))} />}
      {view === 'icons' && <Icons c={c} onPick={(name) => apply((t) => setIcon(t, node, name))} />}
      {view === 'colours' && <Colours c={c} current={place.node.color} onPick={(hex) => apply((t) => setColor(t, node, hex))} />}
      {view === 'list' && (
        <>
          <Item c={c} label="Add child" onClick={() => apply((t) => addChild(t, node), added)} />
          <Item c={c} label="Add sibling" off={!place.parent} onClick={() => apply((t) => addSibling(t, node), added)} />
          <Item c={c} label="Change icon" onClick={() => setView('icons')} />
          <Item c={c} label="Change colour" onClick={() => setView('colours')} />
          <Item c={c} label="Rename" onClick={() => setView('rename')} />
          <Item c={c} label="Move up" off={!canMove(tree, node, -1)} onClick={() => apply((t) => move(t, node, -1))} />
          <Item c={c} label="Move down" off={!canMove(tree, node, 1)} onClick={() => apply((t) => move(t, node, 1))} />
          <Item c={c} label="Delete" off={!place.parent} onClick={() => apply((t) => remove(t, node))} />
        </>
      )}
    </div>
  );
}
