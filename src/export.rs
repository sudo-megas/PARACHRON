//! One all-covering PDF per product (CORE §6): a generated summary page, then
//! every one of that product's documents in tab order.
//!
//! This module mirrors `import.rs` deliberately, down to the shape of its types —
//! a [`Job`] that can cross to a thread, an [`Outcome`] that comes back, a
//! [`commit`] that spawns and rings the window, and a [`run`] that is a plain
//! function of a `Job` and is where every test points. The two are the same kind
//! of thing: file I/O plus MuPDF, off the UI thread, reporting into a slot. Making
//! the second one look like the first is cheaper to read than a second invention.
//!
//! Two things about the summary page are not obvious and are load-bearing.
//!
//! Every text run is registered as a **composite** font rather than a simple one.
//! `TextOptions` defaults to `simple: true` with a Latin encoding, and the three
//! encodings the crate offers are Latin, Greek and Cyrillic — none of which
//! contains `ğ ş ı İ`. Drawn that way those glyphs are dropped silently: the
//! saved PDF raises nothing and simply does not contain the words. This is not a
//! Turkish-mode concern, because product names and serial numbers are user data
//! and an English session has to export `Şarj Cihazı` correctly. Composite for
//! everything, unconditionally — deciding per string is how the near-miss gets
//! reintroduced, and the near-miss is real: `Ü` survives a Latin encoding because
//! it is in Latin-1, so a careless check on the wrong word passes.
//!
//! The sources are all inspected **before** anything is drawn, because the page
//! has to be able to say which documents it could not include, and it cannot say
//! that if it was drawn before anybody tried to open them.

use std::cell::RefCell;
use std::fs::File;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use mupdf::pdf::PdfDocument;
use mupdf::shape::{PdfColor, Shape, TextOptions};
use mupdf::{Point, Size};
use slint::ComponentHandle;
use time::{Date, UtcOffset};

use crate::data::{self, DataError, Product};
use crate::render::{self, ViewError};
use crate::strings::{self, Key, Lang};
use crate::vault::Vault;
use crate::AppWindow;

/// A4 in PDF points (CORE §6: print-friendly). The summary page is always this,
/// whatever the appended documents are — it is a document Parachron authored.
/// Appended pages keep their own sizes; rescaling somebody's invoice to match
/// would be the export quietly altering their evidence.
const PAGE: Size = Size::A4;

/// Margin, and the type sizes the page is set in.
const MARGIN: f32 = 56.0;
const TITLE: f32 = 22.0;
const HEADING: f32 = 9.5;
const BODY: f32 = 12.0;
const COUNTER: f32 = 19.0;
const FOOTNOTE: f32 = 8.5;

/// Distance from a field's label baseline to its value's.
const LABEL_GAP: f32 = 15.0;
/// Distance between one field and the next.
const FIELD_GAP: f32 = 30.0;

/// Ink. Black on white, theme-independent by construction (CORE §6) — the export
/// reads nothing from `Palette`, because a printed page is not a window.
fn ink() -> PdfColor {
    PdfColor::gray(0.0)
}

fn quiet_ink() -> PdfColor {
    PdfColor::gray(0.42)
}

/// What a commit did, or why it did not.
///
/// The written path is deliberately not in here: the caller supplied it as
/// [`Job::destination`] and still has it, so echoing it back would be a field that
/// can only ever agree with something already known.
#[derive(Debug)]
pub enum Outcome {
    Done {
        /// Documents that could not be included, and why. The export still
        /// succeeded; the summary page names them.
        skipped: Vec<(String, ViewError)>,
    },
    Failed(DataError),
}

/// Everything one export needs, in a package that can cross to a thread.
#[derive(Debug, Clone)]
pub struct Job {
    /// Where the product's own files live.
    pub folder: PathBuf,
    /// The product, as the summary page reads it.
    pub product: Product,
    /// Where the user asked for the output.
    pub destination: PathBuf,
    /// Composed on the UI thread, from the offset `main` captured before any
    /// thread existed — so the page's countdown is the one column 3 showed.
    pub today: Date,
    pub lang: Lang,
}

