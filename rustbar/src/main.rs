#![windows_subsystem = "windows"]

//! A minimal top bar for GlazeWM: workspaces on the left, the date in the
//! middle and CPU, memory and battery on the right. One plain GDI window per
//! monitor, plus a hidden window that owns the timer and the menu.

mod glazewm;
mod info;

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use std::sync::LazyLock;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use glazewm::Workspace;
use info::{Info, Names, Sampler};

const WM_GLAZEWM: u32 = WM_APP + 1;
const WM_REBUILD: u32 = WM_APP + 2;

const TIMER_TICK: usize = 1;
const TIMER_REBUILD: usize = 2;
/// How often the date and system numbers refresh.
const TICK_MS: u32 = 5000;
/// Display changes come in bursts; rebuild the bars once they settle.
const REBUILD_DELAY_MS: u32 = 500;

const CMD_STARTUP: usize = 1;
const CMD_EXIT: usize = 2;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "RustBar";

// Layout in 96-DPI pixels; scaled per monitor with `s()`.
const HEIGHT: i32 = 28;
const FONT: i32 = 13;
const WS_PAD_X: i32 = 8;
const WS_PAD_Y: i32 = 3;
/// Space between the screen edges and the outermost items.
const EDGE: i32 = 8;
const WS_GAP: i32 = 4;
const ITEM_GAP: i32 = 20;

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}
const BG: COLORREF = rgb(5, 2, 20);
const BORDER: COLORREF = rgb(17, 15, 32);
const WS_BG: COLORREF = rgb(17, 15, 32);
const WS_DISPLAYED: COLORREF = rgb(42, 40, 55);
const WS_FOCUSED: COLORREF = rgb(40, 58, 137);
const TEXT: COLORREF = rgb(185, 185, 192);
const SUBTEXT: COLORREF = rgb(115, 130, 175);

/// CPU and GPU names, read on first paint.
static NAMES: LazyLock<Names> = LazyLock::new(info::names);

