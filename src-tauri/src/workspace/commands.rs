use std::sync::atomic::{AtomicU32, AtomicPtr, Ordering};
use std::sync::OnceLock;
use std::ffi::c_void;
use serde::Serialize;
use tauri::Emitter;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER};
use windows::core::{HSTRING, IUnknown, GUID};
use windows::Win32::Foundation::{HWND, WIN32_ERROR};
use windows::Win32::System::Registry::*;

static WS_FOREGROUND_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Save the foreground HWND before the bar takes focus.
pub fn set_foreground_hwnd(hwnd: *mut c_void) { WS_FOREGROUND_HWND.store(hwnd, Ordering::Relaxed); }
pub fn get_cached_foreground_hwnd() -> HWND { HWND(WS_FOREGROUND_HWND.load(Ordering::Relaxed)) }
fn skip_bar_hwnd(hwnd: HWND) -> bool {
    hwnd.is_invalid()
}

const CLSID_IMMERSIVE_SHELL: GUID = GUID {
    data1: 0xC2F03A33, data2: 0x21F5, data3: 0x47FA,
    data4: [0xB4, 0xBB, 0x15, 0x63, 0x62, 0xA2, 0xF2, 0x39],
};
const IID_ISERVICE_PROVIDER: GUID = GUID {
    data1: 0x6D5140C1, data2: 0x7436, data3: 0x11CE,
    data4: [0x80, 0x34, 0x00, 0xAA, 0x00, 0x60, 0x09, 0xFA],
};
const CLSID_VDM_INTERNAL: GUID = GUID {
    data1: 0xC5E0CDCA, data2: 0x7B6E, data3: 0x41B2,
    data4: [0x9F, 0xC4, 0xD9, 0x39, 0x75, 0xCC, 0x46, 0x7B],
};
const IID_VDM_INTERNAL: GUID = GUID {
    data1: 0x53F5CA0B, data2: 0x158F, data3: 0x4124,
    data4: [0x90, 0x0C, 0x05, 0x71, 0x58, 0x06, 0x0B, 0x27],
};
const IID_IVIRTUAL_DESKTOP: GUID = GUID {
    data1: 0x3F07F4BE, data2: 0xB107, data3: 0x441A,
    data4: [0xAF, 0x0F, 0x39, 0xD8, 0x25, 0x29, 0x07, 0x2C],
};
const IID_IAPPLICATION_VIEW_COLLECTION: GUID = GUID {
    data1: 0x1841C6D7, data2: 0x4F9D, data3: 0x42C0,
    data4: [0xAF, 0x41, 0x87, 0x47, 0x53, 0x8F, 0x10, 0xE5],
};
const CLSID_VIRTUAL_DESKTOP_PINNED_APPS: GUID = GUID {
    data1: 0xB1F5C0C7, data2: 0x9841, data3: 0x4FC0,
    data4: [0xA1, 0xF9, 0x14, 0xE2, 0x73, 0x75, 0x2E, 0xED],
};
const IID_VIRTUAL_DESKTOP_PINNED_APPS: GUID = GUID {
    data1: 0x4CE81583, data2: 0x1E4C, data3: 0x4CB6,
    data4: [0xA6, 0x30, 0x69, 0x0E, 0x5B, 0xC9, 0xB6, 0xC7],
};

type QIFn = unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> i32;
type ReleaseFn = unsafe extern "system" fn(*mut std::ffi::c_void) -> u32;
type QueryServiceFn = unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *const GUID, *mut *mut std::ffi::c_void) -> i32;
type GetCountFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut u32) -> i32;
type GetCurrentDesktopFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut *mut std::ffi::c_void) -> i32;
type GetDesktopsFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut *mut std::ffi::c_void) -> i32;
type SwitchDesktopFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;
type ArrGetAtFn = unsafe extern "system" fn(*mut std::ffi::c_void, u32, *const GUID, *mut *mut std::ffi::c_void) -> i32;
type DesktopGetIdFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut GUID) -> i32;
type DesktopGetNameFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut HSTRING) -> i32;
type GetViewForHwndFn = unsafe extern "system" fn(*mut std::ffi::c_void, HWND, *mut *mut std::ffi::c_void) -> i32;
type MoveViewFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;
type CreateDesktopFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut *mut std::ffi::c_void) -> i32;
type RemoveDesktopFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;
type DesktopRenameFn = unsafe extern "system" fn(*mut std::ffi::c_void, *const HSTRING) -> i32;
type IsWindowPinnedFn = unsafe extern "system" fn(*mut std::ffi::c_void, HWND, *mut i32) -> i32;
type PinUnpinHwndFn = unsafe extern "system" fn(*mut std::ffi::c_void, HWND) -> i32;

