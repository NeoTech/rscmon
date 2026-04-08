use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use super::ViewMode;
use crate::{
    app::AppState,
    metrics::{HistoryBuffer, TempSource},
};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    _mode: ViewMode,
    state: &AppState,
    _history: &HistoryBuffer,
) {
    let snap = state.snapshot.lock().unwrap();
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let source_label = match snap.temp_source {
        TempSource::LibreHardwareMonitor => "LibreHardwareMonitor",
        TempSource::Acpi => "ACPI WMI",
        TempSource::Unavailable => "Unavailable",
    };

    let title = format!(" Temperatures — {source_label} ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    if snap.temp_source == TempSource::Unavailable || snap.temperatures.is_empty() {
        let msg = match snap.temp_source {
            TempSource::Unavailable => vec![
                Line::from(Span::styled(
                    " No temperature data available.",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    " Run LibreHardwareMonitor as Administrator",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    " for full per-sensor temperatures.",
                    Style::default().fg(Color::DarkGray),
                )),
            ],
            _ => vec![
                Line::from(Span::styled(
                    " No sensor data available from ACPI/PDH.",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    " Install LibreHardwareMonitor and run it as a",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    " service for full per-sensor temperatures.",
                    Style::default().fg(Color::DarkGray),
                )),
            ],
        };
        frame.render_widget(Paragraph::new(msg), inner);
        return;
    }

    // Table header + rows
    let header = Row::new(vec![
        Cell::from("Sensor").style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::White),
        ),
        Cell::from("Type").style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::White),
        ),
        Cell::from("°C").style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::White),
        ),
        Cell::from("Max °C").style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::White),
        ),
    ]);

    let rows: Vec<Row> = snap
        .temperatures
        .iter()
        .map(|t| {
            let temp_color = if t.value_celsius >= 90.0 {
                Color::Red
            } else if t.value_celsius >= 70.0 {
                Color::Yellow
            } else {
                Color::Green
            };
            Row::new(vec![
                Cell::from(t.label.clone()),
                Cell::from(t.sensor_type.clone()).style(Style::default().fg(Color::DarkGray)),
                Cell::from(format!("{:.1}", t.value_celsius))
                    .style(Style::default().fg(temp_color)),
                Cell::from(
                    t.max_celsius
                        .map(|m| format!("{m:.1}"))
                        .unwrap_or_else(|| "--".into()),
                )
                .style(Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Min(24),
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Length(8),
        ratatui::layout::Constraint::Length(8),
    ];

    let table = Table::new(rows, widths).header(header).column_spacing(1);

    frame.render_widget(table, inner);
}
