// Parachron — a desktop vault for purchases.
//
// Wire-up only: resolve the vault, scan it, fill the string table, hand the
// result to the UI, run. The interesting work lives in `data`, `vault`,
// `viewer` and `config`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod config;
mod data;
mod details;
mod editor;
mod export;
mod import;
mod lang;
mod relocate;
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

    // Resolving the vault now reads `config.toml` on the way, because the
    // vault's location is in it (Chron9). Still single-threaded file I/O, so
    // `local_offset` above keeps the ordering Chron4 requires.
    let (paths, settings, entries) = open_vault();

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

    // The About pane. Four values pushed once and two clipboard callbacks — it
    // takes no language, because nothing it pushes is UI copy.
    let _about = about::install(&app);
    // Chron9. Pushed here rather than inside `install` because it is the one
    // value in the pane that can change while the window is open.
    about::set_vault(&app, paths.as_ref().map(|paths| paths.vault.as_path()));

    // The add/edit sheet. It hands finished work to the vault, which is what
    // puts it on screen.
    let editors = editor::install(
        &app,
        products_root.clone(),
        lang,
        Rc::clone(&vault),
        Rc::clone(&viewer),
    );

    // Column 3's other button. It reads the selected product from the vault and
    // writes wherever the user points it, on a thread of its own.
    let exports = export::install(&app, products_root, lang, offset, Rc::clone(&vault));

    // Moving the vault (Chron9). It reaches the four owners of the products root
    // so that a finished move retargets all of them and re-reads the disk once —
    // the same single-route arrangement Chron6 established for the language.
    //
    // Installed only when there is a vault to move *from*. A missing home
    // directory, a `config.toml` that would not parse, or a configured vault
    // that is not there all leave `paths` as `None`, and moving needs a source.
    let relocations = paths.as_ref().map(|paths| {
        relocate::install(
            &app,
            paths.clone(),
            lang,
            Rc::clone(&vault),
            Rc::clone(&viewer),
            editors.clone(),
            exports.clone(),
        )
    });

    // The language switch, last, because it is the only thing that needs to reach
    // all five of the above. What it returns is where the session's language lives
    // from here on — `lang` the local is only the value it started at.
    let language = lang::install(
        &app,
        lang,
        Rc::clone(&vault),
        viewer,
        editors,
        Rc::clone(&themes),
        exports,
        relocations.clone(),
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
            // Chron9, and read from its owner like every other field here. A
            // move that happened during the session changed this, and taking it
            // from the config loaded at startup would write the old location
            // back over the new one — the fourth time that shape would have bit.
            vault: relocations
                .as_ref()
                .map(|r| r.current())
                .unwrap_or_else(|| settings.vault.clone()),
        };
        // Reported to stderr and nowhere else, and Chron8 looked at whether that
        // could be better rather than leaving it to be rediscovered.
        //
        // It cannot, without changing what shutdown means. This runs after
        // `app.hide()`, so there is no window left to put a message in; showing
        // one would turn "the app is closing" into "the app is asking you
        // something", which is the wrong trade for a preferences file. Saving
        // *before* hiding would mean writing a window size the user is still able
        // to change.
        //
        // So the limitation is written down instead of fixed: on Linux this
        // reaches a terminal if the app was launched from one, and on a release
        // Windows build `windows_subsystem = "windows"` means there is no stderr
        // at all and a failed config save is silent. The vault itself is never at
        // risk — this is the file that remembers a theme and a window size.
        if let Err(detail) = persist(&paths.config, &session) {
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
/// `Clone` but no longer `Copy`: Chron9's `vault` is an owned `String`, so this
/// is passed by reference and `persist` clones the one field that needs it.
#[derive(Clone)]
struct Session {
    lang: Lang,
    sort: SortMode,
    theme: Theme,
    width: f32,
    height: f32,
    /// Where the vault is, as the session last knew it (Chron9).
    ///
    /// A field here rather than carried through from the loaded config, for the
    /// reason `persist` gives at length: a session can *change* this — that is
    /// the whole milestone — and a value read from the config at startup would
    /// silently write the old location back over the new one on exit. That is
    /// the same bug the sort mode, the theme and the language each had in turn,
    /// and this one would leave a user's documents in a folder the app no longer
    /// pointed at.
    vault: Option<String>,
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
fn persist(path: &std::path::Path, session: &Session) -> Result<(), String> {
    Config {
        // Normalised on the way out by construction: these come from typed
        // values, so an unrecognised string in the file is rewritten as the
        // default it already fell back to on load.
        lang: session.lang.code().to_string(),
        sort: session.sort.code().to_string(),
        theme: session.theme.code().to_string(),
        window_width: session.width as u32,
        window_height: session.height as u32,
        // Chron9's field, and the sixth this function names. It is a plain
        // carry-through when nothing moved the vault and the new location when
        // something did — either way it is read from the live owner, never from
        // the config `main` loaded at startup.
        vault: session.vault.clone(),
    }
    .save(path)
}

/// Resolve the vault, create it on first run, and read what is in it.
///
/// Any failure along the way becomes a visible broken entry rather than a
/// startup crash (CORE §3).
///
/// **The config is read in the middle of this, and that ordering is Chron9's.**
/// The vault's location comes out of `config.toml`, so the file has to be read
/// before there is a `products/` to scan — which is why this returns the
/// settings rather than leaving `main` to load them afterwards. There is one
/// place that knows how a vault is found, and it is this function.
///
/// A `None` for the paths means "do not write anything back on the way out".
/// That matters most in the middle case: if `config.toml` could not be parsed,
/// the app does not know what was in it, and saving a freshly-defaulted config
/// over the top would delete the `vault` line that names where the user's
/// documents are.
fn open_vault() -> (Option<Paths>, Config, Vec<Entry>) {
    let base = match Paths::resolve() {
        Ok(paths) => paths,
        Err(reason) => {
            return (
                None,
                Config::default(),
                vec![Entry::Broken {
                    folder: String::new(),
                    reason,
                }],
            );
        }
    };

    let settings = match Config::load(&base.config) {
        Ok(settings) => settings,
        Err(detail) => {
            let folder = base.config.display().to_string();
            return (
                None,
                Config::default(),
                vec![Entry::Broken {
                    folder,
                    reason: data::DataError::ConfigUnreadable(detail),
                }],
            );
        }
    };

    let paths = base.with_vault(settings.vault.as_deref());

    if let Err(reason) = paths.ensure() {
        // The vault's path, not the data directory's: when a configured vault is
        // missing this is the whole message, and `~/.local/share/parachron` is
        // the one directory the user certainly did not mean.
        let folder = paths.vault.display().to_string();
        return (None, settings, vec![Entry::Broken { folder, reason }]);
    }

    let entries = data::scan(&paths.products);
    (Some(paths), settings, entries)
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
    table.set_search_placeholder(tr(lang, Key::SearchPlaceholder).into());
    table.set_search_no_matches(tr(lang, Key::SearchNoMatches).into());
    table.set_search_clear(tr(lang, Key::SearchClear).into());
    table.set_about_glyph(tr(lang, Key::AboutGlyph).into());
    table.set_about_wordmark(tr(lang, Key::AboutWordmark).into());
    table.set_about_subtitle(tr(lang, Key::AboutSubtitle).into());
    table.set_about_maker(tr(lang, Key::AboutMaker).into());
    table.set_about_maker_name(tr(lang, Key::AboutMakerName).into());
    table.set_about_version(tr(lang, Key::AboutVersion).into());
    table.set_about_released(tr(lang, Key::AboutReleased).into());
    table.set_about_source(tr(lang, Key::AboutSource).into());
    table.set_about_source_url(tr(lang, Key::AboutSourceUrl).into());
    table.set_about_docs(tr(lang, Key::AboutDocs).into());
    table.set_about_docs_url(tr(lang, Key::AboutDocsUrl).into());
    table.set_about_not_links(tr(lang, Key::AboutNotLinks).into());
    table.set_about_license(tr(lang, Key::AboutLicense).into());
    table.set_about_read_license(tr(lang, Key::AboutReadLicense).into());
    table.set_about_motto(tr(lang, Key::AboutMotto).into());
    table.set_action_vault_location(tr(lang, Key::ActionVaultLocation).into());
    table.set_action_move(tr(lang, Key::ActionMove).into());
    table.set_relocate_title(tr(lang, Key::RelocateTitle).into());
    table.set_relocate_from(tr(lang, Key::RelocateFrom).into());
    table.set_relocate_to(tr(lang, Key::RelocateTo).into());
    table.set_about_vault(tr(lang, Key::AboutVault).into());
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
            vault: None,
        };
        persist(&path, &session).expect("config must save");

        let reloaded = Config::load(&path).expect("the config just written must load");
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

    /// Chron9, and the fourth time this shape has had to be tested.
    ///
    /// The sort mode, the theme and the language each shipped once with
    /// `persist` carrying the *loaded* value through instead of the session's,
    /// so a change made in the app never reached the disk. This field is the one
    /// where that bug would leave a user's documents in a folder the app had
    /// stopped pointing at — so it gets the same test, written the same way: a
    /// stale value on disk first, and the assertion is that it did not survive.
    #[test]
    fn a_vault_moved_during_the_session_is_the_one_that_gets_written() {
        let dir = tempfile::tempdir().expect("a temp dir must be available");
        let path = dir.path().join("config.toml");

        Config {
            vault: Some("/mnt/old-disk/parachron".to_string()),
            ..Config::default()
        }
        .save(&path)
        .expect("the stale config must be written first");

        let session = Session {
            lang: Lang::En,
            sort: SortMode::Added,
            theme: Theme::Dark,
            width: 1280.0,
            height: 800.0,
            vault: Some("/mnt/ironwolf/parachron".to_string()),
        };
        persist(&path, &session).expect("config must save");

        let reloaded = Config::load(&path).expect("the config just written must load");
        assert_eq!(
            reloaded.vault.as_deref(),
            Some("/mnt/ironwolf/parachron"),
            "the session's vault is written, not the one that was loaded"
        );
    }

    /// Moving back to the default writes no key at all rather than the default
    /// path spelled out, so the file a user reads matches the file a fresh
    /// install would have.
    #[test]
    fn a_vault_returned_to_the_default_writes_no_key() {
        let dir = tempfile::tempdir().expect("a temp dir must be available");
        let path = dir.path().join("config.toml");

        Config {
            vault: Some("/mnt/ironwolf/parachron".to_string()),
            ..Config::default()
        }
        .save(&path)
        .expect("the stale config must be written first");

        let session = Session {
            lang: Lang::En,
            sort: SortMode::Added,
            theme: Theme::Dark,
            width: 1280.0,
            height: 800.0,
            vault: None,
        };
        persist(&path, &session).expect("config must save");

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("vault"),
            "a default vault writes no key:\n{text}"
        );
        assert_eq!(
            Config::load(&path).expect("must load").vault,
            None,
            "and reads back as the default"
        );
    }
}
