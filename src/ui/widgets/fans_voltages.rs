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
    _history: &HistoryBuffer,
) {
    let snap = state.snapshot.lock().unwrap();
    let fans = &snap.fans;
    let voltages = &snap.voltages;

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
        " Fans & Voltages ({} fans, {} rails){mode_tag} ",
        fans.len(),
        voltages.len()
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

    if fans.is_empty() && voltages.is_empty() {
        frame.render_widget(
            Paragraph::new(" No fan/voltage data (LHM required)")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    match mode {
        // ── Default: fan RPM + voltage table ──────────────────────────────────
        ViewMode::Default => {
            let mut y = inner.y;
            let mut remaining = inner.height;

            for fan in fans.iter() {
                if remaining == 0 {
                    break;
                }
                let line = format!(" {:<24} {:>6} RPM", fan.label, fan.rpm);
                frame.render_widget(
                    Paragraph::new(line).style(Style::default().fg(Color::Cyan)),
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

            if remaining >= 1 && !voltages.is_empty() {
                frame.render_widget(
                    Paragraph::new(" ── Voltages ──").style(Style::default().fg(Color::DarkGray)),
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

            for volt in voltages.iter() {
                if remaining == 0 {
                    break;
                }
                let line = format!(" {:<24} {:>6.3} V", volt.label, volt.value_v);
                frame.render_widget(
                    Paragraph::new(line).style(Style::default().fg(Color::Yellow)),
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

        // ── Graph: fan RPM sparkline (first fan) ──────────────────────────────
        ViewMode::Graph => {
            // We don't have a dedicated history buffer for fan RPM yet;
            // show the first fan's RPM as a static bar.
            if let Some(fan) = fans.first() {
                let rpm = fan.rpm as u64;
                let max = fan.max_rpm.map(|m| m as u64).unwrap_or(rpm.max(3000));
                let data = vec![rpm; inner.width as usize];
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(max.max(1))
                        .style(Style::default().fg(Color::Cyan))
                        .bar_set(symbols::bar::NINE_LEVELS),
                    inner,
                );
            } else {
                frame.render_widget(
                    Paragraph::new(" No fan data").style(Style::default().fg(Color::DarkGray)),
                    inner,
                );
            }
        }

        // ── Gauge: fan RPM bars ───────────────────────────────────────────────
        ViewMode::Gauge => {
            let mut y = inner.y;
            let mut remaining = inner.height;
            for fan in fans.iter() {
                if remaining == 0 {
                    break;
                }
                let max = fan.max_rpm.unwrap_or(3000).max(1) as f64;
                let ratio = (fan.rpm as f64 / max).clamp(0.0, 1.0);
                let label = format!(" {} {:>5} RPM", fan.label, fan.rpm);
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(Color::Cyan))
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
                remaining -= 1;
            }
        }
    }
}
