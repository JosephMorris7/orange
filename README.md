<p align="center">
  <img src="docs/brand/mark.svg" alt="Orange logo" width="96" height="96">
</p>

<h1 align="center">Orange</h1>

<p align="center">
  Drag a file. Tap a key. Drop it on the wheel.<br>
  An offline file converter and toolbox for Windows 10 and 11 that lives in your tray.
</p>

<p align="center">
  <a href="https://github.com/JosephMorris7/orange/releases/latest/download/Orange-setup.exe"><b>Download for Windows</b></a>
  &nbsp;·&nbsp;
  <a href="https://github.com/JosephMorris7/orange/releases/latest">All releases</a>
  &nbsp;·&nbsp;
  <a href="https://github.com/JosephMorris7/orange/issues">Report a problem</a>
</p>

---

## How it works

1. Start dragging any file or folder.
2. Tap **Shift** (convert), **Ctrl** (tools) or **Alt** (actions). The keys are configurable in Settings.
3. A wheel opens under your cursor. Drop the file on a wedge and it runs. Press **Esc** to cancel.

Results are saved next to the original and never overwrite an existing file.

## Features

- **Convert** images, video, audio, PDFs, documents, slides, spreadsheets, subtitles and archives. The list of source and target formats is computed from a registry (run `orange counts` to see it), never hard-coded.
- **26 tools**: compress, crop, edit, annotate, redact, add background, resize, collage, edit metadata, trim, merge, split, rotate, lock and unlock PDFs, create archives and more.
- **Actions**: share, float to a shelf, copy, move, copy the path, open a terminal in the folder.
- **Works offline.** No account and no uploads. Files are processed on your PC.
- **Real redaction.** The Redact tool destroys the covered pixels in a new copy and strips its metadata.
- **Everything included.** The installer ships ffmpeg, pandoc, qpdf, ImageMagick, poppler, 7-Zip and ExifTool. LibreOffice (for Word, Excel and PowerPoint files, about 350 MB) is optional: the installer offers it at the end of setup, and Settings has an **Install LibreOffice** button for later.

Not supported: Pages and RTFD, which are macOS-only formats.

## Install

1. Download **Orange-setup.exe** from the link above.
2. Run it. Windows may say it protected your PC because the installer is not signed yet. Choose **More info**, then **Run anyway**.
3. Drag a file and tap Shift. The tray icon opens Settings.

Share the `.exe` installer, not the `.msi`. Only the `.exe` has the LibreOffice setup step.

## Project layout

| Part | Where | Notes |
|---|---|---|
| Conversion engine | `crates/orange-convert` | Source-to-target pairs come from a registry. |
| File tools | `crates/orange-tools` | The 26 tools. |
| Shared base | `crates/orange-core` | Helper-program discovery, job context, output naming. |
| Command line | `crates/orange-cli` | `orange convert`, `orange tool`, `orange deps`, `orange counts`. Useful for testing without the UI. |
| Windows app | `src-tauri` | Tauri 2. Drag session logic, wheel overlay, tray, settings, shelf. Windows-only code is in `src-tauri/src/win`. |
| UI | `ui` | Vite and TypeScript, no framework. Runs in a plain browser with a mock backend (`npm run dev`). |
| Website | `site` | Edit `index.fragment.html`, then run `python3 site/wrap.py`. |

Formats and helper programs are described in `docs/CONTRACT.md`.

## Build from source

You need Rust (stable), Node 22, the Visual Studio C++ build tools and the WebView2 runtime (included with Windows 11).

```powershell
.\scripts\fetch-deps.ps1            # downloads and gathers the helper programs
npm --prefix ui install
cargo install tauri-cli --version "^2" --locked
cd src-tauri
cargo tauri build                   # installers land in target\release\bundle
```

Development: run `cargo tauri dev` from `src-tauri`. To open the wheel without a drag, start the app with `--demo-wheel [convert|tools|actions] [file]` or use the tray menu.

Build releases on a Windows PC. The GitHub workflow runs `fetch-deps.ps1` on a runner that may not have winget, in which case the installer lacks some helpers.

Linux and macOS (engine and UI development only): run `scripts/fetch-deps.sh`, then `cargo test --workspace`.

## Status

- The engine, tools, drag-session logic, settings, job runner and file actions have automated tests (`cargo test --workspace`).
- Tried by hand on Windows 11 x64: installing, the tray, settings, the wheel opening on a key tap during a drag, dropping on a wedge, and the Tools, Actions and shelf flows.
- Least tested: the share sheet, OCR and face detection (WinRT), other Windows versions, ARM64, multi-monitor layouts and unusual DPI settings.
- Installers are unsigned, so SmartScreen will warn.

Two things learned the hard way, documented in the code: the wheel needs its own invisible native drop-target window over it (`src-tauri/src/win/droptarget.rs`) because the web view's windows belong to another process and never receive a drag; and the overlay must never take focus or the drag is cancelled.

If something misbehaves, the log is at `%APPDATA%\Orange\orange.log`.

## Privacy

Orange makes no network connections of its own. The optional **Install LibreOffice** button starts winget, which downloads LibreOffice. Files are processed locally. Settings are stored in `%APPDATA%\Orange\settings.json`.

## Logo and icons

The master logo is `docs/brand/mark.svg`. Run `python3 scripts/make-icons.py` to regenerate every icon in `src-tauri/icons/` from it. If you change the logo, copy the same file to `ui/public/mark.svg` and `site/mark.svg`.

## Licence and credits

Copyright (c) 2026 **Joseph Morris**. Orange is released under the [MIT Licence](LICENSE): you may use, copy, modify and distribute it, as long as the copyright notice and licence text stay with it.

The helper programs Orange runs keep their own licences: ffmpeg (LGPL or GPL, depending on the build), ImageMagick, pandoc (GPL), qpdf (Apache-2.0), poppler (GPL), 7-Zip (LGPL), ExifTool (Perl or GPL) and LibreOffice (MPL). Check them before you redistribute an installer.

Orange is an independent project. It borrows the idea of a drag-and-tap radial menu, which exists in other apps on other platforms, and is not affiliated with or endorsed by their makers. Product names and logos belong to their owners.

## Author

**Joseph Morris**

- GitHub: [JosephMorris7](https://github.com/JosephMorris7)
- Email: joseph931508@gmail.com
- LinkedIn: [joseph-morris-028271300](https://www.linkedin.com/in/joseph-morris-028271300)
