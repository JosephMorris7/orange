import { chips, colorCtl, field, numberCtl, slider } from './common';
import { h } from '../dom';
import { isTauri } from '../api';

export type Background = { type: 'solid'; color: string } | { type: 'gradient'; from: string; to: string; angle: number } | { type: 'image'; path: string };

/** Solid / gradient / image background picker shared by "Add background" and "Collage". */
export function backgroundCtl(onChange: () => void, allowImage: boolean): { el: HTMLElement; get(): Background; css(): string } {
  let type: Background['type'] = 'gradient';
  let imagePath = '';
  const solid = colorCtl('#ffffff', onChange), from = colorCtl('#ffb37a', onChange), to = colorCtl('#ff6a13', onChange);
  const angle = slider('Angle', 0, 360, 5, 135, onChange);
  const fileLbl = h('span', { class: 'muted small' }, 'No image chosen');
  const pickBtn = h('button', { class: 'btn small', type: 'button', onclick: async () => {
    let p: string | null = null;
    if (isTauri()) { const { open } = await import('@tauri-apps/plugin-dialog'); const r = await open({ multiple: false, filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'tiff', 'heic', 'avif'] }] }); p = typeof r === 'string' ? r : null; }
    else p = 'C:\\Users\\you\\Pictures\\wallpaper.jpg';
    if (p) { imagePath = p; fileLbl.textContent = p.split(/[\\/]/).pop() ?? p; onChange(); }
  } }, 'Choose image…');
  const solidRow = field('Colour', solid.el);
  const gradRow = h('div', {}, h('div', { class: 'two' }, field('From', from.el), field('To', to.el)), angle.el);
  const imgRow = h('div', { class: 'row' }, pickBtn, fileLbl);
  const kinds: [string, string][] = [['solid', 'Colour'], ['gradient', 'Gradient']];
  if (allowImage) kinds.push(['image', 'Image']);
  const sync = () => { solidRow.style.display = type === 'solid' ? '' : 'none'; gradRow.style.display = type === 'gradient' ? '' : 'none'; imgRow.style.display = type === 'image' ? '' : 'none'; };
  const seg = chips(kinds, 'gradient', (v) => { type = v as Background['type']; sync(); onChange(); });
  sync();
  const get = (): Background => type === 'solid' ? { type, color: solid.get() } : type === 'gradient' ? { type, from: from.get(), to: to.get(), angle: angle.get() } : { type, path: imagePath };
  const css = (): string => { const b = get(); return b.type === 'solid' ? b.color : b.type === 'gradient' ? `linear-gradient(${b.angle}deg, ${b.from}, ${b.to})` : '#ddd'; };
  void numberCtl;
  return { el: h('div', {}, seg.el, h('div', { style: 'height:10px' }), solidRow, gradRow, imgRow), get, css };
}
