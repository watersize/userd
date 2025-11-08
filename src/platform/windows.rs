#![cfg(target_os = "windows")]
#![allow(non_snake_case, non_camel_case_types, dead_code, unused_unsafe, unused_variables)]
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock, mpsc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use std::os::raw::{c_int, c_void};

type HWND = *mut c_void;
type HINSTANCE = *mut c_void;
type HDC = *mut c_void;
type HBRUSH = *mut c_void;
type HMODULE = *mut c_void;
type LPARAM = isize;
type WPARAM = usize;
type LRESULT = isize;
type UINT = u32;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
const CW_USEDEFAULT: i32 = 0x80000000u32 as i32;
const SW_SHOW: i32 = 5;
const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const GWLP_USERDATA: i32 = -21;

#[repr(C)]
struct WNDCLASSEXW {
    cbSize: u32,
    style: u32,
    lpfnWndProc: Option<extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT>,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: HINSTANCE,
    hIcon: *mut c_void,
    hCursor: *mut c_void,
    hbrBackground: HBRUSH,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
    hIconSm: *mut c_void,
}

#[repr(C)]
struct PAINTSTRUCT {
    hdc: HDC,
    fErase: i32,
    rcPaint: [i32;4],
    fRestore: i32,
    fIncUpdate: i32,
    rgbReserved: [u8;32],
}

#[repr(C)]
struct POINT { x: i32, y: i32 }

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: UINT,
    wParam: WPARAM,
    lParam: LPARAM,
    time: u32,
    pt: POINT,
}

#[repr(C)]
struct BITMAPINFOHEADER {
    biSize: u32,
    biWidth: i32,
    biHeight: i32,
    biPlanes: u16,
    biBitCount: u16,
    biCompression: u32,
    biSizeImage: u32,
    biXPelsPerMeter: i32,
    biYPelsPerMeter: i32,
    biClrUsed: u32,
    biClrImportant: u32,
}

