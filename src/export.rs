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
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use mupdf::pdf::PdfDocument;
use mupdf::shape::{PdfColor, Shape, TextOptions, TextboxOptions};
use mupdf::{Point, Rect, Size};
use slint::ComponentHandle;
use time::{Date, UtcOffset};

use crate::AppWindow;
use crate::data::{self, Product};
use crate::render::{self, ViewError};
use crate::strings::{self, Key, Lang};
use crate::vault::Vault;

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

/// Space between a label and the value under it, and between one field and the
/// next. Everything else about vertical position is whatever the previous block
/// actually consumed, because a wrapped value's height is not known until it is
/// drawn.
const LABEL_GAP: f32 = 4.0;
const FIELD_GAP: f32 = 14.0;

/// The longest filename a filesystem will accept, in bytes. 255 on Linux, on
/// Windows, and on macOS; the suggestion is capped to it rather than to a
/// character count, because a Turkish letter is two bytes.
const NAME_MAX: usize = 255;

/// Ink. Black on white, theme-independent by construction (CORE §6) — the export
/// reads nothing from `Palette`, because a printed page is not a window.
fn ink() -> PdfColor {
    PdfColor::gray(0.0)
}

fn quiet_ink() -> PdfColor {
    PdfColor::gray(0.42)
}

