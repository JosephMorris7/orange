import type { Mount } from './common';
import { field, toggleCtl } from './common';
import { api } from '../api';
import { h } from '../dom';

const toLocal = (taken: string): string => { const m = /^(\d{4}):(\d\d):(\d\d)[ T](\d\d):(\d\d)(?::(\d\d))?/.exec(taken); return m ? `${m[1]}-${m[2]}-${m[3]}T${m[4]}:${m[5]}` : ''; };
const toExif = (v: string): string => { const m = /^(\d{4})-(\d\d)-(\d\d)T(\d\d):(\d\d)/.exec(v); return m ? `${m[1]}:${m[2]}:${m[3]} ${m[4]}:${m[5]}:00` : ''; };

export const mountMetadata: Mount = async ({ paths, body, changed }) => {
  const single = paths.length === 1;
  let meta: Record<string, unknown> = {};
  if (single) { try { meta = await api.readMetadata(paths[0]); } catch { /* shows empty fields */ } }
  const gps = (meta.gps ?? null) as { lat: number; lon: number } | null;
  const str = (k: string) => (typeof meta[k] === 'string' ? (meta[k] as string) : '');
  const ph = single ? '' : 'Keep each file’s value';
  const mk = (key: string, label: string, val: string, type = 'text') => {
    const input = h('input', { type, value: val, placeholder: ph, autocomplete: 'off' });
    input.addEventListener('input', changed);
    return { key, input, initial: val, row: field(label, input) };
  };
  const rows = [mk('Make', 'Camera make', str('make')), mk('Model', 'Camera model', str('model')), mk('DateTimeOriginal', 'Date taken', toLocal(str('taken')), 'datetime-local'),
    mk('GPSLatitude', 'Latitude', gps ? String(gps.lat) : '', 'number'), mk('GPSLongitude', 'Longitude', gps ? String(gps.lon) : '', 'number'),
    mk('Artist', 'Author', str('artist')), mk('Copyright', 'Copyright', str('copyright')), mk('ImageDescription', 'Description', str('description'))];
  for (const r of rows) if (r.key.startsWith('GPS')) r.input.setAttribute('step', 'any');
  const sGps = toggleCtl('Remove location (GPS)', false, changed), sCam = toggleCtl('Remove camera details', false, changed), sAll = toggleCtl('Remove all metadata', false, changed);
  const inPlace = toggleCtl('Change the original instead of saving a copy', false, changed);
  body.append(h('div', { style: 'max-width:620px' },
    paths.length > 1 ? h('p', { class: 'muted small', style: 'margin-bottom:12px' }, `Only the fields you fill in change, on all ${paths.length} files.`) : null,
    h('div', { class: 'two' }, ...rows.map((r) => r.row)),
    h('h3', { style: 'margin:8px 0 10px' }, 'Remove'), sGps.el, sCam.el, sAll.el,
    h('div', { style: 'height:10px' }), inPlace.el));
  return {
    getParams: () => {
      const set: Record<string, string | number> = {};
      for (const r of rows) {
        const v = r.input.value.trim();
        if (v === r.initial.trim() && single) continue;
        if (!v) continue;
        set[r.key] = r.key === 'DateTimeOriginal' ? toExif(v) : r.key.startsWith('GPS') ? Number(v) : v;
      }
      const strip = [sGps.get() && 'gps', sCam.get() && 'camera', sAll.get() && 'all'].filter(Boolean);
      return { set, strip, in_place: inPlace.get() };
    },
    validate: () => {
      for (const r of rows) if (r.key.startsWith('GPS') && r.input.value && !Number.isFinite(Number(r.input.value))) return 'Latitude and longitude must be numbers.';
      return null;
    },
    cta: 'Save metadata',
  };
};