// --- Vtable layout detection ---
// Pre-24H2 (build < 26100, e.g. 22621):
//   3:GetCount 4:MoveViewToDesktop 5:CanViewMoveDesktop
//   6:GetCurrentDesktop 7:GetDesktops 8:GetAdjacentDesktop
//   9:SwitchDesktop 10:CreateDesktop 11:MoveDesktop 12:RemoveDesktop 13:FindDesktop
// 24H2+ (build >= 26100, e.g. 26100/26200): EnsureConnection inserted at 8, SwitchDesktopWithAnimation at 11
//   3:GetCount 4:MoveViewToDesktop 5:CanViewMoveDesktop
//   6:GetCurrentDesktop 7:GetDesktops 8:EnsureConnection
//   9:GetAdjacentDesktop 10:SwitchDesktop 11:SwitchDesktopWithAnimation
//   12:CreateDesktop 13:MoveDesktop 14:RemoveDesktop 15:FindDesktop

struct VtableLayout {
    switch_desktop: usize,
    create_desktop: usize,
    remove_desktop: usize,
}

fn detect_layout() -> VtableLayout {
    if is_24h2_or_later() {
        VtableLayout { switch_desktop: 10, create_desktop: 12, remove_desktop: 14 }
    } else {
        VtableLayout { switch_desktop: 9, create_desktop: 10, remove_desktop: 12 }
    }
}

fn layout() -> &'static VtableLayout {
    static LAYOUT: OnceLock<VtableLayout> = OnceLock::new();
    LAYOUT.get_or_init(detect_layout)
}

fn is_24h2_or_later() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        let key = windows::core::w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, key, None, KEY_READ, &mut hkey) != WIN32_ERROR(0) {
            return false;
        }
        let mut buf = [0u16; 10];
        let mut len = (buf.len() * 2) as u32;
        let result = RegQueryValueExW(hkey, windows::core::w!("CurrentBuild"), None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut len));
        let _ = RegCloseKey(hkey);
        if result != WIN32_ERROR(0) { return false; }
        let char_count = (len / 2) as usize;
        let s = String::from_utf16_lossy(&buf[..char_count]).trim_end_matches('\0').to_owned();
        let build: u32 = s.trim().parse().unwrap_or(0);
        eprintln!("[zenith:ws] Windows build {}", build);
        build >= 26100
    }
}

// --- COM helpers ---

unsafe fn get_vtable(ptr: *mut std::ffi::c_void) -> *mut *mut std::ffi::c_void {
    *(ptr as *mut *mut *mut std::ffi::c_void)
}

unsafe fn release_com(ptr: *mut std::ffi::c_void) {
    if ptr.is_null() { return; }
    let vtbl = get_vtable(ptr);
    let release: ReleaseFn = std::mem::transmute(*vtbl.add(2));
    release(ptr);
}

struct ManagerPtr(*mut std::ffi::c_void);
unsafe impl Send for ManagerPtr {}
unsafe impl Sync for ManagerPtr {}

fn cached_manager() -> Option<*mut std::ffi::c_void> {
    static CACHE: OnceLock<ManagerPtr> = OnceLock::new();
    if let Some(ptr) = CACHE.get() {
        return Some(ptr.0);
    }
    unsafe {
        let mgr = create_manager_internal()?;
        let _ = CACHE.set(ManagerPtr(mgr));
        Some(mgr)
    }
}