thread_local! {
    static CONTROL: Cell<HWND> = const { Cell::new(null_mut()) };
    static BARS: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
    static INFO: RefCell<Info> = RefCell::new(Info::default());
    static SAMPLER: RefCell<Sampler> = RefCell::new(Sampler::default());
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Scales a 96-DPI length to `dpi`.
fn s(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

fn main() {
    unsafe {
        CreateMutexW(null(), 0, wide("rustbar-single-instance").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinst = GetModuleHandleW(null());
        let control_class = wide("RustBarControl");
        let bar_class = wide("RustBar");
        let classes: [(&Vec<u16>, WNDPROC); 2] = [(&control_class, Some(control_proc)), (&bar_class, Some(bar_proc))];
        for (class, proc) in classes {
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: proc,
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: LoadImageW(hinst, 1 as *const u16, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE) as HICON,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class.as_ptr(),
            };
            RegisterClassW(&wc);
        }

        // A hidden top-level window: it gets WM_DISPLAYCHANGE broadcasts and
        // outlives the bars, which are recreated when monitors change.
        let control = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            control_class.as_ptr(),
            wide("rustbar").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        );
        CONTROL.set(control);
        tick();
        SetTimer(control, TIMER_TICK, TICK_MS, None);
        create_bars();

        let control = control as isize;
        glazewm::spawn(move || {
            PostMessageW(control as HWND, WM_GLAZEWM, 0, 0);
        });

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn control_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wparam == TIMER_TICK => {
            tick();
            0
        }
        WM_TIMER if wparam == TIMER_REBUILD => {
            KillTimer(hwnd, TIMER_REBUILD);
            create_bars();
            0
        }
        WM_DISPLAYCHANGE | WM_REBUILD => {
            SetTimer(hwnd, TIMER_REBUILD, REBUILD_DELAY_MS, None);
            0
        }
        WM_GLAZEWM => {
            redraw();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONUP => {
            let (x, y) = ((lparam & 0xFFFF) as i16 as i32, (lparam >> 16 & 0xFFFF) as i16 as i32);
            if let Some(ws) = workspace_at(hwnd, x, y) {
                glazewm::command(&format!("focus --workspace {}", ws.name));
            }
            0
        }
        WM_SETCURSOR => {
            let mut pt: POINT = zeroed();
            GetCursorPos(&mut pt);
            ScreenToClient(hwnd, &mut pt);
            let cursor = if workspace_at(hwnd, pt.x, pt.y).is_some() { IDC_HAND } else { IDC_ARROW };
            SetCursor(LoadCursorW(null_mut(), cursor));
            1
        }
        WM_RBUTTONUP => {
            menu();
            0
        }
        WM_DPICHANGED => {
            PostMessageW(CONTROL.get(), WM_REBUILD, 0, 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Samples the clock and system numbers, redrawing if anything changed.
fn tick() {
    let info = SAMPLER.with_borrow_mut(|s| s.sample());
    if INFO.with_borrow(|old| *old != info) {
        INFO.set(info);
        redraw();
    }
}

fn redraw() {
    BARS.with_borrow(|bars| {
        for &bar in bars {
            unsafe { InvalidateRect(bar, null(), 0) };
        }
    });
}

/// Destroys any existing bars and creates one along the top of each monitor.
unsafe fn create_bars() {
    for bar in BARS.take() {
        DestroyWindow(bar);
    }
    unsafe extern "system" fn add(mon: HMONITOR, _: HDC, rect: *mut RECT, _: LPARAM) -> BOOL {
        let r = *rect;
        let (mut dpi, mut dpi_y) = (96, 96);
        GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y);
        let bar = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            wide("RustBar").as_ptr(),
            wide("rustbar").as_ptr(),
            WS_POPUP | WS_VISIBLE,
            r.left,
            r.top,
            r.right - r.left,
            s(HEIGHT, dpi),
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        SetWindowLongPtrW(bar, GWLP_USERDATA, mon as isize);
        BARS.with_borrow_mut(|bars| bars.push(bar));
        TRUE
    }
    EnumDisplayMonitors(null_mut(), null(), Some(add), 0);
}

unsafe fn create_font(dpi: u32) -> HFONT {
    CreateFontW(
        -s(FONT, dpi), 0, 0, 0, FW_NORMAL as _, 0, 0, 0, DEFAULT_CHARSET as _, OUT_DEFAULT_PRECIS as _,
        CLIP_DEFAULT_PRECIS as _, ANTIALIASED_QUALITY as _, (VARIABLE_PITCH | FF_SWISS) as _,
        wide("Segoe UI").as_ptr(),
    )
}

/// The workspaces of the bar's monitor, matched by HMONITOR or else position.
unsafe fn workspaces(hwnd: HWND) -> Vec<Workspace> {
    let mon = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    let mut r: RECT = zeroed();
    GetWindowRect(hwnd, &mut r);
    let monitors = glazewm::MONITORS.lock().unwrap();
    let found = monitors.iter().find(|m| m.handle == mon).or_else(|| monitors.iter().find(|m| m.x == r.left && m.y == r.top));
    found.map(|m| m.workspaces.clone()).unwrap_or_default()
}

fn text_width(hdc: HDC, text: &[u16]) -> i32 {
    let mut size: SIZE = unsafe { zeroed() };
    unsafe { GetTextExtentPoint32W(hdc, text.as_ptr(), text.len() as i32, &mut size) };
    size.cx
}

/// Workspace buttons from the left edge; `hdc` must have the bar font selected.
unsafe fn layout(hdc: HDC, hwnd: HWND) -> Vec<(RECT, Workspace)> {
    let dpi = GetDpiForWindow(hwnd);
    let mut tm: TEXTMETRICW = zeroed();
    GetTextMetricsW(hdc, &mut tm);
    let height = tm.tmHeight + 2 * s(WS_PAD_Y, dpi);
    let top = (s(HEIGHT, dpi) - height) / 2;
    let mut x = s(EDGE, dpi);
    workspaces(hwnd)
        .into_iter()
        .map(|ws| {
            let w = text_width(hdc, &ws.label.encode_utf16().collect::<Vec<_>>()) + 2 * s(WS_PAD_X, dpi);
            let r = RECT { left: x, top, right: x + w, bottom: top + height };
            x += w + s(WS_GAP, dpi);
            (r, ws)
        })
        .collect()
}

/// The workspace button under client point (`x`, `y`), if any.
unsafe fn workspace_at(hwnd: HWND, x: i32, y: i32) -> Option<Workspace> {
    let hdc = GetDC(hwnd);
    let font = SelectObject(hdc, create_font(GetDpiForWindow(hwnd)));
    let hit = layout(hdc, hwnd)
        .into_iter()
        .find(|(r, _)| x >= r.left && x < r.right && y >= r.top && y < r.bottom);
    DeleteObject(SelectObject(hdc, font));
    ReleaseDC(hwnd, hdc);
    hit.map(|(_, ws)| ws)
}

unsafe fn fill(hdc: HDC, r: &RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, r, brush);
    DeleteObject(brush);
}

unsafe fn draw_text(hdc: HDC, text: &str, r: &mut RECT, color: COLORREF, align: u32) {
    let text: Vec<u16> = text.encode_utf16().collect();
    SetTextColor(hdc, color);
    DrawTextW(hdc, text.as_ptr(), text.len() as i32, r, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_NOCLIP | align);
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let dpi = GetDpiForWindow(hwnd);

    // Draw into a bitmap first so updates do not flicker.
    let mem = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
    let old_bmp = SelectObject(mem, bmp);
    let old_font = SelectObject(mem, create_font(dpi));
    SetBkMode(mem, TRANSPARENT as _);

    fill(mem, &rc, BG);
    fill(mem, &RECT { top: rc.bottom - 1, ..rc }, BORDER);

    for (mut r, ws) in layout(mem, hwnd) {
        let color = if ws.focused {
            WS_FOCUSED
        } else if ws.displayed {
            WS_DISPLAYED
        } else {
            WS_BG
        };
        fill(mem, &r, color);
        draw_text(mem, &ws.label, &mut r, TEXT, DT_CENTER);
    }

    INFO.with_borrow(|info| {
        draw_text(mem, &info.date, &mut rc.clone(), TEXT, DT_CENTER);

        // Label/value pairs, laid out from the right edge leftwards.
        let mut items = Vec::new();
        if let Some(wifi) = &info.wifi {
            items.push(("wifi", wifi.clone().unwrap_or_else(|| "off".into())));
        }
        items.push((NAMES.cpu.as_str(), format!("{}%", info.cpu)));
        items.push(("mem", format!("{}%", info.mem)));
        if let Some(gpu) = info.gpu {
            items.push((NAMES.gpu.as_str(), format!("{gpu}%")));
        }
        if let Some((percent, charging)) = info.battery {
            items.push(("bat", format!("{percent}%{}", if charging { "+" } else { "" })));
        }
        let space = text_width(mem, &[b' ' as u16]);
        let mut x = rc.right - s(EDGE, dpi);
        for (label, value) in items.iter().rev() {
            for (text, color) in [(value.as_str(), TEXT), (*label, SUBTEXT)] {
                let w = text_width(mem, &text.encode_utf16().collect::<Vec<_>>());
                draw_text(mem, text, &mut RECT { left: x - w, right: x, ..rc }, color, DT_LEFT);
                x -= w + space;
            }
            x -= s(ITEM_GAP, dpi) - space;
        }
    });

    BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
    DeleteObject(SelectObject(mem, old_font));
    SelectObject(mem, old_bmp);
    DeleteObject(bmp);
    DeleteDC(mem);
    EndPaint(hwnd, &ps);
}

unsafe fn menu() {
    let hwnd = CONTROL.get();
    let menu = CreatePopupMenu();
    let startup_flag = if startup_enabled() { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(menu, MF_STRING | startup_flag, CMD_STARTUP, wide("Start with Windows").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, CMD_EXIT, wide("Exit").as_ptr());

    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null());
    DestroyMenu(menu);

    match cmd as usize {
        CMD_STARTUP => set_startup(!startup_enabled()),
        CMD_EXIT => PostQuitMessage(0),
        _ => {}
    }
}

fn startup_enabled() -> bool {
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide(RUN_VALUE).as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            null_mut(),
        ) == ERROR_SUCCESS
    }
}

fn set_startup(enable: bool) {
    unsafe {
        if enable {
            let Ok(exe) = std::env::current_exe() else { return };
            let data = wide(&format!("\"{}\"", exe.display()));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                wide(RUN_KEY).as_ptr(),
                wide(RUN_VALUE).as_ptr(),
                REG_SZ,
                data.as_ptr() as *const c_void,
                (data.len() * 2) as u32,
            );
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_VALUE).as_ptr());
        }
    }
}
