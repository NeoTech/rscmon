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

pub fn render_overview(
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

    if snap.gpus.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(" GPU Overview ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(" No GPU data — waiting for PDH / NVML…")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let gpu = &snap.gpus[0];
    let pct = gpu.util_pct as f64;
    let spark_color = pct_color(pct);

    let mode_tag = match mode {
        ViewMode::Default => "",
        ViewMode::Graph => " [graph]",
        ViewMode::Gauge => " [gauge]",
    };
    let title = Line::from(format!(" {} | {:.1}%{} ", gpu.name, pct, mode_tag));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Build stats parts once — used by Default and Gauge modes
    let mut stats_parts: Vec<String> = vec![format!("{:.1}%", gpu.util_pct)];
    if let (Some(used), Some(total)) = (gpu.vram_used_bytes, gpu.vram_total_bytes) {
        stats_parts.push(format!("VRAM {} / {}", fmt_bytes(used), fmt_bytes(total)));
    }
    if let Some(t) = gpu.temp_celsius {
        stats_parts.push(format!("{:.0}°C", t));
    }
    if let Some(p) = gpu.power_mw {
        stats_parts.push(format!("{:.1}W", p as f64 / 1000.0));
    }

    match mode {
        // ── Default: stats row + sparkline ───────────────────────────────────
        ViewMode::Default => {
            let mut y = inner.y;
            let mut remaining = inner.height;

            if remaining >= 1 {
                let info = format!(" {} ", stats_parts.join("  ·  "));
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
            if remaining >= 2 {
                y += 1;
                remaining -= 1;
            }
            if remaining >= 1 && !history.gpu_util.is_empty() {
                let data: Vec<u64> = history.gpu_util[0].iter().copied().collect();
                let hist_max = data.iter().copied().max().unwrap_or(10).max(10);
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(hist_max)
                        .style(Style::default().fg(spark_color))
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

        // ── Graph: full-area sparkline ────────────────────────────────────────
        ViewMode::Graph => {
            if !history.gpu_util.is_empty() {
                let data: Vec<u64> = history.gpu_util[0].iter().copied().collect();
                let hist_max = data.iter().copied().max().unwrap_or(10).max(10);
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(hist_max)
                        .style(Style::default().fg(spark_color))
                        .bar_set(symbols::bar::NINE_LEVELS),
                    inner,
                );
            }
        }

        // ── Gauge: util + VRAM gauges, no sparkline ───────────────────────────
        ViewMode::Gauge => {
            let mut y = inner.y;
            let mut remaining = inner.height;

            // GPU util gauge (fills all remaining rows proportionally)
            if remaining >= 1 {
                let label = format!(" GPU  {pct:.1}%");
                frame.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(spark_color))
                        .ratio((pct / 100.0).clamp(0.0, 1.0))
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

            // VRAM gauge
            if remaining >= 1 {
                if let (Some(used), Some(total)) = (gpu.vram_used_bytes, gpu.vram_total_bytes) {
                    let vram_pct = if total > 0 {
                        used as f64 / total as f64
                    } else {
                        0.0
                    };
                    let label = format!(" VRAM {} / {}", fmt_bytes(used), fmt_bytes(total));
                    frame.render_widget(
                        Gauge::default()
                            .gauge_style(Style::default().fg(pct_color(vram_pct * 100.0)))
                            .ratio(vram_pct.clamp(0.0, 1.0))
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

            // Stats line
            if remaining >= 1 {
                let info = format!(" {} ", stats_parts[1..].join("  ·  "));
                frame.render_widget(
                    Paragraph::new(info).style(Style::default().fg(Color::DarkGray)),
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

pub fn render_detail(
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

    if snap.gpus.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(" GPU Detail ");
        frame.render_widget(block, area);
        return;
    }

    let gpu = &snap.gpus[0];
    let util_pct = gpu.util_pct as f64;

    let mode_tag = match mode {
        ViewMode::Default => "",
        ViewMode::Graph => " [graph]",
        ViewMode::Gauge => " [gauge]",
    };
    let title = Line::from(format!(" GPU Detail — {}{} ", gpu.name, mode_tag));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut y = inner.y;
    let mut remaining = inner.height;

    // GPU util gauge — always shown in all modes
    if remaining >= 1 {
        let ratio = (util_pct / 100.0).clamp(0.0, 1.0);
        let label = format!(" GPU  {util_pct:.1}%");
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(pct_color(util_pct)))
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

    // VRAM gauge
    if remaining >= 1 {
        if let (Some(used), Some(total)) = (gpu.vram_used_bytes, gpu.vram_total_bytes) {
            let vram_pct = if total > 0 {
                used as f64 / total as f64
            } else {
                0.0
            };
            let label = format!(" VRAM {} / {}", fmt_bytes(used), fmt_bytes(total));
            frame.render_widget(
                Gauge::default()
                    .gauge_style(Style::default().fg(pct_color(vram_pct * 100.0)))
                    .ratio(vram_pct.clamp(0.0, 1.0))
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

    // Stats line
    if remaining >= 1 {
        let mut parts: Vec<String> = Vec::new();
        if let Some(t) = gpu.temp_celsius {
            parts.push(format!("Temp: {t:.0}°C"));
        }
        if let Some(p) = gpu.power_mw {
            parts.push(format!("Power: {:.1}W", p as f64 / 1000.0));
        }
        if let Some(c) = gpu.clock_gpu_mhz {
            parts.push(format!("GPU Clk: {c}MHz"));
        }
        if let Some(m) = gpu.clock_mem_mhz {
            parts.push(format!("Mem Clk: {m}MHz"));
        }
        if let Some(f) = gpu.fan_speed_pct {
            parts.push(format!("Fan: {f}%"));
        }
        frame.render_widget(
            Paragraph::new(parts.join("  ")).style(Style::default().fg(Color::Gray)),
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

    // Sparkline — only in Default and Graph modes
    match mode {
        ViewMode::Gauge => {}
        _ => {
            let min_rows = if mode == ViewMode::Graph { 1 } else { 2 };
            if remaining >= min_rows && !history.gpu_util.is_empty() {
                let data: Vec<u64> = history.gpu_util[0].iter().copied().collect();
                let hist_max = data.iter().copied().max().unwrap_or(10).max(10);
                frame.render_widget(
                    Sparkline::default()
                        .data(&data)
                        .max(hist_max)
                        .style(Style::default().fg(pct_color(util_pct)))
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
    }
}
