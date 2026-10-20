import './styles/tool.css';
import { api, on, isTauri, baseName, waitForJob, type ToolInfo } from './api';
import { h, $, applyTheme } from './dom';
import { icon } from './icons';
import type { Dialog, Mount } from './tooldialogs/common';

const root = $('#app');

async function loadMount(ui: ToolInfo['ui']): Promise<Mount> {
  switch (ui) {
    case 'form': return (await import('./tooldialogs/form')).mountForm;
    case 'edit_photo': return (await import('./tooldialogs/edit')).mountEdit;
    case 'crop': return (await import('./tooldialogs/crop')).mountCrop;
    case 'redact': return (await import('./tooldialogs/redact')).mountRedact;
    case 'annotate': return (await import('./tooldialogs/annotate')).mountAnnotate;
    case 'add_background': return (await import('./tooldialogs/background')).mountBackground;
    case 'collage': return (await import('./tooldialogs/collage')).mountCollage;
    case 'metadata': return (await import('./tooldialogs/metadata')).mountMetadata;
    case 'trim': return (await import('./tooldialogs/trim')).mountTrim;
    case 'reorder': return (await import('./tooldialogs/reorder')).mountReorder;
    case 'pages': return (await import('./tooldialogs/pages')).mountPages;
    default: return (await import('./tooldialogs/form')).mountForm;
  }
}

async function closeWindow(): Promise<void> {
  if (isTauri()) { const { getCurrentWindow } = await import('@tauri-apps/api/window'); await getCurrentWindow().close(); } else window.close();
}

async function init(): Promise<void> {
  const q = new URLSearchParams(location.search);
  const toolId = q.get('tool') ?? 'compress-image';
  let paths: string[] = [];
  try { paths = JSON.parse(q.get('paths') ?? '[]') as string[]; } catch { /* handled below */ }
  if (!paths.length && !isTauri()) paths = (q.get('demo_paths') ?? 'C:\\Users\\you\\Pictures\\sunset.png|C:\\Users\\you\\Pictures\\beach.jpg').split('|');
  applyTheme((await api.getSettings()).theme);

  let info: ToolInfo;
  try { info = await api.toolInfo(toolId); } catch (e) { root.textContent = String(e); return; }
  document.title = `${info.title} — Orange`;

  const body = h('div', {});
  const error = h('span', { class: 'err', role: 'alert' });
  const run = h('button', { class: 'btn primary', type: 'button' }, 'Run');
  const cancel = h('button', { class: 'btn', type: 'button', onclick: () => void closeWindow() }, 'Cancel');
  const names = paths.length === 1 ? baseName(paths[0]) : `${paths.length} files`;
  const shell = h('div', { class: 'shell' },
    h('header', {}, h('span', { class: 'ti' }, icon(info.icon, 22)), h('div', { class: 'grow' }, h('h1', {}, info.title), h('p', { class: 'muted small' }, `${names} · ${info.description}`))),
    h('main', {}, body),
    h('footer', {}, error, h('span', { class: 'grow' }), cancel, run));
  root.replaceChildren(shell);

  let dialog: Dialog;
  try {
    const mount = await loadMount(info.ui);
    dialog = await mount({ info, paths, body, changed: () => { error.textContent = ''; } });
  } catch (e) { body.replaceChildren(h('p', { class: 'err' }, `Couldn't open this tool: ${String(e instanceof Error ? e.message : e)}`)); run.setAttribute('disabled', ''); return; }
  run.textContent = dialog.cta ?? 'Run';

  run.addEventListener('click', async () => {
    const problem = dialog.validate?.() ?? null;
    if (problem) { error.textContent = problem; return; }
    const files = dialog.getPaths?.() ?? paths;
    const bar = h('i', {}), msg = h('p', { class: 'muted' }, 'Starting…');
    const stop = h('button', { class: 'btn', type: 'button' }, 'Stop');
    shell.querySelector('main')!.replaceChildren(h('div', { style: 'max-width:520px;margin:40px auto;display:grid;gap:14px' }, h('h2', {}, `Working on ${names}`), h('div', { class: 'progress' }, bar), msg, h('div', {}, stop)));
    shell.querySelector('footer')!.replaceChildren();
    try {
      const id = await api.runTool(info.id, files, dialog.getParams());
      stop.addEventListener('click', () => { void api.cancelJob(id); stop.setAttribute('disabled', ''); });
      const outs = await waitForJob(id, (p) => { bar.style.width = `${Math.round(p.fraction * 100)}%`; msg.textContent = p.message; });
      showDone(outs);
    } catch (e) {
      const m = e instanceof Error ? e.message : String(e);
      if (m === 'cancelled') { void closeWindow(); return; }
      showFail(m);
    }
  });

  function showDone(outs: string[]): void {
    shell.querySelector('main')!.replaceChildren(h('div', { class: 'done', style: 'max-width:560px;margin:30px auto' },
      h('h2', { style: 'margin-bottom:14px' }, outs.length === 1 ? 'Done. Saved 1 file.' : `Done. Saved ${outs.length} files.`),
      ...outs.slice(0, 12).map((o) => h('div', { class: 'li' }, icon('check', 18), h('span', { class: 'nm', title: o }, baseName(o)), h('button', { class: 'btn small', onclick: () => void api.revealInFolder(o) }, icon('folder', 14), 'Show'))),
      outs.length > 12 ? h('p', { class: 'muted small' }, `…and ${outs.length - 12} more in the same folder.`) : null));
    shell.querySelector('footer')!.replaceChildren(h('span', { class: 'grow' }), h('button', { class: 'btn primary', onclick: () => void closeWindow() }, 'Close'));
  }
  function showFail(msg: string): void {
    shell.querySelector('main')!.replaceChildren(h('div', { style: 'max-width:560px;margin:30px auto' }, h('h2', { class: 'err', style: 'margin-bottom:10px' }, 'That didn’t work'), h('p', {}, msg)));
    shell.querySelector('footer')!.replaceChildren(h('span', { class: 'grow' }), h('button', { class: 'btn', onclick: () => void closeWindow() }, 'Close'));
  }
  void on;
}
void init();
