//! The acceptance criteria that need a real element tree, driven headlessly.
//!
//! Everything lives in one test function on purpose: the Slint testing backend
//! is installed process-wide and its components are not `Send`, so a single
//! test keeps every window on one thread. Later milestones add sections to it
//! rather than test functions beside it.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use i_slint_backend_testing as testing;
use slint::{ComponentHandle, LogicalSize, Model, ModelRc, VecModel};
use time::{Date, Month};

use crate::data::{DataError, Entry, Product};
use crate::strings::{Key, Lang};
use crate::theme::{Theme, Themes};
use crate::vault::{SortMode, Vault};
use crate::{AppWindow, Palette, ProductItem};

/// Widths must land exactly on 25 / 50 / 25 (CORE §4), so no tolerance here
/// beyond float noise.
fn assert_columns(app: &AppWindow, width: f32) {
    let col = |id: &str| {
        testing::ElementHandle::find_by_element_id(app, id)
            .next()
            .unwrap_or_else(|| panic!("element {id} not realised"))
            .size()
            .width
    };

    let (c1, c2, c3) = (
        col("AppWindow::col1"),
        col("AppWindow::col2"),
        col("AppWindow::col3"),
    );

    assert!(
        (c1 - width * 0.25).abs() < 0.5,
        "column 1 is {c1} at window width {width}, expected {}",
        width * 0.25
    );
    assert!(
        (c2 - width * 0.50).abs() < 0.5,
        "column 2 is {c2} at window width {width}, expected {}",
        width * 0.50
    );
    assert!(
        (c3 - width * 0.25).abs() < 0.5,
        "column 3 is {c3} at window width {width}, expected {}",
        width * 0.25
    );
    assert!(
        (c1 + c2 + c3 - width).abs() < 0.5,
        "columns sum to {} but the window is {width} wide",
        c1 + c2 + c3
    );
}

fn item(label: &str, name: &str, broken: bool) -> ProductItem {
    ProductItem {
        label: label.into(),
        name: name.into(),
        detail: Default::default(),
        broken,
        warning: false,
    }
}

/// Every element carrying `id` in the realised tree.
fn elements(app: &AppWindow, id: &str) -> Vec<testing::ElementHandle> {
    testing::ElementHandle::find_by_element_id(app, id).collect()
}

fn click(element: &testing::ElementHandle) {
    element.mock_single_click(slint::platform::PointerEventButton::Left);
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real calendar date")
}

/// A vault holding one of each state column 1 and column 3 can be in.
///
/// Every string Chron6 has to re-translate comes from one of these: the warning
/// prefix and `Missing files:` from the incomplete one, `Broken entry` and a
/// `DataError` from the broken one, and the countdown from both healthy ones —
/// one expired, one not.
fn seeded_entries() -> Vec<Entry> {
    let product = |folder: &str, name: &str, added: u8, end: Date, missing: Vec<String>| {
        Entry::Ok(Product {
            folder: folder.to_string(),
            name: name.to_string(),
            serial: "ABC123XYZ".to_string(),
            link: "https://store.example/p".to_string(),
            purchase_date: day(2026, Month::March, 14),
            warranty_start: day(2026, Month::March, 14),
            warranty_end: end,
            pdfs: if missing.is_empty() {
                Vec::new()
            } else {
                missing.clone()
            },
            added: day(2026, Month::August, added),
            missing_pdfs: missing,
            extra: Default::default(),
        })
    };

    vec![
        // Warranty ending well past any date this test could run on.
        product("monitor", "QD-OLED Monitor", 1, day(2099, Month::March, 14), Vec::new()),
        product("drive", "IronWolf Pro", 2, day(2025, Month::January, 1), Vec::new()),
        product(
            "charger",
            "Şarj Cihazı",
            3,
            day(2099, Month::March, 14),
            vec!["gone.pdf".to_string()],
        ),
        Entry::Broken {
            folder: "test-broken".to_string(),
            reason: DataError::MissingToml,
        },
    ]
}

/// The owners `main` installs, kept alive for the rest of the test.
struct Stack {
    vault: Rc<RefCell<Vault>>,
    themes: Rc<RefCell<Themes>>,
    language: Rc<Cell<Lang>>,
    _details: crate::details::Details,
    _viewer: Rc<crate::viewer::Viewer>,
}

