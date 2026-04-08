//! GPU metrics collection.
//!
//! Priority order:
//! 1. NVML (NVIDIA only, richest data — enabled with `--features nvidia`)
//! 2. PDH GPU Engine counters (Windows, all vendors, utilization % only)
//! 3. Empty — show placeholder in UI

use anyhow::Result;

use super::{GpuMetrics, GpuVendor};

// ---------------------------------------------------------------------------
// NVIDIA via nvml-wrapper (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "nvidia")]
mod nvml_collector {
    use super::*;
    use nvml_wrapper::{enum_wrappers::device::TemperatureSensor, Nvml};
    use std::sync::OnceLock;

    static NVML: OnceLock<Option<Nvml>> = OnceLock::new();

    pub fn nvml() -> Option<&'static Nvml> {
        NVML.get_or_init(|| match Nvml::init() {
            Ok(n) => {
                log::info!("NVML initialised");
                Some(n)
            }
            Err(e) => {
                log::warn!("NVML unavailable: {e}");
                None
            }
        })
        .as_ref()
    }

    pub fn collect() -> Vec<GpuMetrics> {
        let Some(nvml) = nvml() else { return vec![] };
        let count = match nvml.device_count() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        (0..count)
            .filter_map(|i| {
                let dev = nvml.device_by_index(i).ok()?;
                let name = dev.name().unwrap_or_else(|_| format!("NVIDIA GPU {i}"));
                let util = dev.utilization_rates().ok();
                let util_pct = util.as_ref().map(|u| u.gpu as f32).unwrap_or(0.0);
                let mem_util_pct = util.map(|u| u.memory as f32);
                let mem = dev.memory_info().ok();
                let vram_used_bytes = mem.as_ref().map(|m| m.used);
                let vram_total_bytes = mem.map(|m| m.total);
                let temp = dev
                    .temperature(TemperatureSensor::Gpu)
                    .ok()
                    .map(|t| t as f32);
                let power_mw = dev.power_usage().ok();
                let clock_gpu_mhz = dev
                    .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Graphics)
                    .ok();
                let clock_mem_mhz = dev
                    .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Memory)
                    .ok();
                let fan_speed_pct = dev.fan_speed(0).ok();

                Some(GpuMetrics {
                    name,
                    vendor: GpuVendor::Nvidia,
                    util_pct,
                    mem_util_pct,
                    vram_used_bytes,
                    vram_total_bytes,
                    temp_celsius: temp,
                    power_mw,
                    clock_gpu_mhz,
                    clock_mem_mhz,
                    fan_speed_pct,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Public collector
// ---------------------------------------------------------------------------

pub struct GpuCollector {
    /// Last successfully collected snapshot (returned when nothing new yet).
    last: Vec<GpuMetrics>,
    /// PDH GPU Engine utilization — vendor-agnostic fallback (Windows only).
    #[cfg(target_os = "windows")]
    pdh: Option<crate::windows::pdh::GpuPdhCollector>,
}

impl GpuCollector {
    pub fn new() -> Result<Self> {
        // Eagerly trigger NVML init so the first collect() is fast.
        #[cfg(feature = "nvidia")]
        let _ = nvml_collector::nvml();

        #[cfg(target_os = "windows")]
        let pdh = crate::windows::pdh::GpuPdhCollector::new();

        Ok(Self {
            last: vec![],
            #[cfg(target_os = "windows")]
            pdh,
        })
    }

    pub fn collect(&mut self) -> Vec<GpuMetrics> {
        // ── 1. Try NVML (NVIDIA) ─────────────────────────────────────────────
        #[cfg(feature = "nvidia")]
        {
            let gpus = nvml_collector::collect();
            if !gpus.is_empty() {
                self.last = gpus;
                return self.last.clone();
            }
        }

        // ── 2. Try PDH GPU Engine counters (all vendors, Windows) ────────────
        #[cfg(target_os = "windows")]
        if let Some(pdh) = &mut self.pdh {
            let gpu_utils = pdh.collect();
            if !gpu_utils.is_empty() {
                let gpus: Vec<GpuMetrics> = gpu_utils
                    .into_iter()
                    .map(|(idx, pct)| GpuMetrics {
                        name: format!("GPU {idx}"),
                        vendor: GpuVendor::Unknown,
                        util_pct: pct,
                        ..Default::default()
                    })
                    .collect();
                self.last = gpus;
                return self.last.clone();
            }
        }

        // ── 3. No data — return last known (or empty on startup) ─────────────
        self.last.clone()
    }
}
