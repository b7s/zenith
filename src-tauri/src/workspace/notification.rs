use std::sync::atomic::{AtomicU32, Ordering};
use tauri::Emitter;
use windows::core::GUID;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER, CoInitializeEx, COINIT_APARTMENTTHREADED};

type QIFn = unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> i32;
type ReleaseFn = unsafe extern "system" fn(*mut std::ffi::c_void) -> u32;
type QueryServiceFn = unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *const GUID, *mut *mut std::ffi::c_void) -> i32;
type RegisterFn = unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut u32) -> i32;
#[allow(dead_code)]
type UnregisterFn = unsafe extern "system" fn(*mut std::ffi::c_void, u32) -> i32;

const CLSID_IMMERSIVE_SHELL: GUID = GUID {
    data1: 0xC2F03A33, data2: 0x21F5, data3: 0x47FA,
    data4: [0xB4, 0xBB, 0x15, 0x63, 0x62, 0xA2, 0xF2, 0x39],
};
const IID_ISERVICE_PROVIDER: GUID = GUID {
    data1: 0x6D5140C1, data2: 0x7436, data3: 0x11CE,
    data4: [0x80, 0x34, 0x00, 0xAA, 0x00, 0x60, 0x09, 0xFA],
};
const CLSID_VIRTUAL_NOTIFICATION_SERVICE: GUID = GUID {
    data1: 0xA501FDEC, data2: 0x4A09, data3: 0x464C,
    data4: [0xAE, 0x4E, 0x1B, 0x9C, 0x21, 0xB8, 0x49, 0x18],
};
const IID_NOTIFICATION_SERVICE: GUID = GUID {
    data1: 0x0CD45E71, data2: 0xD927, data3: 0x4F15,
    data4: [0x8B, 0x0A, 0x8F, 0xEF, 0x52, 0x53, 0x37, 0xBF],
};
const IID_NOTIFICATION: GUID = GUID {
    data1: 0xB9E5E94D, data2: 0x233E, data3: 0x49AB,
    data4: [0xAF, 0x5C, 0x2B, 0x45, 0x41, 0xC3, 0xAA, 0xDE],
};

#[repr(C)]
struct SinkVtable {
    qi: QIFn,
    add_ref: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    created: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    destroy_begin: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    destroy_failed: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    destroyed: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    moved: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i64, i64) -> i32,
    name_changed: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    view_changed: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    current_changed: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    wallpaper_changed: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    switched: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    remote_connected: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
}

#[repr(C)]
struct Sink {
    vtable: &'static SinkVtable,
    ref_count: AtomicU32,
    app_handle: tauri::AppHandle,
}

