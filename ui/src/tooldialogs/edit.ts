import type { Mount } from './common';
import { chips, imageStage, livePreview, slider, toggleCtl } from './common';
import { h } from '../dom';
import { icon } from '../icons';

const SLIDERS: [string, string, number, number][] = [
  ['exposure', 'Exposure', -100, 100], ['brightness', 'Brightness', -100, 100], ['contrast', 'Contrast', -100, 100], ['saturation', 'Saturation', -100, 100], ['warmth', 'Warmth', -100, 100],
  ['sharpness', 'Sharpness', 0, 100], ['blur', 'Blur', 0, 100], ['vignette', 'Vignette', 0, 100],
];

export const mountEdit: Mount = async ({ paths, info, body, changed }) => {
  const stage = await imageStage(paths[0]);
  const prev = livePreview(stage, info.id, paths[0], () => get());
  const upd = () => { prev.refresh(); changed(); };
  const sl = Object.fromEntries(SLIDERS.map(([k, l, lo, hi]) => [k, slider(l, lo, hi, 1, 0, upd)]));
  const filter = chips([['none', 'None'], ['mono', 'Mono'], ['sepia', 'Sepia'], ['vivid', 'Vivid'], ['fade', 'Fade'], ['noir', 'Noir']], 'none', upd);
  let rotate = 0;
  const flipH = toggleCtl('Flip horizontally', false, upd), flipV = toggleCtl('Flip vertically', false, upd);
  const rotBtn = h('button', { class: 'btn small', type: 'button', onclick: () => { rotate = (rotate + 90) % 360; rotLbl.textContent = `${rotate}°`; upd(); } }, icon('rotate-cw', 14), 'Rotate 90°');
  const rotLbl = h('span', { class: 'pill' }, '0°');
  const reset = h('button', { class: 'btn small', type: 'button', onclick: () => { for (const c of Object.values(sl)) c.set(0); filter.set('none'); rotate = 0; rotLbl.textContent = '0°'; flipH.set(false); flipV.set(false); upd(); } }, 'Reset all');
  const compare = h('button', { class: 'btn small', type: 'button' }, 'Hold to compare');
  compare.addEventListener('pointerdown', () => prev.showOriginal(true));
  for (const ev of ['pointerup', 'pointerleave', 'pointercancel']) compare.addEventListener(ev, () => prev.showOriginal(false));
  const get = () => ({ ...Object.fromEntries(Object.entries(sl).map(([k, c]) => [k, c.get()])), filter: filter.get(), rotate, flip_h: flipH.get(), flip_v: flipV.get() });
  body.append(h('div', { class: 'cols' }, stage.el, h('div', { class: 'side' },
    h('h3', {}, 'Light and colour'), ...['exposure', 'brightness', 'contrast', 'saturation', 'warmth'].map((k) => sl[k].el),
    h('h3', {}, 'Detail and effects'), ...['sharpness', 'blur', 'vignette'].map((k) => sl[k].el),
    h('h3', {}, 'Filter'), filter.el,
    h('h3', {}, 'Orientation'), h('div', { class: 'row wrap' }, rotBtn, rotLbl), flipH.el, flipV.el,
    h('div', { class: 'row', style: 'margin-top:12px' }, compare, reset))));
  return { getParams: get, cta: 'Save edited copy' };
};
