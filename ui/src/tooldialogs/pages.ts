import type { Mount } from './common';
import { chips, field, numberCtl, selectCtl } from './common';
import { api } from '../api';
import { h } from '../dom';

/** "1-3,5,7-9" validated against the page count. */
export function rangesOk(txt: string, pages: number): boolean {
  if (!txt.trim()) return false;
  return txt.split(',').every((part) => {
    const m = /^\s*(\d+)\s*(?:-\s*(\d+))?\s*$/.exec(part);
    if (!m) return false;
    const a = Number(m[1]), b = m[2] ? Number(m[2]) : a;
    return a >= 1 && b >= a && (pages <= 0 || b <= pages);
  });
}

export const mountPages: Mount = async ({ paths, info, body, changed }) => {
  let pages = 0;
  try { pages = Number((await api.fileInfo(paths[0]) as { pages?: number }).pages ?? 0); } catch { /* unknown */ }
  const count = h('p', { class: 'muted small', style: 'margin-bottom:12px' }, pages ? `${paths.length > 1 ? 'First file has ' : ''}${pages} page${pages === 1 ? '' : 's'}.` : '');
  if (info.id === 'split-pdf') {
    let mode = 'each';
    const n = numberCtl(1, Math.max(1, pages || 999), 1, 2, changed, 'pages');
    const ranges = h('input', { type: 'text', placeholder: '1-3, 5, 7-9', value: '' });
    ranges.addEventListener('input', changed);
    const nRow = field('Pages per file', n.el), rRow = field('Page ranges', ranges, 'One output file per range.');
    const sync = () => { nRow.style.display = mode === 'every_n' ? '' : 'none'; rRow.style.display = mode === 'ranges' ? '' : 'none'; };
    const seg = chips([['each', 'Every page'], ['every_n', 'Every N pages'], ['ranges', 'Custom ranges']], 'each', (v) => { mode = v; sync(); changed(); });
    sync();
    body.append(h('div', { style: 'max-width:520px' }, count, h('h3', { style: 'margin-bottom:10px' }, 'Split'), seg.el, h('div', { style: 'height:12px' }), nRow, rRow));
    return { getParams: () => ({ mode, n: n.get(), ranges: ranges.value }), validate: () => (mode === 'ranges' && !rangesOk(ranges.value, pages) ? 'Enter page ranges like 1-3, 5, 7-9 within the document.' : null), cta: 'Split' };
  }
  const deg = selectCtl([['90', 'Clockwise 90°'], ['180', '180°'], ['270', 'Counter-clockwise 90°']], '90', changed);
  let which = 'all';
  const sel = h('input', { type: 'text', placeholder: '1, 3-5', value: '' });
  sel.addEventListener('input', changed);
  const selRow = field('Pages', sel, 'Applies to the first PDF’s page numbers for each file.');
  const sync = () => { selRow.style.display = which === 'some' ? '' : 'none'; };
  const seg = chips([['all', 'All pages'], ['some', 'Selected pages']], 'all', (v) => { which = v; sync(); changed(); });
  sync();
  body.append(h('div', { style: 'max-width:520px' }, count, field('Rotate', deg.el), seg.el, h('div', { style: 'height:12px' }), selRow));
  return { getParams: () => ({ degrees: Number(deg.get()), pages: which === 'all' ? 'all' : sel.value.trim() }), validate: () => (which === 'some' && !rangesOk(sel.value, pages) ? 'Enter pages like 1, 3-5 within the document.' : null), cta: 'Rotate' };
};
