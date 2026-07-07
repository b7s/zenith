use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub cpu_percent: f64,
    pub ram_used: u64,
    pub ram_total: u64,
    pub ram_percent: f64,
}

struct CpuSample {
    idle: u64,
    kernel: u64,
    user: u64,
    at: Instant,
}

static LAST_CPU: Mutex<Option<CpuSample>> = Mutex::new(None);

fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

fn cpu_percent() -> f64 {
    unsafe {
        let mut idle = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut kernel = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut user = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_err() {
            return 0.0;
        }

        let idle_v = filetime_to_u64(idle);
        let kernel_v = filetime_to_u64(kernel);
        let user_v = filetime_to_u64(user);

        let now = Instant::now();
        let mut guard = match LAST_CPU.lock() {
            Ok(g) => g,
            Err(_) => return 0.0,
        };

        let new_total = kernel_v.wrapping_add(user_v);
        let old_total;
        let old_idle;
        let elapsed_secs;
        match &*guard {
            Some(prev) => {
                old_total = prev.kernel.wrapping_add(prev.user);
                old_idle = prev.idle;
                elapsed_secs = now.duration_since(prev.at).as_secs_f64();
            }
            None => {
                *guard = Some(CpuSample {
                    idle: idle_v,
                    kernel: kernel_v,
                    user: user_v,
                    at: now,
                });
                return 0.0;
            }
        }

        *guard = Some(CpuSample {
            idle: idle_v,
            kernel: kernel_v,
            user: user_v,
            at: now,
        });

        if elapsed_secs < 0.001 {
            return 0.0;
        }

        let total_delta = new_total.wrapping_sub(old_total) as f64;
        let idle_delta = idle_v.wrapping_sub(old_idle) as f64;
        if total_delta <= 0.0 {
            return 0.0;
        }
        let busy = (total_delta - idle_delta).max(0.0);
        let pct = (busy / total_delta) * 100.0;
        pct.clamp(0.0, 100.0)
    }
}

fn ram_stats() -> (u64, u64, f64) {
    unsafe {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut status).is_err() {
            return (0, 0, 0.0);
        }
        let total = status.ullTotalPhys;
        let avail = status.ullAvailPhys;
        let used = total.saturating_sub(avail);
        let pct = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        (used, total, pct.clamp(0.0, 100.0))
    }
}

#[tauri::command]
pub fn get_system_stats() -> SystemStats {
    let cpu = cpu_percent();
    let (used, total, pct) = ram_stats();
    SystemStats {
        cpu_percent: cpu,
        ram_used: used,
        ram_total: total,
        ram_percent: pct,
    }
}
