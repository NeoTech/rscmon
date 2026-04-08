use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols,
    text::Line,
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame,
};

use super::ViewMode;
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
    let freqs = &snap.cpu.freq_mhz_per_core;
    let avg_mhz = if freqs.is_empty() {
        0u64
    } else {
        freqs.iter().sum::<u64>() / freqs.len() as u64
    };
    let avg_ghz = avg_mhz as f64 / 1000.0;

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
    let title = Line::from(format!(" CPU Frequency — {avg_ghz:.2} GHz{mode_tag} "));
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
        // ── Default: per-core frequency bars ─────────────────────────────────
        ViewMode::Default => {
            let n = freqs.len();
            if n == 0 {
                frame.render_widget(
                    Paragraph::new(" No frequency data")
                        .style(Style::default().fg(Color::DarkGray)),
                    inner,
                );
                return;
            }
            let max_mhz = freqs.iter().copied().max().unwrap_or(1).max(1);
            let bar_h = 1u16;
            let mut y = inner.y;
            let mut rows_left = inner.height;
            for (i, &mhz) in freqs.iter().enumerate() {
                if rows_left == 0 {
                    break;
                }
                let ratio = (mhz as f64 / max_mhz as f64).clamp(0.0, 1.0);
                let ghz = mhz as f64 / 1000.0;
                let label = format!(" C{i:<2} {ghz:.2}GHz");
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(Color::Cyan))
                        .ratio(ratio)
                        .label(label),
                    Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: bar_h,
                    },
                );
                y += bar_h;
                rows_left -= bar_h;
            }
        }

        // ── Graph: average frequency sparkline ────────────────────────────────
        ViewMode::Graph => {
            let data: Vec<u64> = history.cpu_freq_avg.iter().copied().collect();
            let max = data.iter().copied().max().unwrap_or(1).max(1);
            frame.render_widget(
                Sparkline::default()
                    .data(&data)
                    .max(max)
                    .style(Style::default().fg(Color::Cyan))
                    .bar_set(symbols::bar::NINE_LEVELS),
                inner,
            );
        }

        // ── Gauge: large average GHz gauge ────────────────────────────────────
        ViewMode::Gauge => {
            // We treat max as 6 GHz for display purposes
            const MAX_GHZ: f64 = 6.0;
            let ratio = (avg_ghz / MAX_GHZ).clamp(0.0, 1.0);
            for row in 0..inner.height {
                let label = if row == inner.height / 2 {
                    format!(" {avg_ghz:.2} GHz")
                } else {
                    String::new()
                };
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(Color::Cyan))
                        .ratio(ratio)
                        .label(label),
                    Rect {
                        x: inner.x,
                        y: inner.y + row,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
        }
    }
}
