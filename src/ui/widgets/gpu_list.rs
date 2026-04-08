use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols,
    text::Line,
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
    let gpus = &snap.gpus;

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
    let count = gpus.len();
    let title = Line::from(format!(" GPU List ({count}){mode_tag} "));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if gpus.is_empty() {
        frame.render_widget(
            Paragraph::new(" No GPUs detected").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    match mode {
        // ── Default: table — name / util / VRAM / temp / power ────────────────
        ViewMode::Default => {
            let mut y = inner.y;
            let mut remaining = inner.height;
            for gpu in gpus.iter() {
                if remaining == 0 {
                    break;
                }
                let vram_str = match (gpu.vram_used_bytes, gpu.vram_total_bytes) {
                    (Some(used), Some(total)) => {
                        format!("{}/{}", fmt_bytes(used), fmt_bytes(total))
                    }
                    _ => "N/A".into(),
                };
                let temp_str = gpu
                    .temp_celsius
                    .map(|t| format!("{t:.0}°C"))
                    .unwrap_or_else(|| "N/A".into());
                let power_str = gpu
                    .power_mw
                    .map(|p| format!("{:.1}W", p as f64 / 1000.0))
                    .unwrap_or_else(|| "N/A".into());
                let line = format!(
                    " {:.<20} {:>5.1}% VRAM:{} T:{} P:{}",
                    gpu.name, gpu.util_pct, vram_str, temp_str, power_str
                );
                frame.render_widget(
                    Paragraph::new(line).style(Style::default().fg(pct_color(gpu.util_pct as f64))),
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

        // ── Graph: overlapping sparklines (one per GPU) ───────────────────────
        ViewMode::Graph => {
            for (i, util_hist) in history.gpu_util.iter().enumerate() {
                let data: Vec<u64> = util_hist.iter().copied().collect();
                let color = match i % 4 {
                    0 => Color::Green,
                    1 => Color::Cyan,
                    2 => Color::Yellow,
                    _ => Color::Magenta,
                };
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(100)
                        .style(Style::default().fg(color))
                        .bar_set(symbols::bar::NINE_LEVELS),
                    inner,
                );
            }
        }

        // ── Gauge: stacked util bars per GPU ──────────────────────────────────
        ViewMode::Gauge => {
            let rows_per_gpu = (inner.height / count.max(1) as u16).max(1);
            let mut y = inner.y;
            for (i, gpu) in gpus.iter().enumerate() {
                let util = gpu.util_pct as f64;
                let ratio = (util / 100.0).clamp(0.0, 1.0);
                for row in 0..rows_per_gpu {
                    if y >= inner.y + inner.height {
                        break;
                    }
                    let label = if row == rows_per_gpu / 2 {
                        format!(" GPU{i} {util:.1}%")
                    } else {
                        String::new()
                    };
                    frame.render_widget(
                        Gauge::default()
                            .gauge_style(Style::default().fg(pct_color(util)))
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
