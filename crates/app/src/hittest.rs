//! Windows: native hit testing for the frameless window so the integrated
//! bar behaves like a real title bar (drag, double-click to maximize, Win+
//! arrow snapping, Snap Layouts on the maximize button) and the edges resize.
//! A window subclass answers `WM_NCHITTEST` from regions the chrome publishes
//! each frame. Other platforms: no-ops.

#[derive(Default, Clone)]
pub struct Regions {
    /// Physical pixels, window-client coordinates.
    pub enabled: bool,
    pub resizable: bool,
    pub bar: [i32; 4],
    pub interactive: Vec<[i32; 4]>,
    pub max_button: [i32; 4],
    pub border: i32,
}

#[derive(Default)]
pub struct NativeEvents {
    pub toggle_maximize: bool,
    pub max_hover: bool,
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTMAXBUTTON, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE,
        WM_NCMOUSEMOVE,
    };

    use super::{NativeEvents, Regions};

    static REGIONS: Mutex<Regions> = Mutex::new(Regions { enabled: false, resizable: true, bar: [0; 4], interactive: Vec::new(), max_button: [0; 4], border: 6 });
    static EVENTS: Mutex<NativeEvents> = Mutex::new(NativeEvents { toggle_maximize: false, max_hover: false });

    fn inside(r: &[i32; 4], x: i32, y: i32) -> bool {
        x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        unsafe {
            match msg {
                WM_NCHITTEST => {
                    let Ok(regions) = REGIONS.lock() else { return DefSubclassProc(hwnd, msg, wparam, lparam) };
                    if !regions.enabled {
                        return DefSubclassProc(hwnd, msg, wparam, lparam);
                    }
                    let mut pt = POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 };
                    let _ = ScreenToClient(hwnd, &mut pt);
                    let mut rc = windows::Win32::Foundation::RECT::default();
                    let _ = GetClientRect(hwnd, &mut rc);
                    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
                    let maximized = IsZoomed(hwnd).as_bool();
                    if regions.resizable && !maximized {
                        let b = regions.border;
                        let left = pt.x < b;
                        let right = pt.x >= w - b;
                        let top = pt.y < b;
                        let bottom = pt.y >= h - b;
                        let hit = match (left, right, top, bottom) {
                            (true, _, true, _) => HTTOPLEFT,
                            (_, true, true, _) => HTTOPRIGHT,
                            (true, _, _, true) => HTBOTTOMLEFT,
                            (_, true, _, true) => HTBOTTOMRIGHT,
                            (true, _, _, _) => HTLEFT,
                            (_, true, _, _) => HTRIGHT,
                            (_, _, true, _) => HTTOP,
                            (_, _, _, true) => HTBOTTOM,
                            _ => 0,
                        };
                        if hit != 0 {
                            return LRESULT(hit as isize);
                        }
                    }
                    if inside(&regions.max_button, pt.x, pt.y) {
                        return LRESULT(HTMAXBUTTON as isize);
                    }
                    if inside(&regions.bar, pt.x, pt.y) {
                        if regions.interactive.iter().any(|r| inside(r, pt.x, pt.y)) {
                            return LRESULT(HTCLIENT as isize);
                        }
                        return LRESULT(HTCAPTION as isize);
                    }
                    LRESULT(HTCLIENT as isize)
                }
                // Snap Layouts needs HTMAXBUTTON from the hit test; then the clicks arrive as
                // non-client messages, which we turn into our own maximize toggle.
                WM_NCMOUSEMOVE => {
                    if let Ok(mut e) = EVENTS.lock() {
                        e.max_hover = wparam.0 as u32 == HTMAXBUTTON;
                    }
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
                WM_NCMOUSELEAVE => {
                    if let Ok(mut e) = EVENTS.lock() {
                        e.max_hover = false;
                    }
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
                WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wparam.0 as u32 == HTMAXBUTTON => LRESULT(0),
                WM_NCLBUTTONUP if wparam.0 as u32 == HTMAXBUTTON => {
                    if let Ok(mut e) = EVENTS.lock() {
                        e.toggle_maximize = true;
                    }
                    LRESULT(0)
                }
                _ => DefSubclassProc(hwnd, msg, wparam, lparam),
            }
        }
    }

    pub fn install(hwnd: isize) {
        unsafe {
            let ok = SetWindowSubclass(HWND(hwnd as *mut _), Some(proc), 0x6175_6469, 0);
            if !ok.as_bool() {
                tracing::warn!("window subclass failed; native drag/snap unavailable");
            }
        }
    }

    pub fn publish(r: Regions) {
        if let Ok(mut g) = REGIONS.lock() {
            *g = r;
        }
    }

    pub fn take_events() -> NativeEvents {
        EVENTS.lock().map(|mut e| NativeEvents { toggle_maximize: std::mem::take(&mut e.toggle_maximize), max_hover: e.max_hover }).unwrap_or_default()
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{NativeEvents, Regions};
    pub fn install(_hwnd: isize) {}
    pub fn publish(_r: Regions) {}
    pub fn take_events() -> NativeEvents {
        NativeEvents::default()
    }
}

pub use imp::{install, publish, take_events};

/// Install on a winit window (Windows only; no-op elsewhere).
pub fn install_on(window: &winit::window::Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(h) = window.window_handle() {
        if let RawWindowHandle::Win32(w) = h.as_raw() {
            install(w.hwnd.get());
        }
    }
}

pub fn rect_px(r: egui::Rect, ppp: f32) -> [i32; 4] {
    [(r.min.x * ppp).floor() as i32, (r.min.y * ppp).floor() as i32, (r.max.x * ppp).ceil() as i32, (r.max.y * ppp).ceil() as i32]
}
