pub mod battery;
pub mod cpu_cores;
pub mod cpu_frequency;
pub mod cpu_overview;
pub mod cpu_power;
pub mod disk;
pub mod disk_detail;
pub mod disk_space;
pub mod fans_voltages;
pub mod gpu;
pub mod gpu_list;
pub mod memory;
pub mod memory_detail;
pub mod network_io;
pub mod process_list;
pub mod temperatures;
pub mod uptime;

use ratatui::{layout::Rect, Frame};

use crate::{app::AppState, metrics::HistoryBuffer};

// ---------------------------------------------------------------------------
// Widget catalogue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WidgetKind {
    CpuOverview,
    CpuCores,
    CpuFrequency,
    CpuPower,
    GpuOverview,
    GpuDetail,
    GpuList,
    MemoryOverview,
    MemoryDetail,
    DiskIO,
    DiskDetail,
    DiskSpace,
    NetworkIO,
    FansVoltages,
    ProcessList,
    Battery,
    Uptime,
    Temperatures,
    SystemInfo,
}

impl WidgetKind {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            WidgetKind::CpuOverview => "CPU Overview",
            WidgetKind::CpuCores => "CPU Cores",
            WidgetKind::CpuFrequency => "CPU Frequency",
            WidgetKind::CpuPower => "CPU Power",
            WidgetKind::GpuOverview => "GPU Overview",
            WidgetKind::GpuDetail => "GPU Detail",
            WidgetKind::GpuList => "GPU List",
            WidgetKind::MemoryOverview => "Memory",
            WidgetKind::MemoryDetail => "Memory Detail",
            WidgetKind::DiskIO => "Disk I/O",
            WidgetKind::DiskDetail => "Disk Detail",
            WidgetKind::DiskSpace => "Disk Space",
            WidgetKind::NetworkIO => "Network I/O",
            WidgetKind::FansVoltages => "Fans & Voltages",
            WidgetKind::ProcessList => "Process List",
            WidgetKind::Battery => "Battery",
            WidgetKind::Uptime => "Uptime",
            WidgetKind::Temperatures => "Temperatures",
            WidgetKind::SystemInfo => "System Info",
        }
    }

    /// Returns the next widget in the cycle.
    pub fn next(&self) -> WidgetKind {
        match self {
            WidgetKind::CpuOverview => WidgetKind::CpuCores,
            WidgetKind::CpuCores => WidgetKind::CpuFrequency,
            WidgetKind::CpuFrequency => WidgetKind::CpuPower,
            WidgetKind::CpuPower => WidgetKind::GpuOverview,
            WidgetKind::GpuOverview => WidgetKind::GpuDetail,
            WidgetKind::GpuDetail => WidgetKind::GpuList,
            WidgetKind::GpuList => WidgetKind::MemoryOverview,
            WidgetKind::MemoryOverview => WidgetKind::MemoryDetail,
            WidgetKind::MemoryDetail => WidgetKind::DiskIO,
            WidgetKind::DiskIO => WidgetKind::DiskDetail,
            WidgetKind::DiskDetail => WidgetKind::DiskSpace,
            WidgetKind::DiskSpace => WidgetKind::NetworkIO,
            WidgetKind::NetworkIO => WidgetKind::FansVoltages,
            WidgetKind::FansVoltages => WidgetKind::ProcessList,
            WidgetKind::ProcessList => WidgetKind::Battery,
            WidgetKind::Battery => WidgetKind::Uptime,
            WidgetKind::Uptime => WidgetKind::Temperatures,
            WidgetKind::Temperatures => WidgetKind::SystemInfo,
            WidgetKind::SystemInfo => WidgetKind::CpuOverview,
        }
    }
}

// ---------------------------------------------------------------------------
// Visualization mode — per pane, cycles independently of widget type
// ---------------------------------------------------------------------------

/// Controls how a widget presents its data.
///
/// Not every widget uses every mode; widgets fall back gracefully when a mode
/// has no distinct meaning (e.g. `Temperatures` only has one sensible view).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ViewMode {
    /// Default: gauges / bars with a history sparkline below.
    #[default]
    Default,
    /// Sparkline / graph fills the full inner area — maximises history chart.
    Graph,
    /// Single large filled bar per metric — no history.
    Gauge,
}

