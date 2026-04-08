use anyhow::Result;
use sysinfo::{CpuRefreshKind, RefreshKind, System};

use super::CpuMetrics;

pub struct CpuCollector {
    sys: System,
}

impl CpuCollector {
    pub fn new() -> Result<Self> {
        let mut sys =
            System::new_with_specifics(RefreshKind::new().with_cpu(CpuRefreshKind::everything()));
        // First refresh seeds the counter; second refresh gives real deltas.
        sys.refresh_cpu_all();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_cpu_all();
        Ok(Self { sys })
    }

    pub fn collect(&mut self) -> CpuMetrics {
        self.sys.refresh_cpu_all();

        let global_pct = self.sys.global_cpu_usage();
        let per_core_pct: Vec<f32> = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        let core_count = self.sys.physical_core_count().unwrap_or(per_core_pct.len());
        let thread_count = per_core_pct.len();
        let model_name = self
            .sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_owned())
            .unwrap_or_else(|| "Unknown CPU".into());

        let freq_mhz_per_core: Vec<u64> = self.sys.cpus().iter().map(|c| c.frequency()).collect();

        CpuMetrics {
            global_pct,
            per_core_pct,
            freq_mhz_per_core,
            model_name,
            core_count,
            thread_count,
        }
    }
}
