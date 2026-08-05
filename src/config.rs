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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `"en"` or `"tr"` (CORE §4).
    pub lang: String,
    /// Theme id; the full set of palettes arrives in Chron5.
    pub theme: String,
    /// `"added"`, `"name"` or `"purchase"`; the toggles arrive in Chron4.
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
        fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
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

    #[test]
    fn round_trips_through_toml() {
        let config = Config {
            lang: "tr".to_string(),
            ..Config::default()
        };
        let text = toml::to_string_pretty(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }
}
