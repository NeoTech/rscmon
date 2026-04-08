use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame,
};

use super::{fmt_bytes, fmt_bytes_sec, ViewMode};
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
    let networks = snap.networks.clone();
    drop(snap);

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
        " Network I/O ({} adapters){mode_tag} ",
        networks.len()
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

    if networks.is_empty() {
        frame.render_widget(
            Paragraph::new(" No network adapters detected")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let selected = state.network_selected.min(networks.len().saturating_sub(1));
    let detail_open = state.network_detail_open && focused;

    match mode {
        // ── Default / Gauge: per-adapter Rx/Tx bars with selection ───────────
        ViewMode::Default | ViewMode::Gauge => {
            if detail_open {
                let detail_height = 7u16.min(inner.height / 2);
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(detail_height)])
                    .split(inner);
                render_adapter_list(frame, chunks[0], &networks, selected, history, true);
                render_adapter_detail(frame, chunks[1], &networks, selected, history);
            } else {
                render_adapter_list(frame, inner, &networks, selected, history, focused);
            }
        }

        // ── Graph: selected adapter Rx sparkline ─────────────────────────────
        ViewMode::Graph => {
            if let Some(rx_hist) = history.net_rx.get(selected) {
                let data: Vec<u64> = rx_hist.iter().copied().collect();
                let max = history
                    .net_rx_max
                    .get(selected)
                    .copied()
                    .unwrap_or(1)
                    .max(1);
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(max)
                        .style(Style::default().fg(Color::Green))
                        .bar_set(symbols::bar::NINE_LEVELS),
                    inner,
                );
            }
        }
    }
}

fn render_adapter_list(
    frame: &mut Frame,
    area: Rect,
    networks: &[crate::metrics::NetworkMetrics],
    selected: usize,
    history: &HistoryBuffer,
    show_hint: bool,
) {
    let hint_height: u16 = if show_hint { 1 } else { 0 };
    let list_area = Rect {
        height: area.height.saturating_sub(hint_height),
        ..area
    };
    let hint_area = Rect {
        y: area.y + list_area.height,
        height: hint_height,
        ..area
    };

    // Rows per adapter: name + Rx + Tx = 3 rows
    let rows_per = 3u16;
    let mut y = list_area.y;
    let mut remaining = list_area.height;

    for (i, net) in networks.iter().enumerate() {
        let rx_max = history.net_rx_max.get(i).copied().unwrap_or(1).max(1) as f64;
        let tx_max = history.net_tx_max.get(i).copied().unwrap_or(1).max(1) as f64;
        let is_sel = i == selected;

        if remaining == 0 {
            break;
        }

        // Adapter name row — highlighted if selected
        let name_style = if is_sel {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let sel_indicator = if is_sel { "▶ " } else { "  " };
        frame.render_widget(
            Paragraph::new(format!("{sel_indicator}{}", net.name)).style(name_style),
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
        y += 1;
        remaining -= 1;

        if remaining == 0 || rows_per < 2 {
            break;
        }

        // Rx bar
        let rx_ratio = (net.rx_bytes_sec / rx_max).clamp(0.0, 1.0);
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(Color::Green))
                .ratio(rx_ratio)
                .label(format!(" Rx {}", fmt_bytes_sec(net.rx_bytes_sec))),
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
        y += 1;
        remaining -= 1;

        if remaining == 0 {
            break;
        }

        // Tx bar
        let tx_ratio = (net.tx_bytes_sec / tx_max).clamp(0.0, 1.0);
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(Color::Cyan))
                .ratio(tx_ratio)
                .label(format!(" Tx {}", fmt_bytes_sec(net.tx_bytes_sec))),
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
        y += 1;
        remaining -= 1;
    }

    if show_hint && hint_area.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Enter: detail  j/k: select ",
                Style::default().fg(Color::DarkGray),
            )),
            hint_area,
        );
    }
}

fn render_adapter_detail(
    frame: &mut Frame,
    area: Rect,
    networks: &[crate::metrics::NetworkMetrics],
    selected: usize,
    _history: &HistoryBuffer,
) {
    let net = match networks.get(selected) {
        Some(n) => n,
        None => return,
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Line::from(format!(" {} — detail ", net.name)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(" Rx/s    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                fmt_bytes_sec(net.rx_bytes_sec),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   Tx/s  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                fmt_bytes_sec(net.tx_bytes_sec),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Rx pkt/s ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.0}", net.rx_packets_sec),
                Style::default().fg(Color::Green),
            ),
            Span::styled("  Tx pkt/s ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.0}", net.tx_packets_sec),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Rx err/s ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.0}", net.rx_errors_sec),
                Style::default().fg(if net.rx_errors_sec > 0.0 {
                    Color::Red
                } else {
                    Color::White
                }),
            ),
            Span::styled("  Tx err/s ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.0}", net.tx_errors_sec),
                Style::default().fg(if net.tx_errors_sec > 0.0 {
                    Color::Red
                } else {
                    Color::White
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Total Rx  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                fmt_bytes(net.rx_bytes_total),
                Style::default().fg(Color::White),
            ),
            Span::styled("   Total Tx  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                fmt_bytes(net.tx_bytes_total),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}
