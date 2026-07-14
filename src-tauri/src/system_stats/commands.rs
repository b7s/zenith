use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use windows::core::w;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives,
};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
    PDH_MORE_DATA,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    REG_VALUE_TYPE,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

const DRIVE_FIXED: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStats {
    pub name: String,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HdStats {
    pub mount: String,
    pub used: u64,
    pub total: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub name: String,
    pub recv_bps: f64,
    pub send_bps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub cpu_percent: f64,
    pub cpu_ghz: f64,
    pub ram_used: u64,
    pub ram_total: u64,
    pub ram_percent: f64,
    pub gpu: Vec<GpuStats>,
    pub hd: Vec<HdStats>,
    pub network: Vec<NetworkStats>,
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
        let path = w!("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0");
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, path, None, KEY_READ, &mut hkey).is_err() {
            return 0.0;
        }

        let name = w!("~MHz");
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
        if ret.is_err() {
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

/// Extract the numeric phys index from a GPU Engine instance name like `pid_1234_eng_0_phys_0`
fn extract_phys(instance: &str) -> u32 {
    if let Some(pos) = instance.rfind("phys_") {
        let suffix = &instance[pos + 5..];
        suffix
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    } else {
        0
    }
}

/// Return a human-readable name for a GPU based on its phys index
fn gpu_name(phys_idx: u32) -> String {
    format!("GPU{}", phys_idx)
}

fn gpu_stats() -> Vec<GpuStats> {
    unsafe {
        let mut query = PDH_HQUERY::default();
        if PdhOpenQueryW(None, 0, &mut query) != 0 {
            return vec![];
        }

        let path = w!("\\GPU Engine(*)\\Utilization Percentage");
        let mut counter = PDH_HCOUNTER::default();
        if PdhAddEnglishCounterW(query, path, 0, &mut counter) != 0 {
            PdhCloseQuery(query);
            return vec![];
        }

        if PdhCollectQueryData(query) != 0 {
            PdhCloseQuery(query);
            return vec![];
        }

        std::thread::sleep(std::time::Duration::from_millis(10));

        if PdhCollectQueryData(query) != 0 {
            PdhCloseQuery(query);
            return vec![];
        }

        let mut buf_size: u32 = 0;
        let mut item_count: u32 = 0;
        let ret = PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut buf_size,
            &mut item_count,
            None,
        );
        if ret != 0 && ret != PDH_MORE_DATA {
            PdhCloseQuery(query);
            return vec![];
        }

        let mut buf = vec![0u8; buf_size as usize];
        let ret = PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut buf_size,
            &mut item_count,
            Some(buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
        );
        PdhCloseQuery(query);

        if ret != 0 || item_count == 0 {
            return vec![];
        }

        let items = std::slice::from_raw_parts(
            buf.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
            item_count as usize,
        );

        let mut sums: std::collections::BTreeMap<u32, f64> = std::collections::BTreeMap::new();

        for item in items {
            if item.FmtValue.CStatus != 0 {
                continue;
            }
            let v = (item.FmtValue.Anonymous.doubleValue).clamp(0.0, 100.0);
            let name = String::from_utf16_lossy(item.szName.as_wide());
            let phys = extract_phys(&name);
            *sums.entry(phys).or_insert(0.0) += v;
        }

        sums.into_iter()
            .map(|(phys, pct)| GpuStats {
                name: gpu_name(phys),
                percent: pct.min(100.0),
            })
            .collect()
    }
}

fn hd_stats() -> Vec<HdStats> {
    unsafe {
        let drives = GetLogicalDrives();
        let mut result: Vec<HdStats> = Vec::new();

        for letter in 0..26u8 {
            if drives & (1 << letter) == 0 {
                continue;
            }
            let root: Vec<u16> = format!("{}:\\", (b'A' + letter) as char)
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let drive_type = GetDriveTypeW(windows::core::PCWSTR(root.as_ptr()));
            if drive_type != DRIVE_FIXED {
                continue;
            }

            let mount = format!("{}:", (b'A' + letter) as char);
            let path: Vec<u16> = format!("{}:\\", (b'A' + letter) as char)
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let mut free: u64 = 0;
            let mut total: u64 = 0;
            let mut free_total: u64 = 0;
            if GetDiskFreeSpaceExW(
                windows::core::PCWSTR(path.as_ptr()),
                Some(&mut free),
                Some(&mut total),
                Some(&mut free_total),
            )
            .is_err()
            {
                continue;
            }

            if total == 0 {
                continue;
            }
            let used = total.saturating_sub(free_total);
            let pct = (used as f64 / total as f64) * 100.0;
            result.push(HdStats {
                mount,
                used,
                total,
                percent: pct.clamp(0.0, 100.0),
            });
        }

        result
    }
}

struct NetSample {
    in_octets: u64,
    out_octets: u64,
    at: Instant,
}

static NET_PREV: Mutex<Option<std::collections::HashMap<u32, NetSample>>> = Mutex::new(None);

fn network_stats() -> Vec<NetworkStats> {
    unsafe {
        let mut table_ptr: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table_ptr).is_err() {
            return vec![];
        }
        if table_ptr.is_null() {
            return vec![];
        }

        let table = &*table_ptr;
        let num = table.NumEntries as usize;
        let rows = std::slice::from_raw_parts(table.Table.as_ptr(), num);

        let now = Instant::now();
        let mut guard = match NET_PREV.lock() {
            Ok(g) => g,
            Err(_) => {
                FreeMibTable(table_ptr as *const _ as *const _);
                return vec![];
            }
        };

        let prev_map = guard.get_or_insert_with(std::collections::HashMap::new);
        let mut result: Vec<NetworkStats> = Vec::new();

        for row in rows {
            // Skip pure loopback (type 24) and tunnel (131) interfaces
            if row.Type == 24 || row.Type == 131 {
                continue;
            }

            // Get adapter name from Alias
            let alias: String = row.Alias
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8 as char)
                .collect();
            let name = if alias.is_empty() {
                format!("if{}", row.InterfaceIndex)
            } else {
                alias.trim_end_matches('\0').to_string()
            };

            let idx = row.InterfaceIndex;
            let cur_in = row.InOctets;
            let cur_out = row.OutOctets;

            let (recv_bps, send_bps) = match prev_map.get(&idx) {
                Some(prev) => {
                    let elapsed = now.duration_since(prev.at).as_secs_f64();
                    if elapsed > 0.001 {
                        let din = cur_in.saturating_sub(prev.in_octets) as f64 / elapsed;
                        let dout = cur_out.saturating_sub(prev.out_octets) as f64 / elapsed;
                        (din, dout)
                    } else {
                        (0.0, 0.0)
                    }
                }
                None => (0.0, 0.0),
            };

            prev_map.insert(idx, NetSample {
                in_octets: cur_in,
                out_octets: cur_out,
                at: now,
            });

            result.push(NetworkStats {
                name,
                recv_bps,
                send_bps,
            });
        }

        FreeMibTable(table_ptr as *const _ as *const _);
        result
    }
}

#[tauri::command]
pub fn get_system_stats() -> SystemStats {
    let cpu = cpu_percent();
    let ghz = cpu_ghz();
    let (r_used, r_total, r_pct) = ram_stats();
    let gpu = gpu_stats();
    let hd = hd_stats();
    let network = network_stats();
    SystemStats {
        cpu_percent: cpu,
        cpu_ghz: ghz,
        ram_used: r_used,
        ram_total: r_total,
        ram_percent: r_pct,
        gpu,
        hd,
        network,
    }
}
