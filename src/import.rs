//! Getting files from the user's disk into the vault, without blocking the UI.
//!
//! The picker is a thin edge: it hands back a `Vec<PathBuf>` and everything
//! after that is ordinary code that takes paths. That is not tidiness for its
//! own sake — a portal dialog is drawn by the desktop's own portal service on
//! the real session, so it cannot be driven inside the isolated display the
//! click tests run on. Keeping the dialog at the edge means the whole import
//! path is testable by handing it paths, and only the click that opens the
//! dialog needs a person.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::data::{self, DataError, Draft};
use crate::render::{self, ViewError};
use crate::AppWindow;

/// File name for a picked file whose own name is unusable.
const FILE_FALLBACK: &str = "document";

/// What a commit did, or why it did not.
#[derive(Debug)]
pub enum Outcome {
    Done {
        folder: String,
        /// Paths whose bytes have changed, for the render worker to forget.
        invalidate: Vec<PathBuf>,
    },
    /// A picked file was not a PDF Parachron can show. Nothing was written.
    Refused { file: String, reason: ViewError },
    Failed(DataError),
}

/// Everything a commit needs, in one package that can cross to a thread.
#[derive(Debug)]
pub struct Job {
    pub products_root: PathBuf,
    /// The folder being edited, or `None` when adding a new product.
    pub folder: Option<String>,
    pub draft: Draft,
    /// Files to bring in: where each one is now, and what to call it here.
    pub imports: Vec<(PathBuf, String)>,
    /// Copies to delete, by file name.
    pub removals: Vec<String>,
}

/// Ask for PDFs. `done` runs on the UI thread with whatever was chosen, which
/// is an empty list if the dialog was cancelled.
pub fn pick(
    window: &slint::Window,
    title: &str,
    filter: &str,
    done: impl FnOnce(Vec<PathBuf>) + 'static,
) {
    let handle = window.window_handle();
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter, &["pdf"])
        .set_parent(&handle);

    // `AsyncFileDialog` on the portal backend already does exactly the right
    // thing: it runs the blocking D-Bus call on a thread of its own and wakes a
    // waker when the answer comes back, which is what `spawn_local` polls on
    // the event loop. The blocking `FileDialog` would park this thread inside a
    // D-Bus read that has no timeout — a window frozen for as long as the
    // dialog is open, and permanently if the portal never answers.
    let _ = slint::spawn_local(async move {
        let files = dialog.pick_files().await.unwrap_or_default();
        done(files.into_iter().map(|f| f.path().to_path_buf()).collect());
    });
}

/// Run a commit on a thread and ring the window when it lands.
///
/// The outcome travels in `slot` rather than in the closure because the editor
/// it belongs to is full of `Rc`s and cannot cross a thread boundary. This only
/// rings the bell; the editor reads the slot on the UI thread.
pub fn commit(job: Job, slot: Arc<Mutex<Option<Outcome>>>, weak: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        let outcome = run(job);
        if let Ok(mut slot) = slot.lock() {
            *slot = Some(outcome);
        }
        let _ = weak.upgrade_in_event_loop(|app| app.invoke_form_commit_finished());
    });
}

/// Write one product to disk.
///
/// The order is chosen for what a crash leaves behind. Every picked file is
/// looked at *before* anything is written, so a refusal leaves the vault
/// exactly as it was. Adding then writes the manifest last, so an interrupted
/// add leaves a folder full of PDFs and no manifest — which scans as broken,
/// shows up in the list, and can be finished by hand. Editing writes the
/// manifest before deleting anything, so an interrupted edit leaves an unlisted
/// orphan file rather than a manifest pointing at something that is gone.
pub fn run(job: Job) -> Outcome {
    let unreadable = |e: std::io::Error| DataError::Unreadable(e.to_string());

    for (source, name) in &job.imports {
        if let Err(reason) = inspect(source) {
            return Outcome::Refused {
                file: name.clone(),
                reason,
            };
        }
    }

    let adding = job.folder.is_none();
    let folder = match &job.folder {
        Some(folder) => folder.clone(),
        None => data::unique_folder(&job.products_root, &data::folder_slug(&job.draft.name)),
    };
    let home = job.products_root.join(&folder);

    if adding {
        if let Err(e) = fs::create_dir_all(&home) {
            return Outcome::Failed(unreadable(e));
        }
    }

    let mut invalidate = Vec::new();
    for (source, name) in &job.imports {
        let destination = home.join(name);
        if let Err(e) = fs::copy(source, &destination) {
            return Outcome::Failed(unreadable(e));
        }
        invalidate.push(destination);
    }

    if let Err(reason) = data::write_manifest(&home, &job.draft.manifest()) {
        return Outcome::Failed(reason);
    }

    for name in &job.removals {
        let path = home.join(name);
        // A copy that will not delete is an orphan, not a failure worth
        // undoing a good save for. The manifest no longer lists it either way.
        let _ = fs::remove_file(&path);
        invalidate.push(path);
    }

    Outcome::Done { folder, invalidate }
}

