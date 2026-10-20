import type { Mount } from './common';
import { chips, clamp, imageStage, numberCtl, svgEl } from './common';
import { h } from '../dom';

type Handle = 'move' | 'nw' | 'ne' | 'sw' | 'se';

export const mountCrop: Mount = async ({ paths, body, changed }) => {
  const stage = await imageStage(paths[0]);
  const W = stage.width, H = stage.height;
  const multi = paths.length > 1;
  let ratio = 'free';
  let r = { x: Math.round(W * 0.1), y: Math.round(H * 0.1), w: Math.round(W * 0.8), h: Math.round(H * 0.8) };
  let mode: 'region' | 'ratio' = 'region';

  const dim = svgEl('path', { 'fill-rule': 'evenodd', fill: 'rgba(22,17,13,.55)' });
  const box = svgEl('rect', { fill: 'none', stroke: '#ff6a13', 'stroke-width': Math.max(2, W / 400), cursor: 'move' });
  const hs = Math.max(8, W / 70);
  const handles: Record<string, SVGRectElement> = {};
  for (const k of ['nw', 'ne', 'sw', 'se'] as const) { handles[k] = svgEl('rect', { width: hs * 2, height: hs * 2, fill: '#fff', stroke: '#ff6a13', 'stroke-width': hs / 5 }); stage.svg.append(handles[k]); }
  stage.svg.prepend(dim); stage.svg.insertBefore(box, handles.nw);

  const nx = numberCtl(0, W, 1, r.x, () => fromInputs()), ny = numberCtl(0, H, 1, r.y, () => fromInputs());
  const nw = numberCtl(1, W, 1, r.w, () => fromInputs()), nh = numberCtl(1, H, 1, r.h, () => fromInputs());

  const ratioVal = (): number | null => { if (ratio === 'free') return null; if (ratio === 'original') return W / H; const [a, b] = ratio.split(':').map(Number); return a / b; };
  function norm(): void {
    const rv = ratioVal();
    r.w = clamp(Math.round(r.w), 8, W); r.h = clamp(Math.round(r.h), 8, H);
    if (rv) { r.h = Math.round(r.w / rv); if (r.h > H) { r.h = H; r.w = Math.round(H * rv); } }
    r.x = clamp(Math.round(r.x), 0, W - r.w); r.y = clamp(Math.round(r.y), 0, H - r.h);
  }
  function paint(): void {
    norm();
    dim.setAttribute('d', `M0 0H${W}V${H}H0Z M${r.x} ${r.y}V${r.y + r.h}H${r.x + r.w}V${r.y}Z`);
    for (const [k, [ax, ay]] of Object.entries({ nw: [r.x, r.y], ne: [r.x + r.w, r.y], sw: [r.x, r.y + r.h], se: [r.x + r.w, r.y + r.h] })) { handles[k].setAttribute('x', String(ax - hs)); handles[k].setAttribute('y', String(ay - hs)); }
    box.setAttribute('x', String(r.x)); box.setAttribute('y', String(r.y)); box.setAttribute('width', String(r.w)); box.setAttribute('height', String(r.h));
    nx.set(r.x); ny.set(r.y); nw.set(r.w); nh.set(r.h);
    stage.svg.style.display = mode === 'region' ? '' : 'none';
    changed();
  }
  function fromInputs(): void { r = { x: nx.get(), y: ny.get(), w: nw.get(), h: nh.get() }; paint(); }

  let drag: { kind: Handle; start: { x: number; y: number }; orig: typeof r } | null = null;
  const begin = (kind: Handle) => (e: PointerEvent) => { e.preventDefault(); (e.target as Element).setPointerCapture(e.pointerId); drag = { kind, start: stage.point(e), orig: { ...r } }; };
  box.addEventListener('pointerdown', begin('move'));
  for (const k of ['nw', 'ne', 'sw', 'se'] as const) handles[k].addEventListener('pointerdown', begin(k));
  stage.svg.addEventListener('pointermove', (e) => {
    if (!drag) return;
    const p = stage.point(e), dx = p.x - drag.start.x, dy = p.y - drag.start.y, o = drag.orig;
    if (drag.kind === 'move') r = { ...o, x: clamp(o.x + dx, 0, W - o.w), y: clamp(o.y + dy, 0, H - o.h) };
    else {
      let x0 = o.x, y0 = o.y, x1 = o.x + o.w, y1 = o.y + o.h;
      if (drag.kind.includes('w')) x0 = clamp(o.x + dx, 0, x1 - 8); else x1 = clamp(o.x + o.w + dx, x0 + 8, W);
      if (drag.kind.includes('n')) y0 = clamp(o.y + dy, 0, y1 - 8); else y1 = clamp(o.y + o.h + dy, y0 + 8, H);
      r = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
      const rv = ratioVal();
      if (rv) { r.h = r.w / rv; if (drag.kind.includes('n')) r.y = y1 - r.h; }
    }
    paint();
  });
  stage.svg.addEventListener('pointerup', () => { drag = null; });

  const ratios: [string, string][] = [['free', 'Free'], ['original', 'Original'], ['1:1', '1:1'], ['16:9', '16:9'], ['4:3', '4:3'], ['3:2', '3:2'], ['4:5', '4:5'], ['3:4', '3:4'], ['9:16', '9:16']];
  const ratioChips = chips(ratios, 'free', (v) => { ratio = v; if (mode === 'region') { /* norm() re-derives the height */ } paint(); });
  const modeChips = chips([['region', 'Exact region'], ['ratio', 'Centre by ratio']], 'region', (v) => { mode = v as 'region' | 'ratio'; if (mode === 'ratio' && ratio === 'free') { ratio = '16:9'; ratioChips.set(ratio); } paint(); });
  function params(): unknown {
    if (mode === 'ratio') return { mode: 'ratio', ratio: ratio === 'original' ? `${W}:${H}` : ratio };
    return { x: r.x, y: r.y, width: r.w, height: r.h };
  }
  paint();
  body.append(h('div', { class: 'cols' }, stage.el, h('div', { class: 'side' },
    multi ? h('p', { class: 'muted small', style: 'margin-bottom:10px' }, `The crop applies to all ${paths.length} images (clamped to each one's size). Preview shows the first.`) : null,
    h('h3', {}, 'Mode'), modeChips.el,
    h('h3', {}, 'Aspect ratio'), ratioChips.el,
    h('h3', {}, 'Region in pixels'),
    h('div', { class: 'two' }, h('div', { class: 'field' }, h('label', {}, 'X'), nx.el), h('div', { class: 'field' }, h('label', {}, 'Y'), ny.el),
      h('div', { class: 'field' }, h('label', {}, 'Width'), nw.el), h('div', { class: 'field' }, h('label', {}, 'Height'), nh.el)),
    h('p', { class: 'muted small' }, `Image is ${W} × ${H} px.`))));
  return { getParams: params, cta: 'Crop' };
};
