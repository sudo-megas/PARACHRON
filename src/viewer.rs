//! Column 2's state machine: which document, which page, at what size.
//!
//! Holds no MuPDF types — it decides *what* to draw and hands the request to
//! [`crate::render`], which owns everything MuPDF on its own thread.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slint::{
    ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode, VecModel,
};

use crate::data::Product;
use crate::render::{Renderer, Response, ViewError};
use crate::strings::{self, Key, Lang};
use crate::{AppWindow, DocTab};

/// Zoom is a multiplier of the *fit* scale, so 1× always means "the whole page
/// is visible" whatever the window size.
pub const ZOOM_MIN: f32 = 1.0;
pub const ZOOM_MAX: f32 = 4.0;

/// Chrome around the preview, matching `viewer.slint`. Used only as a fallback
/// when the layout has not reported its geometry yet.
const TITLE_BAR: f32 = 48.0;
const TAB_ROW: f32 = 40.0;
const CONTROL_ROW: f32 = 40.0;
const SERIAL_STRIP: f32 = 44.0;

/// How long the window must sit still before a resize is worth re-rendering.
const RESIZE_SETTLE: Duration = Duration::from_millis(120);
/// How long the "copied" confirmation stays up.
const COPIED_LINGER: Duration = Duration::from_millis(1500);

/// One document tab.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tab {
    /// File name as written in `product.toml`.
    file: String,
    /// What the tab reads — the file-name stem (CORE §4's `[Invoice] [Garanti]`).
    label: String,
    /// Named in the manifest but not on disk.
    missing: bool,
}

/// The documents of one product, as the viewer needs them.
///
/// The viewer used to hold the whole entry list plus an index into it, purely
/// to reach these four fields. Passing them in instead removes the second copy
/// of the list — and with it the question of how to re-index a selection when
/// the vault re-sorts, because there is no index into a list the viewer does
/// not own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocSet {
    /// Folder name, which is how document paths are built (CORE §3).
    pub folder: String,
    pub serial: String,
    pub pdfs: Vec<String>,
    pub missing: Vec<String>,
}

impl DocSet {
    pub fn of(product: &Product) -> Self {
        Self {
            folder: product.folder.clone(),
            serial: product.serial.clone(),
            pdfs: product.pdfs.clone(),
            missing: product.missing_pdfs.clone(),
        }
    }
}

#[derive(Debug)]
struct State {
    products_root: PathBuf,
    /// Folder of the product on show. `None` means nothing is selected, or the
    /// selection is a folder that would not parse.
    folder: Option<String>,
    tabs: Vec<Tab>,
    active_tab: usize,
    page: usize,
    pages: usize,
    zoom: f32,
    /// Preview area in logical pixels, as reported by the layout.
    viewport: (f32, f32),
    serial: String,
    /// Bumped on every state change; responses carrying an older one are stale.
    token: u64,
    /// State rather than a value captured into each closure, so Chron6 can
    /// change it. Re-registering handlers is not an alternative — Slint allows
    /// one handler per callback, and setting one from inside that callback's
    /// own handler panics.
    lang: Lang,
}

/// What the UI should be told after a state change.
#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    tabs: Vec<DocTab>,
    active_tab: i32,
    serial: String,
    zoom: f32,
    page_index: i32,
    page_count: i32,
    page_label: String,
}

/// What to do about pixels after a state change.
#[derive(Debug, Clone, PartialEq)]
enum Plan {
    Render {
        token: u64,
        path: PathBuf,
        page: usize,
        target: (u32, u32),
    },
    Show(ViewError),
    /// Nothing selected, no documents, or the layout has not settled.
    Idle,
}

impl State {
    fn new(products_root: PathBuf, lang: Lang) -> Self {
        Self {
            products_root,
            folder: None,
            tabs: Vec::new(),
            active_tab: 0,
            page: 0,
            pages: 0,
            zoom: ZOOM_MIN,
            viewport: (0.0, 0.0),
            serial: String::new(),
            token: 0,
            lang,
        }
    }

