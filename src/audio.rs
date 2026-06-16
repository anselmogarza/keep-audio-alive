use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

const TONE_HZ: f32 = 50.0;
const ONE_SHOT_DURATION: Duration = Duration::from_secs(3);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Names of every available output device, in enumeration order.
/// The index into this list is what `Settings::device_name` resolves through.
pub fn list_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .map(|devs| {
            devs.filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_device(host: &cpal::Host, device_name: Option<&str>) -> Result<cpal::Device, String> {
    match device_name {
        Some(name) => host
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| {
                d.description()
                    .map(|desc| desc.name() == name)
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("output device '{name}' not found")),
        None => host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_string()),
    }
}

/// Plays a short 50Hz tone once on the named device (or the system default
/// when `None`) at `amplitude` (0.0–1.0) and returns once it finishes.
pub fn play_tone(device_name: Option<&str>, amplitude: f32) -> Result<(), String> {
    with_tone_stream(device_name, amplitude, || {
        std::thread::sleep(ONE_SHOT_DURATION);
        Ok(())
    })
}

/// Plays the 50Hz tone continuously (no gaps) at `amplitude` (0.0–1.0) until
/// `stop_rx` receives anything or disconnects. Returns once playback stops.
pub fn run_continuous(
    device_name: Option<&str>,
    amplitude: f32,
    stop_rx: &Receiver<()>,
) -> Result<(), String> {
    with_tone_stream(device_name, amplitude, || loop {
        match stop_rx.recv_timeout(STOP_POLL_INTERVAL) {
            Err(RecvTimeoutError::Timeout) => continue,
            _ => return Ok(()),
        }
    })
}

/// Opens the tone stream, starts it, forces this app's session to full volume,
/// then runs `wait` until it returns. The stream stays alive (and audible) for
/// the whole call and stops when it's dropped on return.
fn with_tone_stream<F>(device_name: Option<&str>, amplitude: f32, wait: F) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    let host = cpal::default_host();
    let device = resolve_device(&host, device_name)?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, config.into(), amplitude),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, config.into(), amplitude),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, config.into(), amplitude),
        other => Err(format!("unsupported sample format: {other:?}")),
    }?;

    stream.play().map_err(|e| e.to_string())?;
    // Playing creates this process's audio session; now pin it to 100% so the
    // tone is at full level regardless of where the app sits in the mixer.
    #[cfg(windows)]
    crate::winvol::force_app_volume_max();

    wait()
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    amplitude: f32,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let sample_rate = config.sample_rate as f32;
    let channels = config.channels as usize;
    let phase_step = TONE_HZ / sample_rate;
    let mut phase = 0f32;

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                for frame in data.chunks_mut(channels) {
                    let sample = (phase * std::f32::consts::TAU).sin() * amplitude;
                    phase = (phase + phase_step).fract();
                    let value = T::from_sample(sample);
                    for slot in frame.iter_mut() {
                        *slot = value;
                    }
                }
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
