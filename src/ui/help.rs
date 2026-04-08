use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

const HELP_TEXT: &[(&str, &str)] = &[
    ("Navigation", ""),
    ("  h / ←", "Move focus left"),
    ("  l / →", "Move focus right"),
    ("  k / ↑", "Move focus up"),
    ("  j / ↓", "Move focus down"),
    ("  Tab", "Cycle focus forward"),
    ("  Shift+Tab", "Cycle focus backward"),
    ("", ""),
    ("Pane management", ""),
    ("  |", "Split pane vertically (left | right)"),
    ("  -", "Split pane horizontally (top / bottom)"),
    ("  x", "Close focused pane"),
    ("  w", "Open widget picker for focused pane"),
    ("  v", "Cycle view mode (default → graph → gauge)"),
    ("  > / <", "Resize focused pane"),
    ("", ""),
    ("App", ""),
    ("  ?", "Toggle this help"),
    ("  q  /  Ctrl+C", "Quit"),
];

pub fn render(frame: &mut Frame, area: Rect) {
    let width = 56u16.min(area.width.saturating_sub(4));
    let height = (HELP_TEXT.len() as u16 + 4).min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help — resource_monitor ")
        .title_alignment(Alignment::Center);

    let lines: Vec<Line> = HELP_TEXT
        .iter()
        .map(|(key, desc)| {
            if desc.is_empty() {
                // Section header or blank line
                Line::from(vec![Span::styled(
                    format!(" {key}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )])
            } else {
                Line::from(vec![
                    Span::styled(format!("{key:<18}"), Style::default().fg(Color::Green)),
                    Span::raw(desc.to_string()),
                ])
            }
        })
        .collect();

    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(para, popup);
}
