//! The About view (CORE §4).
//!
//! Four values and two clipboard callbacks. That is the whole module, and the
//! smallness is the design rather than an accident.
//!
//! Every label in the pane is bound to the `Strings` global in `about.slint`, so
//! `apply_strings` relabels it on a language switch like anything else. Every
//! value this module pushes — the version, the build date, the licence id — is
//! language-independent, so nothing here goes stale and About needs no
//! `set_lang` and no row in `lang.rs`'s table of Rust-composed sites.
//!
//! That is worth stating because About is the first surface in the app that is
//! neither bound-only nor protected by a sheet. `lang.rs` warns that the form
//! and the picker only stay fresh because their backdrops make `Document ▾`
//! unreachable; About leaves column 1 and the title bar live. The way past that
//! is to have nothing that needs refreshing, not to add a sixth refresh path.

use std::rc::Rc;

use slint::{ComponentHandle, Timer, TimerMode};
use std::time::Duration;

use crate::AppWindow;
use crate::data;
use crate::strings::{self, Key, Lang};

/// How long a "copied" confirmation stays up.
///
/// The third copy of this number — `details.rs` and `viewer.rs` have the other
/// two — and the three are deliberately the same gesture, so they should be the
/// same duration. Lifting them into one place is a tidy-up this milestone did
/// not take, and the note is here so the next person does not assume they were
/// meant to differ.
const COPIED_LINGER: Duration = Duration::from_millis(1500);

/// Kept alive for the life of the window: dropping the timers would cancel a
/// confirmation mid-flight.
pub struct About {
    _source_copied: Rc<Timer>,
    _docs_copied: Rc<Timer>,
}

/// The version, as Cargo knows it.
///
/// Read from the manifest at compile time rather than written down a second
/// time, so the About pane and `Cargo.toml` cannot disagree (CORE §4).
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The licence, likewise — `AGPL-3.0-only` lives in `Cargo.toml` and nowhere
/// else. It is not in the string table because it is an SPDX identifier, not a
/// word: it is the same eleven characters in every language.
fn license_id() -> &'static str {
    env!("CARGO_PKG_LICENSE")
}

/// The build date, rendered the way Parachron renders every date.
///
/// `build.rs` emits it as ISO, which is CORE §3's storage form, and this is the
/// display half of that same rule going through the same `fmt_date` the details
/// column and the export summary use. A date that would not parse falls back to
/// the raw string rather than to an empty row — a stamp nobody can read still
/// beats a blank line where a date should be.
fn released() -> String {
    let raw = env!("PARACHRON_BUILD_DATE");
    let iso = time::macros::format_description!("[year]-[month]-[day]");
    match time::Date::parse(raw, &iso) {
        Ok(date) => data::fmt_date(date),
        Err(_) => raw.to_string(),
    }
}

/// The full licence text, bundled into the binary.
///
/// `include_str!` rather than a path read at runtime, because an installed
/// binary has no repository beside it — CORE §7 puts the file under
/// `/usr/share/licences/` for the distribution's tooling, and this is the copy
/// the About pane shows. It is a legal instrument quoted verbatim and therefore
/// not UI copy: it does not belong in `strings.rs` and must not be translated.
fn license_text() -> &'static str {
    include_str!("../LICENSE")
}

/// Copy `text`, and report whether it landed.
///
/// Same stance as the purchase link and the serial strip: a clipboard that will
/// not open is not worth taking the app down for, and a confirmation is only
/// shown when there is something to confirm.
fn copy(text: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_string()))
        .is_ok()
}

pub fn install(app: &AppWindow) -> About {
    // `app_*` rather than `about_*`, because the `Strings` global already has an
    // `about-version` and an `about-released` — those are the *labels*, "Version"
    // and "Release date", and they translate. These are the values beside them
    // and do not. Two names that differ by nothing but their object would be a
    // trap for whoever edits this next.
    app.set_app_version(version().into());
    app.set_app_released(released().into());
    app.set_app_license_id(license_id().into());
    app.set_app_license_text(license_text().into());

    let source_copied = Rc::new(Timer::default());
    let docs_copied = Rc::new(Timer::default());

    app.on_about_copy_source({
        let copied = Rc::clone(&source_copied);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            // `Lang::En` is not a guess. Both URL keys are listed in
            // `SAME_IN_BOTH`, and a test asserts their two sides are identical —
            // an address is not translated, so which language is asked makes no
            // difference and asking for the window's would be machinery
            // pretending to matter.
            if !copy(strings::get(Lang::En, Key::AboutSourceUrl)) {
                return;
            }
            app.set_about_source_copied(true);
            let weak = app.as_weak();
            copied.start(TimerMode::SingleShot, COPIED_LINGER, move || {
                if let Some(app) = weak.upgrade() {
                    app.set_about_source_copied(false);
                }
            });
        }
    });

    app.on_about_copy_docs({
        let copied = Rc::clone(&docs_copied);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            if !copy(strings::get(Lang::En, Key::AboutDocsUrl)) {
                return;
            }
            app.set_about_docs_copied(true);
            let weak = app.as_weak();
            copied.start(TimerMode::SingleShot, COPIED_LINGER, move || {
                if let Some(app) = weak.upgrade() {
                    app.set_about_docs_copied(false);
                }
            });
        }
    });

    About {
        _source_copied: source_copied,
        _docs_copied: docs_copied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_and_licence_come_from_the_manifest() {
        // Not asserted against a literal `"0.1.0"`: that would be the second
        // place the version is written down, which is the thing this avoids.
        assert!(!version().is_empty());
        assert!(version().contains('.'));
        assert_eq!(license_id(), "AGPL-3.0-only");
    }

    #[test]
    fn the_build_date_is_stamped_and_displayable() {
        let shown = released();
        // `DD-MM-YYYY` (CORE §3's display form), so ten characters with hyphens
        // in the third and sixth. If `build.rs` ever emits something unparseable
        // this fails here rather than showing an ISO date in the pane.
        assert_eq!(shown.len(), 10, "unexpected build date: {shown}");
        assert_eq!(shown.as_bytes()[2], b'-');
        assert_eq!(shown.as_bytes()[5], b'-');
    }

    #[test]
    fn the_bundled_licence_is_the_whole_agpl() {
        let text = license_text();
        assert!(text.contains("GNU AFFERO GENERAL PUBLIC LICENSE"));
        // The last section, so a truncated include is caught rather than a
        // merely non-empty one passing.
        assert!(text.contains("END OF TERMS AND CONDITIONS"));
        assert!(
            text.len() > 30_000,
            "licence looks truncated: {}",
            text.len()
        );
    }

    #[test]
    fn the_two_addresses_are_the_same_in_both_languages() {
        // What `install` relies on when it reads them as `Lang::En`.
        for key in [Key::AboutSourceUrl, Key::AboutDocsUrl] {
            assert_eq!(strings::get(Lang::En, key), strings::get(Lang::Tr, key));
        }
    }
}