/// Wire up the real owners, in `main`'s order.
fn install_stack(app: &AppWindow) -> Stack {
    // Nothing here opens a file, so the root only has to be a path.
    let root = PathBuf::from("/nonexistent/parachron/products");
    // Falls back to UTC in a process that already has threads, which a test
    // harness does. The countdown assertions below are built not to care.
    let offset = crate::data::local_offset();

    let viewer = Rc::new(crate::viewer::install(app, root.clone(), Lang::En));
    let vault = crate::vault::install(
        app,
        root.clone(),
        seeded_entries(),
        SortMode::Added,
        Lang::En,
        offset,
        Rc::clone(&viewer),
    );
    let details = crate::details::install(app, Rc::clone(&vault));
    let editors = crate::editor::install(
        app,
        root,
        Lang::En,
        Rc::clone(&vault),
        Rc::clone(&viewer),
    );
    let themes = crate::theme::install(app, Theme::Dark, Lang::En);
    let language = crate::lang::install(
        app,
        Lang::En,
        Rc::clone(&vault),
        Rc::clone(&viewer),
        editors,
        Rc::clone(&themes),
    );

    Stack {
        vault,
        themes,
        language,
        _details: details,
        _viewer: viewer,
    }
}

#[test]
fn the_window_meets_the_criteria_that_need_a_real_element_tree() {
    testing::init_no_event_loop();

    let app = AppWindow::new().expect("window must open");
    crate::apply_strings(&app, Lang::En);

    // Criterion 2: two valid products and one broken folder, all three listed.
    let rows = vec![
        item("QD-OLED Monitor", "QD-OLED Monitor", false),
        item("IronWolf Pro 6TB", "IronWolf Pro 6TB", false),
        item("⚠ test-broken", "Broken entry: test-broken", true),
    ];
    app.set_products(ModelRc::new(VecModel::from(rows)));

    // ── Criterion 3: 25/50/25 at any size, and a hard 1000×700 floor ──────
    for (w, h) in [(1400.0, 900.0), (1000.0, 700.0), (1920.0, 1080.0)] {
        app.window().set_size(LogicalSize::new(w, h));
        assert_columns(&app, w);
    }

    // The other half of criterion 3 — the 1000×700 floor — is enforced by the
    // window manager from the constraints Slint hands to winit. The headless
    // backend has no window manager and sets whatever size it is asked for, so
    // that half is verified against a real window instead (see Chron1 notes).

    // ── Criterion 4: clicking a product updates the centre column ─────────
    app.window().set_size(LogicalSize::new(1400.0, 900.0));
    assert_eq!(app.get_selected_index(), -1, "nothing selected at startup");

    let touch: Vec<_> =
        testing::ElementHandle::find_by_element_id(&app, "AppWindow::row-touch").collect();
    assert_eq!(touch.len(), 3, "every product gets a row, broken ones too");

    touch[1].mock_single_click(slint::platform::PointerEventButton::Left);
    assert_eq!(app.get_selected_index(), 1);
    assert_eq!(app.get_selected_name(), "IronWolf Pro 6TB");
    assert!(!app.get_selected_broken());

    // The broken folder is selectable and reports itself as broken.
    touch[2].mock_single_click(slint::platform::PointerEventButton::Left);
    assert_eq!(app.get_selected_index(), 2);
    assert_eq!(app.get_selected_name(), "Broken entry: test-broken");
    assert!(app.get_selected_broken());

    // ── Criterion 5, from the other direction: the UI's text comes from the
    // string table, so switching language changes what is on screen ───────
    crate::apply_strings(&app, Lang::Tr);
    assert_eq!(
        app.global::<crate::Strings>().get_nav_about(),
        strings_get(Lang::Tr, Key::NavAbout)
    );
    crate::apply_strings(&app, Lang::En);

    // ── Install the real owners ───────────────────────────────────────────
    //
    // Everything above drove the `.slint` side with a hand-built model, which is
    // what Chron1's criteria are about. From here the Rust owners are wired up as
    // `main` wires them, because Chron5's and Chron6's criteria are about what
    // those owners push. The vault takes over the product list at this point and
    // replaces the three rows above with its own.
    //
    // Installed once, in `main`'s order. A second `install` on the same window
    // would silently replace a callback's only handler.
    let stack = install_stack(&app);
    let themes = Rc::clone(&stack.themes);

    // ── Chron5: the theme picker, criteria 1, 2 and 8 ─────────────────────
    let palette = app.global::<Palette>();

    // Installing paints the window, and the initializers in `palette.slint` are
    // this same theme — which is what makes a default start flash-free.
    assert_eq!(palette.get_bg(), colour(Theme::Dark.palette().bg));

    // Criterion 1: THEME opens a picker listing every theme in CORE §5, with the
    // one in effect marked. The picker is only reachable by clicking, because
    // whether it is open is the `.slint` side's business.
    assert!(elements(&app, "ThemeRow::touch").is_empty(), "the picker starts closed");
    let button = elements(&app, "Details::theme-button");
    assert_eq!(button.len(), 1, "column 3 has exactly one THEME button");
    click(&button[0]);

    // The model is the source of truth for "listed"; the realised rows are what
    // can be clicked. A `ListView` virtualizes, so the two can differ — and the
    // card is deliberately sized so that here they do not.
    assert_eq!(
        app.get_theme_rows().row_count(),
        Theme::ALL.len(),
        "every theme from CORE §5 is listed"
    );
    let rows = elements(&app, "ThemeRow::touch");
    assert_eq!(
        rows.len(),
        Theme::ALL.len(),
        "all eleven rows are realised, so none is reachable only by scrolling"
    );
    let marked: Vec<usize> = app
        .get_theme_rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.active)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(marked, [1], "exactly the active theme is marked, and it is Dark");

    // Criterion 2: choosing one repaints at once, and the rows follow.
    let latte = Theme::ALL.iter().position(|t| *t == Theme::Latte).unwrap();
    click(&rows[latte]);

    assert_eq!(themes.borrow().current(), Theme::Latte);

    // Every one of the twelve roles, not a sample of them. `theme::apply` maps
    // twelve struct fields onto twelve setters by hand, and a dropped or crossed
    // pair — `set_selection` omitted, leaving Default Dark's selection on all
    // eleven themes — would pass a spot check on `bg` and `text`, pass every
    // contrast test (which reads the table, not the window) and pass every grep.
    assert_palette_pushed(&app, Theme::Latte);
    let marked: Vec<usize> = app
        .get_theme_rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.active)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(marked, [latte], "the tick moved with the choice");

    // The sheet stays up after a choice — there is nothing to cancel, and eleven
    // themes are meant to be comparable without reopening the picker each time.
    assert_eq!(elements(&app, "ThemeRow::touch").len(), Theme::ALL.len());

    // Clicking the row already in effect changes nothing, and must not churn.
    click(&elements(&app, "ThemeRow::touch")[latte]);
    assert_eq!(themes.borrow().current(), Theme::Latte);

    // Criterion 8: at CORE §4's floor the picker still fits inside the window and
    // every theme is *visible* — not merely realised.
    //
    // That distinction is the whole point of this block. The first version of the
    // picker used a `ListView`, and a `ListView` realises a row just past the edge
    // of its viewport: counting realised rows said all eleven were there while the
    // eleventh sat below the fold behind a scrollbar. Only a screenshot showed it.
    // Asking where each row actually is closes the gap.
    app.window().set_size(LogicalSize::new(1000.0, 700.0));

    let card = elements(&app, "Sheet::card");
    assert_eq!(card.len(), 1, "the picker is built on the shared sheet");
    let (card_top, card_size) = (card[0].absolute_position(), card[0].size());
    assert!(
        card_size.width <= 1000.0 && card_size.height <= 700.0,
        "the card is {card_size:?} at the 1000x700 floor"
    );
    assert!(
        card_top.y >= 0.0 && card_top.y + card_size.height <= 700.0,
        "the card runs off the window: top {} height {}",
        card_top.y,
        card_size.height
    );

    let rows = elements(&app, "ThemeRow::touch");
    assert_eq!(rows.len(), Theme::ALL.len(), "every row is realised at the floor");
    for (index, row) in rows.iter().enumerate() {
        let (top, size) = (row.absolute_position(), row.size());
        assert!(size.height > 0.0, "row {index} has no height");
        assert!(
            top.y >= card_top.y && top.y + size.height <= card_top.y + card_size.height,
            "row {index} ({}) is outside the card: row {}..{} vs card {}..{}",
            strings_get(Lang::En, Theme::ALL[index].name()),
            top.y,
            top.y + size.height,
            card_top.y,
            card_top.y + card_size.height
        );
    }

    // And Close dismisses it.
    click(&elements(&app, "ThemeSheet::close")[0]);
    assert!(elements(&app, "ThemeRow::touch").is_empty(), "Close closes the picker");

    // ── Chron6: the language switch, criteria 1–5 ─────────────────────────
    app.window().set_size(LogicalSize::new(1400.0, 900.0));
    let strings = app.global::<crate::Strings>();

    // Select a product first, so column 3 and the viewer have something to say.
    // It has to happen with the menu closed: an open menu lays a full-window
    // TouchArea over everything to catch the dismissing click, so a row click
    // while it is up dismisses the menu instead of selecting anything.
    let rows = elements(&app, "AppWindow::row-touch");
    assert_eq!(rows.len(), 4, "the vault's own rows, not the hand-built ones");
    // Insertion order, so: monitor, drive, charger, then the broken folder.
    click(&rows[2]);
    assert_eq!(app.get_selected_name(), "Şarj Cihazı", "Turkish name intact");

    // Criterion 1: `Document ▾` lists both languages, the one in effect marked.
    assert!(elements(&app, "AppWindow::menu-lang-en").is_empty(), "the menu starts closed");
    click(&elements(&app, "AppWindow::menu-button")[0]);

    assert_eq!(elements(&app, "AppWindow::menu-lang-en").len(), 1);
    assert_eq!(elements(&app, "AppWindow::menu-lang-tr").len(), 1);
    assert_eq!(app.get_lang_mode(), 0, "English is in effect and marked");

    // What every composed string reads before the switch.
    let before_detail = app.get_selected_detail().to_string();
    let before_days = app.get_details_days_left().to_string();
    assert!(
        before_detail.contains(strings_get(Lang::En, Key::MissingFiles)),
        "the incomplete product explains itself in English: {before_detail:?}"
    );
    assert!(
        before_days.ends_with(strings_get(Lang::En, Key::DaysUnit)),
        "the countdown is in English: {before_days:?}"
    );

    // Criterion 2 and 3: choosing Türkçe relabels the bound strings *and*
    // everything Rust composed — without the product being reselected.
    click(&elements(&app, "AppWindow::menu-lang-tr")[0]);

    assert_eq!(stack.language.get(), Lang::Tr, "the session's language changed");
    assert_eq!(app.get_lang_mode(), 1, "the tick moved to Türkçe");
    assert_eq!(strings.get_nav_about(), strings_get(Lang::Tr, Key::NavAbout));
    assert_eq!(strings.get_action_export(), strings_get(Lang::Tr, Key::ActionExport));

    let after_detail = app.get_selected_detail().to_string();
    let after_days = app.get_details_days_left().to_string();
    assert!(
        after_detail.contains(strings_get(Lang::Tr, Key::MissingFiles)),
        "`Missing files` follows the switch: {after_detail:?}"
    );
    assert_ne!(before_detail, after_detail, "a composed row must not go stale");
    assert!(
        after_days.ends_with(strings_get(Lang::Tr, Key::DaysUnit)),
        "the countdown follows the switch: {after_days:?}"
    );
    // Turkish takes no plural after a numeral, so only the unit word changes —
    // the number in front of it must not.
    assert_eq!(
        before_days.trim_end_matches(strings_get(Lang::En, Key::DaysUnit)).trim(),
        after_days.trim_end_matches(strings_get(Lang::Tr, Key::DaysUnit)).trim(),
        "the switch changed the number of days, not just its unit"
    );

    // Criterion 4: the selection, the row and the open page are untouched.
    assert_eq!(app.get_selected_index(), 2, "still the same row");
    assert_eq!(app.get_selected_name(), "Şarj Cihazı");
    assert_eq!(app.get_page_index(), 0);

    // A broken folder's reason and an expired warranty both follow too.
    click(&elements(&app, "AppWindow::row-touch")[3]);
    assert!(app.get_selected_broken());
    assert_eq!(
        app.get_selected_detail(),
        strings_get(Lang::Tr, Key::ErrMissingToml),
        "a DataError is rendered through the table, not cached in English"
    );
    click(&elements(&app, "AppWindow::row-touch")[1]);
    assert_eq!(
        app.get_details_days_left(),
        strings_get(Lang::Tr, Key::WarrantyExpired),
        "an expired warranty reads as expired in Turkish"
    );

    // Chron5's picker rows are looked up in Rust, so they need re-pushing too —
    // and two of the eleven translate.
    let dark_row = Theme::ALL.iter().position(|t| *t == Theme::Dark).unwrap();
    assert_eq!(
        app.get_theme_rows().row_data(dark_row).unwrap().label,
        strings_get(Lang::Tr, Key::ThemeDefaultDark),
        "the picker's translatable rows follow the switch"
    );
    let mocha_row = Theme::ALL.iter().position(|t| *t == Theme::Mocha).unwrap();
    assert_eq!(
        app.get_theme_rows().row_data(mocha_row).unwrap().label,
        strings_get(Lang::Tr, Key::ThemeCatppuccinMocha),
        "and a proper noun stays itself"
    );

    // Criterion 5: switching to the language already in effect changes nothing.
    // The check that matters is that it does not re-plan — a re-plan bumps the
    // viewer's generation and asks for the page again, which on a large invoice
    // is a visible blink for no reason.
    click(&elements(&app, "AppWindow::menu-button")[0]);
    let settled = app.get_details_days_left().to_string();
    click(&elements(&app, "AppWindow::menu-lang-tr")[0]);
    assert_eq!(stack.language.get(), Lang::Tr);
    assert_eq!(app.get_details_days_left(), settled.as_str());

    // And back to English, so the vault's rows end as they started.
    click(&elements(&app, "AppWindow::menu-button")[0]);
    click(&elements(&app, "AppWindow::menu-lang-en")[0]);
    assert_eq!(stack.language.get(), Lang::En);
    assert_eq!(app.get_lang_mode(), 0);
    assert_eq!(
        app.get_details_days_left(),
        strings_get(Lang::En, Key::WarrantyExpired)
    );

    // Criterion 10: what is on disk is not UI copy and does not translate.
    assert_eq!(
        stack.vault.borrow().selected_folder().as_deref(),
        Some("drive"),
        "a folder name is an identity, not a label"
    );
}

