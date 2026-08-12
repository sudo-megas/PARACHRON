//! The add/edit form: what is in it, whether it is valid, and what Save does.
//!
//! The form owns no window and writes no files. It reads what was typed,
//! decides whether it is a product, and hands a [`Job`] to [`crate::import`],
//! which does the writing on a thread of its own. What comes back goes to the
//! vault, which is the only thing that changes the list.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use time::{Date, OffsetDateTime};

use crate::data::{self, Draft, Product};
use crate::import::{self, Job, Outcome};
use crate::render::Invalidator;
use crate::strings::{self, Key, Lang};
use crate::vault::{self, Vault};
use crate::viewer::Viewer;
use crate::{AppWindow, FormDoc};

/// One document the form is showing.
#[derive(Debug, Clone)]
enum Doc {
    /// Already in the product folder.
    Kept(String),
    /// Chosen in this session and not copied anywhere yet.
    Picked { source: PathBuf, name: String },
}

impl Doc {
    fn name(&self) -> &str {
        match self {
            Doc::Kept(name) => name,
            Doc::Picked { name, .. } => name,
        }
    }
}

/// The text in the fields, read out of the window before anything is borrowed.
struct Typed {
    name: String,
    serial: String,
    link: String,
    purchase: String,
    start: String,
    end: String,
}

impl Typed {
    fn read(app: &AppWindow) -> Self {
        Self {
            name: app.get_form_name().to_string(),
            serial: app.get_form_serial().to_string(),
            link: app.get_form_link().to_string(),
            purchase: app.get_form_purchase_date().to_string(),
            start: app.get_form_warranty_start().to_string(),
            end: app.get_form_warranty_end().to_string(),
        }
    }
}

/// What validation concluded: a message per field, and a product if there were
/// none.
#[derive(Default)]
struct Report {
    name: String,
    purchase: String,
    start: String,
    end: String,
    draft: Option<Draft>,
}

impl Report {
    fn clean(&self) -> bool {
        self.name.is_empty()
            && self.purchase.is_empty()
            && self.start.is_empty()
            && self.end.is_empty()
    }
}

struct Editor {
    products_root: PathBuf,
    /// Lets a commit ask the render worker to release its handle on a removed
    /// file before deleting it, rather than after (see [`Job::invalidator`]).
    invalidator: Invalidator,
    lang: Lang,
    /// The folder being edited. `None` means a new product.
    editing: Option<String>,
    docs: Vec<Doc>,
    /// Copies to delete when this is saved.
    removals: Vec<String>,
    /// Preserved from the product being edited; today for a new one.
    added: Date,
    /// Manifest keys Parachron has no field for, carried through the round trip.
    extra: toml::Table,
    /// Validation stays quiet until Save has been pressed once. Nagging about
    /// an empty name while somebody is still typing it is just rude.
    tried: bool,
    busy: bool,
    /// The file the last commit refused, so its row can say so.
    refused: Option<String>,
    /// Where a finished commit leaves its result. The commit runs on a thread
    /// and this editor is full of `Rc`s, so the outcome travels here rather
    /// than in a closure.
    slot: Arc<Mutex<Option<Outcome>>>,
}

impl Editor {
    /// Today, for a product being added.
    ///
    /// UTC rather than local: reading the local offset is only sound while the
    /// process is single-threaded, and by now the render worker is running.
    /// Chron4 captures the offset at startup and both callers move to it. For
    /// an `added` date used as a tie-break in insertion order, a few hours is
    /// not worth more than this.
    fn today() -> Date {
        OffsetDateTime::now_utc().date()
    }

    fn reset(&mut self) {
        self.editing = None;
        self.docs.clear();
        self.removals.clear();
        self.added = Self::today();
        self.extra = Default::default();
        self.tried = false;
        self.busy = false;
        self.refused = None;
    }

