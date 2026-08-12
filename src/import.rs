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
use std::time::Duration;

use crate::AppWindow;
use crate::data::{self, DataError, Draft};
use crate::render::{self, Invalidator, ViewError};

/// File name for a picked file whose own name is unusable.
const FILE_FALLBACK: &str = "document";

/// Conservative upper bound on a file stem, in bytes, so `stem + "-9999" +
/// extension` stays well under Windows' 260-character `MAX_PATH` (even after
/// the vault's own path prefix) and ext4's 255-byte `NAME_MAX`.
const STEM_MAX: usize = 100;

/// How many times an add retries claiming a fresh product folder, or a copy
/// retries a fresh file name, when the one it picked turns out to already
/// exist. Bounded so a pathological vault fails loudly rather than looping.
const MAX_COLLISION_ATTEMPTS: u32 = 20;

/// What a commit did, or why it did not.
#[derive(Debug)]
pub enum Outcome {
    Done {
        folder: String,
        /// Paths whose bytes have changed, for the render worker to forget.
        invalidate: Vec<PathBuf>,
    },
    /// A picked file was not a PDF Parachron can show. Nothing was written.
    Refused {
        file: String,
        reason: ViewError,
    },
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
    /// Lets a removal ask the render worker to drop its open handle on a file
    /// before it is deleted, rather than after. `None` in tests that have no
    /// render worker running; every production caller has a real [`Viewer`]
    /// and always provides one.
    ///
    /// [`Viewer`]: crate::viewer::Viewer
    pub invalidator: Option<Invalidator>,
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
///
/// An add additionally undoes the folder it created on any later failure
/// (copy, manifest write): a folder this call created and could not finish is
/// nobody's data yet, and leaving it behind is exactly the permanent broken
/// row — multiplied on every retry — that "scans as broken... and can be
/// finished by hand" above used to require of a user for an ordinary failure
/// like a full disk, not just a crash.
pub fn run(job: Job) -> Outcome {
    let unreadable = |e: std::io::Error| DataError::Unreadable(e.to_string());

    // `main` hands out an empty `products_root` when the vault could not be
    // opened at all (no home directory, a `config.toml` that will not parse,
    // a configured vault that is not mounted) — see `main::open_vault`. An
    // empty path is relative, and joining a folder name onto it does not
    // fail, it just points *into the process's current working directory*,
    // wherever a desktop entry or shell happened to launch this from. This is
    // the one check that holds regardless of whether the UI remembered to
    // disable Add Document for that state.
    if !job.products_root.is_absolute() {
        return Outcome::Failed(DataError::Unreadable("no vault is open".to_string()));
    }

    for (source, name) in &job.imports {
        if let Err(reason) = inspect(source) {
            return Outcome::Refused {
                file: name.clone(),
                reason,
            };
        }
    }

    let adding = job.folder.is_none();
    let mut folder = match &job.folder {
        Some(folder) => folder.clone(),
        None => data::unique_folder(&job.products_root, &data::folder_slug(&job.draft.name)),
    };
    let mut home = job.products_root.join(&folder);

    // Whether *this* call created `home`, so a failure below can clean up
    // after itself and so a caller can tell a fresh add from an edit.
    let mut created = false;

    if adding {
        // `unique_folder`'s answer is a snapshot of the directory, not a lock
        // on a name — a second Parachron instance, or a sync client
        // materialising another machine's folder, can create the same name in
        // the window between that check and this one. `fs::create_dir` (not
        // `create_dir_all`) is what makes the race detectable: it fails with
        // `AlreadyExists` instead of silently succeeding against — and about
        // to write into — a folder this call does not own. `home` must
        // already have an existing parent (the vault's `products/`), so
        // `create_dir` is also what refuses to resurrect a vanished vault
        // directory the way `create_dir_all` would.
        for attempt in 0..MAX_COLLISION_ATTEMPTS {
            match fs::create_dir(&home) {
                Ok(()) => {
                    created = true;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if attempt + 1 == MAX_COLLISION_ATTEMPTS {
                        return Outcome::Failed(unreadable(e));
                    }
                    folder = data::unique_folder(
                        &job.products_root,
                        &data::folder_slug(&job.draft.name),
                    );
                    home = job.products_root.join(&folder);
                }
                Err(e) => return Outcome::Failed(unreadable(e)),
            }
        }
    }

    let mut invalidate = Vec::new();
    for (source, name) in &job.imports {
        match copy_into(source, &home, name) {
            Ok(destination) => invalidate.push(destination),
            Err(e) => {
                if created {
                    let _ = fs::remove_dir_all(&home);
                }
                return Outcome::Failed(unreadable(e));
            }
        }
    }

    if let Err(reason) = data::write_manifest(&home, &job.draft.manifest()) {
        if created {
            let _ = fs::remove_dir_all(&home);
        }
        return Outcome::Failed(reason);
    }

    for name in &job.removals {
        let path = home.join(name);
        // The render worker may still hold this exact file open from the
        // viewer's last render of it. Invalidating first asks it to drop that
        // handle before the delete is attempted rather than after — after is
        // what the outcome used to do, once this had already crossed back to
        // the UI thread and further still to the render worker's own queue,
        // a much longer window than sending it from right here.
        if let Some(invalidator) = &job.invalidator {
            invalidator.invalidate(&path);
        }
        if fs::remove_file(&path).is_err() {
            // The worker processes its queue on its own thread, so there is
            // no guarantee it has already released the handle. One short
            // retry covers the ordinary race without turning a delete that
            // will never succeed into a hang.
            std::thread::sleep(Duration::from_millis(30));
            if let Err(e) = fs::remove_file(&path) {
                // A copy that will not delete is an orphan, not a failure
                // worth undoing a good save for — the manifest no longer
                // lists it either way — but it is not nothing either: this is
                // the one place that outcome is worth a word, since the
                // sheet's own UI already told the user the document was gone.
                eprintln!(
                    "parachron: could not delete {} ({e}); the copy remains in the vault folder",
                    path.display()
                );
            }
        }
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

/// Copy `source` into `home` under `name`, never overwriting a file that is
/// already there.
///
/// `destination_name` already avoided every name the editor knew about at the
/// moment the file was picked, but that list is a snapshot: it does not see a
/// name the disk gains afterward — the same name arriving on a case- or
/// normalisation-insensitive filesystem, a second Parachron instance, a sync
/// client, or simply the time the sheet sat open between the pick and this
/// Save. An exclusive create is what actually enforces "never overwrite" —
/// `fs::copy` would truncate a same-named file that already exists — and
/// numbering a fresh name on a collision, the same way `destination_name`
/// does, turns a stale snapshot into a retry instead of a silent loss.
fn copy_into(source: &Path, home: &Path, name: &str) -> std::io::Result<PathBuf> {
    let (stem, extension) = split_stem(name);
    let mut candidate = name.to_string();

    for attempt in 2..2 + MAX_COLLISION_ATTEMPTS {
        let destination = home.join(&candidate);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(mut file) => {
                let mut input = fs::File::open(source)?;
                std::io::copy(&mut input, &mut file)?;
                return Ok(destination);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                candidate = format!("{stem}-{attempt}{extension}");
            }
            Err(e) => return Err(e),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("no free name for {name} after {MAX_COLLISION_ATTEMPTS} attempts"),
    ))
}

/// Split "stem.ext" into ("stem", ".ext"), the same way a numbered collision
/// is built both in [`destination_name`] and in [`copy_into`]'s retry.
fn split_stem(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.to_string(), String::new()),
    }
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
    // file name may contain, not about where it came from. The character set
    // matches `data::folder_slug`'s Windows-safety pass — CORE §7 ships a
    // Windows binary, and a vault that syncs onto one must not contain a file
    // it cannot open any more than a folder it cannot open — rather than only
    // the two characters that would otherwise break a path.
    const ILLEGAL: [char; 9] = ['/', '\\', ':', '?', '*', '<', '>', '|', '"'];
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !ILLEGAL.contains(c))
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    let cleaned = if cleaned.is_empty() {
        format!("{FILE_FALLBACK}.pdf")
    } else {
        cleaned
    };

    let (stem, extension) = split_stem(&cleaned);

    // Reserved regardless of extension or directory — the same check
    // `folder_slug` applies to a directory name, reused rather than
    // duplicated, folded to lowercase only for this comparison so the
    // returned name otherwise keeps the case the source file had.
    let stem = if data::is_reserved(&stem.to_ascii_lowercase()) {
        format!("{stem}-{FILE_FALLBACK}")
    } else {
        stem
    };

    // Truncated on a character boundary so a multi-byte name (this app is
    // used in Turkish, CORE §4) never splits mid-character.
    let mut stem = stem;
    if stem.len() > STEM_MAX {
        stem.truncate(STEM_MAX);
        while !stem.is_char_boundary(stem.len()) {
            stem.pop();
        }
    }
    let cleaned = format!("{stem}{extension}");

    // Case-insensitive (ASCII only — ASCII case-folding, not `to_lowercase`,
    // because `data::fold`'s own comment records `to_lowercase` mangling the
    // Turkish `İ` into a combining mark, which would make two visibly
    // different names compare equal): Windows, exFAT/FAT32 (a vault on a
    // removable drive is very often one of these even from Linux) and macOS's
    // default filesystems all treat "Invoice.pdf" and "invoice.pdf" as one
    // file, and `fs::copy`/the exclusive create in `copy_into` would not
    // otherwise know that "free" name is the same file as one already listed.
    if !taken.iter().any(|name| name.eq_ignore_ascii_case(&cleaned)) {
        return cleaned;
    }

    for n in 2..=9999 {
        let candidate = format!("{stem}-{n}{extension}");
        if !taken
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&candidate))
        {
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

    /// On Windows, exFAT/FAT32 (very often what a vault on a removable drive
    /// is formatted as, even from Linux) and macOS's default filesystems,
    /// "Invoice.pdf" and "invoice.pdf" are the same file. The byte-exact
    /// comparison this pins against would hand back "invoice.pdf" as free —
    /// a name `fs::copy` (or `copy_into`'s exclusive create) would then either
    /// truncate the original through, or (with the create_new fix) refuse and
    /// number a new one, which is correct but still worth pinning at the
    /// `taken`-comparison layer so the common case never needs the fallback.
    #[test]
    fn destination_name_treats_names_as_taken_regardless_of_ascii_case() {
        let name = destination_name(Path::new("/tmp/invoice.pdf"), &["Invoice.pdf".to_string()]);
        assert_eq!(name, "invoice-2.pdf");
    }

    #[test]
    fn destination_name_avoids_windows_reserved_device_names() {
        for raw in [
            "/tmp/nul.pdf",
            "/tmp/NUL.pdf",
            "/tmp/con.pdf",
            "/tmp/COM1.pdf",
        ] {
            let name = destination_name(Path::new(raw), &[]);
            let stem = name.split('.').next().unwrap().to_ascii_lowercase();
            assert!(
                !matches!(stem.as_str(), "nul" | "con" | "com1"),
                "{raw} produced the reserved name {name}, which Windows resolves to a device"
            );
        }
    }

    #[test]
    fn destination_name_strips_characters_windows_cannot_use_in_a_file_name() {
        let name = destination_name(Path::new("/tmp/Scan 2026-08-12 14:37:02.pdf"), &[]);
        for illegal in [':', '?', '*', '<', '>', '|', '"'] {
            assert!(!name.contains(illegal), "{name} still contains {illegal:?}");
        }
    }

    #[test]
    fn destination_name_bounds_the_stem_length() {
        let long = "a".repeat(500);
        let name = destination_name(Path::new(&format!("/tmp/{long}.pdf")), &[]);
        assert!(
            name.len() < 150,
            "produced a {}-byte name, which risks MAX_PATH on Windows once joined onto a vault path",
            name.len()
        );
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
            invalidator: None,
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

    /// `main` hands out an empty `products_root` when no vault could be opened
    /// at all — see `main::open_vault`. Joining a folder name onto an empty
    /// path does not fail, it silently resolves relative to wherever the
    /// process's current directory happens to be, so this has to be refused
    /// explicitly rather than left to a `fs::create_dir` that would often
    /// succeed there too.
    #[test]
    fn a_relative_products_root_is_refused_rather_than_written_relative_to_the_working_directory() {
        let outcome = run(Job {
            products_root: PathBuf::new(),
            folder: None,
            draft: draft("Should Never Land", &["invoice.pdf"]),
            imports: vec![(fixture("sample.pdf"), "invoice.pdf".to_string())],
            removals: Vec::new(),
            invalidator: None,
        });

        assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
        assert!(
            !Path::new("should-never-land").exists(),
            "a folder was created relative to the test binary's working directory"
        );
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
            invalidator: None,
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
            invalidator: None,
        });

        // The edit: same folder, one more file.
        let outcome = run(Job {
            products_root: products.clone(),
            folder: Some("monitor".to_string()),
            draft: draft("Monitor", &["invoice.pdf", "warranty.pdf"]),
            imports: vec![(fixture("multipage.pdf"), "warranty.pdf".to_string())],
            removals: Vec::new(),
            invalidator: None,
        });
        assert!(matches!(outcome, Outcome::Done { .. }), "{outcome:?}");

        let home = products.join("monitor");
        assert!(
            home.join("invoice.pdf").is_file(),
            "the first file survives"
        );
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
            invalidator: None,
        });

        let outcome = run(Job {
            products_root: products.clone(),
            folder: Some("monitor".to_string()),
            draft: draft("Monitor", &[]),
            imports: Vec::new(),
            removals: vec!["their-invoice.pdf".to_string()],
            invalidator: None,
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
            invalidator: None,
        });
        let Outcome::Done { invalidate, .. } = outcome else {
            panic!("must succeed");
        };

        // The render worker caches by path with no modification time, so every
        // path written has to be named or it will serve the old pixels.
        assert_eq!(invalidate, vec![products.join("monitor/invoice.pdf")]);
    }

    /// `unique_folder`'s answer is a snapshot, not a lock. This simulates a
    /// second Parachron instance (or a sync client materialising another
    /// machine's folder) winning the race: the name it would have picked is
    /// already there, with its own product in it, by the time this call
    /// actually creates a directory.
    #[test]
    fn a_racing_folder_is_never_silently_adopted() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(&products).unwrap();

        let existing = products.join("monitor");
        fs::create_dir_all(&existing).unwrap();
        data::write_manifest(&existing, &draft("Existing Product", &[]).manifest()).unwrap();

        let outcome = run(Job {
            products_root: products.clone(),
            folder: None,
            draft: draft("Monitor", &["invoice.pdf"]),
            imports: vec![(fixture("sample.pdf"), "invoice.pdf".to_string())],
            removals: Vec::new(),
            invalidator: None,
        });

        let Outcome::Done { folder, .. } = outcome else {
            panic!("must still succeed, into a different folder: {outcome:?}");
        };
        assert_eq!(
            folder, "monitor-2",
            "the folder already claimed by somebody else must never be adopted"
        );

        let entries = data::scan(&products);
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .any(|e| matches!(e, data::Entry::Ok(p) if p.name == "Existing Product")),
            "the folder this call did not create must be completely untouched: {entries:?}"
        );
    }

    /// A failure partway through an add — after the folder was created, before
    /// the manifest is written — must not leave a permanent half-built folder
    /// for `scan` to report as broken forever, and must not multiply on retry.
    #[test]
    fn a_failed_add_does_not_leave_a_broken_folder_behind() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(&products).unwrap();

        // A NUL byte is not something `destination_name` could ever produce
        // (it filters control characters), but a manifest this app did not
        // write is not a trusted input, and this stands in for any copy
        // failure that occurs after the folder already exists — a full disk
        // included.
        let outcome = run(Job {
            products_root: products.clone(),
            folder: None,
            draft: draft("Monitor", &["bad\0name.pdf"]),
            imports: vec![(fixture("sample.pdf"), "bad\0name.pdf".to_string())],
            removals: Vec::new(),
            invalidator: None,
        });

        assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
        assert!(
            data::scan(&products).is_empty(),
            "a folder this call created and could not finish must not be left behind"
        );
    }

    /// `destination_name`'s `taken` list is a snapshot the editor built when
    /// the file was picked; it can go stale by the time the commit actually
    /// runs. The copy itself has to be the real guarantee against overwriting
    /// an existing document.
    #[test]
    fn a_copy_never_truncates_a_file_already_at_the_destination() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(&products).unwrap();

        run(Job {
            products_root: products.clone(),
            folder: None,
            draft: draft("Monitor", &["invoice.pdf"]),
            imports: vec![(fixture("sample.pdf"), "invoice.pdf".to_string())],
            removals: Vec::new(),
            invalidator: None,
        });

        let home = products.join("monitor");
        let original = fs::read(home.join("invoice.pdf")).unwrap();

        // Asks to write "invoice.pdf" again — exactly the name already there,
        // as if the `taken` snapshot that chose this name had gone stale.
        let outcome = run(Job {
            products_root: products.clone(),
            folder: Some("monitor".to_string()),
            draft: draft("Monitor", &["invoice.pdf", "invoice-2.pdf"]),
            imports: vec![(fixture("multipage.pdf"), "invoice.pdf".to_string())],
            removals: Vec::new(),
            invalidator: None,
        });
        assert!(matches!(outcome, Outcome::Done { .. }), "{outcome:?}");

        assert_eq!(
            fs::read(home.join("invoice.pdf")).unwrap(),
            original,
            "the original file's bytes must survive a same-name collision at copy time"
        );
        assert!(
            home.join("invoice-2.pdf").is_file(),
            "the colliding import must land under a renumbered name instead of being dropped"
        );
    }
}
