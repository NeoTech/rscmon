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
    let bat = &snap.battery;

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
    let title = Line::from(format!(" Battery — {}%{mode_tag} ", bat.charge_pct));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if !bat.present {
        frame.render_widget(
            Paragraph::new(" No battery present (desktop system)")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let pct = bat.charge_pct as f64;
    let status = if bat.charging {
        "Charging"
    } else if bat.ac_online {
        "Plugged in"
    } else {
        "Discharging"
    };
    let time_str = bat
        .time_remaining_secs
        .map(|s| {
            let h = s / 3600;
            let m = (s % 3600) / 60;
            format!("{h}h {m}m remaining")
        })
        .unwrap_or_else(|| "—".into());

    match mode {
        // ── Default: charge% gauge + status row ──────────────────────────────
        ViewMode::Default => {
            let ratio = (pct / 100.0).clamp(0.0, 1.0);
            let mut y = inner.y;
            let mut remaining = inner.height;

            // Status line
            if remaining >= 1 {
                let info = format!(" {status}  {time_str}");
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

            // Gauge rows
            for row in 0..remaining {
                let label = if row == remaining / 2 {
                    format!(" {pct:.0}%")
                } else {
                    String::new()
                };
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(pct_color(pct)))
                        .ratio(ratio)
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

        // ── Graph: charge% sparkline ──────────────────────────────────────────
        ViewMode::Graph => {
            let data: Vec<u64> = history.battery_pct.iter().copied().collect();
            frame.render_widget(
                Sparkline::default()
                    .data(&data)
                    .max(100)
                    .style(Style::default().fg(pct_color(pct)))
                    .bar_set(symbols::bar::NINE_LEVELS),
                inner,
            );
        }

        // ── Gauge: tall charge gauge ──────────────────────────────────────────
        ViewMode::Gauge => {
            let ratio = (pct / 100.0).clamp(0.0, 1.0);
            for row in 0..inner.height {
                let label = if row == inner.height / 2 {
                    format!(" Battery {pct:.0}%  {status}")
                } else {
                    String::new()
                };
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(pct_color(pct)))
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
