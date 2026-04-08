use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub mod battery;
pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod power;
pub mod process;
pub mod temperature;

pub const HISTORY_LEN: usize = 120;

// ---------------------------------------------------------------------------
// Per-metric data transferred from background thread to UI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct CpuMetrics {
    pub global_pct: f32,
    pub per_core_pct: Vec<f32>,
    /// Current frequency per logical core in MHz (0 if unavailable).
    pub freq_mhz_per_core: Vec<u64>,
    pub model_name: String,
    pub core_count: usize,
    pub thread_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub commit_total_bytes: u64,
    pub commit_limit_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

impl Default for GpuVendor {
    fn default() -> Self {
        GpuVendor::Unknown
    }
}

#[derive(Debug, Clone, Default)]
pub struct GpuMetrics {
    pub name: String,
    #[allow(dead_code)]
    pub vendor: GpuVendor,
    pub util_pct: f32,
    #[allow(dead_code)]
    pub mem_util_pct: Option<f32>,
    pub vram_used_bytes: Option<u64>,
    pub vram_total_bytes: Option<u64>,
    pub temp_celsius: Option<f32>,
    pub power_mw: Option<u32>,
    pub clock_gpu_mhz: Option<u32>,
    pub clock_mem_mhz: Option<u32>,
    pub fan_speed_pct: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct DiskMetrics {
    pub name: String,
    pub read_bytes_sec: f64,
    pub write_bytes_sec: f64,
    pub iops_read: f64,
    pub iops_write: f64,
    pub queue_depth: f64,
    pub read_latency_ms: f64,
    pub write_latency_ms: f64,
}

#[derive(Debug, Clone, Default)]
pub struct VolumeMetrics {
    /// e.g. "C:"
    pub mount_point: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkMetrics {
    pub name: String,
    pub rx_bytes_sec: f64,
    pub tx_bytes_sec: f64,
    /// Packets received per second
    pub rx_packets_sec: f64,
    /// Packets sent per second
    pub tx_packets_sec: f64,
    /// Receive errors per second
    pub rx_errors_sec: f64,
    /// Transmit errors per second
    pub tx_errors_sec: f64,
    /// Total bytes received (cumulative)
    pub rx_bytes_total: u64,
    /// Total bytes sent (cumulative)
    pub tx_bytes_total: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TempSource {
    LibreHardwareMonitor,
    Acpi,
    Unavailable,
}

impl Default for TempSource {
    fn default() -> Self {
        TempSource::Unavailable
    }
}

#[derive(Debug, Clone, Default)]
pub struct TempReading {
    pub label: String,
    pub value_celsius: f32,
    pub max_celsius: Option<f32>,
    pub sensor_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct FanReading {
    pub label: String,
    pub rpm: u32,
    pub max_rpm: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct VoltageReading {
    pub label: String,
    pub value_v: f32,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessMetrics {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub mem_bytes: u64,
    /// Number of threads
    pub thread_count: u32,
    /// Process start time as Unix timestamp (seconds), 0 if unavailable
    pub start_time_secs: u64,
    /// Full path to the executable, empty if unavailable / access denied
    pub exe_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct BatteryMetrics {
    pub present: bool,
    pub charge_pct: u8,
    pub ac_online: bool,
    pub charging: bool,
    pub time_remaining_secs: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum PowerSource {
    Lhm,
    Rapl,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, Default)]
pub struct CpuPowerMetrics {
    pub package_watts: Option<f32>,
    pub cores_watts: Option<f32>,
    pub source: PowerSource,
}

/// Snapshot of all metrics, produced by the background collector.
#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub gpus: Vec<GpuMetrics>,
    pub disks: Vec<DiskMetrics>,
    pub volumes: Vec<VolumeMetrics>,
    pub networks: Vec<NetworkMetrics>,
    pub temperatures: Vec<TempReading>,
    pub fans: Vec<FanReading>,
    pub voltages: Vec<VoltageReading>,
    pub temp_source: TempSource,
    pub processes: Vec<ProcessMetrics>,
    pub battery: BatteryMetrics,
    pub cpu_power: CpuPowerMetrics,
    pub collected_at: Option<Instant>,
}

// ---------------------------------------------------------------------------
// History buffers — kept on the main thread and updated each tick
// ---------------------------------------------------------------------------

pub struct HistoryBuffer {
    pub cpu_global: VecDeque<u64>,
    pub cpu_per_core: Vec<VecDeque<u64>>,
    /// Average frequency across all cores, in MHz
    pub cpu_freq_avg: VecDeque<u64>,
    pub mem_used_pct: VecDeque<u64>,
    pub swap_used_pct: VecDeque<u64>,
    /// per-GPU utilization %
    pub gpu_util: Vec<VecDeque<u64>>,
    /// per-GPU VRAM used %
    pub gpu_vram_pct: Vec<VecDeque<u64>>,
    /// per-GPU temperature in °C (scaled ×1 as u64)
    pub gpu_temp: Vec<VecDeque<u64>>,
    /// per-GPU power in milliwatts
    pub gpu_power_mw: Vec<VecDeque<u64>>,
    /// per-disk read bytes/sec (raw u64, scaled by max)
    pub disk_read: Vec<VecDeque<u64>>,
    /// per-disk write bytes/sec
    pub disk_write: Vec<VecDeque<u64>>,
    /// per-disk IOPS read
    pub disk_iops_read: Vec<VecDeque<u64>>,
    /// per-disk IOPS write
    pub disk_iops_write: Vec<VecDeque<u64>>,
    /// running maximums for dynamic sparkline scaling
    pub disk_read_max: Vec<u64>,
    pub disk_write_max: Vec<u64>,
    pub disk_iops_read_max: Vec<u64>,
    pub disk_iops_write_max: Vec<u64>,
    /// per-network adapter Rx bytes/sec
    pub net_rx: Vec<VecDeque<u64>>,
    /// per-network adapter Tx bytes/sec
    pub net_tx: Vec<VecDeque<u64>>,
    pub net_rx_max: Vec<u64>,
    pub net_tx_max: Vec<u64>,
    /// CPU package power in milliwatts
    pub cpu_power_mw: VecDeque<u64>,
    /// Battery charge percent
    pub battery_pct: VecDeque<u64>,
    /// Total process count over time
    pub process_count: VecDeque<u64>,
}

impl HistoryBuffer {
    pub fn new() -> Self {
        Self {
            cpu_global: VecDeque::with_capacity(HISTORY_LEN),
            cpu_per_core: Vec::new(),
            cpu_freq_avg: VecDeque::with_capacity(HISTORY_LEN),
            mem_used_pct: VecDeque::with_capacity(HISTORY_LEN),
            swap_used_pct: VecDeque::with_capacity(HISTORY_LEN),
            gpu_util: Vec::new(),
            gpu_vram_pct: Vec::new(),
            gpu_temp: Vec::new(),
            gpu_power_mw: Vec::new(),
            disk_read: Vec::new(),
            disk_write: Vec::new(),
            disk_iops_read: Vec::new(),
            disk_iops_write: Vec::new(),
            disk_read_max: Vec::new(),
            disk_write_max: Vec::new(),
            disk_iops_read_max: Vec::new(),
            disk_iops_write_max: Vec::new(),
            net_rx: Vec::new(),
            net_tx: Vec::new(),
            net_rx_max: Vec::new(),
            net_tx_max: Vec::new(),
            cpu_power_mw: VecDeque::with_capacity(HISTORY_LEN),
            battery_pct: VecDeque::with_capacity(HISTORY_LEN),
            process_count: VecDeque::with_capacity(HISTORY_LEN),
        }
    }

    fn push(buf: &mut VecDeque<u64>, val: u64) {
        if buf.len() >= HISTORY_LEN {
            buf.pop_front();
        }
        buf.push_back(val);
    }

    pub fn update(&mut self, snap: &MetricsSnapshot) {
        // CPU global
        Self::push(
            &mut self.cpu_global,
            snap.cpu.global_pct.clamp(0.0, 100.0) as u64,
        );

        // CPU per-core
        let nc = snap.cpu.per_core_pct.len();
        if self.cpu_per_core.len() != nc {
            self.cpu_per_core
                .resize_with(nc, || VecDeque::with_capacity(HISTORY_LEN));
        }
        for (i, &pct) in snap.cpu.per_core_pct.iter().enumerate() {
            Self::push(&mut self.cpu_per_core[i], pct.clamp(0.0, 100.0) as u64);
        }

        // CPU freq avg
        let freq_avg = if snap.cpu.freq_mhz_per_core.is_empty() {
            0
        } else {
            snap.cpu.freq_mhz_per_core.iter().sum::<u64>() / snap.cpu.freq_mhz_per_core.len() as u64
        };
        Self::push(&mut self.cpu_freq_avg, freq_avg);

        // Memory
        let mem_pct = if snap.memory.total_bytes > 0 {
            (snap.memory.used_bytes * 100 / snap.memory.total_bytes).min(100)
        } else {
            0
        };
        Self::push(&mut self.mem_used_pct, mem_pct);

        let swap_pct = if snap.memory.swap_total_bytes > 0 {
            (snap.memory.swap_used_bytes * 100 / snap.memory.swap_total_bytes).min(100)
        } else {
            0
        };
        Self::push(&mut self.swap_used_pct, swap_pct);

        // GPUs
        let ng = snap.gpus.len();
        if self.gpu_util.len() != ng {
            self.gpu_util
                .resize_with(ng, || VecDeque::with_capacity(HISTORY_LEN));
            self.gpu_vram_pct
                .resize_with(ng, || VecDeque::with_capacity(HISTORY_LEN));
            self.gpu_temp
                .resize_with(ng, || VecDeque::with_capacity(HISTORY_LEN));
            self.gpu_power_mw
                .resize_with(ng, || VecDeque::with_capacity(HISTORY_LEN));
        }
        for (i, gpu) in snap.gpus.iter().enumerate() {
            Self::push(&mut self.gpu_util[i], gpu.util_pct.clamp(0.0, 100.0) as u64);
            let vram_pct = match (gpu.vram_used_bytes, gpu.vram_total_bytes) {
                (Some(used), Some(total)) if total > 0 => (used * 100 / total).min(100),
                _ => 0,
            };
            Self::push(&mut self.gpu_vram_pct[i], vram_pct);
            Self::push(
                &mut self.gpu_temp[i],
                gpu.temp_celsius.unwrap_or(0.0) as u64,
            );
            Self::push(&mut self.gpu_power_mw[i], gpu.power_mw.unwrap_or(0) as u64);
        }

        // Disks
        let nd = snap.disks.len();
        if self.disk_read.len() != nd {
            self.disk_read
                .resize_with(nd, || VecDeque::with_capacity(HISTORY_LEN));
            self.disk_write
                .resize_with(nd, || VecDeque::with_capacity(HISTORY_LEN));
            self.disk_iops_read
                .resize_with(nd, || VecDeque::with_capacity(HISTORY_LEN));
            self.disk_iops_write
                .resize_with(nd, || VecDeque::with_capacity(HISTORY_LEN));
            self.disk_read_max.resize(nd, 1);
            self.disk_write_max.resize(nd, 1);
            self.disk_iops_read_max.resize(nd, 1);
            self.disk_iops_write_max.resize(nd, 1);
        }
        for (i, disk) in snap.disks.iter().enumerate() {
            let r = disk.read_bytes_sec as u64;
            let w = disk.write_bytes_sec as u64;
            let ir = disk.iops_read as u64;
            let iw = disk.iops_write as u64;
            self.disk_read_max[i] = self.disk_read_max[i].max(r).max(1);
            self.disk_write_max[i] = self.disk_write_max[i].max(w).max(1);
            self.disk_iops_read_max[i] = self.disk_iops_read_max[i].max(ir).max(1);
            self.disk_iops_write_max[i] = self.disk_iops_write_max[i].max(iw).max(1);
            Self::push(&mut self.disk_read[i], r);
            Self::push(&mut self.disk_write[i], w);
            Self::push(&mut self.disk_iops_read[i], ir);
            Self::push(&mut self.disk_iops_write[i], iw);
        }

        // Networks
        let nn = snap.networks.len();
        if self.net_rx.len() != nn {
            self.net_rx
                .resize_with(nn, || VecDeque::with_capacity(HISTORY_LEN));
            self.net_tx
                .resize_with(nn, || VecDeque::with_capacity(HISTORY_LEN));
            self.net_rx_max.resize(nn, 1);
            self.net_tx_max.resize(nn, 1);
        }
        for (i, net) in snap.networks.iter().enumerate() {
            let rx = net.rx_bytes_sec as u64;
            let tx = net.tx_bytes_sec as u64;
            self.net_rx_max[i] = self.net_rx_max[i].max(rx).max(1);
            self.net_tx_max[i] = self.net_tx_max[i].max(tx).max(1);
            Self::push(&mut self.net_rx[i], rx);
            Self::push(&mut self.net_tx[i], tx);
        }

        // CPU power
        let mw = snap
            .cpu_power
            .package_watts
            .map(|w| (w * 1000.0) as u64)
            .unwrap_or(0);
        Self::push(&mut self.cpu_power_mw, mw);

        // Battery
        Self::push(&mut self.battery_pct, snap.battery.charge_pct as u64);

        // Process count
        Self::push(&mut self.process_count, snap.processes.len() as u64);
    }
}

// ---------------------------------------------------------------------------
// Shared handle
// ---------------------------------------------------------------------------

pub type SharedSnapshot = Arc<Mutex<MetricsSnapshot>>;

pub fn new_shared_snapshot() -> SharedSnapshot {
    Arc::new(Mutex::new(MetricsSnapshot::default()))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snap_with_cpu(global: f32, per_core: Vec<f32>) -> MetricsSnapshot {
        let mut s = MetricsSnapshot::default();
        s.cpu.global_pct = global;
        s.cpu.per_core_pct = per_core;
        s
    }

    fn make_snap_with_mem(total: u64, used: u64) -> MetricsSnapshot {
        let mut s = MetricsSnapshot::default();
        s.memory.total_bytes = total;
        s.memory.used_bytes = used;
        s.memory.available_bytes = total.saturating_sub(used);
        s
    }

    #[test]
    fn history_cpu_push_and_len() {
        let mut h = HistoryBuffer::new();
        let snap = make_snap_with_cpu(50.0, vec![40.0, 60.0]);
        for _ in 0..10 {
            h.update(&snap);
        }
        assert_eq!(h.cpu_global.len(), 10);
        assert_eq!(h.cpu_global.back(), Some(&50));
        assert_eq!(h.cpu_per_core.len(), 2);
        assert_eq!(h.cpu_per_core[0].back(), Some(&40));
        assert_eq!(h.cpu_per_core[1].back(), Some(&60));
    }

    #[test]
    fn history_cpu_capped_at_max_len() {
        let mut h = HistoryBuffer::new();
        let snap = make_snap_with_cpu(100.0, vec![100.0]);
        for _ in 0..(HISTORY_LEN + 20) {
            h.update(&snap);
        }
        assert_eq!(h.cpu_global.len(), HISTORY_LEN);
    }

    #[test]
    fn history_mem_pct_computed_correctly() {
        let mut h = HistoryBuffer::new();
        let gib = 1024 * 1024 * 1024u64;
        let snap = make_snap_with_mem(4 * gib, 3 * gib);
        h.update(&snap);
        assert_eq!(h.mem_used_pct.back(), Some(&75));
    }

    #[test]
    fn history_mem_pct_zero_total() {
        let mut h = HistoryBuffer::new();
        let snap = make_snap_with_mem(0, 0);
        h.update(&snap);
        assert_eq!(h.mem_used_pct.back(), Some(&0));
    }

    #[test]
    fn history_disk_max_tracks_peak() {
        let mut h = HistoryBuffer::new();
        let mut snap = MetricsSnapshot::default();
        snap.disks.push(DiskMetrics {
            name: "Disk 0".into(),
            read_bytes_sec: 1_000_000.0,
            write_bytes_sec: 500_000.0,
            ..Default::default()
        });
        h.update(&snap);
        snap.disks[0].read_bytes_sec = 5_000_000.0;
        h.update(&snap);
        assert_eq!(h.disk_read_max[0], 5_000_000);
        snap.disks[0].read_bytes_sec = 100.0;
        h.update(&snap);
        assert_eq!(h.disk_read_max[0], 5_000_000);
    }

    #[test]
    fn history_gpu_pct_computed() {
        let mut h = HistoryBuffer::new();
        let mut snap = MetricsSnapshot::default();
        snap.gpus.push(GpuMetrics {
            util_pct: 80.0,
            vram_used_bytes: Some(4 * 1024 * 1024 * 1024),
            vram_total_bytes: Some(8 * 1024 * 1024 * 1024),
            ..Default::default()
        });
        h.update(&snap);
        assert_eq!(h.gpu_util[0].back(), Some(&80));
        assert_eq!(h.gpu_vram_pct[0].back(), Some(&50));
    }

    #[test]
    fn history_gpu_adapts_to_gpu_count_change() {
        let mut h = HistoryBuffer::new();
        let mut snap = MetricsSnapshot::default();
        snap.gpus.push(GpuMetrics {
            util_pct: 50.0,
            ..Default::default()
        });
        snap.gpus.push(GpuMetrics {
            util_pct: 70.0,
            ..Default::default()
        });
        h.update(&snap);
        assert_eq!(h.gpu_util.len(), 2);
        snap.gpus.truncate(1);
        h.update(&snap);
        assert_eq!(h.gpu_util.len(), 1);
    }

    #[test]
    fn history_network_tracks_max() {
        let mut h = HistoryBuffer::new();
        let mut snap = MetricsSnapshot::default();
        snap.networks.push(NetworkMetrics {
            name: "Ethernet".into(),
            rx_bytes_sec: 1_000_000.0,
            tx_bytes_sec: 500_000.0,
            ..Default::default()
        });
        h.update(&snap);
        snap.networks[0].rx_bytes_sec = 10_000_000.0;
        h.update(&snap);
        assert_eq!(h.net_rx_max[0], 10_000_000);
    }
}
