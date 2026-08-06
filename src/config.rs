//! App state persisted between runs (CORE §3): chosen theme, language, sort
//! mode and window size.
//!
//! `config.toml` lives in the data dir beside `products/`, not in a separate
//! config dir — the whole vault is one rsync-friendly tree.
//!
//! A missing or malformed config is never fatal: the defaults take over and
//! the next save rewrites the file.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Window geometry the app opens at before the user has resized anything.
/// Comfortably above the 1000×700 floor CORE §4 sets.
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 800;

/// CORE §4's floor, in the one place Rust can enforce it.
///
/// `app.slint` declares the same numbers as `min-width`/`min-height`, and those
/// are a *constraint handed to a window manager* — which is free to be
/// approximate, and on Chron1's own evidence was: a stored 400×300 came up at
/// roughly 1280×700 rather than at 1000×700. That is not a bug to fix in Slint;
/// it is a reason not to have the floor exist in only one place.
///
/// What this catches that the `.slint` side cannot: `Config::load` defaults a
/// field that is *absent or unparseable*, and `window_width = 300` parses as a
/// `u32` perfectly well. Before Chron8 it went straight into `set_size`.
pub const MIN_WIDTH: u32 = 1000;
pub const MIN_HEIGHT: u32 = 700;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `"en"` or `"tr"` (CORE §4), switched at runtime from `Document ▾`.
    pub lang: String,
    /// One of the eleven theme ids from CORE §5; see `theme::Theme::code`.
    pub theme: String,
    /// `"added"`, `"name"` or `"purchase"`.
    pub sort: String,
    pub window_width: u32,
    pub window_height: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // CORE §4: English is always the default. The app never reads the
            // system locale — Turkish is only ever a deliberate user choice.
            lang: "en".to_string(),
            theme: "default-dark".to_string(),
            sort: "added".to_string(),
            window_width: DEFAULT_WIDTH,
            window_height: DEFAULT_HEIGHT,
        }
    }
}

impl Config {
    /// Read `config.toml`, falling back to defaults for anything unreadable or
    /// malformed. Individual bad fields fall back on their own, because the
    /// struct is `#[serde(default)]`.
    pub fn load(path: &Path) -> Self {
        let mut config: Self = fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        config.clamp_to_floor();
        config
    }

    /// Raise a stored window size up to CORE §4's floor.
    ///
    /// A pure function of two numbers, which is the point: the floor becomes
    /// something a test can check without a display, a window manager or a
    /// screenshot. Three milestones have now written down that the 1000×700
    /// minimum is enforced by a window manager the test harness does not have —
    /// this is the half of it that no longer needs one.
    ///
    /// It clamps up and never down. A window larger than the floor is the user
    /// having resized it, which is theirs to decide.
    fn clamp_to_floor(&mut self) {
        self.window_width = self.window_width.max(MIN_WIDTH);
        self.window_height = self.window_height.max(MIN_HEIGHT);
    }

    /// Write `config.toml`. Returns the OS message on failure so the caller can
    /// report it — a config that will not save must not take the app down.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_core() {
        let config = Config::default();
        assert_eq!(config.lang, "en");
        assert_eq!(config.theme, "default-dark");
        assert_eq!(config.sort, "added");
        assert!(config.window_width >= 1000 && config.window_height >= 700);
    }

    #[test]
    fn a_missing_file_yields_defaults() {
        assert_eq!(
            Config::load(Path::new("/nonexistent/parachron/config.toml")),
            Config::default()
        );
    }

    #[test]
    fn a_partial_file_keeps_defaults_for_absent_fields() {
        let config: Config = toml::from_str("theme = \"noctalia\"").unwrap();
        assert_eq!(config.theme, "noctalia");
        assert_eq!(config.lang, "en");
        assert_eq!(config.sort, "added");
    }

    /// The search query is deliberately not a setting (Chron8).
    ///
    /// A sort order that survives a restart reorders the list; a filter that
    /// survives one *hides* most of it, and an app that opens showing three of
    /// eleven products — with a search box the user has forgotten they filled in
    /// — has lost the other eight as far as they can tell. Asserted against the
    /// written file rather than the struct, because the thing that would break
    /// this is somebody adding a field, and a field is what shows up here.
    #[test]
    fn the_written_config_holds_no_search_query() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(!text.contains("query"), "config.toml grew a query field:\n{text}");
        // The five that are settings, so this test fails if one goes missing
        // rather than only if one is added.
        for key in ["lang", "theme", "sort", "window_width", "window_height"] {
            assert!(text.contains(key), "config.toml lost {key}:\n{text}");
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config {
            lang: "tr".to_string(),
            ..Config::default()
        };
        let text = toml::to_string_pretty(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    /// CORE §4's floor, checked without a display. Three milestones recorded
    /// that the 1000×700 minimum was enforced by a window manager the harness
    /// does not have, and the part that went untested with it is the arithmetic:
    /// `load` defaults a field that is *absent or unparseable*, and
    /// `window_width = 300` parses as a `u32` perfectly well. Before Chron8 it
    /// went straight into `set_size`, so a hand-edited or corrupted config could
    /// open a window narrower than the layout was ever designed for.
    #[test]
    fn a_stored_window_below_the_floor_is_raised_to_it_when_the_config_loads() {
        // The floor written out in full, so that lowering the constants is a
        // failing test rather than a silently weakened one — every other
        // assertion here is phrased in terms of MIN_WIDTH/MIN_HEIGHT.
        assert_eq!((MIN_WIDTH, MIN_HEIGHT), (1000, 700), "CORE §4's number");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "theme = \"noctalia\"\nwindow_width = 300\nwindow_height = 200\n",
        )
        .unwrap();

        let config = Config::load(&path);

        // Proof that the file was read at all, and it has to come first. `load`
        // swallows an unreadable or malformed config and hands back
        // `Config::default()` — which is 1280×800, already above the floor — so
        // a test that only looked at the size would pass just as happily if the
        // file had never been parsed, or if the clamp were deleted outright.
        assert_eq!(
            config.theme, "noctalia",
            "the config on disk was not the one that got loaded"
        );

        // Exactly the floor, not merely at or above it, for the same reason:
        // 1280×800 satisfies ">= 1000×700" too, and that is precisely what a
        // silent fallback would produce.
        assert_eq!(config.window_width, MIN_WIDTH);
        assert_eq!(config.window_height, MIN_HEIGHT);
    }

    /// The clamp raises and never lowers. A window larger than the floor is the
    /// user having dragged it there, which is theirs to decide — a loader that
    /// "corrected" 1600×1000 back down would undo a deliberate resize on every
    /// start, and the next save would write the correction to disk.
    #[test]
    fn a_window_above_the_floor_is_left_exactly_as_the_user_sized_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "window_width = 1600\nwindow_height = 1000\n").unwrap();

        let config = Config::load(&path);

        // Both numbers differ from the 1280×800 defaults, so these assertions
        // also fail if the file went unread — no separate guard needed here.
        assert_eq!(config.window_width, 1600);
        assert_eq!(config.window_height, 1000);
    }
}
