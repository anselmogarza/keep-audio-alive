# Keep Audio Alive

Windows tray app that prevents audio devices from going idle by playing an adjustable sine wave on your selected output device.

## Features

- **Tone settings...** opens a small native Windows window:
  - Frequency slider: **20-20,000 Hz**, logarithmic spacing, with an exact whole-Hz input.
  - Volume slider: **0.01-100%**, logarithmic spacing, with an exact fractional-percentage input (for example, **0.05%**).
  - Sliders apply immediately. Use arrow keys for smaller adjustments. Click **Apply exact values** after typing numbers.
  - Separate **Mute tone** checkbox preserves the selected volume.
  - **Start preview / Stop preview** for continuous tuning on the selected output device. Preview temporarily replaces scheduled playback; stopping it or closing the window resumes the configured mode.
  - Optional **Set this app's Windows mixer volume to 100%**. Enabled by default to preserve existing behavior; disabling it leaves the current mixer level alone. It does not restore a previous mixer value.
- Smooth frequency and amplitude changes, with short fades at playback start and stop.
- **Interval** mode: a three-second tone every 30 seconds / 1 minute / 2 minutes / 5 minutes.
- **Continuous** mode: an uninterrupted tone until disabled.
- Select any available output device, or use the system default.
- **Start with Windows** toggle (per-user registry Run key, no admin needed).
- Settings persist in `%APPDATA%\KeepAudioAlive\config.json`. Existing configurations, including integer volumes, are preserved. Defaults remain 50 Hz and 80% amplitude.
- Lives in the system tray; settings are shown only when requested.

## Finding a quiet setting

Open **Tone settings...** from the tray menu, enter a low volume such as **0.1%**, click **Apply exact values**, then **Start preview**. Adjust frequency and volume while listening. **Stop preview** returns to your configured playback mode. Leave the app running longer than your monitors' usual idle timeout to check whether the chosen setting keeps them awake. Mute produces silence and may not prevent a device from sleeping.

The percentage controls the generated signal's amplitude, not a measured sound level. The Windows mixer, master volume, output format, and monitor hardware also affect the result. Very small amplitudes may be rounded away by an output device. Frequencies above the selected device's usable sample-rate limit are capped internally.

## Build on Windows (GNU toolchain)

Requires Rust's `stable-x86_64-pc-windows-gnu` toolchain and MinGW-w64 GCC. For an MSYS2 UCRT64 installation:

```powershell
$env:PATH = 'C:\msys64\ucrt64\bin;' + $env:PATH
cargo +stable-x86_64-pc-windows-gnu test --locked --target x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu build --release --locked --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/keep-audio-alive.exe`.

You can also build using the MSVC Rust toolchain with Visual Studio C++ Build Tools installed, using `cargo build --release`.

## Install (optional)

Place `scripts/install.ps1` alongside the built `.exe`, then run:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1
```

The script copies the executable to `%LOCALAPPDATA%\KeepAudioAlive\`, creates a Start Menu shortcut, enables autostart, and launches the app. `scripts/uninstall.ps1` reverses the installation, keeping settings. Both run per-user without administrator rights.

## Tray menu

- **Keep Audio Alive**: enable or disable scheduled playback.
- **Start with Windows**: toggle autostart.
- **Mode / Interval**: configure continuous or periodic playback.
- **Tone settings...**: adjust frequency and volume, mute, and preview.
- **Output Device**: choose a device; refresh the list after plugging one in. Device or mixer-override changes take effect by reopening active playback.
- **Play tone now**: play a three-second test without overlapping other playback; continuous preview already plays the tone.
- **Quit**: stop playback and exit.
