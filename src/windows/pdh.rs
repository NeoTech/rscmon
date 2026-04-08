//! PDH (Performance Data Helper) — GPU Engine utilization counters.
//!
//! Uses `\GPU Engine(*engtype_3D*)\Utilization Percentage` to provide
//! vendor-agnostic GPU 3D utilization on Windows.  Works for AMD, Intel, and
//! any other GPU that exposes the Windows GPU Engine performance counters.
//! NVIDIA is preferably handled by NVML (richer data) and falls back here.

#![allow(dead_code)]

use std::collections::HashMap;

use anyhow::Result;

// ---------------------------------------------------------------------------
// PDH GPU collector (Windows-only inner impl)
// ---------------------------------------------------------------------------

/// PDH error: buffer too small — expected on the size-probe call.
const PDH_MORE_DATA: u32 = 0x800007D2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Parses a stable "GPU key" from a PDH instance name.
///
/// Instance format:
/// `pid_X_luid_0xAAAAAAAA_0xBBBBBBBB_phys_N_eng_M_engtype_3D`
///
/// We key on `luid_0xAAAAAAAA_0xBBBBBBBB_phys_N` which uniquely identifies
/// one physical GPU even when the system has multiple adapters.
fn parse_gpu_key(name: &str) -> Option<String> {
    let luid_pos = name.find("_luid_")?;
    let after_luid = &name[luid_pos + 6..]; // skip "_luid_"

    let phys_pos = after_luid.find("_phys_")?;
    let after_phys = &after_luid[phys_pos + 6..]; // skip "_phys_"
    let phys_end = after_phys.find('_').unwrap_or(after_phys.len());

    Some(format!(
        "luid_{}_phys_{}",
        &after_luid[..phys_pos],
        &after_phys[..phys_end]
    ))
}

// ---------------------------------------------------------------------------
// Public stub for non-Windows builds
// ---------------------------------------------------------------------------

/// Placeholder so non-Windows code can reference the type.
#[cfg(not(target_os = "windows"))]
pub struct GpuPdhCollector;

#[cfg(not(target_os = "windows"))]
impl GpuPdhCollector {
    pub fn new() -> Option<Self> {
        None
    }

    pub fn collect(&mut self) -> Vec<(usize, f32)> {
        vec![]
    }
}

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub struct GpuPdhCollector {
    query: isize,
    counter: isize,
    /// True once a second `PdhCollectQueryData` has been made (rate counters
    /// need at least two samples before they return meaningful values).
    ready: bool,
    /// Stable ordered list of GPU keys; index = GPU display number.
    gpu_keys: Vec<String>,
}

#[cfg(target_os = "windows")]
impl GpuPdhCollector {
    /// Open a PDH query for 3D GPU engine utilization.
    /// Returns `None` if the counter path is unavailable (e.g. no GPU driver
    /// installed or running in a VM without GPU pass-through).
    pub fn new() -> Option<Self> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Performance::{
            PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhOpenQueryW,
        };