impl ViewMode {
    pub fn next(self) -> ViewMode {
        match self {
            ViewMode::Default => ViewMode::Graph,
            ViewMode::Graph => ViewMode::Gauge,
            ViewMode::Gauge => ViewMode::Default,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Default => "default",
            ViewMode::Graph => "graph",
            ViewMode::Gauge => "gauge",
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch render to the right widget
// ---------------------------------------------------------------------------

pub fn render_widget(
    kind: &WidgetKind,
    mode: ViewMode,
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    state: &AppState,
    history: &HistoryBuffer,
) {
    match kind {
        WidgetKind::CpuOverview => cpu_overview::render(frame, area, focused, mode, state, history),
        WidgetKind::CpuCores => cpu_cores::render(frame, area, focused, mode, state, history),
        WidgetKind::CpuFrequency => {
            cpu_frequency::render(frame, area, focused, mode, state, history)
        }
        WidgetKind::CpuPower => cpu_power::render(frame, area, focused, mode, state, history),
        WidgetKind::GpuOverview => gpu::render_overview(frame, area, focused, mode, state, history),
        WidgetKind::GpuDetail => gpu::render_detail(frame, area, focused, mode, state, history),
        WidgetKind::GpuList => gpu_list::render(frame, area, focused, mode, state, history),
        WidgetKind::MemoryOverview => memory::render(frame, area, focused, mode, state, history),
        WidgetKind::MemoryDetail => {
            memory_detail::render(frame, area, focused, mode, state, history)
        }
        WidgetKind::DiskIO => disk::render(frame, area, focused, mode, state, history),
        WidgetKind::DiskDetail => disk_detail::render(frame, area, focused, mode, state, history),
        WidgetKind::DiskSpace => disk_space::render(frame, area, focused, mode, state, history),
        WidgetKind::NetworkIO => network_io::render(frame, area, focused, mode, state, history),
        WidgetKind::FansVoltages => {
            fans_voltages::render(frame, area, focused, mode, state, history)
        }
        WidgetKind::ProcessList => process_list::render(frame, area, focused, mode, state, history),
        WidgetKind::Battery => battery::render(frame, area, focused, mode, state, history),
        WidgetKind::Uptime => uptime::render(frame, area, focused, mode, state, history),
        WidgetKind::Temperatures => {
            temperatures::render(frame, area, focused, mode, state, history)
        }
        WidgetKind::SystemInfo => render_sysinfo(frame, area, focused, state),
    }
}

fn render_sysinfo(frame: &mut Frame, area: Rect, focused: bool, state: &AppState) {
    use ratatui::{
        style::{Color, Style},
        text::Line,
        widgets::{Block, Borders, Paragraph},
    };

    let snap = state.snapshot.lock().unwrap();
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let lines = vec![
        Line::from(format!("CPU:    {}", snap.cpu.model_name)),
        Line::from(format!(
            "Cores:  {} physical / {} logical",
            snap.cpu.core_count, snap.cpu.thread_count
        )),
        Line::from(format!(
            "RAM:    {:.1} GiB total",
            snap.memory.total_bytes as f64 / 1024.0 / 1024.0 / 1024.0
        )),
        Line::from(format!("GPUs:   {}", snap.gpus.len())),
        Line::from(format!("Disks:  {}", snap.disks.len())),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" System Info ");

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Shared helpers used across widgets
// ---------------------------------------------------------------------------

/// Format bytes into a human-readable string.
pub fn fmt_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= GB {
        format!("{:.2} GiB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MiB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KiB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Format bytes/sec into human-readable throughput.
pub fn fmt_bytes_sec(bps: f64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    if bps >= GB {
        format!("{:.2} GiB/s", bps / GB)
    } else if bps >= MB {
        format!("{:.1} MiB/s", bps / MB)
    } else if bps >= KB {
        format!("{:.0} KiB/s", bps / KB)
    } else {
        format!("{bps:.0} B/s")
    }
}

/// Choose a color based on a percentage value.
pub fn pct_color(pct: f64) -> ratatui::style::Color {
    use ratatui::style::Color;
    if pct >= 90.0 {
        Color::Red
    } else if pct >= 70.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}
