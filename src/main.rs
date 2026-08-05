// Parachron — a desktop vault for purchases.
//
// Wire-up only: resolve the vault, scan it, fill the string table, hand the
// result to the UI, run. The interesting work lives in `data`, `vault`,
// `viewer` and `config`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod data;
mod render;
mod strings;
mod vault;
mod viewer;

use std::rc::Rc;

use config::Config;
use data::{Entry, Paths};
use strings::{Key, Lang};
use vault::SortMode;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let (paths, entries) = open_vault();

    let settings = paths
        .as_ref()
        .map(|paths| Config::load(&paths.config))
        .unwrap_or_default();
    let lang = Lang::from_code(&settings.lang);
    let sort = SortMode::from_code(&settings.sort);

    let app = AppWindow::new()?;
    apply_strings(&app, lang);

    let products_root = paths
        .as_ref()
        .map(|paths| paths.products.clone())
        .unwrap_or_default();

    // Column 2. Kept alive for the life of the window — dropping it stops the
    // render thread.
    let viewer = Rc::new(viewer::install(&app, products_root.clone(), lang));

    // Column 1, and the only writer of the product list. Filling the window is
    // part of installing it, so there is one code path for "show the list"
    // rather than one for startup and another for everything after.
    let _vault = vault::install(&app, products_root, entries, sort, lang, viewer);

    // Show first, then resize. Sizing an unshown window is silently discarded:
    // `preferred-width`/`preferred-height` from `app.slint` win when the window
    // is first mapped, so the saved size never took effect.
    //
    // Logical pixels throughout: `min-width`/`min-height` in the `.slint` file
    // are logical too, so a HiDPI display cannot make the stored size mean
    // something different from the declared floor.
    app.show()?;
    app.window().set_size(slint::LogicalSize::new(
        settings.window_width as f32,
        settings.window_height as f32,
    ));

    slint::run_event_loop()?;
    app.hide()?;

    // Persist whatever the session changed. A config that will not save is
    // reported, never fatal.
    if let Some(paths) = &paths {
        let size = app.window().size().to_logical(app.window().scale_factor());
        if let Err(detail) = persist(&paths.config, settings, lang, sort, size.width, size.height) {
            eprintln!("{}: {detail}", tr(lang, Key::ErrConfigSave));
        }
    }

    Ok(())
}

/// Write the session's state back to `config.toml`.
fn persist(
    path: &std::path::Path,
    settings: Config,
    lang: Lang,
    sort: SortMode,
    width: f32,
    height: f32,
) -> Result<(), String> {
    Config {
        // Normalise on the way out, so an unrecognised `lang` or `sort` is
        // rewritten as the default it already fell back to.
        lang: lang.code().to_string(),
        sort: sort.code().to_string(),
        window_width: width as u32,
        window_height: height as u32,
        ..settings
    }
    .save(path)
}

/// Resolve the vault, create it on first run, and read what is in it.
///
/// Any failure along the way becomes a visible broken entry rather than a
/// startup crash (CORE §3).
fn open_vault() -> (Option<Paths>, Vec<Entry>) {
    let paths = match Paths::resolve() {
        Ok(paths) => paths,
        Err(reason) => {
            return (
                None,
                vec![Entry::Broken {
                    folder: String::new(),
                    reason,
                }],
            );
        }
    };

    if let Err(reason) = paths.ensure() {
        let folder = paths.data.display().to_string();
        return (None, vec![Entry::Broken { folder, reason }]);
    }

    let entries = data::scan(&paths.products);
    (Some(paths), entries)
}

/// Fill the Slint string table. Called again whenever the language changes
/// (Chron6).
fn apply_strings(app: &AppWindow, lang: Lang) {
    let table = app.global::<Strings>();
    table.set_app_title(tr(lang, Key::AppTitle).into());
    table.set_menu_document(tr(lang, Key::MenuDocument).into());
    table.set_action_add_document(tr(lang, Key::ActionAddDocument).into());
    table.set_action_theme(tr(lang, Key::ActionTheme).into());
    table.set_action_export(tr(lang, Key::ActionExport).into());
    table.set_nav_about(tr(lang, Key::NavAbout).into());
    table.set_list_empty(tr(lang, Key::ListEmpty).into());
    table.set_select_prompt(tr(lang, Key::SelectPrompt).into());
    table.set_details_placeholder(tr(lang, Key::DetailsPlaceholder).into());
    table.set_no_documents(tr(lang, Key::NoDocuments).into());
    table.set_rendering(tr(lang, Key::Rendering).into());
    table.set_prev_page(tr(lang, Key::PrevPage).into());
    table.set_next_page(tr(lang, Key::NextPage).into());
    table.set_zoom_label(tr(lang, Key::ZoomLabel).into());
    table.set_serial_label(tr(lang, Key::SerialLabel).into());
    table.set_copied(tr(lang, Key::Copied).into());
    table.set_prev_glyph(tr(lang, Key::PrevGlyph).into());
    table.set_next_glyph(tr(lang, Key::NextGlyph).into());
    table.set_copy_glyph(tr(lang, Key::CopyGlyph).into());
}

/// Shorthand for a string-table lookup.
fn tr(lang: Lang, key: Key) -> &'static str {
    strings::get(lang, key)
}

#[cfg(test)]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exiting_writes_the_session_state_back() {
        let dir = tempfile::tempdir().expect("a temp dir must be available");
        let path = dir.path().join("config.toml");

        // An unrecognised language on the way in is normalised on the way out.
        let stale = Config {
            lang: "klingon".to_string(),
            theme: "noctalia".to_string(),
            sort: "sideways".to_string(),
            ..Config::default()
        };
        persist(&path, stale, Lang::En, SortMode::Name, 1440.0, 910.0).expect("config must save");

        let reloaded = Config::load(&path);
        assert_eq!(reloaded.lang, "en");
        assert_eq!(reloaded.sort, "name", "the session's sort mode is written");
        assert_eq!(reloaded.theme, "noctalia", "unrelated settings survive");
        assert_eq!(reloaded.window_width, 1440);
        assert_eq!(reloaded.window_height, 910);
    }
}