        unsafe {
            let mut query: isize = 0;
            let status = PdhOpenQueryW(PCWSTR::null(), 0, &mut query);
            if status != 0 {
                log::warn!("PdhOpenQueryW failed: {status:#x}");
                return None;
            }

            let path = wide(r"\GPU Engine(*engtype_3D*)\Utilization Percentage");
            let mut counter: isize = 0;
            let status = PdhAddEnglishCounterW(query, PCWSTR(path.as_ptr()), 0, &mut counter);
            if status != 0 {
                log::warn!(
                    "PdhAddEnglishCounterW failed: {status:#x} \
                     (GPU Engine counters may not be available)"
                );
                PdhCloseQuery(query);
                return None;
            }

            // Seed the rate counter — first sample is always stale.
            PdhCollectQueryData(query);

            log::info!("PDH GPU Engine collector initialised");
            Some(Self {
                query,
                counter,
                ready: false,
                gpu_keys: Vec::new(),
            })
        }
    }

    /// Collect one sample.  Returns `(gpu_index, util_pct)` pairs, where
    /// `gpu_index` is a stable 0-based integer assigned in sorted-key order.
    ///
    /// Returns an empty `Vec` on the first call (rate counters need two
    /// samples; the first call seeds the second).
    pub fn collect(&mut self) -> Vec<(usize, f32)> {
        use windows::Win32::System::Performance::{
            PdhCollectQueryData, PdhGetFormattedCounterArrayW, PDH_FMT_COUNTERVALUE_ITEM_W,
            PDH_FMT_DOUBLE,
        };

        unsafe {
            let status = PdhCollectQueryData(self.query);
            if status != 0 {
                log::debug!("PdhCollectQueryData: {status:#x}");
                return vec![];
            }

            // First call seeds the counter; return nothing yet.
            if !self.ready {
                self.ready = true;
                return vec![];
            }

            // ── Step 1: probe required buffer size ──────────────────────────
            let mut buf_size: u32 = 0;
            let mut item_count: u32 = 0;
            let s1 = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                None,
            );
            // PDH_MORE_DATA is the expected return when buffer is null/too small.
            if s1 != PDH_MORE_DATA || buf_size == 0 {
                if s1 != 0 && s1 != PDH_MORE_DATA {
                    log::debug!("PDH size probe unexpected status: {s1:#x}");
                }
                return vec![];
            }

            // ── Step 2: allocate buffer and fill ────────────────────────────
            let mut buffer: Vec<u8> = vec![0u8; buf_size as usize];
            let s2 = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                Some(buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
            );
            if s2 != 0 {
                log::debug!("PdhGetFormattedCounterArrayW: {s2:#x}");
                return vec![];
            }

            // ── Step 3: parse items, group by GPU key ────────────────────────
            let item_size = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            // (sum, count) per GPU key
            let mut sums: HashMap<String, (f64, u32)> = HashMap::new();

            for i in 0..item_count as usize {
                let item_ptr =
                    buffer.as_ptr().add(i * item_size) as *const PDH_FMT_COUNTERVALUE_ITEM_W;
                let item = &*item_ptr;

                // szName is a PWSTR pointing into the same buffer — safe while
                // `buffer` is alive.
                if item.szName.0.is_null() {
                    continue;
                }
                let name = {
                    let mut len = 0usize;
                    while *item.szName.0.add(len) != 0 {
                        len += 1;
                        if len > 512 {
                            break;
                        }
                    }
                    String::from_utf16_lossy(std::slice::from_raw_parts(item.szName.0, len))
                };

                let val = item.FmtValue.Anonymous.doubleValue;
                if !val.is_finite() || val < 0.0 {
                    continue;
                }

                if let Some(key) = parse_gpu_key(&name) {
                    let e = sums.entry(key).or_insert((0.0, 0));
                    e.0 += val;
                    e.1 += 1;
                }
            }

            if sums.is_empty() {
                return vec![];
            }

            // ── Step 4: stable key ordering → index ─────────────────────────
            for key in sums.keys() {
                if !self.gpu_keys.contains(key) {
                    self.gpu_keys.push(key.clone());
                }
            }
            self.gpu_keys.sort();

            self.gpu_keys
                .iter()
                .enumerate()
                .filter_map(|(idx, key)| {
                    sums.get(key).map(|&(sum, count)| {
                        let avg = if count > 0 { sum / count as f64 } else { 0.0 };
                        (idx, avg.clamp(0.0, 100.0) as f32)
                    })
                })
                .collect()
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for GpuPdhCollector {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Performance::PdhCloseQuery(self.query);
        }
    }
}

// PDH query handle is an opaque isize; safe to move to another thread as long
// as it is used from only one thread at a time (guaranteed by &mut self).
#[cfg(target_os = "windows")]
unsafe impl Send for GpuPdhCollector {}

