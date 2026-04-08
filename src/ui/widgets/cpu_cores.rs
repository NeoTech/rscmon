use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Sparkline},
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
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let core_count = history.cpu_per_core.len();

    let mode_tag = match mode {
        ViewMode::Default => "",
        ViewMode::Graph => " [graph]",
        ViewMode::Gauge => " [gauge]",
    };
    let title = Line::from(format!(
        " CPU Cores — {} threads | {:.1}% overall{} ",
        core_count, snap.cpu.global_pct, mode_tag
    ));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 || core_count == 0 {
        return;
    }

    match mode {
        // ── Default / Graph: label + sparkline grid ──────────────────────────
        // Graph gives each sparkline the full cell height (no label row).
        ViewMode::Default | ViewMode::Graph => {
            let col_width = 20u16.min(inner.width);
            let cols = (inner.width / col_width).max(1) as usize;
            // Default: 3 rows per core (1 label + 2 spark).
            // Graph: 2 rows per core (no label — more chart).
            let (row_height, has_label) = match mode {
                ViewMode::Graph => (2u16, false),
                _ => (3u16, true),
            };
            let rows_available = (inner.height / row_height) as usize;
            let visible = (cols * rows_available).min(core_count);

            for (i, core_history) in history.cpu_per_core.iter().take(visible).enumerate() {
                let col = (i % cols) as u16;
                let row = (i / cols) as u16;

                let x = inner.x + col * col_width;
                let y = inner.y + row * row_height;
                let w = col_width.min(inner.right().saturating_sub(x));
                let h = row_height.min(inner.bottom().saturating_sub(y));

                if w == 0 || h == 0 {
                    break;
                }

                let pct = snap.cpu.per_core_pct.get(i).copied().unwrap_or(0.0) as f64;
                let color = pct_color(pct);

                let spark_y = if has_label { y + 1 } else { y };
                let spark_h = if has_label { h.saturating_sub(1) } else { h };

                if has_label && h >= 1 {
                    let label = format!(" C{i:<2} {pct:>5.1}%");
                    frame.render_widget(
                        ratatui::widgets::Paragraph::new(Line::from(vec![Span::styled(
                            label,
                            Style::default().fg(color),
                        )])),
                        Rect {
                            x,
                            y,
                            width: w,
                            height: 1,
                        },
                    );
                }

                if spark_h > 0 {
                    let data: Vec<u64> = core_history.iter().copied().collect();
                    frame.render_widget(
                        Sparkline::default()
                            .data(&data)
                            .max(100)
                            .style(Style::default().fg(color))
                            .bar_set(symbols::bar::NINE_LEVELS),
                        Rect {
                            x,
                            y: spark_y,
                            width: w,
                            height: spark_h,
                        },
                    );
                }
            }
        }

        // ── Gauge: one gauge row per core ────────────────────────────────────
        ViewMode::Gauge => {
            let visible = (inner.height as usize).min(core_count);
            for i in 0..visible {
                let pct = snap.cpu.per_core_pct.get(i).copied().unwrap_or(0.0) as f64;
                let color = pct_color(pct);
                let label = format!(" C{i:<2}  {pct:.1}%");
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(color))
                        .ratio((pct / 100.0).clamp(0.0, 1.0))
                        .label(label),
                    Rect {
                        x: inner.x,
                        y: inner.y + i as u16,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
        }
    }
}
