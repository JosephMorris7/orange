// A stand-in backend so every page can run (and be screenshotted) in a plain browser.
// It mirrors docs/CONTRACT.md; nothing here ships to users of the real app.
import type { ToolInfo, ParamSpec, WheelItem, Settings } from './api';

type Handler = (p: unknown) => void;
const listeners = new Map<string, Set<Handler>>();
export function emit(event: string, payload: unknown): void { listeners.get(event)?.forEach((h) => h(payload)); }

const num = (key: string, label: string, min: number, max: number, step: number, def: number, unit?: string, help?: string, when?: [string, string[]]): ParamSpec =>
  ({ type: 'number', key, label, min, max, step, default: def, unit, help, show_when: when ? { key: when[0], values: when[1] } : undefined });
const sel = (key: string, label: string, options: [string, string][], def: string, help?: string, when?: [string, string[]]): ParamSpec =>
  ({ type: 'select', key, label, options, default: def, help, show_when: when ? { key: when[0], values: when[1] } : undefined });
const txt = (key: string, label: string, def: string, placeholder?: string): ParamSpec => ({ type: 'text', key, label, default: def, placeholder });

const T = (id: string, label: string, icon: string, title: string, description: string, kinds: string[], min: number, max: number | null, ui: ToolInfo['ui'], params: ParamSpec[] = [], deps: string[] = []): ToolInfo =>
  ({ id, label, icon, title, description, kinds, min_files: min, max_files: max, ui, params, deps });

