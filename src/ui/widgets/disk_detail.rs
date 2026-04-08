use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols,
    text::Line,
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame,
};

use super::{fmt_bytes_sec, ViewMode};
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
    let title = Line::from(format!(" Disk Detail ({} disks){mode_tag} ", disks.len()));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if disks.is_empty() {
        frame.render_widget(
            Paragraph::new(" No disks detected").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    match mode {
        // ── Default: IOPS R/W + queue depth per disk ──────────────────────────
        ViewMode::Default => {
            let mut y = inner.y;
            let mut remaining = inner.height;
            for disk in disks.iter() {
                if remaining == 0 {
                    break;
                }
                let line = format!(
                    " {:.<16} R:{:>6.0} W:{:>6.0} IOPS  Q:{:.1}",
                    disk.name, disk.iops_read, disk.iops_write, disk.queue_depth
                );
                frame.render_widget(
                    Paragraph::new(line).style(Style::default().fg(Color::White)),
                    Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: 1,
                    },
                );
                y += 1;
                remaining -= 1;

                if remaining == 0 {
                    break;
                }
                let line2 = format!(
                    "   Throughput  R:{}  W:{}",
                    fmt_bytes_sec(disk.read_bytes_sec),
                    fmt_bytes_sec(disk.write_bytes_sec)
                );
                frame.render_widget(
                    Paragraph::new(line2).style(Style::default().fg(Color::DarkGray)),
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

        // ── Graph: IOPS sparklines per disk ───────────────────────────────────
        ViewMode::Graph => {
            // Show first disk read IOPS sparkline as the primary graph
            if let Some(iops_hist) = history.disk_iops_read.first() {
                let data: Vec<u64> = iops_hist.iter().copied().collect();
                let max = history
                    .disk_iops_read_max
                    .first()
                    .copied()
                    .unwrap_or(1)
                    .max(1);
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(max)
                        .style(Style::default().fg(Color::Green))
                        .bar_set(symbols::bar::NINE_LEVELS),
                    inner,
                );
            }
        }

        // ── Gauge: queue-depth bars per disk ──────────────────────────────────
        ViewMode::Gauge => {
            let n = disks.len();
            let rows_per = (inner.height / n.max(1) as u16).max(1);
            let mut y = inner.y;
            for (_i, disk) in disks.iter().enumerate() {
                // Treat queue depth 0-32 as the scale
                const MAX_Q: f64 = 32.0;
                let ratio = (disk.queue_depth / MAX_Q).clamp(0.0, 1.0);
                for row in 0..rows_per {
                    if y >= inner.y + inner.height {
                        break;
                    }
                    let label = if row == rows_per / 2 {
                        format!(" {}  Q:{:.1}", disk.name, disk.queue_depth)
                    } else {
                        String::new()
                    };
                    frame.render_widget(
                        Gauge::default()
                            .gauge_style(Style::default().fg(Color::Yellow))
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
