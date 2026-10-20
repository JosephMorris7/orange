# Orange

Drag a file, tap a key, drop it on a wheel. Orange is an offline file converter and toolbox for Windows 10/11 that lives in the tray.

1. Start dragging any file or folder.
2. Tap **Shift** (formats), **Ctrl** (tools) or **Alt** (actions). The keys are configurable.
3. A wheel opens under the cursor. Drop on a wedge and it runs. **Esc** cancels.

Orange is an independent project. It borrows the idea of a drag-and-tap radial menu, which exists in other apps on other platforms, and is not affiliated with or endorsed by their makers.

## What is in the box

| Part | Where | Notes |
|---|---|---|
| Conversion engine | `crates/orange-convert` | Source-to-target pairs are computed from a registry (`orange counts`), never hard-coded. |
| 26 file tools | `crates/orange-tools` | Compress, crop, edit, annotate, redact, background, collage, metadata, trim, merge, split, rotate, lock, archive and more. |
| Shared base | `crates/orange-core` | Dependency discovery, job context, output naming (never overwrites). |
| Command line | `crates/orange-cli` | `orange convert`, `orange tool`, `orange deps`, `orange counts`. Handy for testing without the UI. |
| Windows app | `src-tauri` | Tauri 2. Drag session logic, wheel overlay, tray, settings, shelf. Windows-only code is in `src-tauri/src/win`. |
| UI | `ui` | Vite + TypeScript, no framework. Runs in a plain browser with a mock backend (`npm run dev`). |
| Landing page | `site` | Edit `index.fragment.html`, then `python3 site/wrap.py`. |

Formats and helper programs: see `docs/CONTRACT.md`. Pixel work uses ImageMagick, media uses ffmpeg, documents use pandoc (and LibreOffice if installed), PDFs use qpdf and poppler. A missing helper only greys out the options that need it.

Not supported: Pages and RTFD (macOS-only formats).

## Honest status

* The conversion engine, the tools, the drag-session state machine, settings, job runner and file-action helpers have automated tests, run on Linux (`cargo test --workspace`).
* Tested by hand on one Windows 11 x64 PC: building the installer, installing, tray, settings, the low-level hooks, the wheel opening on a modifier tap during a file drag, dropping on a wedge to convert, and the Tools, Actions and shelf flows. Other Windows versions, ARM64, multi-monitor layouts and unusual DPI setups are untested.
* Windows-specific behaviour is the fragile part. Two things were learned the hard way and are documented in the code: the wheel needs its own invisible native drop-target window over it (`src-tauri/src/win/droptarget.rs`), because the web view's internal windows live in another process and Windows never offers them a drag; and the overlay must never take focus or the drag is cancelled.
* The share sheet, OCR and face detection (WinRT), and signed installers have had little or no testing.
* Installers are unsigned (SmartScreen will warn).

If something misbehaves, the log is `%APPDATA%\Orange\orange.log` (it records hook events, wheel placement and drag events).

## Build

Requirements: Rust stable, Node 22, Visual Studio C++ build tools, WebView2 runtime (included with Windows 11).

```powershell
.\scripts\fetch-deps.ps1            # ffmpeg, pandoc, qpdf from GitHub; ImageMagick, poppler, 7-Zip, exiftool via winget
npm --prefix ui install
cargo install tauri-cli --version "^2" --locked
cd src-tauri
cargo tauri build                   # installers in target\release\bundle
```

Development: `cargo tauri dev` from `src-tauri`. To open the wheel without any drag, run the app with `--demo-wheel [convert|tools|actions] [file]` or use the tray menu "Convert…".

Linux/macOS (engine and UI development only): `scripts/fetch-deps.sh`, then `cargo test --workspace`.

## Licences of what you bundle

Orange itself is MIT. The helper programs keep their own licences (ffmpeg LGPL/GPL depending on the build, ImageMagick, pandoc GPL, qpdf Apache-2.0, poppler GPL, 7-Zip LGPL, exiftool Perl/GPL). Check them before you redistribute an installer.

## Privacy

Orange makes no network connections of its own. Files are processed locally. Settings are stored in `%APPDATA%\Orange\settings.json`.
