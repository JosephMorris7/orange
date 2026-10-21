<# Downloads ExifTool for Windows into src-tauri\resources\bin\exiftool. Run:  .\scripts\fetch-exiftool.ps1 #>
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$ua  = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36'
$bin = Join-Path $PSScriptRoot '..\src-tauri\resources\bin'
$dest = Join-Path $bin 'exiftool'
$tmp = Join-Path $env:TEMP 'orange-exiftool'
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
New-Item -ItemType Directory -Force $tmp, $bin | Out-Null

# The Windows zip lags behind the source release, so read the real link from the download page first.
$cands = @()
try {
  $page = (Invoke-WebRequest 'https://exiftool.org/' -UserAgent $ua -UseBasicParsing).Content
  foreach ($m in [regex]::Matches($page, 'exiftool-(\d+\.\d+)_64\.zip')) { if ($cands -notcontains $m.Groups[1].Value) { $cands += $m.Groups[1].Value } }
} catch { Write-Warning "home page: $_" }
try { $v = ((Invoke-WebRequest 'https://exiftool.org/ver.txt' -UserAgent $ua -UseBasicParsing).Content).ToString().Trim(); if ($cands -notcontains $v) { $cands += $v } } catch { Write-Warning "ver.txt: $_" }
if (-not $cands.Count) { throw 'Could not find the current ExifTool version (is exiftool.org reachable from this PC?)' }
$ver = $cands[0]
Write-Host "ExifTool version $ver"

$zip = Join-Path $tmp 'exiftool.zip'
$ok = $false
foreach ($v in $cands) {
  foreach ($u in @("https://exiftool.org/exiftool-${v}_64.zip", "https://exiftool.org/exiftool-${v}_64.zip".Replace('_64','_32'))) {
    try { Invoke-WebRequest $u -OutFile $zip -UserAgent $ua -UseBasicParsing; if ((Get-Item $zip).Length -gt 1MB) { $ok = $true; Write-Host "Downloaded $u"; break } } catch { Write-Warning "$u : not available" }
  }
  if ($ok) { break }
}
if (-not $ok) { throw 'Download failed from every source. Download exiftool-XX.XX_64.zip by hand from https://exiftool.org and unzip it into src-tauri\resources\bin\exiftool' }

Expand-Archive $zip -DestinationPath "$tmp\x" -Force
$exe = Get-ChildItem "$tmp\x" -Recurse -Filter 'exiftool*.exe' | Select-Object -First 1
if (-not $exe) { throw 'exiftool.exe not found inside the zip' }
if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $exe.DirectoryName '*') $dest -Recurse -Force
$k = Join-Path $dest 'exiftool(-k).exe'
if ((Test-Path $k) -and -not (Test-Path (Join-Path $dest 'exiftool.exe'))) { Rename-Item $k 'exiftool.exe' }
if (-not (Test-Path (Join-Path $dest 'exiftool_files'))) { Write-Warning 'exiftool_files folder is missing: ExifTool will not run.' }
& (Join-Path $dest 'exiftool.exe') -ver
Write-Host "ExifTool is in place: $dest" -ForegroundColor Green
