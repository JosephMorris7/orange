import './styles/app.css';
import { api, on, isTauri, baseName, dirName, type Settings, type DepInfo, type JobDone, type JobProgress } from './api';
import { h, $, applyTheme } from './dom';
import { icon } from './icons';

const root = $('#app');
root.className = 'app';
let settings: Settings;
let deps: DepInfo = {};
let counts = { version: '', conversion_option_count: 0, tool_count: 0 };

interface JobRow { id: string; fraction: number; message: string; done?: JobDone }
const jobs: JobRow[] = [];

const KEYS = ['Shift', 'Control', 'Alt'] as const;
const DEP_NOTES: Record<string, string> = {
  ffmpeg: 'video and audio', ffprobe: 'media info', magick: 'images and all photo tools', pandoc: 'documents', soffice: 'Office files (optional, large)',
  pdftoppm: 'PDF to image', '7z': '7z and RAR archives', exiftool: 'metadata editing', qpdf: 'PDF split, merge, rotate, lock',
};

function toast(msg: string): void {
  const t = h('div', { class: 'toast', role: 'status' }, msg);
  document.body.append(t);
  setTimeout(() => t.remove(), 2400);
}

async function save(patch: (s: Settings) => void): Promise<void> {
  const next = structuredClone(settings);
  patch(next);
  try { settings = await api.setSettings(next); applyTheme(settings.theme); } catch (e) { toast(String(e)); }
  render();
}

/** Each modifier can only be bound once: picking one that is already taken swaps the two. */
function rebind(slot: 'convert' | 'tools' | 'actions', key: string): void {
  void save((s) => {
    const old = s.keys[slot];
    for (const other of ['convert', 'tools', 'actions'] as const) if (other !== slot && s.keys[other] === key) s.keys[other] = old;
    s.keys[slot] = key;
  });
}

function wheelMock(): string {
  // a small static wheel used as the illustration; drawn with the same maths as the real overlay
  const n = 6, R = 100, r = 36, c = 110, gap = 5;
  let d = '';
  for (let i = 0; i < n; i++) {
    const a0 = (i / n) * Math.PI * 2 - Math.PI / n, a1 = ((i + 1) / n) * Math.PI * 2 - Math.PI / n;
    const p = (rad: number, a: number) => `${(c + rad * Math.sin(a)).toFixed(1)} ${(c - rad * Math.cos(a)).toFixed(1)}`;
    const go = gap / 2 / R, gi = gap / 2 / r;
    const hot = i === 1;
    d += `<path d="M${p(R, a0 + go)} A${R} ${R} 0 0 1 ${p(R, a1 - go)} L${p(r, a1 - gi)} A${r} ${r} 0 0 0 ${p(r, a0 + gi)}Z" fill="${hot ? '#f25c05' : 'rgba(251,246,239,.1)'}"/>`;
  }
  const labels = ['JPG', 'PNG', 'WEBP', 'PDF', 'TIFF', 'AVIF'];
  const t = labels.map((l, i) => { const a = (i / n) * Math.PI * 2; const x = c + 68 * Math.sin(a), y = c - 68 * Math.cos(a); return `<text x="${x.toFixed(1)}" y="${(y + 4).toFixed(1)}" text-anchor="middle" font-size="12" font-weight="700" fill="${i === 1 ? '#16110d' : '#fbf6ef'}">${l}</text>`; }).join('');
  return `<svg viewBox="0 0 220 220" role="img" aria-label="Illustration of the Orange wheel"><circle cx="${c}" cy="${c}" r="${R + 6}" fill="#16110d"/>${d}${t}<circle cx="${c}" cy="${c}" r="${r - 6}" fill="#16110d" stroke="#ffb37a" stroke-opacity=".4"/><circle cx="${c}" cy="${c - 12}" r="3" fill="#f25c05"/></svg>`;
}

function hero(): HTMLElement {
  const k = settings.keys;
  return h('section', { class: 'card hero' },
    h('div', {},
      h('h2', {}, 'How Orange works'),
      h('ol', {},
        h('li', {}, h('b', {}, '1'), h('span', {}, 'Start dragging any file in Explorer, on the desktop or in another app.')),
        h('li', {}, h('b', {}, '2'), h('span', { html: `While you hold it, tap <kbd>${k.convert}</kbd> for formats, <kbd>${k.tools}</kbd> for tools or <kbd>${k.actions}</kbd> for actions.` })),
        h('li', {}, h('b', {}, '3'), h('span', {}, 'A wheel opens under your cursor. Drop the file on a wedge and it runs. Esc cancels.'))),
      h('p', { class: 'muted small', style: 'margin-top:12px' }, 'Everything runs offline on this PC. Converted files are saved next to the original and never overwrite it.'),
      h('div', { class: 'row wrap', style: 'margin-top:14px' },
        h('button', { class: 'btn primary', onclick: tryFiles }, icon('file-plus', 16), 'Choose files…'))),
    h('div', { class: 'illu', html: wheelMock() }));
}

