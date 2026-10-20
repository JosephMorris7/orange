import type { Mount } from './common';
import { chips, colorCtl, field, imageStage, livePreview, numberCtl, svgEl } from './common';
import { api, type Region } from '../api';
import { h } from '../dom';
import { icon } from '../icons';

export const mountRedact: Mount = async ({ paths, info, body, changed }) => {
  const stage = await imageStage(paths[0]);
  const W = stage.width, H = stage.height;
  let regions: Region[] = [];
  let style: 'solid' | 'blur' = 'solid';
  const color = colorCtl('#000000', () => upd());
  const radius = numberCtl(5, 100, 1, 30, () => upd(), 'px');
  const prev = livePreview(stage, info.id, paths[0], () => params());
  const list = h('div', { class: 'list' });
  const status = h('p', { class: 'muted small', style: 'margin-top:8px' }, 'Drag on the picture to cover an area.');
  stage.inner.classList.add('draw');
  const layer = svgEl('g');
  stage.svg.append(layer);

  function paintRegions(): void {
    layer.replaceChildren(...regions.map((r) => svgEl('rect', { x: r.x, y: r.y, width: r.w, height: r.h, fill: 'rgba(255,106,19,.18)', stroke: '#ff6a13', 'stroke-width': Math.max(2, W / 400), 'stroke-dasharray': `${W / 120} ${W / 200}` })));
    list.replaceChildren(...regions.map((r, i) => h('div', { class: 'li' }, h('span', { class: 'nm' }, `Area ${i + 1} · ${Math.round(r.w)}×${Math.round(r.h)}`),
      h('button', { class: 'btn ghost icon-only', 'aria-label': `Remove area ${i + 1}`, onclick: () => { regions.splice(i, 1); upd(); } }, icon('close', 14)))));
  }
  function upd(): void { paintRegions(); prev.refresh(); changed(); }
  const params = () => ({ regions: regions.map((r) => ({ x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.w), h: Math.round(r.h) })), style, color: color.get(), blur_radius: radius.get() });

  let start: { x: number; y: number } | null = null; let live: SVGRectElement | null = null;
  stage.svg.addEventListener('pointerdown', (e) => { e.preventDefault(); stage.svg.setPointerCapture(e.pointerId); start = stage.point(e); live = svgEl('rect', { fill: 'rgba(255,106,19,.25)', stroke: '#ff6a13', 'stroke-width': Math.max(2, W / 400) }); layer.append(live); });
  stage.svg.addEventListener('pointermove', (e) => {
    if (!start || !live) return;
    const p = stage.point(e);
    live.setAttribute('x', String(Math.min(start.x, p.x))); live.setAttribute('y', String(Math.min(start.y, p.y)));
    live.setAttribute('width', String(Math.abs(p.x - start.x))); live.setAttribute('height', String(Math.abs(p.y - start.y)));
  });
  stage.svg.addEventListener('pointerup', (e) => {
    if (!start) return;
    const p = stage.point(e);
    const r = { x: Math.min(start.x, p.x), y: Math.min(start.y, p.y), w: Math.abs(p.x - start.x), h: Math.abs(p.y - start.y) };
    start = null; live?.remove(); live = null;
    if (r.w > W / 200 && r.h > H / 200) { regions.push(r); upd(); }
  });

  async function detect(what: 'text' | 'faces'): Promise<void> {
    status.textContent = what === 'text' ? 'Looking for text…' : 'Looking for faces…';
    try {
      const found = await api.detectRegions(paths[0], what);
      if (!found.length) { status.textContent = what === 'text' ? 'No text found (or text detection is not available on this system).' : 'No faces found (or face detection is not available on this system).'; return; }
      regions.push(...found.map((f) => ({ x: Math.max(0, f.x), y: Math.max(0, f.y), w: Math.min(W, f.w), h: Math.min(H, f.h) })));
      status.textContent = `Added ${found.length} area${found.length > 1 ? 's' : ''}. Remove any you want to keep visible.`;
      upd();
    } catch (e) { status.textContent = String(e); }
  }

  const styleChips = chips([['solid', 'Solid colour'], ['blur', 'Pixelate']], 'solid', (v) => { style = v as 'solid' | 'blur'; colorRow.style.display = style === 'solid' ? '' : 'none'; radiusRow.style.display = style === 'blur' ? '' : 'none'; upd(); });
  const colorRow = field('Colour', color.el), radiusRow = field('Strength', radius.el);
  radiusRow.style.display = 'none';
  body.append(h('div', { class: 'cols' }, stage.el, h('div', { class: 'side' },
    h('h3', {}, 'Find automatically'),
    h('div', { class: 'tools' },
      h('button', { class: 'btn small', type: 'button', onclick: () => void detect('text') }, icon('text', 14), 'Text'),
      h('button', { class: 'btn small', type: 'button', onclick: () => void detect('faces') }, icon('eye-off', 14), 'Faces')),
    status,
    h('h3', {}, 'Cover with'), styleChips.el, colorRow, radiusRow,
    h('h3', {}, 'Areas'), list,
    h('p', { class: 'muted small', style: 'margin-top:10px' }, 'Covered pixels are destroyed in the saved copy, and its metadata is removed.'))));
  void H;
  return { getParams: params, validate: () => (regions.length ? null : 'Draw at least one area to cover.'), cta: 'Redact' };
};
