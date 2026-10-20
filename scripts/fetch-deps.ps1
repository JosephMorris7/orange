<#
 Downloads the helper programs Orange shells out to into src-tauri\resources\bin so the installer can ship them.
 Run from the repo root in PowerShell:  .\scripts\fetch-deps.ps1
 Needs `winget`-independent downloads only (GitHub releases). Review the licences of what you bundle:
 ffmpeg (GPL/LGPL build you choose), ImageMagick (Apache-style), pandoc (GPL), 7-Zip (LGPL), exiftool (Perl/GPL), qpdf (Apache-2.0), poppler (GPL).
 LibreOffice is NOT bundled (very large); Orange uses it only if installed.
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

# ImageMagick, poppler, 7-Zip and exiftool have no stable GitHub download URL: install them with winget.
# Orange finds them on PATH, so they do not need to be copied into the bin folder.
foreach ($id in 'ImageMagick.ImageMagick', 'oschwartz10612.Poppler', '7zip.7zip', 'OliverBetz.ExifTool') {
  try { winget install --id $id -e --accept-package-agreements --accept-source-agreements } catch { Write-Warning "Could not install $id : $_" }
}
Write-Host 'Close and reopen PowerShell so the newly installed programs are on PATH.'

Write-Host "Done. Contents of $bin :"; Get-ChildItem $bin | Select-Object -First 40 Name
