use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols,
    text::Line,
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame,
};

use super::{pct_color, ViewMode};
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
    let pct = snap.cpu.global_pct as f64;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let spark_color = pct_color(pct);

    let mode_tag = match mode {
        ViewMode::Default => "",
        ViewMode::Graph => " [graph]",
        ViewMode::Gauge => " [gauge]",
    };
    let title = Line::from(format!(
        " CPU — {} | {:.1}%{} ",
        snap.cpu.model_name, pct, mode_tag
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
        // ── Default: info row + sparkline ────────────────────────────────────
        ViewMode::Default => {
            let mut y = inner.y;
            let mut remaining = inner.height;

            if remaining >= 1 {
                let info = format!(
                    " {} physical cores · {} logical processors",
                    snap.cpu.core_count, snap.cpu.thread_count
                );
                frame.render_widget(
                    Paragraph::new(info).style(Style::default().fg(Color::DarkGray)),
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
            if remaining >= 2 {
                y += 1;
                remaining -= 1;
            }
            if remaining >= 1 {
                let data: Vec<u64> = history.cpu_global.iter().copied().collect();
                let hist_max = data.iter().copied().max().unwrap_or(10).max(10);
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(hist_max)
                        .style(Style::default().fg(spark_color))
                        .bar_set(symbols::bar::NINE_LEVELS),
                    Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: remaining,
                    },
                );
            }
        }

        // ── Graph: full-area sparkline, no overhead ───────────────────────────
        ViewMode::Graph => {
            let data: Vec<u64> = history.cpu_global.iter().copied().collect();
            let hist_max = data.iter().copied().max().unwrap_or(10).max(10);
            frame.render_widget(
                Sparkline::default()
                    .data(&data)
                    .max(hist_max)
                    .style(Style::default().fg(spark_color))
                    .bar_set(symbols::bar::NINE_LEVELS),
                inner,
            );
        }

        // ── Gauge: single large filled bar ───────────────────────────────────
        ViewMode::Gauge => {
            let mut y = inner.y;
            let mut remaining = inner.height;

            // Info row
            if remaining >= 1 {
                let info = format!(
                    " {} cores · {} threads",
                    snap.cpu.core_count, snap.cpu.thread_count
                );
                frame.render_widget(
                    Paragraph::new(info).style(Style::default().fg(Color::DarkGray)),
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
            // Big gauge — fills all remaining height, each row is one bar
            for row in 0..remaining {
                let label = if row == remaining / 2 {
                    format!(" CPU  {pct:.1}%")
                } else {
                    String::new()
                };
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(pct_color(pct)))
                        .ratio((pct / 100.0).clamp(0.0, 1.0))
                        .label(label),
                    Rect {
                        x: inner.x,
                        y: y + row,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
        }
    }
}
