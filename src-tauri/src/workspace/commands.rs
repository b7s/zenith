use std::sync::atomic::{AtomicU32, Ordering};
use serde::Serialize;
use tauri::Emitter;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER};
use windows::core::{IUnknown, GUID};

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

// Vtable function pointer types
type QIFn = unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> i32;
type ReleaseFn = unsafe extern "system" fn(*mut std::ffi::c_void) -> u32;
type QueryServiceFn = unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *const GUID, *mut *mut std::ffi::c_void) -> i32;
type GetCountFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut u32) -> i32;
type GetCurrentDesktopFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut *mut std::ffi::c_void) -> i32;
type GetDesktopsFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut *mut std::ffi::c_void) -> i32;
type SwitchDesktopFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;
type ArrGetAtFn = unsafe extern "system" fn(*mut std::ffi::c_void, u32, *const GUID, *mut *mut std::ffi::c_void) -> i32;
type DesktopGetIdFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut GUID) -> i32;

unsafe fn get_vtable(ptr: *mut std::ffi::c_void) -> *mut *mut std::ffi::c_void {
    *(ptr as *mut *mut *mut std::ffi::c_void)
}

unsafe fn release_com(ptr: *mut std::ffi::c_void) {
    if ptr.is_null() { return; }
    let vtbl = get_vtable(ptr);
    let release: ReleaseFn = std::mem::transmute(*vtbl.add(2));
    release(ptr);
}

/// Get the `IVirtualDesktopManagerInternal` via:
/// 1. CoCreate(CLSID_ImmersiveShell) → IServiceProvider
/// 2. QueryService(CLSID_VDM_INTERNAL, IID_VDM_INTERNAL) → IVirtualDesktopManagerInternal
unsafe fn get_manager_internal() -> Option<*mut std::ffi::c_void> {
    // Step 1: Create ImmersiveShell and get IServiceProvider
    let shell: IUnknown = match CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[zenith:ws] CoCreateInstance(ImmersiveShell) FAILED: {e}");
            return None;
        }
    };
    let shell_raw = std::mem::transmute::<IUnknown, *mut std::ffi::c_void>(shell);

    // QI for IServiceProvider
    let provider = query_interface(shell_raw, &IID_ISERVICE_PROVIDER);
    release_com(shell_raw);
    let provider = match provider {
        Some(p) => p,
        None => {
            eprintln!("[zenith:ws] QI for IServiceProvider FAILED");
            return None;
        }
    };

    // Step 2: QueryService for IVirtualDesktopManagerInternal
    let service_vtbl = get_vtable(provider);
    let query_svc: QueryServiceFn = std::mem::transmute(*service_vtbl.add(3));
    let mut mgr: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = query_svc(provider, &CLSID_VDM_INTERNAL as *const _, &IID_VDM_INTERNAL as *const _, &mut mgr);
    release_com(provider);

    if hr >= 0 && !mgr.is_null() {
        eprintln!("[zenith:ws] get_manager_internal OK mgr={:p}", mgr);
        Some(mgr)
    } else {
        eprintln!("[zenith:ws] QueryService for VirtualDesktopManagerInternal FAILED hr=0x{:08X}", hr as u32);
        None
    }
}

unsafe fn query_interface(unk: *mut std::ffi::c_void, iid: *const GUID) -> Option<*mut std::ffi::c_void> {
    let vtbl = get_vtable(unk);
    let qi: QIFn = std::mem::transmute(*vtbl.add(0));
    let mut out: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = qi(unk, iid, &mut out);
    if hr >= 0 && !out.is_null() { Some(out) } else { None }
}

/// IVirtualDesktopManagerInternal vtable (IID 53F5CA0B-158F-4124-900C-057158060B27):
/// 0: QI  1: AddRef  2: Release
/// 3: GetCount                (UINT*)
/// 4: MoveViewToDesktop
/// 5: CanViewMoveDesktops
/// 6: GetCurrentDesktop       (IVirtualDesktop**)
/// 7: GetDesktops             (IObjectArray**)
/// 8: GetAdjacentDesktop
/// 9: SwitchDesktop           (IVirtualDesktop*)
/// 10: CreateDesktop
/// 11: MoveDesktop
/// 12: RemoveDesktop
/// 13: FindDesktop

unsafe fn get_desktop_count(mgr: *mut std::ffi::c_void) -> u32 {
    let vtbl = get_vtable(mgr);
    let func: GetCountFn = std::mem::transmute(*vtbl.add(3));
    let mut count = 0u32;
    let hr = func(mgr, &mut count);
    eprintln!("[zenith:ws] get_desktop_count hr=0x{:08X} count={}", hr as u32, count);
    count
}

unsafe fn get_current_desktop_ptr(mgr: *mut std::ffi::c_void) -> Option<*mut std::ffi::c_void> {
    let vtbl = get_vtable(mgr);
    let func: GetCurrentDesktopFn = std::mem::transmute(*vtbl.add(6));
    let mut desktop: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = func(mgr, &mut desktop);
    eprintln!("[zenith:ws] get_current_desktop hr=0x{:08X} desktop={:p}", hr as u32, desktop);
    if hr >= 0 && !desktop.is_null() { Some(desktop) } else { None }
}

