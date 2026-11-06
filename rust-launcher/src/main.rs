#![windows_subsystem = "windows"]

//! A lightweight Alt+Space program launcher: a borderless Win32 popup with a
//! search box and an owner-drawn result list, plus a tray icon.

mod files;
mod history;
mod index;
mod matcher;

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU64, Ordering};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, EM_SETSEL, MEASUREITEMSTRUCT, ODS_SELECTED};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_SYSTEM_AWARE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use files::Hit;
use history::History;
use index::{Item, Target};

const HOTKEY_ID: i32 = 1;
const WM_TRAY: u32 = WM_APP + 1;
const WM_INDEXED: u32 = WM_APP + 2;
const WM_FILES: u32 = WM_APP + 3;

const TIMER_FILES: usize = 1;
/// Typing pause before a file search starts.
const FILE_DELAY_MS: u32 = 150;

const CMD_STARTUP: usize = 1;
const CMD_REINDEX: usize = 2;
const CMD_EXIT: usize = 3;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "RustLauncher";

// Layout in 96-DPI pixels; scaled with `s()`.
const WIDTH: i32 = 640;
const EDIT_BAND: i32 = 56;
const ROW: i32 = 48;
const PAD: i32 = 8;
const MAX_ROWS: usize = 8;

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}
const BG: COLORREF = rgb(32, 32, 32);
const SEL: COLORREF = rgb(58, 58, 66);
const TEXT: COLORREF = rgb(240, 240, 240);
const SUBTEXT: COLORREF = rgb(150, 150, 150);

#[derive(Clone, Copy)]
struct Ui {
    hwnd: HWND,
    edit: HWND,
    list: HWND,
    font_big: HFONT,
    font_small: HFONT,
    bg: HBRUSH,
    sel: HBRUSH,
}

struct State {
    shortcuts: Vec<Item>,
    apps: Vec<Item>,
    /// Files and folders from the launch history.
    recent: Vec<Item>,
    items: Vec<Item>,
    /// File hits of the latest search (generation `files_gen`).
    files: Vec<Hit>,
    files_gen: u64,
    /// Programs and files in display order.
    results: Vec<Item>,
    history: History,
    icons: HashMap<String, HICON>,
}

/// Results of a background index scan, sent to the UI thread.
struct Indexed {
    slot: Slot,
    items: Vec<Item>,
}

#[derive(Clone, Copy)]
enum Slot {
    Shortcuts,
    Apps,
    Recent,
}

/// A batch of file hits from the search with generation `gen`.
struct FileBatch {
    gen: u64,
    hits: Vec<Hit>,
}

/// Bumped on every query change; a file search stops once it is outdated.
static SEARCH_GEN: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
enum Mode {
    Normal,
    Admin,
    OpenFolder,
}

thread_local! {
    static UI: OnceCell<Ui> = const { OnceCell::new() };
    static STATE: RefCell<State> = RefCell::new(State {
        shortcuts: Vec::new(),
        apps: Vec::new(),
        recent: Vec::new(),
        items: Vec::new(),
        files: Vec::new(),
        files_gen: 0,
        results: Vec::new(),
        history: History::empty(),
        icons: HashMap::new(),
    });
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}

fn try_ui() -> Option<Ui> {
    UI.with(|u| u.get().copied())
}

