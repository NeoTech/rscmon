use anyhow::Result;
use std::time::Instant;

use super::NetworkMetrics;

// ---------------------------------------------------------------------------
// Network I/O via PDH  \Network Interface(*)\Bytes Received/sec + Bytes Sent/sec
// Physical adapters only — filter out loopback and well-known virtual names.
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod win_net {
    use super::*;
    use crate::windows::pdh::{pdh_add_counter, pdh_collect, pdh_open_query, pdh_query_value};

    pub struct NetPdhCollector {
        query: isize,
        /// (adapter_name, hcounter_rx, hcounter_tx, hrx_pkts, htx_pkts, hrx_err, htx_err)
        adapters: Vec<(
            String,
            isize,
            isize,
            isize,
            isize,
            isize,
            isize,
            u64,
            u64,
            Instant,
        )>,
    }

    // Names that identify virtual / loopback adapters — filtered out.
    fn is_virtual(name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        n.contains("loopback")
            || n.contains("isatap")
            || n.contains("teredo")
            || n.contains("6to4")
            || n.contains("pseudo")
            || n.contains("miniport")
            || n.contains("wsl")
            || n.contains("hyper-v")
            || n.contains("virtualbox")
            || n.contains("vmware")
            || n.contains("tap")
    }

    impl NetPdhCollector {
        pub fn new() -> Option<Self> {
            let query = pdh_open_query()?;
            let names = enumerate_net_instances(query)?;

            let mut adapters = Vec::new();
            for name in &names {
                if is_virtual(name) {
                    continue;
                }
                let rx_path = format!("\\Network Interface({name})\\Bytes Received/sec\0");
                let tx_path = format!("\\Network Interface({name})\\Bytes Sent/sec\0");
                let rx_pkt_path = format!("\\Network Interface({name})\\Packets Received/sec\0");
                let tx_pkt_path = format!("\\Network Interface({name})\\Packets Sent/sec\0");
                let rx_err_path = format!("\\Network Interface({name})\\Packets Received Errors\0");
                let tx_err_path = format!("\\Network Interface({name})\\Packets Outbound Errors\0");

                if let (Some(hrx), Some(htx)) = (
                    pdh_add_counter(query, &rx_path),
                    pdh_add_counter(query, &tx_path),
                ) {
                    // Optional counters — PDH may not have them on all adapters
                    let hrx_pkt = pdh_add_counter(query, &rx_pkt_path).unwrap_or(0);
                    let htx_pkt = pdh_add_counter(query, &tx_pkt_path).unwrap_or(0);
                    let hrx_err = pdh_add_counter(query, &rx_err_path).unwrap_or(0);
                    let htx_err = pdh_add_counter(query, &tx_err_path).unwrap_or(0);
                    adapters.push((
                        name.clone(),
                        hrx,
                        htx,
                        hrx_pkt,
                        htx_pkt,
                        hrx_err,
                        htx_err,
                        0u64,
                        0u64,
                        Instant::now(),
                    ));
                }
            }

            // Seed the counters with a first collection.
            pdh_collect(query);

            Some(Self { query, adapters })
        }

        pub fn collect(&mut self) -> Vec<NetworkMetrics> {
            pdh_collect(self.query);
            self.adapters
                .iter_mut()
                .filter_map(
                    |(
                        name,
                        hrx,
                        htx,
                        hrx_pkt,
                        htx_pkt,
                        hrx_err,
                        htx_err,
                        rx_total,
                        tx_total,
                        _last_time,
                    )| {
                        let rx = pdh_query_value(self.query, *hrx).unwrap_or(0.0);
                        let tx = pdh_query_value(self.query, *htx).unwrap_or(0.0);
                        let rx_pkts = if *hrx_pkt != 0 {
                            pdh_query_value(self.query, *hrx_pkt).unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        let tx_pkts = if *htx_pkt != 0 {
                            pdh_query_value(self.query, *htx_pkt).unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        let rx_err = if *hrx_err != 0 {
                            pdh_query_value(self.query, *hrx_err).unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        let tx_err = if *htx_err != 0 {
                            pdh_query_value(self.query, *htx_err).unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        // Accumulate totals (crude integration, ~0.5s intervals)
                        *rx_total = rx_total.saturating_add(rx as u64);
                        *tx_total = tx_total.saturating_add(tx as u64);
                        Some(NetworkMetrics {
                            name: name.clone(),
                            rx_bytes_sec: rx,
                            tx_bytes_sec: tx,
                            rx_packets_sec: rx_pkts,
                            tx_packets_sec: tx_pkts,
                            rx_errors_sec: rx_err,
                            tx_errors_sec: tx_err,
                            rx_bytes_total: *rx_total,
                            tx_bytes_total: *tx_total,
                        })
                    },
                )
                .collect()
        }
    }

    fn enumerate_net_instances(query: isize) -> Option<Vec<String>> {
        use windows::core::{PCWSTR, PWSTR};
        use windows::Win32::System::Performance::PdhEnumObjectItemsW;

        let object: Vec<u16> = "Network Interface\0".encode_utf16().collect();
        let mut counter_list_size: u32 = 0;
        let mut instance_list_size: u32 = 0;

        // First call to get required sizes
        unsafe {
            PdhEnumObjectItemsW(
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR(object.as_ptr()),
                PWSTR::null(),
                &mut counter_list_size,
                PWSTR::null(),
                &mut instance_list_size,
                windows::Win32::System::Performance::PERF_DETAIL_WIZARD,
                0,
            );
        }

        if instance_list_size == 0 {
            return None;
        }

        let mut counter_buf = vec![0u16; counter_list_size as usize + 1];
        let mut instance_buf = vec![0u16; instance_list_size as usize + 1];

        let ret = unsafe {
            PdhEnumObjectItemsW(
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR(object.as_ptr()),
                PWSTR(counter_buf.as_mut_ptr()),
                &mut counter_list_size,
                PWSTR(instance_buf.as_mut_ptr()),
                &mut instance_list_size,
                windows::Win32::System::Performance::PERF_DETAIL_WIZARD,
                0,
            )
        };

        if ret != 0 {
            return None;
        }

        // Instance list is a double-null-terminated list of null-terminated strings
        let mut names = Vec::new();
        let mut start = 0usize;
        for i in 0..instance_buf.len() {
            if instance_buf[i] == 0 {
                if i > start {
                    let s = String::from_utf16_lossy(&instance_buf[start..i]);
                    names.push(s);
                } else if i == start {
                    break; // double null — end of list
                }
                start = i + 1;
            }
        }

        // Suppress unused query warning
        let _ = query;

        Some(names)
    }
}

// ---------------------------------------------------------------------------
// Public collector
// ---------------------------------------------------------------------------

pub struct NetworkCollector {
    #[cfg(target_os = "windows")]
    inner: Option<win_net::NetPdhCollector>,
}

impl NetworkCollector {
    pub fn new() -> Result<Self> {
        Ok(Self {
            #[cfg(target_os = "windows")]
            inner: win_net::NetPdhCollector::new(),
        })
    }

    pub fn collect(&mut self) -> Vec<NetworkMetrics> {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref mut c) = self.inner {
                return c.collect();
            }
        }
        vec![]
    }
}
