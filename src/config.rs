//! App state persisted between runs (CORE §3): chosen theme, language, sort
//! mode and window size.
//!
//! `config.toml` lives in the platform's data directory. Until Chron9 that was
//! also where `products/` was, and the two were described as one rsync-friendly
//! tree; they can now be told apart, because this file holds the `vault` key
//! that says where `products/` is and therefore cannot live inside it.
//!
//! A *missing* config is never fatal — the defaults take over and the next save
//! writes the file. A config that exists and cannot be read or parsed is
//! reported rather than defaulted, which is a Chron9 change and is explained on
//! [`Config::load`]: the defaults now include an opinion about where the user's
//! documents are.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::data;

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
    /// Where `products/` lives, when the user has moved it (Chron9).
    ///
    /// Absent — the ordinary case, and every install before this key existed —
    /// means the vault is the data directory this file is already in. The key
    /// has to live here rather than in the vault for the obvious reason: the app
    /// would need the vault's location in order to read the setting that gives
    /// it the vault's location.
    ///
    /// A `String` rather than a `PathBuf` because that is what TOML can hold. On
    /// Linux a path is bytes with no encoding guarantee, so a path that is not
    /// valid UTF-8 cannot be written down here at all; it is refused when it is
    /// chosen rather than lossily converted into one that nearly works.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
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
            // No vault key means the vault is the directory this file is in,
            // which is what every install had before Chron9 and is what an
            // install that never touches the feature keeps having.
            vault: None,
        }
    }
}

impl Config {
    /// Read `config.toml`.
    ///
    /// **A file that is absent and a file that will not parse stopped being the
    /// same thing in Chron9,** and the reason is the `vault` key. Before it,
    /// this function was infallible: anything it could not read became
    /// `Config::default()`, the app opened on Default Dark, and the user noticed
    /// a wrong theme and shrugged. With a `vault` key that same fallback yields
    /// `vault: None` — so the app points at the *default* vault, shows whatever
    /// is or is not there, and never mentions the drive the products are
    /// actually on. Nothing on screen would be wrong, exactly; it would simply
    /// be describing a different vault, and the user would have no way to tell.
    ///
    /// That is not the missing-vault case [`crate::data::Paths::ensure`] guards.
    /// There, the vault is gone and the app says so. Here the vault is fine and
    /// the *pointer* is gone, which is invisible unless this function refuses to
    /// guess.
    ///
    /// So: no file at all is the ordinary first run and yields the defaults,
    /// because a config that does not exist cannot have configured a vault. A
    /// file that exists and cannot be read or parsed is an error, and `main`
    /// turns it into a visible broken entry naming the file — the same treatment
    /// a `product.toml` that will not parse has had since Chron1. That the app's
    /// own config was the one file exempt from that rule is worth noticing; it
    /// only started to matter when the file gained a key that points somewhere.
    ///
    /// Individual *absent* fields still fall back on their own, because the
    /// struct is `#[serde(default)]`. It is a file that is not TOML, or a field
    /// whose type is wrong, that stops here.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            // Present but unreadable — a permission bit, a directory where a
            // file should be — is the same unknown as a file that will not
            // parse, and gets the same answer rather than a cheerful default.
            Err(e) => return Err(e.to_string()),
        };
        let mut config: Self =
            toml::from_str(&text).map_err(|e| crate::data::first_line(&e.to_string()))?;
        config.clamp_to_floor();
        Ok(config)
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
    ///
    /// Routed through [`data::write_atomic`] rather than a plain `fs::write`,
    /// which is what every product manifest already goes through and for the
    /// same reason: `fs::write` truncates the existing file before it writes a
    /// single byte of the new one, so an interruption between the two — and
    /// `save` runs during shutdown, exactly when a machine is likeliest to be
    /// powered off or a session killed — used to leave `config.toml` empty or
    /// half-written. That file holds the `vault` key: the only pointer to
    /// where the user's documents are, not just a theme.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        data::write_atomic(path, &text).map_err(|e| match e {
            data::DataError::Unreadable(detail) => detail,
            other => format!("{other:?}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `save` is now the same atomic write every product manifest already uses:
    /// this pins that the round trip still works, and that it leaves no
    /// `.config.toml.tmp` litter behind — the temp file `write_atomic` cleans up
    /// on success, unlike the old `fs::write` which never created one at all
    /// but also never protected the file it wrote either.
    #[test]
    fn save_round_trips_through_load_and_leaves_no_temporary_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let config = Config {
            lang: "tr".to_string(),
            vault: Some("/mnt/ironwolf/parachron".to_string()),
            ..Config::default()
        };
        config.save(&path).expect("a fresh config.toml must save");

        assert_eq!(Config::load(&path).unwrap(), config);

        let strays: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .filter(|name| name != "config.toml")
            .collect();
        assert!(strays.is_empty(), "temporary left behind: {strays:?}");
    }

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
            Ok(Config::default())
        );
    }

    /// Chron9. The one case that must stay cheerful: no file at all is a first
    /// run, and a config that does not exist cannot have named a vault.
    #[test]
    fn an_absent_file_is_not_an_error_because_it_cannot_have_named_a_vault() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(&dir.path().join("config.toml")).expect("absent is not an error");
        assert_eq!(config.vault, None);
    }

    /// Chron9, and the point of making `load` fallible.
    ///
    /// A file that will not parse used to yield the defaults, which cost a theme
    /// and nothing else. It now costs sight of the vault: `vault: None` points
    /// the app at the default one while the products sit on another disk, and
    /// nothing on screen would say so. So it is an error, and `main` shows it.
    #[test]
    fn a_config_that_will_not_parse_is_an_error_rather_than_a_silent_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "this is not = = toml\n").unwrap();

        let failure = Config::load(&path).expect_err("a broken config must not be guessed at");
        assert!(!failure.is_empty(), "the reason must be reportable");
    }

    /// The dangerous shape specifically: a file that *does* name a vault and
    /// then fails to parse. Guessing here points the app at the wrong disk.
    #[test]
    fn a_broken_config_that_names_a_vault_never_degrades_to_the_default_vault() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "vault = \"/mnt/ironwolf/parachron\"\nlang = [oops\n").unwrap();

        assert!(
            Config::load(&path).is_err(),
            "a config naming a vault must not fall back to the default vault"
        );
    }

    #[test]
    fn a_vault_path_round_trips_and_is_written_only_when_set() {
        let plain = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(
            !plain.contains("vault"),
            "an unconfigured vault writes no key:\n{plain}"
        );

        let moved = Config {
            vault: Some("/mnt/ironwolf/parachron".to_string()),
            ..Config::default()
        };
        let text = toml::to_string_pretty(&moved).unwrap();
        assert!(
            text.contains("vault"),
            "a configured vault is written:\n{text}"
        );
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), moved);
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
        assert!(
            !text.contains("query"),
            "config.toml grew a query field:\n{text}"
        );
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

        let config = Config::load(&path).expect("a valid config must load");

        // Proof that the file was read at all, and it has to come first. An
        // absent config still hands back `Config::default()` — which is
        // 1280×800, already above the floor — so a test that only looked at the
        // size would pass just as happily if the file had never been read, or if
        // the clamp were deleted outright.
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

        let config = Config::load(&path).expect("a valid config must load");

        // Both numbers differ from the 1280×800 defaults, so these assertions
        // also fail if the file went unread — no separate guard needed here.
        assert_eq!(config.window_width, 1600);
        assert_eq!(config.window_height, 1000);
    }
}
