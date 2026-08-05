//! Chron1 acceptance criteria 2, 3 and 4, driven headlessly.
//!
//! Everything lives in one test function on purpose: the Slint testing backend
//! is installed process-wide and its components are not `Send`, so a single
//! test keeps every window on one thread.

use i_slint_backend_testing as testing;
use slint::{ComponentHandle, LogicalSize, ModelRc, VecModel};

use crate::strings::{Key, Lang};
use crate::{AppWindow, ProductItem};

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

#[test]
fn window_skeleton_meets_chron1_criteria() {
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
}

fn strings_get(lang: Lang, key: Key) -> &'static str {
    crate::strings::get(lang, key)
}
