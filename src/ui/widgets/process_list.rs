use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame,
};

use super::{fmt_bytes, pct_color, ViewMode};
use crate::{app::AppState, metrics::HistoryBuffer};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    mode: ViewMode,
    state: &AppState,
    history: &HistoryBuffer,
) {
    let snap = state.snapshot.lock().unwrap();
    let processes = snap.processes.clone();
    drop(snap);

    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let mode_tag = match mode {
        ViewMode::Default => "",
        ViewMode::Graph => " [graph]",
        ViewMode::Gauge => " [gauge]",
    };
    let title = Line::from(format!(
        " Process List ({} procs){mode_tag} ",
        processes.len()
    ));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    match mode {
        // ── Default: scrollable table with selection + detail panel ───────────
        ViewMode::Default => {
            if processes.is_empty() {
                frame.render_widget(
                    Paragraph::new(" No processes").style(Style::default().fg(Color::DarkGray)),
                    inner,
                );
                return;
            }

            let selected = state
                .process_selected
                .unwrap_or(0)
                .min(processes.len().saturating_sub(1));
            let detail_open = state.process_detail_open && focused;

            if detail_open {
                // Split area: list on top, detail panel below
                let detail_height = 8u16.min(inner.height / 2);
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(2), Constraint::Length(detail_height)])
                    .split(inner);
                render_process_list(
                    frame,
                    chunks[0],
                    &processes,
                    selected,
                    state.process_scroll,
                    true,
                );
                render_process_detail(frame, chunks[1], &processes, selected);
            } else {
                render_process_list(
                    frame,
                    inner,
                    &processes,
                    selected,
                    state.process_scroll,
                    focused,
                );
            }
        }

        // ── Graph: process count sparkline ────────────────────────────────────
        ViewMode::Graph => {
            let data: Vec<u64> = history.process_count.iter().copied().collect();
            let max = data.iter().copied().max().unwrap_or(1).max(1);
            frame.render_widget(
                Sparkline::default()
                    .data(&data)
                    .max(max)
                    .style(Style::default().fg(Color::Green))
                    .bar_set(symbols::bar::NINE_LEVELS),
                inner,
            );
        }

        // ── Gauge: top-5 CPU% bars ────────────────────────────────────────────
        ViewMode::Gauge => {
            let top: Vec<_> = processes.iter().take(5).collect();
            let rows_each = (inner.height / top.len().max(1) as u16).max(1);
            let mut y = inner.y;
            for proc in &top {
                if y >= inner.y + inner.height {
                    break;
                }
                let ratio = (proc.cpu_pct as f64 / 100.0).clamp(0.0, 1.0);
                for row in 0..rows_each {
                    if y >= inner.y + inner.height {
                        break;
                    }
                    let label = if row == rows_each / 2 {
                        format!(
                            " {} {:.1}%",
                            &proc.name[..proc.name.len().min(20)],
                            proc.cpu_pct
                        )
                    } else {
                        String::new()
                    };
                    frame.render_widget(
                        Gauge::default()
                            .gauge_style(Style::default().fg(pct_color(proc.cpu_pct as f64)))
                            .ratio(ratio)
                            .label(label),
                        Rect {
                            x: inner.x,
                            y,
                            width: inner.width,
                            height: 1,
                        },
                    );
                    y += 1;
                }
            }
        }
    }
}

