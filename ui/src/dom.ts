// Tiny DOM helpers (no framework).
type Attrs = Record<string, string | number | boolean | ((e: Event) => void) | undefined | null>;
export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs = {}, ...kids: (Node | string | null | undefined | false)[]): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k.startsWith('on') && typeof v === 'function') el.addEventListener(k.slice(2).toLowerCase(), v as EventListener);
    else if (k === 'class') el.className = String(v);
    else if (k === 'html') el.innerHTML = String(v);
    else if (v === true) el.setAttribute(k, '');
    else el.setAttribute(k, String(v));
  }
  for (const kid of kids) {
    if (kid == null || kid === false) continue;
    if (kid instanceof Node) el.append(kid);
    else if (kid.startsWith('<svg')) el.insertAdjacentHTML('beforeend', kid); // icons from icons.ts
    else el.append(document.createTextNode(kid));
  }
  return el;
}
export const $ = <T extends HTMLElement = HTMLElement>(sel: string, root: ParentNode = document): T => root.querySelector(sel) as T;
export function debounce<A extends unknown[]>(fn: (...a: A) => void, ms: number): (...a: A) => void {
  let t: number | undefined;
  return (...a: A) => { window.clearTimeout(t); t = window.setTimeout(() => fn(...a), ms); };
}
export function applyTheme(theme: string): void {
  if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
}
export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ['KB', 'MB', 'GB']; let i = -1;
  do { n /= 1024; i++; } while (n >= 1024 && i < 2);
  return `${n.toFixed(n < 10 ? 1 : 0)} ${u[i]}`;
}
export function fmtTime(sec: number): string {
  const s = Math.max(0, sec); const m = Math.floor(s / 60); const r = s - m * 60;
  return `${m}:${r.toFixed(1).padStart(4, '0')}`;
}
export function parseTime(txt: string): number | null {
  const t = txt.trim(); if (!t) return null;
  if (t.includes(':')) { const [m, s] = t.split(':'); const v = Number(m) * 60 + Number(s); return Number.isFinite(v) ? v : null; }
  const v = Number(t); return Number.isFinite(v) ? v : null;
}
