import type { Mount } from './common';
import { field, imageStage, livePreview, selectCtl, slider } from './common';
import { backgroundCtl } from './bg';
import { h } from '../dom';

export const mountBackground: Mount = async ({ paths, info, body, changed }) => {
  const stage = await imageStage(paths[0]);
  const m = Math.max(stage.width, stage.height);
  const prev = livePreview(stage, info.id, paths[0], () => params(), 1000);
  const upd = () => { prev.refresh(); changed(); };
  const bg = backgroundCtl(upd, true);
  const ratio = selectCtl([['auto', 'Fit the picture'], ['1:1', 'Square 1:1'], ['16:9', 'Wide 16:9'], ['4:5', 'Portrait 4:5'], ['9:16', 'Story 9:16'], ['3:4', '3:4'], ['4:3', '4:3']], 'auto', upd);
  const padding = slider('Spacing', 0, Math.round(m / 4), Math.max(1, Math.round(m / 200)), Math.round(m * 0.07), upd);
  const corner = slider('Corners', 0, Math.round(m / 8), Math.max(1, Math.round(m / 400)), Math.round(m * 0.02), upd);
  const sBlur = slider('Shadow blur', 0, 100, 1, 30, upd), sOp = slider('Shadow', 0, 100, 1, 35, upd), sOff = slider('Shadow drop', 0, 80, 1, 16, upd);
  const params = () => ({ background: bg.get(), ratio: ratio.get(), padding: padding.get(), corner_radius: corner.get(), shadow: { blur: sBlur.get(), opacity: sOp.get() / 100, offset_y: sOff.get() } });
  body.append(h('div', { class: 'cols' }, stage.el, h('div', { class: 'side' },
    h('h3', {}, 'Background'), bg.el,
    h('h3', {}, 'Canvas'), field('Shape', ratio.el), padding.el, corner.el,
    h('h3', {}, 'Shadow'), sBlur.el, sOp.el, sOff.el,
    paths.length > 1 ? h('p', { class: 'muted small' }, `Applies to all ${paths.length} images. Preview shows the first.`) : null)));
  return { getParams: params, validate: () => { const b = bg.get(); return b.type === 'image' && !b.path ? 'Choose a background image.' : null; }, cta: 'Add background' };
};