const presets: [string, string][] = [['small', 'Smallest'], ['balanced', 'Balanced'], ['high', 'High quality']];
export const TOOLS: ToolInfo[] = [
  T('compress-image', 'COMPRESS', 'compress', 'Compress images', 'Reduce file size to a target size, a percentage, or a preset.', ['image'], 1, null, 'form', [
    sel('mode', 'Method', [['preset', 'Preset'], ['target_size', 'Target file size'], ['reduce', 'Reduce by percentage']], 'preset'),
    sel('preset', 'Preset', presets, 'balanced', undefined, ['mode', ['preset']]),
    num('target_kb', 'Target size', 10, 50000, 10, 500, 'KB', undefined, ['mode', ['target_size']]),
    num('percent', 'Reduce by', 5, 95, 5, 50, '%', undefined, ['mode', ['reduce']]),
    num('max_dimension', 'Longest side (0 = keep)', 0, 8000, 10, 0, 'px'),
  ], ['magick']),
  T('edit-metadata', 'METADATA', 'tag', 'Edit metadata', 'Edit camera, date, location and author tags, or strip location / all metadata.', ['image'], 1, null, 'metadata', [], ['exiftool']),
  T('edit-photo', 'EDIT', 'sliders', 'Edit photo', 'Exposure, colour, sharpness, effects, rotate and flip with a live preview.', ['image'], 1, 1, 'edit_photo', [], ['magick']),
  T('annotate-photo', 'ANNOTATE', 'pencil', 'Annotate photo', 'Add arrows, shapes, text and numbered callouts.', ['image'], 1, 1, 'annotate', [], ['magick']),
  T('add-background', 'ADD BG', 'image-plus', 'Add a background', 'Place the image on a colour, gradient or another image.', ['image'], 1, null, 'add_background', [], ['magick']),
  T('crop-image', 'CROP', 'crop', 'Crop images', 'Crop to a region or aspect ratio.', ['image'], 1, null, 'crop', [], ['magick']),
  T('redact-photo', 'REDACT', 'eye-off', 'Redact photo', 'Cover faces, text or any region. Metadata is stripped.', ['image'], 1, 1, 'redact', [], ['magick']),
  T('resize-image', 'RESIZE', 'resize', 'Resize images', 'Scale by width, height, percentage or to fit inside a box.', ['image'], 1, null, 'form', [
    sel('mode', 'Resize by', [['width', 'Width'], ['height', 'Height'], ['percent', 'Percentage'], ['fit', 'Fit inside box']], 'width'),
    num('width', 'Width', 1, 20000, 1, 1920, 'px', undefined, ['mode', ['width', 'fit']]),
    num('height', 'Height', 1, 20000, 1, 1080, 'px', undefined, ['mode', ['height', 'fit']]),
    num('percent', 'Percentage', 1, 400, 1, 50, '%', undefined, ['mode', ['percent']]),
  ], ['magick']),
  T('create-pdf', 'TO PDF', 'file-plus', 'Create a PDF', 'Combine the selected images into one PDF, one page per image.', ['image'], 2, null, 'reorder', [
    sel('page_size', 'Page size', [['fit', 'Fit to image'], ['a4', 'A4'], ['letter', 'US Letter']], 'fit'),
    sel('orientation', 'Orientation', [['auto', 'Automatic'], ['portrait', 'Portrait'], ['landscape', 'Landscape']], 'auto'),
    num('margin', 'Margin', 0, 100, 1, 0, 'pt'),
  ], ['magick']),
  T('collage', 'COLLAGE', 'grid', 'Make a collage', 'Combine images in a grid, row, column or featured layout.', ['image'], 2, 30, 'collage', [], ['magick']),
  T('compress-video', 'COMPRESS', 'compress', 'Compress video', 'Re-encode to H.264 with a smaller size.', ['video'], 1, null, 'form', [
    sel('mode', 'Method', [['preset', 'Preset'], ['target_mb', 'Target file size']], 'preset'),
    sel('preset', 'Preset', presets, 'balanced', undefined, ['mode', ['preset']]),
    num('target_mb', 'Target size', 1, 20000, 1, 25, 'MB', undefined, ['mode', ['target_mb']]),
    sel('max_height', 'Limit height', [['0', 'Keep original'], ['2160', '2160p'], ['1080', '1080p'], ['720', '720p'], ['480', '480p']], '0'),
  ], ['ffmpeg', 'ffprobe']),
  T('trim-video', 'TRIM', 'scissors', 'Trim video', 'Keep only the part between a start and an end time.', ['video'], 1, 1, 'trim', [], ['ffmpeg', 'ffprobe']),
  T('merge-video', 'MERGE', 'merge', 'Merge videos', 'Join videos end to end in the order shown.', ['video'], 2, null, 'reorder', [], ['ffmpeg', 'ffprobe']),
  T('remove-audio', 'MUTE', 'volume-x', 'Remove audio', 'Save a copy of the video without its sound track.', ['video'], 1, null, 'none', [], ['ffmpeg', 'ffprobe']),
  T('resize-video', 'RESIZE', 'resize', 'Resize video', 'Scale the video to a new height.', ['video'], 1, null, 'form', [sel('height', 'Height', [['2160', '2160p'], ['1440', '1440p'], ['1080', '1080p'], ['720', '720p'], ['480', '480p'], ['360', '360p']], '720')], ['ffmpeg', 'ffprobe']),
  T('compress-audio', 'COMPRESS', 'gauge', 'Compress audio', 'Re-encode at a lower bitrate.', ['audio'], 1, null, 'form', [sel('bitrate_kbps', 'Bitrate', [['64', '64 kbps'], ['96', '96 kbps'], ['128', '128 kbps'], ['192', '192 kbps'], ['256', '256 kbps']], '128')], ['ffmpeg', 'ffprobe']),
  T('trim-audio', 'TRIM', 'scissors', 'Trim audio', 'Keep only the part between a start and an end time.', ['audio'], 1, 1, 'trim', [], ['ffmpeg', 'ffprobe']),
  T('merge-audio', 'MERGE', 'merge', 'Merge audio', 'Join audio files end to end.', ['audio'], 2, null, 'reorder', [], ['ffmpeg', 'ffprobe']),
  T('normalize-audio', 'LEVEL', 'music', 'Normalize volume', 'Even out loudness to a target level.', ['audio'], 1, null, 'form', [num('target_lufs', 'Target loudness', -30, -5, 1, -16, 'LUFS', '-16 suits most streaming and podcasts.')], ['ffmpeg', 'ffprobe']),
  T('compress-pdf', 'COMPRESS', 'compress', 'Compress PDF', 'Re-compress embedded images and optimize structure.', ['pdf'], 1, null, 'form', [sel('level', 'Level', [['low', 'Light (best quality)'], ['medium', 'Balanced'], ['high', 'Strong (smallest)']], 'medium')]),
  T('split-pdf', 'SPLIT', 'split', 'Split PDF', 'Split into single pages, chunks, or custom ranges.', ['pdf'], 1, 1, 'pages', [], ['qpdf']),
  T('merge-pdf', 'MERGE', 'merge', 'Merge PDFs', 'Combine PDFs into one document.', ['pdf'], 2, null, 'reorder', [], ['qpdf']),
  T('rotate-pdf', 'ROTATE', 'rotate', 'Rotate PDF pages', 'Rotate all pages or selected pages.', ['pdf'], 1, null, 'pages', [], ['qpdf']),
  T('protect-pdf', 'LOCK', 'lock', 'Lock or unlock PDF', 'Add or remove a password.', ['pdf'], 1, null, 'form', [sel('action', 'Action', [['lock', 'Lock with password'], ['unlock', 'Remove password']], 'lock'), txt('password', 'Password', '', 'Enter password')], ['qpdf']),
  T('shift-subtitles', 'SHIFT', 'clock', 'Shift subtitle timing', 'Move every cue earlier or later.', ['subtitle'], 1, null, 'form', [num('offset_ms', 'Shift by', -600000, 600000, 100, 1000, 'ms', 'Positive = later, negative = earlier.')]),
  T('create-archive', 'ZIP', 'archive', 'Create an archive', 'Pack files and folders into one archive.', ['image', 'video', 'audio', 'pdf', 'document', 'folder', 'other'], 1, null, 'form', [sel('format', 'Format', [['zip', 'ZIP'], ['tar', 'TAR'], ['gz', 'TAR.GZ'], ['7z', '7z (needs 7-Zip)']], 'zip'), txt('name', 'Archive name', 'Archive')]),
];

