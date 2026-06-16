//! Forces *this* application's audio session to full volume, independent of
//! the Windows master volume slider.
//!
//! Windows tracks a per-application volume (the per-app sliders you see in the
//! Volume Mixer) separately from the master/endpoint volume. Setting this
//! app's session to 1.0 means it always plays at the loudest level the master
//! allows — even if the user has the app turned down in the mixer — without
//! touching the master volume or any other app.

use std::ptr;

use windows::core::Result;
use windows::Win32::Foundation::FALSE;
use windows::Win32::Media::Audio::{
    eRender, IAudioSessionManager, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

/// Pins this process's audio session to 100% (and unmutes it) on every active
/// render endpoint. Best-effort: logs and moves on if anything fails, since a
/// volume tweak should never take down playback.
pub fn force_app_volume_max() {
    if let Err(err) = set_max() {
        eprintln!("failed to force app volume to max: {err}");
    }
}

fn set_max() -> Result<()> {
    unsafe {
        // cpal already initialises COM on the playback thread; a second call
        // here is a harmless no-op (or RPC_E_CHANGED_MODE), which we ignore.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;

        // Apply to every active output endpoint. Each call only ever affects
        // this process's own session on that endpoint, never the master or
        // other apps, so it's safe to do across all of them rather than
        // guessing which one cpal picked.
        for i in 0..collection.GetCount()? {
            let device = collection.Item(i)?;
            let manager: IAudioSessionManager = device.Activate(CLSCTX_ALL, None)?;
            let volume: ISimpleAudioVolume = manager.GetSimpleAudioVolume(None, 0)?;
            volume.SetMasterVolume(1.0, ptr::null())?;
            volume.SetMute(FALSE, ptr::null())?;
        }
    }

    Ok(())
}