const IID_IUNKNOWN: GUID = GUID {
    data1: 0x00000000, data2: 0x0000, data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

unsafe extern "system" fn sink_qi(this: *mut std::ffi::c_void, riid: *const GUID, out: *mut *mut std::ffi::c_void) -> i32 {
    if riid.is_null() || out.is_null() { return 0x80004002u32 as i32; }
    let iid = *riid;
    if iid != IID_IUNKNOWN && iid != IID_NOTIFICATION { return 0x80004002u32 as i32; }
    std::ptr::write(out, this);
    let sink = &*(this as *const Sink);
    sink.ref_count.fetch_add(1, Ordering::Relaxed);
    0
}

unsafe extern "system" fn sink_add_ref(this: *mut std::ffi::c_void) -> u32 {
    let sink = &*(this as *const Sink);
    sink.ref_count.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn sink_release(this: *mut std::ffi::c_void) -> u32 {
    let sink = &*(this as *const Sink);
    let prev = sink.ref_count.fetch_sub(1, Ordering::Relaxed);
    if prev == 1 {
        drop(Box::from_raw(this as *mut Sink));
        0
    } else {
        prev - 1
    }
}

unsafe extern "system" fn sink_current_changed(this: *mut std::ffi::c_void, _old: *mut std::ffi::c_void, _new: *mut std::ffi::c_void) -> i32 {
    let sink = &*(this as *const Sink);
    let active = crate::workspace::commands::get_active_workspace();
    let _ = sink.app_handle.emit(crate::shared::EVENT_WORKSPACE_CHANGED, active);
    0
}

unsafe extern "system" fn sink_created(_this: *mut std::ffi::c_void, _desktop: *mut std::ffi::c_void) -> i32 { 0 }
unsafe extern "system" fn sink_destroy_begin(_this: *mut std::ffi::c_void, _a: *mut std::ffi::c_void, _b: *mut std::ffi::c_void) -> i32 { 0 }
unsafe extern "system" fn sink_destroy_failed(_this: *mut std::ffi::c_void, _a: *mut std::ffi::c_void, _b: *mut std::ffi::c_void) -> i32 { 0 }
unsafe extern "system" fn sink_destroyed(_this: *mut std::ffi::c_void, _a: *mut std::ffi::c_void, _b: *mut std::ffi::c_void) -> i32 { 0 }
unsafe extern "system" fn sink_moved(_this: *mut std::ffi::c_void, _desktop: *mut std::ffi::c_void, _old: i64, _new: i64) -> i32 { 0 }
unsafe extern "system" fn sink_name_changed(_this: *mut std::ffi::c_void, _desktop: *mut std::ffi::c_void, _name: *mut std::ffi::c_void) -> i32 { 0 }
unsafe extern "system" fn sink_view_changed(_this: *mut std::ffi::c_void, _view: *mut std::ffi::c_void) -> i32 { 0 }
unsafe extern "system" fn sink_wallpaper_changed(_this: *mut std::ffi::c_void, _desktop: *mut std::ffi::c_void, _name: *mut std::ffi::c_void) -> i32 { 0 }
unsafe extern "system" fn sink_switched(_this: *mut std::ffi::c_void, _desktop: *mut std::ffi::c_void) -> i32 { 0 }
unsafe extern "system" fn sink_remote_connected(_this: *mut std::ffi::c_void, _desktop: *mut std::ffi::c_void) -> i32 { 0 }

const VTABLE: SinkVtable = SinkVtable {
    qi: sink_qi,
    add_ref: sink_add_ref,
    release: sink_release,
    created: sink_created,
    destroy_begin: sink_destroy_begin,
    destroy_failed: sink_destroy_failed,
    destroyed: sink_destroyed,
    moved: sink_moved,
    name_changed: sink_name_changed,
    view_changed: sink_view_changed,
    current_changed: sink_current_changed,
    wallpaper_changed: sink_wallpaper_changed,
    switched: sink_switched,
    remote_connected: sink_remote_connected,
};

unsafe fn get_vtable(ptr: *mut std::ffi::c_void) -> *mut *mut std::ffi::c_void {
    *(ptr as *mut *mut *mut std::ffi::c_void)
}

pub fn setup(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        unsafe { let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); }
        unsafe {
            let shell: windows::core::IUnknown = match CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[zenith:notif] CoCreateInstance FAILED: {e}");
                    return;
                }
            };
            let shell_raw = std::mem::transmute::<windows::core::IUnknown, *mut std::ffi::c_void>(shell);
            let provider = {
                let vtbl = get_vtable(shell_raw);
                let qi: QIFn = std::mem::transmute(*vtbl.add(0));
                let mut out: *mut std::ffi::c_void = std::ptr::null_mut();
                let hr = qi(shell_raw, &IID_ISERVICE_PROVIDER, &mut out);
                let _ = get_vtable(shell_raw);
                let release: ReleaseFn = std::mem::transmute(*get_vtable(shell_raw).add(2));
                release(shell_raw);
                if hr < 0 || out.is_null() { return; }
                out
            };

            let svc = {
                let vtbl = get_vtable(provider);
                let qs: QueryServiceFn = std::mem::transmute(*vtbl.add(3));
                let mut out: *mut std::ffi::c_void = std::ptr::null_mut();
                let hr = qs(provider, &CLSID_VIRTUAL_NOTIFICATION_SERVICE as *const _, &IID_NOTIFICATION_SERVICE as *const _, &mut out);
                let release: ReleaseFn = std::mem::transmute(*get_vtable(provider).add(2));
                release(provider);
                if hr < 0 || out.is_null() { return; }
                out
            };

            let sink = Box::new(Sink {
                vtable: &VTABLE,
                ref_count: AtomicU32::new(1),
                app_handle,
            });
            let sink_ptr = Box::into_raw(sink) as *mut std::ffi::c_void;

            let svc_vtbl = get_vtable(svc);
            let register: RegisterFn = std::mem::transmute(*svc_vtbl.add(3));
            let mut cookie: u32 = 0;
            let hr = register(svc, sink_ptr, &mut cookie);
            if hr >= 0 {
                eprintln!("[zenith:notif] registered sink cookie={}", cookie);
            } else {
                eprintln!("[zenith:notif] register FAILED hr=0x{:08X}", hr as u32);
                drop(Box::from_raw(sink_ptr as *mut Sink));
                let release: ReleaseFn = std::mem::transmute(*get_vtable(svc).add(2));
                release(svc);
                return;
            }

            // Keep thread alive with a message pump so COM can dispatch callbacks
            use windows::Win32::UI::WindowsAndMessaging::{MSG, GetMessageW, TranslateMessage, DispatchMessageW};
            loop {
                let mut msg = MSG::default();
                let ret = GetMessageW(&mut msg, None, 0, 0);
                if ret.0 == 0 { break; }
                if ret.0 == -1 { break; }
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
        }
    });
}
