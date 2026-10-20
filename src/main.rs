#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod startup;
#[cfg(windows)]
mod winvol;

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use trayicon::{Icon, MenuBuilder, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::{Window, WindowId};

const ICON_ACTIVE: &[u8] = include_bytes!("../assets/icon_active.ico");
const ICON_IDLE: &[u8] = include_bytes!("../assets/icon_idle.ico");

const INTERVALS: &[(&str, u64)] = &[
    ("30 seconds", 30),
    ("1 minute", 60),
    ("2 minutes", 120),
    ("5 minutes", 300),
];

const FREQUENCIES: &[(&str, u16)] = &[
    ("50 Hz (Low bass, original)", 50),
    ("100 Hz (Bass)", 100),
    ("250 Hz (Low-mid)", 250),
    ("500 Hz (Mid)", 500),
    ("1000 Hz (High-mid)", 1000),
    ("2000 Hz (High)", 2000),
];

const VOLUMES: &[(&str, u8)] = &[
    ("1%", 1),
    ("2%", 2),
    ("3%", 3),
    ("4%", 4),
    ("5%", 5),
    ("10%", 10),
    ("15%", 15),
    ("20%", 20),
    ("40%", 40),
    ("60%", 60),
    ("80%", 80),
    ("100%", 100),
];

#[derive(Clone, Eq, PartialEq, Debug)]
enum AppEvent {
    ToggleEnabled,
    ToggleStartup,
    SetMode(config::Mode),
    SetInterval(u64),
    SetVolume(u8),
    SetFrequency(u16),
    SelectDevice(Option<usize>),
    RefreshDevices,
    PlayNow,
    Quit,
}

struct Shared {
    settings: Mutex<config::Settings>,
    devices: Mutex<Vec<String>>,
}

fn build_menu(settings: &config::Settings, devices: &[String]) -> MenuBuilder<AppEvent> {
    let mode_menu = MenuBuilder::new()
        .checkable(
            "Interval (short tone every N)",
            settings.mode == config::Mode::Interval,
            AppEvent::SetMode(config::Mode::Interval),
        )
        .checkable(
            "Continuous (loop until off)",
            settings.mode == config::Mode::Continuous,
            AppEvent::SetMode(config::Mode::Continuous),
        );

    let mut interval_menu = MenuBuilder::new();
    for (label, secs) in INTERVALS {
        interval_menu = interval_menu.checkable(
            label,
            settings.interval_secs == *secs,
            AppEvent::SetInterval(*secs),
        );
    }

    let mut volume_menu = MenuBuilder::new();
    for (label, pct) in VOLUMES {
        volume_menu = volume_menu.checkable(
            label,
            settings.volume_percent == *pct,
            AppEvent::SetVolume(*pct),
        );
    }

    let mut frequency_menu = MenuBuilder::new();
    for (label, hz) in FREQUENCIES {
        frequency_menu = frequency_menu.checkable(
            label,
            settings.frequency_hz == *hz,
            AppEvent::SetFrequency(*hz),
        );
    }

    let mut device_menu = MenuBuilder::new().checkable(
        "System Default",
        settings.device_name.is_none(),
        AppEvent::SelectDevice(None),
    );
    for (i, name) in devices.iter().enumerate() {
        let checked = settings.device_name.as_deref() == Some(name.as_str());
        device_menu =
            device_menu.checkable(name.as_str(), checked, AppEvent::SelectDevice(Some(i)));
    }
    device_menu = device_menu
        .separator()
        .item("Refresh device list", AppEvent::RefreshDevices);

    MenuBuilder::new()
        .checkable(
            "Keep Audio Alive",
            settings.enabled,
            AppEvent::ToggleEnabled,
        )
        .checkable(
            "Start with Windows",
            startup::is_enabled(),
            AppEvent::ToggleStartup,
        )
        .submenu("Mode", mode_menu)
        .submenu("Interval", interval_menu)
        .submenu("Volume", volume_menu)
        .submenu("Tone Frequency", frequency_menu)
        .submenu("Output Device", device_menu)
        .separator()
        .item("Play tone now", AppEvent::PlayNow)
        .separator()
        .item("Quit", AppEvent::Quit)
}

/// Drives playback. Re-reads settings each cycle and is woken early via
/// `nudge_rx` whenever the user changes enabled/mode/interval/device, so a
/// change (including switching mode or stopping continuous play) takes
/// effect immediately rather than after the current tone/interval ends.
fn scheduler_loop(shared: Arc<Shared>, nudge_rx: Receiver<()>) {
    loop {
        let (enabled, mode, interval_secs, device_name, amplitude, frequency_hz) = {
            let s = shared.settings.lock().unwrap();
            (
                s.enabled,
                s.mode,
                s.interval_secs,
                s.device_name.clone(),
                s.amplitude(),
                s.frequency_hz,
            )
        };

        if !enabled {
            match nudge_rx.recv() {
                Ok(()) => continue,
                Err(_) => break,
            }
        }

        match mode {
            config::Mode::Interval => {
                match nudge_rx.recv_timeout(Duration::from_secs(interval_secs)) {
                    Ok(()) => continue,
                    Err(RecvTimeoutError::Timeout) => {
                        if let Err(err) =
                            audio::play_tone(device_name.as_deref(), amplitude, frequency_hz)
                        {
                            eprintln!("failed to play tone: {err}");
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            config::Mode::Continuous => {
                // Returns as soon as a nudge arrives (settings changed) or
                // the stream errors; either way we loop back and re-read.
                if let Err(err) = audio::run_continuous(
                    device_name.as_deref(),
                    amplitude,
                    frequency_hz,
                    &nudge_rx,
                ) {
                    eprintln!("failed to start continuous tone: {err}");
                    if nudge_rx.recv_timeout(Duration::from_secs(5))
                        == Err(RecvTimeoutError::Disconnected)
                    {
                        break;
                    }
                }
            }
        }
    }
}

struct App {
    window: Option<Window>,
    tray_icon: TrayIcon<AppEvent>,
    shared: Arc<Shared>,
    nudge: Sender<()>,
}

impl App {
    fn refresh_menu(&mut self) {
        let settings = self.shared.settings.lock().unwrap();
        let devices = self.shared.devices.lock().unwrap();
        let menu = build_menu(&settings, &devices);
        drop(settings);
        drop(devices);
        let _ = self.tray_icon.set_menu(&menu);
    }

    fn refresh_icon(&mut self) {
        let enabled = self.shared.settings.lock().unwrap().enabled;
        let icon =
            Icon::from_buffer(if enabled { ICON_ACTIVE } else { ICON_IDLE }, None, None).unwrap();
        let _ = self.tray_icon.set_icon(&icon);
    }

    fn persist_and_refresh(&mut self) {
        config::save(&self.shared.settings.lock().unwrap());
        self.refresh_menu();
        let _ = self.nudge.send(());
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Tray-only app: create a hidden, taskbar-skipped window so winit has
        // an event loop to drive, without showing any UI.
        let attrs = Window::default_attributes()
            .with_visible(false)
            .with_skip_taskbar(true);
        self.window = Some(event_loop.create_window(attrs).unwrap());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::ToggleEnabled => {
                {
                    let mut settings = self.shared.settings.lock().unwrap();
                    settings.enabled = !settings.enabled;
                }
                self.persist_and_refresh();
                self.refresh_icon();
            }
            AppEvent::ToggleStartup => {
                if let Err(err) = startup::set_enabled(!startup::is_enabled()) {
                    eprintln!("failed to toggle start-with-windows: {err}");
                }
                self.refresh_menu();
            }
            AppEvent::SetMode(mode) => {
                self.shared.settings.lock().unwrap().mode = mode;
                self.persist_and_refresh();
            }
            AppEvent::SetInterval(secs) => {
                self.shared.settings.lock().unwrap().interval_secs = secs;
                self.persist_and_refresh();
            }
            AppEvent::SetVolume(pct) => {
                self.shared.settings.lock().unwrap().volume_percent = pct;
                self.persist_and_refresh();
            }
            AppEvent::SetFrequency(hz) => {
                self.shared.settings.lock().unwrap().frequency_hz = hz;
                self.persist_and_refresh();
            }
            AppEvent::SelectDevice(idx) => {
                let name = idx.and_then(|i| self.shared.devices.lock().unwrap().get(i).cloned());
                self.shared.settings.lock().unwrap().device_name = name;
                self.persist_and_refresh();
            }
            AppEvent::RefreshDevices => {
                *self.shared.devices.lock().unwrap() = audio::list_output_devices();
                self.refresh_menu();
            }
            AppEvent::PlayNow => {
                let (device_name, amplitude, frequency_hz) = {
                    let s = self.shared.settings.lock().unwrap();
                    (s.device_name.clone(), s.amplitude(), s.frequency_hz)
                };
                std::thread::spawn(move || {
                    if let Err(err) =
                        audio::play_tone(device_name.as_deref(), amplitude, frequency_hz)
                    {
                        eprintln!("failed to play tone: {err}");
                    }
                });
            }
            AppEvent::Quit => event_loop.exit(),
        }
    }
}

fn main() {
    let settings = config::load();
    let devices = audio::list_output_devices();
    let initial_icon = if settings.enabled {
        ICON_ACTIVE
    } else {
        ICON_IDLE
    };

    let shared = Arc::new(Shared {
        settings: Mutex::new(settings.clone()),
        devices: Mutex::new(devices.clone()),
    });

    let (nudge_tx, nudge_rx) = std::sync::mpsc::channel();
    {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || scheduler_loop(shared, nudge_rx));
    }

    let event_loop = EventLoop::<AppEvent>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();

    let tray_icon = TrayIconBuilder::new()
        .sender(move |event: &AppEvent| {
            let _ = proxy.send_event(event.clone());
        })
        .icon_from_buffer(initial_icon)
        .tooltip("Keep Audio Alive")
        .menu(build_menu(&settings, &devices))
        .build()
        .expect("failed to create tray icon");

    let mut app = App {
        window: None,
        tray_icon,
        shared,
        nudge: nudge_tx,
    };
    event_loop.run_app(&mut app).expect("event loop error");
}
