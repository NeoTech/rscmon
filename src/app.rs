use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{backend::CrosstermBackend, layout::Rect, Frame, Terminal};

use crate::metrics::{HistoryBuffer, SharedSnapshot};
use crate::settings;
use crate::ui::{
    help,
    tiling::{FocusDir, TileTree},
    widget_picker,
    widgets::{self, WidgetKind},
};

// ---------------------------------------------------------------------------
// AppState — readable by widgets via shared reference
// ---------------------------------------------------------------------------

pub struct AppState {
    pub snapshot: SharedSnapshot,
    /// Scroll offset for the ProcessList widget (row index of first visible row).
    pub process_scroll: usize,
    /// Currently highlighted row in ProcessList; None means no selection yet.
    pub process_selected: Option<usize>,
    /// Whether the ProcessList detail panel is open.
    pub process_detail_open: bool,

    /// Currently highlighted volume index in DiskSpace.
    pub disk_space_selected: usize,
    /// Whether the DiskSpace detail panel is open.
    pub disk_space_detail_open: bool,

    /// Currently highlighted adapter index in NetworkIO.
    pub network_selected: usize,
    /// Whether the NetworkIO detail panel is open.
    pub network_detail_open: bool,
}

// ---------------------------------------------------------------------------
// App — owns the event loop, tiles, and history
// ---------------------------------------------------------------------------

pub struct App {
    pub state: AppState,
    pub tiles: TileTree,
    pub history: HistoryBuffer,
    pub show_help: bool,
    pub show_widget_picker: bool,
    /// Index into `widget_picker::ALL_WIDGETS` currently highlighted.
    pub picker_idx: usize,
    pub should_quit: bool,
    tick_rate: Duration,
    last_tick: Instant,
}

