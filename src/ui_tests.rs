//! The acceptance criteria that need a real element tree, driven headlessly.
//!
//! Everything lives in one test function on purpose: the Slint testing backend
//! is installed process-wide and its components are not `Send`, so a single
//! test keeps every window on one thread. Later milestones add sections to it
//! rather than test functions beside it.

use i_slint_backend_testing as testing;
use slint::{ComponentHandle, LogicalSize, Model, ModelRc, VecModel};

use crate::strings::{Key, Lang};
use crate::theme::Theme;
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

    // ── Chron5: the theme picker, criteria 1, 2 and 8 ─────────────────────
    let themes = crate::theme::install(&app, Theme::Dark, Lang::En);
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
    assert_eq!(
        palette.get_bg(),
        colour(Theme::Latte.palette().bg),
        "the palette was pushed, not merely recorded"
    );
    assert_eq!(palette.get_text(), colour(Theme::Latte.palette().text));
    assert_eq!(
        palette.get_backdrop().alpha(),
        (Theme::Latte.palette().backdrop >> 24) as u8,
        "a light theme gets a light theme's scrim"
    );
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

fn strings_get(lang: Lang, key: Key) -> &'static str {
    crate::strings::get(lang, key)
}