    /// Point the viewer at a product's documents.
    ///
    /// `keep_view` asks to stay on the same *file* if it is still there, which
    /// is what a re-sort and a save both want — neither of those is a change of
    /// tab or of product, so CORE §4's reset rule does not apply to them. A
    /// fresh click passes `false` and starts at page one, fitted.
    fn show(&mut self, doc: Option<DocSet>, keep_view: bool) {
        let showing = if keep_view {
            self.tabs.get(self.active_tab).map(|tab| tab.file.clone())
        } else {
            None
        };

        let Some(doc) = doc else {
            // Nothing selected, or a broken folder: column 2 keeps Chron1's
            // reason display instead of the viewer.
            self.folder = None;
            self.serial.clear();
            self.tabs.clear();
            self.reset_view();
            return;
        };

        self.folder = Some(doc.folder);
        self.serial = doc.serial;
        // Tabs are always rebuilt, even when the view is kept: the whole point
        // of re-scanning after an import is that `missing` changed, and a
        // preserved tab would still claim its file is absent.
        self.tabs = doc
            .pdfs
            .iter()
            .map(|file| Tab {
                file: file.clone(),
                label: tab_label(file),
                missing: doc.missing.contains(file),
            })
            .collect();

        match showing.and_then(|file| self.tabs.iter().position(|tab| tab.file == file)) {
            Some(index) => self.active_tab = index,
            None => self.reset_view(),
        }
    }

    /// Back to the first page, fitted.
    fn reset_view(&mut self) {
        self.active_tab = 0;
        self.page = 0;
        self.pages = 0;
        self.zoom = ZOOM_MIN;
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            tabs: self
                .tabs
                .iter()
                .map(|tab| DocTab {
                    label: tab.label.clone().into(),
                    missing: tab.missing,
                })
                .collect(),
            active_tab: self.active_tab as i32,
            serial: self.serial.clone(),
            zoom: self.zoom,
            page_index: self.page as i32,
            page_count: self.pages as i32,
            // Punctuation only — no translatable text in this string.
            page_label: if self.pages == 0 {
                String::new()
            } else {
                format!("{} / {}", self.page + 1, self.pages)
            },
        }
    }

    /// Decide what to draw next, claiming a fresh generation as it goes.
    ///
    /// The token is bumped first and unconditionally. Chron2 bumped it only on
    /// the path that asks for pixels, which meant every other transition — no
    /// tabs, no product, a tab whose file is missing — carried on sharing the
    /// previous request's token. A response for the document the user had just
    /// moved away from then still matched, and `receive` applied it over the
    /// top of the new state.
    fn plan(&mut self, scale: f32) -> Plan {
        self.token += 1;

        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Plan::Idle;
        };
        if tab.missing {
            return Plan::Show(ViewError::Missing);
        }
        let Some(folder) = &self.folder else {
            return Plan::Idle;
        };
        // Addressed by folder, never by name (CORE §3).
        let path = self.products_root.join(folder).join(&tab.file);

        let (width, height) = self.viewport;
        if width < 1.0 || height < 1.0 {
            // The layout has not run yet; the resize callback will come back.
            return Plan::Idle;
        }

        // Zoom folds straight into the target box: asking for twice the pane
        // and fitting the page inside it *is* 2× zoom.
        let scale = scale.max(0.1);
        let target = (
            ((width * scale * self.zoom).round() as u32).max(1),
            ((height * scale * self.zoom).round() as u32).max(1),
        );

        Plan::Render {
            token: self.token,
            path,
            page: self.page,
            target,
        }
    }
}

