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
use crate::{AppWindow, FormDoc, Palette, ProductItem};

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
        product(
            "monitor",
            "QD-OLED Monitor",
            1,
            day(2099, Month::March, 14),
            Vec::new(),
        ),
        product(
            "drive",
            "IronWolf Pro",
            2,
            day(2025, Month::January, 1),
            Vec::new(),
        ),
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
        root.clone(),
        Lang::En,
        Rc::clone(&vault),
        Rc::clone(&viewer),
    );
    let themes = crate::theme::install(app, Theme::Dark, Lang::En);
    let exports = crate::export::install(app, root.clone(), Lang::En, offset, Rc::clone(&vault));

    // Chron9. The vault-location entry needs a `Paths`, and `Paths::resolve`
    // would reach the real home directory — so this is built against the same
    // nonexistent root the rest of the stack uses. Nothing in the headless test
    // opens the picker; what is asserted is that the menu entry exists and is
    // labelled from the string table.
    let relocations = crate::relocate::install(
        app,
        crate::data::Paths::for_test(PathBuf::from("/nonexistent/parachron")),
        Lang::En,
        Rc::clone(&vault),
        Rc::clone(&viewer),
        editors.clone(),
        exports.clone(),
    );

    let language = crate::lang::install(
        app,
        Lang::En,
        Rc::clone(&vault),
        Rc::clone(&viewer),
        editors,
        Rc::clone(&themes),
        exports,
        Some(relocations),
    );

    // `root` is what every owner above was built with; naming it here keeps the
    // unused-variable warning honest rather than silencing it with an underscore.
    let _ = root;

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
    assert!(
        elements(&app, "ThemeRow::touch").is_empty(),
        "the picker starts closed"
    );
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
    assert_eq!(
        marked,
        [1],
        "exactly the active theme is marked, and it is Dark"
    );

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
    assert_eq!(
        rows.len(),
        Theme::ALL.len(),
        "every row is realised at the floor"
    );
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
    assert!(
        elements(&app, "ThemeRow::touch").is_empty(),
        "Close closes the picker"
    );

    // ── Chron6: the language switch, criteria 1–5 ─────────────────────────
    app.window().set_size(LogicalSize::new(1400.0, 900.0));
    let strings = app.global::<crate::Strings>();

    // Select a product first, so column 3 and the viewer have something to say.
    // It has to happen with the menu closed: an open menu lays a full-window
    // TouchArea over everything to catch the dismissing click, so a row click
    // while it is up dismisses the menu instead of selecting anything.
    let rows = elements(&app, "AppWindow::row-touch");
    assert_eq!(
        rows.len(),
        4,
        "the vault's own rows, not the hand-built ones"
    );
    // Insertion order, so: monitor, drive, charger, then the broken folder.
    click(&rows[2]);
    assert_eq!(
        app.get_selected_name(),
        "Şarj Cihazı",
        "Turkish name intact"
    );

    // Criterion 1: `Document ▾` lists both languages, the one in effect marked.
    assert!(
        elements(&app, "AppWindow::menu-lang-en").is_empty(),
        "the menu starts closed"
    );
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

    assert_eq!(
        stack.language.get(),
        Lang::Tr,
        "the session's language changed"
    );
    assert_eq!(app.get_lang_mode(), 1, "the tick moved to Türkçe");
    assert_eq!(
        strings.get_nav_about(),
        strings_get(Lang::Tr, Key::NavAbout)
    );
    assert_eq!(
        strings.get_action_export(),
        strings_get(Lang::Tr, Key::ActionExport)
    );

    let after_detail = app.get_selected_detail().to_string();
    let after_days = app.get_details_days_left().to_string();
    assert!(
        after_detail.contains(strings_get(Lang::Tr, Key::MissingFiles)),
        "`Missing files` follows the switch: {after_detail:?}"
    );
    assert_ne!(
        before_detail, after_detail,
        "a composed row must not go stale"
    );
    assert!(
        after_days.ends_with(strings_get(Lang::Tr, Key::DaysUnit)),
        "the countdown follows the switch: {after_days:?}"
    );
    // Turkish takes no plural after a numeral, so only the unit word changes —
    // the number in front of it must not.
    assert_eq!(
        before_days
            .trim_end_matches(strings_get(Lang::En, Key::DaysUnit))
            .trim(),
        after_days
            .trim_end_matches(strings_get(Lang::Tr, Key::DaysUnit))
            .trim(),
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

    // ── Chron7: EXPORT is gated on there being something to export ────────
    //
    // Criterion 13. The button reads `filled`, which already means "a product is
    // selected and its manifest parsed", so this is checking that binding rather
    // than a flag of its own.
    let export_button = || {
        elements(&app, "Details::export-button")
            .pop()
            .expect("column 3 has an EXPORT button")
    };

    // A healthy product: live.
    click(&elements(&app, "AppWindow::row-touch")[0]);
    assert!(app.get_details_filled());
    assert!(export_button().accessible_enabled().unwrap_or(false));

    // A folder whose manifest will not parse: inert.
    click(&elements(&app, "AppWindow::row-touch")[3]);
    assert!(app.get_selected_broken());
    assert!(!app.get_details_filled());
    assert!(
        !export_button().accessible_enabled().unwrap_or(true),
        "a folder that will not parse has no product to export"
    );

    // Clicking it anyway does nothing at all — no status, no panic.
    click(&export_button());
    assert_eq!(app.get_export_status(), "");
    assert!(!app.get_export_failed());

    // The status line does not survive a change of product.
    //
    // It is a claim about one product: `Saved — Not included: gone.pdf` left over
    // from the last export, sitting above the next product's details or above a
    // broken folder's "Details appear here", says something untrue. Nothing in the
    // vault's own push used to touch it, and the line is a sibling of both the
    // filled and unfilled branches of column 3, so it survived the column emptying.
    app.set_export_status("Saved".into());
    app.set_export_failed(true);
    click(&elements(&app, "AppWindow::row-touch")[0]);
    assert_eq!(
        app.get_export_status(),
        "",
        "a status from the previous product is still on screen"
    );
    assert!(!app.get_export_failed());

    app.set_export_status("Saved".into());
    click(&elements(&app, "AppWindow::row-touch")[3]);
    assert_eq!(
        app.get_export_status(),
        "",
        "a status survived onto a folder that has no product at all"
    );

    // But a *running* export's line does survive, and this is the interaction the
    // two fixes above collided over.
    //
    // `details::show` clears the status, and it runs from every `vault::push` —
    // including the one `lang::switch` ends with. So an unconditional clear erased
    // the "Exporting…" that `Exports::set_lang` had re-said one line earlier, and
    // switching language mid-export still blanked the line and left a live button
    // that silently did nothing. A sort toggle or a form save did the same. Neither
    // of the two tests above could see it: one sets the status by hand and clicks a
    // row, the other never reaches `set_lang` with an export in flight.
    app.set_export_running(true);
    app.set_export_status("Exporting…".into());

    click(&elements(&app, "AppWindow::row-touch")[0]);
    assert_eq!(
        app.get_export_status(),
        "Exporting…",
        "a selection change blanked a running export"
    );
    click(&elements(&app, "AppWindow::sort-name")[0]);
    assert_eq!(
        app.get_export_status(),
        "Exporting…",
        "a sort toggle blanked a running export"
    );
    click(&elements(&app, "AppWindow::menu-button")[0]);
    click(&elements(&app, "AppWindow::menu-lang-tr")[0]);
    assert_eq!(
        app.get_export_status(),
        strings_get(Lang::Tr, Key::Exporting),
        "a language switch must re-say what is happening, not blank it"
    );

    // And once it is no longer running, the line is clearable again.
    app.set_export_running(false);
    click(&elements(&app, "AppWindow::row-touch")[1]);
    assert_eq!(app.get_export_status(), "");

    // ── Chron8: the search bar, criteria 12–15 and 17 ─────────────────────
    //
    // The sections above leave the window in Turkish and the list ordered by
    // name — Chron7's running-export block toggles the sort chip to prove a
    // status line survives it, and has no reason to toggle it back. Both are put
    // back here, so the rows numbered below are the ones CORE §4's insertion
    // order gives and the strings named below are the ones on screen.
    click(&elements(&app, "AppWindow::menu-button")[0]);
    click(&elements(&app, "AppWindow::menu-lang-en")[0]);
    click(&elements(&app, "AppWindow::sort-name")[0]);
    assert_eq!(stack.vault.borrow().sort(), SortMode::Added);
    assert_eq!(
        app.get_search_query(),
        "",
        "the query is session state and the session has not typed one yet"
    );

    // A broken folder's heading is composed in Rust from the string table, so it
    // is composed the same way here rather than written out a second time — the
    // hand-built row at the top of this test is fixture data, not vault output.
    let broken_name = format!("{}: test-broken", strings_get(Lang::En, Key::BrokenTitle));
    // `name` rather than `label`: the incomplete product's label carries the
    // warning prefix, and what a person types into the bar is a product's name.
    let names = || -> Vec<String> {
        app.get_products()
            .iter()
            .map(|row| row.name.to_string())
            .collect()
    };
    let all = vec![
        "QD-OLED Monitor".to_string(),
        "IronWolf Pro".to_string(),
        "Şarj Cihazı".to_string(),
        broken_name.clone(),
    ];
    assert_eq!(names(), all, "the whole vault, in insertion order");

    // What a keystroke does: the two-way binding writes the property, and the
    // bar's `edited` callback tells Rust. Nothing here can type into a
    // `TextInput`, so both halves are driven by hand; the clear affordance below
    // is the one part of the bar this test reaches through a real click.
    let search = |query: &str| {
        app.set_search_query(query.into());
        app.invoke_search_changed(query.into());
    };
    // A `Text` labels itself for accessibility, so the sentence actually on
    // screen can be read back rather than inferred from the properties behind it.
    let on_screen =
        |text: &str| testing::ElementHandle::find_by_accessible_label(&app, text).count();

    // Criterion 12, and criterion 13 with it: the query narrows the list to what
    // matched, and the fold runs on both sides — `sarj`, typed on a keyboard
    // with no Turkish on it, finds `Şarj Cihazı`. Matching the stored strings
    // instead would leave that product findable only by somebody who can already
    // type `Ş`, which is the one person who does not need the bar.
    search("sarj");
    assert_eq!(
        names(),
        ["Şarj Cihazı"],
        "one entry matched, and it is that one"
    );
    assert_eq!(
        elements(&app, "AppWindow::row-touch").len(),
        1,
        "the rows on screen followed the model rather than the model alone"
    );

    // Criterion 15: a query matching nothing empties the list, and the sentence
    // left behind must not be the empty-vault one. The two share a single `Text`
    // whose content switches on `search-query == ""` — one binding apart — and
    // getting it the wrong way round tells somebody who mistyped four characters
    // that their vault is empty, which for an app whose promise is keeping their
    // documents is the most alarming thing it could say.
    search("zzzz");
    assert!(names().is_empty(), "nothing in the vault matches that");
    assert_ne!(
        app.get_search_query(),
        "",
        "an empty list under an empty query is the empty *vault*, a different state"
    );
    assert_eq!(
        on_screen(strings_get(Lang::En, Key::SearchNoMatches)),
        1,
        "no matches, and the list says so"
    );
    assert_eq!(
        on_screen(strings_get(Lang::En, Key::ListEmpty)),
        0,
        "a typo was reported as an empty vault"
    );

    // Criterion 12's other half: clearing restores every entry, in the order the
    // sort mode says — not in the order they matched and not in whatever order
    // the filter happened to walk the vault in.
    search("");
    assert_eq!(names(), all, "clearing the query left entries hidden");
    assert_eq!(elements(&app, "AppWindow::row-touch").len(), 4);
    assert_eq!(
        on_screen(strings_get(Lang::En, Key::SearchNoMatches)),
        0,
        "the no-matches line outlived the query that produced it"
    );

    // Criterion 17: the clear affordance, which is a real click rather than two
    // properties set by hand. It writes the `TextInput` and lets the binding
    // carry the value back out to `search-query` — two chained two-way bindings,
    // which is the part worth a test rather than an argument.
    //
    // It is also the only assertion in this section that proves a query set from
    // Rust reaches the widget at all: `clear` is realised under `text != ""`
    // *inside* `SearchBar`, so its existence is the inbound half of the same
    // chain the click exercises on the way back out.
    search("sarj");
    let clear = elements(&app, "SearchBar::clear");
    assert_eq!(
        clear.len(),
        1,
        "there is something to clear, so there is a way to"
    );
    click(&clear[0]);
    assert_eq!(
        app.get_search_query(),
        "",
        "the affordance did not empty the bar"
    );
    assert_eq!(names(), all, "and the list did not come back with it");
    assert!(
        elements(&app, "SearchBar::clear").is_empty(),
        "an empty bar still spends width on a clear affordance"
    );

    // ── Chron8: a filter may hide the open product's row, criterion 16 ────
    //
    // The regression Chron3 warned about, and the reason Chron8 split the gate in
    // two. Until the search bar, `selected-index == -1` and "nothing is selected"
    // were the same state; a query that excludes the open product makes them
    // different, and the viewer is gated on the second one. Chron3 measured what
    // gating on the index costs here: a momentary -1 tears the viewer down and
    // pays the resize debounce to build it again, so typing four characters would
    // blink the invoice somebody is reading four times over.
    //
    // `selected-open` is pushed from Rust, so reading it back proves the vault's
    // half and nothing about the `.slint` half — a gate regressed to
    // `selected-index >= 0` would leave the flag true and still tear the viewer
    // down. Only the element tree tells those two apart, which is why
    // `AppWindow::viewer` is the assertion that matters in this section.
    click(&elements(&app, "AppWindow::row-touch")[1]);
    assert_eq!(app.get_selected_name(), "IronWolf Pro");
    assert_eq!(app.get_selected_index(), 1);
    assert!(app.get_selected_open());
    assert_eq!(
        elements(&app, "AppWindow::viewer").len(),
        1,
        "an open product is hosted by the viewer"
    );
    let (page, zoom) = (app.get_page_index(), app.get_zoom());

    search("sarj");
    assert_eq!(
        app.get_selected_index(),
        -1,
        "the open product's row is filtered out, so no row is highlighted"
    );
    assert!(
        app.get_selected_open(),
        "the query narrowed the list, not the app — the product is still open"
    );
    assert_eq!(
        app.get_selected_name(),
        "IronWolf Pro",
        "a keystroke changed product"
    );
    assert_eq!(
        stack.vault.borrow().selected_folder().as_deref(),
        Some("drive"),
        "and the vault agrees about which one is open"
    );
    assert_eq!(
        elements(&app, "AppWindow::viewer").len(),
        1,
        "a keystroke tore the viewer down and will pay the debounce to rebuild it"
    );
    // Cheap, and close to vacuous against a vault with no files behind it —
    // written down because they are what criterion 16 is about, not because they
    // are hard to satisfy here.
    assert_eq!(app.get_page_index(), page, "the open document changed page");
    assert_eq!(app.get_zoom(), zoom, "the open document changed zoom");

    search("");
    assert_eq!(
        app.get_selected_index(),
        1,
        "the row came back without its highlight"
    );
    assert!(app.get_selected_open());
    assert_eq!(elements(&app, "AppWindow::viewer").len(), 1);

    // ── Chron8: clicking a row in a filtered list, criteria 12 and 14 ─────
    //
    // A row index and an entry index were the same number from Chron1 until the
    // filter arrived. Getting the remap wrong does not crash — it selects a
    // different product than the one clicked, silently — so this clicks rows in a
    // list where the two numbers cannot agree, and asserts on the folder that
    // comes back, which is a product's identity (CORE §3) rather than its label.
    //
    // `ro` matches the drive (entry 1) on its name and the unreadable folder
    // (entry 3) on its folder name. That second one is criterion 14: the list has
    // never hidden a folder that will not parse and does not start here. Neither
    // row's index is its entry's, so an implementation that indexed `entries`
    // directly would answer row 1 with the drive and row 0 with the monitor, and
    // every assertion below would name the wrong product.
    search("ro");
    assert_eq!(names(), ["IronWolf Pro".to_string(), broken_name.clone()]);

    click(&elements(&app, "AppWindow::row-touch")[1]);
    assert_eq!(
        stack.vault.borrow().selected_folder().as_deref(),
        Some("test-broken"),
        "row 1 of this list is entry 3, and a click selected whatever entry 1 is"
    );
    assert!(app.get_selected_broken());
    assert_eq!(app.get_selected_name(), broken_name.as_str());
    assert_eq!(
        app.get_selected_index(),
        1,
        "the highlight left the row that was clicked"
    );
    // And the control for the section above: the viewer is gone here, which is
    // what makes its presence while the filter hid the drive's row an answer
    // rather than a constant. A folder that will not parse keeps Chron1's
    // reason display instead.
    assert!(
        elements(&app, "AppWindow::viewer").is_empty(),
        "a folder that will not parse has no document to host"
    );

    click(&elements(&app, "AppWindow::row-touch")[0]);
    assert_eq!(
        stack.vault.borrow().selected_folder().as_deref(),
        Some("drive"),
        "row 0 of this list is entry 1, and a click selected whatever entry 0 is"
    );
    assert!(!app.get_selected_broken());
    assert_eq!(app.get_selected_name(), "IronWolf Pro");
    assert_eq!(app.get_selected_index(), 0);

    // One more, because a mis-remap that lands on another *product* rather than
    // on a broken folder shows itself in the composed row text rather than in a
    // flag: `cihaz` leaves the charger alone on row 0, two rows above where it
    // sits in the vault, and it is the only entry carrying a missing-file line.
    search("cihaz");
    assert_eq!(names(), ["Şarj Cihazı"]);
    click(&elements(&app, "AppWindow::row-touch")[0]);
    assert_eq!(
        stack.vault.borrow().selected_folder().as_deref(),
        Some("charger")
    );
    assert_eq!(app.get_selected_name(), "Şarj Cihazı");
    assert!(
        app.get_selected_detail().contains("gone.pdf"),
        "row 0 is entry 2 and brings entry 2's detail with it: {:?}",
        app.get_selected_detail()
    );
    search("");

    // ── Chron8: the About strip, criteria 1 and 8 ─────────────────────────
    //
    // `about-open` is a private property — whether a pane is on screen is the
    // `.slint` side's business, the same call Chron5 made for the theme picker —
    // so there is no getter to read it back with. The strip declares itself
    // checkable and publishes the flag as its accessible state, which is what
    // that state is for. The pane's own element is the other half of every
    // assertion here, because "the accessible mirror is wired" and "the pane
    // rendered" are two different claims and only the second one is the feature.
    let strip = || {
        elements(&app, "AppWindow::about-strip")
            .pop()
            .expect("column 1 has an About strip")
    };
    assert!(
        elements(&app, "About::content").is_empty(),
        "About starts closed"
    );
    assert_eq!(strip().accessible_checked(), Some(false));

    // Criterion 1: the strip opens the pane and reads as the active view.
    click(&strip());
    assert_eq!(strip().accessible_checked(), Some(true));
    assert_eq!(
        elements(&app, "About::content").len(),
        1,
        "the pane is on screen"
    );

    // Criterion 1's other half. About *covers* columns 2 and 3 rather than
    // replacing them, so all three ids are still where `assert_columns` looks for
    // them and still 25/50/25 — including at CORE §4's floor, which is where a
    // pane that had displaced a column would show it first.
    app.window().set_size(LogicalSize::new(1000.0, 700.0));
    assert_columns(&app, 1000.0);
    app.window().set_size(LogicalSize::new(1400.0, 900.0));
    assert_columns(&app, 1400.0);
    assert_eq!(
        elements(&app, "AppWindow::row-touch").len(),
        4,
        "column 1 stays live under the pane, and it is not staying live"
    );

    // Criterion 8: the strip is the way in and one of the ways out, so there is
    // never a pane up with no visible way back to what it covered.
    click(&strip());
    assert_eq!(strip().accessible_checked(), Some(false));
    assert!(
        elements(&app, "About::content").is_empty(),
        "clicking the strip a second time left the pane up"
    );

    // Criterion 8 again: choosing a product is choosing to look at it. Without
    // this the click lands, the selection changes, and nothing visible happens
    // because the columns that would show it are covered — a list that looks
    // broken. The charger is selected going in, so the row clicked here is a
    // different product and the pane cannot be said to have closed onto the one
    // that was already open.
    click(&strip());
    assert_eq!(elements(&app, "About::content").len(), 1);
    click(&elements(&app, "AppWindow::row-touch")[0]);
    assert_eq!(app.get_selected_name(), "QD-OLED Monitor");
    assert!(
        elements(&app, "About::content").is_empty(),
        "a product was selected behind a pane that stayed up"
    );
    assert_eq!(strip().accessible_checked(), Some(false));
    assert_eq!(
        elements(&app, "AppWindow::viewer").len(),
        1,
        "and the product it chose is what is now on screen"
    );

    // ── Chron9: the vault-location entry, criteria 3 and 12 ───────────────
    //
    // The picker itself cannot be driven here — it is a portal dialog drawn by
    // the desktop's own service, the boundary Chron3 documented and Chron7
    // restated. What can be asserted is everything up to the click: that the
    // entry is in `Document ▾`, that it is labelled from the string table in
    // both languages, and that it is enabled — a menu row that exists but is
    // dead is the shape this would fail in, since `relocate::install` is what
    // turns it on and is skipped entirely when there is no vault to move from.
    click(&elements(&app, "AppWindow::menu-button")[0]);
    let vault_entry = elements(&app, "AppWindow::menu-vault");
    assert_eq!(
        vault_entry.len(),
        1,
        "Document ▾ has no vault-location entry"
    );
    assert_eq!(
        vault_entry[0].accessible_label().as_deref(),
        Some(strings_get(Lang::En, Key::ActionVaultLocation)),
        "the entry is labelled from the string table, not from a literal"
    );
    assert_eq!(
        vault_entry[0].accessible_enabled(),
        Some(true),
        "the entry is dead, which is what a skipped relocate::install looks like"
    );

    // And in Turkish, because a key added to one table and not the other is the
    // one mistake `strings.rs`'s exhaustiveness test cannot catch on its own —
    // it proves both sides exist, not that the window reads the right one.
    click(&elements(&app, "AppWindow::menu-lang-tr")[0]);
    click(&elements(&app, "AppWindow::menu-button")[0]);
    assert_eq!(
        elements(&app, "AppWindow::menu-vault")[0]
            .accessible_label()
            .as_deref(),
        Some(strings_get(Lang::Tr, Key::ActionVaultLocation)),
    );
    click(&elements(&app, "AppWindow::menu-lang-en")[0]);

    // ── The add/edit sheet (FormSheet) ─────────────────────────────────────
    //
    // `add-document()` itself opens nothing that needs a person — the picker
    // only shows up behind `add-pdf`, which this does not touch — so this
    // exercises the real callback rather than driving `form-open` by hand.
    app.invoke_add_document();
    assert!(app.get_form_open(), "add-document() did not open the sheet");
    assert_eq!(
        app.get_form_heading(),
        strings_get(Lang::En, Key::FormAddTitle)
    );
    assert_eq!(app.get_form_name(), "", "a fresh add starts blank");

    let save = || elements(&app, "FormSheet::save");
    let add_pdf = || elements(&app, "FormSheet::add-pdf");
    assert_eq!(save().len(), 1);
    assert_eq!(
        save()[0].accessible_enabled(),
        Some(true),
        "nothing is running yet"
    );
    assert_eq!(add_pdf()[0].accessible_enabled(), Some(true));

    // `form-busy` and `form-docs` are driven by hand rather than through a
    // real `form-save()` — that call dispatches a commit onto a thread of its
    // own, which needs the event loop this test does not run to ever report
    // back. What is asserted here is the sheet's own rendering: whether it
    // disables the right things while `busy` is true, which is plain
    // property-to-element wiring and does not need the thread that flips the
    // property for real.
    app.set_form_docs(ModelRc::new(VecModel::from(vec![FormDoc {
        label: "invoice.pdf".into(),
        detail: "".into(),
        failed: false,
    }])));
    assert_eq!(
        elements(&app, "FormSheet::remove-doc")[0].accessible_enabled(),
        Some(true)
    );

    app.set_form_busy(true);
    assert_eq!(
        save()[0].accessible_enabled(),
        Some(false),
        "Save must not be pressable a second time while a commit is in flight"
    );
    assert_eq!(add_pdf()[0].accessible_enabled(), Some(false));
    assert_eq!(
        elements(&app, "FormSheet::remove-doc")[0].accessible_enabled(),
        Some(false),
        "a document removed while a save is in flight is not seen by the \
         commit already under way — see editor::on_form_remove_pdf"
    );

    // Left as it was found: `busy` reset before `cancel()`, because the real
    // guard behind that callback also checks it, and a stray `true` here
    // would make the very next call a silent no-op instead of closing the
    // sheet.
    app.set_form_busy(false);
    app.set_form_docs(ModelRc::new(VecModel::from(Vec::<FormDoc>::new())));
    app.invoke_form_cancel();
    assert!(!app.get_form_open(), "cancel did not close the sheet");

    // ── The vault relocation sheet (RelocateSheet) ──────────────────────────
    //
    // `relocate::install`'s own `running` guard lives behind a picker-filled
    // `Option<PathBuf>` that only a real folder dialog can set, so — same
    // reasoning as the form above — this drives the sheet's properties by
    // hand and checks what it renders, which is the half of Chron9's own
    // finding that does not need the portal.
    let cancel_or_close = || elements(&app, "RelocateSheet::cancel-or-close");
    let move_btn = || elements(&app, "RelocateSheet::move-btn");

    app.set_relocate_open(true);
    app.set_relocate_running(true);
    assert!(
        cancel_or_close().is_empty(),
        "nothing must be pressable while files are in flight"
    );
    assert!(move_btn().is_empty());

    app.set_relocate_running(false);
    assert_eq!(cancel_or_close().len(), 1);
    assert_eq!(
        cancel_or_close()[0].accessible_label().as_deref(),
        Some(strings_get(Lang::En, Key::ActionCancel)),
        "before a move finishes, the button reads Cancel"
    );
    assert_eq!(
        move_btn().len(),
        1,
        "not running, not failed, not done: Move is offered"
    );

    app.set_relocate_done(true);
    assert_eq!(
        cancel_or_close()[0].accessible_label().as_deref(),
        Some(strings_get(Lang::En, Key::ActionClose)),
        "a button reading Cancel over a finished move invites undoing one"
    );
    assert!(
        move_btn().is_empty(),
        "a finished move does not offer to start another over the same click"
    );

    app.set_relocate_done(false);
    app.set_relocate_failed(true);
    assert_eq!(
        cancel_or_close()[0].accessible_label().as_deref(),
        Some(strings_get(Lang::En, Key::ActionCancel)),
        "a refusal is not a finished move"
    );
    assert!(
        move_btn().is_empty(),
        "the failed destination is not retried by the same button"
    );

    // Left closed, the same way the sheets above were.
    app.set_relocate_failed(false);
    app.set_relocate_open(false);
}

/// The Slint colour `theme.rs` would have pushed for an `0xRRGGBB` value.
fn colour(value: u32) -> slint::Color {
    slint::Color::from_argb_u8(0xff, (value >> 16) as u8, (value >> 8) as u8, value as u8)
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

/// Assert all fourteen roles reached the `Palette` global.
///
/// Reads the window back rather than the table, so it fails if `theme::apply`
/// forgets a setter or pairs two of them the wrong way round.
///
/// `accent2` and `accent3` are here for a reason worth stating: `palette.slint`
/// initializes them to Default Dark's own two hues, so a dropped setter would
/// not blank anything or crash — every one of the other ten themes would just
/// quietly wear Default Dark's amber and violet on its column rules, in an app
/// whose whole point is that the theme you picked is the theme you get. That is
/// exactly the class of failure a screenshot of one theme cannot catch.
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
        (table.get_accent2(), colour(p.accent2), "accent2"),
        (table.get_accent3(), colour(p.accent3), "accent3"),
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
