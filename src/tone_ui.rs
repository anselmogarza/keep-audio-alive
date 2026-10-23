//! Native Windows tone controls; the tray remains the app's primary interface.
use crate::{AppEvent, config::Settings};
use std::{
    cell::RefCell,
    ffi::c_void,
    sync::atomic::{AtomicIsize, Ordering},
};
use winit::event_loop::EventLoopProxy;
type Handle = isize;
#[repr(C)]
struct WndClass {
    style: u32,
    proc: Option<unsafe extern "system" fn(Handle, u32, usize, isize) -> isize>,
    cls_extra: i32,
    wnd_extra: i32,
    instance: Handle,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu: *const u16,
    name: *const u16,
}
#[repr(C)]
#[derive(Default)]
struct Msg {
    hwnd: Handle,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    x: i32,
    y: i32,
    private: u32,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const WndClass) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        param: *const c_void,
    ) -> Handle;
    fn DefWindowProcW(hwnd: Handle, msg: u32, wp: usize, lp: isize) -> isize;
    fn DestroyWindow(hwnd: Handle) -> i32;
    fn GetMessageW(msg: *mut Msg, hwnd: Handle, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn IsDialogMessageW(hwnd: Handle, msg: *const Msg) -> i32;
    fn PostQuitMessage(code: i32);
    fn SendMessageW(hwnd: Handle, msg: u32, wp: usize, lp: isize) -> isize;
    fn SetWindowTextW(hwnd: Handle, text: *const u16) -> i32;
    fn GetWindowTextW(hwnd: Handle, text: *mut u16, max: i32) -> i32;
    fn SetForegroundWindow(hwnd: Handle) -> i32;
    fn ShowWindow(hwnd: Handle, cmd: i32) -> i32;
    fn LoadCursorW(instance: Handle, name: *const u16) -> Handle;
    fn MessageBoxW(hwnd: Handle, text: *const u16, title: *const u16, flags: u32) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
}
#[link(name = "comctl32")]
unsafe extern "system" {
    fn InitCommonControls();
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetStockObject(index: i32) -> Handle;
}
static WINDOW: AtomicIsize = AtomicIsize::new(0);
thread_local! { static UI: RefCell<Option<Ui>> = const { RefCell::new(None) }; }
struct Ui {
    proxy: EventLoopProxy<AppEvent>,
    freq: Handle,
    volume: Handle,
    hz: Handle,
    percent: Handle,
    mixer: Handle,
    muted: Handle,
    status: Handle,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn emit(proxy: &EventLoopProxy<AppEvent>, event: AppEvent) {
    let _ = proxy.send_event(event);
}
fn position(value: f32, min: f32, max: f32) -> usize {
    ((value.clamp(min, max) / min).ln() / (max / min).ln() * 1000.0).round() as usize
}
fn value(pos: isize, min: f32, max: f32) -> f32 {
    min * (max / min).powf(pos as f32 / 1000.0)
}
unsafe fn text(hwnd: Handle, s: &str) {
    unsafe {
        SetWindowTextW(hwnd, wide(s).as_ptr());
    }
}
unsafe fn read(hwnd: Handle) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), 128) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}
unsafe fn checked(hwnd: Handle) -> bool {
    unsafe { SendMessageW(hwnd, 0xF0, 0, 0) == 1 }
}
// Match the native control creation API: parent, class, label, style, rectangle, ID.
#[allow(clippy::too_many_arguments)]
unsafe fn control(
    parent: Handle,
    class: &str,
    title: &str,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: isize,
) -> Handle {
    unsafe {
        let hwnd = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(title).as_ptr(),
            0x50000000 | style,
            x,
            y,
            w,
            h,
            parent,
            id,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        SendMessageW(hwnd, 0x30, GetStockObject(17) as usize, 1);
        hwnd
    }
}
pub fn focus_existing() -> bool {
    let hwnd = WINDOW.load(Ordering::SeqCst);
    if hwnd == 0 {
        return false;
    }
    unsafe {
        ShowWindow(hwnd, 9);
        SetForegroundWindow(hwnd);
    }
    true
}
pub fn show_error(error: &str) {
    unsafe {
        MessageBoxW(
            WINDOW.load(Ordering::SeqCst),
            wide(error).as_ptr(),
            wide("Keep Audio Alive").as_ptr(),
            0x10,
        );
    }
}
pub fn show(settings: Settings, proxy: EventLoopProxy<AppEvent>) {
    unsafe {
        InitCommonControls();
        let name = wide("KeepAudioAliveToneSettings");
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WndClass {
            style: 0,
            proc: Some(wndproc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: 0,
            cursor: LoadCursorW(0, 32512usize as *const u16),
            background: 16,
            menu: std::ptr::null(),
            name: name.as_ptr(),
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            0x10000,
            name.as_ptr(),
            wide("Keep Audio Alive - Tone settings").as_ptr(),
            0x00CA0000,
            i32::MIN,
            i32::MIN,
            570,
            455,
            0,
            0,
            instance,
            std::ptr::null(),
        );
        if hwnd == 0 {
            show_error("Could not open tone settings.");
            return;
        }
        control(
            hwnd,
            "STATIC",
            "Frequency (20 - 20,000 Hz)",
            0,
            20,
            18,
            320,
            24,
            0,
        );
        // Tab stop + TBS_NOTICKS: 1,001 automatic ticks otherwise form a solid bar.
        let freq = control(hwnd, "msctls_trackbar32", "", 0x10010, 16, 45, 360, 40, 101);
        let hz = control(
            hwnd,
            "EDIT",
            &settings.frequency_hz.to_string(),
            0x00810080,
            390,
            48,
            100,
            25,
            102,
        );
        control(hwnd, "STATIC", "Hz", 0, 495, 52, 35, 24, 0);
        control(
            hwnd,
            "STATIC",
            "Volume (0.01 - 100%)",
            0,
            20,
            100,
            340,
            24,
            0,
        );
        let volume = control(
            hwnd,
            "msctls_trackbar32",
            "",
            0x10010,
            16,
            128,
            360,
            40,
            103,
        );
        let percent = control(
            hwnd,
            "EDIT",
            &format!("{:.4}", settings.volume_percent),
            0x00810080,
            390,
            132,
            100,
            25,
            104,
        );
        control(hwnd, "STATIC", "%", 0, 495, 136, 35, 24, 0);
        control(
            hwnd,
            "BUTTON",
            "Apply exact values",
            0x10000,
            350,
            176,
            175,
            30,
            105,
        );
        let muted = control(hwnd, "BUTTON", "Mute tone", 0x10003, 20, 178, 190, 28, 106);
        SendMessageW(muted, 0xF1, settings.muted as usize, 0);
        let mixer = control(
            hwnd,
            "BUTTON",
            "Set this app's Windows mixer volume to 100%",
            0x10003,
            20,
            220,
            510,
            28,
            107,
        );
        SendMessageW(mixer, 0xF1, settings.force_mixer_volume as usize, 0);
        control(
            hwnd,
            "STATIC",
            "Sliders apply live. Use arrow keys for fine adjustments.\r\nExact values: enter both numbers, then click Apply.\r\nSettings save automatically. Preview uses the selected output device.",
            0,
            20,
            258,
            520,
            64,
            0,
        );
        control(
            hwnd,
            "BUTTON",
            "Start preview",
            0x10000,
            20,
            340,
            145,
            32,
            108,
        );
        control(
            hwnd,
            "BUTTON",
            "Stop preview",
            0x10000,
            180,
            340,
            145,
            32,
            109,
        );
        let status = control(hwnd, "STATIC", "Preview stopped", 0, 20, 382, 510, 24, 0);
        for (slider, pos) in [
            (freq, position(settings.frequency_hz as f32, 20.0, 20000.0)),
            (volume, position(settings.volume_percent, 0.01, 100.0)),
        ] {
            SendMessageW(slider, 0x406, 1, 1000isize << 16);
            SendMessageW(slider, 0x405, 1, pos as isize);
        }
        UI.with(|ui| {
            *ui.borrow_mut() = Some(Ui {
                proxy,
                freq,
                volume,
                hz,
                percent,
                mixer,
                muted,
                status,
            })
        });
        WINDOW.store(hwnd, Ordering::SeqCst);
        ShowWindow(hwnd, 5);
        SetForegroundWindow(hwnd);
        let mut msg = Msg::default();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        WINDOW.store(0, Ordering::SeqCst);
        UI.with(|ui| {
            if let Some(ui) = ui.borrow_mut().take() {
                emit(&ui.proxy, AppEvent::Preview(false));
            }
        });
    }
}
unsafe extern "system" fn wndproc(hwnd: Handle, msg: u32, wp: usize, lp: isize) -> isize {
    unsafe {
        match msg {
            0x10 => {
                DestroyWindow(hwnd);
                return 0;
            }
            0x2 => {
                PostQuitMessage(0);
                return 0;
            }
            0x114 | 0x111 => {
                UI.with(|cell| {
                    let Ok(mut borrow) = cell.try_borrow_mut() else {
                        return;
                    };
                    let Some(ui) = borrow.as_mut() else { return };
                    if msg == 0x114 {
                        if lp == ui.freq {
                            let hz = value(SendMessageW(ui.freq, 0x400, 0, 0), 20.0, 20000.0)
                                .round() as u16;
                            text(ui.hz, &hz.to_string());
                            emit(&ui.proxy, AppEvent::SetFrequency(hz));
                        } else if lp == ui.volume {
                            let pct = value(SendMessageW(ui.volume, 0x400, 0, 0), 0.01, 100.0);
                            text(ui.percent, &format!("{pct:.4}"));
                            emit(&ui.proxy, AppEvent::SetVolume(pct));
                        }
                    } else if wp >> 16 == 0 {
                        match wp & 0xffff {
                            105 => {
                                let hz = read(ui.hz).trim().parse::<u16>();
                                let pct = read(ui.percent).trim().parse::<f32>();
                                if let (Ok(hz), Ok(pct)) = (hz, pct)
                                    && (20..=20000).contains(&hz)
                                    && pct.is_finite()
                                    && (0.01..=100.0).contains(&pct)
                                {
                                    SendMessageW(
                                        ui.freq,
                                        0x405,
                                        1,
                                        position(hz as f32, 20.0, 20000.0) as isize,
                                    );
                                    SendMessageW(
                                        ui.volume,
                                        0x405,
                                        1,
                                        position(pct, 0.01, 100.0) as isize,
                                    );
                                    emit(&ui.proxy, AppEvent::SetFrequency(hz));
                                    emit(&ui.proxy, AppEvent::SetVolume(pct));
                                    text(ui.status, "Exact values applied");
                                    return;
                                }
                                text(ui.status, "Enter 20-20000 whole Hz and 0.01-100% volume.");
                            }
                            106 => emit(&ui.proxy, AppEvent::SetMuted(checked(ui.muted))),
                            107 => emit(&ui.proxy, AppEvent::SetMixer(checked(ui.mixer))),
                            108 => {
                                emit(&ui.proxy, AppEvent::Preview(true));
                                text(ui.status, "Preview playing - changes apply live");
                            }
                            109 => {
                                emit(&ui.proxy, AppEvent::Preview(false));
                                text(ui.status, "Preview stopped");
                            }
                            _ => {}
                        }
                    }
                });
                return 0;
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logarithmic_controls_cover_endpoints_and_low_volumes() {
        for (min, max) in [(20.0, 20000.0), (0.01, 100.0)] {
            assert!((value(0, min, max) - min).abs() < 0.0001);
            assert!((value(1000, min, max) - max).abs() < 0.01);
            for input in [min, (min * max).sqrt(), max] {
                let output = value(position(input, min, max) as isize, min, max);
                assert!((output / input - 1.0).abs() < 0.005);
            }
        }
        assert!(position(0.1, 0.01, 100.0) > position(0.01, 0.01, 100.0));
    }
}