fn ui() -> Ui {
    try_ui().expect("UI not initialized")
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Scales a 96-DPI length to the system DPI.
fn s(v: i32) -> i32 {
    unsafe { v * GetDpiForSystem() as i32 / 96 }
}

fn main() {
    unsafe {
        CreateMutexW(null(), 0, wide("rust-launcher-single-instance").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        CoInitializeEx(null(), COINIT_APARTMENTTHREADED as _);

        let ui = create_ui();
        UI.with(|u| u.set(ui).ok());
        STATE.with_borrow_mut(|st| st.history = History::load());
        TASKBAR_CREATED.set(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()));

        if RegisterHotKey(ui.hwnd, HOTKEY_ID, MOD_ALT | MOD_NOREPEAT, VK_SPACE as u32) == 0 {
            MessageBoxW(
                null_mut(),
                wide("Alt+Space is already used by another program (e.g. Flow Launcher or PowerToys Run).").as_ptr(),
                wide("Rust Launcher").as_ptr(),
                MB_ICONERROR,
            );
            return;
        }
        add_tray(ui.hwnd);
        start_index(ui.hwnd, true);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if handle_key(&msg) {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.hwnd == ui.list && msg.message == WM_LBUTTONUP {
                launch(Mode::Normal);
            }
        }
    }
}

unsafe fn create_ui() -> Ui {
    let hinst = GetModuleHandleW(null());
    let class = wide("RustLauncher");
    let bg = CreateSolidBrush(BG);
    let wc = WNDCLASSW {
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: app_icon(GetSystemMetrics(SM_CXICON)),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: bg,
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    RegisterClassW(&wc);

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide("Rust Launcher").as_ptr(),
        WS_POPUP,
        0,
        0,
        s(WIDTH),
        s(EDIT_BAND),
        null_mut(),
        null_mut(),
        hinst,
        null(),
    );
    let corner = DWMWCP_ROUND;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE as _,
        &corner as *const _ as *const c_void,
        size_of::<i32>() as u32,
    );

    let edit = CreateWindowExW(
        0,
        wide("EDIT").as_ptr(),
        null(),
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
        s(16),
        s(12),
        s(WIDTH - 32),
        s(32),
        hwnd,
        null_mut(),
        hinst,
        null(),
    );
    let list = CreateWindowExW(
        0,
        wide("LISTBOX").as_ptr(),
        null(),
        WS_CHILD | WS_VISIBLE | (LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_NODATA | LBS_NOINTEGRALHEIGHT) as u32,
        s(PAD),
        s(EDIT_BAND),
        s(WIDTH - 2 * PAD),
        0,
        hwnd,
        null_mut(),
        hinst,
        null(),
    );

    let font = |px: i32| {
        CreateFontW(
            -s(px), 0, 0, 0, FW_NORMAL as _, 0, 0, 0, DEFAULT_CHARSET as _, OUT_DEFAULT_PRECIS as _,
            CLIP_DEFAULT_PRECIS as _, CLEARTYPE_QUALITY as _, (DEFAULT_PITCH | FF_DONTCARE) as _,
            wide("Segoe UI").as_ptr(),
        )
    };
    let font_edit = font(24);
    SendMessageW(edit, WM_SETFONT, font_edit as usize, 1);

    Ui {
        hwnd,
        edit,
        list,
        font_big: font(17),
        font_small: font(12),
        bg,
        sel: CreateSolidBrush(SEL),
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(ui) = try_ui() else {
        if msg == WM_MEASUREITEM {
            (*(lparam as *mut MEASUREITEMSTRUCT)).itemHeight = s(ROW) as u32;
            return 1;
        }
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    match msg {
        WM_HOTKEY => {
            if IsWindowVisible(hwnd) != 0 && GetForegroundWindow() == hwnd {
                hide();
            } else {
                show();
            }
            0
        }
        WM_ACTIVATE => {
            if wparam & 0xFFFF == WA_INACTIVE as usize {
                hide();
            }
            0
        }
        WM_CLOSE => {
            hide();
            0
        }
        WM_COMMAND => {
            if (wparam >> 16) as u32 == EN_CHANGE && lparam as HWND == ui.edit {
                on_query_changed();
            }
            0
        }
        WM_TIMER if wparam == TIMER_FILES => {
            KillTimer(hwnd, TIMER_FILES);
            start_file_search(hwnd, get_text(ui.edit));
            0
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let hdc = wparam as HDC;
            SetTextColor(hdc, TEXT);
            SetBkColor(hdc, BG);
            ui.bg as LRESULT
        }
        WM_DRAWITEM => {
            draw_item(&ui, &*(lparam as *const DRAWITEMSTRUCT));
            1
        }
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP => show(),
                WM_RBUTTONUP => tray_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_INDEXED => {
            on_indexed(*Box::from_raw(lparam as *mut Indexed));
            0
        }
        WM_FILES => {
            on_files(*Box::from_raw(lparam as *mut FileBatch));
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ if msg == TASKBAR_CREATED.get() => {
            add_tray(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Keyboard handling for the search box and list. Returns true if consumed.
unsafe fn handle_key(msg: &MSG) -> bool {
    let ui = ui();
    if msg.message != WM_KEYDOWN || (msg.hwnd != ui.edit && msg.hwnd != ui.list) {
        return false;
    }
    let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
    let shift = GetKeyState(VK_SHIFT as i32) < 0;
    match msg.wParam as u16 {
        VK_ESCAPE => hide(),
        VK_RETURN if ctrl => launch(Mode::Admin),
        VK_RETURN if shift => launch(Mode::OpenFolder),
        VK_RETURN => launch(Mode::Normal),
        VK_DOWN => move_selection(1),
        VK_UP => move_selection(-1),
        VK_TAB => move_selection(if shift { -1 } else { 1 }),
        VK_NEXT => move_selection(MAX_ROWS as isize),
        VK_PRIOR => move_selection(-(MAX_ROWS as isize)),
        VK_BACK if ctrl => delete_word(),
        _ => return false,
    }
    true
}

fn get_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

unsafe fn delete_word() {
    let edit = ui().edit;
    let text = get_text(edit);
    let trimmed = text.trim_end();
    let cut = trimmed
        .char_indices()
        .rev()
        .find(|&(_, c)| c.is_whitespace() || "\\/.-_".contains(c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    SetWindowTextW(edit, wide(&trimmed[..cut]).as_ptr());
    SendMessageW(edit, EM_SETSEL, cut, cut as isize);
}

unsafe fn move_selection(delta: isize) {
    let list = ui().list;
    let count = SendMessageW(list, LB_GETCOUNT, 0, 0);
    if count <= 0 {
        return;
    }
    let cur = SendMessageW(list, LB_GETCURSEL, 0, 0).max(0);
    let next = if delta.abs() == 1 {
        (cur + delta).rem_euclid(count) // arrows wrap around
    } else {
        (cur + delta).clamp(0, count - 1)
    };
    SendMessageW(list, LB_SETCURSEL, next as usize, 0);
}

/// Cancels the running file search and schedules a new one once typing
/// pauses, then re-ranks right away with the hits found so far.
fn on_query_changed() {
    let ui = ui();
    SEARCH_GEN.fetch_add(1, Ordering::SeqCst);
    unsafe { KillTimer(ui.hwnd, TIMER_FILES) };
    if get_text(ui.edit).trim().chars().count() >= files::MIN_QUERY {
        unsafe { SetTimer(ui.hwnd, TIMER_FILES, FILE_DELAY_MS, None) };
    } else {
        STATE.with_borrow_mut(|st| st.files.clear());
    }
    refilter(false);
}

/// Re-ranks results for the current query and resizes the window to fit.
/// With `keep_selection`, an item the user moved to stays selected.
fn refilter(keep_selection: bool) {
    let ui = ui();
    let query = get_text(ui.edit);
    let cur = if keep_selection { unsafe { SendMessageW(ui.list, LB_GETCURSEL, 0, 0) } } else { 0 };
    let (n, sel) = STATE.with_borrow_mut(|st| {
        let prev = usize::try_from(cur).ok().filter(|&i| i > 0).and_then(|i| st.results.get(i)).map(|i| i.key.clone());
        st.results = matcher::rank(&st.items, &st.files, &query, &st.history, history::now())
            .into_iter()
            .cloned()
            .collect();
        let sel = prev.and_then(|key| st.results.iter().position(|i| i.key == key)).unwrap_or(0);
        (st.results.len(), sel)
    });
    unsafe {
        SendMessageW(ui.list, LB_SETCOUNT, n, 0);
        if n > 0 {
            SendMessageW(ui.list, LB_SETCURSEL, sel, 0);
        }
        InvalidateRect(ui.list, null(), 1);
        let rows = n.min(MAX_ROWS) as i32;
        let list_h = rows * s(ROW);
        let height = s(EDIT_BAND) + if rows > 0 { list_h + s(PAD) } else { 0 };
        SetWindowPos(ui.hwnd, null_mut(), 0, 0, s(WIDTH), height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        MoveWindow(ui.list, s(PAD), s(EDIT_BAND), s(WIDTH - 2 * PAD), list_h, 1);
    }
}

unsafe fn show() {
    let ui = ui();
    // Center horizontally on the monitor under the mouse, a quarter down.
    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);
    let mut mi: MONITORINFO = zeroed();
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST), &mut mi);
    let wa = mi.rcWork;
    let x = wa.left + (wa.right - wa.left - s(WIDTH)) / 2;
    let y = wa.top + (wa.bottom - wa.top) / 4;
    SetWindowPos(ui.hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE);

    SetWindowTextW(ui.edit, wide("").as_ptr());
    refilter(false);
    ShowWindow(ui.hwnd, SW_SHOW);
    SetForegroundWindow(ui.hwnd);
    SetFocus(ui.edit);
    // Pick up newly installed shortcuts in the background.
    start_index(ui.hwnd, false);
}

unsafe fn hide() {
    let hwnd = ui().hwnd;
    ShowWindow(hwnd, SW_HIDE);
    SEARCH_GEN.fetch_add(1, Ordering::SeqCst);
    KillTimer(hwnd, TIMER_FILES);
    STATE.with_borrow_mut(|st| st.files.clear());
}

unsafe fn launch(mode: Mode) {
    let ui = ui();
    let sel = SendMessageW(ui.list, LB_GETCURSEL, 0, 0);
    let target = STATE.with_borrow_mut(|st| {
        let item = st.results.get(usize::try_from(sel).ok()?)?.clone();
        if !matches!(mode, Mode::OpenFolder) {
            st.history.record(&item.key, history::now());
            st.history.save();
        }
        Some(item.target)
    });
    match target {
        Some(target) => run_target(&target, mode),
        None => {
            let query = get_text(ui.edit);
            if query.trim().is_empty() {
                return;
            }
            run_command(query.trim(), mode);
        }
    }
    hide();
}

fn shell_execute(verb: Option<&str>, file: &str, params: Option<&str>) -> bool {
    let verb = verb.map(wide);
    let params = params.map(wide);
    let file = wide(file);
    unsafe {
        let r = ShellExecuteW(
            null_mut(),
            verb.as_ref().map_or(null(), |v| v.as_ptr()),
            file.as_ptr(),
            params.as_ref().map_or(null(), |p| p.as_ptr()),
            null(),
            SW_SHOWNORMAL,
        );
        r as isize > 32
    }
}

fn run_target(target: &Target, mode: Mode) {
    match (target, mode) {
        (Target::Shortcut(path), Mode::Normal) => shell_execute(None, path, None),
        (Target::Shortcut(path), Mode::Admin) => shell_execute(Some("runas"), path, None),
        (Target::Shortcut(path), Mode::OpenFolder) => {
            shell_execute(None, "explorer.exe", Some(&format!("/select,\"{path}\"")))
        }
        (Target::App(id), _) => shell_execute(None, "explorer.exe", Some(&format!(r"shell:AppsFolder\{id}"))),
    };
}

/// Fallback when nothing matches: run the query like the Win+R dialog.
fn run_command(query: &str, mode: Mode) {
    let verb = matches!(mode, Mode::Admin).then_some("runas");
    let (file, params) = match query.split_once(' ') {
        Some((f, p)) if !std::path::Path::new(query).exists() => (f, Some(p)),
        _ => (query, None),
    };
    shell_execute(verb, file, params);
}

/// Sends `payload` to the UI thread; the handler for `msg` takes ownership.
fn post<T>(hwnd: isize, msg: u32, payload: T) {
    unsafe {
        let p = Box::into_raw(Box::new(payload));
        if PostMessageW(hwnd as HWND, msg, 0, p as isize) == 0 {
            drop(Box::from_raw(p));
        }
    }
}

fn start_index(hwnd: HWND, with_apps: bool) {
    let hwnd = hwnd as isize;
    let keys = STATE.with_borrow(|st| st.history.path_keys());
    std::thread::spawn(move || {
        let shortcuts = index::scan_shortcuts();
        let recent = index::recent_files(&keys, &shortcuts);
        post(hwnd, WM_INDEXED, Indexed { slot: Slot::Shortcuts, items: shortcuts });
        post(hwnd, WM_INDEXED, Indexed { slot: Slot::Recent, items: recent });
        if with_apps {
            post(hwnd, WM_INDEXED, Indexed { slot: Slot::Apps, items: index::scan_apps() });
        }
    });
}

fn start_file_search(hwnd: HWND, query: String) {
    let hwnd = hwnd as isize;
    let gen = SEARCH_GEN.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        files::search(
            &query,
            || SEARCH_GEN.load(Ordering::Relaxed) == gen,
            |hits| post(hwnd, WM_FILES, FileBatch { gen, hits }),
        );
    });
}

/// The first batch of a search replaces the previous hits, later ones add.
fn on_files(batch: FileBatch) {
    if batch.gen != SEARCH_GEN.load(Ordering::SeqCst) {
        return;
    }
    let changed = STATE.with_borrow_mut(|st| {
        if st.files_gen != batch.gen {
            st.files_gen = batch.gen;
            st.files = batch.hits;
            return true;
        }
        if batch.hits.is_empty() {
            return false;
        }
        st.files.extend(batch.hits);
        true
    });
    if changed {
        refilter(true);
    }
}

fn on_indexed(msg: Indexed) {
    let changed = STATE.with_borrow_mut(|st| {
        let slot = match msg.slot {
            Slot::Shortcuts => &mut st.shortcuts,
            Slot::Apps => &mut st.apps,
            Slot::Recent => &mut st.recent,
        };
        let same = slot.len() == msg.items.len() && slot.iter().zip(&msg.items).all(|(a, b)| a.key == b.key);
        if same {
            return false;
        }
        *slot = msg.items;
        st.items = index::merge(&st.shortcuts, &st.apps, &st.recent);
        true
    });
    if changed {
        refilter(true);
    }
}

fn icon_for(icons: &mut HashMap<String, HICON>, item: &Item) -> HICON {
    *icons.entry(item.key.clone()).or_insert_with(|| unsafe {
        let mut sfi: SHFILEINFOW = zeroed();
        let flags = SHGFI_ICON | SHGFI_LARGEICON;
        match &item.target {
            Target::Shortcut(path) => {
                SHGetFileInfoW(wide(path).as_ptr(), 0, &mut sfi, size_of::<SHFILEINFOW>() as u32, flags);
            }
            Target::App(id) => {
                let mut pidl = null_mut();
                let name = wide(&format!(r"shell:AppsFolder\{id}"));
                if SHParseDisplayName(name.as_ptr(), null_mut(), &mut pidl, 0, null_mut()) >= 0 {
                    SHGetFileInfoW(pidl as _, 0, &mut sfi, size_of::<SHFILEINFOW>() as u32, flags | SHGFI_PIDL);
                    ILFree(pidl);
                }
            }
        }
        sfi.hIcon
    })
}

unsafe fn draw_item(ui: &Ui, dis: &DRAWITEMSTRUCT) {
    if dis.itemID == u32::MAX {
        return;
    }
    let Some((name, subtitle, icon)) = STATE.with(|st| {
        let mut st = st.try_borrow_mut().ok()?;
        let item = st.results.get(dis.itemID as usize)?.clone();
        let icon = icon_for(&mut st.icons, &item);
        Some((item.name.clone(), item.subtitle().to_string(), icon))
    }) else {
        return;
    };

    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let selected = dis.itemState & ODS_SELECTED != 0;
    FillRect(hdc, &rc, if selected { ui.sel } else { ui.bg });
    if !icon.is_null() {
        let size = s(32);
        DrawIconEx(hdc, rc.left + s(8), rc.top + (rc.bottom - rc.top - size) / 2, icon, size, size, 0, null_mut(), DI_NORMAL);
    }
    SetBkMode(hdc, TRANSPARENT as _);

    let text_left = rc.left + s(52);
    let mut name = wide(&name);
    SelectObject(hdc, ui.font_big);
    SetTextColor(hdc, TEXT);
    let mut r = RECT { left: text_left, top: rc.top + s(4), right: rc.right - s(8), bottom: rc.top + s(28) };
    DrawTextW(hdc, name.as_mut_ptr(), -1, &mut r, DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);

    let mut subtitle = wide(&subtitle);
    SelectObject(hdc, ui.font_small);
    SetTextColor(hdc, SUBTEXT);
    let mut r = RECT { left: text_left, top: rc.top + s(27), right: rc.right - s(8), bottom: rc.bottom - s(2) };
    DrawTextW(hdc, subtitle.as_mut_ptr(), -1, &mut r, DT_SINGLELINE | DT_PATH_ELLIPSIS | DT_NOPREFIX);
}

/// The arrow icon embedded by build.rs (resource id 1), at `size` pixels.
unsafe fn app_icon(size: i32) -> HICON {
    LoadImageW(GetModuleHandleW(null()), 1 as *const u16, IMAGE_ICON, size, size, LR_DEFAULTCOLOR) as HICON
}

unsafe fn add_tray(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = zeroed();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = app_icon(GetSystemMetrics(SM_CXSMICON));
    for (dst, src) in nid.szTip.iter_mut().zip("Rust Launcher (Alt+Space)".encode_utf16()) {
        *dst = src;
    }
    Shell_NotifyIconW(NIM_ADD, &nid);
}

unsafe fn remove_tray(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = zeroed();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

unsafe fn tray_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    let startup_flag = if startup_enabled() { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(menu, MF_STRING | startup_flag, CMD_STARTUP, wide("Start with Windows").as_ptr());
    AppendMenuW(menu, MF_STRING, CMD_REINDEX, wide("Reindex programs").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, CMD_EXIT, wide("Exit").as_ptr());

    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null());
    DestroyMenu(menu);

    match cmd as usize {
        CMD_STARTUP => set_startup(!startup_enabled()),
        CMD_REINDEX => start_index(hwnd, true),
        CMD_EXIT => {
            remove_tray(hwnd);
            DestroyWindow(hwnd);
        }
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