unsafe fn create_manager_internal() -> Option<*mut std::ffi::c_void> {
    let shell: IUnknown = match CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER) {
        Ok(s) => s,
        Err(e) => { eprintln!("[zenith:ws] CoCreateInstance FAILED: {e}"); return None; }
    };
    let shell_raw = std::mem::transmute::<IUnknown, *mut std::ffi::c_void>(shell);
    let provider = {
        let vtbl = get_vtable(shell_raw);
        let qi: QIFn = std::mem::transmute(*vtbl.add(0));
        let mut out: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = qi(shell_raw, &IID_ISERVICE_PROVIDER, &mut out);
        release_com(shell_raw);
        if hr < 0 || out.is_null() { eprintln!("[zenith:ws] QI IServiceProvider FAILED"); return None; }
        out
    };
    let service_vtbl = get_vtable(provider);
    let query_svc: QueryServiceFn = std::mem::transmute(*service_vtbl.add(3));
    let mut mgr: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = query_svc(provider, &CLSID_VDM_INTERNAL as *const _, &IID_VDM_INTERNAL as *const _, &mut mgr);
    release_com(provider);
    if hr >= 0 && !mgr.is_null() { Some(mgr) } else {
        eprintln!("[zenith:ws] QueryService VDM Internal FAILED hr=0x{:08X}", hr as u32);
        None
    }
}

unsafe fn get_view_collection() -> Option<*mut std::ffi::c_void> {
    let shell: IUnknown = match CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER) {
        Ok(s) => s,
        Err(e) => { eprintln!("[zenith:ws] view col CoCreateInstance FAILED: {e}"); return None; }
    };
    let shell_raw = std::mem::transmute::<IUnknown, *mut std::ffi::c_void>(shell);
    let provider = {
        let vtbl = get_vtable(shell_raw);
        let qi: QIFn = std::mem::transmute(*vtbl.add(0));
        let mut out: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = qi(shell_raw, &IID_ISERVICE_PROVIDER, &mut out);
        release_com(shell_raw);
        if hr < 0 || out.is_null() { eprintln!("[zenith:ws] view col QI FAILED"); return None; }
        out
    };
    let service_vtbl = get_vtable(provider);
    let query_svc: QueryServiceFn = std::mem::transmute(*service_vtbl.add(3));
    let mut col: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = query_svc(provider, &CLSID_IMMERSIVE_SHELL as *const _, &IID_IAPPLICATION_VIEW_COLLECTION as *const _, &mut col);
    release_com(provider);
    if hr >= 0 && !col.is_null() { Some(col) } else {
        eprintln!("[zenith:ws] query view collection FAILED hr=0x{:08X}", hr as u32);
        None
    }
}

// --- Desktop operations (use layout() for variable slots) ---

unsafe fn get_desktop_count(mgr: *mut std::ffi::c_void) -> u32 {
    let vtbl = get_vtable(mgr);
    let func: GetCountFn = std::mem::transmute(*vtbl.add(3));
    let mut count = 0u32;
    func(mgr, &mut count);
    count
}

unsafe fn get_current_desktop_ptr(mgr: *mut std::ffi::c_void) -> Option<*mut std::ffi::c_void> {
    let vtbl = get_vtable(mgr);
    let func: GetCurrentDesktopFn = std::mem::transmute(*vtbl.add(6));
    let mut desktop: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = func(mgr, &mut desktop);
    if hr >= 0 && !desktop.is_null() { Some(desktop) } else { None }
}

unsafe fn switch_to_desktop(mgr: *mut std::ffi::c_void, desktop: *mut std::ffi::c_void) -> bool {
    let slot = layout().switch_desktop;
    let vtbl = get_vtable(mgr);
    let func: SwitchDesktopFn = std::mem::transmute(*vtbl.add(slot));
    let hr = func(mgr, desktop);
    if hr < 0 { eprintln!("[zenith:ws] switch_desktop slot {} FAILED hr=0x{:08X}", slot, hr as u32); }
    hr >= 0
}

unsafe fn desktop_get_id(desktop: *mut std::ffi::c_void) -> GUID {
    let vtbl = get_vtable(desktop);
    let func: DesktopGetIdFn = std::mem::transmute(*vtbl.add(4));
    let mut guid: GUID = std::mem::zeroed();
    func(desktop, &mut guid);
    guid
}

unsafe fn desktop_get_name(desktop: *mut std::ffi::c_void) -> Option<String> {
    let vtbl = get_vtable(desktop);
    let func: DesktopGetNameFn = std::mem::transmute(*vtbl.add(5));
    let mut name = HSTRING::default();
    let hr = func(desktop, &mut name);
    if hr >= 0 && !name.is_empty() {
        Some(name.to_string_lossy().to_string())
    } else {
        None
    }
}

