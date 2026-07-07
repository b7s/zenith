use serde::{Deserialize, Serialize};
use windows::Win32::System::Power::GetSystemPowerStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub percent: i32,
    pub charging: bool,
    pub has_battery: bool,
}

#[tauri::command]
pub fn get_battery_status() -> BatteryInfo {
    unsafe {
        let mut status = std::mem::zeroed();
        if GetSystemPowerStatus(&mut status).is_ok() {
            let has_battery = status.BatteryFlag & 128 == 0;
            let percent = if status.BatteryLifePercent == 255 {
                -1
            } else {
                status.BatteryLifePercent as i32
            };
            let charging = status.ACLineStatus == 1;
            BatteryInfo {
                percent,
                charging,
                has_battery,
            }
        } else {
            BatteryInfo {
                percent: -1,
                charging: false,
                has_battery: false,
            }
        }
    }
}