/// Open a picked file far enough to know it is a PDF Parachron can show.
///
/// The same two calls the viewer makes, so "is this a readable PDF" has one
/// answer in the app rather than two that can disagree. This runs on the commit
/// thread, never on the UI thread: MuPDF contexts are per-thread, and Chron2's
/// rule that the UI thread does not call MuPDF is worth keeping.
fn inspect(path: &Path) -> Result<(), ViewError> {
    let document = render::open_document(path)?;
    render::page_count(&document)?;
    Ok(())
}

/// A file name that is safe to write into a product folder and is not already
/// taken by one of `taken`.
///
/// Attaching the same file twice, or two files that happen to share a name, is
/// ordinary — an invoice and a warranty card exported from two different sites
/// are both called `download.pdf` more often than anyone would like.
pub fn destination_name(source: &Path, taken: &[String]) -> String {
    let raw = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    // `file_name` has already dropped any directory part; this is about what a
    // file name may contain, not about where it came from.
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\'))
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    let cleaned = if cleaned.is_empty() {
        format!("{FILE_FALLBACK}.pdf")
    } else {
        cleaned
    };

    if !taken.iter().any(|name| name == &cleaned) {
        return cleaned;
    }

    // Number the copy, keeping the extension where the eye expects it.
    let (stem, extension) = match cleaned.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (cleaned.clone(), String::new()),
    };
    for n in 2..=9999 {
        let candidate = format!("{stem}-{n}{extension}");
        if !taken.iter().any(|name| name == &candidate) {
            return candidate;
        }
    }
    format!("{stem}-9999{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month};

    fn draft(name: &str, pdfs: &[&str]) -> Draft {
        let date = Date::from_calendar_date(2026, Month::March, 14).unwrap();
        Draft {
            name: name.to_string(),
            serial: "ABC123XYZ".to_string(),
            link: String::new(),
            purchase_date: date,
            warranty_start: date,
            warranty_end: date,
            pdfs: pdfs.iter().map(|s| s.to_string()).collect(),
            added: date,
            extra: Default::default(),
        }
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn a_picked_file_keeps_its_own_name() {
        let name = destination_name(Path::new("/home/someone/Invoice.pdf"), &[]);
        assert_eq!(name, "Invoice.pdf");
    }

    #[test]
    fn two_files_with_the_same_name_do_not_overwrite_each_other() {
        let mut taken: Vec<String> = Vec::new();
        for expected in ["download.pdf", "download-2.pdf", "download-3.pdf"] {
            let name = destination_name(Path::new("/tmp/download.pdf"), &taken);
            assert_eq!(name, expected);
            taken.push(name);
        }
    }

    #[test]
    fn an_unusable_file_name_still_produces_something_writable() {
        for source in ["/tmp/...", "/tmp/   ", "/tmp/."] {
            let name = destination_name(Path::new(source), &[]);
            assert!(!name.is_empty(), "{source} produced an empty name");
            assert!(!name.contains('/'), "{source} produced {name}");
            assert_ne!(name, ".");
            assert_ne!(name, "..");
        }
    }

    #[test]
    fn adding_a_product_writes_the_folder_the_pdf_and_the_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(&products).unwrap();

        let outcome = run(Job {
            products_root: products.clone(),
            folder: None,
            draft: draft("QD-OLED Monitor", &["invoice.pdf"]),
            imports: vec![(fixture("sample.pdf"), "invoice.pdf".to_string())],
            removals: Vec::new(),
        });

        let Outcome::Done { folder, invalidate } = outcome else {
            panic!("a valid add must succeed: {outcome:?}");
        };
        assert_eq!(folder, "qd-oled-monitor");
        assert_eq!(invalidate.len(), 1);

        let home = products.join(&folder);
        assert!(home.join("product.toml").is_file());
        assert!(home.join("invoice.pdf").is_file());

        // And it reads back as the product that was written.
        let entries = data::scan(&products);
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0], data::Entry::Ok(p) if p.name == "QD-OLED Monitor"));
    }

    #[test]
    fn a_file_that_is_not_a_pdf_is_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(&products).unwrap();

        let outcome = run(Job {
            products_root: products.clone(),
            folder: None,
            draft: draft("Broken Import", &["nope.pdf"]),
            imports: vec![(fixture("corrupt.pdf"), "nope.pdf".to_string())],
            removals: Vec::new(),
        });

        assert!(
            matches!(outcome, Outcome::Refused { .. }),
            "expected a refusal, got {outcome:?}"
        );
        assert!(
            data::scan(&products).is_empty(),
            "a refused import must leave the vault exactly as it was"
        );
    }

    #[test]
    fn attaching_another_pdf_later_leaves_the_first_one_alone() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(&products).unwrap();

        run(Job {
            products_root: products.clone(),
            folder: None,
            draft: draft("Monitor", &["invoice.pdf"]),
            imports: vec![(fixture("sample.pdf"), "invoice.pdf".to_string())],
            removals: Vec::new(),
        });

        // The edit: same folder, one more file.
        let outcome = run(Job {
            products_root: products.clone(),
            folder: Some("monitor".to_string()),
            draft: draft("Monitor", &["invoice.pdf", "warranty.pdf"]),
            imports: vec![(fixture("multipage.pdf"), "warranty.pdf".to_string())],
            removals: Vec::new(),
        });
        assert!(matches!(outcome, Outcome::Done { .. }), "{outcome:?}");

        let home = products.join("monitor");
        assert!(home.join("invoice.pdf").is_file(), "the first file survives");
        assert!(home.join("warranty.pdf").is_file());

        let entries = data::scan(&products);
        let data::Entry::Ok(product) = &entries[0] else {
            panic!("still a valid product");
        };
        assert_eq!(product.pdfs, ["invoice.pdf", "warranty.pdf"]);
        assert!(product.missing_pdfs.is_empty());
    }

    #[test]
    fn removing_a_document_deletes_our_copy_and_not_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(&products).unwrap();

        // A source file of the user's own, somewhere else entirely.
        let source = dir.path().join("their-invoice.pdf");
        fs::copy(fixture("sample.pdf"), &source).unwrap();

        run(Job {
            products_root: products.clone(),
            folder: None,
            draft: draft("Monitor", &["their-invoice.pdf"]),
            imports: vec![(source.clone(), "their-invoice.pdf".to_string())],
            removals: Vec::new(),
        });

        let outcome = run(Job {
            products_root: products.clone(),
            folder: Some("monitor".to_string()),
            draft: draft("Monitor", &[]),
            imports: Vec::new(),
            removals: vec!["their-invoice.pdf".to_string()],
        });
        assert!(matches!(outcome, Outcome::Done { .. }), "{outcome:?}");

        assert!(
            !products.join("monitor/their-invoice.pdf").exists(),
            "our copy is gone"
        );
        assert!(
            source.is_file(),
            "the file the user picked from is never touched"
        );
    }

    #[test]
    fn a_commit_reports_every_path_whose_bytes_changed() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(&products).unwrap();

        let outcome = run(Job {
            products_root: products.clone(),
            folder: None,
            draft: draft("Monitor", &["invoice.pdf"]),
            imports: vec![(fixture("sample.pdf"), "invoice.pdf".to_string())],
            removals: Vec::new(),
        });
        let Outcome::Done { invalidate, .. } = outcome else {
            panic!("must succeed");
        };

        // The render worker caches by path with no modification time, so every
        // path written has to be named or it will serve the old pixels.
        assert_eq!(invalidate, vec![products.join("monitor/invoice.pdf")]);
    }
}
