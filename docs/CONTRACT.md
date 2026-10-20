# Orange — architecture & contract

Orange is a Windows drag-and-drop file converter/toolbox (an original product inspired by the
"drag a file, tap a key, drop on a radial wheel" interaction). Offline only. Tauri 2 (Rust + web UI).

```
orange/
  Cargo.toml                workspace (root owner: shell workstream adds "src-tauri")
  crates/orange-core        FROZEN base: Kind, Dep/Capabilities, Ctx, exec, paths, Error   (additive changes only)
  crates/orange-convert     conversion engine                                  [workstream: convert]
  crates/orange-tools       the 26 file tools                                  [workstream: tools]
  crates/orange-cli         `orange` headless CLI (dispatches to the two crates' cli::run)
  src-tauri/                Tauri app: commands, settings, windows, Windows glue [workstream: shell]
  ui/                       Vite + TypeScript frontend (vanilla TS, no framework) [workstream: ui]
  site/                     landing page (static HTML)                          [workstream: site]
  scripts/fetch-deps.ps1    downloads portable ffmpeg/ImageMagick/pandoc/exiftool/7zip/poppler/qpdf into src-tauri/resources/bin
  .github/workflows/        windows-latest CI build
```

Workstreams only edit their own paths. If you need a change in `orange-core`, make it additive and tell the lead.
Each workstream builds with its own `CARGO_TARGET_DIR` (e.g. `/home/claude/target-<name>`) to avoid lock contention.

## Product behaviour