/// The Slint colour `theme.rs` would have pushed for an `0xRRGGBB` value.
fn colour(value: u32) -> slint::Color {
    slint::Color::from_argb_u8(
        0xff,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}

/// `0xAARRGGBB`, for the one role that carries its own alpha.
fn colour_argb(value: u32) -> slint::Color {
    slint::Color::from_argb_u8(
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}

/// Assert all twelve roles reached the `Palette` global.
///
/// Reads the window back rather than the table, so it fails if `theme::apply`
/// forgets a setter or pairs two of them the wrong way round.
fn assert_palette_pushed(app: &AppWindow, theme: Theme) {
    let p = theme.palette();
    let table = app.global::<Palette>();
    let code = theme.code();

    for (got, want, role) in [
        (table.get_bg(), colour(p.bg), "bg"),
        (table.get_panel(), colour(p.panel), "panel"),
        (table.get_raised(), colour(p.raised), "raised"),
        (table.get_border(), colour(p.border), "border"),
        (table.get_text(), colour(p.text), "text"),
        (table.get_muted(), colour(p.muted), "muted"),
        (table.get_accent(), colour(p.accent), "accent"),
        (table.get_danger(), colour(p.danger), "danger"),
        (table.get_selection(), colour(p.selection), "selection"),
        (table.get_paper(), colour(p.paper), "paper"),
        (table.get_paper_edge(), colour(p.paper_edge), "paper-edge"),
        (table.get_backdrop(), colour_argb(p.backdrop), "backdrop"),
    ] {
        assert_eq!(got, want, "{code}: {role} did not reach the Palette global");
    }
}

fn strings_get(lang: Lang, key: Key) -> &'static str {
    crate::strings::get(lang, key)
}