unsafe fn get_desktop_array(mgr: *mut std::ffi::c_void) -> Option<*mut std::ffi::c_void> {
    let vtbl = get_vtable(mgr);
    let func: GetDesktopsFn = std::mem::transmute(*vtbl.add(7));
    let mut arr: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = func(mgr, &mut arr);
    if hr >= 0 && !arr.is_null() { Some(arr) } else { None }
}

unsafe fn get_desktop_at_index(arr: *mut std::ffi::c_void, index: u32) -> Option<*mut std::ffi::c_void> {
    let arr_vtbl = get_vtable(arr);
    let get_at: ArrGetAtFn = std::mem::transmute(*arr_vtbl.add(4));
    let mut desktop: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = get_at(arr, index, &IID_IVIRTUAL_DESKTOP as *const _, &mut desktop);
    if hr >= 0 && !desktop.is_null() { Some(desktop) } else { None }
}

// --- Public operations ---

fn try_com_get_workspaces() -> Option<Vec<DesktopInfo>> {
    unsafe {
        let mgr = cached_manager()?;
        let count = get_desktop_count(mgr);
        if count == 0 { return None; }
        let arr = get_desktop_array(mgr)?;
        let workspaces: Vec<DesktopInfo> = (0..count).map(|i| {
            let name = get_desktop_at_index(arr, i).and_then(|d| {
                let n = desktop_get_name(d);
                release_com(d);
                n
            });
            DesktopInfo { id: i, label: name.unwrap_or_else(|| format!("{}", i + 1)) }
        }).collect();
        release_com(arr);
        Some(workspaces)
    }
}

fn try_com_get_active() -> Option<u32> {
    unsafe {
        let mgr = cached_manager()?;
        let count = get_desktop_count(mgr);
        if count == 0 { return None; }
        let current = get_current_desktop_ptr(mgr)?;
        let target_id = desktop_get_id(current);
        release_com(current);
        let arr = get_desktop_array(mgr)?;
        let mut found = 0u32;
        for i in 0..count {
            if let Some(d) = get_desktop_at_index(arr, i) {
                let did = desktop_get_id(d);
                release_com(d);
                if did == target_id { found = i; break; }
            }
        }
        release_com(arr);
        Some(found)
    }
}

fn try_com_switch(index: u32) -> bool {
    unsafe {
        let mgr = match cached_manager() { Some(m) => m, None => return false };
        let count = get_desktop_count(mgr);
        if index >= count { return false; }
        let arr = match get_desktop_array(mgr) { Some(a) => a, None => return false };
        let desktop = get_desktop_at_index(arr, index);
        release_com(arr);
        match desktop {
            Some(d) => { let r = switch_to_desktop(mgr, d); release_com(d); r }
            None => false
        }
    }
}

fn try_com_move_window_to_desktop(window_index: u32) -> bool {
    unsafe {
        let hwnd = get_cached_foreground_hwnd();
        if skip_bar_hwnd(hwnd) { eprintln!("[zenith:ws] move: no foreground window"); return false; }

        let col = match get_view_collection() { Some(c) => c, None => return false };
        let col_vtbl = get_vtable(col);
        let get_view: GetViewForHwndFn = std::mem::transmute(*col_vtbl.add(6));
        let mut view: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = get_view(col, hwnd, &mut view);
        if hr < 0 || view.is_null() {
            eprintln!("[zenith:ws] move: GetViewForHwnd FAILED hr=0x{:08X}", hr as u32);
            release_com(col);
            return false;
        }
        release_com(col);

        let mgr = match cached_manager() { Some(m) => m, None => { release_com(view); return false; } };
        let count = get_desktop_count(mgr);
        if window_index >= count { release_com(view); return false; }

        let arr = match get_desktop_array(mgr) { Some(a) => a, None => { release_com(view); return false; } };
        let desktop = match get_desktop_at_index(arr, window_index) { Some(d) => d, None => { release_com(arr); release_com(view); return false; } };
        release_com(arr);

        let mgr_vtbl = get_vtable(mgr);
        let move_fn: MoveViewFn = std::mem::transmute(*mgr_vtbl.add(4));
        let result = move_fn(mgr, view, desktop) >= 0;
        if !result { eprintln!("[zenith:ws] move: MoveViewToDesktop FAILED"); }

        release_com(desktop);
        release_com(view);
        result
    }
}