async function tryFiles(): Promise<void> {
  const files = await api.pickFiles();
  if (files.length) await api.openWheel('convert', files);
}

function settingsCard(): HTMLElement {
  const s = settings;
  const keySel = (slot: 'convert' | 'tools' | 'actions', label: string) => {
    const sel = h('select', { 'aria-label': label, onchange: (e) => rebind(slot, (e.target as HTMLSelectElement).value) }, ...KEYS.map((k) => h('option', { value: k, selected: s.keys[slot] === k }, k)));
    return [h('label', {}, label), sel];
  };
  const toggle = (label: string, checked: boolean, fn: (v: boolean) => void) => h('label', { class: 'toggle' }, h('input', { type: 'checkbox', checked, onchange: (e) => fn((e.target as HTMLInputElement).checked) }), label);
  return h('section', { class: 'card' },
    h('div', { class: 'section-title' }, icon('sliders', 18), h('h2', {}, 'Settings')),
    h('div', { class: 'label', style: 'margin-bottom:8px' }, 'Tap while dragging'),
    h('div', { class: 'keys' }, ...keySel('convert', 'Convert wheel'), ...keySel('tools', 'Tools wheel'), ...keySel('actions', 'Actions wheel')),
    h('div', { class: 'field' },
      h('span', { class: 'label' }, 'Save results'),
      h('div', { class: 'seg', role: 'group', 'aria-label': 'Save location' },
        h('button', { 'aria-pressed': String(s.output.mode === 'beside'), onclick: () => void save((x) => { x.output.mode = 'beside'; }) }, 'Next to the original'),
        h('button', { 'aria-pressed': String(s.output.mode === 'folder'), onclick: () => void save((x) => { x.output.mode = 'folder'; }) }, 'Always in one folder')),
      s.output.mode === 'folder' && h('div', { class: 'row' },
        h('input', { type: 'text', readonly: true, value: s.output.folder ?? '', placeholder: 'Choose a folder…', 'aria-label': 'Output folder' }),
        h('button', { class: 'btn', onclick: () => void pickFolder() }, 'Browse…'))),
    toggle('Start Orange when I sign in', s.launch_at_login, (v) => void save((x) => { x.launch_at_login = v; })),
    toggle('Show a notification when a job finishes', s.notifications, (v) => void save((x) => { x.notifications = v; })),
    toggle('Play a short animation when Orange starts', s.startup_animation, (v) => void save((x) => { x.startup_animation = v; })),
    h('div', { class: 'field' }, h('label', { for: 'scale' }, `Wheel size · ${Math.round(s.wheel_scale * 100)}%`),
      h('input', { id: 'scale', type: 'range', min: 0.8, max: 1.4, step: 0.05, value: s.wheel_scale, onchange: (e) => void save((x) => { x.wheel_scale = Number((e.target as HTMLInputElement).value); }) })),
    h('div', { class: 'field' }, h('label', { for: 'theme' }, 'Theme'),
      h('select', { id: 'theme', onchange: (e) => void save((x) => { x.theme = (e.target as HTMLSelectElement).value as Settings['theme']; }) },
        ...(['system', 'light', 'dark'] as const).map((t) => h('option', { value: t, selected: s.theme === t }, t[0].toUpperCase() + t.slice(1))))));
}

async function pickFolder(): Promise<void> {
  let folder: string | null = null;
  if (isTauri()) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const r = await open({ directory: true, multiple: false, title: 'Save results to…' });
    folder = typeof r === 'string' ? r : null;
  } else folder = 'C:\\Users\\you\\Converted';
  if (folder) void save((x) => { x.output.folder = folder; x.output.mode = 'folder'; });
}