/// Run an export on a thread and ring the window when it lands.
///
/// A thread of its own rather than the render worker, for the two reasons
/// `import.rs` gives: MuPDF contexts are per-thread and Chron2's rule that the UI
/// thread never calls MuPDF is worth keeping, and the render worker's queue
/// deliberately drops all but the newest job — right for pixels, silent data loss
/// for a file somebody asked to be written.
pub fn commit(job: Job, slot: Arc<Mutex<Option<Outcome>>>, weak: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        let outcome = run(job);
        if let Ok(mut slot) = slot.lock() {
            *slot = Some(outcome);
        }
        let _ = weak.upgrade_in_event_loop(|app| app.invoke_export_finished());
    });
}

/// Build one product's export and write it.
///
/// Nothing here calls `Renderer::invalidate`, and nothing should: the output goes
/// to a path the user chose, outside the vault, and the product's own files are
/// read without being changed. Chron3 added that message because imports write
/// over paths the viewer may have cached; export writes nowhere the render worker
/// has heard of.
pub fn run(job: Job) -> Outcome {
    let mupdf_failed = |e: mupdf::Error| DataError::Malformed(e.to_string());

    // Every source is opened before anything is drawn, so the summary page can
    // name what it had to leave out — and so a refusal costs nothing, since
    // nothing has been written yet.
    let mut sources = Vec::new();
    let mut skipped = Vec::new();
    for name in &job.product.pdfs {
        match render::open_pdf(&job.folder.join(name)) {
            Ok(document) => sources.push(document),
            // A bad file does not fail the export. CORE §6 says the output covers
            // the product, and a product with one unreadable invoice still has a
            // warranty worth carrying. Refusing outright would be the app deciding
            // the user does not get their summary because one file is broken.
            Err(reason) => skipped.push((name.clone(), reason)),
        }
    }

    let mut out = PdfDocument::new();
    if let Err(e) = summary(&mut out, &job, &skipped) {
        return Outcome::Failed(mupdf_failed(e));
    }

    for source in &sources {
        if let Err(e) = out.insert_pdf(source, Default::default()) {
            return Outcome::Failed(mupdf_failed(e));
        }
    }

    // `write_to` rather than `save`, which takes a `&str` and so cannot express a
    // destination that is not valid UTF-8 — and on Linux a path is bytes, so that
    // is a real file somebody could have picked rather than a hypothetical.
    match File::create(&job.destination) {
        Ok(mut file) => match out.write_to(&mut file) {
            Ok(_) => Outcome::Done { skipped },
            Err(e) => Outcome::Failed(mupdf_failed(e)),
        },
        Err(e) => Outcome::Failed(DataError::Unreadable(e.to_string())),
    }
}

