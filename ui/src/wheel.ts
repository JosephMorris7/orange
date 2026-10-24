import { api, isTauri, on, baseName, type Mode, type WheelItem, type WheelShow } from './api';
import { icon } from './icons';

// ---- geometry (viewBox units; the window is 640x640 logical at scale 1) -------------------------
const SIZE = 640, C = 320, R_OUT = 262, R_IN = 94, GEO_GAP = 10, MAX_PER_PAGE = 8;
const TAU = Math.PI * 2;

interface Slot { item: WheelItem | null; more: boolean }

const stage = document.getElementById('stage')!;
const DEV = !isTauri();
if (DEV) document.body.classList.add('dev');

let mode: Mode = 'convert';
let paths: string[] = [];
let items: WheelItem[] = [];
let page = 0;
let hover = -1;
let visible = false;
let loadSeq = 0;
let moreTimer: number | undefined;
let slots: Slot[] = [];
let empty = false; // the file has been checked and nothing can be done with it

const titles: Record<Mode, string> = { convert: 'CONVERT', tools: 'TOOLS', actions: 'ACTIONS' };

function point(r: number, a: number): [number, number] { return [C + r * Math.sin(a), C - r * Math.cos(a)]; }

function wedgePath(a0: number, a1: number): string {
  const go = (GEO_GAP / 2) / R_OUT, gi = (GEO_GAP / 2) / R_IN;
  const [x0, y0] = point(R_OUT, a0 + go), [x1, y1] = point(R_OUT, a1 - go);
  const [x2, y2] = point(R_IN, a1 - gi), [x3, y3] = point(R_IN, a0 + gi);
  const large = a1 - a0 > Math.PI ? 1 : 0;
  return `M${x0.toFixed(2)} ${y0.toFixed(2)} A${R_OUT} ${R_OUT} 0 ${large} 1 ${x1.toFixed(2)} ${y1.toFixed(2)} L${x2.toFixed(2)} ${y2.toFixed(2)} A${R_IN} ${R_IN} 0 ${large} 0 ${x3.toFixed(2)} ${y3.toFixed(2)}Z`;
}

function buildSlots(): void {
  const pageSize = items.length <= MAX_PER_PAGE ? MAX_PER_PAGE : MAX_PER_PAGE - 1;
  const pages = Math.max(1, Math.ceil(items.length / pageSize));
  page = Math.min(page, pages - 1);
  const slice = items.slice(page * pageSize, page * pageSize + pageSize);
  slots = slice.map((item) => ({ item, more: false }));
  if (pages > 1) slots.push({ item: null, more: true });
}

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));

function centreText(): { title: string; sub: string } {
  const slot = hover >= 0 ? slots[hover] : undefined;
  if (slot?.more) return { title: 'MORE', sub: 'hold to flip the page' };
  if (slot?.item) return { title: slot.item.label, sub: slot.item.hint ?? (mode === 'convert' ? `to ${slot.item.label}` : '') };
  if (!paths.length) return { title: titles[mode], sub: 'move onto the wheel' };
  if (empty && paths.length) return { title: titles[mode], sub: 'no options for this file' };
  const name = paths.length === 1 ? baseName(paths[0]) : `${paths.length} files`;
  return { title: titles[mode], sub: name.length > 26 ? name.slice(0, 24) + '…' : name };
}

