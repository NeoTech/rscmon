//! Widget picker overlay.
//!
//! Opened with `w` — shows a centered list of all available widget types.
//! The currently active widget is highlighted.  Navigate with j/k or arrows,
//! confirm with Enter, dismiss with Esc or `w`.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
    Frame,
};

use crate::ui::widgets::WidgetKind;

/// All widget kinds in display order.
pub const ALL_WIDGETS: &[WidgetKind] = &[
    WidgetKind::CpuOverview,
    WidgetKind::CpuCores,
    WidgetKind::CpuFrequency,
    WidgetKind::CpuPower,
    WidgetKind::GpuOverview,
    WidgetKind::GpuDetail,
    WidgetKind::GpuList,
    WidgetKind::MemoryOverview,
    WidgetKind::MemoryDetail,
    WidgetKind::DiskIO,
    WidgetKind::DiskDetail,
    WidgetKind::DiskSpace,
    WidgetKind::NetworkIO,
    WidgetKind::FansVoltages,
    WidgetKind::ProcessList,
    WidgetKind::Battery,
    WidgetKind::Uptime,
    WidgetKind::Temperatures,
    WidgetKind::SystemInfo,
];

pub fn render(frame: &mut Frame, area: Rect, selected_idx: usize, current: &WidgetKind) {
    let n = ALL_WIDGETS.len() as u16;
    // popup size: border (2) + header (1) + blank (1) + items + blank (1) = n + 5 rows
    let height = (n + 5).min(area.height.saturating_sub(2));
    let width = 36u16.min(area.width.saturating_sub(4));
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
        .title(" Select Widget ")
        .title_alignment(Alignment::Center);

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height == 0 {
        return;
    }

    // Build list items — show a marker on the currently active widget.
    let items: Vec<ListItem> = ALL_WIDGETS
        .iter()
        .map(|kind| {
            let active = kind == current;
            let prefix = if active { "* " } else { "  " };
            let label = format!("{prefix}{}", kind.label());
            let style = if active {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let hint_line = Line::from(vec![
        Span::styled("  j/k", Style::default().fg(Color::Green)),
        Span::styled(" navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Green)),
        Span::styled(" select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Green)),
        Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
    ]);

    // Split inner area: top rows for list, bottom 1 row for hint
    let list_height = inner.height.saturating_sub(1);
    let list_area = Rect {
        height: list_height,
        ..inner
    };
    let hint_area = Rect {
        y: inner.y + list_height,
        height: 1,
        ..inner
    };

    let mut list_state = ListState::default();
    list_state.select(Some(selected_idx));

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, list_area, &mut list_state);
    frame.render_widget(ratatui::widgets::Paragraph::new(hint_line), hint_area);
}