/// Tab text for a file name: the stem, first letter raised.
///
/// `invoice.pdf` → `Invoice`, `garanti.pdf` → `Garanti` — the labels CORE §4's
/// wireframe shows. Deriving from the file rather than translating keeps the
/// tab honest about what is on disk.
fn tab_label(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let stem = if stem.is_empty() { file } else { stem };

    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Render a [`ViewError`] through the string table.
pub fn describe(lang: Lang, error: &ViewError) -> String {
    let text = |key| strings::get(lang, key).to_string();
    match error {
        ViewError::Missing => text(Key::ErrPdfMissing),
        ViewError::Encrypted => text(Key::ErrEncrypted),
        ViewError::NoPages => text(Key::ErrNoPages),
        ViewError::Unreadable(detail) => {
            format!("{}: {detail}", strings::get(lang, Key::ErrUnreadable))
        }
        ViewError::NotAPdf(detail) if detail.is_empty() => text(Key::ErrNotAPdf),
        ViewError::NotAPdf(detail) => {
            format!("{}: {detail}", strings::get(lang, Key::ErrNotAPdf))
        }
        ViewError::RenderFailed(detail) if detail.is_empty() => text(Key::ErrRenderFailed),
        ViewError::RenderFailed(detail) => {
            format!("{}: {detail}", strings::get(lang, Key::ErrRenderFailed))
        }
    }
}

/// Everything the viewer needs kept alive for the life of the window, plus the
/// handles the vault reaches it through.
pub struct Viewer {
    state: Arc<Mutex<State>>,
    renderer: Rc<Renderer>,
    _resize: Rc<Timer>,
    _copied: Rc<Timer>,
}

impl Viewer {
    /// Show one product's documents, or nothing at all.
    ///
    /// The vault decides what is selected and calls this; the viewer no longer
    /// listens for the click itself, because a Slint callback holds exactly one
    /// handler and the vault needs that one.
    pub fn show(&self, app: &AppWindow, doc: Option<DocSet>, keep_view: bool) {
        self.state.lock().unwrap().show(doc, keep_view);
        apply(app, &self.state, &self.renderer);
    }

    /// Forget what the render worker remembers about a file that has changed.
    pub fn invalidate(&self, path: &Path) {
        self.renderer.invalidate(path);
    }
}

/// Wire the viewer into the window: callbacks in, rendered pages out.
pub fn install(app: &AppWindow, products_root: PathBuf, lang: Lang) -> Viewer {
    let state = Arc::new(Mutex::new(State::new(products_root, lang)));

    // Responses arrive on the worker thread; hop to the UI before touching it.
    let renderer = Rc::new(Renderer::spawn({
        let weak = app.as_weak();
        let state = Arc::clone(&state);
        move |response| {
            let weak = weak.clone();
            let state = Arc::clone(&state);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    receive(&app, &state, response);
                }
            });
        }
    }));

    let resize = Rc::new(Timer::default());
    let copied = Rc::new(Timer::default());

    // `on_product_selected` is deliberately absent: the vault owns the
    // selection and registers it. A Slint callback holds one handler, so a
    // second registration here would silently replace the vault's.

    app.on_tab_selected({
        let state = Arc::clone(&state);
        let renderer = Rc::clone(&renderer);
        let weak = app.as_weak();
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            {
                let mut state = state.lock().unwrap();
                if index < 0 || index as usize >= state.tabs.len() {
                    return;
                }
                // A new document starts fitted at page one.
                state.reset_view();
                state.active_tab = index as usize;
            }
            apply(&app, &state, &renderer);
        }
    });

    app.on_page_requested({
        let state = Arc::clone(&state);
        let renderer = Rc::clone(&renderer);
        let weak = app.as_weak();
        move |page| {
            let Some(app) = weak.upgrade() else { return };
            {
                let mut state = state.lock().unwrap();
                if page < 0 || page as usize >= state.pages {
                    return;
                }
                state.page = page as usize;
            }
            apply(&app, &state, &renderer);
        }
    });

    app.on_zoom_changed({
        let state = Arc::clone(&state);
        let renderer = Rc::clone(&renderer);
        let weak = app.as_weak();
        let resize = Rc::clone(&resize);
        move |zoom| {
            let Some(app) = weak.upgrade() else { return };
            {
                let mut state = state.lock().unwrap();
                let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
                if (zoom - state.zoom).abs() < f32::EPSILON {
                    return;
                }
                state.zoom = zoom;
            }
            // Dragging the slider emits a stream of values; re-render once it
            // settles rather than on every step.
            let state = Arc::clone(&state);
            let renderer = Rc::clone(&renderer);
            let weak = app.as_weak();
            resize.start(TimerMode::SingleShot, RESIZE_SETTLE, move || {
                if let Some(app) = weak.upgrade() {
                    apply(&app, &state, &renderer);
                }
            });
        }
    });

    app.on_viewport_resized({
        let state = Arc::clone(&state);
        let renderer = Rc::clone(&renderer);
        let weak = app.as_weak();
        let resize = Rc::clone(&resize);
        move |width, height| {
            let first = {
                let mut state = state.lock().unwrap();
                let was_unset = state.viewport.0 < 1.0 || state.viewport.1 < 1.0;
                state.viewport = (width, height);
                was_unset
            };

            let state = Arc::clone(&state);
            let renderer = Rc::clone(&renderer);
            let weak = weak.clone();
            let run = move || {
                if let Some(app) = weak.upgrade() {
                    apply(&app, &state, &renderer);
                }
            };

            // The very first measurement is what unblocks the initial render,
            // so do not make the user wait out the debounce for it.
            if first {
                run();
            } else {
                resize.start(TimerMode::SingleShot, RESIZE_SETTLE, run);
            }
        }
    });

    app.on_copy_serial({
        let state = Arc::clone(&state);
        let weak = app.as_weak();
        let copied = Rc::clone(&copied);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let serial = state.lock().unwrap().serial.clone();
            if serial.is_empty() {
                return;
            }

            // A clipboard that will not open is not worth taking the app down
            // for — the serial is still on screen either way.
            let ok = arboard::Clipboard::new()
                .and_then(|mut clipboard| clipboard.set_text(serial))
                .is_ok();
            if !ok {
                return;
            }

            app.set_serial_copied(true);
            let weak = app.as_weak();
            copied.start(TimerMode::SingleShot, COPIED_LINGER, move || {
                if let Some(app) = weak.upgrade() {
                    app.set_serial_copied(false);
                }
            });
        }
    });

    Viewer {
        state,
        renderer,
        _resize: resize,
        _copied: copied,
    }
}

