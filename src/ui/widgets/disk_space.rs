use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame,
};

use super::{fmt_bytes, fmt_bytes_sec, pct_color, ViewMode};
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
    let volumes = &snap.volumes;
    let disks = &snap.disks;

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
        " Disk Space ({} volumes){mode_tag} ",
        volumes.len()
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

    if volumes.is_empty() {
        frame.render_widget(
            Paragraph::new(" No volumes detected").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let selected = state
        .disk_space_selected
        .min(volumes.len().saturating_sub(1));
    let detail_open = state.disk_space_detail_open && focused;

    match mode {
        // ── Default / Gauge ────────────────────────────────────────────────────
        ViewMode::Default | ViewMode::Gauge => {
            if detail_open {
                // Split: list on top, detail panel below
                let detail_height = 7u16.min(inner.height / 2);
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(detail_height)])
                    .split(inner);
                render_volume_list(frame, chunks[0], volumes, selected, true);
                render_volume_detail(frame, chunks[1], volumes, disks, selected, history);
            } else {
                render_volume_list(frame, inner, volumes, selected, focused);
            }
        }

        // ── Graph: selected-volume used% sparkline ────────────────────────────
        ViewMode::Graph => {
            let vol = &volumes[selected];
            let used_pct = if vol.total_bytes > 0 {
                vol.used_bytes as f64 / vol.total_bytes as f64 * 100.0
            } else {
                0.0
            };
            // Static line (disk space doesn't change fast) — use width-long flat array
            let data: Vec<u64> = vec![used_pct as u64; inner.width as usize];
            frame.render_widget(
                Sparkline::default()
                    .data(&data)
                    .max(100)
                    .style(Style::default().fg(pct_color(used_pct)))
                    .bar_set(symbols::bar::NINE_LEVELS),
                inner,
            );
        }
    }
}

// Render the list of volumes with one selected row highlighted
fn render_volume_list(
    frame: &mut Frame,
    area: Rect,
    volumes: &[crate::metrics::VolumeMetrics],
    selected: usize,
    show_hint: bool,
) {
    let hint_height: u16 = if show_hint { 1 } else { 0 };
    let list_area = Rect {
        height: area.height.saturating_sub(hint_height),
        ..area
    };
    let hint_area = Rect {
        y: area.y + list_area.height,
        height: hint_height,
        ..area
    };

    let mut y = list_area.y;
    let mut remaining = list_area.height;
    for (i, vol) in volumes.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        let used_pct = if vol.total_bytes > 0 {
            vol.used_bytes as f64 / vol.total_bytes as f64 * 100.0
        } else {
            0.0
        };
        let ratio = (used_pct / 100.0).clamp(0.0, 1.0);
        let label = format!(
            " {} {:.1}%  {} / {}",
            vol.mount_point,
            used_pct,
            fmt_bytes(vol.used_bytes),
            fmt_bytes(vol.total_bytes)
        );
        let is_sel = i == selected;
        let fg = if is_sel {
            Color::Black
        } else {
            pct_color(used_pct)
        };
        let bg = if is_sel {
            pct_color(used_pct)
        } else {
            Color::Reset
        };
        let style = Style::default().fg(fg).bg(bg);
        let mut gauge = Gauge::default()
            .gauge_style(style)
            .ratio(ratio)
            .label(label);
        if is_sel {
            gauge = gauge.gauge_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(pct_color(used_pct))
                    .add_modifier(Modifier::BOLD),
            );
        }
        frame.render_widget(
            gauge,
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
        y += 1;
        remaining -= 1;
    }

    if show_hint && hint_area.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Enter: detail  j/k: select ",
                Style::default().fg(Color::DarkGray),
            )),
            hint_area,
        );
    }
}

// Render the detail panel for the selected volume
fn render_volume_detail(
    frame: &mut Frame,
    area: Rect,
    volumes: &[crate::metrics::VolumeMetrics],
    disks: &[crate::metrics::DiskMetrics],
    selected: usize,
    _history: &HistoryBuffer,
) {
    let vol = &volumes[selected];
    let used_pct = if vol.total_bytes > 0 {
        vol.used_bytes as f64 / vol.total_bytes as f64 * 100.0
    } else {
        0.0
    };

    // Try to find a matching disk entry by index (best-effort)
    let disk = disks.get(0); // may be None on stub builds

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Line::from(format!(" {} — detail ", vol.mount_point)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let free_pct = 100.0 - used_pct;

    let lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(" Used   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({:.1}%)", fmt_bytes(vol.used_bytes), used_pct),
                Style::default()
                    .fg(pct_color(used_pct))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Free   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({:.1}%)", fmt_bytes(vol.free_bytes), free_pct),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Total  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                fmt_bytes(vol.total_bytes),
                Style::default().fg(Color::White),
            ),
        ]),
        if let Some(d) = disk {
            Line::from(vec![
                Span::styled(" I/O    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "R {} / W {}  IOPS {:.0}r {:.0}w  Q {:.1}  Lat {:.1}/{:.1}ms",
                        fmt_bytes_sec(d.read_bytes_sec),
                        fmt_bytes_sec(d.write_bytes_sec),
                        d.iops_read,
                        d.iops_write,
                        d.queue_depth,
                        d.read_latency_ms,
                        d.write_latency_ms,
                    ),
                    Style::default().fg(Color::Cyan),
                ),
            ])
        } else {
            Line::from(Span::styled(
                " I/O    n/a",
                Style::default().fg(Color::DarkGray),
            ))
        },
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}
