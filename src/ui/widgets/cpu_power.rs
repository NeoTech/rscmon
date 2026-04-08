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
    let power = &snap.cpu_power;

    let pkg_w = power.package_watts.unwrap_or(0.0);
    let cores_w = power.cores_watts.unwrap_or(0.0);

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
    let source_str = match power.source {
        crate::metrics::PowerSource::Lhm => "LHM",
        crate::metrics::PowerSource::Rapl => "RAPL",
        crate::metrics::PowerSource::Unavailable => "N/A",
    };
    let title = Line::from(format!(
        " CPU Power — {pkg_w:.1}W [{source_str}]{mode_tag} "
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

    if power.package_watts.is_none() && power.cores_watts.is_none() {
        frame.render_widget(
            Paragraph::new(" CPU power unavailable (needs LHM or RAPL)")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    match mode {
        // ── Default: package W + cores W ──────────────────────────────────────
        ViewMode::Default => {
            let mut y = inner.y;
            let mut remaining = inner.height;

            let lines = vec![
                format!(" Package power:  {pkg_w:.2} W"),
                format!(" Core power:     {cores_w:.2} W"),
                format!(" Source:         {source_str}"),
            ];
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

            if remaining >= 2 {
                y += 1;
                remaining -= 1;
            }
            if remaining >= 1 {
                let data: Vec<u64> = history.cpu_power_mw.iter().copied().collect();
                let max = data.iter().copied().max().unwrap_or(1).max(1);
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(max)
                        .style(Style::default().fg(Color::Yellow))
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

        // ── Graph: power sparkline ────────────────────────────────────────────
        ViewMode::Graph => {
            let data: Vec<u64> = history.cpu_power_mw.iter().copied().collect();
            let max = data.iter().copied().max().unwrap_or(1).max(1);
            frame.render_widget(
                Sparkline::default()
                    .data(&data)
                    .max(max)
                    .style(Style::default().fg(Color::Yellow))
                    .bar_set(symbols::bar::NINE_LEVELS),
                inner,
            );
        }

        // ── Gauge: package power gauge (0–200 W scale) ───────────────────────
        ViewMode::Gauge => {
            const MAX_W: f64 = 200.0;
            let ratio = (pkg_w as f64 / MAX_W).clamp(0.0, 1.0);
            for row in 0..inner.height {
                let label = if row == inner.height / 2 {
                    format!(" Package {pkg_w:.1} W")
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
                        y: inner.y + row,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
        }
    }
}