function render(opening = false): void {
  buildSlots();
  const n = Math.max(slots.length, 1);
  const w = TAU / n;
  const ghost = !paths.length && !slots.length;
  const wedges = (ghost ? Array.from({ length: 6 }, () => ({ item: null, more: false }) as Slot) : slots).map((s, i, arr) => {
    const nn = arr.length, ww = TAU / nn;
    const mid = i * ww, a0 = mid - ww / 2, a1 = mid + ww / 2;
    const [lx, ly] = point((R_IN + R_OUT) / 2 + 6, mid);
    const it = s.item;
    const hot = i === hover && !ghost;
    const cls = ['wedge', hot ? 'hot' : '', it && !it.enabled ? 'off' : '', s.more ? 'more' : '', it && !it.icon ? 'big' : ''].join(' ');
    let inner = '';
    if (s.more) inner = `<g class="ico" transform="translate(${lx - 12} ${ly - 20})">${icon('more', 24)}</g><text class="lbl" x="${lx}" y="${ly + 22}">MORE</text>`;
    else if (it?.icon) inner = `<g class="ico" transform="translate(${lx - 12} ${ly - 24})">${icon(it.icon, 24)}</g><text class="lbl" x="${lx}" y="${ly + 22}">${esc(it.label)}</text>`;
    else if (it) inner = `<text class="lbl" x="${lx}" y="${ly + 6}">${esc(it.label)}</text>`;
    return `<g class="${cls}" data-i="${i}" style="--i:${i}"><path d="${wedgePath(a0, a1)}"/>${inner}</g>`;
  }).join('');
  void w;
  const c = centreText();
  stage.className = opening ? 'opening' : '';
  stage.innerHTML = `<svg class="wheel" viewBox="0 0 ${SIZE} ${SIZE}" role="application" aria-label="${titles[mode]} wheel">
    <g class="disc-group"><circle class="shadow-ring" cx="${C}" cy="${C}" r="${R_OUT + 6}"/><circle class="disc" cx="${C}" cy="${C}" r="${R_OUT + 10}"/></g>
    ${wedges}
    <g class="hub-group"><circle class="hub" cx="${C}" cy="${C}" r="${R_IN - 10}"/>
      <circle class="hub-dot" cx="${C}" cy="${C - 40}" r="3.5"/>
      <text class="hub-title" x="${C}" y="${C + 4}">${esc(c.title)}</text><text class="hub-sub" x="${C}" y="${C + 26}">${esc(c.sub)}</text></g>
  </svg>`;
}

function updateHover(next: number): void {
  if (next === hover) return;
  hover = next;
  window.clearTimeout(moreTimer);
  const slot = hover >= 0 ? slots[hover] : undefined;
  if (slot?.more) moreTimer = window.setTimeout(() => { page += 1; if (page * (MAX_PER_PAGE - 1) >= items.length) page = 0; hover = -1; render(); }, 450);
  // cheap update: toggle classes instead of re-rendering the whole SVG
  stage.querySelectorAll('.wedge').forEach((g, i) => g.classList.toggle('hot', i === hover && slots.length > 0));
  const c = centreText();
  const t = stage.querySelector('.hub-title'), s = stage.querySelector('.hub-sub');
  if (t) t.textContent = c.title;
  if (s) s.textContent = c.sub;
}

/** Map a point in the webview (CSS px) to a wedge index, -1 for the hub / outside. */
function hitTest(cssX: number, cssY: number): number {
  const box = stage.getBoundingClientRect();
  const k = SIZE / box.width;
  const dx = (cssX - box.left) * k - C, dy = (cssY - box.top) * k - C;
  const r = Math.hypot(dx, dy);
  if (r < R_IN - 4 || r > R_OUT + 14 || !slots.length) return -1;
  const a = (Math.atan2(dx, -dy) + TAU) % TAU;
  const w = TAU / slots.length;
  return Math.floor(((a + w / 2) % TAU) / w);
}

async function loadItems(): Promise<void> {
  const seq = ++loadSeq;
  if (!paths.length) { items = []; render(); return; }
  try {
    const got = await api.wheelItems(mode, paths);
    void api.log(`wheel items for ${mode}: ${got.length}`);
    if (seq !== loadSeq) return;
    items = got; page = 0; hover = -1; empty = got.length === 0;
  } catch (e) { void api.log(`wheel items failed: ${String(e)}`); items = []; empty = false; }
  render();
}