/// Push state to the window, then act on what it needs drawn.
fn apply(app: &AppWindow, state: &Arc<Mutex<State>>, renderer: &Renderer) {
    let scale = app.window().scale_factor();

    // Take the lock only long enough to decide; Slint setters below can run
    // bindings that call straight back into these callbacks.
    let (snapshot, plan, lang) = {
        let mut state = state.lock().unwrap();
        if state.viewport.0 < 1.0 || state.viewport.1 < 1.0 {
            state.viewport = fallback_viewport(app);
        }
        (state.snapshot(), state.plan(scale), state.lang)
    };

    app.set_doc_tabs(ModelRc::new(VecModel::from(snapshot.tabs)));
    app.set_active_tab(snapshot.active_tab);
    app.set_selected_serial(snapshot.serial.into());
    app.set_zoom(snapshot.zoom);
    app.set_page_index(snapshot.page_index);
    app.set_page_count(snapshot.page_count);
    app.set_page_label(snapshot.page_label.into());

    match plan {
        Plan::Render {
            token,
            path,
            page,
            target,
        } => {
            app.set_viewer_error(Default::default());
            app.set_viewer_busy(true);
            renderer.request(token, path, page, target);
        }
        Plan::Show(error) => {
            app.set_viewer_busy(false);
            app.set_viewer_error(describe(lang, &error).into());
        }
        Plan::Idle => {
            app.set_viewer_busy(false);
            app.set_viewer_error(Default::default());
        }
    }
}