// ---------------------------------------------------------------------------
// Thermal Zone PDH collector (Windows-only)
// ---------------------------------------------------------------------------
//
// Uses `\Thermal Zone Information(*)\Temperature` which is an instantaneous
// counter returning temperatures in tenths of Kelvin.
// `celsius = (value / 10.0) - 273.15`
//
// Instance names look like `\_TZ.THRM`, `\_TZ.TZ00`, etc.

/// A reading from the PDH Thermal Zone counter.
#[derive(Debug, Clone)]
pub struct ThermalZoneReading {
    /// ACPI instance name, e.g. `\_TZ.THRM`
    pub name: String,
    /// Temperature in degrees Celsius
    pub celsius: f32,
}

/// Placeholder so non-Windows code can reference the type.
#[cfg(not(target_os = "windows"))]
pub struct ThermalPdhCollector;

#[cfg(not(target_os = "windows"))]
impl ThermalPdhCollector {
    pub fn new() -> Option<Self> {
        None
    }
    pub fn collect(&mut self) -> Vec<ThermalZoneReading> {
        vec![]
    }
}

#[cfg(target_os = "windows")]
pub struct ThermalPdhCollector {
    query: isize,
    counter: isize,
}

#[cfg(target_os = "windows")]
impl ThermalPdhCollector {
    /// Open a PDH query for thermal zone temperatures.
    /// Returns `None` if the counter path is unavailable.
    pub fn new() -> Option<Self> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Performance::{
            PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhOpenQueryW,
        };

        unsafe {
            let mut query: isize = 0;
            let status = PdhOpenQueryW(PCWSTR::null(), 0, &mut query);
            if status != 0 {
                log::warn!("ThermalPdh: PdhOpenQueryW failed: {status:#x}");
                return None;
            }

            let path = wide(r"\Thermal Zone Information(*)\Temperature");
            let mut counter: isize = 0;
            let status = PdhAddEnglishCounterW(query, PCWSTR(path.as_ptr()), 0, &mut counter);
            if status != 0 {
                log::warn!(
                    "ThermalPdh: PdhAddEnglishCounterW failed: {status:#x} \
                     (Thermal Zone counters may not be available)"
                );
                PdhCloseQuery(query);
                return None;
            }

            // Instantaneous counter — no seed needed, but one collect to prime.
            PdhCollectQueryData(query);

            log::info!("PDH Thermal Zone collector initialised");
            Some(Self { query, counter })
        }
    }

    /// Collect one sample.  Returns one `ThermalZoneReading` per zone.
    pub fn collect(&mut self) -> Vec<ThermalZoneReading> {
        use windows::Win32::System::Performance::{
            PdhCollectQueryData, PdhGetFormattedCounterArrayW, PDH_FMT_COUNTERVALUE_ITEM_W,
            PDH_FMT_DOUBLE,
        };

        unsafe {
            let status = PdhCollectQueryData(self.query);
            if status != 0 {
                log::debug!("ThermalPdh: PdhCollectQueryData: {status:#x}");
                return vec![];
            }

            // Step 1: probe required buffer size
            let mut buf_size: u32 = 0;
            let mut item_count: u32 = 0;
            let s1 = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                None,
            );
            if s1 != PDH_MORE_DATA || buf_size == 0 {
                if s1 != 0 && s1 != PDH_MORE_DATA {
                    log::debug!("ThermalPdh: size probe status: {s1:#x}");
                }
                return vec![];
            }

            // Step 2: allocate and fill
            let mut buffer: Vec<u8> = vec![0u8; buf_size as usize];
            let s2 = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                Some(buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
            );
            if s2 != 0 {
                log::debug!("ThermalPdh: PdhGetFormattedCounterArrayW: {s2:#x}");
                return vec![];
            }

            // Step 3: parse items
            let item_size = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            let mut readings = Vec::new();

            for i in 0..item_count as usize {
                let item_ptr =
                    buffer.as_ptr().add(i * item_size) as *const PDH_FMT_COUNTERVALUE_ITEM_W;
                let item = &*item_ptr;

                if item.szName.0.is_null() {
                    continue;
                }
                let name = {
                    let mut len = 0usize;
                    while *item.szName.0.add(len) != 0 {
                        len += 1;
                        if len > 512 {
                            break;
                        }
                    }
                    String::from_utf16_lossy(std::slice::from_raw_parts(item.szName.0, len))
                };

                let raw = item.FmtValue.Anonymous.doubleValue;
                if !raw.is_finite() || raw <= 0.0 {
                    continue;
                }

                // Tenths of Kelvin → Celsius
                let celsius = (raw / 10.0) as f32 - 273.15;

                // Filter obviously garbage values
                if celsius < -50.0 || celsius > 200.0 {
                    log::debug!(
                        "ThermalPdh: discarding out-of-range reading '{name}' = {celsius:.1}°C"
                    );
                    continue;
                }

                readings.push(ThermalZoneReading { name, celsius });
            }

            readings
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ThermalPdhCollector {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Performance::PdhCloseQuery(self.query);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe impl Send for ThermalPdhCollector {}

// ---------------------------------------------------------------------------
// Generic PDH helper functions (Windows only)
// ---------------------------------------------------------------------------

/// Open a new PDH query. Returns `None` on failure.
#[cfg(target_os = "windows")]
pub fn pdh_open_query() -> Option<isize> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Performance::PdhOpenQueryW;
    let mut query: isize = 0;
    let status = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
    if status != 0 {
        return None;
    }
    Some(query)
}

/// Add a counter by English path. Returns `None` on failure.
#[cfg(target_os = "windows")]
pub fn pdh_add_counter(query: isize, path: &str) -> Option<isize> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Performance::PdhAddEnglishCounterW;
    let path_wide: Vec<u16> = path.encode_utf16().collect();
    let mut counter: isize = 0;
    let status =
        unsafe { PdhAddEnglishCounterW(query, PCWSTR(path_wide.as_ptr()), 0, &mut counter) };
    if status != 0 {
        return None;
    }
    Some(counter)
}

