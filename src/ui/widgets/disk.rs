use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols,
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
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let disk_count = snap.disks.len();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" Disk I/O — {disk_count} physical drive(s) "));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if disk_count == 0 {
        frame.render_widget(
            Paragraph::new(" No physical drives detected (try running as admin).")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    match mode {
        // ── Default / Gauge: R/W gauge bars per disk ─────────────────────────
        ViewMode::Default | ViewMode::Gauge => {
            const PER_DISK: u16 = 2;
            let disks_visible = ((inner.height / PER_DISK) as usize).max(1).min(disk_count);

            for i in 0..disks_visible {
                let y_base = inner.y + i as u16 * PER_DISK;
                let h_left = inner.height.saturating_sub(i as u16 * PER_DISK);
                if h_left == 0 {
                    break;
                }

                let disk = &snap.disks[i];
                let r_bps = disk.read_bytes_sec;
                let w_bps = disk.write_bytes_sec;

                let read_max = history.disk_read_max.get(i).copied().unwrap_or(1).max(1) as f64;
                let write_max = history.disk_write_max.get(i).copied().unwrap_or(1).max(1) as f64;

                // Read gauge
                let read_ratio = (r_bps / read_max).clamp(0.0, 1.0);
                let read_label = format!(" {} · R  {}", disk.name, fmt_bytes_sec(r_bps));
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(Color::Green).bg(Color::Black))
                        .ratio(read_ratio)
                        .label(read_label),
                    Rect {
                        x: inner.x,
                        y: y_base,
                        width: inner.width,
                        height: 1,
                    },
                );

                // Write gauge
                if h_left >= 2 {
                    let write_ratio = (w_bps / write_max).clamp(0.0, 1.0);
                    let name_width = disk.name.len() + 4;
                    let indent = " ".repeat(name_width.min(inner.width as usize));
                    let write_label = format!("{indent}W  {}", fmt_bytes_sec(w_bps));
                    frame.render_widget(
                        Gauge::default()
                            .gauge_style(Style::default().fg(Color::Yellow).bg(Color::Black))
                            .ratio(write_ratio)
                            .label(write_label),
                        Rect {
                            x: inner.x,
                            y: y_base + 1,
                            width: inner.width,
                            height: 1,
                        },
                    );
                }
            }

            // Overflow indicator
            if disks_visible < disk_count {
                let hidden = disk_count - disks_visible;
                let overflow_y = inner.y + disks_visible as u16 * PER_DISK;
                if overflow_y < inner.bottom() {
                    frame.render_widget(
                        Paragraph::new(format!(" … +{hidden} more drive(s) (resize pane to see)"))
                            .style(Style::default().fg(Color::DarkGray)),
                        Rect {
                            x: inner.x,
                            y: overflow_y,
                            width: inner.width,
                            height: 1,
                        },
                    );
                }
            }
        }

        // ── Graph: R + W sparklines per disk ─────────────────────────────────
        ViewMode::Graph => {
            // Each disk gets 2 rows: R sparkline + W sparkline.
            // If the pane is very short, we squeeze as many disks as fit.
            const PER_DISK: u16 = 2;
            let disks_visible = ((inner.height / PER_DISK) as usize).max(1).min(disk_count);

            for i in 0..disks_visible {
                let y_base = inner.y + i as u16 * PER_DISK;
                let h_left = inner.height.saturating_sub(i as u16 * PER_DISK);
                if h_left == 0 {
                    break;
                }

                let disk = &snap.disks[i];

                // Read sparkline row
                if let Some(read_hist) = history.disk_read.get(i) {
                    let read_max = history.disk_read_max.get(i).copied().unwrap_or(1).max(1);
                    let data: Vec<u64> = read_hist.iter().copied().collect();
                    let label_width = (disk.name.len() + 5).min(inner.width as usize) as u16;
                    // Draw a tiny label to the left (max 16 chars)
                    let label_w = label_width.min(16).min(inner.width / 2);
                    let spark_x = inner.x + label_w;
                    let spark_w = inner.width.saturating_sub(label_w);

                    if label_w > 0 {
                        let lbl = format!(" {}·R", disk.name);
                        frame.render_widget(
                            Paragraph::new(lbl).style(Style::default().fg(Color::Green)),
                            Rect {
                                x: inner.x,
                                y: y_base,
                                width: label_w,
                                height: 1,
                            },
                        );
                    }
                    if spark_w > 0 {
                        frame.render_widget(
                            Sparkline::default()
                                .data(&data)
                                .max(read_max)
                                .style(Style::default().fg(Color::Green))
                                .bar_set(symbols::bar::NINE_LEVELS),
                            Rect {
                                x: spark_x,
                                y: y_base,
                                width: spark_w,
                                height: 1,
                            },
                        );
                    }
                }

                // Write sparkline row
                if h_left >= 2 {
                    if let Some(write_hist) = history.disk_write.get(i) {
                        let write_max = history.disk_write_max.get(i).copied().unwrap_or(1).max(1);
                        let data: Vec<u64> = write_hist.iter().copied().collect();
                        let label_w = ((disk.name.len() + 5) as u16).min(16).min(inner.width / 2);
                        let spark_x = inner.x + label_w;
                        let spark_w = inner.width.saturating_sub(label_w);

                        if label_w > 0 {
                            let lbl = format!(" {}·W", disk.name);
                            frame.render_widget(
                                Paragraph::new(lbl).style(Style::default().fg(Color::Yellow)),
                                Rect {
                                    x: inner.x,
                                    y: y_base + 1,
                                    width: label_w,
                                    height: 1,
                                },
                            );
                        }
                        if spark_w > 0 {
                            frame.render_widget(
                                Sparkline::default()
                                    .data(&data)
                                    .max(write_max)
                                    .style(Style::default().fg(Color::Yellow))
                                    .bar_set(symbols::bar::NINE_LEVELS),
                                Rect {
                                    x: spark_x,
                                    y: y_base + 1,
                                    width: spark_w,
                                    height: 1,
                                },
                            );
                        }
                    }
                }
            }

            // Overflow indicator
            if disks_visible < disk_count {
                let hidden = disk_count - disks_visible;
                let overflow_y = inner.y + disks_visible as u16 * PER_DISK;
                if overflow_y < inner.bottom() {
                    frame.render_widget(
                        Paragraph::new(format!(" … +{hidden} more drive(s) (resize pane to see)"))
                            .style(Style::default().fg(Color::DarkGray)),
                        Rect {
                            x: inner.x,
                            y: overflow_y,
                            width: inner.width,
                            height: 1,
                        },
                    );
                }
            }
        }
    }
}
