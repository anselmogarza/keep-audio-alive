# Installs Keep Audio Alive for the current user (no admin rights needed).
# Run from the folder containing keep-audio-alive.exe:
#   powershell -ExecutionPolicy Bypass -File install.ps1

$ErrorActionPreference = 'Stop'

$exeName   = 'keep-audio-alive.exe'
$sourceExe = Join-Path $PSScriptRoot $exeName
$installDir = Join-Path $env:LOCALAPPDATA 'KeepAudioAlive'
$targetExe  = Join-Path $installDir $exeName

if (-not (Test-Path $sourceExe)) {
    Write-Error "Can't find $exeName next to this script. Place install.ps1 alongside the built .exe."
}

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Copy-Item -Path $sourceExe -Destination $targetExe -Force

$startMenuPrograms = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs'
$shortcutPath = Join-Path $startMenuPrograms 'Keep Audio Alive.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $targetExe
$shortcut.WorkingDirectory = $installDir
$shortcut.Description = 'Keep Audio Alive'
$shortcut.Save()

# Register autostart on login (per-user Run key, no admin needed). Points at the
# installed exe and matches the value the app's own "Start with Windows" toggle
# writes, so the tray menu shows it as already enabled. uninstall.ps1 removes it.
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
Set-ItemProperty -Path $runKey -Name 'KeepAudioAlive' -Value "`"$targetExe`""

Write-Host "Installed to $targetExe"
Write-Host "Start Menu shortcut: $shortcutPath"
Write-Host "Autostart on login: enabled (toggle anytime via the tray's 'Start with Windows')."

Start-Process -FilePath $targetExe
