use std::sync::Mutex;
use tauri::AppHandle;
static LISTENER: Mutex<Option<winvd::DesktopEventThread>> = Mutex::new(None);

/// Start the virtual-desktop event listener using `winvd::listen_desktop_events`.
/// Sends `zenith:workspace-changed` to the frontend when desktops change, are
/// created/destroyed, or renamed.
///
/// Replaces the previous hand-rolled COM notification sink. `winvd` handles all
/// vtable layout differences across Windows builds and is safe to use across
/// threads.
///
/// The returned `DesktopEventThread` MUST be kept alive for the duration of the
/// program — when it's dropped, the listener is closed and the worker thread
/// is joined. We store it in a `Mutex<Option<...>>` so dropping it only happens
/// at process exit (lock guards prevent early drop).
pub fn setup(app_handle: AppHandle) {
    let mut guard = LISTENER.lock().expect("event listener mutex poisoned");
    if guard.is_none() {
        match super::commands::setup_events(app_handle) {
            Ok(handle) => {
                *guard = Some(handle);
            }
            Err(e) => eprintln!("[zenith:ws] event listener failed: {e:?}"),
        }
    }
}