fn render_process_list(
    frame: &mut Frame,
    area: Rect,
    processes: &[crate::metrics::ProcessMetrics],
    selected: usize,
    scroll: usize,
    show_hint: bool,
) {
    if area.height == 0 {
        return;
    }

    // Reserve 1 row for header, 1 for hint
    let hint_rows: u16 = if show_hint { 1 } else { 0 };
    let header_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let list_height = area.height.saturating_sub(1 + hint_rows);
    let list_area = Rect {
        y: area.y + 1,
        height: list_height,
        ..area
    };
    let hint_area = Rect {
        y: area.y + 1 + list_height,
        height: hint_rows,
        ..area
    };

    // Header
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                " {:>6}  {:<22} {:>7}  {:>9}  {:>7}",
                "PID", "Name", "CPU%", "Memory", "Threads"
            ),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ))),
        header_area,
    );

    if list_area.height == 0 {
        return;
    }

    // Auto-scroll so the selected row is visible
    let visible = list_area.height as usize;
    let start = if selected < scroll {
        selected
    } else if selected >= scroll + visible {
        selected + 1 - visible
    } else {
        scroll
    };

    let mut y = list_area.y;
    for (i, proc) in processes.iter().enumerate().skip(start).take(visible) {
        let is_sel = i == selected;
        let cpu_color = pct_color(proc.cpu_pct as f64);
        let fg = if is_sel { Color::Black } else { cpu_color };
        let bg = if is_sel { cpu_color } else { Color::Reset };
        let style = Style::default().fg(fg).bg(bg).add_modifier(if is_sel {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
        let line = format!(
            " {:>6}  {:<22} {:>6.1}%  {:>9}  {:>7}",
            proc.pid,
            &proc.name[..proc.name.len().min(22)],
            proc.cpu_pct,
            fmt_bytes(proc.mem_bytes),
            proc.thread_count,
        );
        frame.render_widget(
            Paragraph::new(line).style(style),
            Rect {
                x: list_area.x,
                y,
                width: list_area.width,
                height: 1,
            },
        );
        y += 1;
    }

    if show_hint && hint_area.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Enter: detail  j/k: select  Esc: close ",
                Style::default().fg(Color::DarkGray),
            )),
            hint_area,
        );
    }
}

fn render_process_detail(
    frame: &mut Frame,
    area: Rect,
    processes: &[crate::metrics::ProcessMetrics],
    selected: usize,
) {
    let proc = match processes.get(selected) {
        Some(p) => p,
        None => return,
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Line::from(format!(" PID {} — {} ", proc.pid, proc.name)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Format start time
    let start_str = if proc.start_time_secs > 0 {
        format_unix_time(proc.start_time_secs)
    } else {
        String::from("n/a")
    };

    let exe = if proc.exe_path.is_empty() {
        "<unknown>"
    } else {
        &proc.exe_path
    };
    // Truncate exe path if too wide
    let exe_display = if exe.len() > inner.width.saturating_sub(12) as usize {
        let start = exe
            .len()
            .saturating_sub(inner.width.saturating_sub(15) as usize);
        format!("…{}", &exe[start..])
    } else {
        exe.to_owned()
    };

    let lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(" CPU     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.1}%", proc.cpu_pct),
                Style::default()
                    .fg(pct_color(proc.cpu_pct as f64))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   Memory  ", Style::default().fg(Color::DarkGray)),
            Span::styled(fmt_bytes(proc.mem_bytes), Style::default().fg(Color::Cyan)),
            Span::styled("   Threads  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                proc.thread_count.to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Started ", Style::default().fg(Color::DarkGray)),
            Span::styled(start_str, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" Path    ", Style::default().fg(Color::DarkGray)),
            Span::styled(exe_display, Style::default().fg(Color::Yellow)),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Format a Unix timestamp (seconds since epoch) as "YYYY-MM-DD HH:MM:SS" on Windows.
fn format_unix_time(secs: u64) -> String {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::SYSTEMTIME;
        use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
        // Convert Unix epoch seconds to FILETIME (100-ns intervals since 1601-01-01)
        let ft_val = (secs as u64 + 11_644_473_600) * 10_000_000;
        let ft = windows::Win32::Foundation::FILETIME {
            dwLowDateTime: (ft_val & 0xFFFF_FFFF) as u32,
            dwHighDateTime: (ft_val >> 32) as u32,
        };
        unsafe {
            let mut utc = SYSTEMTIME::default();
            if FileTimeToSystemTime(&ft, &mut utc).is_ok() {
                let mut local = SYSTEMTIME::default();
                if SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_ok() {
                    return format!(
                        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                        local.wYear,
                        local.wMonth,
                        local.wDay,
                        local.wHour,
                        local.wMinute,
                        local.wSecond
                    );
                }
            }
        }
        format!("{secs}")
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("{secs}")
    }
}