impl App {
    pub fn new(snapshot: SharedSnapshot, saved: Option<settings::Settings>) -> Self {
        let (tiles, focused_id) = match saved {
            Some(s) => {
                let mut tree = s.tiles;
                tree.focused_id = s.focused_id;
                let fid = tree.focused_id;
                (tree, fid)
            }
            None => {
                let tree = TileTree::default_layout();
                let fid = tree.focused_id;
                (tree, fid)
            }
        };
        let _ = focused_id; // already set on the tree above

        Self {
            state: AppState {
                snapshot,
                process_scroll: 0,
                process_selected: None,
                process_detail_open: false,
                disk_space_selected: 0,
                disk_space_detail_open: false,
                network_selected: 0,
                network_detail_open: false,
            },
            tiles,
            history: HistoryBuffer::new(),
            show_help: false,
            show_widget_picker: false,
            picker_idx: 0,
            should_quit: false,
            tick_rate: Duration::from_millis(500),
            last_tick: Instant::now(),
        }
    }

    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        while !self.should_quit {
            // Render
            let mut rects_for_nav: Vec<(usize, Rect)> = Vec::new();
            terminal.draw(|frame| {
                let area = frame.area();
                rects_for_nav = self.render(frame, area);
            })?;

            // Event / tick handling
            let timeout = self
                .tick_rate
                .checked_sub(self.last_tick.elapsed())
                .unwrap_or(Duration::ZERO);

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    // Windows double-key fix: only handle Press events
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    self.handle_key(key.code, key.modifiers, &rects_for_nav);
                }
            }

            if self.last_tick.elapsed() >= self.tick_rate {
                self.on_tick();
                self.last_tick = Instant::now();
            }
        }

        // Persist layout before exiting.
        let saved = settings::Settings::from_tile_tree(&self.tiles);
        if let Err(e) = settings::save(&saved) {
            log::warn!("Failed to save settings: {e}");
        }

        Ok(())
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) -> Vec<(usize, Rect)> {
        // Reserve the bottom row for the status bar.
        let tile_area = Rect {
            height: area.height.saturating_sub(1),
            ..area
        };
        let status_area = Rect {
            y: area.bottom().saturating_sub(1),
            height: 1,
            ..area
        };

        // Collect tile rects for this frame's layout.
        let mut leaf_rects: Vec<(usize, Rect)> = Vec::new();
        self.tiles.root.compute_rects(tile_area, &mut leaf_rects);

        // Render each tile.
        self.render_tiles(frame, &leaf_rects);

        // Status bar.
        self.render_status_bar(frame, status_area);

        if self.show_help {
            help::render(frame, area);
        }

        if self.show_widget_picker {
            let current = self
                .tiles
                .widget_of_focused()
                .cloned()
                .unwrap_or(WidgetKind::CpuOverview);
            widget_picker::render(frame, area, self.picker_idx, &current);
        }

        leaf_rects
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        use ratatui::{
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::Paragraph,
        };

        let widget_name = self
            .tiles
            .widget_of_focused()
            .map(|w| w.label())
            .unwrap_or("—");

        let mode = self.tiles.view_mode_of_focused();

        let time_str = local_time_hms();

        // Left: focused widget name + mode
        let left = Span::styled(
            format!(" {} [{}] ", widget_name, mode.label()),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        // Centre: key hints
        let hints = Span::styled(
            "  ?:help  |:split-v  -:split-h  w:widget  v:mode  x:close  >/<:resize  ⇧↑↓←→:pane  q:quit  ",
            Style::default().fg(Color::DarkGray),
        );

        // Right: clock
        let right = Span::styled(
            format!(" {time_str} "),
            Style::default().fg(Color::White).bg(Color::DarkGray),
        );

        let bar_style = Style::default().bg(Color::Reset);
        let line = Line::from(vec![left, hints, right]);
        frame.render_widget(Paragraph::new(line).style(bar_style), area);
    }

    fn render_tiles(&self, frame: &mut Frame, leaf_rects: &[(usize, Rect)]) {
        // Collect (id -> (WidgetKind, ViewMode)) from the tree.
        let mut leaves: Vec<(
            usize,
            crate::ui::widgets::WidgetKind,
            crate::ui::widgets::ViewMode,
        )> = Vec::new();
        self.tiles.root.collect_leaves(&mut leaves);

        for &(id, rect) in leaf_rects {
            let focused = id == self.tiles.focused_id;
            if let Some((_, kind, mode)) = leaves.iter().find(|(lid, _, _)| *lid == id) {
                widgets::render_widget(
                    kind,
                    *mode,
                    frame,
                    rect,
                    focused,
                    &self.state,
                    &self.history,
                );
            }
        }
    }

    fn on_tick(&mut self) {
        // Pull latest snapshot and update history buffers
        let snap = self.state.snapshot.lock().unwrap().clone();
        self.history.update(&snap);
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers, rects: &[(usize, Rect)]) {
        // ── Widget picker intercepts most keys while open ─────────────────────
        if self.show_widget_picker {
            match code {
                // Navigate down
                KeyCode::Char('j') | KeyCode::Down => {
                    let n = widget_picker::ALL_WIDGETS.len();
                    self.picker_idx = (self.picker_idx + 1) % n;
                }
                // Navigate up
                KeyCode::Char('k') | KeyCode::Up => {
                    let n = widget_picker::ALL_WIDGETS.len();
                    self.picker_idx = (self.picker_idx + n - 1) % n;
                }
                // Confirm selection
                KeyCode::Enter => {
                    let kind = widget_picker::ALL_WIDGETS[self.picker_idx].clone();
                    self.tiles.set_focused_widget(kind);
                    self.show_widget_picker = false;
                }
                // Dismiss without changing
                KeyCode::Esc | KeyCode::Char('w') => {
                    self.show_widget_picker = false;
                }
                // Quit still works even from the picker
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true
                }
                _ => {}
            }
            return;
        }

        match code {
            // Quit
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }

            // Help toggle
            KeyCode::Char('?') => self.show_help = !self.show_help,

            // Shift+Arrow: move focus between panes (spatial navigation)
            KeyCode::Left if modifiers.contains(KeyModifiers::SHIFT) => {
                self.tiles.move_focus(FocusDir::Left, rects)
            }
            KeyCode::Right if modifiers.contains(KeyModifiers::SHIFT) => {
                self.tiles.move_focus(FocusDir::Right, rects)
            }
            KeyCode::Up if modifiers.contains(KeyModifiers::SHIFT) => {
                self.tiles.move_focus(FocusDir::Up, rects)
            }
            KeyCode::Down if modifiers.contains(KeyModifiers::SHIFT) => {
                self.tiles.move_focus(FocusDir::Down, rects)
            }

            // j/k / plain Up/Down: per-widget navigation when a navigable widget is focused
            KeyCode::Char('j') | KeyCode::Down => {
                let focused_widget = self.tiles.widget_of_focused().cloned();
                match focused_widget {
                    Some(WidgetKind::ProcessList) => {
                        let snap = self.state.snapshot.lock().unwrap();
                        let count = snap.processes.len();
                        drop(snap);
                        let sel = self.state.process_selected.unwrap_or(0);
                        let new_sel = (sel + 1).min(count.saturating_sub(1));
                        self.state.process_selected = Some(new_sel);
                        // Auto-scroll so selection stays visible
                        self.state.process_scroll = self.state.process_scroll.max(
                            new_sel.saturating_sub(0), // will be clamped in widget render
                        );
                    }
                    Some(WidgetKind::DiskSpace) => {
                        let snap = self.state.snapshot.lock().unwrap();
                        let count = snap.volumes.len();
                        drop(snap);
                        self.state.disk_space_selected =
                            (self.state.disk_space_selected + 1).min(count.saturating_sub(1));
                    }
                    Some(WidgetKind::NetworkIO) => {
                        let snap = self.state.snapshot.lock().unwrap();
                        let count = snap.networks.len();
                        drop(snap);
                        self.state.network_selected =
                            (self.state.network_selected + 1).min(count.saturating_sub(1));
                    }
                    _ => {}
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let focused_widget = self.tiles.widget_of_focused().cloned();
                match focused_widget {
                    Some(WidgetKind::ProcessList) => {
                        let sel = self.state.process_selected.unwrap_or(0);
                        let new_sel = sel.saturating_sub(1);
                        self.state.process_selected = Some(new_sel);
                    }
                    Some(WidgetKind::DiskSpace) => {
                        self.state.disk_space_selected =
                            self.state.disk_space_selected.saturating_sub(1);
                    }
                    Some(WidgetKind::NetworkIO) => {
                        self.state.network_selected = self.state.network_selected.saturating_sub(1);
                    }
                    _ => {}
                }
            }

            // h/l: pane focus navigation (vim keys, always navigate between panes)
            KeyCode::Char('h') => self.tiles.move_focus(FocusDir::Left, rects),
            KeyCode::Char('l') => self.tiles.move_focus(FocusDir::Right, rects),

            // Focus cycling
            KeyCode::Tab => self.tiles.cycle_focus(true),
            KeyCode::BackTab => self.tiles.cycle_focus(false),

            // Pane splitting
            KeyCode::Char('|') => self.tiles.split_h(),
            KeyCode::Char('-') => self.tiles.split_v(),

            // Close pane
            KeyCode::Char('x') => self.tiles.close_focused(),

            // Open widget picker for focused pane
            KeyCode::Char('w') => {
                // Pre-select the currently active widget so the cursor lands on it.
                let current = self
                    .tiles
                    .widget_of_focused()
                    .cloned()
                    .unwrap_or(WidgetKind::CpuOverview);
                self.picker_idx = widget_picker::ALL_WIDGETS
                    .iter()
                    .position(|k| k == &current)
                    .unwrap_or(0);
                self.show_widget_picker = true;
            }

            // Cycle visualization mode for focused pane
            KeyCode::Char('v') => self.tiles.cycle_view_mode(),

            // Resize
            KeyCode::Char('>') => self.tiles.resize(true),
            KeyCode::Char('<') => self.tiles.resize(false),

            // Enter: open/close detail panel for navigable widgets
            KeyCode::Enter => {
                let focused_widget = self.tiles.widget_of_focused().cloned();
                match focused_widget {
                    Some(WidgetKind::ProcessList) => {
                        if self.state.process_selected.is_some() {
                            self.state.process_detail_open = !self.state.process_detail_open;
                        }
                    }
                    Some(WidgetKind::DiskSpace) => {
                        self.state.disk_space_detail_open = !self.state.disk_space_detail_open;
                    }
                    Some(WidgetKind::NetworkIO) => {
                        self.state.network_detail_open = !self.state.network_detail_open;
                    }
                    _ => {}
                }
            }

            // Esc: close detail panels (or help)
            KeyCode::Esc => {
                if self.show_help {
                    self.show_help = false;
                } else {
                    self.state.process_detail_open = false;
                    self.state.disk_space_detail_open = false;
                    self.state.network_detail_open = false;
                }
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

/// Return the local wall-clock time as "HH:MM:SS".
fn local_time_hms() -> String {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::SystemInformation::GetLocalTime;
        unsafe {
            let st = GetLocalTime();
            format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        String::from("--:--:--")
    }
}
