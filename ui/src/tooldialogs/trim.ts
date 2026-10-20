import type { Mount } from './common';
import { toggleCtl } from './common';
import { api, fileUrl } from '../api';
import { h, fmtTime, parseTime } from '../dom';

export const mountTrim: Mount = async ({ paths, info, body, changed }) => {
  const fi = await api.fileInfo(paths[0]);
  const duration = Number((fi as { duration?: number }).duration ?? 0);
  const isVideo = info.kinds.includes('video');
  const media = h(isVideo ? 'video' : 'audio', { controls: true, preload: 'metadata', src: fileUrl(paths[0]) }) as HTMLMediaElement;
  let start = 0, end = duration;
  const sIn = h('input', { type: 'text', value: fmtTime(start), 'aria-label': 'Start' }), eIn = h('input', { type: 'text', value: fmtTime(end), 'aria-label': 'End' });
  const sR = h('input', { type: 'range', min: 0, max: duration, step: 0.1, value: 0, 'aria-label': 'Start slider' }), eR = h('input', { type: 'range', min: 0, max: duration, step: 0.1, value: duration, 'aria-label': 'End slider' });
  const note = h('p', { class: 'muted small' });
  const precise = toggleCtl('Frame-accurate cut (slower, re-encodes)', false, changed);
  function sync(): void {
    sIn.value = fmtTime(start); eIn.value = fmtTime(end); sR.value = String(start); eR.value = String(end);
    note.textContent = `Keeps ${fmtTime(Math.max(0, end - start))} of ${fmtTime(duration)}.`; changed();
  }
  sR.addEventListener('input', () => { start = Math.min(Number(sR.value), end); sync(); media.currentTime = start; });
  eR.addEventListener('input', () => { end = Math.max(Number(eR.value), start); sync(); });
  sIn.addEventListener('change', () => { const v = parseTime(sIn.value); start = v == null ? start : Math.min(Math.max(0, v), duration); sync(); });
  eIn.addEventListener('change', () => { const v = parseTime(eIn.value); end = v == null ? end : Math.min(Math.max(0, v), duration); sync(); });
  const useCur = (which: 'start' | 'end') => h('button', { class: 'btn small', type: 'button', onclick: () => { const t = media.currentTime; if (which === 'start') start = Math.min(t, end); else end = Math.max(t, start); sync(); } }, 'Use current time');
  sync();
  body.append(h('div', { class: 'cols' }, h('div', {}, media),
    h('div', { class: 'side' },
      h('h3', {}, 'Start'), h('div', { class: 'row' }, sIn, useCur('start')), sR,
      h('h3', {}, 'End'), h('div', { class: 'row' }, eIn, useCur('end')), eR,
      h('div', { style: 'height:8px' }), note, precise.el)));
  return { getParams: () => ({ start, end, precise: precise.get() }), validate: () => (end - start < 0.1 ? 'The end must be after the start.' : null), cta: 'Trim' };
};