#[repr(C)]
struct BITMAPINFO {
    bmiHeader: BITMAPINFOHEADER,
    bmiColors: [u8;4],
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(dwExStyle: u32, lpClassName: *const u16, lpWindowName: *const u16,
                       dwStyle: u32, x: i32, y: i32, nWidth: i32, nHeight: i32,
                       hWndParent: HWND, hMenu: *mut c_void, hInstance: HINSTANCE, lpParam: *mut c_void) -> HWND;
    fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> i32;
    fn UpdateWindow(hWnd: HWND) -> i32;
    fn DefWindowProcW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    fn DispatchMessageW(lpmsg: *const MSG) -> LRESULT;
    fn TranslateMessage(lpmsg: *const MSG) -> i32;
    fn GetMessageW(lpMsg: *mut MSG, hWnd: HWND, wMsgFilterMin: u32, wMsgFilterMax: u32) -> i32;
    fn PostQuitMessage(nExitCode: i32);
    fn InvalidateRect(hWnd: HWND, lpRect: *const c_void, bErase: i32) -> i32;
    fn BeginPaint(hWnd: HWND, lpPaint: *mut PAINTSTRUCT) -> HDC;
    fn EndPaint(hWnd: HWND, lpPaint: *mut PAINTSTRUCT) -> i32;
    fn SetWindowLongPtrW(hWnd: HWND, nIndex: i32, dwNewLong: isize) -> isize;
    fn GetWindowLongPtrW(hWnd: HWND, nIndex: i32) -> isize;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn SetDIBitsToDevice(hdc: HDC, xDest: c_int, yDest: c_int, w: u32, h: u32,
                         xSrc: c_int, ySrc: c_int, StartScan: u32, cLines: u32,
                         lpvBits: *const c_void, lpbmi: *const BITMAPINFO, ColorUse: u32) -> c_int;
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

pub fn show_message(title: &str, text: &str) {
    // fallback to MessageBox via CreateWindow/MessageBoxW already existed earlier; keep simple
    println!("{}: {}", title, text);
}

pub enum WindowCommand {
    Blit(Vec<u8>, i32, i32), // buffer (RGBA32), w, h
    DrawRect(i32,i32,i32,i32,u8,u8,u8,u8), // x,y,w,h, r,g,b,a
    Clear(u8,u8,u8,u8), // r,g,b,a
    Present,
    DrawText(i32,i32,String), // x,y,text (very simple stub)
    Close,
}

type Sender = mpsc::Sender<WindowCommand>;

static REGISTRY: OnceLock<Mutex<HashMap<u64, Sender>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static EVENTS: OnceLock<Mutex<Vec<(u64, (i32,i32))>>> = OnceLock::new();
static HANDLERS: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();
static HWND_MAP: OnceLock<Mutex<HashMap<usize, u64>>> = OnceLock::new();
static WIDGETS: OnceLock<Mutex<HashMap<u64, Vec<Widget>>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<u64, Sender>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn events_registry() -> &'static Mutex<Vec<(u64, (i32,i32))>> {
    EVENTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn handlers_registry() -> &'static Mutex<HashMap<u64, String>> {
    HANDLERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hwnd_map() -> &'static Mutex<HashMap<usize, u64>> {
    HWND_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn push_event(win_id: u64, x: i32, y: i32) {
    let reg = events_registry();
    if let Ok(mut g) = reg.lock() { g.push((win_id, (x,y))); }
}

pub fn drain_events() -> Vec<(u64, (i32,i32))> {
    let reg = events_registry();
    if let Ok(mut g) = reg.lock() {
        let out = g.drain(..).collect();
        return out;
    }
    Vec::new()
}

pub fn register_handler(win_id: u64, handler: &str) {
    let reg = handlers_registry();
    if let Ok(mut g) = reg.lock() { g.insert(win_id, handler.to_string()); }
}

pub fn get_handler(win_id: u64) -> Option<String> {
    let reg = handlers_registry();
    if let Ok(g) = reg.lock() { g.get(&win_id).cloned() } else { None }
}

#[derive(Debug, Clone)]
pub struct Widget {
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub handler: String,
}

fn widgets_registry() -> &'static Mutex<HashMap<u64, Vec<Widget>>> {
    WIDGETS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a rectangular widget on a window. Returns widget id.
pub fn register_widget(win_id: u64, x: i32, y: i32, w: i32, h: i32, handler: &str) -> u64 {
    let wid = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let widget = Widget { id: wid, x, y, w, h, handler: handler.to_string() };
    if let Ok(mut reg) = widgets_registry().lock() {
        reg.entry(win_id).or_insert_with(Vec::new).push(widget);
    }
    wid
}

/// Register a widget using a simple vertical stacking layout (auto X/Y) based on existing widgets.
pub fn register_widget_auto(win_id: u64, _label: &str, handler: &str) -> u64 {
    // compute y as 10 + n*30
    let mut y = 10i32;
    let x = 10i32;
    let w = 120i32;
    let h = 28i32;
    if let Ok(reg) = widgets_registry().lock() {
        if let Some(list) = reg.get(&win_id) {
            y = 10 + (list.len() as i32) * 34;
        }
    }
    register_widget(win_id, x, y, w, h, handler)
}

fn find_widget_hit(win_id: u64, px: i32, py: i32) -> Option<Widget> {
    if let Ok(reg) = widgets_registry().lock() {
        if let Some(list) = reg.get(&win_id) {
            for w in list.iter() {
                if px >= w.x && px < w.x + w.w && py >= w.y && py < w.y + w.h {
                    return Some(w.clone());
                }
            }
        }
    }
    None
}

/// Create a window and a worker thread which owns it. The worker listens for Blit commands and
/// on WM_PAINT uses SetDIBitsToDevice to draw the provided RGBA32 buffer (top-down).
pub fn create_window(title: &str, w: i32, h: i32) -> u64 {
    let (tx, rx) = mpsc::channel::<WindowCommand>();
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    registry().lock().unwrap().insert(id, tx.clone());
    let title = title.to_string();
    std::thread::spawn(move || {
        extern "system" fn wndproc(hWnd: HWND, msg: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
            if msg == WM_PAINT {
                // retrieve pointer to buffer holder and perform paint
                unsafe {
                    let mut ps: PAINTSTRUCT = std::mem::zeroed();
                    let hdc = BeginPaint(hWnd, &mut ps as *mut _);
                    // painting performed in worker loop which sets buffer; here we simply end paint
                    let _ = hdc;
                    EndPaint(hWnd, &mut ps as *mut _);
                }
                return 0;
            } else if msg == WM_DESTROY {
                unsafe { PostQuitMessage(0); }
                return 0;
            }
            // handle mouse click
            if msg == 0x0201 /* WM_LBUTTONDOWN */ {
                // extract x,y from l_param
                let lx = (l_param & 0xFFFF) as i16 as i32;
                let ly = ((l_param >> 16) & 0xFFFF) as i16 as i32;
                // find window id from hwnd map
                let mut win_id_opt: Option<u64> = None;
                if let Ok(map) = hwnd_map().lock() {
                    if let Some(id) = map.get(&(hWnd as usize)) { win_id_opt = Some(*id); }
                }
                if let Some(win_id) = win_id_opt {
                    // find widget hit
                    if let Some(widget) = find_widget_hit(win_id, lx, ly) {
                        // push event by handler name
                        let handler = widget.handler.clone();
                        let reg = events_registry();
                        if let Ok(mut g) = reg.lock() {
                            // reuse events vector for (win_id, (x,y)) but we'll push in handlers form by encoding handler name into HANDLERS map? simpler: store handler mapping in EVENTS as u64->ignored, but to avoid changing many parts, push as before and handlers_lookup will be used.
                            // We'll push as a special negative id mapping by storing win_id as widget id in first field and use handlers registry to map widget id to name.
                            g.push((widget.id, (lx, ly)));
                        }
                        // also save handler name for widget id
                        if let Ok(mut wmap) = handlers_registry().lock() {
                            wmap.insert(widget.id, handler);
                        }
                    } else {
                        // no widget hit: push window-level event
                        push_event(win_id, lx, ly);
                    }
                }
            }
            unsafe { DefWindowProcW(hWnd, msg, w_param, l_param) }
        }

        let class_name = format!("userd_window_{}", id);
        let wname = to_wide(&class_name);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: null_mut(),
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: wname.as_ptr(),
            hIconSm: null_mut(),
        };
            unsafe {
                RegisterClassExW(&wc as *const _);
                let wnd_title = to_wide(&title);
                let hwnd = CreateWindowExW(0, wname.as_ptr(), wnd_title.as_ptr(), WS_OVERLAPPEDWINDOW,
                                           CW_USEDEFAULT, CW_USEDEFAULT, w, h,
                                           null_mut(), null_mut(), null_mut(), null_mut());
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_SHOW);
                UpdateWindow(hwnd);
            }

            // Shared persistent buffer: allocate full RGBA buffer for window size and store in GWLP_USERDATA
            let bufsize = (w as usize).saturating_mul(h as usize).saturating_mul(4);
            let buffer_holder: Box<Mutex<Vec<u8>>> = Box::new(Mutex::new(vec![0u8; bufsize]));
            let bh_ptr = Box::into_raw(buffer_holder) as isize;
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, bh_ptr); }
            // store hwnd -> id mapping for event lookup
            if let Ok(mut map) = hwnd_map().lock() {
                map.insert(hwnd as usize, id);
            }

            // spawn a small loop that receives commands (blit/drawrect) and triggers InvalidateRect
            let rx_local = rx;
            let hwnd_local = hwnd as usize;
            std::thread::spawn(move || {
                let canvas_w = w as usize;
                let canvas_h = h as usize;
                for cmd in rx_local {
                    match cmd {
                        WindowCommand::Blit(buf, bw, bh) => {
                            // replace buffer contents (if sizes match) or resize
                            unsafe {
                                let bh_ptr = GetWindowLongPtrW(hwnd_local as HWND, GWLP_USERDATA) as *mut Mutex<Vec<u8>>;
                                if !bh_ptr.is_null() {
                                    if let Ok(mut guard) = (*bh_ptr).lock() {
                                        let expected = (bw as usize).saturating_mul(bh as usize).saturating_mul(4);
                                        if guard.len() == expected {
                                            guard.copy_from_slice(&buf[..expected.min(buf.len())]);
                                        } else {
                                            *guard = vec![0u8; expected];
                                            let copy_len = expected.min(buf.len());
                                            guard[..copy_len].copy_from_slice(&buf[..copy_len]);
                                        }
                                    }
                                }
                                // request paint
                                InvalidateRect(hwnd_local as HWND, null(), 1);
                            }
                        }
                        WindowCommand::Clear(rr,gg,bb,aa) => {
                            unsafe {
                                let bh_ptr = GetWindowLongPtrW(hwnd_local as HWND, GWLP_USERDATA) as *mut Mutex<Vec<u8>>;
                                if !bh_ptr.is_null() {
                                    if let Ok(mut guard) = (*bh_ptr).lock() {
                                        for i in (0..guard.len()).step_by(4) {
                                            guard[i+0] = rr;
                                            guard[i+1] = gg;
                                            guard[i+2] = bb;
                                            guard[i+3] = aa;
                                        }
                                    }
                                }
                                InvalidateRect(hwnd_local as HWND, null(), 1);
                            }
                        }
                        WindowCommand::Present => {
                            // just request repaint (buffer already stored)
                            unsafe { InvalidateRect(hwnd_local as HWND, null(), 1); }
                        }
                        WindowCommand::DrawText(x,y,txt) => {
                            // very small placeholder: draw a simple colored rectangle behind where text would be
                            unsafe {
                                let bh_ptr = GetWindowLongPtrW(hwnd_local as HWND, GWLP_USERDATA) as *mut Mutex<Vec<u8>>;
                                if !bh_ptr.is_null() {
                                    if let Ok(mut guard) = (*bh_ptr).lock() {
                                        let tw = 8usize * txt.len();
                                        let th = 12usize;
                                        let cx = x.max(0) as usize;
                                        let cy = y.max(0) as usize;
                                        for py in cy..(cy+th).min(canvas_h) {
                                            for px in cx..(cx+tw).min(canvas_w) {
                                                let idx = (py * canvas_w + px) * 4;
                                                if idx + 3 < guard.len() {
                                                    // background: dark gray
                                                    guard[idx+0] = 60;
                                                    guard[idx+1] = 60;
                                                    guard[idx+2] = 60;
                                                    guard[idx+3] = 255;
                                                }
                                            }
                                        }
                                    }
                                }
                                InvalidateRect(hwnd_local as HWND, null(), 1);
                            }
                        }
                        WindowCommand::DrawRect(x,y,ww,hh,rr,gg,bb,aa) => {
                            unsafe {
                                let bh_ptr = GetWindowLongPtrW(hwnd_local as HWND, GWLP_USERDATA) as *mut Mutex<Vec<u8>>;
                                if !bh_ptr.is_null() {
                                    if let Ok(mut guard) = (*bh_ptr).lock() {
                                        if guard.len() < canvas_w.saturating_mul(canvas_h).saturating_mul(4) { /* skip if buffer unexpected */ }
                                        // clamp coordinates
                                        let rx0 = x.max(0) as usize;
                                        let ry0 = y.max(0) as usize;
                                        let rx1 = (x + ww).min(canvas_w as i32) as usize;
                                        let ry1 = (y + hh).min(canvas_h as i32) as usize;
                                        for py in ry0..ry1 {
                                            for px in rx0..rx1 {
                                                let idx = (py * canvas_w + px) * 4;
                                                if idx + 3 < guard.len() {
                                                    guard[idx + 0] = rr;
                                                    guard[idx + 1] = gg;
                                                    guard[idx + 2] = bb;
                                                    guard[idx + 3] = aa;
                                                }
                                            }
                                        }
                                    }
                                }
                                InvalidateRect(hwnd_local as HWND, null(), 1);
                            }
                        }
                        WindowCommand::Close => {
                            unsafe { PostQuitMessage(0); }
                            break;
                        }
                    }
                }
                // cleanup box
                unsafe {
                    let bh_ptr = GetWindowLongPtrW(hwnd_local as HWND, GWLP_USERDATA) as *mut Mutex<Vec<u8>>;
                    if !bh_ptr.is_null() {
                        let _ = Box::from_raw(bh_ptr);
                    }
                }
            });

            // Message loop
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            loop {
                // Use GetMessageW to block until messages
                let ret = unsafe { GetMessageW(&mut msg as *mut MSG, null_mut(), 0, 0) };
                if ret <= 0 { break; }
                unsafe { TranslateMessage(&msg as *const MSG); }
                unsafe { DispatchMessageW(&msg as *const MSG); }

                // On each loop try to paint if buffer exists
                unsafe {
                    let bh_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Mutex<Option<(Vec<u8>, i32, i32, usize)>>;
                    if !bh_ptr.is_null() {
                        if let Ok(mut guard) = (*bh_ptr).lock() {
                                if let Some((ref buf, bw, bh, _)) = *guard {
                                // perform SetDIBitsToDevice
                                let bmi = BITMAPINFO {
                                    bmiHeader: BITMAPINFOHEADER {
                                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                                        biWidth: bw,
                                        biHeight: -bh, // top-down
                                        biPlanes: 1,
                                        biBitCount: 32,
                                        biCompression: 0, // BI_RGB
                                        biSizeImage: 0,
                                        biXPelsPerMeter: 0,
                                        biYPelsPerMeter: 0,
                                        biClrUsed: 0,
                                        biClrImportant: 0,
                                    },
                                    bmiColors: [0,0,0,0],
                                };
                                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                                let hdc = BeginPaint(hwnd as HWND, &mut ps as *mut _);
                                let _ = SetDIBitsToDevice(hdc, 0, 0, bw as u32, bh as u32, 0, 0, 0, bh as u32,
                                                         buf.as_ptr() as *const c_void, &bmi as *const _, 0);
                                EndPaint(hwnd as HWND, &mut ps as *mut _);
                                // clear buffer after paint
                                *guard = None;
                            }
                        }
                    }
                }
            }
        }
    });
    id
}

