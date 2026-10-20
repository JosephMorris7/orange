#!/usr/bin/env bash
# Development helper for Linux/macOS: installs the helper programs from your package manager so
# `cargo test` can exercise the real conversions. (On Windows use fetch-deps.ps1.)
set -euo pipefail
if command -v apt-get >/dev/null; then
  sudo apt-get update
  sudo apt-get install -y ffmpeg imagemagick pandoc poppler-utils p7zip-full libimage-exiftool-perl qpdf
elif command -v brew >/dev/null; then
  brew install ffmpeg imagemagick pandoc poppler p7zip exiftool qpdf
else
  echo "Install ffmpeg, ImageMagick, pandoc, poppler-utils, 7-Zip, exiftool and qpdf with your package manager."; exit 1
fi
echo "Optional: install LibreOffice to enable Word/Excel/PowerPoint conversions."
