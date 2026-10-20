import './styles/shelf.css';
import { api, on, isTauri, baseName, dirName, extOf } from './api';
import { h, $, applyTheme } from './dom';
import { icon, iconForExt } from './icons';

const root = $('#app');
let list: string[] = [];
let dragIcon = '';

async function startDragOut(paths: string[]): Promise<void> {
  if (!isTauri() || !paths.length) return;
  const { startDrag } = await import('@crabnebula/tauri-plugin-drag');
  await startDrag({ item: paths, icon: dragIcon });
}

async function hideWindow(): Promise<void> {
  if (!isTauri()) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow().hide();
}

function render(): void {
  const items = list.map((p) => {
    const row = h('div', { class: 'item', draggable: true, title: p },
      icon(iconForExt(extOf(p)), 20),
      h('div', { class: 'grow', style: 'min-width:0' }, h('div', { class: 'nm' }, baseName(p)), h('div', { class: 'dir' }, dirName(p))),
      h('button', { class: 'btn ghost icon-only', 'aria-label': `Remove ${baseName(p)}`, onclick: (e) => { e.stopPropagation(); void api.shelfRemove(p); } }, icon('close', 14)));
    row.addEventListener('dragstart', (e) => { e.preventDefault(); void startDragOut([p]); });
    return row;
  });
  root.replaceChildren(h('div', { class: 'shelf' },
    h('div', { class: 'bar', 'data-tauri-drag-region': true }, h('img', { src: './mark.svg', alt: '' }), h('b', { 'data-tauri-drag-region': true }, 'Float shelf'),
      h('button', { class: 'btn ghost icon-only', 'aria-label': 'Hide shelf', onclick: () => void hideWindow() }, icon('close', 16))),
    h('div', {}, items.length ? h('div', { class: 'list' }, ...items) : h('div', { class: 'drop', id: 'drop' }, 'Drop files here to park them, then drag them out anywhere.'),
      items.length ? h('div', { class: 'drop', id: 'drop', style: 'padding:12px' }, 'Drop more files here') : null),
    h('div', { class: 'foot' },
      h('button', { class: 'btn small', disabled: !list.length, onclick: () => void startDragOut(list) }, icon('layers', 14), 'Drag all out'),
      h('span', { class: 'grow' }),
      h('button', { class: 'btn small', disabled: !list.length, onclick: () => void api.shelfClear() }, icon('trash', 14), 'Clear'))));
}

async function init(): Promise<void> {
  applyTheme((await api.getSettings()).theme);
  list = await api.shelfList();
  try { dragIcon = await api.dragIcon(); } catch { dragIcon = ''; }
  render();
  await on<string[]>('shelf:changed', (l) => { list = l; render(); });
  if (isTauri()) {
    const { getCurrentWebview } = await import('@tauri-apps/api/webview');
    await getCurrentWebview().onDragDropEvent((e) => {
      const drop = document.getElementById('drop');
      if (e.payload.type === 'enter' || e.payload.type === 'over') drop?.classList.add('over');
      else if (e.payload.type === 'drop') { drop?.classList.remove('over'); void api.shelfAdd(e.payload.paths); }
      else drop?.classList.remove('over');
    });
  }
}
void init();
