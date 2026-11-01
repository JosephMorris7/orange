// The finish card (see finish.rs): shown by the tray when a job is done. Rust sends what to say; this draws it.
// "Show in folder" selects the result in Explorer; "Undo" asks Rust to delete the files that job created (never the originals).
import { api, on, isTauri, type FinishCard } from './api';
import { h, $ } from './dom';

const root = $('#app');
if (!isTauri()) document.body.classList.add('dev');

const SHOW_MS = 8000;
let card: FinishCard | null = null;
let remaining = SHOW_MS;
let hovering = false;
let timer: number | undefined;
let busy = false;

function close(): void {
  window.clearInterval(timer);
  void api.finishClose();
}

function tickIcon(): string {
  return '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5 L10 17.5 L19 7"/></svg>';
}

async function undo(): Promise<void> {
  if (busy) return;
  busy = true;
  window.clearInterval(timer);
  let n = 0;
  try { n = await api.finishUndo(); } catch { n = 0; }
  root.replaceChildren(h('div', { class: 'card' }, h('div', { class: 'row' }, h('div', { class: 'txt' },
    h('div', { class: 'title' }, n > 0 ? 'Removed' : 'Nothing to undo'),
    h('div', { class: 'detail' }, n > 0 ? `${n} file${n === 1 ? '' : 's'} deleted. Your originals were not touched.` : 'The result was left where it is.')))));
  window.setTimeout(close, 1800);
}

async function reveal(): Promise<void> {
  if (card) { try { await api.revealInFolder(card.first_output); } catch { /* Explorer could not open it */ } }
  close();
}

function render(): void {
  if (!card) return;
  const c = card;
  const bar = h('div', {}); const barWrap = h('div', { class: 'bar', 'aria-hidden': 'true' }, bar);
  const undoBtn = c.can_undo ? h('button', { class: 'btn ghost', onclick: () => void undo() }, '') : null;
  const setUndoLabel = () => { if (undoBtn) undoBtn.textContent = `Undo · ${Math.max(1, Math.ceil(remaining / 1000))}s`; };
  const el = h('div', { class: 'card', role: 'status', 'aria-live': 'polite' },
    h('div', { class: 'row' },
      h('div', { class: 'tick', html: tickIcon() }),
      h('div', { class: 'txt' }, h('div', { class: 'title', title: c.title }, c.title), h('div', { class: 'detail' }, c.detail)),
      h('button', { class: 'x', 'aria-label': 'Dismiss', onclick: close }, '×')),
    h('div', { class: 'btns' }, h('button', { class: 'btn', onclick: () => void reveal() }, 'Show in folder'), undoBtn),
    barWrap);
  el.addEventListener('pointerenter', () => { hovering = true; });
  el.addEventListener('pointerleave', () => { hovering = false; });
  root.replaceChildren(el);
  remaining = SHOW_MS; busy = false;
  window.clearInterval(timer);
  setUndoLabel();
  timer = window.setInterval(() => {
    if (hovering) return;
    remaining -= 50;
    bar.style.width = `${Math.max(0, (remaining / SHOW_MS) * 100)}%`;
    setUndoLabel();
    if (remaining <= 0) close();
  }, 50);
}

async function init(): Promise<void> {
  await on<FinishCard>('finish:show', (c) => { card = c; render(); });
  card = await api.finishReady();
  if (card) render(); else close();
}
window.addEventListener('keydown', (e) => { if (e.key === 'Escape') close(); });
void init();
