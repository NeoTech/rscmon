use anyhow::Result;

use super::BatteryMetrics;

// ---------------------------------------------------------------------------
// Battery status via Win32 GetSystemPowerStatus
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod win_battery {
    use super::*;
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    pub fn collect() -> BatteryMetrics {
        let mut status = SYSTEM_POWER_STATUS::default();
        let ok = unsafe { GetSystemPowerStatus(&mut status) };
        if ok.is_err() {
            return BatteryMetrics::default();
        }

        // BatteryFlag: 128 = no battery present
        let present = status.BatteryFlag != 128 && status.BatteryFlag != 255;
        if !present {
            return BatteryMetrics {
                present: false,
                ..Default::default()
            };
        }

        let charge_pct = if status.BatteryLifePercent == 255 {
            0
        } else {
            status.BatteryLifePercent
        };

        let ac_online = status.ACLineStatus == 1;

        // BatteryFlag bits: 8 = charging
        let charging = (status.BatteryFlag & 8) != 0;

        // BatteryLifeTime: seconds remaining; 0xFFFFFFFF = unknown
        let time_remaining_secs = if status.BatteryLifeTime == u32::MAX {
            None
        } else {
            Some(status.BatteryLifeTime)
        };

        BatteryMetrics {
            present: true,
            charge_pct,
            ac_online,
            charging,
            time_remaining_secs,
        }
    }
}

// ---------------------------------------------------------------------------
// Public collector
// ---------------------------------------------------------------------------

pub struct BatteryCollector;

impl BatteryCollector {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn collect(&self) -> BatteryMetrics {
        #[cfg(target_os = "windows")]
        {
            win_battery::collect()
        }
        #[cfg(not(target_os = "windows"))]
        {
            BatteryMetrics::default()
        }
    }
}
