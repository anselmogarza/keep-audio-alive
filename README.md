# Keep Audio Alive

Windows tray app that prevents audio devices (soundbars, Bluetooth speakers,
USB DACs, anything) from going idle. Periodically plays a short 50Hz tone on
the output device of your choice — not locked to one device or Bluetooth, pick
any from the tray menu.

## Features

- 50Hz tone with a selectable amplitude (20 / 40 / 60 / 80 / 100%, default 80%)
  from the tray's **Volume** menu
- Forces this app's own volume to 100% in the Windows Volume Mixer on each
  play, so the tone stays loud even if the app is turned down there — the
  master volume and other apps are left untouched
- Two modes:
  - **Interval** — short 3s tone every 30s / 1m / 2m / 5m
  - **Continuous** — loops the tone with no gaps until you turn it off
- Pick the output device from a live list, or use the system default
- "Start with Windows" toggle (per-user registry Run key, no admin needed)
- Settings persisted to `%APPDATA%\KeepAudioAlive\config.json`
- Lives entirely in the system tray, no window

## Build (cross-compiled from macOS)

Requires the Windows GNU target and a mingw-w64 linker:

```bash
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64

cargo build --release --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/keep-audio-alive.exe`

Copy that single `.exe` to a Windows machine and run it — no runtime install
needed.

## Install (optional)

There's no MSI/EXE installer (building one needs WiX/NSIS, which are
Windows-only tools — can't produce them from this macOS box). Instead,
`scripts/install.ps1` does what an installer would: copies the `.exe` to
`%LOCALAPPDATA%\KeepAudioAlive\`, adds a Start Menu shortcut, enables autostart
on login, and launches it. Run it from the same folder as the built `.exe`:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1
```

`scripts/uninstall.ps1` reverses it (stops the app, removes the shortcut,
install dir, and the "run on login" registry entry — settings are kept).
Both run per-user, no admin rights needed.

## Usage

1. Launch the `.exe`; a speaker icon appears in the system tray
2. Right-click (or left-click) for the menu:
   - **Keep Audio Alive** — toggle on/off
   - **Start with Windows** — registers/unregisters launch on login
     (`HKCU\...\CurrentVersion\Run`, no admin needed)
   - **Mode** — Interval (periodic short tone) or Continuous (loops non-stop)
   - **Interval** — how often the tone plays (Interval mode only)
   - **Volume** — tone amplitude (20–100%)
   - **Output Device** — which device to target ("System Default" or any
     enumerated device; "Refresh device list" rescans after plugging
     something in)
   - **Play tone now** — trigger immediately, e.g. to test a device
   - **Quit**

## How it works

A background thread sleeps for the configured interval, then opens a short
output stream on the selected device and plays a 50Hz sine wave at the
amplitude chosen in the Volume menu — enough to keep the device's auto-sleep
timer from firing. Right after starting playback it uses WASAPI's `ISimpleAudioVolume` to
set this process's audio session to 100% (the per-app slider in the Windows
Volume Mixer), so the tone plays as loud as the master volume allows even when
the app has been turned down in the mixer. Only this app's session is changed —
the master/endpoint volume and every other app are left as they are, so a low
master volume still scales the tone down proportionally.
