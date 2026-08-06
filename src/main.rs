// Parachron — a desktop vault for purchases.
//
// Wire-up only: resolve the vault, scan it, fill the string table, hand the
// result to the UI, run. The interesting work lives in `data`, `vault`,
// `viewer` and `config`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod data;
mod details;
mod editor;
mod import;
mod lang;
mod render;
mod strings;
mod theme;
mod vault;
mod viewer;

use std::rc::Rc;

use config::Config;
use data::{Entry, Paths};
use strings::{Key, Lang};
use theme::Theme;
use vault::SortMode;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    // First, before anything spawns a thread. `time` will not work the local
    // offset out once the process has more than one, and the render worker
    // starts with the window a few lines below.
    let offset = data::local_offset();

    let (paths, entries) = open_vault();

    let settings = paths
        .as_ref()
        .map(|paths| Config::load(&paths.config))
        .unwrap_or_default();
    let lang = Lang::from_code(&settings.lang);
    let sort = SortMode::from_code(&settings.sort);
    let theme = Theme::from_code(&settings.theme);

    let app = AppWindow::new()?;
    apply_strings(&app, lang);

    // Colours before anything measures or draws. `palette.slint`'s initializers
    // are Default Dark, so a default start paints the same frame either way and
    // there is no flash; a non-default theme is in place before the window is
    // shown, which is the other half of the same promise.
    let themes = theme::install(&app, theme, lang);

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
    let vault = vault::install(
        &app,
        products_root.clone(),
        entries,
        sort,
        lang,
        offset,
        Rc::clone(&viewer),
    );

    // Column 3. The vault computes its contents while it is already deciding
    // what the list looks like; this only wires up copying the link.
    let _details = details::install(&app, Rc::clone(&vault));

    // The add/edit sheet. It hands finished work to the vault, which is what
    // puts it on screen.
    let editors = editor::install(
        &app,
        products_root,
        lang,
        Rc::clone(&vault),
        Rc::clone(&viewer),
    );

    // The language switch, last, because it is the only thing that needs to reach
    // all four of the above. What it returns is where the session's language
    // lives from here on — `lang` the local is only the value it started at.
    let language = lang::install(
        &app,
        lang,
        Rc::clone(&vault),
        viewer,
        editors,
        Rc::clone(&themes),
    );

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
        // Every value read from its owner, never from the locals this function
        // started with. That is the third time this mattered: the sort mode,
        // then the theme, now the language.
        let session = Session {
            lang: language.get(),
            sort: vault.borrow().sort(),
            theme: themes.borrow().current(),
            width: size.width,
            height: size.height,
        };
        if let Err(detail) = persist(&paths.config, session) {
            eprintln!("{}: {detail}", tr(session.lang, Key::ErrConfigSave));
        }
    }

    Ok(())
}

/// What this run changed, gathered at the moment the window closes.
///
/// A struct rather than five positional arguments because `persist` had reached
/// six. Every field is read from whichever owner holds the live value — the sort
/// mode from the vault, the theme from `Themes`, the language from the cell
/// `lang::install` returns — because reading any of them from the local `main`
/// computed at startup is exactly the bug that shipped three times.
#[derive(Clone, Copy)]
struct Session {
    lang: Lang,
    sort: SortMode,
    theme: Theme,
    width: f32,
    height: f32,
}

/// Write the session's state back to `config.toml`.
///
/// **Every field is named, and there is no `..` spread.** There used to be one —
/// `..settings`, carrying the loaded config through for anything not overwritten
/// — and it is the direct cause of three bugs in a row: the sort mode never
/// reached the disk until Chron4 noticed, the theme would not have until Chron5
/// did, and the language would not in Chron6. Each time, a value the session had
/// changed was silently replaced by the value that was loaded.
///
/// Naming every field turns the next one into a compile error instead. `Config`
/// holds exactly the five things a session owns, so a sixth field cannot be added
/// without this function failing to build and somebody deciding, on purpose,
/// whether the session owns it. `..Config::default()` would be no better than
/// `..settings` — it would silently reset rather than silently carry.
fn persist(path: &std::path::Path, session: Session) -> Result<(), String> {
    Config {
        // Normalised on the way out by construction: these come from typed
        // values, so an unrecognised string in the file is rewritten as the
        // default it already fell back to on load.
        lang: session.lang.code().to_string(),
        sort: session.sort.code().to_string(),
        theme: session.theme.code().to_string(),
        window_width: session.width as u32,
        window_height: session.height as u32,
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
    table.set_action_edit_document(tr(lang, Key::ActionEditDocument).into());
    table.set_field_name(tr(lang, Key::FieldName).into());
    table.set_field_link(tr(lang, Key::FieldLink).into());
    table.set_field_purchase_date(tr(lang, Key::FieldPurchaseDate).into());
    table.set_field_warranty_start(tr(lang, Key::FieldWarrantyStart).into());
    table.set_field_warranty_end(tr(lang, Key::FieldWarrantyEnd).into());
    table.set_field_documents(tr(lang, Key::FieldDocuments).into());
    table.set_date_hint(tr(lang, Key::DateHint).into());
    table.set_action_add_pdf(tr(lang, Key::ActionAddPdf).into());
    table.set_action_save(tr(lang, Key::ActionSave).into());
    table.set_action_cancel(tr(lang, Key::ActionCancel).into());
    table.set_remove_glyph(tr(lang, Key::RemoveGlyph).into());
    table.set_checking(tr(lang, Key::Checking).into());
    table.set_no_documents_yet(tr(lang, Key::NoDocumentsYet).into());
    table.set_warranty_left(tr(lang, Key::WarrantyLeft).into());
    table.set_sort_name(tr(lang, Key::SortName).into());
    table.set_sort_purchase(tr(lang, Key::SortPurchase).into());
    table.set_sort_by_name(tr(lang, Key::SortByName).into());
    table.set_sort_by_purchase(tr(lang, Key::SortByPurchase).into());
    table.set_theme_title(tr(lang, Key::ThemeTitle).into());
    table.set_action_close(tr(lang, Key::ActionClose).into());
    table.set_check_glyph(tr(lang, Key::CheckGlyph).into());
    table.set_menu_language(tr(lang, Key::MenuLanguage).into());
    table.set_lang_english(tr(lang, Key::LangEnglish).into());
    table.set_lang_turkish(tr(lang, Key::LangTurkish).into());
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

        // A file already on disk holding values the session is about to replace,
        // two of which do not parse. Nothing in it may survive.
        Config {
            lang: "klingon".to_string(),
            theme: "noctalia".to_string(),
            sort: "sideways".to_string(),
            ..Config::default()
        }
        .save(&path)
        .expect("the stale config must be written first");

        let session = Session {
            lang: Lang::En,
            sort: SortMode::Name,
            theme: Theme::Latte,
            width: 1440.0,
            height: 910.0,
        };
        persist(&path, session).expect("config must save");

        let reloaded = Config::load(&path);
        assert_eq!(reloaded.lang, "en", "an unrecognised value is normalised");
        assert_eq!(reloaded.sort, "name", "the session's sort mode is written");
        // Chron5. This assertion used to read the other way round — `theme` was
        // the *unrelated* setting that survived a save, because `persist` carried
        // it through from load. That was the defect; a theme chosen in the picker
        // never reached the disk. Its changing is the evidence the plumbing
        // landed.
        assert_eq!(
            reloaded.theme, "catppuccin-latte",
            "the session's theme is written, not the one that was loaded"
        );
        assert_eq!(reloaded.window_width, 1440);
        assert_eq!(reloaded.window_height, 910);
    }
}
