use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::ViewMode;
use crate::{app::AppState, metrics::HistoryBuffer};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    _mode: ViewMode, // Uptime ignores mode — single view always
    state: &AppState,
    _history: &HistoryBuffer,
) {
    let snap = state.snapshot.lock().unwrap();
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let (uptime_str, boot_str) = get_uptime();

    let title = Line::from(format!(" Uptime — {uptime_str} "));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let proc_count = snap.processes.len();
    let lines = vec![
        format!(" Uptime:    {uptime_str}"),
        format!(" Boot time: {boot_str}"),
        format!(" Processes: {proc_count}"),
    ];

    let mut y = inner.y;
    let mut remaining = inner.height;
    for line in &lines {
        if remaining == 0 {
            break;
        }
        frame.render_widget(
            Paragraph::new(line.as_str()).style(Style::default().fg(Color::White)),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );
        y += 1;
        remaining -= 1;
    }
}

/// Returns (uptime_string, boot_time_string).
fn get_uptime() -> (String, String) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};
        let tick_ms = unsafe { GetTickCount64() };
        let total_secs = tick_ms / 1000;
        let d = total_secs / 86400;
        let h = (total_secs % 86400) / 3600;
        let m = (total_secs % 3600) / 60;
        let s = total_secs % 60;
        let uptime = if d > 0 {
            format!("{d}d {h:02}:{m:02}:{s:02}")
        } else {
            format!("{h:02}:{m:02}:{s:02}")
        };

        // Compute boot time = now - uptime
        let boot = unsafe {
            let now = GetLocalTime();
            // Best-effort: just show the current system time minus uptime as a string.
            // For an exact boot timestamp we'd need SystemTimeToFileTime math.
            let _ = now;
            format!("(~{d}d {h:02}h ago)")
        };

        (uptime, boot)
    }
    #[cfg(not(target_os = "windows"))]
    {
        ("N/A".into(), "N/A".into())
    }
}