    fn load(&mut self, product: &Product) {
        self.reset();
        self.editing = Some(product.folder.clone());
        self.docs = product.pdfs.iter().cloned().map(Doc::Kept).collect();
        self.added = product.added;
        self.extra = product.extra.clone();
    }

    fn taken(&self) -> Vec<String> {
        self.docs.iter().map(|doc| doc.name().to_string()).collect()
    }

    /// Chron6. Nothing is re-pushed: the form's heading and its per-field
    /// messages are only on screen while the sheet is up, and while the sheet is
    /// up its backdrop covers the window — so `Document ▾` is unreachable and the
    /// language cannot change underneath it. The next `open()` composes them
    /// afresh in whatever language is then in effect.
    ///
    /// If a later milestone ever makes a sheet dismissable by clicking away, or
    /// puts a menu above one, this is the paragraph that stops being true.
    fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
    }

    /// Decide whether what was typed is a product.
    fn check(&self, typed: &Typed) -> Report {
        let text = |key| strings::get(self.lang, key).to_string();
        let mut report = Report::default();

        let name = typed.name.trim().to_string();
        if name.is_empty() {
            report.name = text(Key::ErrNameRequired);
        }

        let purchase = data::parse_date(&typed.purchase);
        let start = data::parse_date(&typed.start);
        let end = data::parse_date(&typed.end);

        if purchase.is_none() {
            report.purchase = text(Key::ErrDateInvalid);
        }
        if start.is_none() {
            report.start = text(Key::ErrDateInvalid);
        }
        if end.is_none() {
            report.end = text(Key::ErrDateInvalid);
        }

        // Only worth saying once both ends are real dates.
        if let (Some(start), Some(end)) = (start, end)
            && end < start
        {
            report.end = text(Key::ErrWarrantyBackwards);
        }

        // A warranty that starts before the item was purchased is not a real
        // warranty — almost always a typo'd year in one of the two fields —
        // and nothing else here or in `Draft`/`Manifest` catches it. Checked
        // against `start` rather than `end`: a warranty is allowed to start
        // the same day it is purchased, and `end < start` above already
        // covers the case where `end` alone is the one out of order.
        if let (Some(purchase), Some(start)) = (purchase, start)
            && start < purchase
        {
            report.purchase = text(Key::ErrPurchaseAfterWarranty);
        }

        if let (true, Some(purchase_date), Some(warranty_start), Some(warranty_end)) =
            (report.clean(), purchase, start, end)
        {
            report.draft = Some(Draft {
                name,
                serial: typed.serial.trim().to_string(),
                link: typed.link.trim().to_string(),
                purchase_date,
                warranty_start,
                warranty_end,
                pdfs: self.taken(),
                added: self.added,
                extra: self.extra.clone(),
            });
        }

        report
    }

    fn job(&self, draft: Draft) -> Job {
        Job {
            products_root: self.products_root.clone(),
            folder: self.editing.clone(),
            draft,
            imports: self
                .docs
                .iter()
                .filter_map(|doc| match doc {
                    Doc::Picked { source, name } => Some((source.clone(), name.clone())),
                    Doc::Kept(_) => None,
                })
                .collect(),
            removals: self.removals.clone(),
            invalidator: Some(self.invalidator.clone()),
        }
    }

    fn rows(&self) -> Vec<FormDoc> {
        self.docs
            .iter()
            .map(|doc| {
                let failed = self.refused.as_deref() == Some(doc.name());
                let detail = match doc {
                    // Where it is coming from, which is the difference between
                    // one `download.pdf` and another.
                    Doc::Picked { source, .. } => source
                        .parent()
                        .map(|dir| dir.display().to_string())
                        .unwrap_or_default(),
                    Doc::Kept(_) => String::new(),
                };
                FormDoc {
                    label: doc.name().into(),
                    detail: detail.into(),
                    failed,
                }
            })
            .collect()
    }
}