const ext = (p: string) => (p.split('.').pop() ?? '').toLowerCase();
const kindOf = (p: string): string => {
  const e = ext(p);
  if (/^(png|jpe?g|webp|gif|bmp|tiff?|heic|avif|svg)$/.test(e)) return 'image';
  if (/^(mp4|mov|mkv|avi|webm|m4v)$/.test(e)) return 'video';
  if (/^(mp3|wav|flac|m4a|aac|ogg|opus)$/.test(e)) return 'audio';
  if (e === 'pdf') return 'pdf';
  if (/^(srt|vtt)$/.test(e)) return 'subtitle';
  return 'other';
};
const TARGETS: Record<string, string[]> = {
  image: ['jpg', 'png', 'webp', 'pdf', 'tiff', 'bmp', 'avif', 'heic'],
  video: ['mp4', 'mov', 'mkv', 'webm', 'gif', 'mp3'],
  audio: ['mp3', 'wav', 'flac', 'm4a', 'ogg', 'opus'],
  pdf: ['png', 'jpg', 'txt', 'docx'],
  subtitle: ['srt', 'vtt'],
  other: ['zip'],
};

const ACTIONS: WheelItem[] = [
  ['share', 'SHARE', 'share'], ['float', 'FLOAT', 'layers'], ['copy', 'COPY', 'copy'], ['move', 'MOVE', 'folder-move'], ['copy_path', 'COPY PATH', 'link'], ['terminal', 'TERMINAL', 'terminal'],
].map(([id, label, icon]) => ({ id, label, icon, enabled: true, hint: null }));

let settings: Settings = {
  keys: { convert: 'Shift', tools: 'Control', actions: 'Alt' }, output: { mode: 'beside', folder: null },
  launch_at_login: false, notifications: true, wheel_scale: 1, theme: 'system', first_run_done: false,
};
let shelf: string[] = [];
let jobSeq = 0;

