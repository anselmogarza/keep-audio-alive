# Removes Keep Audio Alive: stops it, deletes the install dir, the Start Menu
# shortcut, and the "run on login" registry entry (if it was enabled).
#   powershell -ExecutionPolicy Bypass -File uninstall.ps1
#
# Note: settings (%APPDATA%\KeepAudioAlive\config.json) are left in place.

$ErrorActionPreference = 'SilentlyContinue'

$exeName    = 'keep-audio-alive.exe'
$installDir = Join-Path $env:LOCALAPPDATA 'KeepAudioAlive'

Get-Process -Name 'keep-audio-alive' | Stop-Process -Force

Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'KeepAudioAlive'

$startMenuPrograms = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs'
Remove-Item -Path (Join-Path $startMenuPrograms 'Keep Audio Alive.lnk') -Force

Remove-Item -Path $installDir -Recurse -Force

Write-Host "Uninstalled. Settings at `$env:APPDATA\KeepAudioAlive\config.json were left in place."
