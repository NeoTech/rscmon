use anyhow::Result;
use std::time::Instant;

use super::{DiskMetrics, VolumeMetrics};

// ---------------------------------------------------------------------------
// Physical disk IO via DeviceIoControl(IOCTL_DISK_PERFORMANCE)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod win_disk {
    use super::*;
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_NO_BUFFERING, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::IO::DeviceIoControl,
    };

    // 0x70020 = CTL_CODE(IOCTL_DISK_BASE=7, 0x0008, METHOD_BUFFERED, FILE_READ_ACCESS)
    const IOCTL_DISK_PERFORMANCE: u32 = 0x0007_0020;

    /// Matches the Win32 DISK_PERFORMANCE layout (96 bytes on x64).
    #[repr(C)]
    #[derive(Default, Copy, Clone)]
    pub struct DiskPerf {
        pub bytes_read: i64,
        pub bytes_written: i64,
        pub read_time: i64,  // 100-ns units
        pub write_time: i64, // 100-ns units
        pub idle_time: i64,  // 100-ns units
        pub read_count: u32,
        pub write_count: u32,
        pub queue_depth: u32,
        pub split_count: u32,
        pub query_time: i64,
        pub storage_device_number: u32,
        pub storage_manager_name: [u16; 8],
        pub _pad: u32,
    }

    pub fn open_physical_drive(index: u32) -> Option<HANDLE> {
        let path: Vec<u16> = format!("\\\\.\\PhysicalDrive{index}\0")
            .encode_utf16()
            .collect();
        let h = unsafe {
            CreateFileW(
                windows::core::PCWSTR(path.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_NO_BUFFERING,
                None,
            )
        };
        h.ok()
    }

    pub fn query_disk_perf(handle: HANDLE) -> Option<DiskPerf> {
        let mut perf = DiskPerf::default();
        let mut returned: u32 = 0;
        let result = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_PERFORMANCE,
                None,
                0,
                Some(&mut perf as *mut _ as *mut _),
                std::mem::size_of::<DiskPerf>() as u32,
                Some(&mut returned),
                None,
            )
        };
        if result.is_ok() && returned >= 32 {
            Some(perf)
        } else {
            None
        }
    }

    pub struct DiskCollectorInner {
        /// (drive_index, last_perf, last_sample_time)
        drives: Vec<(u32, DiskPerf, Instant)>,
    }

    impl DiskCollectorInner {
        pub fn new() -> Self {
            let mut drives = Vec::new();
            for i in 0..16u32 {
                if let Some(h) = open_physical_drive(i) {
                    if let Some(p) = query_disk_perf(h) {
                        drives.push((i, p, Instant::now()));
                    }
                    unsafe {
                        let _ = CloseHandle(h);
                    }
                }
            }
            DiskCollectorInner { drives }
        }

        pub fn collect(&mut self) -> Vec<DiskMetrics> {
            let mut result = Vec::with_capacity(self.drives.len());
            for (idx, last_perf, last_time) in &mut self.drives {
                let Some(h) = open_physical_drive(*idx) else {
                    continue;
                };
                let metrics = if let Some(p) = query_disk_perf(h) {
                    let elapsed = last_time.elapsed().as_secs_f64().max(0.001);

                    let read_bps = (p.bytes_read - last_perf.bytes_read).max(0) as f64 / elapsed;
                    let write_bps =
                        (p.bytes_written - last_perf.bytes_written).max(0) as f64 / elapsed;

                    let delta_read_count = p.read_count.saturating_sub(last_perf.read_count) as f64;
                    let delta_write_count =
                        p.write_count.saturating_sub(last_perf.write_count) as f64;
                    let iops_read = delta_read_count / elapsed;
                    let iops_write = delta_write_count / elapsed;

                    // read_time / write_time are cumulative 100-ns units
                    let delta_read_time = (p.read_time - last_perf.read_time).max(0) as f64 * 1e-4; // → ms
                    let delta_write_time =
                        (p.write_time - last_perf.write_time).max(0) as f64 * 1e-4;
                    let read_latency_ms = if delta_read_count > 0.0 {
                        delta_read_time / delta_read_count
                    } else {
                        0.0
                    };
                    let write_latency_ms = if delta_write_count > 0.0 {
                        delta_write_time / delta_write_count
                    } else {
                        0.0
                    };

                    *last_perf = p;
                    *last_time = Instant::now();

                    DiskMetrics {
                        name: format!("Disk {idx}"),
                        read_bytes_sec: read_bps,
                        write_bytes_sec: write_bps,
                        iops_read,
                        iops_write,
                        queue_depth: p.queue_depth as f64,
                        read_latency_ms,
                        write_latency_ms,
                    }
                } else {
                    DiskMetrics {
                        name: format!("Disk {idx}"),
                        ..Default::default()
                    }
                };
                unsafe {
                    let _ = CloseHandle(h);
                }
                result.push(metrics);
            }
            result
        }
    }
}

// ---------------------------------------------------------------------------
// Volume free space via GetDiskFreeSpaceEx  (drive letters A–Z)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn collect_volumes() -> Vec<VolumeMetrics> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut volumes = Vec::new();
    for letter in b'A'..=b'Z' {
        let path_str = format!("{}:\\\0", letter as char);
        let path: Vec<u16> = path_str.encode_utf16().collect();
        let mut free_bytes_available: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                windows::core::PCWSTR(path.as_ptr()),
                Some(&mut free_bytes_available),
                Some(&mut total_bytes),
                Some(&mut total_free_bytes),
            )
        };
        if ok.is_ok() && total_bytes > 0 {
            let used_bytes = total_bytes.saturating_sub(total_free_bytes);
            volumes.push(VolumeMetrics {
                mount_point: format!("{}:", letter as char),
                total_bytes,
                free_bytes: total_free_bytes,
                used_bytes,
            });
        }
    }
    volumes
}

// ---------------------------------------------------------------------------
// Public collector
// ---------------------------------------------------------------------------

pub struct DiskCollector {
    #[cfg(target_os = "windows")]
    inner: win_disk::DiskCollectorInner,
}

impl DiskCollector {
    pub fn new() -> Result<Self> {
        Ok(Self {
            #[cfg(target_os = "windows")]
            inner: win_disk::DiskCollectorInner::new(),
        })
    }

    pub fn collect(&mut self) -> Vec<DiskMetrics> {
        #[cfg(target_os = "windows")]
        {
            self.inner.collect()
        }
        #[cfg(not(target_os = "windows"))]
        {
            vec![]
        }
    }

    pub fn collect_volumes(&self) -> Vec<VolumeMetrics> {
        #[cfg(target_os = "windows")]
        {
            collect_volumes()
        }
        #[cfg(not(target_os = "windows"))]
        {
            vec![]
        }
    }
}
