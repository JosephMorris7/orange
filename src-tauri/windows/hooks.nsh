; Extra steps for the Orange installer (NSIS).
; After Orange is installed, offer to install LibreOffice, which Orange uses to convert Word, Excel and
; PowerPoint files. It is downloaded from its official source by winget (or the download page is opened
; when winget is not available). Skipped when it is already installed and in silent installs.

!macro NSIS_HOOK_POSTINSTALL
  IfSilent orange_lo_done
  IfFileExists "$PROGRAMFILES64\LibreOffice\program\soffice.exe" orange_lo_done
  IfFileExists "$PROGRAMFILES\LibreOffice\program\soffice.exe" orange_lo_done
  MessageBox MB_YESNO|MB_ICONQUESTION "Orange can also install LibreOffice (free, about 350 MB download).$\r$\n$\r$\nIt is only needed to convert Word, Excel and PowerPoint files; everything else works without it.$\r$\n$\r$\nInstall LibreOffice now?" IDNO orange_lo_done
  ExecWait 'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "if (Get-Command winget -ErrorAction SilentlyContinue) { winget install --id TheDocumentFoundation.LibreOffice -e --accept-package-agreements --accept-source-agreements } else { Start-Process https://www.libreoffice.org/download/download-libreoffice/; Write-Host Opened the LibreOffice download page. }; Start-Sleep -Seconds 4"'
  orange_lo_done:
!macroend