/// What the language switch reaches the form through.
///
/// An opaque handle rather than the `Editor` itself, the same shape
/// `details::Details` uses: the form's internals are nobody else's business and
/// the only thing anything outside this module needs to do to it is tell it the
/// language changed.
/// `Clone` since Chron9: the language switch and the vault move both need to
/// reach the form, and both are handed a handle rather than the one owner.
/// Cloning an `Rc` here shares the same `Editor`, which is the point.
#[derive(Clone)]
pub struct Editors {
    editor: Rc<RefCell<Editor>>,
}

impl Editors {
    pub fn set_lang(&self, lang: Lang) {
        self.editor.borrow_mut().set_lang(lang);
    }

    /// Chron9. Imports land under the new root from here on; see
    /// `viewer::Viewer::set_products_root` for why this is a copy rather than a
    /// share.
    pub fn set_products_root(&self, root: PathBuf) {
        self.editor.borrow_mut().products_root = root;
    }
}

/// Wire the form into the window.
pub fn install(
    app: &AppWindow,
    products_root: PathBuf,
    lang: Lang,
    vault: Rc<RefCell<Vault>>,
    viewer: Rc<Viewer>,
) -> Editors {
    let editor = Rc::new(RefCell::new(Editor {
        products_root,
        invalidator: viewer.invalidator(),
        lang,
        editing: None,
        docs: Vec::new(),
        removals: Vec::new(),
        added: Editor::today(),
        extra: Default::default(),
        tried: false,
        busy: false,
        refused: None,
        slot: Arc::new(Mutex::new(None)),
    }));

    app.on_add_document({
        let editor = Rc::clone(&editor);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            editor.borrow_mut().reset();
            open(&app, &editor, Key::FormAddTitle, None);
        }
    });

    app.on_edit_document({
        let editor = Rc::clone(&editor);
        let vault = Rc::clone(&vault);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            // A folder whose manifest will not parse has nothing to pre-fill a
            // form with, and guessing at it would be worse than the text editor
            // the user already has.
            let Some(product) = vault::selected_product(&vault) else {
                return;
            };
            editor.borrow_mut().load(&product);
            open(&app, &editor, Key::FormEditTitle, Some(&product));
        }
    });

    app.on_form_edited({
        let editor = Rc::clone(&editor);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let typed = Typed::read(&app);
            let report = {
                let editor = editor.borrow();
                // Silent until the first Save; after that, errors clear as
                // they are fixed rather than waiting for another attempt.
                if !editor.tried {
                    return;
                }
                editor.check(&typed)
            };
            show_errors(&app, &report);
        }
    });

    app.on_form_add_pdf({
        let editor = Rc::clone(&editor);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let lang = editor.borrow().lang;

            let editor = Rc::clone(&editor);
            let inner = app.as_weak();
            import::pick(
                app.window(),
                strings::get(lang, Key::ActionAddPdf),
                strings::get(lang, Key::FilterPdf),
                move |paths| {
                    let Some(app) = inner.upgrade() else { return };
                    if paths.is_empty() {
                        return;
                    }
                    {
                        let mut editor = editor.borrow_mut();
                        for path in paths {
                            let name = import::destination_name(&path, &editor.taken());
                            editor.docs.push(Doc::Picked { source: path, name });
                        }
                        editor.refused = None;
                    }
                    app.set_form_notice(SharedString::new());
                    show_docs(&app, &editor);
                },
            );
        }
    });

    app.on_form_remove_pdf({
        let editor = Rc::clone(&editor);
        let weak = app.as_weak();
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            {
                let mut editor = editor.borrow_mut();
                if editor.busy {
                    // A commit already has this editor's `removals` — and
                    // everything else about this draft — copied into a `Job`
                    // on its way to another thread. Changing it now would be
                    // invisible to that commit: the row would disappear from
                    // the sheet as if the removal had been saved, while the
                    // file it named stays on disk untouched, because the
                    // `Job` already in flight has no way to learn of it. The
                    // form.slint side of this disables the button for the
                    // same reason; this is the guard that holds even if that
                    // one is ever bypassed.
                    return;
                }
                if index < 0 || index as usize >= editor.docs.len() {
                    return;
                }
                match editor.docs.remove(index as usize) {
                    // Our copy goes when the save goes through. The file the
                    // user imported it from is never touched.
                    Doc::Kept(name) => editor.removals.push(name),
                    // Never copied anywhere, so there is nothing to delete.
                    Doc::Picked { .. } => {}
                }
                editor.refused = None;
            }
            show_docs(&app, &editor);
        }
    });

    app.on_form_cancel({
        let editor = Rc::clone(&editor);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            if editor.borrow().busy {
                // A commit is already writing; letting the sheet go now would
                // leave nobody to report what happened.
                return;
            }
            editor.borrow_mut().reset();
            close(&app);
        }
    });

    app.on_form_save({
        let editor = Rc::clone(&editor);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let typed = Typed::read(&app);

            // Everything is decided under the borrow; nothing touches the
            // window until it has been dropped.
            let (report, job, slot) = {
                let mut editor = editor.borrow_mut();
                if editor.busy {
                    return;
                }
                editor.tried = true;
                let report = editor.check(&typed);
                let job = report.draft.clone().map(|draft| editor.job(draft));
                if job.is_some() {
                    editor.busy = true;
                    editor.refused = None;
                }
                (report, job, Arc::clone(&editor.slot))
            };

            show_errors(&app, &report);
            app.set_form_busy(job.is_some());
            if let Some(job) = job {
                app.set_form_notice(SharedString::new());
                import::commit(job, slot, app.as_weak());
            }
        }
    });

    app.on_form_commit_finished({
        let editor = Rc::clone(&editor);
        let vault = Rc::clone(&vault);
        let viewer = Rc::clone(&viewer);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };

            let outcome = {
                let editor = editor.borrow();
                editor.slot.lock().ok().and_then(|mut slot| slot.take())
            };
            let Some(outcome) = outcome else { return };

            let lang = editor.borrow().lang;
            app.set_form_busy(false);

            match outcome {
                Outcome::Done { folder, invalidate } => {
                    // The worker caches by path and has no idea a file has been
                    // replaced, so every path just written has to be named.
                    for path in &invalidate {
                        viewer.invalidate(path);
                    }
                    editor.borrow_mut().reset();
                    close(&app);
                    vault::rescan(&vault, &app, &viewer, Some(&folder));
                }
                Outcome::Refused { file, reason } => {
                    {
                        let mut editor = editor.borrow_mut();
                        editor.busy = false;
                        editor.refused = Some(file.clone());
                    }
                    show_docs(&app, &editor);
                    app.set_form_notice(
                        format!("{file}: {}", crate::viewer::describe(lang, &reason)).into(),
                    );
                }
                Outcome::Failed(reason) => {
                    editor.borrow_mut().busy = false;
                    app.set_form_notice(
                        format!(
                            "{}: {}",
                            strings::get(lang, Key::ErrSaveFailed),
                            vault::describe(lang, &reason)
                        )
                        .into(),
                    );
                }
            }
        }
    });

    Editors { editor }
}