unsafe fn create_desktop_com(mgr: *mut std::ffi::c_void) -> bool {
    let slot = layout().create_desktop;
    let vtbl = get_vtable(mgr);
    let func: CreateDesktopFn = std::mem::transmute(*vtbl.add(slot));
    let mut desktop: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = func(mgr, &mut desktop);
    if hr >= 0 && !desktop.is_null() { release_com(desktop); true }
    else { eprintln!("[zenith:ws] create desktop slot {} FAILED hr=0x{:08X}", slot, hr as u32); false }
}

unsafe fn remove_desktop_at_index(mgr: *mut std::ffi::c_void, index: u32) -> bool {
    let count = get_desktop_count(mgr);
    if count < 2 || index >= count { return false; }
    let fallback_index = if index > 0 { 0u32 } else { 1u32 };
    let arr = match get_desktop_array(mgr) { Some(a) => a, None => return false };
    let desktop = get_desktop_at_index(arr, index);
    let fallback = get_desktop_at_index(arr, fallback_index);
    release_com(arr);
    match (desktop, fallback) {
        (Some(d), Some(f)) => {
            let slot = layout().remove_desktop;
            let vtbl = get_vtable(mgr);
            let func: RemoveDesktopFn = std::mem::transmute(*vtbl.add(slot));
            let hr = func(mgr, d, f);
            release_com(d); release_com(f);
            if hr < 0 { eprintln!("[zenith:ws] remove desktop slot {} FAILED hr=0x{:08X}", slot, hr as u32); }
            hr >= 0
        }
        _ => false,
    }
}

unsafe fn rename_desktop_at_index(mgr: *mut std::ffi::c_void, index: u32, name: &str) -> bool {
    let arr = match get_desktop_array(mgr) { Some(a) => a, None => return false };
    let desktop = match get_desktop_at_index(arr, index) { Some(d) => d, None => { release_com(arr); return false; } };
    release_com(arr);
    let vtbl = get_vtable(desktop);
    let func: DesktopRenameFn = std::mem::transmute(*vtbl.add(6));
    let hstr = HSTRING::from(name);
    let hr = func(desktop, &hstr);
    release_com(desktop);
    hr >= 0
}

fn get_pinned_apps() -> Option<*mut std::ffi::c_void> {
    unsafe {
        let shell: IUnknown = match CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER) { Ok(s) => s, Err(_) => return None };
        let shell_raw = std::mem::transmute::<IUnknown, *mut std::ffi::c_void>(shell);
        let provider = {
            let vtbl = get_vtable(shell_raw);
            let qi: QIFn = std::mem::transmute(*vtbl.add(0));
            let mut out: *mut std::ffi::c_void = std::ptr::null_mut();
            let hr = qi(shell_raw, &IID_ISERVICE_PROVIDER, &mut out);
            release_com(shell_raw);
            if hr < 0 || out.is_null() { return None; }
            out
        };
        let vtbl = get_vtable(provider);
        let qs: QueryServiceFn = std::mem::transmute(*vtbl.add(3));
        let mut out: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = qs(provider, &CLSID_VIRTUAL_DESKTOP_PINNED_APPS as *const _, &IID_VIRTUAL_DESKTOP_PINNED_APPS as *const _, &mut out);
        release_com(provider);
        if hr >= 0 && !out.is_null() { Some(out) } else { None }
    }
}

unsafe fn toggle_pin_hwnd(hwnd: HWND) -> Result<bool, String> {
    let pinned = match get_pinned_apps() { Some(p) => p, None => return Err("pinned apps service unavailable".into()) };
    let vtbl = get_vtable(pinned);
    let mut is_pinned: i32 = 0;
    let is_pinned_fn: IsWindowPinnedFn = std::mem::transmute(*vtbl.add(3));
    let hr = is_pinned_fn(pinned, hwnd, &mut is_pinned);
    if hr < 0 { release_com(pinned); return Err("IsWindowPinned failed".into()); }
    if is_pinned != 0 {
        let unpin: PinUnpinHwndFn = std::mem::transmute(*vtbl.add(5));
        let hr = unpin(pinned, hwnd);
        release_com(pinned);
        if hr < 0 { return Err("UnpinWindow failed".into()); }
        Ok(false)
    } else {
        let pin: PinUnpinHwndFn = std::mem::transmute(*vtbl.add(4));
        let hr = pin(pinned, hwnd);
        release_com(pinned);
        if hr < 0 { return Err("PinWindow failed".into()); }
        Ok(true)
    }
}

