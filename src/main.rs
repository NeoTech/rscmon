mod app;
mod error;
mod metrics;
mod settings;
mod ui;
#[cfg(target_os = "windows")]
mod windows;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use metrics::{
    battery::BatteryCollector, cpu::CpuCollector, disk::DiskCollector, gpu::GpuCollector,
    memory::MemoryCollector, network::NetworkCollector, power::PowerCollector,
    process::ProcessCollector, temperature::TempCollector, MetricsSnapshot, TempReading,
};

fn main() -> Result<()> {
    env_logger::init();

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    let snapshot = metrics::new_shared_snapshot();
    let snapshot_bg = Arc::clone(&snapshot);

    thread::spawn(move || {
        if let Err(e) = run_collector(snapshot_bg) {
            log::error!("Metrics collector error: {e}");
        }
    });

    let result = app::App::new(snapshot, settings::load()).run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_collector(snapshot: Arc<Mutex<MetricsSnapshot>>) -> Result<()> {
    let mut cpu = CpuCollector::new()?;
    let mut mem = MemoryCollector::new()?;
    let mut gpu = GpuCollector::new()?;
    let mut disk = DiskCollector::new()?;
    let mut temp = TempCollector::new()?;
    let mut net = NetworkCollector::new()?;
    let mut proc = ProcessCollector::new()?;
    let battery = BatteryCollector::new()?;
    let mut power = PowerCollector::new()?;
    let temp_source = temp.source();

    thread::sleep(Duration::from_millis(600));

    loop {
        let cpu_data = cpu.collect();
        let mem_data = mem.collect();
        let gpu_data = gpu.collect();
        let disk_data = disk.collect();
        let volume_data = disk.collect_volumes();
        let (mut temp_data, fan_data, voltage_data, _) = temp.collect();
        let net_data = net.collect();
        let proc_data = proc.collect();
        let battery_data = battery.collect();
        let power_data = power.collect();

        // Augment temperature list with GPU temps from NVML
        for (i, gpu) in gpu_data.iter().enumerate() {
            if let Some(t) = gpu.temp_celsius {
                let label = if gpu.name.is_empty() {
                    format!("GPU {i}")
                } else {
                    gpu.name.clone()
                };
                let already_present = temp_data.iter().any(|r: &TempReading| {
                    r.sensor_type.to_lowercase().contains("temperature")
                        && r.label.to_lowercase().contains(&label.to_lowercase())
                });
                if !already_present {
                    temp_data.push(TempReading {
                        label,
                        value_celsius: t,
                        max_celsius: None,
                        sensor_type: "GPU Temperature".into(),
                    });
                }
            }
        }

        {
            let mut snap = snapshot.lock().unwrap();
            snap.cpu = cpu_data;
            snap.memory = mem_data;
            snap.gpus = gpu_data;
            snap.disks = disk_data;
            snap.volumes = volume_data;
            snap.networks = net_data;
            snap.temperatures = temp_data;
            snap.fans = fan_data;
            snap.voltages = voltage_data;
            snap.temp_source = temp_source.clone();
            snap.processes = proc_data;
            snap.battery = battery_data;
            snap.cpu_power = power_data;
            snap.collected_at = Some(std::time::Instant::now());
        }

        thread::sleep(Duration::from_millis(1000));
    }
}
