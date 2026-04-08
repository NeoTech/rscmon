use anyhow::Result;

use super::CpuPowerMetrics;

// ---------------------------------------------------------------------------
// CPU power collection
// Priority 1: LibreHardwareMonitor WMI  SensorType='Power'
// Priority 2: PDH \Power Meter(*)\Power  (RAPL fallback on supported Intel CPUs)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod win_power {
    use super::*;

    // ── LHM WMI path ────────────────────────────────────────────────────────

    pub fn collect_lhm(con: &wmi::WMIConnection) -> Option<CpuPowerMetrics> {
        let rows: Vec<std::collections::HashMap<String, wmi::Variant>> = con
            .raw_query("SELECT Name, Value FROM Sensor WHERE SensorType='Power'")
            .ok()?;

        let mut package_watts: Option<f32> = None;
        let mut cores_watts: Option<f32> = None;

        for row in rows {
            let name = row
                .get("Name")
                .and_then(variant_as_str)
                .unwrap_or_default()
                .to_lowercase();
            let value = row.get("Value").and_then(variant_as_f32);
            if let Some(w) = value {
                if name.contains("package") || name.contains("cpu package") {
                    package_watts = Some(w);
                } else if name.contains("core") && !name.contains("uncore") {
                    cores_watts = Some(cores_watts.unwrap_or(0.0) + w);
                }
            }
        }

        if package_watts.is_none() && cores_watts.is_none() {
            return None;
        }
        Some(CpuPowerMetrics {
            package_watts,
            cores_watts,
            source: crate::metrics::PowerSource::Lhm,
        })
    }

    // ── PDH RAPL fallback ────────────────────────────────────────────────────

    pub struct RaplCollector {
        query: isize,
        hcounter: isize,
    }

    impl RaplCollector {
        pub fn new() -> Option<Self> {
            use crate::windows::pdh::{pdh_add_counter, pdh_collect, pdh_open_query};
            let query = pdh_open_query()?;
            let path = "\\Power Meter(*)\\Power\0";
            let hcounter = pdh_add_counter(query, path)?;
            pdh_collect(query); // seed
            Some(Self { query, hcounter })
        }

        pub fn collect(&mut self) -> Option<CpuPowerMetrics> {
            use crate::windows::pdh::{pdh_collect, pdh_query_value};
            pdh_collect(self.query);
            let w = pdh_query_value(self.query, self.hcounter)?;
            if w <= 0.0 {
                return None;
            }
            Some(CpuPowerMetrics {
                package_watts: Some(w as f32),
                cores_watts: None,
                source: crate::metrics::PowerSource::Rapl,
            })
        }
    }

    fn variant_as_str(v: &wmi::Variant) -> Option<String> {
        match v {
            wmi::Variant::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn variant_as_f32(v: &wmi::Variant) -> Option<f32> {
        match v {
            wmi::Variant::R4(f) => Some(*f),
            wmi::Variant::R8(f) => Some(*f as f32),
            wmi::Variant::I4(i) => Some(*i as f32),
            wmi::Variant::UI4(u) => Some(*u as f32),
            _ => None,
        }
    }

    // ── Backend enum ─────────────────────────────────────────────────────────

    pub enum PowerBackend {
        Lhm(wmi::WMIConnection),
        Rapl(RaplCollector),
        Unavailable,
    }

    pub fn detect() -> PowerBackend {
        // Try LHM first
        if let Ok(com) = wmi::COMLibrary::new() {
            if let Ok(con) =
                wmi::WMIConnection::with_namespace_path("ROOT\\LibreHardwareMonitor", com)
            {
                // Quick probe
                let probe: Result<Vec<std::collections::HashMap<String, wmi::Variant>>, _> =
                    con.raw_query("SELECT Value FROM Sensor WHERE SensorType='Power'");
                if probe.map(|r| !r.is_empty()).unwrap_or(false) {
                    log::info!("CPU power source: LibreHardwareMonitor WMI");
                    return PowerBackend::Lhm(con);
                }
            }
        }
        // Try PDH RAPL
        if let Some(rapl) = RaplCollector::new() {
            log::info!("CPU power source: PDH RAPL Power Meter");
            return PowerBackend::Rapl(rapl);
        }
        log::warn!("No CPU power source available");
        PowerBackend::Unavailable
    }
}

// ---------------------------------------------------------------------------
// Public collector
// ---------------------------------------------------------------------------

pub struct PowerCollector {
    #[cfg(target_os = "windows")]
    backend: win_power::PowerBackend,
}

impl PowerCollector {
    pub fn new() -> Result<Self> {
        Ok(Self {
            #[cfg(target_os = "windows")]
            backend: win_power::detect(),
        })
    }

    pub fn collect(&mut self) -> CpuPowerMetrics {
        #[cfg(target_os = "windows")]
        {
            use win_power::PowerBackend;
            match &mut self.backend {
                PowerBackend::Lhm(con) => win_power::collect_lhm(con).unwrap_or_default(),
                PowerBackend::Rapl(rapl) => rapl.collect().unwrap_or_default(),
                PowerBackend::Unavailable => CpuPowerMetrics::default(),
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            CpuPowerMetrics::default()
        }
    }
}
