// Parachron — a desktop vault for purchases.
//
// Wire-up only: resolve the vault, scan it, fill the string table, hand the
// result to the UI, run. The interesting work lives in `data`, `config` and
// `strings`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod data;
mod render;
mod strings;
mod viewer;

use slint::{ModelRc, VecModel};

use config::Config;
use data::{DataError, Entry, Paths};
use strings::{Key, Lang};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let (paths, entries) = open_vault();

    let settings = paths
        .as_ref()
        .map(|paths| Config::load(&paths.config))
        .unwrap_or_default();
    let lang = Lang::from_code(&settings.lang);

    let app = AppWindow::new()?;
    apply_strings(&app, lang);

    let rows: Vec<ProductItem> = entries.iter().map(|entry| row(entry, lang)).collect();
    app.set_products(ModelRc::new(VecModel::from(rows)));

    // Column 2. Kept alive for the life of the window — dropping it stops the
    // render thread.
    let products_root = paths
        .as_ref()
        .map(|paths| paths.products.clone())
        .unwrap_or_default();
    let _viewer = viewer::install(&app, products_root, entries, lang);

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
        if let Err(detail) = persist(&paths.config, settings, lang, size.width, size.height) {
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
    width: f32,
    height: f32,
) -> Result<(), String> {
    Config {
        // Normalise on the way out, so an unrecognised `lang` is rewritten as
        // the English it already fell back to.
        lang: lang.code().to_string(),
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

/// Turn one vault entry into a list row.
///
/// Every string a row carries — prefixes included — is assembled from the
/// string table, so the `.slint` side never holds text of its own.
fn row(entry: &Entry, lang: Lang) -> ProductItem {
    match entry {
        Entry::Ok(product) => {
            let incomplete = !product.missing_pdfs.is_empty();
            let prefix = if incomplete {
                tr(lang, Key::WarnPrefix)
            } else {
                ""
            };
            let detail = if incomplete {
                format!(
                    "{}: {}",
                    tr(lang, Key::MissingFiles),
                    product.missing_pdfs.join(", ")
                )
            } else {
                String::new()
            };

            ProductItem {
                label: format!("{prefix}{}", product.name).into(),
                name: product.name.clone().into(),
                detail: detail.into(),
                broken: false,
                warning: incomplete,
            }
        }
        Entry::Broken { folder, reason } => {
            // A failure with no folder behind it (no home directory) falls back
            // to the generic heading.
            let heading = tr(lang, Key::BrokenTitle);
            let label = if folder.is_empty() {
                format!("{}{heading}", tr(lang, Key::BrokenPrefix))
            } else {
                format!("{}{folder}", tr(lang, Key::BrokenPrefix))
            };
            let name = if folder.is_empty() {
                heading.to_string()
            } else {
                format!("{heading}: {folder}")
            };

            ProductItem {
                label: label.into(),
                name: name.into(),
                detail: describe(lang, reason).into(),
                broken: true,
                warning: false,
            }
        }
    }
}

/// Render a [`DataError`] as readable text in the chosen language. The trailing
/// detail is diagnostic payload from the OS or the TOML parser and stays as-is.
fn describe(lang: Lang, error: &DataError) -> String {
    match error {
        DataError::NoHome => tr(lang, Key::ErrNoHome).to_string(),
        DataError::MissingToml => tr(lang, Key::ErrMissingToml).to_string(),
        DataError::Unreadable(detail) => format!("{}: {detail}", tr(lang, Key::ErrUnreadable)),
        DataError::Malformed(detail) => format!("{}: {detail}", tr(lang, Key::ErrMalformed)),
        DataError::InvalidDate { field, detail } => {
            format!("{} ({field}): {detail}", tr(lang, Key::ErrInvalidDate))
        }
    }
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
    use time::{Date, Month};

    fn product(name: &str, missing: &[&str]) -> data::Product {
        let date = Date::from_calendar_date(2026, Month::March, 14).unwrap();
        data::Product {
            folder: name.to_string(),
            name: name.to_string(),
            serial: String::new(),
            link: String::new(),
            purchase_date: date,
            warranty_start: date,
            warranty_end: date,
            pdfs: Vec::new(),
            added: date,
            missing_pdfs: missing.iter().map(|name| name.to_string()).collect(),
        }
    }

    #[test]
    fn a_healthy_product_row_carries_no_prefix_and_no_detail() {
        let item = row(&Entry::Ok(product("Monitor", &[])), Lang::En);
        assert_eq!(item.label, "Monitor");
        assert_eq!(item.name, "Monitor");
        assert!(item.detail.is_empty());
        assert!(!item.broken);
    }

    #[test]
    fn missing_files_flag_the_row_without_breaking_it() {
        let item = row(&Entry::Ok(product("Drive", &["invoice.pdf"])), Lang::En);
        assert!(item.label.starts_with(strings::get(Lang::En, Key::WarnPrefix)));
        assert!(item.detail.contains("invoice.pdf"));
        assert!(!item.broken);
    }

    #[test]
    fn a_broken_folder_stays_visible_and_explains_itself() {
        let item = row(
            &Entry::Broken {
                folder: "test-broken".to_string(),
                reason: DataError::MissingToml,
            },
            Lang::En,
        );
        assert!(item.broken);
        assert!(item.label.contains("test-broken"));
        assert_eq!(item.detail, strings::get(Lang::En, Key::ErrMissingToml));
    }

    #[test]
    fn exiting_writes_the_session_state_back() {
        let path = std::env::temp_dir().join("parachron-persist-test.toml");
        let _ = std::fs::remove_file(&path);

        // An unrecognised language on the way in is normalised on the way out.
        let stale = Config {
            lang: "klingon".to_string(),
            theme: "noctalia".to_string(),
            ..Config::default()
        };
        persist(&path, stale, Lang::En, 1440.0, 910.0).expect("config must save");

        let reloaded = Config::load(&path);
        assert_eq!(reloaded.lang, "en");
        assert_eq!(reloaded.theme, "noctalia", "unrelated settings survive");
        assert_eq!(reloaded.window_width, 1440);
        assert_eq!(reloaded.window_height, 910);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rows_translate() {
        let entry = Entry::Broken {
            folder: "bozuk".to_string(),
            reason: DataError::MissingToml,
        };
        assert_eq!(
            row(&entry, Lang::Tr).detail,
            strings::get(Lang::Tr, Key::ErrMissingToml)
        );
    }
}
