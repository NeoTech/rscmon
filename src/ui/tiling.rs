use ratatui::layout::Rect;

use super::widgets::{ViewMode, WidgetKind};

// ---------------------------------------------------------------------------
// BSP Tile Tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SplitDir {
    Horizontal, // left | right
    Vertical,   // top / bottom
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TileNode {
    Leaf {
        id: usize,
        widget: WidgetKind,
        mode: ViewMode,
    },
    Split {
        dir: SplitDir,
        /// Fraction [0.1, 0.9] — what fraction goes to the first child.
        ratio: f32,
        first: Box<TileNode>,
        second: Box<TileNode>,
    },
}

impl TileNode {
    /// Collect all leaf (id, rect) pairs by recursively computing layout.
    pub fn compute_rects(&self, area: Rect, out: &mut Vec<(usize, Rect)>) {
        match self {
            TileNode::Leaf { id, .. } => out.push((*id, area)),
            TileNode::Split {
                dir,
                ratio,
                first,
                second,
            } => {
                let (a, b) = split_rect(area, *dir, *ratio);
                first.compute_rects(a, out);
                second.compute_rects(b, out);
            }
        }
    }

    /// Collect leaf IDs in depth-first order.
    pub fn leaf_ids(&self) -> Vec<usize> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    fn collect_ids(&self, out: &mut Vec<usize>) {
        match self {
            TileNode::Leaf { id, .. } => out.push(*id),
            TileNode::Split { first, second, .. } => {
                first.collect_ids(out);
                second.collect_ids(out);
            }
        }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            TileNode::Leaf { .. } => 1,
            TileNode::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }

    fn widget_for_id(&self, id: usize) -> Option<&WidgetKind> {
        match self {
            TileNode::Leaf {
                id: lid, widget, ..
            } if *lid == id => Some(widget),
            TileNode::Leaf { .. } => None,
            TileNode::Split { first, second, .. } => {
                first.widget_for_id(id).or_else(|| second.widget_for_id(id))
            }
        }
    }

    fn widget_for_id_mut(&mut self, id: usize) -> Option<&mut WidgetKind> {
        match self {
            TileNode::Leaf {
                id: lid, widget, ..
            } if *lid == id => Some(widget),
            TileNode::Leaf { .. } => None,
            TileNode::Split { first, second, .. } => first
                .widget_for_id_mut(id)
                .or_else(|| second.widget_for_id_mut(id)),
        }
    }

    fn mode_for_id(&self, id: usize) -> Option<ViewMode> {
        match self {
            TileNode::Leaf { id: lid, mode, .. } if *lid == id => Some(*mode),
            TileNode::Leaf { .. } => None,
            TileNode::Split { first, second, .. } => {
                first.mode_for_id(id).or_else(|| second.mode_for_id(id))
            }
        }
    }

    fn mode_for_id_mut(&mut self, id: usize) -> Option<&mut ViewMode> {
        match self {
            TileNode::Leaf { id: lid, mode, .. } if *lid == id => Some(mode),
            TileNode::Leaf { .. } => None,
            TileNode::Split { first, second, .. } => first
                .mode_for_id_mut(id)
                .or_else(|| second.mode_for_id_mut(id)),
        }
    }

    /// Collect all (id, widget, mode) triples for rendering.
    pub fn collect_leaves(&self, out: &mut Vec<(usize, WidgetKind, ViewMode)>) {
        match self {
            TileNode::Leaf { id, widget, mode } => out.push((*id, widget.clone(), *mode)),
            TileNode::Split { first, second, .. } => {
                first.collect_leaves(out);
                second.collect_leaves(out);
            }
        }
    }

    /// Split the leaf with the given `target_id` in `dir`, giving the new leaf `new_id`/`new_widget`.
    fn split_leaf(
        self,
        target_id: usize,
        new_id: usize,
        new_widget: WidgetKind,
        dir: SplitDir,
    ) -> TileNode {
        match self {
            TileNode::Leaf { id, widget, mode } if id == target_id => TileNode::Split {
                dir,
                ratio: 0.5,
                first: Box::new(TileNode::Leaf { id, widget, mode }),
                second: Box::new(TileNode::Leaf {
                    id: new_id,
                    widget: new_widget,
                    mode: ViewMode::Default,
                }),
            },
            TileNode::Leaf { id, widget, mode } => TileNode::Leaf { id, widget, mode },
            TileNode::Split {
                dir: d,
                ratio,
                first,
                second,
            } => TileNode::Split {
                dir: d,
                ratio,
                first: Box::new(first.split_leaf(target_id, new_id, new_widget.clone(), dir)),
                second: Box::new(second.split_leaf(target_id, new_id, new_widget, dir)),
            },
        }
    }

