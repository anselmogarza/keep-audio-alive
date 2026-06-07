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

Write-Host "Installed to $targetExe"
Write-Host "Start Menu shortcut: $shortcutPath"
Write-Host "Launch it, then use the tray menu's 'Start with Windows' to run it on login."

Start-Process -FilePath $targetExe
