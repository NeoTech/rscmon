use ratatui::{
    layout::Rect,
    style::{Color, Style},
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
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let mem = &snap.memory;
    let used_pct = if mem.total_bytes > 0 {
        mem.used_bytes as f64 / mem.total_bytes as f64
    } else {
        0.0
    };
    let swap_pct = if mem.swap_total_bytes > 0 {
        mem.swap_used_bytes as f64 / mem.swap_total_bytes as f64
    } else {
        0.0
    };
    let commit_pct = if mem.commit_limit_bytes > 0 {
        mem.commit_total_bytes as f64 / mem.commit_limit_bytes as f64
    } else {
        0.0
    };

    let title = format!(
        " Memory — {} / {} ({:.1}%) ",
        fmt_bytes(mem.used_bytes),
        fmt_bytes(mem.total_bytes),
        used_pct * 100.0,
    );

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
        // ── Default: gauges + sparkline below ────────────────────────────────
        ViewMode::Default => {
            let mut y = inner.y;
            let mut remaining = inner.height;

            remaining = render_gauges(
                frame, inner, mem, used_pct, swap_pct, commit_pct, &mut y, remaining,
            );

            // ── Available / free line ────────────────────────────────────────
            if remaining >= 1 {
                let avail = format!(" Available: {}", fmt_bytes(mem.available_bytes));
                frame.render_widget(
                    Paragraph::new(avail).style(Style::default().fg(Color::Gray)),
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

            // ── Sparkline section ────────────────────────────────────────────
            // Require at least 3 rows: 1 for the label + 2 for the chart
            if remaining >= 3 {
                let sep_label = Line::from(vec![
                    Span::styled(" RAM ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "─".repeat(inner.width.saturating_sub(6) as usize),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                frame.render_widget(
                    Paragraph::new(sep_label),
                    Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: 1,
                    },
                );
                y += 1;
                remaining -= 1;

                let spark_h = remaining.min(5);
                let data: Vec<u64> = history.mem_used_pct.iter().copied().collect();
                let hist_max = data.iter().copied().max().unwrap_or(10).max(10);
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(hist_max)
                        .style(Style::default().fg(pct_color(used_pct * 100.0)))
                        .bar_set(symbols::bar::NINE_LEVELS),
                    Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: spark_h,
                    },
                );
            }
        }

        // ── Graph: full-area sparkline ────────────────────────────────────────
        ViewMode::Graph => {
            let data: Vec<u64> = history.mem_used_pct.iter().copied().collect();
            let hist_max = data.iter().copied().max().unwrap_or(10).max(10);
            frame.render_widget(
                Sparkline::default()
                    .data(&data)
                    .max(hist_max)
                    .style(Style::default().fg(pct_color(used_pct * 100.0)))
                    .bar_set(symbols::bar::NINE_LEVELS),
                inner,
            );
        }

        // ── Gauge: gauges only, no sparkline ─────────────────────────────────
        ViewMode::Gauge => {
            let mut y = inner.y;
            let remaining = inner.height;

            let remaining = render_gauges(
                frame, inner, mem, used_pct, swap_pct, commit_pct, &mut y, remaining,
            );

            // Available line in any leftover space
            if remaining >= 1 {
                let avail = format!(" Available: {}", fmt_bytes(mem.available_bytes));
                frame.render_widget(
                    Paragraph::new(avail).style(Style::default().fg(Color::Gray)),
                    Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
        }
    }
}

/// Render RAM / Swap / Commit gauges into the given area.
/// Advances `*y` and decrements `remaining` for each rendered row.
/// Returns the updated remaining row count.
fn render_gauges(
    frame: &mut Frame,
    inner: Rect,
    mem: &crate::metrics::MemoryMetrics,
    used_pct: f64,
    swap_pct: f64,
    commit_pct: f64,
    y: &mut u16,
    mut remaining: u16,
) -> u16 {
    // RAM gauge
    if remaining >= 1 {
        let label = format!(
            " RAM  {} / {}",
            fmt_bytes(mem.used_bytes),
            fmt_bytes(mem.total_bytes)
        );
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(pct_color(used_pct * 100.0)))
                .ratio(used_pct.clamp(0.0, 1.0))
                .label(label),
            Rect {
                x: inner.x,
                y: *y,
                width: inner.width,
                height: 1,
            },
        );
        *y += 1;
        remaining -= 1;
    }

    // Swap gauge
    if remaining >= 1 && mem.swap_total_bytes > 0 {
        let label = format!(
            " Swap {} / {}",
            fmt_bytes(mem.swap_used_bytes),
            fmt_bytes(mem.swap_total_bytes)
        );
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(pct_color(swap_pct * 100.0)))
                .ratio(swap_pct.clamp(0.0, 1.0))
                .label(label),
            Rect {
                x: inner.x,
                y: *y,
                width: inner.width,
                height: 1,
            },
        );
        *y += 1;
        remaining -= 1;
    }

    // Commit gauge
    if remaining >= 1 && mem.commit_limit_bytes > 0 {
        let label = format!(
            " Commit {} / {}",
            fmt_bytes(mem.commit_total_bytes),
            fmt_bytes(mem.commit_limit_bytes)
        );
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(pct_color(commit_pct * 100.0)))
                .ratio(commit_pct.clamp(0.0, 1.0))
                .label(label),
            Rect {
                x: inner.x,
                y: *y,
                width: inner.width,
                height: 1,
            },
        );
        *y += 1;
        remaining -= 1;
    }

    remaining
}