/// Trigger a collect on the query.
#[cfg(target_os = "windows")]
pub fn pdh_collect(query: isize) {
    unsafe {
        windows::Win32::System::Performance::PdhCollectQueryData(query);
    }
}

/// Read a single formatted double value from a counter. Returns `None` on failure.
#[cfg(target_os = "windows")]
pub fn pdh_query_value(_query: isize, counter: isize) -> Option<f64> {
    use windows::Win32::System::Performance::{PdhGetFormattedCounterValue, PDH_FMT_DOUBLE};
    let mut value = windows::Win32::System::Performance::PDH_FMT_COUNTERVALUE::default();
    let mut counter_type: u32 = 0;
    let status = unsafe {
        PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, Some(&mut counter_type), &mut value)
    };
    if status != 0 {
        return None;
    }
    let v = unsafe { value.Anonymous.doubleValue };
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

// Legacy stub kept for compatibility (unused after refactor)

pub struct PdhQuery {
    _handle: isize,
}

impl PdhQuery {
    pub fn new() -> Result<Self> {
        Ok(Self { _handle: 0 })
    }
}

impl Drop for PdhQuery {
    fn drop(&mut self) {
        // PdhCloseQuery would go here
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gpu_key_standard_instance() {
        let name = "pid_0_luid_0x00000000_0x0001234A_phys_0_eng_0_engtype_3D";
        let key = parse_gpu_key(name).unwrap();
        assert!(key.contains("luid_"), "key={key}");
        assert!(key.contains("phys_0"), "key={key}");
    }

    #[test]
    fn parse_gpu_key_two_gpus_differ() {
        let a = "pid_0_luid_0x00000000_0x00001111_phys_0_eng_0_engtype_3D";
        let b = "pid_0_luid_0x00000000_0x00002222_phys_0_eng_0_engtype_3D";
        assert_ne!(parse_gpu_key(a), parse_gpu_key(b));
    }

    #[test]
    fn parse_gpu_key_missing_luid_returns_none() {
        assert!(parse_gpu_key("engtype_3D").is_none());
    }
}