/// Draw the summary page onto `doc` as its first page.
fn summary(
    doc: &mut PdfDocument,
    job: &Job,
    skipped: &[(String, ViewError)],
) -> Result<(), mupdf::Error> {
    let tr = |key| strings::get(job.lang, key);
    let product = &job.product;

    // The page has to be dropped before `commit` takes the document again, which
    // is what this block is for.
    let mut page = doc.new_page(PAGE)?;
    {
        let mut shape = Shape::new(&mut page)?;
        let mut y = MARGIN + TITLE;

        // ── Wordmark and product name ────────────────────────────────────
        shape.insert_text(
            Point::new(MARGIN, y),
            tr(Key::AppTitle),
            &text(FOOTNOTE, quiet_ink()),
        )?;
        y += TITLE;
        shape.insert_text(
            Point::new(MARGIN, y),
            &product.name,
            &text(TITLE, ink()),
        )?;
        y += 18.0;

        // A rule under the name. `finish` paints what has been drawn since the
        // last one, so the line has to be finished before any more text.
        shape.draw_line(
            Point::new(MARGIN, y),
            Point::new(PAGE.width - MARGIN, y),
        )?;
        shape.finish(&rule())?;
        y += FIELD_GAP;

        // ── The fields CORE §6 asks for ──────────────────────────────────
        let fields = [
            (Key::SerialLabel, product.serial.clone()),
            (Key::FieldPurchaseDate, data::fmt_date(product.purchase_date)),
            (Key::FieldWarrantyStart, data::fmt_date(product.warranty_start)),
            (Key::FieldWarrantyEnd, data::fmt_date(product.warranty_end)),
            (Key::FieldLink, product.link.clone()),
        ];
        for (label, value) in fields {
            shape.insert_text(
                Point::new(MARGIN, y),
                tr(label),
                &text(HEADING, quiet_ink()),
            )?;
            if !value.is_empty() {
                shape.insert_text(
                    Point::new(MARGIN, y + LABEL_GAP),
                    &value,
                    &text(BODY, ink()),
                )?;
            }
            y += FIELD_GAP + LABEL_GAP;
        }

        // ── The counter this app exists for ──────────────────────────────
        //
        // Through the same `days_left` and the same `countdown` column 3 uses, so
        // the number on the page and the number on screen cannot disagree — which
        // is why CORE §6 says "days left at time of export" rather than "days
        // left".
        y += 8.0;
        shape.insert_text(
            Point::new(MARGIN, y),
            tr(Key::WarrantyLeft),
            &text(HEADING, quiet_ink()),
        )?;
        let remaining = data::days_left(product.warranty_end, job.today);
        shape.insert_text(
            Point::new(MARGIN, y + COUNTER),
            &crate::details::countdown(remaining, job.lang),
            &text(COUNTER, ink()),
        )?;

        // ── What could not be included ───────────────────────────────────
        //
        // Recorded on the page rather than only in the window: a notice in column
        // 3 is gone when the app closes, and this file is what gets emailed to a
        // shop six months later. The same principle that keeps broken folders
        // visible in the list with a readable reason.
        if !skipped.is_empty() {
            let mut note = MARGIN + PAGE.height * 0.62;
            shape.insert_text(
                Point::new(MARGIN, note),
                tr(Key::ExportSkipped),
                &text(HEADING, quiet_ink()),
            )?;
            for (name, reason) in skipped {
                note += LABEL_GAP;
                shape.insert_text(
                    Point::new(MARGIN, note),
                    &format!("{name} — {}", crate::viewer::describe(job.lang, reason)),
                    &text(FOOTNOTE + 1.0, ink()),
                )?;
            }
        }

        // ── Footer ───────────────────────────────────────────────────────
        let footer = PAGE.height - MARGIN;
        shape.draw_line(
            Point::new(MARGIN, footer - 14.0),
            Point::new(PAGE.width - MARGIN, footer - 14.0),
        )?;
        shape.finish(&rule())?;
        shape.insert_text(
            Point::new(MARGIN, footer),
            &format!("{} {}", tr(Key::ExportedOn), data::fmt_date(job.today)),
            &text(FOOTNOTE, quiet_ink()),
        )?;

        shape.commit(doc, true)?;
    }

    Ok(())
}

/// One text style.
///
/// `simple: false` is the whole point and is not a tuning knob — see the module
/// header. Everything else here is typography.
fn text(size: f32, colour: PdfColor) -> TextOptions<'static> {
    TextOptions {
        fontsize: size,
        fill: Some(colour),
        simple: false,
        ..Default::default()
    }
}

/// A hairline rule, stroked rather than filled.
fn rule() -> mupdf::shape::FinishOptions {
    mupdf::shape::FinishOptions {
        color: Some(PdfColor::gray(0.72)),
        width: 0.6,
        close_path: false,
        ..Default::default()
    }
}

/// `Parachron-<product name>-<DD-MM-YYYY>.pdf` (CORE §6).
///
/// Sanitised, not slugged. `data::folder_slug` exists and is the wrong tool here:
/// it lowercases and folds to ASCII, so `Şarj Cihazı` would be suggested as
/// `sarj-cihazi` — correct for a directory that has to survive being copied onto
/// Windows, and a downgrade for a filename somebody is about to read in a save
/// dialog. This keeps the name as written and removes only what makes a filename
/// invalid, which is what `import::destination_name` already does for picked
/// files. It is a suggestion in any case: the dialog lets the user type whatever
/// they like, and the app writes where it is told.
pub fn suggested_name(product: &Product, today: Date) -> String {
    let cleaned: String = product
        .name
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':'))
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();

    if cleaned.is_empty() {
        format!("Parachron-{}.pdf", data::fmt_date(today))
    } else {
        format!("Parachron-{cleaned}-{}.pdf", data::fmt_date(today))
    }
}