unsafe fn switch_to_desktop(mgr: *mut std::ffi::c_void, desktop: *mut std::ffi::c_void) -> bool {
    let vtbl = get_vtable(mgr);
    let func: SwitchDesktopFn = std::mem::transmute(*vtbl.add(9));
    let hr = func(mgr, desktop);
    eprintln!("[zenith:ws] switch_desktop hr=0x{:08X}", hr as u32);
    hr >= 0
}

unsafe fn desktop_get_id(desktop: *mut std::ffi::c_void) -> GUID {
    let vtbl = get_vtable(desktop);
    let func: DesktopGetIdFn = std::mem::transmute(*vtbl.add(4));
    let mut guid: GUID = std::mem::zeroed();
    let hr = func(desktop, &mut guid);
    eprintln!("[zenith:ws] desktop_get_id hr=0x{:08X} guid={:08X}-{:04X}-{:04X}", hr as u32, guid.data1, guid.data2, guid.data3);
    guid
}

unsafe fn get_desktop_at_index(mgr: *mut std::ffi::c_void, index: u32) -> Option<*mut std::ffi::c_void> {
    let vtbl = get_vtable(mgr);
    let func: GetDesktopsFn = std::mem::transmute(*vtbl.add(7));
    let mut arr: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr = func(mgr, &mut arr);
    eprintln!("[zenith:ws] get_desktops hr=0x{:08X} arr={:p}", hr as u32, arr);
    if hr < 0 || arr.is_null() { return None; }

    let arr_vtbl = get_vtable(arr);
    let get_at: ArrGetAtFn = std::mem::transmute(*arr_vtbl.add(4));
    let mut desktop: *mut std::ffi::c_void = std::ptr::null_mut();
    let hr2 = get_at(arr, index, &IID_IVIRTUAL_DESKTOP as *const _, &mut desktop);
    eprintln!("[zenith:ws] get_desktop_at_index({}) GetAt hr=0x{:08X} desktop={:p}", index, hr2 as u32, desktop);
    release_com(arr);
    if hr2 >= 0 && !desktop.is_null() { Some(desktop) } else { None }
}

fn try_com_get_workspaces() -> Option<Vec<DesktopInfo>> {
    eprintln!("[zenith:ws] try_com_get_workspaces");
    unsafe {
        let mgr = get_manager_internal()?;
        let count = get_desktop_count(mgr);
        if count == 0 { release_com(mgr); return None; }

        let workspaces: Vec<DesktopInfo> = (0..count).map(|i| {
            DesktopInfo { id: i, label: format!("{}", i + 1) }
        }).collect();

        release_com(mgr);
        eprintln!("[zenith:ws] returning {} workspaces", count);
        Some(workspaces)
    }
}

fn try_com_get_active() -> Option<u32> {
    eprintln!("[zenith:ws] try_com_get_active");
    unsafe {
        let mgr = get_manager_internal()?;
        let count = get_desktop_count(mgr);
        if count == 0 { release_com(mgr); return None; }

        let current = get_current_desktop_ptr(mgr);
        let active_idx = match current {
            Some(c) => {
                let target_id = desktop_get_id(c);
                release_com(c);
                let mut found = 0u32;
                for i in 0..count {
                    if let Some(d) = get_desktop_at_index(mgr, i) {
                        let did = desktop_get_id(d);
                        release_com(d);
                        if did == target_id { found = i; break; }
                    }
                }
                eprintln!("[zenith:ws] active desktop idx={}", found);
                found
            }
            None => {
                eprintln!("[zenith:ws] no current desktop ptr, returning 0");
                0
            }
        };

        release_com(mgr);
        Some(active_idx)
    }
}

fn try_com_switch(index: u32) -> bool {
    eprintln!("[zenith:ws] try_com_switch index={}", index);
    unsafe {
        let mgr = match get_manager_internal() {
            Some(m) => m,
            None => {
                eprintln!("[zenith:ws] try_com_switch: get_manager_internal FAILED");
                return false;
            }
        };

        let count = get_desktop_count(mgr);
        if index >= count {
            eprintln!("[zenith:ws] try_com_switch: index {} >= count {}", index, count);
            release_com(mgr);
            return false;
        }

        let desktop = get_desktop_at_index(mgr, index);
        let ok = match desktop {
            Some(d) => {
                eprintln!("[zenith:ws] try_com_switch: got desktop ptr={:p}, switching...", d);
                let result = switch_to_desktop(mgr, d);
                release_com(d);
                result
            }
            None => {
                eprintln!("[zenith:ws] try_com_switch: get_desktop_at_index returned None");
                false
            }
        };

        release_com(mgr);
        ok
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
    eprintln!("[zenith:ws] switch_workspace command id={}", id);
    let ok = try_com_switch(id);
    eprintln!("[zenith:ws] switch_workspace command result={}", ok);
    FALLBACK_ACTIVE.store(id, Ordering::Relaxed);
    let _ = app.emit(crate::shared::EVENT_WORKSPACE_CHANGED, id);
    Ok(())
}
