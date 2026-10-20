import type { Mount } from './common';
import { field, numberCtl, selectCtl, slider } from './common';
import { backgroundCtl } from './bg';
import { reorderList } from './reorder';
import { api, fileUrl } from '../api';
import { h } from '../dom';

export const mountCollage: Mount = ({ paths, body, changed }) => {
  const preview = h('div', { class: 'collage' });
  const thumbs = new Map<string, string>();
  const upd = () => { paint(); changed(); };
  const rl = reorderList(paths, upd);
  const layout = selectCtl([['grid', 'Grid'], ['row', 'Single row'], ['column', 'Single column'], ['featured', 'Featured first']], 'grid', upd);
  const cols = numberCtl(1, 8, 1, Math.min(3, Math.max(2, Math.ceil(Math.sqrt(paths.length)))), upd);
  const spacing = slider('Spacing', 0, 120, 2, 24, upd), corner = slider('Corners', 0, 120, 2, 24, upd);
  const cell = numberCtl(200, 3000, 50, 800, upd, 'px');
  const bg = backgroundCtl(upd, false);
  const params = () => ({ layout: layout.get(), columns: cols.get(), spacing: spacing.get(), corner_radius: corner.get(), cell_size: cell.get(), background: bg.get() });

  function paint(): void {
    const files = rl.get(), p = params();
    const n = p.layout === 'row' ? files.length : p.layout === 'column' ? 1 : Math.max(1, p.columns);
    preview.style.gridTemplateColumns = `repeat(${n}, 1fr)`;
    preview.style.background = bg.css();
    preview.style.gap = `${Math.round(p.spacing / 4)}px`;
    preview.style.padding = `${Math.round(p.spacing / 3)}px`;
    preview.replaceChildren(...files.map((f, i) => {
      const d = h('div', { style: `background-image:url('${thumbs.get(f) ?? ''}');border-radius:${Math.round(p.corner_radius / 5)}px;${p.layout === 'featured' && i === 0 ? `grid-column:span ${Math.min(2, n)};grid-row:span 2;aspect-ratio:auto;` : ''}` });
      return d;
    }));
  }
  for (const f of paths) void api.webPreview(f, 240).then((t) => { thumbs.set(f, fileUrl(t)); paint(); }).catch(() => undefined);
  paint();
  body.append(h('div', { class: 'cols' }, h('div', {}, h('h3', { style: 'margin-bottom:8px' }, 'Rough layout preview'), preview, h('h3', { style: 'margin:16px 0 8px' }, 'Order (first = featured)'), rl.el),
    h('div', { class: 'side' }, h('h3', {}, 'Layout'), field('Style', layout.el), field('Columns', cols.el), spacing.el, corner.el, field('Cell size', cell.el),
      h('h3', {}, 'Background'), bg.el)));
  return { getParams: params, getPaths: () => rl.get(), cta: 'Make collage' };
};