/// Derive the preview size from the window when the layout has not reported
/// one yet. Column 2 is exactly half the window (CORE §4) minus the chrome
/// above and below the preview.
fn fallback_viewport(app: &AppWindow) -> (f32, f32) {
    let size = app.window().size().to_logical(app.window().scale_factor());
    (
        (size.width * 0.5 - 1.0).max(1.0),
        (size.height - TITLE_BAR - TAB_ROW - CONTROL_ROW - SERIAL_STRIP).max(1.0),
    )
}

/// Apply one render result, ignoring anything the user has already moved past.
fn receive(app: &AppWindow, state: &Arc<Mutex<State>>, response: Response) {
    let (current, lang) = {
        let state = state.lock().unwrap();
        (state.token, state.lang)
    };

    match response {
        Response::Ready {
            token,
            page,
            pages,
            raster,
        } => {
            if token != current {
                return;
            }
            {
                let mut state = state.lock().unwrap();
                state.pages = pages;
                state.page = page.min(pages.saturating_sub(1));
            }

            let scale = app.window().scale_factor().max(0.1);
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                &raster.rgba,
                raster.width,
                raster.height,
            );

            app.set_page(Image::from_rgba8(buffer));
            // Drawn at exactly the pixels it was rendered for — anything else
            // would resample a page that is already the right size.
            app.set_page_width(raster.width as f32 / scale);
            app.set_page_height(raster.height as f32 / scale);
            app.set_page_index(page as i32);
            app.set_page_count(pages as i32);
            app.set_page_label(format!("{} / {}", page + 1, pages).into());
            app.set_viewer_error(Default::default());
            app.set_viewer_busy(false);
        }
        Response::Failed { token, error } => {
            if token != current {
                return;
            }
            app.set_viewer_busy(false);
            app.set_page_count(0);
            app.set_page_label(Default::default());
            app.set_viewer_error(describe(lang, &error).into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Product;
    use time::{Date, Month};

    fn product(folder: &str, pdfs: &[&str], missing: &[&str]) -> Product {
        let date = Date::from_calendar_date(2026, Month::March, 14).unwrap();
        Product {
            folder: folder.to_string(),
            name: folder.to_string(),
            serial: "ABC123XYZ".to_string(),
            link: String::new(),
            purchase_date: date,
            warranty_start: date,
            warranty_end: date,
            pdfs: pdfs.iter().map(|s| s.to_string()).collect(),
            added: date,
            missing_pdfs: missing.iter().map(|s| s.to_string()).collect(),
            extra: Default::default(),
        }
    }

    /// A viewer showing nothing, with a laid-out viewport — the shape most of
    /// these tests want.
    fn state() -> State {
        let mut state = State::new(PathBuf::from("/vault/products"), Lang::En);
        state.viewport = (600.0, 800.0);
        state
    }

    /// A two-document product, both files present.
    fn monitor() -> DocSet {
        DocSet::of(&product(
            "test-monitor",
            &["invoice.pdf", "garanti.pdf"],
            &[],
        ))
    }

    /// A two-document product whose second file is not on disk.
    fn drive() -> DocSet {
        DocSet::of(&product(
            "test-drive",
            &["invoice.pdf", "gone.pdf"],
            &["gone.pdf"],
        ))
    }

    #[test]
    fn showing_a_product_builds_a_tab_per_document_in_order() {
        let mut state = state();
        state.show(Some(monitor()), false);

        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.tabs[0].label, "Invoice");
        assert_eq!(state.tabs[1].label, "Garanti");
        assert_eq!(state.serial, "ABC123XYZ");
        assert!(state.tabs.iter().all(|tab| !tab.missing));
    }

    #[test]
    fn a_document_listed_but_absent_is_flagged_not_dropped() {
        let mut state = state();
        state.show(Some(drive()), false);

        assert_eq!(state.tabs.len(), 2, "the missing file still gets a tab");
        assert!(!state.tabs[0].missing);
        assert!(state.tabs[1].missing);
    }

    #[test]
    fn a_broken_folder_offers_no_documents() {
        let mut state = state();
        state.show(None, false);

        assert!(state.tabs.is_empty());
        assert!(state.serial.is_empty());
        assert_eq!(state.plan(1.0), Plan::Idle);
    }

    #[test]
    fn showing_a_missing_document_says_why_instead_of_rendering() {
        let mut state = state();
        state.show(Some(drive()), false);
        state.active_tab = 1;

        assert_eq!(state.plan(1.0), Plan::Show(ViewError::Missing));
    }

    #[test]
    fn a_render_targets_the_viewport_scaled_by_zoom_and_display() {
        let mut state = state();
        state.show(Some(monitor()), false);

        let Plan::Render {
            path,
            target,
            page,
            token,
        } = state.plan(2.0)
        else {
            panic!("a present document must render");
        };
        assert_eq!(
            path,
            PathBuf::from("/vault/products/test-monitor/invoice.pdf")
        );
        assert_eq!(page, 0);
        assert_eq!(token, 1);
        // 600×800 logical at 2× display scale, 1× zoom.
        assert_eq!(target, (1200, 1600));

        state.zoom = 2.0;
        let Plan::Render { target, token, .. } = state.plan(2.0) else {
            panic!("zooming still renders");
        };
        assert_eq!(target, (2400, 3200), "zoom multiplies the fit box");
        assert_eq!(token, 2, "every request supersedes the last");
    }

    /// The Chron2 defect this milestone fixes.
    ///
    /// A transition that does not ask for pixels used to leave the token alone,
    /// so a response already in flight for the previous document still matched
    /// and was applied over the top of the new state.
    #[test]
    fn every_transition_supersedes_an_in_flight_render_not_just_another_render() {
        let mut state = state();
        state.show(Some(monitor()), false);

        let Plan::Render { token: flying, .. } = state.plan(1.0) else {
            panic!("a present document must render");
        };

        // Move to a product whose first tab is missing: this shows a message
        // rather than asking for pixels, and must still supersede.
        state.show(Some(drive()), false);
        state.active_tab = 1;
        assert_eq!(state.plan(1.0), Plan::Show(ViewError::Missing));
        assert!(
            state.token > flying,
            "a missing-file state must supersede the render still in flight \
             (token {} vs {flying})",
            state.token
        );

        // Same again for the idle paths.
        let before = state.token;
        state.show(None, false);
        assert_eq!(state.plan(1.0), Plan::Idle);
        assert!(state.token > before, "an empty selection supersedes too");
    }

    #[test]
    fn nothing_renders_before_the_layout_has_a_size() {
        let mut state = state();
        state.show(Some(monitor()), false);
        state.viewport = (0.0, 0.0);

        assert_eq!(state.plan(1.0), Plan::Idle);
    }

    #[test]
    fn switching_document_or_product_returns_to_a_fitted_first_page() {
        let mut state = state();
        state.show(Some(monitor()), false);
        state.page = 7;
        state.pages = 12;
        state.zoom = 3.5;

        state.reset_view();
        assert_eq!(state.page, 0);
        assert_eq!(state.zoom, ZOOM_MIN);
        assert_eq!(state.active_tab, 0);

        // Showing another product resets too, and re-reads the serial.
        state.page = 4;
        state.zoom = 2.0;
        state.show(Some(drive()), false);
        assert_eq!(state.page, 0);
        assert_eq!(state.zoom, ZOOM_MIN);
        assert_eq!(state.serial, "ABC123XYZ");
    }

    #[test]
    fn keeping_the_view_stays_on_the_same_file_at_the_same_page() {
        let mut state = state();
        state.show(Some(monitor()), false);
        state.active_tab = 1;
        state.page = 4;
        state.pages = 9;
        state.zoom = 2.5;

        // What a re-sort does: same documents, nothing about the view changed.
        state.show(Some(monitor()), true);

        assert_eq!(state.active_tab, 1, "still on Garanti");
        assert_eq!(state.page, 4, "still on the same page");
        assert_eq!(state.zoom, 2.5, "still at the same zoom");
    }

    #[test]
    fn keeping_the_view_falls_back_to_page_one_when_the_file_is_gone() {
        let mut state = state();
        state.show(Some(monitor()), false);
        state.active_tab = 1;
        state.page = 4;
        state.zoom = 2.5;

        // What removing that document in the edit form does.
        let mut trimmed = monitor();
        trimmed.pdfs.retain(|file| file != "garanti.pdf");
        state.show(Some(trimmed), true);

        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.active_tab, 0);
        assert_eq!(state.page, 0);
        assert_eq!(state.zoom, ZOOM_MIN);
    }

    #[test]
    fn a_file_that_has_arrived_stops_being_flagged_as_missing() {
        let mut state = state();
        state.show(Some(drive()), false);
        assert!(state.tabs[1].missing);

        // What importing the absent file does: same tabs, nothing missing now.
        let mut found = drive();
        found.missing.clear();
        state.show(Some(found), true);

        assert!(
            !state.tabs[1].missing,
            "tabs are rebuilt on every show, so `missing` cannot go stale"
        );
    }

    #[test]
    fn the_page_label_is_one_based_and_blank_before_a_count_is_known() {
        let mut state = state();
        state.show(Some(monitor()), false);
        assert_eq!(state.snapshot().page_label, "");

        state.pages = 12;
        state.page = 1;
        let snapshot = state.snapshot();
        assert_eq!(snapshot.page_label, "2 / 12");
        assert_eq!(snapshot.page_index, 1);
        assert_eq!(snapshot.page_count, 12);
    }

    #[test]
    fn tab_labels_come_from_the_file_stem() {
        assert_eq!(tab_label("invoice.pdf"), "Invoice");
        assert_eq!(tab_label("garanti.pdf"), "Garanti");
        assert_eq!(tab_label("warranty-card.pdf"), "Warranty-card");
    }

    #[test]
    fn tab_labels_survive_odd_file_names() {
        assert_eq!(tab_label("no-extension"), "No-extension");
        assert_eq!(tab_label(".pdf"), ".pdf");
        assert_eq!(tab_label(""), "");
        // Only the last dot is an extension.
        assert_eq!(tab_label("scan.2026.pdf"), "Scan.2026");
    }

    #[test]
    fn tab_labels_handle_non_ascii() {
        // Turkish dotted capital: the point of using `to_uppercase` rather
        // than byte arithmetic.
        assert_eq!(tab_label("ürün.pdf"), "Ürün");
    }

    #[test]
    fn errors_render_through_the_string_table_in_both_languages() {
        for error in [
            ViewError::Missing,
            ViewError::Encrypted,
            ViewError::NoPages,
            ViewError::NotAPdf(String::new()),
            ViewError::RenderFailed(String::new()),
            ViewError::Unreadable("detail".to_string()),
        ] {
            for lang in [Lang::En, Lang::Tr] {
                assert!(!describe(lang, &error).is_empty(), "{error:?} in {lang:?}");
            }
        }
        assert_ne!(
            describe(Lang::En, &ViewError::Encrypted),
            describe(Lang::Tr, &ViewError::Encrypted),
        );
    }

    #[test]
    fn diagnostic_detail_is_appended_not_swallowed() {
        let text = describe(Lang::En, &ViewError::Unreadable("no such device".into()));
        assert!(text.contains("no such device"));
    }
}