    /// Remove the leaf with `target_id`. Returns `None` if this node itself should be removed.
    fn remove_leaf(self, target_id: usize) -> Option<TileNode> {
        match self {
            TileNode::Leaf { id, .. } if id == target_id => None,
            TileNode::Leaf { id, widget, mode } => Some(TileNode::Leaf { id, widget, mode }),
            TileNode::Split {
                dir,
                ratio,
                first,
                second,
            } => {
                let new_first = first.remove_leaf(target_id);
                let new_second = second.remove_leaf(target_id);
                match (new_first, new_second) {
                    (None, None) => None,
                    (Some(n), None) | (None, Some(n)) => Some(n),
                    (Some(f), Some(s)) => Some(TileNode::Split {
                        dir,
                        ratio,
                        first: Box::new(f),
                        second: Box::new(s),
                    }),
                }
            }
        }
    }

    fn adjust_ratio(&mut self, target_id: usize, delta: f32) {
        match self {
            TileNode::Leaf { .. } => {}
            TileNode::Split {
                first,
                second,
                ratio,
                ..
            } => {
                let ids = first.leaf_ids();
                if ids.contains(&target_id) {
                    *ratio = (*ratio + delta).clamp(0.1, 0.9);
                } else {
                    *ratio = (*ratio - delta).clamp(0.1, 0.9);
                    // recurse
                    first.adjust_ratio(target_id, delta);
                    second.adjust_ratio(target_id, delta);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TileTree — owns the root and tracks focus
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TileTree {
    pub root: TileNode,
    pub focused_id: usize,
    next_id: usize,
}

impl TileTree {
    /// Create the default startup layout.
    pub fn default_layout() -> Self {
        // ┌──────────┬──────────┐
        // │  CPU Ov  │  GPU     │
        // ├──────────┼──────────┤
        // │  Memory  │  Disk IO │
        // ├──────────┴──────────┤
        // │     Temperatures    │
        // └─────────────────────┘
        let cpu = TileNode::Leaf {
            id: 0,
            widget: WidgetKind::CpuOverview,
            mode: ViewMode::Default,
        };
        let gpu = TileNode::Leaf {
            id: 1,
            widget: WidgetKind::GpuOverview,
            mode: ViewMode::Default,
        };
        let mem = TileNode::Leaf {
            id: 2,
            widget: WidgetKind::MemoryOverview,
            mode: ViewMode::Default,
        };
        let disk = TileNode::Leaf {
            id: 3,
            widget: WidgetKind::DiskIO,
            mode: ViewMode::Default,
        };
        let temp = TileNode::Leaf {
            id: 4,
            widget: WidgetKind::Temperatures,
            mode: ViewMode::Default,
        };

        let top_row = TileNode::Split {
            dir: SplitDir::Horizontal,
            ratio: 0.5,
            first: Box::new(cpu),
            second: Box::new(gpu),
        };
        let mid_row = TileNode::Split {
            dir: SplitDir::Horizontal,
            ratio: 0.5,
            first: Box::new(mem),
            second: Box::new(disk),
        };
        let top_mid = TileNode::Split {
            dir: SplitDir::Vertical,
            ratio: 0.5,
            first: Box::new(top_row),
            second: Box::new(mid_row),
        };
        let root = TileNode::Split {
            dir: SplitDir::Vertical,
            ratio: 0.7,
            first: Box::new(top_mid),
            second: Box::new(temp),
        };

        Self {
            root,
            focused_id: 0,
            next_id: 5,
        }
    }

    fn alloc_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    #[allow(dead_code)]
    pub fn widget_of_focused(&self) -> Option<&WidgetKind> {
        self.root.widget_for_id(self.focused_id)
    }

    /// Cycle the widget type in the focused pane (forward one step).
    #[allow(dead_code)]
    pub fn cycle_widget(&mut self) {
        if let Some(w) = self.root.widget_for_id_mut(self.focused_id) {
            *w = w.next();
        }
    }

    /// Set an explicit widget kind for the focused pane.
    pub fn set_focused_widget(&mut self, kind: WidgetKind) {
        if let Some(w) = self.root.widget_for_id_mut(self.focused_id) {
            *w = kind;
        }
    }

    /// Get the current view mode of the focused pane.
    pub fn view_mode_of_focused(&self) -> ViewMode {
        self.root.mode_for_id(self.focused_id).unwrap_or_default()
    }

    /// Cycle the visualization mode for the focused pane.
    pub fn cycle_view_mode(&mut self) {
        if let Some(m) = self.root.mode_for_id_mut(self.focused_id) {
            *m = m.next();
        }
    }

    /// Split focused pane horizontally (left | right).
    pub fn split_h(&mut self) {
        let new_id = self.alloc_id();
        let widget = self
            .root
            .widget_for_id(self.focused_id)
            .cloned()
            .unwrap_or(WidgetKind::CpuOverview)
            .next();
        let old_root = std::mem::replace(
            &mut self.root,
            TileNode::Leaf {
                id: 0,
                widget: WidgetKind::CpuOverview,
                mode: ViewMode::Default,
            },
        );
        self.root = old_root.split_leaf(self.focused_id, new_id, widget, SplitDir::Horizontal);
        self.focused_id = new_id;
    }

    /// Split focused pane vertically (top / bottom).
    pub fn split_v(&mut self) {
        let new_id = self.alloc_id();
        let widget = self
            .root
            .widget_for_id(self.focused_id)
            .cloned()
            .unwrap_or(WidgetKind::CpuOverview)
            .next();
        let old_root = std::mem::replace(
            &mut self.root,
            TileNode::Leaf {
                id: 0,
                widget: WidgetKind::CpuOverview,
                mode: ViewMode::Default,
            },
        );
        self.root = old_root.split_leaf(self.focused_id, new_id, widget, SplitDir::Vertical);
        self.focused_id = new_id;
    }

    /// Close the focused pane (only if more than one pane exists).
    pub fn close_focused(&mut self) {
        if self.root.leaf_count() <= 1 {
            return;
        }
        let ids = self.root.leaf_ids();
        let next_focus = {
            let pos = ids.iter().position(|&x| x == self.focused_id).unwrap_or(0);
            if pos > 0 {
                ids[pos - 1]
            } else {
                ids.get(1).copied().unwrap_or(0)
            }
        };
        let old_root = std::mem::replace(
            &mut self.root,
            TileNode::Leaf {
                id: 0,
                widget: WidgetKind::CpuOverview,
                mode: ViewMode::Default,
            },
        );
        if let Some(new_root) = old_root.remove_leaf(self.focused_id) {
            self.root = new_root;
        }
        self.focused_id = next_focus;
    }

    /// Cycle focus forward or backward through leaves.
    pub fn cycle_focus(&mut self, forward: bool) {
        let ids = self.root.leaf_ids();
        if ids.is_empty() {
            return;
        }
        let pos = ids.iter().position(|&x| x == self.focused_id).unwrap_or(0);
        let next = if forward {
            (pos + 1) % ids.len()
        } else {
            pos.checked_sub(1).unwrap_or(ids.len() - 1)
        };
        self.focused_id = ids[next];
    }

    /// Move focus spatially in a direction using the rendered rects.
    pub fn move_focus(&mut self, dir: FocusDir, rects: &[(usize, Rect)]) {
        let Some(&(_, cur_rect)) = rects.iter().find(|(id, _)| *id == self.focused_id) else {
            return;
        };
        let cx = cur_rect.x as i32 + cur_rect.width as i32 / 2;
        let cy = cur_rect.y as i32 + cur_rect.height as i32 / 2;

        let mut best: Option<(usize, i32)> = None;
        for &(id, rect) in rects {
            if id == self.focused_id {
                continue;
            }
            let rx = rect.x as i32 + rect.width as i32 / 2;
            let ry = rect.y as i32 + rect.height as i32 / 2;
            let dx = rx - cx;
            let dy = ry - cy;
            let is_candidate = match dir {
                FocusDir::Left => dx < -2,
                FocusDir::Right => dx > 2,
                FocusDir::Up => dy < -1,
                FocusDir::Down => dy > 1,
            };
            if !is_candidate {
                continue;
            }
            // Manhattan distance weighted toward the primary axis
            let dist = match dir {
                FocusDir::Left | FocusDir::Right => dx.abs() * 1 + dy.abs() * 3,
                FocusDir::Up | FocusDir::Down => dy.abs() * 1 + dx.abs() * 3,
            };
            if best.map_or(true, |(_, d)| dist < d) {
                best = Some((id, dist));
            }
        }
        if let Some((id, _)) = best {
            self.focused_id = id;
        }
    }

    /// Adjust the split ratio of the pane containing the focused leaf.
    pub fn resize(&mut self, larger: bool) {
        let delta = if larger { 0.05_f32 } else { -0.05_f32 };
        self.root.adjust_ratio(self.focused_id, delta);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FocusDir {
    Left,
    Right,
    Up,
    Down,
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

fn split_rect(area: Rect, dir: SplitDir, ratio: f32) -> (Rect, Rect) {
    match dir {
        SplitDir::Horizontal => {
            let w1 = ((area.width as f32 * ratio) as u16).min(area.width.saturating_sub(1));
            let w2 = area.width - w1;
            let a = Rect {
                x: area.x,
                y: area.y,
                width: w1,
                height: area.height,
            };
            let b = Rect {
                x: area.x + w1,
                y: area.y,
                width: w2,
                height: area.height,
            };
            (a, b)
        }
        SplitDir::Vertical => {
            let h1 = ((area.height as f32 * ratio) as u16).min(area.height.saturating_sub(1));
            let h2 = area.height - h1;
            let a = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: h1,
            };
            let b = Rect {
                x: area.x,
                y: area.y + h1,
                width: area.width,
                height: h2,
            };
            (a, b)
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::WidgetKind;

    fn area() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 50,
        }
    }

    #[test]
    fn default_layout_has_five_panes() {
        let tree = TileTree::default_layout();
        assert_eq!(tree.root.leaf_count(), 5);
    }

    #[test]
    fn default_layout_focused_on_first_pane() {
        let tree = TileTree::default_layout();
        assert_eq!(tree.focused_id, 0);
    }

    #[test]
    fn compute_rects_covers_entire_area() {
        let tree = TileTree::default_layout();
        let mut rects: Vec<(usize, Rect)> = Vec::new();
        tree.root.compute_rects(area(), &mut rects);
        assert_eq!(rects.len(), 5);
        // Total width of all leaf rects at any horizontal split should not exceed area width
        for (_, r) in &rects {
            assert!(r.right() <= area().right(), "rect overflows: {r:?}");
            assert!(r.bottom() <= area().bottom(), "rect overflows: {r:?}");
        }
    }

    #[test]
    fn split_h_increases_pane_count() {
        let mut tree = TileTree::default_layout();
        let before = tree.root.leaf_count();
        tree.split_h();
        assert_eq!(tree.root.leaf_count(), before + 1);
    }

    #[test]
    fn split_v_increases_pane_count() {
        let mut tree = TileTree::default_layout();
        let before = tree.root.leaf_count();
        tree.split_v();
        assert_eq!(tree.root.leaf_count(), before + 1);
    }

    #[test]
    fn close_focused_decreases_pane_count() {
        let mut tree = TileTree::default_layout();
        let before = tree.root.leaf_count();
        tree.close_focused();
        assert_eq!(tree.root.leaf_count(), before - 1);
    }

    #[test]
    fn close_last_pane_is_noop() {
        let mut tree = TileTree {
            root: TileNode::Leaf {
                id: 0,
                widget: WidgetKind::CpuOverview,
                mode: ViewMode::Default,
            },
            focused_id: 0,
            next_id: 1,
        };
        tree.close_focused();
        assert_eq!(tree.root.leaf_count(), 1);
    }

    #[test]
    fn cycle_focus_wraps_around() {
        let mut tree = TileTree::default_layout();
        let ids = tree.root.leaf_ids();
        // Move to the last leaf
        for _ in 0..ids.len() {
            tree.cycle_focus(true);
        }
        // Should have wrapped back to start
        assert_eq!(tree.focused_id, ids[0]);
    }

    #[test]
    fn cycle_widget_changes_kind() {
        let mut tree = TileTree::default_layout();
        let before = tree.root.widget_for_id(tree.focused_id).cloned();
        tree.cycle_widget();
        let after = tree.root.widget_for_id(tree.focused_id).cloned();
        assert_ne!(before, after);
    }

    #[test]
    fn resize_clamps_ratio() {
        let mut tree = TileTree::default_layout();
        // Expand 100 times — ratio should stay <= 0.9
        for _ in 0..100 {
            tree.resize(true);
        }
        // After many resizes the tree should still be renderable (no panic)
        let mut rects: Vec<(usize, Rect)> = Vec::new();
        tree.root.compute_rects(area(), &mut rects);
        assert!(!rects.is_empty());
    }

    #[test]
    fn widget_kind_cycle_is_exhaustive() {
        // Every WidgetKind should cycle back to itself after 19 steps
        let start = WidgetKind::CpuOverview;
        let mut current = start.clone();
        for _ in 0..19 {
            current = current.next();
        }
        assert_eq!(current, start);
    }
}