/// Fill the sheet and show it.
fn open(app: &AppWindow, editor: &Rc<RefCell<Editor>>, heading: Key, product: Option<&Product>) {
    let lang = editor.borrow().lang;
    app.set_form_heading(strings::get(lang, heading).into());

    let (name, serial, link, purchase, start, end) = match product {
        Some(product) => (
            product.name.clone(),
            product.serial.clone(),
            product.link.clone(),
            data::fmt_date(product.purchase_date),
            data::fmt_date(product.warranty_start),
            data::fmt_date(product.warranty_end),
        ),
        None => Default::default(),
    };

    app.set_form_name(name.into());
    app.set_form_serial(serial.into());
    app.set_form_link(link.into());
    app.set_form_purchase_date(purchase.into());
    app.set_form_warranty_start(start.into());
    app.set_form_warranty_end(end.into());

    show_errors(app, &Report::default());
    app.set_form_notice(SharedString::new());
    app.set_form_busy(false);
    show_docs(app, editor);
    app.set_form_open(true);
}

fn close(app: &AppWindow) {
    app.set_form_open(false);
    app.set_form_notice(SharedString::new());
    app.set_form_busy(false);
    show_errors(app, &Report::default());
}

fn show_errors(app: &AppWindow, report: &Report) {
    app.set_form_name_error(report.name.as_str().into());
    app.set_form_purchase_date_error(report.purchase.as_str().into());
    app.set_form_warranty_start_error(report.start.as_str().into());
    app.set_form_warranty_end_error(report.end.as_str().into());
}