static FALLBACK_COUNT: AtomicU32 = AtomicU32::new(0);
static FALLBACK_ACTIVE: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, Serialize)]
pub struct DesktopInfo {
    pub id: u32,
    pub label: String,
}

#[tauri::command]
pub fn get_workspaces() -> Vec<DesktopInfo> {
    if let Some(ws) = try_com_get_workspaces() {
        FALLBACK_COUNT.store(ws.len() as u32, Ordering::Relaxed);
        return ws;
    }
    let n = FALLBACK_COUNT.load(Ordering::Relaxed).max(3);
    (0..n).map(|i| DesktopInfo { id: i, label: format!("{}", i + 1) }).collect()
}

#[tauri::command]
pub fn get_active_workspace() -> u32 {
    if let Some(idx) = try_com_get_active() {
        FALLBACK_ACTIVE.store(idx, Ordering::Relaxed);
        return idx;
    }
    FALLBACK_ACTIVE.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn switch_workspace(app: tauri::AppHandle, id: u32) -> Result<(), String> {
    let ok = try_com_switch(id);
    FALLBACK_ACTIVE.store(id, Ordering::Relaxed);
    let _ = app.emit(crate::shared::EVENT_WORKSPACE_CHANGED, id);
    if !ok { return Err("switch failed".into()); }
    Ok(())
}

#[tauri::command]
pub fn move_window_to_desktop(app: tauri::AppHandle, id: u32) -> Result<(), String> {
    let ok = try_com_move_window_to_desktop(id);
    if !ok { return Err("move failed".into()); }
    if let Some(active) = try_com_get_active() {
        FALLBACK_ACTIVE.store(active, Ordering::Relaxed);
        let _ = app.emit(crate::shared::EVENT_WORKSPACE_CHANGED, active);
    }
    Ok(())
}

#[tauri::command]
pub fn create_desktop(app: tauri::AppHandle) -> Result<(), String> {
    let ok = unsafe { match cached_manager() { Some(mgr) => create_desktop_com(mgr), None => false } };
    if ok {
        let _ = app.emit(crate::shared::EVENT_WORKSPACE_CHANGED, get_active_workspace());
        Ok(())
    } else {
        Err("create desktop failed".into())
    }
}

#[tauri::command]
pub fn delete_desktop(app: tauri::AppHandle, id: u32) -> Result<(), String> {
    let ok = unsafe { match cached_manager() { Some(mgr) => remove_desktop_at_index(mgr, id), None => false } };
    if ok {
        let _ = app.emit(crate::shared::EVENT_WORKSPACE_CHANGED, get_active_workspace());
        Ok(())
    } else {
        Err("delete desktop failed".into())
    }
}

#[tauri::command]
pub fn rename_desktop(id: u32, name: String) -> Result<(), String> {
    let ok = unsafe { match cached_manager() { Some(mgr) => rename_desktop_at_index(mgr, id, &name), None => false } };
    if ok { Ok(()) } else { Err("rename desktop failed".into()) }
}

#[tauri::command]
pub fn toggle_pin_window() -> Result<bool, String> {
    let hwnd = get_cached_foreground_hwnd();
    if skip_bar_hwnd(hwnd) { return Err("no foreground window".into()); }
    unsafe { toggle_pin_hwnd(hwnd) }
}

pub fn pin_state() -> bool {
    let hwnd = get_cached_foreground_hwnd();
    if skip_bar_hwnd(hwnd) { return false; }
    unsafe {
        let pinned = match get_pinned_apps() { Some(p) => p, None => return false };
        let vtbl = get_vtable(pinned);
        let mut is_pinned: i32 = 0;
        let fn_ptr: IsWindowPinnedFn = std::mem::transmute(*vtbl.add(3));
        let hr = fn_ptr(pinned, hwnd, &mut is_pinned);
        release_com(pinned);
        hr >= 0 && is_pinned != 0
    }
}