function sampleSvg(tint = 0): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1000" viewBox="0 0 1600 1000"><defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsl(${28 + tint},95%,72%)"/><stop offset="1" stop-color="hsl(${14 + tint},90%,55%)"/></linearGradient></defs><rect width="1600" height="1000" fill="url(#s)"/><circle cx="1150" cy="300" r="120" fill="#fff6e6" opacity=".9"/><path d="M0 760 L300 480 L560 700 L860 400 L1180 720 L1600 520 V1000 H0Z" fill="#16110d" opacity=".85"/><path d="M0 860 L420 640 L760 820 L1100 620 L1600 840 V1000 H0Z" fill="#0e8c7f" opacity=".85"/></svg>`;
  return 'data:image/svg+xml;utf8,' + encodeURIComponent(svg);
}

async function invoke(cmd: string, a: Record<string, unknown>): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 30));
  const paths = (a.paths as string[]) ?? [];
  switch (cmd) {
    case 'app_info': return { version: '0.1.0', conversion_option_count: 248, tool_count: 26, action_count: 6 };
    case 'capabilities': return Object.fromEntries(['ffmpeg', 'ffprobe', 'magick', 'pandoc', 'soffice', 'pdftoppm', '7z', 'exiftool', 'qpdf'].map((d) => [d, { name: d, path: d === 'soffice' || d === '7z' ? null : `C:\\Program Files\\Orange\\bin\\${d}.exe`, found: !(d === 'soffice' || d === '7z') }]));
    case 'wheel_items': {
      const mode = a.mode as string;
      if (mode === 'actions') return ACTIONS;
      if (mode === 'tools') return TOOLS.filter((t) => paths.length >= t.min_files && (t.max_files == null || paths.length <= t.max_files) && paths.every((p) => t.kinds.includes(kindOf(p)))).map((t) => ({ id: t.id, label: t.label, icon: t.icon, enabled: !t.deps.includes('soffice'), hint: null }));
      const k = kindOf(paths[0] ?? ''); const own = ext(paths[0] ?? '').replace('jpeg', 'jpg');
      return (TARGETS[k] ?? []).filter((t) => t !== own).map((t) => ({ id: t, label: t.toUpperCase(), icon: null, enabled: t !== 'heic', hint: t === 'heic' ? 'Needs ImageMagick with HEIC support' : null }));
    }
    case 'wheel_commit': return { kind: a.mode === 'actions' ? 'done' : (a.mode === 'tools' && TOOLS.find((t) => t.id === a.id)?.ui !== 'none') ? 'dialog' : 'job', job_id: 'job-' + ++jobSeq };
    case 'wheel_cancel': return null;
    case 'run_convert': case 'run_tool': {
      const id = 'job-' + ++jobSeq; let f = 0;
      const t = setInterval(() => {
        f += 0.2; emit('job:progress', { job_id: id, fraction: Math.min(f, 1), message: 'Working…' });
        if (f >= 1) { clearInterval(t); emit('job:done', { job_id: id, outputs: paths.map((p) => p.replace(/(\.[^.\\/]+)?$/, a.to ? `.${a.to}` : ' edited$1')), error: null, cancelled: false }); }
      }, 220);
      return id;
    }
    case 'cancel_job': return null;
    case 'tool_info': { const t = TOOLS.find((x) => x.id === a.toolId); if (!t) throw 'unknown tool'; return t; }
    case 'preview_tool': return sampleSvg(Math.round(Number((a.params as Record<string, number>)?.warmth ?? 0) / 4));
    case 'web_preview': return sampleSvg();
    case 'file_info': {
      const k = kindOf(a.path as string);
      if (k === 'image') return { kind: 'image', width: 1600, height: 1000, format: ext(a.path as string).toUpperCase() };
      if (k === 'pdf') return { kind: 'pdf', pages: 6 };
      return { kind: k, duration: 94.5, width: 1920, height: 1080, has_audio: true };
    }
    case 'read_metadata': return { make: 'Fujifilm', model: 'X-T4', lens: null, taken: '2024:06:02 18:21:07', gps: { lat: 35.6595, lon: 139.7005 }, artist: '', copyright: '', description: '', software: null, raw: {} };
    case 'detect_regions': return a.what === 'faces' ? [{ x: 620, y: 220, w: 220, h: 260 }] : [{ x: 160, y: 120, w: 520, h: 70 }, { x: 160, y: 820, w: 380, h: 60 }];
    case 'open_tool_window': return null;
    case 'run_action': return { ok: true, message: 'Done' };
    case 'shelf_list': return shelf;
    case 'shelf_add': shelf = [...new Set([...shelf, ...paths])]; emit('shelf:changed', shelf); return shelf;
    case 'shelf_remove': shelf = shelf.filter((p) => p !== a.path); emit('shelf:changed', shelf); return shelf;
    case 'shelf_clear': shelf = []; emit('shelf:changed', shelf); return shelf;
    case 'reveal_in_folder': return null;
    case 'get_settings': return settings;
    case 'set_settings': settings = a.settings as Settings; emit('settings:changed', settings); return settings;
    case 'pick_files': return ['C:\\Users\\you\\Pictures\\sunset.png'];
    case 'open_wheel': emit('wheel:show', { mode: a.mode, x: 0, y: 0, scale: 1, paths }); return null;
    case 'drag_icon': return '';
    default: throw `mock: unknown command ${cmd}`;
  }
}

export const mock = {
  invoke,
  listen: async (event: string, h: Handler) => { if (!listeners.has(event)) listeners.set(event, new Set()); listeners.get(event)!.add(h); return () => { listeners.get(event)?.delete(h); }; },
  fileUrl: (p: string) => (/^(data:|blob:|https?:)/.test(p) ? p : sampleSvg(p.length % 40)),
  sampleSvg,
};
