use anyhow::Result;

use super::ProcessMetrics;

// ---------------------------------------------------------------------------
// Process list via sysinfo — top-50 by CPU usage
// ---------------------------------------------------------------------------

pub struct ProcessCollector {
    sys: sysinfo::System,
}

impl ProcessCollector {
    pub fn new() -> Result<Self> {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        Ok(Self { sys })
    }

    pub fn collect(&mut self) -> Vec<ProcessMetrics> {
        self.sys
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let mut procs: Vec<ProcessMetrics> = self
            .sys
            .processes()
            .values()
            .map(|p| {
                let exe_path = p
                    .exe()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default();
                ProcessMetrics {
                    pid: p.pid().as_u32(),
                    name: p.name().to_string_lossy().into_owned(),
                    cpu_pct: p.cpu_usage(),
                    mem_bytes: p.memory(),
                    thread_count: p.tasks().map(|t| t.len() as u32).unwrap_or(0),
                    start_time_secs: p.start_time(),
                    exe_path,
                }
            })
            .collect();

        // Sort descending by CPU%, then by memory as tiebreaker
        procs.sort_by(|a, b| {
            b.cpu_pct
                .partial_cmp(&a.cpu_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.mem_bytes.cmp(&a.mem_bytes))
        });

        procs.truncate(50);
        procs
    }
}