async function commit(index: number): Promise<void> {
  const slot = slots[index];
  if (!slot || slot.more || !slot.item || !paths.length) { void api.wheelCancel(); return; }
  if (!slot.item.enabled) {
    const g = stage.querySelectorAll('.wedge')[index];
    g?.classList.add('shake');
    updateHover(index);
    window.setTimeout(() => void api.wheelCancel(), 1100);
    return;
  }
  try { await api.wheelCommit(mode, slot.item.id, paths); } catch (e) { console.error(e); void api.wheelCancel(); }
}

function reset(): void {
  visible = false; paths = []; items = []; slots = []; hover = -1; page = 0; loadSeq++; empty = false;
  window.clearTimeout(moreTimer);
}

function show(p: WheelShow): void {
  void api.log(`wheel:show ${p.mode} (ui)`);
  const wasVisible = visible;
  mode = p.mode;
  visible = true;
  document.body.classList.remove('hidden-wheel');
  if (p.paths && p.paths.length) paths = p.paths;
  hover = -1; page = 0;
  render(!wasVisible);
  if (paths.length) void loadItems();
}

// ---- wiring ---------------------------------------------------------------------------------------
void on<WheelShow>('wheel:show', show);
void on<void>('wheel:hide', () => { reset(); render(); });

const toCss = (x: number, y: number) => ({ x: x / devicePixelRatio, y: y / devicePixelRatio });
let lastDropAt = 0;
/// One handler for file drags, fed by both the native drop target (wheel:drag) and the web view's own events.
function handleDrag(type: string, files: string[] | undefined, x: number, y: number): void {
  const c = toCss(x, y);
  if (type === 'enter') {
    void api.log(`drag enter: ${files?.length ?? 0} file(s), mode=${mode}`);
    if (files && files.length) paths = files;
    void loadItems().then(() => updateHover(hitTest(c.x, c.y)));
  } else if (type === 'over') updateHover(hitTest(c.x, c.y));
  else if (type === 'drop') {
    if (Date.now() - lastDropAt < 800) return; // both sources may report the same drop
    lastDropAt = Date.now();
    void api.log(`drag drop: ${files?.length ?? 0} file(s)`);
    if (files && files.length) paths = files;
    void commit(hitTest(c.x, c.y));
  } else updateHover(-1);
}

interface NativeDrag { type: string; paths?: string[]; x?: number; y?: number }
void on<NativeDrag>('wheel:drag', (e) => handleDrag(e.type, e.paths, e.x ?? 0, e.y ?? 0));

if (isTauri()) {
  void import('@tauri-apps/api/webview').then(async ({ getCurrentWebview }) => {
    await getCurrentWebview().onDragDropEvent((e) => {
      const ev = e.payload;
      if (ev.type === 'leave') handleDrag('leave', undefined, 0, 0);
      else handleDrag(ev.type, 'paths' in ev ? ev.paths : undefined, ev.position.x, ev.position.y);
    });
  });
}

// Pointer interaction: used when the wheel is opened without a drag (tray, settings button) and in the browser preview.
stage.addEventListener('pointermove', (e) => updateHover(hitTest(e.clientX, e.clientY)));
stage.addEventListener('pointerleave', () => updateHover(-1));
stage.addEventListener('click', (e) => { const i = hitTest(e.clientX, e.clientY); if (i < 0) void api.wheelCancel(); else void commit(i); });
window.addEventListener('keydown', (e) => { if (e.key === 'Escape') void api.wheelCancel(); });

render();
if (DEV) {
  const q = new URLSearchParams(location.search);
  const demo = q.get('demo');
  if (demo) {
    const file = q.get('file') ?? 'C:\\Users\\you\\Pictures\\sunset.png';
    window.setTimeout(() => {
      show({ mode: demo as Mode, x: 0, y: 0, scale: 1, paths: file.split('|') });
      const h = q.get('hover');
      if (h) window.setTimeout(() => updateHover(Number(h)), 700);
    }, 80);
  }
}
