use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    REG_VALUE_TYPE,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub cpu_percent: f64,
    pub cpu_ghz: f64,
    pub ram_used: u64,
    pub ram_total: u64,
    pub ram_percent: f64,
    pub gpu_percent: f64,
    pub hd_used: u64,
    pub hd_total: u64,
    pub hd_percent: f64,
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
        let mut idle = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut kernel = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut user = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
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
        let (old_total, old_idle, elapsed_secs) = match &*guard {
            Some(prev) => (
                prev.kernel.wrapping_add(prev.user),
                prev.idle,
                now.duration_since(prev.at).as_secs_f64(),
            ),
            None => {
                *guard = Some(CpuSample { idle: idle_v, kernel: kernel_v, user: user_v, at: now });
                return 0.0;
            }
        };

        *guard = Some(CpuSample { idle: idle_v, kernel: kernel_v, user: user_v, at: now });

        if elapsed_secs < 0.001 {
            return 0.0;
        }

        let total_delta = new_total.wrapping_sub(old_total) as f64;
        let idle_delta = idle_v.wrapping_sub(old_idle) as f64;
        if total_delta <= 0.0 {
            return 0.0;
        }
        ((total_delta - idle_delta).max(0.0) / total_delta * 100.0).clamp(0.0, 100.0)
    }
}

fn cpu_ghz() -> f64 {
    unsafe {
        let mut hkey = HKEY::default();
        let path = windows::core::w!("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0");
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, path, None, KEY_READ, &mut hkey) != WIN32_ERROR(0) {
            return 0.0;
        }

        let name = windows::core::w!("~MHz");
        let mut value: u32 = 0;
        let mut kind = REG_VALUE_TYPE::default();
        let mut size = std::mem::size_of::<u32>() as u32;
        let ret = RegQueryValueExW(
            hkey,
            name,
            None,
            Some(&mut kind),
            Some(&mut value as *mut _ as *mut u8),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        if ret != WIN32_ERROR(0) {
            return 0.0;
        }
        value as f64 / 1000.0
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

fn gpu_percent() -> f64 {
    unsafe {
        let mut query = PDH_HQUERY::default();
        if PdhOpenQueryW(None, 0, &mut query) != 0 {
            return 0.0;
        }

        let mut counter = PDH_HCOUNTER::default();
        let path = windows::core::w!("\\GPU Engine(*)\\Utilization Percentage");
        if PdhAddEnglishCounterW(query, path, 0, &mut counter) != 0 {
            PdhCloseQuery(query);
            return 0.0;
        }

        if PdhCollectQueryData(query) != 0 {
            PdhCloseQuery(query);
            return 0.0;
        }

        std::thread::sleep(std::time::Duration::from_millis(10));

        if PdhCollectQueryData(query) != 0 {
            PdhCloseQuery(query);
            return 0.0;
        }

        let mut value = PDH_FMT_COUNTERVALUE::default();
        let mut dw_type = 0u32;
        if PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, Some(&mut dw_type), &mut value) != 0
        {
            PdhCloseQuery(query);
            return 0.0;
        }

        PdhCloseQuery(query);

        if value.CStatus != 0 {
            return 0.0;
        }
        (value.Anonymous.doubleValue).max(0.0).min(100.0)
    }
}

fn hd_stats() -> (u64, u64, f64) {
    unsafe {
        let mut free: u64 = 0;
        let mut total: u64 = 0;
        let mut free_total: u64 = 0;
        let path = windows::core::w!("C:\\");
        if GetDiskFreeSpaceExW(path, Some(&mut free), Some(&mut total), Some(&mut free_total))
            .is_err()
        {
            return (0, 0, 0.0);
        }
        if total == 0 {
            return (0, 0, 0.0);
        }
        let used = total.saturating_sub(free_total);
        let pct = (used as f64 / total as f64) * 100.0;
        (used, total, pct.clamp(0.0, 100.0))
    }
}

#[tauri::command]
pub fn get_system_stats() -> SystemStats {
    let cpu = cpu_percent();
    let ghz = cpu_ghz();
    let (r_used, r_total, r_pct) = ram_stats();
    let gpu = gpu_percent();
    let (h_used, h_total, h_pct) = hd_stats();
    SystemStats {
        cpu_percent: cpu,
        cpu_ghz: ghz,
        ram_used: r_used,
        ram_total: r_total,
        ram_percent: r_pct,
        gpu_percent: gpu,
        hd_used: h_used,
        hd_total: h_total,
        hd_percent: h_pct,
    }
}
