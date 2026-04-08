use anyhow::Result;

use super::{FanReading, TempReading, TempSource, VoltageReading};

// ---------------------------------------------------------------------------
// Windows temperature collection
// Priority: LibreHardwareMonitor WMI → ACPI WMI → PDH Thermal Zone → Unavailable
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod win_temp {
    use super::*;
    use wmi::{COMLibrary, WMIConnection};

    // LHM and ACPI structs kept for reference; parsed via raw WMI map instead.
    #[allow(dead_code, non_snake_case)]
    struct LhmSensor {
        Name: String,
        Value: f32,
        Max: Option<f32>,
        SensorType: String,
        Parent: Option<String>,
    }

    #[allow(dead_code, non_snake_case)]
    struct AcpiThermal {
        CurrentTemperature: u32,
        CriticalTripPoint: Option<u32>,
        InstanceName: Option<String>,
    }

    pub enum TempBackend {
        LibreHardwareMonitor(WMIConnection),
        Acpi(WMIConnection),
        ThermalPdh(crate::windows::pdh::ThermalPdhCollector),
        Unavailable,
    }

    fn try_lhm(_com: &COMLibrary) -> Option<WMIConnection> {
        // wmi v0.14: COMLibrary is not Clone, so we can't pass &COMLibrary to with_namespace_path.
        // We detect LHM by probing; actual connection built fresh each time.
        // Try to connect to the LHM WMI namespace.
        // Note: wmi v0.14 WMIConnection::with_namespace_path takes ownership of COMLibrary.
        // We create a fresh COMLibrary for detection.
        let fresh_com = COMLibrary::new().ok()?;
        let con =
            WMIConnection::with_namespace_path("ROOT\\LibreHardwareMonitor", fresh_com).ok()?;
        // Probe with a limit-1 query to confirm it works
        let _probe: Vec<std::collections::HashMap<String, wmi::Variant>> = con
            .raw_query("SELECT Name FROM Sensor WHERE SensorType='Temperature'")
            .ok()?;
        Some(con)
    }

    fn try_acpi() -> Option<WMIConnection> {
        let com = COMLibrary::new().ok()?;
        WMIConnection::with_namespace_path("ROOT\\WMI", com).ok()
    }

    pub fn detect() -> TempBackend {
        if let Ok(com) = COMLibrary::new() {
            if let Some(con) = try_lhm(&com) {
                log::info!("Temperature source: LibreHardwareMonitor WMI");
                return TempBackend::LibreHardwareMonitor(con);
            }
        }
        // Try ACPI WMI — probe whether it actually returns any rows.
        if let Some(con) = try_acpi() {
            if let Some(result) = collect_acpi(&con) {
                if !result.0.is_empty() {
                    log::info!("Temperature source: ACPI WMI ({} zones)", result.0.len());
                    return TempBackend::Acpi(con);
                }
            }
            log::info!("ACPI WMI connected but returned no zones; trying PDH thermal counters");
        }
        // Try PDH \Thermal Zone Information(*)\Temperature
        if let Some(pdh) = crate::windows::pdh::ThermalPdhCollector::new() {
            log::info!("Temperature source: PDH Thermal Zone counters");
            return TempBackend::ThermalPdh(pdh);
        }
        log::warn!("No temperature source available");
        TempBackend::Unavailable
    }

    pub fn collect_lhm(
        con: &WMIConnection,
    ) -> Option<(
        Vec<TempReading>,
        Vec<FanReading>,
        Vec<VoltageReading>,
        TempSource,
    )> {
        // Query all sensor types we care about in one go
        let all_rows: Vec<std::collections::HashMap<String, wmi::Variant>> = con
            .raw_query("SELECT Name, Value, Max, SensorType FROM Sensor WHERE SensorType='Temperature' OR SensorType='Fan' OR SensorType='Voltage'")
            .ok()?;

        let mut temps = Vec::new();
        let mut fans = Vec::new();
        let mut voltages = Vec::new();

        for map in all_rows {
            let name = match map.get("Name").and_then(variant_as_str) {
                Some(n) => n,
                None => continue,
            };
            let sensor_type = map
                .get("SensorType")
                .and_then(variant_as_str)
                .unwrap_or_default();
            let value_f32 = map.get("Value").and_then(variant_as_f32);
            let max_f32 = map.get("Max").and_then(variant_as_f32);

            match sensor_type.as_str() {
                "Temperature" => {
                    if let Some(v) = value_f32 {
                        temps.push(TempReading {
                            label: name,
                            value_celsius: v,
                            max_celsius: max_f32,
                            sensor_type: sensor_type.clone(),
                        });
                    }
                }
                "Fan" => {
                    if let Some(v) = value_f32 {
                        fans.push(FanReading {
                            label: name,
                            rpm: v as u32,
                            max_rpm: max_f32.map(|m| m as u32),
                        });
                    }
                }
                "Voltage" => {
                    if let Some(v) = value_f32 {
                        voltages.push(VoltageReading {
                            label: name,
                            value_v: v,
                        });
                    }
                }
                _ => {}
            }
        }

        if temps.is_empty() && fans.is_empty() && voltages.is_empty() {
            None
        } else {
            Some((temps, fans, voltages, TempSource::LibreHardwareMonitor))
        }
    }

    pub fn collect_acpi(con: &WMIConnection) -> Option<(Vec<TempReading>, TempSource)> {
        let results: Vec<std::collections::HashMap<String, wmi::Variant>> = con
            .raw_query("SELECT CurrentTemperature, CriticalTripPoint, InstanceName FROM MSAcpi_ThermalZoneTemperature")
            .ok()?;

        let readings: Vec<TempReading> = results
            .into_iter()
            .filter_map(|map| {
                let raw = map.get("CurrentTemperature").and_then(variant_as_u32)?;
                let celsius = (raw as f32 / 10.0) - 273.15;
                // Discard garbage values from unimplemented ACPI zones
                if celsius < -50.0 || celsius > 200.0 {
                    log::debug!("ACPI: discarding out-of-range reading {celsius:.1}°C");
                    return None;
                }
                let max = map
                    .get("CriticalTripPoint")
                    .and_then(variant_as_u32)
                    .map(|v| (v as f32 / 10.0) - 273.15)
                    .filter(|&m| m > -50.0 && m < 200.0);
                let label = map
                    .get("InstanceName")
                    .and_then(variant_as_str)
                    .unwrap_or_else(|| "Thermal Zone".into());
                Some(TempReading {
                    label,
                    value_celsius: celsius,
                    max_celsius: max,
                    sensor_type: "Temperature".into(),
                })
            })
            .collect();

        if readings.is_empty() {
            None
        } else {
            Some((readings, TempSource::Acpi))
        }
    }

    pub fn collect_thermal_pdh(
        col: &mut crate::windows::pdh::ThermalPdhCollector,
    ) -> (Vec<TempReading>, TempSource) {
        let raw = col.collect();
        let readings = raw
            .into_iter()
            .map(|r| TempReading {
                label: r.name,
                value_celsius: r.celsius,
                max_celsius: None,
                sensor_type: "Temperature".into(),
            })
            .collect();
        (readings, TempSource::Acpi) // report as Acpi source (PDH is the same data)
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
            wmi::Variant::I2(i) => Some(*i as f32),
            wmi::Variant::UI2(u) => Some(*u as f32),
            wmi::Variant::I1(i) => Some(*i as f32),
            wmi::Variant::UI1(u) => Some(*u as f32),
            _ => None,
        }
    }

    fn variant_as_u32(v: &wmi::Variant) -> Option<u32> {
        match v {
            wmi::Variant::UI4(u) => Some(*u),
            wmi::Variant::I4(i) => Some(*i as u32),
            wmi::Variant::UI2(u) => Some(*u as u32),
            wmi::Variant::I2(i) => Some(*i as u32),
            wmi::Variant::UI1(u) => Some(*u as u32),
            wmi::Variant::I1(i) => Some(*i as u32),
            wmi::Variant::UI8(u) => Some(*u as u32),
            wmi::Variant::I8(i) => Some(*i as u32),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Public collector
// ---------------------------------------------------------------------------

pub struct TempCollector {
    #[cfg(target_os = "windows")]
    backend: win_temp::TempBackend,
}

impl TempCollector {
    pub fn new() -> Result<Self> {
        Ok(Self {
            #[cfg(target_os = "windows")]
            backend: win_temp::detect(),
        })
    }

    pub fn collect(
        &mut self,
    ) -> (
        Vec<TempReading>,
        Vec<FanReading>,
        Vec<VoltageReading>,
        TempSource,
    ) {
        #[cfg(target_os = "windows")]
        {
            use win_temp::TempBackend;
            match &mut self.backend {
                TempBackend::LibreHardwareMonitor(con) => win_temp::collect_lhm(con)
                    .unwrap_or_else(|| (vec![], vec![], vec![], TempSource::LibreHardwareMonitor)),
                TempBackend::Acpi(con) => {
                    let (temps, src) =
                        win_temp::collect_acpi(con).unwrap_or_else(|| (vec![], TempSource::Acpi));
                    (temps, vec![], vec![], src)
                }
                TempBackend::ThermalPdh(col) => {
                    let (temps, src) = win_temp::collect_thermal_pdh(col);
                    (temps, vec![], vec![], src)
                }
                TempBackend::Unavailable => (vec![], vec![], vec![], TempSource::Unavailable),
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            (vec![], vec![], vec![], TempSource::Unavailable)
        }
    }

    pub fn source(&self) -> TempSource {
        #[cfg(target_os = "windows")]
        {
            use win_temp::TempBackend;
            match &self.backend {
                TempBackend::LibreHardwareMonitor(_) => TempSource::LibreHardwareMonitor,
                TempBackend::Acpi(_) => TempSource::Acpi,
                TempBackend::ThermalPdh(_) => TempSource::Acpi,
                TempBackend::Unavailable => TempSource::Unavailable,
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            TempSource::Unavailable
        }
    }
}
