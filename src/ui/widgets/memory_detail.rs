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
    let mem = &snap.memory;

    let commit_pct = if mem.commit_limit_bytes > 0 {
        (mem.commit_total_bytes as f64 / mem.commit_limit_bytes as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    let _pool_used = mem.used_bytes.saturating_sub(mem.available_bytes);

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
        " Memory Detail — commit {commit_pct:.1}%{mode_tag} "
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
        // ── Default: commit + pool + fault rate rows ──────────────────────────
        ViewMode::Default => {
            let lines = vec![
                format!(
                    " Commit: {} / {}  ({commit_pct:.1}%)",
                    fmt_bytes(mem.commit_total_bytes),
                    fmt_bytes(mem.commit_limit_bytes)
                ),
                format!(
                    " Used:   {} / {}",
                    fmt_bytes(mem.used_bytes),
                    fmt_bytes(mem.total_bytes)
                ),
                format!(" Avail:  {}", fmt_bytes(mem.available_bytes)),
                format!(
                    " Swap:   {} / {}",
                    fmt_bytes(mem.swap_used_bytes),
                    fmt_bytes(mem.swap_total_bytes)
                ),
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
            if remaining >= 1 {
                y += 1;
                remaining -= 1;
            }
            if remaining >= 1 {
                let data: Vec<u64> = history.mem_used_pct.iter().copied().collect();
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(100)
                        .style(Style::default().fg(pct_color(commit_pct)))
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

        // ── Graph: commit% sparkline ──────────────────────────────────────────
        ViewMode::Graph => {
            let data: Vec<u64> = history.mem_used_pct.iter().copied().collect();
            frame.render_widget(
                Sparkline::default()
                    .data(&data)
                    .max(100)
                    .style(Style::default().fg(pct_color(commit_pct)))
                    .bar_set(symbols::bar::NINE_LEVELS),
                inner,
            );
        }

        // ── Gauge: commit + used gauges ───────────────────────────────────────
        ViewMode::Gauge => {
            let half = inner.height / 2;
            let commit_ratio = (commit_pct / 100.0).clamp(0.0, 1.0);
            let used_pct = if mem.total_bytes > 0 {
                mem.used_bytes as f64 / mem.total_bytes as f64 * 100.0
            } else {
                0.0
            };
            let used_ratio = (used_pct / 100.0).clamp(0.0, 1.0);

            for row in 0..half {
                let label = if row == half / 2 {
                    format!(" Commit {commit_pct:.1}%")
                } else {
                    String::new()
                };
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(pct_color(commit_pct)))
                        .ratio(commit_ratio)
                        .label(label),
                    Rect {
                        x: inner.x,
                        y: inner.y + row,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
            for row in half..inner.height {
                let label = if row == half + (inner.height - half) / 2 {
                    format!(" RAM {used_pct:.1}%")
                } else {
                    String::new()
                };
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(pct_color(used_pct)))
                        .ratio(used_ratio)
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
