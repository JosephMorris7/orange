import type { Mount } from './common';
import { chips, colorCtl, field, imageStage, livePreview, nextId, svgEl } from './common';
import { h } from '../dom';
import { icon } from '../icons';

type Item =
  | { type: 'arrow'; x1: number; y1: number; x2: number; y2: number; color: string; width: number }
  | { type: 'rect'; x: number; y: number; w: number; h: number; color: string; width: number; fill: string | null }
  | { type: 'ellipse'; x: number; y: number; w: number; h: number; color: string; width: number; fill: string | null }
  | { type: 'text'; x: number; y: number; text: string; size: number; color: string; bg: string | null; bold: boolean }
  | { type: 'callout'; x: number; y: number; n: number; size: number; color: string };

type Tool = 'arrow' | 'rect' | 'ellipse' | 'text' | 'callout';

export const mountAnnotate: Mount = async ({ paths, info, body, changed }) => {
  const stage = await imageStage(paths[0]);
  const W = stage.width, H = stage.height;
  const k = Math.max(1, Math.max(W, H) / 1200); // sizes scale with the picture so marks stay readable
  let tool: Tool = 'arrow';
  const items: Item[] = [];
  const color = colorCtl('#ff3b30', () => undefined);
  const prev = livePreview(stage, info.id, paths[0], () => params(), 1000);
  const layer = svgEl('g');
  stage.svg.append(layer);
  stage.inner.classList.add('draw');
  const list = h('div', { class: 'list' });

  const label = (it: Item, i: number) => `${i + 1}. ${it.type === 'text' ? `Text “${it.text.slice(0, 18)}”` : it.type[0].toUpperCase() + it.type.slice(1)}`;
  function drawItem(it: Item): SVGElement {
    const g = svgEl('g');
    if (it.type === 'arrow') {
      const a = Math.atan2(it.y2 - it.y1, it.x2 - it.x1), hl = it.width * 3.2, hw = it.width * 1.9;
      const bx = it.x2 - Math.cos(a) * hl, by = it.y2 - Math.sin(a) * hl;
      g.append(svgEl('line', { x1: it.x1, y1: it.y1, x2: bx, y2: by, stroke: it.color, 'stroke-width': it.width, 'stroke-linecap': 'round' }),
        svgEl('polygon', { points: `${it.x2},${it.y2} ${bx + Math.sin(a) * hw},${by - Math.cos(a) * hw} ${bx - Math.sin(a) * hw},${by + Math.cos(a) * hw}`, fill: it.color }));
    } else if (it.type === 'rect') g.append(svgEl('rect', { x: it.x, y: it.y, width: it.w, height: it.h, stroke: it.color, 'stroke-width': it.width, fill: it.fill ?? 'none' }));
    else if (it.type === 'ellipse') g.append(svgEl('ellipse', { cx: it.x + it.w / 2, cy: it.y + it.h / 2, rx: it.w / 2, ry: it.h / 2, stroke: it.color, 'stroke-width': it.width, fill: it.fill ?? 'none' }));
    else if (it.type === 'text') {
      const t = svgEl('text', { x: it.x, y: it.y + it.size, 'font-size': it.size, 'font-weight': it.bold ? 700 : 400, fill: it.color, 'font-family': 'Segoe UI, sans-serif' });
      t.textContent = it.text;
      if (it.bg) g.append(svgEl('rect', { x: it.x - it.size / 4, y: it.y, width: it.text.length * it.size * 0.58 + it.size / 2, height: it.size * 1.3, fill: it.bg, rx: it.size / 6 }));
      g.append(t);
    } else {
      g.append(svgEl('circle', { cx: it.x, cy: it.y, r: it.size / 2, fill: it.color }));
      const t = svgEl('text', { x: it.x, y: it.y + it.size * 0.17, 'font-size': it.size * 0.55, 'font-weight': 700, fill: '#fff', 'text-anchor': 'middle', 'font-family': 'Segoe UI, sans-serif' });
      t.textContent = String(it.n); g.append(t);
    }
    return g;
  }
  function upd(refreshPreview = true): void {
    layer.replaceChildren(...items.map(drawItem));
    list.replaceChildren(...items.map((it, i) => h('div', { class: 'li' }, h('span', { class: 'nm' }, label(it, i)),
      h('button', { class: 'btn ghost icon-only', 'aria-label': `Remove ${label(it, i)}`, onclick: () => { items.splice(i, 1); upd(); } }, icon('close', 14)))));
    if (refreshPreview) { prev.refresh(); }
    changed();
  }
  const params = () => ({ items: items.map((it) => ({ ...it })) });

  let start: { x: number; y: number } | null = null; let ghost: SVGElement | null = null;
  const ghostFor = (a: { x: number; y: number }, b: { x: number; y: number }): Item | null => {
    const c = color.get(), w = Math.round(6 * k);
    if (tool === 'arrow') return { type: 'arrow', x1: a.x, y1: a.y, x2: b.x, y2: b.y, color: c, width: w };
    if (tool === 'rect' || tool === 'ellipse') return { type: tool, x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y), color: c, width: Math.round(5 * k), fill: null };
    return null;
  };
  stage.svg.addEventListener('pointerdown', (e) => {
    const p = stage.point(e);
    if (tool === 'callout') { items.push({ type: 'callout', x: p.x, y: p.y, n: items.filter((i) => i.type === 'callout').length + 1, size: Math.round(52 * k), color: color.get() }); upd(); return; }
    if (tool === 'text') { askText(e, p); return; }
    e.preventDefault(); stage.svg.setPointerCapture(e.pointerId); start = p;
  });
  stage.svg.addEventListener('pointermove', (e) => {
    if (!start) return;
    const it = ghostFor(start, stage.point(e));
    ghost?.remove();
    if (it) { ghost = drawItem(it); ghost.setAttribute('opacity', '.7'); layer.append(ghost); }
  });
  stage.svg.addEventListener('pointerup', (e) => {
    if (!start) return;
    const it = ghostFor(start, stage.point(e));
    ghost?.remove(); ghost = null; start = null;
    if (!it) return;
    const big = it.type === 'arrow' ? Math.hypot(it.x2 - it.x1, it.y2 - it.y1) > 12 * k : it.type === 'callout' || it.type === 'text' ? true : it.w > 8 * k && it.h > 8 * k;
    if (big) { items.push(it); upd(); }
  });

  const pop = h('div', { style: 'display:none;margin:0 0 12px' });
  function askText(e: PointerEvent, p: { x: number; y: number }): void {
    e.preventDefault();
    const input = h('input', { type: 'text', placeholder: 'Type the note, then press Enter', id: nextId('t') });
    const bold = h('input', { type: 'checkbox', checked: true }), box = h('input', { type: 'checkbox' });
    const commit = () => {
      const v = input.value.trim();
      if (v) items.push({ type: 'text', x: p.x, y: p.y, text: v, size: Math.round(44 * k), color: color.get(), bg: box.checked ? '#000000aa' : null, bold: bold.checked });
      pop.style.display = 'none'; pop.replaceChildren(); upd();
    };
    input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') commit(); if (ev.key === 'Escape') { pop.style.display = 'none'; pop.replaceChildren(); } });
    pop.replaceChildren(field('Text', input), h('label', { class: 'toggle' }, bold, 'Bold'), h('label', { class: 'toggle' }, box, 'Dark backing'), h('button', { class: 'btn small primary', type: 'button', onclick: commit }, 'Add text'));
    pop.style.display = ''; input.focus();
  }

  const tools: [Tool, string, string][] = [['arrow', 'arrow', 'Arrow'], ['rect', 'rect', 'Box'], ['ellipse', 'ellipse', 'Oval'], ['text', 'text', 'Text'], ['callout', 'callout', 'Number']];
  const toolChips = chips(tools.map(([id, , l]) => [id, l] as [string, string]), 'arrow', (v) => { tool = v as Tool; });
  body.append(h('div', { class: 'cols' }, stage.el, h('div', { class: 'side' },
    h('h3', {}, 'Tool'), toolChips.el, h('div', { style: 'height:12px' }),
    field('Colour', color.el), pop,
    h('div', { class: 'row' }, h('button', { class: 'btn small', type: 'button', onclick: () => { items.pop(); upd(); } }, 'Undo'), h('button', { class: 'btn small', type: 'button', onclick: () => { items.length = 0; upd(); } }, 'Clear all')),
    h('h3', {}, 'Marks'), list)));
  void H;
  return { getParams: params, validate: () => (items.length ? null : 'Add at least one mark.'), cta: 'Save annotated copy' };
};