1. User starts dragging a file (or several) in Explorer / desktop.
2. While still dragging, user **taps a modifier key**:
   - **Shift** → *Convert* wheel (target formats for the dragged file's kind)
   - **Ctrl** → *Tools* wheel (file tools applicable to the selection)
   - **Alt** → *Actions* wheel (Share, Float, Copy, Move, Copy Path, Terminal)
   (keys configurable in Settings; the wheel stays open after the key is released until drop/cancel)
3. A transparent always-on-top, non-activating **wheel overlay window** appears centred on the cursor.
   It is a real OLE drop target (Tauri drag-drop events give `paths` on enter and drop, and cursor position on over).
4. Hovering a wedge highlights it; dropping on a wedge commits: convert / open tool dialog (or run immediately) / run action.
5. Esc or releasing the mouse outside the wheel cancels. Press Enter on an Explorer selection is NOT supported (no hook into Explorer); instead the tray menu offers "Convert…" file picker.

Honest scope notes (must appear in README): Windows-only layer (global hooks / overlay / WinRT OCR+faces / share / clipboard) is
written without the ability to compile on Windows in the authoring environment; CI builds it. `Pages` and `RTFD` (macOS formats) are not supported.
Option counts are computed from the registry — never hard-code "275". Never copy Tangerine's name, copy, icons or visual identity.

## Dependencies (external tools)

`orange_core::Dep`: Ffmpeg, Ffprobe, Magick (ImageMagick 7 `magick`; on Linux IM6 `convert` also accepted), Pandoc, Soffice (LibreOffice, optional heavy dep),
Pdftoppm (poppler), SevenZip, Exiftool, Qpdf. Found in `<exe>/bin`, `<exe>/resources/bin`, `$ORANGE_BIN_DIR`, PATH, well-known Windows install dirs.
A missing dep disables only the entries that need it (`available:false` + `note`).
Pixel work (crop, edit, annotate, background, collage, redact, resize, compress) is done by **ImageMagick** so there is one implementation
for both final output and previews. PDF creation: avoid ImageMagick's PDF coder (distro policies disable it) — use a pure-Rust writer or other approach.

## Core crates API (see the stub files for exact signatures)

`orange-convert`: `options_for(path,&caps)->Vec<FormatOption>`, `option_count()`, `convert(input,to,&ctx)->Vec<PathBuf>`.
`orange-tools`: `all_tools()` (exactly 26), `tools_for(paths,&caps)`, `run_tool(id,inputs,&params,&ctx)->Vec<PathBuf>`,
`preview(id,input,&params,&ctx,max_dim)->PathBuf`, `web_preview(input,&ctx,max_dim)->PathBuf`, `file_info(path,&caps)->Value`, `read_metadata(path,&caps)->Value`.
Both crates expose `cli::run(Vec<String>)->i32`, wired as `orange convert …` / `orange tool …`.

## The 26 tools (ids)

| # | id | kinds | files | ui | notes |
|---|----|-------|-------|----|-------|
| 1 | compress-image | image | 1+ | form | |
| 2 | edit-metadata | image | 1+ | metadata | exiftool |
| 3 | edit-photo | image | 1 | edit_photo | live preview |
| 4 | annotate-photo | image | 1 | annotate | |
| 5 | add-background | image | 1+ | add_background | |
| 6 | crop-image | image | 1+ | crop | |
| 7 | redact-photo | image | 1 | redact | regions come from UI (drawn or auto-detected via Windows OCR/face API) |
| 8 | resize-image | image | 1+ | form | |
| 9 | create-pdf | image | 2+ | reorder | one page per image |
| 10 | collage | image | 2+ | collage | |
| 11 | compress-video | video | 1+ | form | |
| 12 | trim-video | video | 1 | trim | |
| 13 | merge-video | video | 2+ | reorder | |
| 14 | remove-audio | video | 1+ | none | |
| 15 | resize-video | video | 1+ | form | |
| 16 | compress-audio | audio | 1+ | form | |
| 17 | trim-audio | audio | 1 | trim | |
| 18 | merge-audio | audio | 2+ | reorder | |
| 19 | normalize-audio | audio | 1+ | form | |
| 20 | compress-pdf | pdf | 1+ | form | |
| 21 | split-pdf | pdf | 1 | pages | multi-output |
| 22 | merge-pdf | pdf | 2+ | reorder | |
| 23 | rotate-pdf | pdf | 1+ | pages | |
| 24 | protect-pdf | pdf | 1+ | form | lock / unlock with password (qpdf) |
| 25 | shift-subtitles | subtitle | 1+ | form | |
| 26 | create-archive | any (incl. folders) | 1+ | form | zip / tar / gz / 7z |

Icon names the frontend ships (tools use these in `ToolInfo.icon`): `compress, tag, sliders, pencil, image-plus, crop, eye-off, resize, file-plus, grid, scissors, merge,
volume-x, gauge, lock, rotate, split, clock, archive, film, music, file`.
Wedge `label` is a short ALL-CAPS word (e.g. `COMPRESS`, `METADATA`, `EDIT`, `ANNOTATE`, `ADD BG`, `CROP`, `REDACT`, `RESIZE`, `TO PDF`, `COLLAGE`, `TRIM`, `MERGE`, `MUTE`, `SPLIT`, `ROTATE`, `LOCK`, `SHIFT`, `ZIP`…).

## Per-tool params (JSON). All pixel coordinates are in **source image pixels** (top-left origin).

- compress-image: `{"mode":"target_size","target_kb":500}` | `{"mode":"reduce","percent":50}` | `{"mode":"preset","preset":"small|balanced|high"}`; optional `"max_dimension":1920`. Output keeps format (PNG→ lossless optimize / quantize; JPG/WebP→ quality search for target).
- edit-metadata: `{"set":{"Make":"..","Model":"..","DateTimeOriginal":"YYYY:MM:DD HH:MM:SS","GPSLatitude":12.34,"GPSLongitude":56.78,"Artist":"..","Copyright":"..","ImageDescription":".."},"strip":["gps"|"camera"|"all"]}` — writes in place **only when** `"in_place":true`, else writes a copy " Edited".
  `read_metadata` returns `{"make":..,"model":..,"lens":..,"taken":..,"gps":{"lat":..,"lon":..}|null,"artist":..,"copyright":..,"description":..,"software":..,"raw":{...}}`.
- edit-photo: `{"exposure":0,"brightness":0,"contrast":0,"saturation":0,"warmth":0,"sharpness":0,"blur":0,"vignette":0,"filter":"none|mono|sepia|vivid|fade|noir","rotate":0,"flip_h":false,"flip_v":false}` (sliders −100..100, sharpness/blur/vignette 0..100, rotate ∈ {0,90,180,270}).
- annotate-photo: `{"items":[{"type":"arrow","x1":0,"y1":0,"x2":0,"y2":0,"color":"#ff3b30","width":8},{"type":"rect","x":0,"y":0,"w":0,"h":0,"color":"#ff3b30","width":6,"fill":null},{"type":"ellipse",...same as rect},{"type":"text","x":0,"y":0,"text":"..","size":48,"color":"#fff","bg":"#000000aa"|null,"bold":true},{"type":"callout","x":0,"y":0,"n":1,"size":56,"color":"#ff3b30"}]}`
- add-background: `{"background":{"type":"solid","color":"#ffffff"}|{"type":"gradient","from":"#..","to":"#..","angle":135}|{"type":"image","path":"C:\\..."},"ratio":"auto|1:1|16:9|4:5|9:16|3:4|4:3","padding":120,"corner_radius":40,"shadow":{"blur":30,"opacity":0.35,"offset_y":16}}` (padding/corner in px of the *source* image scale).
- crop-image: `{"x":0,"y":0,"width":100,"height":100}` — applied to every input (clamped); batch with different sizes: `{"mode":"ratio","ratio":"16:9"}` centre-crops.
- redact-photo: `{"regions":[{"x":0,"y":0,"w":100,"h":40}],"style":"solid|blur","color":"#000000","blur_radius":30}`
- resize-image: `{"mode":"width|height|percent|fit","width":1920,"height":1080,"percent":50}` (aspect always kept; `fit` = within box).
- create-pdf: `{"page_size":"fit|a4|letter","margin":0,"orientation":"auto|portrait|landscape"}` — inputs order = page order.
- collage: `{"layout":"grid|row|column|featured","columns":2,"spacing":24,"corner_radius":24,"cell_size":800,"background":{...as add-background...}}` — inputs order = cell order (first = featured).
- compress-video: `{"preset":"small|balanced|high"}` | `{"target_mb":25}`; optional `"max_height":1080`.
- trim-video / trim-audio: `{"start":0.0,"end":12.5,"precise":false}` seconds. (precise=false → stream copy when possible.)
- remove-audio: `{}`. resize-video: `{"height":720}`.
- merge-*: `{}` — inputs order = order; re-encode if codecs differ.
- compress-audio: `{"bitrate_kbps":128}`. normalize-audio: `{"target_lufs":-16}`.
- compress-pdf: `{"level":"low|medium|high"}`. split-pdf: `{"mode":"each|every_n|ranges","n":2,"ranges":"1-3,5,7-9"}`. rotate-pdf: `{"degrees":90,"pages":"all|1,3-5"}`. protect-pdf: `{"action":"lock|unlock","password":".."}`. merge-pdf: `{}`.
- shift-subtitles: `{"offset_ms":1500}` (negative allowed).
- create-archive: `{"format":"zip|tar|gz|7z","name":"Archive"}`.

`file_info` JSON: see `orange-tools/src/lib.rs` doc comment.

## Tauri commands (UI → backend; `invoke(name, args)`); implemented in `src-tauri/src/commands.rs`

All return JSON; errors are rejected with a human-readable string.

| command | args | returns |
|---|---|---|
| `app_info` | – | `{version, conversion_option_count, tool_count, action_count:6}` |
| `capabilities` | – | `Capabilities.to_json()` map dep → `{name,path,found}` |
| `wheel_items` | `{mode:"convert"\|"tools"\|"actions", paths:string[]}` | `WheelItem[]` |
| `wheel_commit` | `{mode, id, paths}` | `{kind:"job",job_id}` \| `{kind:"dialog"}` \| `{kind:"done"}`; hides the overlay |
| `wheel_cancel` | – | hides overlay |
| `run_convert` | `{paths, to}` | `job_id:string` |
| `run_tool` | `{tool_id, paths, params}` | `job_id:string` |
| `cancel_job` | `{job_id}` | – |
| `tool_info` | `{tool_id}` | `ToolInfo` (JSON of the Rust struct) |
| `preview_tool` | `{tool_id, path, params, max_dim?}` | absolute path of temp PNG (use `convertFileSrc`) |
| `web_preview` | `{path, max_dim?}` | absolute path of browser-displayable image |
| `file_info` | `{path}` | JSON from orange-tools |
| `read_metadata` | `{path}` | JSON |
| `detect_regions` | `{path, what:"text"\|"faces"}` | `{x,y,w,h}[]` in source pixels. Windows: WinRT OcrEngine / FaceDetector. Elsewhere: `[]` |
| `open_tool_window` | `{tool_id, paths}` | – |
| `run_action` | `{action:"share"\|"float"\|"copy"\|"move"\|"copy_path"\|"terminal", paths}` | `{ok:true,message?}`; `move` shows a folder picker |
| `shelf_list` / `shelf_add` / `shelf_remove` / `shelf_clear` | `{paths}` / `{path}` | `string[]` (Float shelf contents) |
| `reveal_in_folder` | `{path}` | – |
| `get_settings` / `set_settings` | `Settings` | `Settings` |
| `pick_files` | – | `string[]` (native picker; tray "Convert…" uses it then opens the Convert wheel centred) |

`WheelItem = {id:string, label:string, icon?:string, enabled:boolean, hint?:string}`.
Actions wheel ids/labels/icons: `share` SHARE(share), `float` FLOAT(layers), `copy` COPY(copy), `move` MOVE(folder-move), `copy_path` COPY PATH(link), `terminal` TERMINAL(terminal).
(Also ship these icons in the UI icon set.) Convert wheel items: `id` = target ext, `label` = uppercase ext, `icon` omitted.

`Settings = {keys:{convert:"Shift",tools:"Control",actions:"Alt"}, output:{mode:"beside"|"folder",folder:string|null}, launch_at_login:boolean, notifications:boolean, wheel_scale:number(0.8..1.4), theme:"system"|"light"|"dark", first_run_done:boolean}`.
Stored at `%APPDATA%/Orange/settings.json`.

## Events (backend → UI; `listen(name)`)

- `job:progress` `{job_id, fraction, message}`
- `job:done` `{job_id, outputs:string[], error:string|null, cancelled:boolean}`
- `wheel:show` `{mode, x, y, scale}` — overlay window made visible; UI should reset hover state, play open animation.
- `wheel:hide` – overlay hidden.
- `settings:changed` `Settings`
- `shelf:changed` `string[]`

The overlay webview itself uses Tauri's native drag-drop events (`getCurrentWebview().onDragDropEvent`):
`enter{paths,position}` → ask `wheel_items`, render wedges; `over{position}` → hit-test wedge & highlight; `drop{paths,position}` → `wheel_commit`; `leave` → keep wheel open (user may re-enter).
Positions are physical px relative to the webview — divide by `devicePixelRatio`.

## Windows layer design (src-tauri/src/win/*, `#[cfg(windows)]`)

- `hooks.rs`: one background thread with `SetWindowsHookExW(WH_MOUSE_LL)` + `WH_KEYBOARD_LL` and a message loop. Track left-button state from the mouse hook.
  A *drag session* = left button down + moved > 6px from press point + press landed on a window of class `CabinetWClass`, `Progman`, `WorkerW`, `#32770`(file dialogs) or the `SysListView32`/`DirectUIHWND` children (accept any app: simply require button held and cursor moved; do not filter by window class unless needed to reduce false positives).
  While a drag session is active, a **tap** (down+up of a configured modifier with no other key between) fires `wheel:show` for that mode.
  Never block in the hook callback: send events over a channel to a worker. Hooks must return quickly (<200ms) or Windows removes them.
  Mouse-up ends the session → after 150 ms hide overlay if no drop was handled.
  Esc during session hides overlay.
- `overlay.rs`: create the `wheel` webview window at startup, hidden: `transparent`, no decorations, `always_on_top`, `skip_taskbar`, `focus:false`, `shadow:false`, 640×640 logical.
  Apply `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST` via `SetWindowLongPtrW` so it never steals focus (that would cancel the drag in the source app).
  Show with `ShowWindow(SW_SHOWNOACTIVATE)`; position centred on the cursor and clamped to the monitor work area (per-monitor DPI aware).
- `actions.rs`: share → `IDataTransferManagerInterop::ShowShareUIForWindow` with storage items (WinRT); fallback: open `shell:` "Nearby Sharing" settings; clipboard copy → `CF_HDROP` (DROPFILES) + `Preferred DropEffect`; move → `IFileOpenDialog` folder picker then `std::fs::rename`/copy+delete; copy_path → CF_UNICODETEXT with one path per line; terminal → `wt.exe -d <dir>` fallback `powershell`/`cmd /K cd /d`; float → shelf window (always-on-top mini window, pin to screen edge, drag files out using `tauri-plugin-drag`).
- `ocr.rs`: `Windows.Media.Ocr.OcrEngine::TryCreateFromUserProfileLanguages` → `RecognizeAsync(SoftwareBitmap)`; word rects → merged line rects. `Windows.Media.FaceAnalysis.FaceDetector::CreateAsync` + `DetectFacesAsync` (needs `Gray8`/`Nv12` bitmap). Return source-pixel rects.
- Non-Windows builds compile the same crate with stubs (`#[cfg(not(windows))]`): hooks no-op; `detect_regions`→`[]`; actions return an error "Windows only" except copy_path/terminal best effort. A dev-only command-line flag `--demo-wheel` (and tray item "Show wheel") must open the overlay at screen centre with a sample file so the UI can be exercised without hooks.
- System tray with: *Convert…*, *Settings*, *Show float shelf*, *Quit*. App stays resident, starts hidden (settings window opens on first run).
- Tauri plugins: `tauri-plugin-dialog`, `tauri-plugin-drag`, `tauri-plugin-autostart`, `tauri-plugin-single-instance`, `tauri-plugin-notification`, `tauri-plugin-shell` not needed.

## UI windows (ui/, Vite multi-page, labels in Rust)

`index.html` (label `main`: onboarding + settings + job history + dependency status) · `wheel.html` (label `wheel`: overlay) ·
`tool.html?tool=<id>&paths=<json>` (label `tool-<n>`: tool dialogs) · `shelf.html` (label `shelf`: float shelf).
The UI must run in a plain browser (`npm run dev`) with a **mock backend** (`ui/src/mock.ts`, selected when `window.__TAURI_INTERNALS__` is undefined)
so it can be exercised and screenshotted without Tauri. The mock should serve realistic WheelItems / ToolInfo (hard-code the 26-tool table above).

## Visual identity (original — do not imitate Tangerine)

Name: **Orange**. Palette: deep ink `#16110d`, warm paper `#fbf6ef`, signature orange `#ff6a13`, apricot `#ffb37a`, plus a teal counter-accent `#0e8c7f`.
Type: system UI stack for the app (Segoe UI Variable on Windows); landing page may use Google Fonts (e.g. "Bricolage Grotesque" + "Inter").
Wheel: translucent acrylic-like disc, 7–8 wedges with 4px gaps, centre label shows the highlighted item, orange highlight, springy open animation.
Mark: a simple original citrus-slice-free glyph (a circle with a segment wedge pulled out — "radial wheel"), not a tangerine illustration.
