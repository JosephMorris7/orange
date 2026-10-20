import type { Mount } from './common';
import { buildForm } from './common';
import { api, baseName, extOf, fileUrl, isTauri } from '../api';
import { h } from '../dom';
import { icon, iconForExt } from '../icons';

/** A reorderable list of files (buttons and drag and drop). */
export function reorderList(initial: string[], onChange: () => void): { el: HTMLElement; get(): string[] } {
  let order = [...initial];
  const list = h('div', { class: 'list' });
  const thumbs = new Map<string, string>();
  async function thumb(p: string): Promise<void> {
    if (thumbs.has(p) || iconForExt(extOf(p)) !== 'image-plus') return;
    try { thumbs.set(p, fileUrl(await api.webPreview(p, 160))); render(); } catch { thumbs.set(p, ''); }
  }
  function move(i: number, d: number): void { const j = i + d; if (j < 0 || j >= order.length) return; [order[i], order[j]] = [order[j], order[i]]; render(); onChange(); }
  let dragIdx = -1;
  function render(): void {
    list.replaceChildren(...order.map((p, i) => {
      const t = thumbs.get(p);
      const row = h('div', { class: 'li', draggable: true, title: p },
        h('span', { class: 'n' }, String(i + 1)),
        t ? h('img', { src: t, alt: '' }) : h('span', { style: 'width:44px;height:44px;display:grid;place-items:center;color:var(--orange)' }, icon(iconForExt(extOf(p)), 24)),
        h('span', { class: 'nm' }, baseName(p)),
        h('button', { class: 'btn ghost icon-only', 'aria-label': 'Move up', disabled: i === 0, onclick: () => move(i, -1) }, icon('up', 16)),
        h('button', { class: 'btn ghost icon-only', 'aria-label': 'Move down', disabled: i === order.length - 1, onclick: () => move(i, 1) }, icon('down', 16)));
      row.addEventListener('dragstart', () => { dragIdx = i; row.classList.add('dragging'); });
      row.addEventListener('dragend', () => row.classList.remove('dragging'));
      row.addEventListener('dragover', (e) => { e.preventDefault(); row.classList.add('over'); });
      row.addEventListener('dragleave', () => row.classList.remove('over'));
      row.addEventListener('drop', (e) => { e.preventDefault(); if (dragIdx < 0 || dragIdx === i) return; const [m] = order.splice(dragIdx, 1); order.splice(i, 0, m); dragIdx = -1; render(); onChange(); });
      return row;
    }));
    if (!isTauri() || order.length) for (const p of order) void thumb(p);
  }
  render();
  const sort = h('button', { class: 'btn small', type: 'button', onclick: () => { order.sort((a, b) => baseName(a).localeCompare(baseName(b), undefined, { numeric: true })); render(); onChange(); } }, 'Sort by name');
  return { el: h('div', {}, h('div', { class: 'row', style: 'margin-bottom:10px' }, sort, h('span', { class: 'muted small' }, 'Drag rows or use the arrows.')), list), get: () => [...order] };
}

export const mountReorder: Mount = ({ info, paths, body, changed }) => {
  const rl = reorderList(paths, changed);
  const form = info.params.length ? buildForm(info.params, changed) : null;
  body.append(h('div', { class: 'cols' }, rl.el, h('div', { class: 'side' }, form ? h('div', {}, h('h3', {}, 'Options'), form.el) : h('p', { class: 'muted small' }, info.description))));
  return { getParams: () => (form ? form.values() : {}), getPaths: () => rl.get(), cta: info.label[0] + info.label.slice(1).toLowerCase() };
};