function jobsCard(): HTMLElement {
  const rows = jobs.slice().reverse().slice(0, 8).map((j) => {
    const d = j.done;
    const status = !d ? h('div', { class: 'progress', style: 'width:110px' }, h('i', { style: `width:${Math.round(j.fraction * 100)}%` }))
      : d.error ? h('span', { class: 'chip bad' }, 'Failed')
      : d.cancelled ? h('span', { class: 'chip' }, 'Cancelled')
      : h('button', { class: 'btn small', onclick: () => void api.revealInFolder(d.outputs[0]) }, icon('folder', 14), 'Show');
    const name = d ? (d.error ?? (d.outputs.length === 1 ? baseName(d.outputs[0]) : `${d.outputs.length} files in ${baseName(dirName(d.outputs[0] ?? ''))}`)) : j.message;
    return h('div', { class: 'job' }, h('span', { html: icon(d?.error ? 'alert' : d ? 'check' : 'gauge', 18) }), h('span', { class: 'name', title: name }, name), status);
  });
  return h('section', { class: 'card' },
    h('div', { class: 'section-title' }, icon('clock', 18), h('h2', {}, 'Recent jobs')),
    rows.length ? h('div', {}, ...rows) : h('p', { class: 'empty' }, 'Nothing yet. Drag a file and tap a key to start.'));
}

function depsCard(): HTMLElement {
  const rows = Object.entries(deps).map(([id, d]) => h('tr', {},
    h('td', {}, h('b', {}, d.name), h('div', { class: 'muted small' }, DEP_NOTES[id] ?? '')),
    h('td', {}, h('span', { class: `chip ${d.found ? 'ok' : 'bad'}` }, d.found ? 'Found' : 'Missing'))));
  const missing = Object.values(deps).filter((d) => !d.found).length;
  return h('section', { class: 'card' },
    h('div', { class: 'section-title' }, icon('archive', 18), h('h2', {}, 'Helper programs')),
    h('p', { class: 'muted small', style: 'margin-bottom:8px' }, missing ? `${missing} not found. Only the options that need them are greyed out in the wheel.` : 'All helper programs found.'),
    h('table', { class: 'deps' }, ...rows),
    h('div', { class: 'row-actions' },
      deps.soffice && !deps.soffice.found && h('button', { class: 'btn small', type: 'button', onclick: () => { void api.installLibreOffice().then(() => toast('LibreOffice setup opened in a new window.')).catch((e) => toast(String(e))); } }, 'Install LibreOffice'),
      h('button', { class: 'btn small', type: 'button', onclick: () => { void api.recheckHelpers().then((d) => { deps = d; render(); toast('Checked again.'); }); } }, 'Re-check')));
}

function render(): void {
  const parts: (Node | false)[] = [
    h('header', { class: 'top' }, h('img', { src: './mark.svg', alt: '' }), h('div', { class: 'grow' }, h('h1', {}, 'Orange'), h('p', { class: 'muted small' }, `Version ${counts.version} · ${counts.conversion_option_count} conversions · ${counts.tool_count} tools · 6 actions`))),
    !settings.first_run_done && h('div', { class: 'banner' }, icon('check', 20), h('span', {}, 'Orange is running in the tray and will start with Windows (you can change that below). Close this window any time; it keeps listening for drags.'),
      h('button', { class: 'btn primary', onclick: () => void save((x) => { x.first_run_done = true; }) }, 'Got it')),
    hero(),
    h('div', { class: 'grid2' }, settingsCard(), h('div', { style: 'display:grid;gap:18px' }, jobsCard(), depsCard())),
    h('p', { class: 'legal' }, 'Orange is an independent project. It is not affiliated with or endorsed by the makers of any similar macOS app. Cannot convert Pages or RTFD files (macOS-only formats).'),
  ];
  root.replaceChildren(...parts.filter((p): p is Node => p !== false));
}

async function init(): Promise<void> {
  [settings, deps, counts] = await Promise.all([api.getSettings(), api.capabilities(), api.appInfo()]);
  applyTheme(settings.theme);
  render();
  await on<Settings>('settings:changed', (s) => { settings = s; applyTheme(s.theme); render(); });
  await on<JobProgress>('job:progress', (p) => {
    let j = jobs.find((x) => x.id === p.job_id);
    if (!j) { j = { id: p.job_id, fraction: 0, message: '' }; jobs.push(j); }
    j.fraction = p.fraction; j.message = p.message; render();
  });
  await on<JobDone>('job:done', (d) => {
    let j = jobs.find((x) => x.id === d.job_id);
    if (!j) { j = { id: d.job_id, fraction: 1, message: '' }; jobs.push(j); }
    j.done = d; render();
  });
}
void init();
