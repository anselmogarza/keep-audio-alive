use crate::config::Settings;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::sync::{Arc, Mutex, mpsc::Receiver};
use std::time::Duration;

pub fn list_output_devices() -> Vec<String> {
    cpal::default_host()
        .output_devices()
        .map(|devs| {
            devs.filter_map(|d| d.description().ok().map(|v| v.name().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn play(
    settings: Arc<Mutex<Settings>>,
    stop: &Receiver<()>,
    continuous: bool,
) -> Result<(), String> {
    let host = cpal::default_host();
    let initial = settings.lock().unwrap().clone();
    let device = match initial.device_name.as_deref() {
        Some(name) => host
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.description().map(|v| v.name() == name).unwrap_or(false))
            .ok_or_else(|| format!("output device '{name}' not found"))?,
        None => host
            .default_output_device()
            .ok_or("no default output device")?,
    };
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let fading_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build::<f32>(&device, config.into(), settings, fading_out.clone())
        }
        cpal::SampleFormat::I16 => {
            build::<i16>(&device, config.into(), settings, fading_out.clone())
        }
        cpal::SampleFormat::U16 => {
            build::<u16>(&device, config.into(), settings, fading_out.clone())
        }
        other => Err(format!("unsupported sample format: {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if initial.force_mixer_volume {
        crate::winvol::force_app_volume_max();
    }
    if continuous {
        let _ = stop.recv();
    } else {
        let _ = stop.recv_timeout(Duration::from_secs(3));
    }
    fading_out.store(true, std::sync::atomic::Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(40));
    drop(stream);
    Ok(())
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    settings: Arc<Mutex<Settings>>,
    fading_out: Arc<std::sync::atomic::AtomicBool>,
) -> Result<cpal::Stream, String> {
    let sample_rate = config.sample_rate as f32;
    let channels = config.channels as usize;
    let mut phase = 0.0_f32;
    let mut amplitude = 0.0_f32;
    let mut frequency = settings.lock().unwrap().frequency_hz as f32;
    // A 5ms time constant smooths changes; the 40ms tail lets stop fades settle.
    let smoothing = 1.0 - (-1.0 / (0.005 * sample_rate)).exp();
    let mut target_amplitude = 0.0;
    let mut target_frequency = frequency;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                // Never block the audio callback on UI or configuration writes.
                if let Ok(s) = settings.try_lock() {
                    target_amplitude = s.amplitude();
                    target_frequency =
                        (s.frequency_hz as f32).clamp(20.0, (sample_rate * 0.49).min(20_000.0));
                }
                if fading_out.load(std::sync::atomic::Ordering::Relaxed) {
                    target_amplitude = 0.0;
                }
                for frame in data.chunks_mut(channels) {
                    amplitude += (target_amplitude - amplitude) * smoothing;
                    frequency += (target_frequency - frequency) * smoothing;
                    let value = T::from_sample((phase * std::f32::consts::TAU).sin() * amplitude);
                    phase = (phase + frequency / sample_rate).fract();
                    frame.fill(value);
                }
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
