import { api, fileUrl, type ParamSpec, type ToolInfo } from '../api';
import { h, debounce } from '../dom';

export interface DialogCtx { info: ToolInfo; paths: string[]; body: HTMLElement; changed: () => void }
export interface Dialog { getParams(): unknown; getPaths?(): string[]; validate?(): string | null; cta?: string }
export type Mount = (ctx: DialogCtx) => Promise<Dialog> | Dialog;

let uid = 0;
export const nextId = (p = 'f'): string => `${p}${++uid}`;

export function field(label: string, control: HTMLElement, help?: string | null): HTMLElement {
  const id = nextId();
  control.id ||= id;
  return h('div', { class: 'field' }, h('label', { for: control.id }, label), control, help ? h('div', { class: 'help' }, help) : null);
}

export interface Ctl<T> { el: HTMLElement; get(): T; set(v: T): void }

export function slider(label: string, min: number, max: number, step: number, value: number, onInput: () => void): Ctl<number> {
  const out = h('output', {}, String(value));
  const input = h('input', { type: 'range', min, max, step, value, 'aria-label': label });
  input.addEventListener('input', () => { out.textContent = input.value; onInput(); });
  return { el: h('div', { class: 'sl' }, h('span', {}, label), input, out), get: () => Number(input.value), set: (v) => { input.value = String(v); out.textContent = String(v); } };
}

export function numberCtl(min: number, max: number, step: number, value: number, onInput: () => void, unit?: string | null): Ctl<number> {
  const input = h('input', { type: 'number', min, max, step, value });
  input.addEventListener('input', onInput);
  const el = unit ? h('div', { class: 'unit' }, input, h('span', {}, unit)) : input;
  return { el, get: () => { const v = Number(input.value); return Number.isFinite(v) ? v : value; }, set: (v) => { input.value = String(v); } };
}

export function selectCtl(options: [string, string][], value: string, onInput: () => void): Ctl<string> {
  const sel = h('select', {}, ...options.map(([v, l]) => h('option', { value: v, selected: v === value }, l)));
  sel.addEventListener('change', onInput);
  return { el: sel, get: () => sel.value, set: (v) => { sel.value = v; } };
}

export function colorCtl(value: string, onInput: () => void): Ctl<string> {
  const input = h('input', { type: 'color', value });
  input.addEventListener('input', onInput);
  return { el: input, get: () => input.value, set: (v) => { input.value = v; } };
}

export function chips(options: [string, string][], value: string, onPick: (v: string) => void): Ctl<string> {
  let cur = value;
  const wrap = h('div', { class: 'chips', role: 'group' });
  const paint = () => wrap.querySelectorAll('button').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.v === cur)));
  for (const [v, l] of options) wrap.append(h('button', { type: 'button', 'data-v': v, onclick: () => { cur = v; paint(); onPick(v); } }, l));
  paint();
  return { el: wrap, get: () => cur, set: (v) => { cur = v; paint(); } };
}

export function toggleCtl(label: string, value: boolean, onInput: () => void): Ctl<boolean> {
  const input = h('input', { type: 'checkbox', checked: value });
  input.addEventListener('change', onInput);
  return { el: h('label', { class: 'toggle' }, input, label), get: () => input.checked, set: (v) => { input.checked = v; } };
}

// ---- generic form from ParamSpec[] ------------------------------------------------------------------
export function buildForm(specs: ParamSpec[], onChange: () => void): { el: HTMLElement; values(): Record<string, unknown> } {
  const wrap = h('div', {});
  const ctls = new Map<string, { spec: ParamSpec; get(): unknown; row: HTMLElement }>();
  const refresh = () => {
    for (const { spec, row } of ctls.values()) {
      const w = spec.show_when;
      row.style.display = !w || w.values.includes(String(ctls.get(w.key)?.get())) ? '' : 'none';
    }
  };
  const changed = () => { refresh(); onChange(); };
  for (const spec of specs) {
    let ctl: { el: HTMLElement; get(): unknown };
    switch (spec.type) {
      case 'number': ctl = numberCtl(spec.min, spec.max, spec.step, spec.default, changed, spec.unit); break;
      case 'select': ctl = selectCtl(spec.options, spec.default, changed); break;
      case 'toggle': ctl = toggleCtl(spec.label, spec.default, changed); break;
      case 'color': ctl = colorCtl(spec.default, changed); break;
      case 'text': {
        const input = h('input', { type: spec.key === 'password' ? 'password' : 'text', value: spec.default, placeholder: spec.placeholder ?? '', autocomplete: 'off' });
        input.addEventListener('input', changed);
        ctl = { el: input, get: () => input.value };
        break;
      }
    }
    const row = spec.type === 'toggle' ? h('div', { class: 'field' }, ctl.el) : field(spec.label, ctl.el, spec.help);
    ctls.set(spec.key, { spec, get: () => ctl.get(), row });
    wrap.append(row);
  }
  refresh();
  return { el: wrap, values: () => Object.fromEntries([...ctls].map(([k, c]) => [k, c.get()])) };
}

// ---- image stage: the picture with an SVG overlay in *source pixel* coordinates ----------------------
export interface Stage {
  el: HTMLElement; inner: HTMLElement; svg: SVGSVGElement; img: HTMLImageElement; width: number; height: number;
  point(e: PointerEvent | MouseEvent): { x: number; y: number };
  setUrl(u: string): void;
}

export async function imageStage(path: string, maxDim = 1600): Promise<Stage> {
  const info = await api.fileInfo(path);
  if (info.kind !== 'image') throw new Error('Not an image');
  const width = Number((info as { width: number }).width), height = Number((info as { height: number }).height);
  const url = fileUrl(await api.webPreview(path, maxDim));
  const img = h('img', { src: url, alt: '', draggable: false });
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
  svg.setAttribute('preserveAspectRatio', 'none');
  const el = h('div', { class: 'stage' }, img);
  el.append(svg);
  const point = (e: PointerEvent | MouseEvent) => {
    const m = svg.getScreenCTM();
    if (!m) return { x: 0, y: 0 };
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    return { x: Math.min(width, Math.max(0, p.x)), y: Math.min(height, Math.max(0, p.y)) };
  };
  return { el: h('div', { class: 'stagewrap' }, el), inner: el, svg, img, width, height, point, setUrl: (u) => { img.src = u; } };
}

/** Debounced, out-of-order-safe live preview: calls the backend and swaps the image when the newest result arrives. */
export function livePreview(stage: Stage, toolId: string, path: string, getParams: () => unknown, maxDim = 900): { refresh(): void; showOriginal(on: boolean): void } {
  let seq = 0;
  const originalUrl = stage.img.src;
  let currentUrl = originalUrl;
  const run = debounce(async () => {
    const mine = ++seq;
    try {
      const out = await api.previewTool(toolId, path, getParams(), maxDim);
      if (mine === seq) { currentUrl = fileUrl(out); stage.setUrl(currentUrl); }
    } catch (e) { console.warn('preview failed', e); }
  }, 220);
  return { refresh: run, showOriginal: (on) => stage.setUrl(on ? originalUrl : currentUrl) };
}

export const svgEl = <K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number> = {}): SVGElementTagNameMap[K] => {
  const e = document.createElementNS('http://www.w3.org/2000/svg', tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
};

export const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));