/// Ask where to write. `done` runs on the UI thread with the chosen path, or is
/// not called at all if the dialog was cancelled.
///
/// A thin edge, exactly as `import::pick` is one, and for the reason Chron3 wrote
/// down: a portal dialog is drawn by the desktop's own portal service in the
/// user's session, so it appears on the real display whatever `DISPLAY` says and
/// cannot be driven under `Xvfb`. Everything past it takes a `PathBuf`, so the
/// whole of `run` is testable by handing it a path and only the click that opens
/// the dialog needs a person. The blocking `FileDialog` is not an option either —
/// it parks the calling thread inside a D-Bus read with no timeout.
/// `done` receives `None` when the dialog was cancelled, and is called either way.
/// That matters rather than being tidy: the caller marks itself busy *before* the
/// dialog opens, so a cancellation has to be observable or the button stays dead
/// for the rest of the session.
pub fn pick_destination(
    window: &slint::Window,
    title: &str,
    filter: &str,
    suggestion: &str,
    done: impl FnOnce(Option<PathBuf>) + 'static,
) {
    let handle = window.window_handle();
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter, &["pdf"])
        .set_file_name(suggestion)
        .set_parent(&handle);

    let _ = slint::spawn_local(async move {
        let chosen = dialog.save_file().await;
        done(chosen.map(|file| file.path().to_path_buf()));
    });
}

/// Render an [`Outcome`]'s failure through the string table.
pub fn describe(lang: Lang, error: &DataError) -> String {
    format!(
        "{}: {}",
        strings::get(lang, Key::ErrExportFailed),
        crate::vault::describe(lang, error)
    )
}

/// What the export needs kept between a click and a thread landing.
struct State {
    products_root: PathBuf,
    lang: Lang,
    /// Read once at startup, while the process was still single-threaded.
    offset: UtcOffset,
    /// An export is running. A second click while one is in flight is ignored
    /// rather than queued: two threads writing two files the user asked for once
    /// is not what they meant.
    busy: bool,
    /// Where a finished export leaves its result. The export runs on a thread and
    /// this module is full of `Rc`s, so the outcome travels here rather than in a
    /// closure — the same reason `import.rs` does it this way.
    slot: Arc<Mutex<Option<Outcome>>>,
}

/// What the language switch reaches the export through.
pub struct Exports {
    state: Rc<RefCell<State>>,
}

impl Exports {
    /// Chron6's switch calls this. The status line is cleared rather than
    /// re-composed: it is a transient sentence about something that has already
    /// happened, and re-translating "Saved" after the fact would be pretending the
    /// export happened in the new language.
    pub fn set_lang(&self, app: &AppWindow, lang: Lang) {
        self.state.borrow_mut().lang = lang;
        status(app, String::new(), false);
    }
}

fn status(app: &AppWindow, text: String, failed: bool) {
    app.set_export_status(text.as_str().into());
    app.set_export_failed(failed);
}

