#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod startup;
mod tone_ui;
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

#[derive(Clone, PartialEq, Debug)]
enum AppEvent {
    ToggleEnabled,
    ToggleStartup,
    SetMode(config::Mode),
    SetInterval(u64),
    SetVolume(f32),
    OpenToneSettings,
    SetMixer(bool),
    SetMuted(bool),
    Preview(bool),
    SetFrequency(u16),
    SelectDevice(Option<usize>),
    RefreshDevices,
    PlayNow,
    Quit,
}

struct Shared {
    settings: Arc<Mutex<config::Settings>>,
    preview: std::sync::atomic::AtomicBool,
    one_shot: std::sync::atomic::AtomicBool,
    quitting: std::sync::atomic::AtomicBool,
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
        .item("Tone settings...", AppEvent::OpenToneSettings)
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
        use std::sync::atomic::Ordering::Relaxed;
        if shared.quitting.load(Relaxed) {
            break;
        }
        if shared.preview.load(Relaxed) || shared.one_shot.swap(false, Relaxed) {
            let preview = shared.preview.load(Relaxed);
            if let Err(err) = audio::play(shared.settings.clone(), &nudge_rx, preview) {
                shared.preview.store(false, Relaxed);
                tone_ui::show_error(&err);
            }
            continue;
        }
        let (enabled, mode, interval_secs) = {
            let s = shared.settings.lock().unwrap();
            (s.enabled, s.mode, s.interval_secs)
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
                        if let Err(err) = audio::play(shared.settings.clone(), &nudge_rx, false) {
                            eprintln!("failed to play tone: {err}");
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            config::Mode::Continuous => {
                // Returns as soon as a nudge arrives (settings changed) or
                // the stream errors; either way we loop back and re-read.
                if let Err(err) = audio::play(shared.settings.clone(), &nudge_rx, true) {
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
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    tone_window: Option<std::thread::JoinHandle<()>>,
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
                config::save(&self.shared.settings.lock().unwrap());
            }
            AppEvent::SetFrequency(hz) => {
                self.shared.settings.lock().unwrap().frequency_hz = hz;
                config::save(&self.shared.settings.lock().unwrap());
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
            AppEvent::OpenToneSettings => {
                if !tone_ui::focus_existing()
                    && self.tone_window.as_ref().is_none_or(|t| t.is_finished())
                {
                    let settings = self.shared.settings.lock().unwrap().clone();
                    let proxy = self.proxy.clone();
                    self.tone_window =
                        Some(std::thread::spawn(move || tone_ui::show(settings, proxy)));
                }
            }
            AppEvent::SetMuted(value) => {
                self.shared.settings.lock().unwrap().muted = value;
                config::save(&self.shared.settings.lock().unwrap());
            }
            AppEvent::SetMixer(value) => {
                self.shared.settings.lock().unwrap().force_mixer_volume = value;
                self.persist_and_refresh();
            }
            AppEvent::Preview(on) => {
                self.shared
                    .preview
                    .store(on, std::sync::atomic::Ordering::Relaxed);
                let _ = self.nudge.send(());
            }
            AppEvent::PlayNow => {
                if !self
                    .shared
                    .preview
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    self.shared
                        .one_shot
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = self.nudge.send(());
                }
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
        settings: Arc::new(Mutex::new(settings.clone())),
        preview: std::sync::atomic::AtomicBool::new(false),
        one_shot: std::sync::atomic::AtomicBool::new(false),
        quitting: std::sync::atomic::AtomicBool::new(false),
        devices: Mutex::new(devices.clone()),
    });

    let (nudge_tx, nudge_rx) = std::sync::mpsc::channel();
    let playback_shared = shared.clone();
    let scheduler = std::thread::spawn(move || scheduler_loop(playback_shared, nudge_rx));

    let event_loop = EventLoop::<AppEvent>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    if std::env::args().any(|arg| arg == "--tone-settings") {
        let _ = proxy.send_event(AppEvent::OpenToneSettings);
    }

    let tray_proxy = proxy.clone();
    let tray_icon = TrayIconBuilder::new()
        .sender(move |event: &AppEvent| {
            let _ = tray_proxy.send_event(event.clone());
        })
        .icon_from_buffer(initial_icon)
        .tooltip("Keep Audio Alive")
        .menu(build_menu(&settings, &devices))
        .build()
        .expect("failed to create tray icon");

    let mut app = App {
        window: None,
        tray_icon,
        shared: shared.clone(),
        nudge: nudge_tx.clone(),
        proxy,
        tone_window: None,
    };
    event_loop.run_app(&mut app).expect("event loop error");
    shared
        .quitting
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = nudge_tx.send(());
    let _ = scheduler.join();
}
