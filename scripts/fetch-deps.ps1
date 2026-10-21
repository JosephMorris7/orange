<#
 Downloads the helper programs Orange shells out to into src-tauri\resources\bin so the installer can ship them.
 Run from the repo root in PowerShell:  .\scripts\fetch-deps.ps1
 ffmpeg, pandoc and qpdf are downloaded from GitHub; the rest come from winget and are copied. Review the licences of what you bundle:
 ffmpeg (GPL/LGPL build you choose), ImageMagick (Apache-style), pandoc (GPL), 7-Zip (LGPL), exiftool (Perl/GPL), qpdf (Apache-2.0), poppler (GPL).
 LibreOffice is NOT bundled (very large); Orange offers an Install LibreOffice button in Settings and uses it only if installed.
#>
$ErrorActionPreference = 'Stop'
$bin = Join-Path $PSScriptRoot '..\src-tauri\resources\bin'
New-Item -ItemType Directory -Force $bin | Out-Null
$tmp = Join-Path $env:TEMP 'orange-deps'; New-Item -ItemType Directory -Force $tmp | Out-Null

function Get-Latest($repo, $pattern) {
  $rel = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers @{ 'User-Agent' = 'orange-build' }
  $asset = $rel.assets | Where-Object { $_.name -match $pattern } | Select-Object -First 1
  if (-not $asset) { throw "No asset matching $pattern in $repo" }
  $out = Join-Path $tmp $asset.name
  Invoke-WebRequest $asset.browser_download_url -OutFile $out
  return $out
}
function Unzip($zip, $dest) { if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }; Expand-Archive $zip -DestinationPath $dest }

# ffmpeg + ffprobe (BtbN "essentials-style" LGPL shared-free build)
$z = Get-Latest 'BtbN/FFmpeg-Builds' 'ffmpeg-master-latest-win64-lgpl\.zip$'
Unzip $z "$tmp\ffmpeg"; Get-ChildItem "$tmp\ffmpeg" -Recurse -Include ffmpeg.exe, ffprobe.exe | Copy-Item -Destination $bin -Force

# pandoc
$z = Get-Latest 'jgm/pandoc' 'windows-x86_64\.zip$'
Unzip $z "$tmp\pandoc"; Get-ChildItem "$tmp\pandoc" -Recurse -Filter pandoc.exe | Copy-Item -Destination $bin -Force

# qpdf
$z = Get-Latest 'qpdf/qpdf' 'msvc64\.zip$'
Unzip $z "$tmp\qpdf"; Get-ChildItem "$tmp\qpdf" -Recurse -Include qpdf.exe, qpdf*.dll | Copy-Item -Destination $bin -Force

# ImageMagick, poppler, 7-Zip and exiftool: installed with winget if they are not on this PC yet, then COPIED into
# resources\bin so the installer ships them (nobody who downloads Orange has to install anything).
function Ensure-Installed($id) {
  if (-not (Get-Command winget -ErrorAction SilentlyContinue)) { Write-Warning "winget is not available; install $id by hand and run this script again."; return }
  winget install --id $id -e --silent --accept-package-agreements --accept-source-agreements | Out-Host   # exit code is non-zero when already installed: ignore it
}
function Copy-Tree($from, $to, $exclude = @()) {
  if (Test-Path $to) { Remove-Item -Recurse -Force $to }
  New-Item -ItemType Directory -Force $to | Out-Null
  Copy-Item -Path (Join-Path $from '*') -Destination $to -Recurse -Force -Exclude $exclude
}
function Find-Under($root, $filter) {
  if (-not (Test-Path $root)) { return $null }
  Get-ChildItem $root -Recurse -Filter $filter -ErrorAction SilentlyContinue | Select-Object -First 1
}
$winget = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'

# ImageMagick (the folder holds magick.exe plus its DLLs and coder modules; it must stay together)
Ensure-Installed 'ImageMagick.ImageMagick'
$im = Get-ChildItem "$env:ProgramFiles\ImageMagick*" -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName 'magick.exe') } | Sort-Object Name -Descending | Select-Object -First 1
if ($im) { Copy-Tree $im.FullName "$bin\imagemagick" @('unins*', '*.html', 'ImageMagick.rdf') } else { Write-Warning 'ImageMagick was not found after install.' }

# 7-Zip
Ensure-Installed '7zip.7zip'
$sz = "$env:ProgramFiles\7-Zip"
if (Test-Path "$sz\7z.exe") { New-Item -ItemType Directory -Force "$bin\7zip" | Out-Null; Copy-Item "$sz\7z.exe", "$sz\7z.dll" "$bin\7zip" -Force } else { Write-Warning '7-Zip was not found after install.' }

# poppler (pdftoppm and the DLLs next to it)
Ensure-Installed 'oschwartz10612.Poppler'
$pp = Find-Under $winget 'pdftoppm.exe'
if ($pp) { New-Item -ItemType Directory -Force "$bin\poppler" | Out-Null; Get-ChildItem $pp.DirectoryName -File | Where-Object { $_.Extension -in '.exe', '.dll' } | Copy-Item -Destination "$bin\poppler" -Force } else { Write-Warning 'poppler was not found after install.' }

# ExifTool (exiftool.exe plus its exiftool_files folder)
Ensure-Installed 'OliverBetz.ExifTool'
$ex = Find-Under $winget 'exiftool*.exe'
if ($ex) {
  Copy-Tree $ex.DirectoryName "$bin\exiftool"
  $k = Join-Path "$bin\exiftool" 'exiftool(-k).exe'
  if ((Test-Path $k) -and -not (Test-Path "$bin\exiftool\exiftool.exe")) { Rename-Item $k 'exiftool.exe' }
} else { Write-Warning 'ExifTool was not found after install.' }

# Check that everything the installer is supposed to carry is really there
$missing = @()
foreach ($f in 'ffmpeg.exe', 'ffprobe.exe', 'pandoc.exe', 'qpdf.exe', 'imagemagick\magick.exe', '7zip\7z.exe', '7zip\7z.dll', 'poppler\pdftoppm.exe', 'exiftool\exiftool.exe') {
  if (-not (Test-Path (Join-Path $bin $f))) { $missing += $f }
}
if ($missing.Count) { Write-Warning ("These are missing from resources\bin, so the installer will not contain them: " + ($missing -join ', ')) } else { Write-Host 'All bundled helper programs are in place.' -ForegroundColor Green }
$size = [math]::Round((Get-ChildItem $bin -Recurse -File | Measure-Object Length -Sum).Sum / 1MB)
Write-Host "resources\bin is about $size MB (the installer compresses it)."

Write-Host "Done. Contents of $bin :"; Get-ChildItem $bin | Select-Object -First 40 Name