/// Wire EXPORT into the window.
pub fn install(
    app: &AppWindow,
    products_root: PathBuf,
    lang: Lang,
    offset: UtcOffset,
    vault: Rc<RefCell<Vault>>,
) -> Exports {
    let state = Rc::new(RefCell::new(State {
        products_root,
        lang,
        offset,
        busy: false,
        slot: Arc::new(Mutex::new(None)),
    }));

    app.on_export({
        let state = Rc::clone(&state);
        let vault = Rc::clone(&vault);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };

            // Read the product from the vault rather than off the window, so what
            // gets exported is the product's own data and not whatever the UI
            // happens to be showing. Taken by value here, at the moment EXPORT was
            // pressed: the window stays live while the dialog is open, so the
            // selection can change underneath it, and the product the user pressed
            // the button on is the one they meant.
            //
            // Read before the borrow below, because `selected_product` borrows the
            // vault and holding two borrows across a dialog is how a re-entrant
            // callback becomes a panic.
            let Some(product) = crate::vault::selected_product(&vault) else {
                return;
            };

            // Busy is claimed *before* the dialog opens, not when it closes.
            //
            // The event loop keeps running while a portal dialog is up — that is
            // the whole point of `spawn_local` — so the window stays interactive
            // and EXPORT can be pressed again. Claiming the flag on the way out of
            // the dialog left a window in which a second click passed the check and
            // opened a second save dialog, whose chosen path would then have been
            // silently dropped by the `busy` check in its own callback. One dialog
            // at a time, and the cancel path below is what gives the flag back.
            //
            // Everything decided under the borrow; nothing touches the window or
            // opens a dialog until it has been dropped.
            let (lang, today, root) = {
                let mut state = state.borrow_mut();
                if state.busy {
                    return;
                }
                state.busy = true;
                (
                    state.lang,
                    data::today(state.offset),
                    state.products_root.clone(),
                )
            };

            let suggestion = suggested_name(&product, today);
            let state = Rc::clone(&state);
            let inner = app.as_weak();
            pick_destination(
                app.window(),
                strings::get(lang, Key::ExportSaveTitle),
                strings::get(lang, Key::FilterPdf),
                &suggestion,
                move |destination| {
                    let Some(app) = inner.upgrade() else { return };

                    let Some(destination) = destination else {
                        // Cancelled. Nothing is written and nothing is said — the
                        // user withdrew the request, which is not an outcome to
                        // report — but the flag has to come back.
                        state.borrow_mut().busy = false;
                        return;
                    };

                    let (job, slot) = {
                        let state = state.borrow();
                        (
                            Job {
                                folder: root.join(&product.folder),
                                product,
                                destination,
                                today,
                                lang: state.lang,
                            },
                            Arc::clone(&state.slot),
                        )
                    };

                    status(&app, strings::get(lang, Key::Exporting).to_string(), false);
                    commit(job, slot, app.as_weak());
                },
            );
        }
    });

    app.on_export_finished({
        let state = Rc::clone(&state);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };

            let (outcome, lang) = {
                let mut state = state.borrow_mut();
                state.busy = false;
                let outcome = state.slot.lock().ok().and_then(|mut slot| slot.take());
                (outcome, state.lang)
            };
            let Some(outcome) = outcome else { return };

            match outcome {
                // Nothing is invalidated here, on purpose. The output went to a
                // path outside the vault and the product's own files were only
                // read, so the render worker's cache is still correct.
                Outcome::Done { skipped, .. } if skipped.is_empty() => {
                    status(&app, strings::get(lang, Key::ExportDone).to_string(), false);
                }
                Outcome::Done { skipped, .. } => {
                    // The full reason for each is on the summary page; column 3 has
                    // room for the names and elides the rest.
                    let names: Vec<&str> =
                        skipped.iter().map(|(name, _)| name.as_str()).collect();
                    status(
                        &app,
                        format!(
                            "{} — {}: {}",
                            strings::get(lang, Key::ExportDone),
                            strings::get(lang, Key::ExportSkipped),
                            names.join(", ")
                        ),
                        false,
                    );
                }
                Outcome::Failed(reason) => status(&app, describe(lang, &reason), true),
            }
        }
    });

    Exports { state }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap()
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    /// A product whose name and serial are Turkish, because that is the case the
    /// spike showed a default font silently loses.
    fn product(pdfs: &[&str]) -> Product {
        Product {
            folder: "sarj-cihazi".to_string(),
            name: "Şarj Cihazı".to_string(),
            serial: "İST-0042-ĞŞ".to_string(),
            link: "https://store.example/p".to_string(),
            purchase_date: day(2026, Month::March, 14),
            warranty_start: day(2026, Month::March, 14),
            warranty_end: day(2029, Month::March, 14),
            pdfs: pdfs.iter().map(|s| s.to_string()).collect(),
            added: day(2026, Month::August, 5),
            missing_pdfs: Vec::new(),
            extra: Default::default(),
        }
    }

    /// A product folder on disk, holding copies of the named fixtures.
    fn vault(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("sarj-cihazi");
        fs::create_dir_all(&folder).unwrap();
        for (name, from) in files {
            fs::copy(fixture(from), folder.join(name)).unwrap();
        }
        (dir, folder)
    }

    fn export(folder: &Path, product: Product, out: &Path, lang: Lang) -> Outcome {
        run(Job {
            folder: folder.to_path_buf(),
            product,
            destination: out.to_path_buf(),
            today: day(2026, Month::August, 6),
            lang,
        })
    }

    /// Every string on page one of the written file, so a test can ask what the
    /// summary actually says rather than that it wrote something.
    fn page_one(path: &Path) -> PdfDocument {
        render::open_pdf(path).expect("the export must be a readable PDF")
    }

    fn finds(doc: &PdfDocument, needle: &str) -> usize {
        doc.load_page(0)
            .expect("page one")
            .search(needle, 8)
            .map(|hits| hits.len())
            .unwrap_or(0)
    }

    #[test]
    fn an_export_is_the_summary_page_then_every_document_in_tab_order() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf"), ("warranty.pdf", "multipage.pdf")]);
        let out = dir.path().join("out.pdf");

        let outcome = export(
            &folder,
            product(&["invoice.pdf", "warranty.pdf"]),
            &out,
            Lang::En,
        );
        let Outcome::Done { skipped } = &outcome else {
            panic!("a valid export must succeed: {outcome:?}");
        };
        assert!(skipped.is_empty());
        assert!(out.is_file(), "the export wrote where it was told");

        // 1 summary + 1 from sample + 3 from multipage.
        let doc = page_one(&out);
        assert_eq!(doc.page_count().unwrap(), 5);
    }

    /// CORE §6's list, in full.
    #[test]
    fn the_summary_page_carries_every_field_core_asks_for() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        export(&folder, product(&["invoice.pdf"]), &out, Lang::En);

        let doc = page_one(&out);
        for needle in [
            "Şarj Cihazı",                 // name
            "İST-0042-ĞŞ",                 // serial
            "14-03-2026",                  // purchase date and warranty start
            "14-03-2029",                  // warranty end
            "https://store.example/p",     // purchase link
            "days",                        // days left, at time of export
            "PARACHRON",
        ] {
            assert!(finds(&doc, needle) > 0, "the summary page never says {needle:?}");
        }
    }

    /// The whole reason the font is registered as composite. A default Latin
    /// encoding drops these glyphs and raises nothing, so the check has to be
    /// "can the text be found again", never "did it write without erroring".
    #[test]
    fn turkish_letters_survive_with_the_app_in_english() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        export(&folder, product(&["invoice.pdf"]), &out, Lang::En);

        let doc = page_one(&out);
        // Each of the four letters a Latin encoding cannot carry, in a real word.
        for needle in ["Şarj", "Cihazı", "İST", "ĞŞ"] {
            assert!(
                finds(&doc, needle) > 0,
                "{needle:?} was dropped — the font is registered as simple again"
            );
        }
        // And `Ürün` is the near-miss: U-umlaut *is* in Latin-1, so a check on a
        // word like this one passes even when the encoding is wrong. Pinned so
        // nobody trusts it as the test.
        let mut turkish = product(&["invoice.pdf"]);
        turkish.name = "Ürün".to_string();
        let near = dir.path().join("near.pdf");
        export(&folder, turkish, &near, Lang::En);
        assert!(finds(&page_one(&near), "Ürün") > 0);
    }

    #[test]
    fn a_turkish_session_gets_a_turkish_summary() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        export(&folder, product(&["invoice.pdf"]), &out, Lang::Tr);

        let doc = page_one(&out);
        assert!(finds(&doc, strings::get(Lang::Tr, Key::SerialLabel)) > 0);
        assert!(finds(&doc, strings::get(Lang::Tr, Key::WarrantyLeft)) > 0);
        assert!(
            finds(&doc, strings::get(Lang::Tr, Key::DaysUnit)) > 0,
            "the countdown's unit is Turkish"
        );
        assert_eq!(
            finds(&doc, strings::get(Lang::En, Key::WarrantyLeft)),
            0,
            "and the English label is not also on the page"
        );
    }

    #[test]
    fn the_counter_is_the_one_column_three_showed() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        let today = day(2026, Month::August, 6);
        export(&folder, product(&["invoice.pdf"]), &out, Lang::En);

        let expected = crate::details::countdown(
            data::days_left(day(2029, Month::March, 14), today),
            Lang::En,
        );
        assert_eq!(expected, "951 days", "checked against the calendar by hand");
        assert!(finds(&page_one(&out), &expected) > 0);
    }

    #[test]
    fn an_expired_warranty_reads_as_expired_never_as_a_negative() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        let mut expired = product(&["invoice.pdf"]);
        expired.warranty_end = day(2025, Month::January, 1);
        export(&folder, expired, &out, Lang::En);

        let doc = page_one(&out);
        assert!(finds(&doc, strings::get(Lang::En, Key::WarrantyExpired)) > 0);
        assert_eq!(finds(&doc, "-583"), 0, "never a negative count");
    }

    /// CORE §6 says the output covers the product. A file that cannot be opened
    /// cannot be appended, so it is skipped — and named on the page, because the
    /// exported file has to carry its own gaps.
    #[test]
    fn a_document_that_cannot_be_included_is_skipped_and_named_on_the_page() {
        let (dir, folder) = vault(&[
            ("invoice.pdf", "sample.pdf"),
            ("locked.pdf", "encrypted.pdf"),
            ("junk.pdf", "corrupt.pdf"),
        ]);
        let out = dir.path().join("out.pdf");

        // `gone.pdf` is listed in the manifest and is not on disk at all.
        let outcome = export(
            &folder,
            product(&["invoice.pdf", "locked.pdf", "junk.pdf", "gone.pdf"]),
            &out,
            Lang::En,
        );
        let Outcome::Done { skipped } = &outcome else {
            panic!("one bad file must not fail the export: {outcome:?}");
        };

        let names: Vec<&str> = skipped.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["locked.pdf", "junk.pdf", "gone.pdf"]);
        assert!(matches!(skipped[0].1, ViewError::Encrypted));
        assert!(matches!(skipped[2].1, ViewError::Missing));

        // Summary plus the one good document.
        let doc = page_one(&out);
        assert_eq!(doc.page_count().unwrap(), 2);
        assert!(finds(&doc, strings::get(Lang::En, Key::ExportSkipped)) > 0);
        for name in names {
            assert!(finds(&doc, name) > 0, "{name} is not named on the page");
        }
    }

    #[test]
    fn a_product_with_no_documents_exports_just_the_summary() {
        let (dir, folder) = vault(&[]);
        let out = dir.path().join("out.pdf");
        let outcome = export(&folder, product(&[]), &out, Lang::En);

        assert!(matches!(outcome, Outcome::Done { .. }), "{outcome:?}");
        let doc = page_one(&out);
        assert_eq!(doc.page_count().unwrap(), 1);
        assert!(finds(&doc, "Şarj Cihazı") > 0);
    }

    #[test]
    fn the_summary_page_is_a4_and_appended_pages_keep_their_own_size() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        export(&folder, product(&["invoice.pdf"]), &out, Lang::En);

        let doc = page_one(&out);
        let summary = doc.load_page(0).unwrap().bounds().unwrap();
        assert!((summary.width() - PAGE.width).abs() < 1.0);
        assert!((summary.height() - PAGE.height).abs() < 1.0);
        // The appended page is whatever it already was, not rescaled to match.
        let appended = doc.load_page(1).unwrap().bounds().unwrap();
        let original = render::open_pdf(&folder.join("invoice.pdf")).unwrap();
        let was = original.load_page(0).unwrap().bounds().unwrap();
        assert!((appended.width() - was.width()).abs() < 1.0);
        assert!((appended.height() - was.height()).abs() < 1.0);
    }

    /// The layout is hand-placed arithmetic, and the failure mode of hand-placed
    /// arithmetic is text drawn off the page — which every other test here would
    /// pass, because `search` finds text in a content stream whether or not it is
    /// inside the media box. So the page is rasterized and looked at: mostly paper,
    /// with ink in the top third where the wordmark and name are, in the middle
    /// where the fields are, and in the bottom sixth where the footer is.
    #[test]
    fn the_summary_page_puts_its_ink_on_the_page() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        export(&folder, product(&["invoice.pdf"]), &out, Lang::En);

        let doc = page_one(&out);
        let raster = render::rasterize(&doc, 0, 620, 877).expect("the summary must render");

        let dark_rows: Vec<usize> = (0..raster.height as usize)
            .filter(|y| {
                let start = y * raster.width as usize * 4;
                raster.rgba[start..start + raster.width as usize * 4]
                    .chunks_exact(4)
                    .any(|px| px[0] < 140)
            })
            .collect();

        assert!(!dark_rows.is_empty(), "the page is blank — nothing landed on it");

        let height = raster.height as usize;
        let first = dark_rows[0];
        let last = *dark_rows.last().unwrap();
        assert!(
            first < height / 3,
            "nothing is drawn in the top third: first ink at row {first} of {height}"
        );
        assert!(
            last > height * 5 / 6,
            "the footer is off the page: last ink at row {last} of {height}"
        );
        assert!(
            dark_rows.iter().any(|y| (height / 3..height * 2 / 3).contains(y)),
            "the middle of the page is empty, so the fields are not where they should be"
        );

        // A summary page is text on paper, so it is overwhelmingly white — a page
        // that is half dark means something is filling an area it should not.
        let white = raster.rgba.chunks_exact(4).filter(|px| px[0] > 200).count();
        let total = raster.rgba.len() / 4;
        assert!(
            white * 10 > total * 9,
            "only {}% of the page is paper",
            white * 100 / total
        );
    }

    #[test]
    fn a_destination_that_cannot_be_written_is_reported_not_a_panic() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();

        let outcome = export(&folder, product(&["invoice.pdf"]), &blocked, Lang::En);
        assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
        for lang in [Lang::En, Lang::Tr] {
            let Outcome::Failed(reason) = &outcome else { unreachable!() };
            assert!(!describe(lang, reason).is_empty());
        }
    }

    /// `write_to` is used rather than `save`, which takes a `&str` — so a path
    /// that is not valid UTF-8 has to work.
    #[cfg(unix)]
    #[test]
    fn a_destination_path_that_is_not_utf8_still_writes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        // A lone 0xFF byte is valid in a Unix filename and not valid UTF-8.
        let mut name = OsString::from_vec(vec![b'o', b'u', b't', 0xff]);
        name.push(".pdf");
        let out = dir.path().join(name);
        assert!(out.to_str().is_none(), "the point of this test");

        let outcome = export(&folder, product(&["invoice.pdf"]), &out, Lang::En);
        assert!(matches!(outcome, Outcome::Done { .. }), "{outcome:?}");
        assert!(out.is_file());
    }

    #[test]
    fn the_suggested_filename_keeps_the_name_readable_and_writable() {
        let today = day(2026, Month::August, 6);
        assert_eq!(
            suggested_name(&product(&[]), today),
            "Parachron-Şarj Cihazı-06-08-2026.pdf",
            "the name is kept as written, not slugged to ASCII"
        );

        // Anything that would make the suggestion unwritable comes out.
        let mut awkward = product(&[]);
        awkward.name = "Monitor / 27\" \u{7}".to_string();
        let name = suggested_name(&awkward, today);
        assert!(!name.contains('/'), "{name}");
        assert!(!name.chars().any(char::is_control), "{name}");

        // A name with nothing usable in it still gives a writable suggestion.
        let mut empty = product(&[]);
        empty.name = "...".to_string();
        assert_eq!(suggested_name(&empty, today), "Parachron-06-08-2026.pdf");
    }

    /// `PdfDocument::open` returns `Ok` for an encrypted file and only then admits
    /// it needs a password, which is why `render::open_pdf` exists. Pinned here
    /// because it is surprising and because losing the check would put an
    /// undecryptable page into somebody's export.
    #[test]
    fn the_pdf_open_used_by_export_refuses_what_the_viewer_refuses() {
        assert!(matches!(
            render::open_pdf(&fixture("encrypted.pdf")),
            Err(ViewError::Encrypted)
        ));
        assert!(matches!(
            render::open_pdf(&fixture("corrupt.pdf")),
            Err(ViewError::NotAPdf(_))
        ));
        assert!(matches!(
            render::open_pdf(&fixture("zero-page.pdf")),
            Err(ViewError::NoPages)
        ));
        assert!(matches!(
            render::open_pdf(&fixture("no-such-file.pdf")),
            Err(ViewError::Missing)
        ));
        assert!(render::open_pdf(&fixture("sample.pdf")).is_ok());
    }
}
