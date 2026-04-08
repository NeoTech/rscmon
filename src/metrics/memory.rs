use anyhow::Result;
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use super::MemoryMetrics;

pub struct MemoryCollector {
    sys: System,
}

impl MemoryCollector {
    pub fn new() -> Result<Self> {
        let mut sys = System::new_with_specifics(
            RefreshKind::new().with_memory(MemoryRefreshKind::everything()),
        );
        sys.refresh_memory();
        Ok(Self { sys })
    }

    pub fn collect(&mut self) -> MemoryMetrics {
        self.sys.refresh_memory();

        let total_bytes = self.sys.total_memory();
        let available_bytes = self.sys.available_memory();
        let used_bytes = total_bytes.saturating_sub(available_bytes);
        let swap_total_bytes = self.sys.total_swap();
        let swap_used_bytes = self.sys.used_swap();

        // Commit charge via Win32 K32GetPerformanceInfo
        let (commit_total_bytes, commit_limit_bytes) = commit_charge();

        MemoryMetrics {
            total_bytes,
            used_bytes,
            available_bytes,
            swap_total_bytes,
            swap_used_bytes,
            commit_total_bytes,
            commit_limit_bytes,
        }
    }
}

#[cfg(target_os = "windows")]
fn commit_charge() -> (u64, u64) {
    use windows::Win32::System::ProcessStatus::{K32GetPerformanceInfo, PERFORMANCE_INFORMATION};
    unsafe {
        let mut pi = PERFORMANCE_INFORMATION {
            cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32,
            ..Default::default()
        };
        let ok = K32GetPerformanceInfo(
            &mut pi,
            std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32,
        );
        if ok.as_bool() {
            let page = pi.PageSize as u64;
            let total = pi.CommitTotal as u64 * page;
            let limit = pi.CommitLimit as u64 * page;
            (total, limit)
        } else {
            (0, 0)
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn commit_charge() -> (u64, u64) {
    (0, 0)
}
