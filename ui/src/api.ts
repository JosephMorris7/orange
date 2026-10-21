// Thin typed layer over the Tauri backend. In a plain browser (npm run dev) the mock backend answers instead.
import { convertFileSrc } from '@tauri-apps/api/core';
import { mock } from './mock';

export type Mode = 'convert' | 'tools' | 'actions';

export interface WheelItem { id: string; label: string; icon?: string | null; enabled: boolean; hint?: string | null }

export type ParamKind =
  | { type: 'number'; min: number; max: number; step: number; default: number; unit?: string | null }
  | { type: 'select'; options: [string, string][]; default: string }
  | { type: 'toggle'; default: boolean }
  | { type: 'text'; default: string; placeholder?: string | null }
  | { type: 'color'; default: string };
export type ParamSpec = ParamKind & { key: string; label: string; help?: string | null; show_when?: { key: string; values: string[] } };

export type ToolUi = 'none' | 'form' | 'crop' | 'edit_photo' | 'annotate' | 'add_background' | 'redact' | 'collage' | 'metadata' | 'trim' | 'reorder' | 'pages';
export interface ToolInfo {
  id: string; label: string; icon: string; title: string; description: string;
  kinds: string[]; min_files: number; max_files: number | null; ui: ToolUi; params: ParamSpec[]; deps: string[];
}

export interface Settings {
  keys: { convert: string; tools: string; actions: string };
  output: { mode: 'beside' | 'folder'; folder: string | null };
  launch_at_login: boolean; notifications: boolean; wheel_scale: number;
  theme: 'system' | 'light' | 'dark'; first_run_done: boolean;
}

export interface JobDone { job_id: string; outputs: string[]; error: string | null; cancelled: boolean }
export interface JobProgress { job_id: string; fraction: number; message: string }
export interface WheelShow { mode: Mode; x: number; y: number; scale: number; paths?: string[] | null }
export interface Region { x: number; y: number; w: number; h: number }
export type DepInfo = Record<string, { name: string; path: string | null; found: boolean }>;
export type FileInfo = { kind: 'image'; width: number; height: number; format: string } | { kind: 'video' | 'audio'; duration: number; width?: number; height?: number; has_audio?: boolean } | { kind: 'pdf'; pages: number } | { kind: string; [k: string]: unknown };

export const isTauri = (): boolean => typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== 'undefined';

export async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!isTauri()) return mock.invoke(cmd, args) as Promise<T>;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

export async function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!isTauri()) return mock.listen(event, handler as (p: unknown) => void);
  const { listen } = await import('@tauri-apps/api/event');
  return listen<T>(event, (e) => handler(e.payload));
}

/** Turn an absolute file path into something an <img> can load. */
export function fileUrl(path: string): string {
  return isTauri() ? convertFileSrc(path) : mock.fileUrl(path);
}

export const baseName = (p: string): string => p.split(/[\\/]/).pop() ?? p;
export const dirName = (p: string): string => { const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/')); return i > 0 ? p.slice(0, i) : p; };
export const extOf = (p: string): string => { const b = baseName(p); const i = b.lastIndexOf('.'); return i > 0 ? b.slice(i + 1).toLowerCase() : ''; };

export const api = {
  appInfo: () => call<{ version: string; conversion_option_count: number; tool_count: number; action_count: number }>('app_info'),
  capabilities: () => call<DepInfo>('capabilities'),
  recheckHelpers: () => call<DepInfo>('recheck_helpers'),
  installLibreOffice: () => call<void>('install_libreoffice'),
  wheelItems: (mode: Mode, paths: string[]) => call<WheelItem[]>('wheel_items', { mode, paths }),
  wheelCommit: (mode: Mode, id: string, paths: string[]) => call<{ kind: 'job'; job_id: string } | { kind: 'dialog' } | { kind: 'done' }>('wheel_commit', { mode, id, paths }),
  wheelCancel: () => call<void>('wheel_cancel'),
  runConvert: (paths: string[], to: string) => call<string>('run_convert', { paths, to }),
  runTool: (toolId: string, paths: string[], params: unknown) => call<string>('run_tool', { toolId, paths, params }),
  cancelJob: (jobId: string) => call<void>('cancel_job', { jobId }),
  toolInfo: (toolId: string) => call<ToolInfo>('tool_info', { toolId }),
  previewTool: (toolId: string, path: string, params: unknown, maxDim = 900) => call<string>('preview_tool', { toolId, path, params, maxDim }),
  webPreview: (path: string, maxDim = 1600) => call<string>('web_preview', { path, maxDim }),
  fileInfo: (path: string) => call<FileInfo>('file_info', { path }),
  readMetadata: (path: string) => call<Record<string, unknown>>('read_metadata', { path }),
  detectRegions: (path: string, what: 'text' | 'faces') => call<Region[]>('detect_regions', { path, what }),
  openToolWindow: (toolId: string, paths: string[]) => call<void>('open_tool_window', { toolId, paths }),
  runAction: (action: string, paths: string[]) => call<{ ok: boolean; message?: string }>('run_action', { action, paths }),
  shelfList: () => call<string[]>('shelf_list'),
  shelfAdd: (paths: string[]) => call<string[]>('shelf_add', { paths }),
  shelfRemove: (path: string) => call<string[]>('shelf_remove', { path }),
  shelfClear: () => call<string[]>('shelf_clear'),
  revealInFolder: (path: string) => call<void>('reveal_in_folder', { path }),
  getSettings: () => call<Settings>('get_settings'),
  setSettings: (settings: Settings) => call<Settings>('set_settings', { settings }),
  pickFiles: () => call<string[]>('pick_files'),
  openWheel: (mode: Mode, paths: string[]) => call<void>('open_wheel', { mode, paths }),
  dragIcon: () => call<string>('drag_icon'),
  log: (msg: string) => (isTauri() ? call<void>('log_line', { msg }).catch(() => undefined) : Promise.resolve()),
};

/** Run a job to completion: resolves with its outputs, rejects with the error text (or 'cancelled'). */
export async function waitForJob(jobId: string, onProgress?: (p: JobProgress) => void): Promise<string[]> {
  return new Promise(async (resolve, reject) => {
    const offs: (() => void)[] = [];
    const finish = () => offs.forEach((f) => f());
    offs.push(await on<JobProgress>('job:progress', (p) => { if (p.job_id === jobId) onProgress?.(p); }));
    offs.push(await on<JobDone>('job:done', (d) => {
      if (d.job_id !== jobId) return;
      finish();
      if (d.cancelled) reject(new Error('cancelled'));
      else if (d.error) reject(new Error(d.error));
      else resolve(d.outputs);
    }));
  });
}
