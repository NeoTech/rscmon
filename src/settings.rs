//! Persistent dashboard settings.
//!
//! Saved to and loaded from `~/.rscmon/settings.yaml`.
//! The file stores the full tile tree (widget assignments, split directions,
//! split ratios) and the focused pane ID so the dashboard is restored exactly
//! as the user left it.
//!
//! All errors are handled gracefully:
//!   - If the file does not exist on load, the default layout is used silently.
//!   - If the file is malformed, a warning is logged and the default is used.
//!   - If the file cannot be written, a warning is logged and the error is ignored.

use anyhow::Result;
use std::path::PathBuf;

use crate::ui::tiling::TileTree;

// ---------------------------------------------------------------------------
// Settings struct
// ---------------------------------------------------------------------------

/// Everything that needs to survive across sessions.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// The complete BSP tile tree, including every pane's widget and view mode.
    pub tiles: TileTree,
    /// The pane ID that was focused when the app last exited.
    pub focused_id: usize,
}

impl Settings {
    /// Build a `Settings` snapshot from a live `TileTree`.
    pub fn from_tile_tree(tree: &TileTree) -> Self {
        // TileTree does not implement Clone, so we serialise/deserialise to
        // produce an owned copy.  This only happens once on quit so the cost
        // is irrelevant.
        let yaml = serde_yaml::to_string(tree).expect("TileTree serialisation should never fail");
        let tiles: TileTree =
            serde_yaml::from_str(&yaml).expect("Round-trip deserialisation should never fail");
        Self {
            focused_id: tree.focused_id,
            tiles,
        }
    }
}

// ---------------------------------------------------------------------------
// File paths
// ---------------------------------------------------------------------------

/// Returns `~/.rscmon/settings.yaml`, or `None` if the home directory cannot
/// be determined.
pub fn settings_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join(".rscmon").join("settings.yaml"))
}

/// Cross-platform home directory.
fn home_dir() -> Option<PathBuf> {
    // std::env::home_dir is deprecated but still works on all platforms we
    // care about.  We prefer the HOME / USERPROFILE environment variables
    // ourselves to avoid any future breakage.
    if let Some(h) = std::env::var_os("USERPROFILE") {
        return Some(PathBuf::from(h));
    }
    if let Some(h) = std::env::var_os("HOME") {
        return Some(PathBuf::from(h));
    }
    // Last resort
    #[allow(deprecated)]
    std::env::home_dir()
}

// ---------------------------------------------------------------------------
// Load
// ---------------------------------------------------------------------------

/// Try to load saved settings.
///
/// Returns `Ok(Some(settings))` on success, `Ok(None)` when no file exists,
/// and logs a warning (returning `Ok(None)`) on parse errors so the caller
/// can always fall back to the default layout.
pub fn load() -> Option<Settings> {
    let path = settings_path()?;

    if !path.exists() {
        return None;
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("Could not read settings file {}: {e}", path.display());
            return None;
        }
    };

    match serde_yaml::from_str::<Settings>(&text) {
        Ok(s) => {
            log::info!("Loaded settings from {}", path.display());
            Some(s)
        }
        Err(e) => {
            log::warn!(
                "Settings file {} is malformed and will be ignored: {e}",
                path.display()
            );
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

/// Serialise `settings` and write it to `~/.rscmon/settings.yaml`.
///
/// Creates the `~/.rscmon/` directory if it does not exist.
/// Logs a warning on any I/O or serialisation error but never panics.
pub fn save(settings: &Settings) -> Result<()> {
    let path = match settings_path() {
        Some(p) => p,
        None => {
            log::warn!("Cannot determine home directory; settings not saved.");
            return Ok(());
        }
    };

    // Ensure the parent directory exists.
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::warn!("Could not create settings directory {}: {e}", dir.display());
            return Ok(());
        }
    }

    let yaml = match serde_yaml::to_string(settings) {
        Ok(y) => y,
        Err(e) => {
            log::warn!("Could not serialise settings: {e}");
            return Ok(());
        }
    };

    if let Err(e) = std::fs::write(&path, yaml.as_bytes()) {
        log::warn!("Could not write settings to {}: {e}", path.display());
    } else {
        log::info!("Settings saved to {}", path.display());
    }

    Ok(())
}