/// Why an export did not happen.
///
/// Its own type rather than a reused [`DataError`], because export's failures are
/// not data-layer failures and saying they are misinforms the user about their own
/// files. Every MuPDF error used to be mapped onto `DataError::Malformed`, which
/// the string table renders as **"product.toml is not valid"** — so an export onto
/// a full disk told somebody to go and fix a manifest that was perfectly fine.
/// `File::create` failing became `Unreadable`, "Could not be read", for a write.
#[derive(Debug)]
pub enum Failure {
    /// The output could not be written; carries the OS message.
    Write(String),
    /// MuPDF could not build or serialise the document.
    Assemble(String),
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
    Failed(Failure),
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
    let assemble = |e: mupdf::Error| Failure::Assemble(e.to_string());

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
        return Outcome::Failed(assemble(e));
    }

    for source in &sources {
        if let Err(e) = out.insert_pdf(source, Default::default()) {
            return Outcome::Failed(assemble(e));
        }
    }

    // `write_to` rather than `save`, which takes a `&str` and so cannot express a
    // destination that is not valid UTF-8 — and on Linux a path is bytes, so that
    // is a real file somebody could have picked rather than a hypothetical.
    //
    // Written to a temporary file beside the destination, synced, then renamed
    // over it — the same shape `data::write_atomic` already gives every product
    // manifest, and for the same reason: `File::create` on the destination
    // directly truncates it before a single byte of the new file exists, so a
    // write that fails partway (a full disk, the case the comment above already
    // discusses) or a crash mid-write would destroy whatever was there rather
    // than simply fail to replace it. The destination here is very often the
    // same file as a previous export, under the deterministic `suggested_name`
    // pattern CORE §6 gives it, so "was there" is routinely a good export the
    // user still wants.
    //
    // Every failure here is `Write`, matching what this returned before: the
    // temporary could not be created, the bytes could not be got into it, or
    // the finished file could not be put where it was asked to go.
    let dir = job.destination.parent().unwrap_or_else(|| Path::new("."));
    let name = job.destination.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.tmp"));

    let mut file = match File::create(&tmp) {
        Ok(file) => file,
        Err(e) => return Outcome::Failed(Failure::Write(e.to_string())),
    };
    if let Err(e) = out.write_to(&mut file) {
        let _ = fs::remove_file(&tmp);
        return Outcome::Failed(Failure::Write(e.to_string()));
    }
    if let Err(e) = file.sync_all() {
        let _ = fs::remove_file(&tmp);
        return Outcome::Failed(Failure::Write(e.to_string()));
    }
    drop(file);

    match fs::rename(&tmp, &job.destination) {
        Ok(()) => Outcome::Done { skipped },
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Outcome::Failed(Failure::Write(e.to_string()))
        }
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
        let footer = PAGE.height - MARGIN;
        let footer_rule = footer - 14.0;

        // Labels come from the string table and are short and known. Values are
        // user data of unbounded length, so every one of them is wrapped — see
        // `wrapped` for what happened before they were.
        let mut y = MARGIN;

        // ── Wordmark and product name ────────────────────────────────────
        shape.insert_text(
            Point::new(MARGIN, y + FOOTNOTE),
            tr(Key::AppTitle),
            &text(FOOTNOTE, quiet_ink()),
        )?;
        y += FOOTNOTE + 10.0;
        y = wrapped(&mut shape, y, &product.name, TITLE, ink(), 2)?;
        y += 10.0;

        // A rule under the name. `finish` paints what has been drawn since the
        // last one, so the line has to be finished before any more text.
        shape.draw_line(Point::new(MARGIN, y), Point::new(PAGE.width - MARGIN, y))?;
        shape.finish(&rule())?;
        y += 22.0;

        // ── The fields CORE §6 asks for ──────────────────────────────────
        let fields = [
            (Key::SerialLabel, product.serial.clone()),
            (
                Key::FieldPurchaseDate,
                data::fmt_date(product.purchase_date),
            ),
            (
                Key::FieldWarrantyStart,
                data::fmt_date(product.warranty_start),
            ),
            (Key::FieldWarrantyEnd, data::fmt_date(product.warranty_end)),
            (Key::FieldLink, product.link.clone()),
        ];
        for (label, value) in fields {
            shape.insert_text(
                Point::new(MARGIN, y + HEADING),
                tr(label),
                &text(HEADING, quiet_ink()),
            )?;
            y += HEADING + LABEL_GAP;
            y = wrapped(&mut shape, y, &value, BODY, ink(), 2)?;
            y += FIELD_GAP;
        }

        // ── The counter this app exists for ──────────────────────────────
        //
        // Through the same `days_left` and the same `countdown` column 3 uses, so
        // the number on the page and the number on screen cannot disagree — which
        // is why CORE §6 says "days left at time of export" rather than "days
        // left".
        y += 6.0;
        shape.insert_text(
            Point::new(MARGIN, y + HEADING),
            tr(Key::WarrantyLeft),
            &text(HEADING, quiet_ink()),
        )?;
        y += HEADING + LABEL_GAP;
        let remaining = data::days_left(product.warranty_end, job.today);
        y = wrapped(
            &mut shape,
            y,
            &crate::details::countdown(remaining, job.lang),
            COUNTER,
            ink(),
            1,
        )?;

        // ── What could not be included ───────────────────────────────────
        //
        // Recorded on the page rather than only in the window: a notice in column
        // 3 is gone when the app closes, and this file is what gets emailed to a
        // shop six months later. The same principle that keeps broken folders
        // visible in the list with a readable reason.
        //
        // Placed below whatever the fields actually needed rather than at a fixed
        // fraction of the page, so a wrapped name or link pushes it down instead of
        // being written over by it. And it stops at the footer rule: a product with
        // thirteen unreadable documents used to run its list off the bottom of the
        // paper, which is the same class of defect as the overflowing link.
        if !skipped.is_empty() {
            y += 30.0;
            shape.insert_text(
                Point::new(MARGIN, y + HEADING),
                tr(Key::ExportSkipped),
                &text(HEADING, quiet_ink()),
            )?;
            y += HEADING + LABEL_GAP;

            let line = (FOOTNOTE + 1.0) * 1.2;
            let mut listed = 0;
            for (name, reason) in skipped {
                if y + line > footer_rule - 4.0 {
                    break;
                }
                y = wrapped(
                    &mut shape,
                    y,
                    &format!("{name} — {}", crate::viewer::describe(job.lang, reason)),
                    FOOTNOTE + 1.0,
                    ink(),
                    2,
                )?;
                listed += 1;
            }

            // A list that had to stop says so, rather than quietly being shorter
            // than the truth — the artefact has to be honest about its own gaps.
            if listed < skipped.len() && y + line <= footer_rule - 4.0 {
                wrapped(
                    &mut shape,
                    y,
                    &format!("+{}", skipped.len() - listed),
                    FOOTNOTE + 1.0,
                    quiet_ink(),
                    1,
                )?;
            }
        }

        // ── Footer ───────────────────────────────────────────────────────
        shape.draw_line(
            Point::new(MARGIN, footer_rule),
            Point::new(PAGE.width - MARGIN, footer_rule),
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

/// Draw `body` inside the page's margins, wrapping, and return the `y` after it.
///
/// **Every piece of user data on the page goes through this**, and the reason is
/// that `insert_text` does not wrap and does not clip: it lays a single line out
/// from the point given and keeps going past the paper's edge. A product called
/// `Samsung Odyssey OLED G8 34-inch Ultrawide Curved Gaming Monitor` with an
/// ordinary store URL under it put ink in column 594 of a 595-point page — the
/// right margin is at 539 — so the end of the name and most of the link were
/// simply off the sheet.
///
/// Nothing caught it. Every test here searches the written file for its text, and
/// `search` reads the content stream rather than the visible area, so text drawn
/// past the media box is still found. The test that rasterizes the page only
/// checked where the ink was *vertically*. It took measuring the rightmost dark
/// column to see it.
///
/// `max_lines` bounds the space a value may claim so one long field cannot push
/// the rest of the page off the bottom. Two lines is about 160 characters at body
/// size, which covers any real link; a value longer than its allowance is clipped
/// by the box rather than escaping it, which is the failure mode worth having.
fn wrapped(
    shape: &mut Shape,
    y: f32,
    body: &str,
    size: f32,
    colour: PdfColor,
    max_lines: u32,
) -> Result<f32, mupdf::Error> {
    if body.is_empty() {
        return Ok(y);
    }

    let line = size * 1.2;
    // Half a size of headroom, because a box exactly `max_lines` tall draws
    // **nothing at all**. `insert_textbox` places a line only if the whole line
    // box fits, and at the descender it does not quite: a one-line box at the
    // counter's 19pt silently produced an empty page region, which the search
    // tests caught only because they were looking for that exact string.
    let allowance = line * max_lines as f32 + size * 0.5;
    let unused = shape.insert_textbox(
        Rect::new(MARGIN, y, PAGE.width - MARGIN, y + allowance),
        body,
        &TextboxOptions {
            fontsize: size,
            fill: Some(colour),
            simple: false,
            ..Default::default()
        },
    )?;

    // `insert_textbox` reports the height it did not use, and reports it negative
    // when the text wanted more room than it was given. Either way what advances
    // `y` is what was actually consumed — clamped to at least one line, so a value
    // can never take up nothing and be written over by the next field.
    let used = (allowance - unused.max(0.0)).clamp(line, allowance);
    Ok(y + used)
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

    let date = data::fmt_date(today);
    if cleaned.is_empty() {
        return format!("Parachron-{date}.pdf");
    }

    // Bounded in **bytes**, not characters, because `NAME_MAX` is 255 bytes on
    // Linux and Turkish letters are two bytes each — so a name that looks well
    // inside the limit can be over it. Without this, a pasted marketplace product
    // title produced a suggestion the dialog could offer and the filesystem would
    // refuse, and the user got an ENAMETOOLONG they had no way to interpret.
    let room = NAME_MAX.saturating_sub(b"Parachron--.pdf".len() + date.len());
    let mut name = cleaned;
    if name.len() > room {
        // Never mid-character: truncating a two-byte letter in half would put
        // invalid UTF-8 into the suggestion.
        let mut end = room;
        while end > 0 && !name.is_char_boundary(end) {
            end -= 1;
        }
        name = name[..end].trim_end();
    }

    if name.is_empty() {
        format!("Parachron-{date}.pdf")
    } else {
        format!("Parachron-{name}-{date}.pdf")
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

/// Render a [`Failure`] through the string table.
///
/// The trailing detail is the OS's or MuPDF's own message and stays as it is, the
/// same way `vault::describe` and `viewer::describe` treat theirs.
pub fn describe(lang: Lang, failure: &Failure) -> String {
    let (key, detail) = match failure {
        Failure::Write(detail) => (Key::ErrExportWrite, detail),
        Failure::Assemble(detail) => (Key::ErrExportAssemble, detail),
    };
    format!(
        "{}: {}: {detail}",
        strings::get(lang, Key::ErrExportFailed),
        strings::get(lang, key)
    )
}

/// What the export needs kept between a click and a thread landing.
struct State {
    products_root: PathBuf,
    lang: Lang,
    /// Read once at startup, while the process was still single-threaded.
    offset: UtcOffset,
    /// The folder of the product being exported, when one is.
    ///
    /// `Some` *is* the busy flag — a second click while one is in flight is
    /// ignored rather than queued, because two threads writing two files the user
    /// asked for once is not what they meant. It carries the folder rather than
    /// just a `bool` so that when the export lands, its status can be withheld if
    /// the user has moved to a different product in the meantime: `Saved` above
    /// somebody else's details is a claim about the wrong thing.
    exporting: Option<String>,
    /// Where a finished export leaves its result. The export runs on a thread and
    /// this module is full of `Rc`s, so the outcome travels here rather than in a
    /// closure — the same reason `import.rs` does it this way.
    slot: Arc<Mutex<Option<Outcome>>>,
}

/// What the language switch reaches the export through.
/// `Clone` since Chron9, for the reason `Editors` gives: two callers now need a
/// handle to the same state.
#[derive(Clone)]
pub struct Exports {
    state: Rc<RefCell<State>>,
}

impl Exports {
    /// Chron9. Exports read the product's files from the new root.
    ///
    /// One of four owners of the products root — see
    /// `viewer::Viewer::set_products_root` for why they are copies.
    pub fn set_products_root(&self, root: PathBuf) {
        self.state.borrow_mut().products_root = root;
    }

    /// Chron6's switch calls this.
    ///
    /// A *finished* export's status is cleared rather than re-composed: it is a
    /// sentence about something that already happened, and re-translating "Saved"
    /// after the fact would pretend the export happened in the new language.
    ///
    /// A *running* one is the opposite case and used to be treated the same, which
    /// was wrong: switching language mid-export blanked "Exporting…" while the
    /// thread was still writing, leaving a live EXPORT button that did nothing
    /// (because `busy` was true) and an app that looked idle while it worked. What
    /// is happening now gets said again, in the new language.
    pub fn set_lang(&self, app: &AppWindow, lang: Lang) {
        self.state.borrow_mut().lang = lang;

        // `export-running` on the window is the single predicate for "an export is
        // in flight", shared with `clear_status`. Asking `State.exporting` here
        // instead would be a second source of truth for the same fact, and the two
        // would only have to disagree once — `State.exporting` exists to carry
        // *which* folder, for the identity check when the export lands.
        if app.get_export_running() {
            status(app, strings::get(lang, Key::Exporting).to_string(), false);
        } else {
            clear_status(app);
        }
    }
}

fn status(app: &AppWindow, text: String, failed: bool) {
    app.set_export_status(text.as_str().into());
    app.set_export_failed(failed);
}

/// Take the status line down — unless an export is running.
///
/// Called by `details::show` on every push as well as from here, and that split is
/// deliberate: `export.rs` is the only thing that ever *says* anything, and a
/// change of product is the only other thing that can *unsay* it. A status is a
/// claim about one product, so `Saved — Not included: gone.pdf` left over from
/// product A above product B's details is a claim about the wrong thing.
///
/// The guard is not a detail. `details::show` runs from every `vault::push`, and
/// `lang::switch` ends in one — so an unconditional clear here erased the line
/// `Exports::set_lang` had just re-said, and switching language mid-export still
/// blanked "Exporting…" and left a live button that silently did nothing. A sort
/// toggle or a form save during an export did the same. The status of work in
/// flight is not about whichever product happens to be selected, so it survives.
pub fn clear_status(app: &AppWindow) {
    if app.get_export_running() {
        return;
    }
    status(app, String::new(), false);
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
        exporting: None,
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
                if state.exporting.is_some() {
                    return;
                }
                state.exporting = Some(product.folder.clone());
                (
                    state.lang,
                    data::today(state.offset),
                    state.products_root.clone(),
                )
            };
            // On the window too, because `details::show` runs from every
            // `vault::push` and has to know not to clear a running export's line.
            app.set_export_running(true);

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
                        state.borrow_mut().exporting = None;
                        app.set_export_running(false);
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
        let vault = Rc::clone(&vault);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };

            let (outcome, lang, exported) = {
                let mut state = state.borrow_mut();
                let exported = state.exporting.take();
                let outcome = state.slot.lock().ok().and_then(|mut slot| slot.take());
                (outcome, state.lang, exported)
            };
            app.set_export_running(false);
            let Some(outcome) = outcome else { return };

            // An export's status is a claim about one product, and the window stayed
            // live while this ran — so if the user has moved on, the claim is
            // withheld rather than posted above somebody else's details. The
            // artefact is still written; only the line about it is dropped, because
            // there is nowhere honest to put it.
            if exported != vault.borrow().selected_folder() {
                clear_status(&app);
                return;
            }

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
                    let names: Vec<&str> = skipped.iter().map(|(name, _)| name.as_str()).collect();
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
        let (dir, folder) = vault(&[
            ("invoice.pdf", "sample.pdf"),
            ("warranty.pdf", "multipage.pdf"),
        ]);
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

        // The write goes through a temporary beside the destination; a
        // successful export must not leave it behind.
        let strays: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .filter(|name| name != "sarj-cihazi" && name != "out.pdf")
            .collect();
        assert!(strays.is_empty(), "temporary left behind: {strays:?}");
    }

    /// `File::create` on the destination directly would truncate whatever was
    /// there — including a good export from a previous run, under the same
    /// deterministic `suggested_name` a periodic re-export reuses — before a
    /// single byte of the new one existed. Writing to a temporary and renaming
    /// over the destination only on success means a write that cannot finish
    /// leaves the previous file exactly as it was. Forced here by pointing the
    /// destination at a path that cannot be renamed onto — an existing
    /// directory — rather than truncated: the failure at the final rename is
    /// the same one a full disk hits earlier, and either way the guarantee
    /// under test is "the thing that was already at `destination` survives".
    #[test]
    fn a_write_that_cannot_finish_leaves_whatever_was_at_the_destination_untouched() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        fs::create_dir(&out).unwrap();
        fs::write(out.join("marker"), b"the previous export's stand-in").unwrap();

        let outcome = export(&folder, product(&["invoice.pdf"]), &out, Lang::En);
        assert!(matches!(outcome, Outcome::Failed(Failure::Write(_))), "{outcome:?}");

        assert!(
            out.join("marker").is_file(),
            "the destination was replaced instead of left alone on failure"
        );

        let strays: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .filter(|name| name != "sarj-cihazi" && name != "out.pdf")
            .collect();
        assert!(strays.is_empty(), "temporary left behind: {strays:?}");
    }

    /// CORE §6's list, in full.
    #[test]
    fn the_summary_page_carries_every_field_core_asks_for() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");
        export(&folder, product(&["invoice.pdf"]), &out, Lang::En);

        let doc = page_one(&out);
        for needle in [
            "Şarj Cihazı",             // name
            "İST-0042-ĞŞ",             // serial
            "14-03-2026",              // purchase date and warranty start
            "14-03-2029",              // warranty end
            "https://store.example/p", // purchase link
            "days",                    // days left, at time of export
            "PARACHRON",
        ] {
            assert!(
                finds(&doc, needle) > 0,
                "the summary page never says {needle:?}"
            );
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

        assert!(
            !dark_rows.is_empty(),
            "the page is blank — nothing landed on it"
        );

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
            dark_rows
                .iter()
                .any(|y| (height / 3..height * 2 / 3).contains(y)),
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

    /// Long user data has to stay inside the margins.
    ///
    /// `insert_text` neither wraps nor clips, so before every value went through
    /// `wrapped` an ordinary product name and store URL put ink in column 594 of a
    /// 595-point page. No search-based test could see it: `search` reads the
    /// content stream, so text drawn past the media box is still found, and the
    /// rasterizing test only looked at the vertical extent. This one measures the
    /// leftmost and rightmost dark columns.
    #[test]
    fn long_values_wrap_instead_of_running_off_the_page() {
        let (dir, folder) = vault(&[("invoice.pdf", "sample.pdf")]);
        let out = dir.path().join("out.pdf");

        let mut p = product(&["invoice.pdf"]);
        p.name = "Samsung Odyssey OLED G8 34-inch Ultrawide Curved Gaming Monitor".to_string();
        p.link = "https://www.example-store.com/products/qd-oled-monitor-27-inch-\
4k-144hz?variant=884412&ref=email_campaign_2026_summer_sale"
            .to_string();
        p.serial = "SN-".to_string() + &"0123456789".repeat(12);
        export(&folder, p, &out, Lang::En);

        let doc = page_one(&out);
        // One pixel per point, so a column index *is* a page coordinate.
        let r = render::rasterize(&doc, 0, 595, 842).unwrap();
        let w = r.width as usize;

        let mut leftmost = w;
        let mut rightmost = 0usize;
        for y in 0..r.height as usize {
            for x in 0..w {
                if r.rgba[(y * w + x) * 4] < 140 {
                    leftmost = leftmost.min(x);
                    rightmost = rightmost.max(x);
                }
            }
        }

        let margin = MARGIN as usize;
        assert!(
            leftmost >= margin - 1,
            "ink at column {leftmost}, left margin is {margin}"
        );
        assert!(
            rightmost <= w - margin + 1,
            "ink at column {rightmost} of {w}: the right margin is at {}, so user data \
             is running off the paper",
            w - margin
        );

        // And wrapping must not have lost anything: the tail of each long value is
        // still in the file.
        assert!(
            finds(&doc, "Gaming Monitor") > 0,
            "the end of the name was dropped"
        );
        assert!(
            finds(&doc, "summer_sale") > 0,
            "the end of the link was dropped, which is data loss in the artefact"
        );
    }

    /// A product whose every document is unreadable must not run its list off the
    /// bottom of the page. The block used to sit at a fixed fraction of the page
    /// height and grow downwards without a stop.
    #[test]
    fn a_long_skipped_list_stops_at_the_footer() {
        let (dir, folder) = vault(&[]);
        let out = dir.path().join("out.pdf");

        // Twenty documents, none of them on disk.
        let names: Vec<String> = (1..=20).map(|n| format!("scan-{n:02}.pdf")).collect();
        let mut p = product(&[]);
        p.pdfs = names;
        let outcome = export(&folder, p, &out, Lang::En);

        let Outcome::Done { skipped } = &outcome else {
            panic!("all-unreadable must still export: {outcome:?}");
        };
        assert_eq!(skipped.len(), 20, "every one is reported to the caller");

        let doc = page_one(&out);
        let r = render::rasterize(&doc, 0, 595, 842).unwrap();
        let w = r.width as usize;
        let last_ink = (0..r.height as usize)
            .rev()
            .find(|y| {
                let start = y * w * 4;
                r.rgba[start..start + w * 4]
                    .chunks_exact(4)
                    .any(|px| px[0] < 140)
            })
            .expect("the page is not blank");

        // The footer's own text is the last thing on the page, and it sits at
        // `height - MARGIN`. Nothing may be drawn below it.
        assert!(
            last_ink <= (PAGE.height - MARGIN) as usize + 4,
            "ink at row {last_ink} of {}: something is drawn below the footer",
            r.height
        );
    }

    /// A failed export must not blame the user's own files.
    ///
    /// Every MuPDF error used to be mapped onto `DataError::Malformed`, which the
    /// string table renders as "product.toml is not valid" — so an export onto a
    /// full disk told somebody to go and repair a manifest that was fine. The old
    /// test asserted only that the message was non-empty, so it passed.
    #[test]
    fn a_failed_export_names_the_right_thing() {
        for lang in [Lang::En, Lang::Tr] {
            let manifest = strings::get(lang, Key::ErrMalformed);
            let unreadable = strings::get(lang, Key::ErrUnreadable);

            let write = describe(lang, &Failure::Write("No space left on device".into()));
            assert!(
                write.contains(strings::get(lang, Key::ErrExportWrite)),
                "{write}"
            );
            assert!(write.contains("No space left on device"), "{write}");
            assert!(
                !write.contains(manifest),
                "a write failure blamed the manifest: {write}"
            );
            assert!(
                !write.contains(unreadable),
                "a write failure was reported as a read failure: {write}"
            );

            let build = describe(lang, &Failure::Assemble("cycle in page tree".into()));
            assert!(
                build.contains(strings::get(lang, Key::ErrExportAssemble)),
                "{build}"
            );
            assert!(build.contains("cycle in page tree"), "{build}");
            assert!(!build.contains(manifest), "{build}");
        }
    }

    /// `NAME_MAX` is 255 *bytes*, and a Turkish letter is two of them — so a name
    /// that looks well inside the limit can be over it. Without the bound the
    /// dialog pre-filled a suggestion the filesystem would refuse.
    #[test]
    fn the_suggested_filename_cannot_exceed_what_a_filesystem_accepts() {
        let today = day(2026, Month::August, 6);

        for name in [
            "x".repeat(400),
            // Two bytes per letter, so 200 characters is 400 bytes.
            "ş".repeat(200),
            // A boundary case: the truncation point lands mid-character.
            "a".repeat(200) + &"ğ".repeat(50),
        ] {
            let mut p = product(&[]);
            p.name = name.clone();
            let suggestion = suggested_name(&p, today);

            assert!(
                suggestion.len() <= NAME_MAX,
                "{} bytes for a {}-byte name",
                suggestion.len(),
                name.len()
            );
            assert!(suggestion.starts_with("Parachron-"));
            assert!(suggestion.ends_with(".pdf"));
            // Truncating mid-character would have produced invalid UTF-8, which
            // `String` cannot even hold — so reaching here at all is the check.
            assert!(suggestion.chars().count() > 0);
        }

        // And a name that fits is untouched.
        assert_eq!(
            suggested_name(&product(&[]), today),
            "Parachron-Şarj Cihazı-06-08-2026.pdf"
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
            let Outcome::Failed(reason) = &outcome else {
                unreachable!()
            };
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