pub fn blit_window(id: u64, buf: Vec<u8>, w: i32, h: i32) -> Result<(), String> {
    let reg = registry();
    let guard = reg.lock().map_err(|_| "registry lock poisoned".to_string())?;
    if let Some(tx) = guard.get(&id) {
        tx.send(WindowCommand::Blit(buf, w, h)).map_err(|e| e.to_string())
    } else {
        Err("window id not found".to_string())
    }
}

/// Draw a rectangle directly into the window's persistent canvas.
/// This enqueues a `DrawRect` command to the window thread which will update the buffer
/// and invalidate the window for repaint.
pub fn canvas_draw_rect(id: u64, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8, a: u8) -> Result<(), String> {
    let reg = registry();
    let guard = reg.lock().map_err(|_| "registry lock poisoned".to_string())?;
    if let Some(tx) = guard.get(&id) {
        tx.send(WindowCommand::DrawRect(x,y,w,h,r,g,b,a)).map_err(|e| e.to_string())
    } else {
        Err("window id not found".to_string())
    }
}

pub fn canvas_clear(id: u64, r: u8, g: u8, b: u8, a: u8) -> Result<(), String> {
    let reg = registry();
    let guard = reg.lock().map_err(|_| "registry lock poisoned".to_string())?;
    if let Some(tx) = guard.get(&id) {
        tx.send(WindowCommand::Clear(r,g,b,a)).map_err(|e| e.to_string())
    } else { Err("window id not found".to_string()) }
}

pub fn canvas_present(id: u64) -> Result<(), String> {
    let reg = registry();
    let guard = reg.lock().map_err(|_| "registry lock poisoned".to_string())?;
    if let Some(tx) = guard.get(&id) {
        tx.send(WindowCommand::Present).map_err(|e| e.to_string())
    } else { Err("window id not found".to_string()) }
}

pub fn canvas_draw_text(id: u64, x: i32, y: i32, text: &str) -> Result<(), String> {
    let reg = registry();
    let guard = reg.lock().map_err(|_| "registry lock poisoned".to_string())?;
    if let Some(tx) = guard.get(&id) {
        tx.send(WindowCommand::DrawText(x,y,text.to_string())).map_err(|e| e.to_string())
    } else { Err("window id not found".to_string()) }
}

pub fn close_window(id: u64) {
    if let Ok(mut guard) = registry().lock() {
        if let Some(tx) = guard.get(&id) {
            let _ = tx.send(WindowCommand::Close);
        }
        guard.remove(&id);
    }
}

pub fn has_windows() -> bool {
    if let Ok(g) = registry().lock() {
        !g.is_empty()
    } else { false }
}