fn show_docs(app: &AppWindow, editor: &Rc<RefCell<Editor>>) {
    let rows = editor.borrow().rows();
    app.set_form_docs(ModelRc::new(VecModel::from(rows)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn editor() -> Editor {
        Editor {
            products_root: PathBuf::from("/vault/products"),
            // A real, momentarily-live channel handle is simpler than a mock:
            // the worker it points at shuts down the instant this temporary
            // `Renderer` is dropped, and `Invalidator::invalidate` already
            // ignores a send to nobody, so this behaves as a harmless no-op.
            invalidator: crate::render::Renderer::spawn(|_| {}).invalidator(),
            lang: Lang::En,
            editing: None,
            docs: Vec::new(),
            removals: Vec::new(),
            added: Date::from_calendar_date(2026, Month::August, 5).unwrap(),
            extra: Default::default(),
            tried: false,
            busy: false,
            refused: None,
            slot: Arc::new(Mutex::new(None)),
        }
    }

    fn typed(name: &str, purchase: &str, start: &str, end: &str) -> Typed {
        Typed {
            name: name.to_string(),
            serial: "ABC123XYZ".to_string(),
            link: "https://store.example/p".to_string(),
            purchase: purchase.to_string(),
            start: start.to_string(),
            end: end.to_string(),
        }
    }

    #[test]
    fn a_filled_in_form_becomes_a_product() {
        let report = editor().check(&typed(
            "QD-OLED Monitor",
            "14-03-2026",
            "14-03-2026",
            "14-03-2029",
        ));
        assert!(report.clean());
        let draft = report.draft.expect("a valid form produces a draft");
        assert_eq!(draft.name, "QD-OLED Monitor");
        assert_eq!(draft.warranty_end.year(), 2029);
    }

    #[test]
    fn a_product_needs_a_name() {
        let report = editor().check(&typed("   ", "14-03-2026", "14-03-2026", "14-03-2029"));
        assert!(!report.name.is_empty());
        assert!(report.draft.is_none(), "nothing is written without a name");
    }

    #[test]
    fn a_date_that_is_not_a_date_stops_the_save_and_says_which_one() {
        let report = editor().check(&typed("Monitor", "31-02-2026", "14-03-2026", "14-03-2029"));
        assert!(
            !report.purchase.is_empty(),
            "the 30th of February is not a day"
        );
        assert!(report.start.is_empty(), "the other dates are fine");
        assert!(report.end.is_empty());
        assert!(report.draft.is_none());
    }

    #[test]
    fn a_warranty_cannot_end_before_it_starts() {
        let report = editor().check(&typed("Monitor", "14-03-2026", "14-03-2029", "14-03-2026"));
        assert_eq!(
            report.end,
            strings::get(Lang::En, Key::ErrWarrantyBackwards)
        );
        assert!(report.draft.is_none());
    }

    #[test]
    fn a_warranty_may_start_and_end_on_the_same_day() {
        let report = editor().check(&typed("Monitor", "14-03-2026", "14-03-2026", "14-03-2026"));
        assert!(report.clean(), "a one-day warranty is odd, not invalid");
    }

    /// A warranty starting years before the item was purchased is a typo, not
    /// a real product — and until now nothing here or in `Draft`/`Manifest`
    /// caught it.
    #[test]
    fn a_warranty_cannot_start_before_the_item_was_purchased() {
        let report = editor().check(&typed("Monitor", "14-03-2026", "14-03-2020", "14-03-2027"));
        assert_eq!(
            report.purchase,
            strings::get(Lang::En, Key::ErrPurchaseAfterWarranty)
        );
        assert!(report.draft.is_none());
    }

    #[test]
    fn a_warranty_may_start_the_same_day_the_item_was_purchased() {
        let report = editor().check(&typed("Monitor", "14-03-2026", "14-03-2026", "14-03-2027"));
        assert!(
            report.clean(),
            "buying and activating on the same day is ordinary"
        );
    }

    #[test]
    fn editing_keeps_the_added_date_and_the_unknown_keys() {
        let mut editor = editor();
        let mut extra = toml::Table::new();
        extra.insert("notes".to_string(), toml::Value::String("keep me".into()));

        let product = Product {
            folder: "monitor".to_string(),
            name: "Monitor".to_string(),
            serial: String::new(),
            link: String::new(),
            purchase_date: Date::from_calendar_date(2024, Month::January, 2).unwrap(),
            warranty_start: Date::from_calendar_date(2024, Month::January, 2).unwrap(),
            warranty_end: Date::from_calendar_date(2027, Month::January, 2).unwrap(),
            pdfs: vec!["invoice.pdf".to_string()],
            added: Date::from_calendar_date(2024, Month::January, 3).unwrap(),
            missing_pdfs: Vec::new(),
            extra,
        };
        editor.load(&product);

        let report = editor.check(&typed("Monitor", "02-01-2024", "02-01-2024", "02-01-2027"));
        let draft = report.draft.expect("a valid edit");
        assert_eq!(draft.added, product.added, "editing is not re-adding");
        assert_eq!(
            draft.extra.get("notes").and_then(|v| v.as_str()),
            Some("keep me"),
        );
        assert_eq!(
            draft.pdfs,
            ["invoice.pdf"],
            "existing documents stay listed"
        );
    }

    #[test]
    fn removing_an_existing_document_schedules_our_copy_for_deletion() {
        let mut editor = editor();
        editor.docs = vec![
            Doc::Kept("invoice.pdf".to_string()),
            Doc::Picked {
                source: PathBuf::from("/home/someone/warranty.pdf"),
                name: "warranty.pdf".to_string(),
            },
        ];

        // The one already in the folder has a copy to clean up.
        if let Doc::Kept(name) = editor.docs.remove(0) {
            editor.removals.push(name);
        }
        assert_eq!(editor.removals, ["invoice.pdf"]);

        // The one only ever picked was never copied, so there is nothing to do.
        editor.docs.remove(0);
        assert_eq!(editor.removals.len(), 1);
    }

    #[test]
    fn a_job_only_copies_the_files_that_are_not_already_there() {
        let mut editor = editor();
        editor.editing = Some("monitor".to_string());
        editor.docs = vec![
            Doc::Kept("invoice.pdf".to_string()),
            Doc::Picked {
                source: PathBuf::from("/home/someone/warranty.pdf"),
                name: "warranty.pdf".to_string(),
            },
        ];

        let report = editor.check(&typed("Monitor", "14-03-2026", "14-03-2026", "14-03-2029"));
        let job = editor.job(report.draft.unwrap());

        assert_eq!(job.folder.as_deref(), Some("monitor"));
        assert_eq!(job.imports.len(), 1, "only the newly picked file is copied");
        assert_eq!(job.imports[0].1, "warranty.pdf");
        assert_eq!(
            job.draft.pdfs,
            ["invoice.pdf", "warranty.pdf"],
            "both are listed, in the order they appear",
        );
    }
}
