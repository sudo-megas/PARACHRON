//! Moving a vault onto the disk its owner chose (Chron9, CORE §3).
//!
//! Parachron copies documents into the vault rather than referencing them where
//! they were found, so a vault grows with the paperwork put into it — and until
//! this module existed it could only grow on whatever disk `$HOME` sits on.
//!
//! Three things here are load-bearing rather than incidental.
//!
//! **The order is copy, verify, remove, and never the other way round.** A
//! failure at any point before the last step has damaged nothing: the source is
//! untouched, the partial destination is cleaned up, and `config.toml` still
//! names the old location, so the next launch opens the vault that still exists.
//! Removing the source only after the copy verifies means there is no instant in
//! which the documents exist in neither place. Every test in this module is
//! written against that invariant rather than against the happy path, because
//! the happy path is the one that gets exercised by hand and the failure path is
//! the one that does not.
//!
//! **`fs::rename` is tried first and is expected to fail.** Crossing a
//! filesystem is what "put it on my other drive" means, and it is exactly what
//! `rename` cannot do — it returns `EXDEV`. The fast path is still worth having,
//! because relocating within one disk is instant and atomic, and an atomic move
//! is strictly better than a copy when it is available.
//!
//! **Progress is reported per file, not per chunk.** A vault is PDFs and moving
//! one across disks takes long enough that a still window reads as a crash, so
//! the worker says where it is; but a message per buffer would flood the event
//! loop for a vault of several hundred documents, and the useful unit is a file
//! anyway — "invoice.pdf" is something a person can read.

use std::cell::RefCell;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use crate::AppWindow;
use crate::config::Config;
use crate::data::Paths;
use crate::editor::Editors;
use crate::export::Exports;
use crate::strings::{self, Key, Lang};
use crate::vault::Vault;
use crate::viewer::Viewer;

/// What a move needs to know. Composed on the UI thread, run on another.
#[derive(Debug, Clone)]
pub struct Job {
    /// The vault as it is now — the directory holding `products/`.
    pub from: PathBuf,
    /// Where the user pointed the folder picker.
    pub to: PathBuf,
}

/// Why a chosen destination cannot be used.
///
/// Structured rather than pre-rendered, like [`crate::data::DataError`], so the
/// UI renders them through the string table (CORE §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The destination is the vault already in use.
    Same,
    /// The destination is inside the current vault.
    ///
    /// This one is not merely pointless, it is destructive: copy-then-remove
    /// would write the copy underneath the source and then delete both.
    Inside,
    /// The destination already holds a `products/`.
    ///
    /// Merging two vaults is a different feature, and doing it by accident
    /// resolves name collisions by whichever file happened to be written last.
    Occupied,
    /// The path is not valid UTF-8, so `config.toml` cannot record it.
    ///
    /// On Linux a filename is bytes with no encoding guarantee and a folder
    /// picker can legitimately return one. TOML has no way to write it down, so
    /// it is refused rather than lossily converted into a similar-looking path
    /// that would be wrong every time it was read back — the same wall Chron7
    /// met from the other side when it chose `write_to` over `save`.
    NotUtf8,
}

/// Why a move that started did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Reading the source vault failed; carries the OS message.
    Read(String),
    /// Writing into the destination failed; carries the file and the OS message.
    Write { file: String, detail: String },
    /// The copy completed but did not match the source.
    ///
    /// Its own variant rather than a `Write`, because it means the destination
    /// looked writable and lied — and the source is deliberately still there.
    Verify(String),
}

/// How far along a running move is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The file being copied, for the sheet to show. A file name as it is on
    /// disk, which CORE §4 says is not UI copy and never translates.
    pub current: String,
}

/// What a finished move left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The vault is now at this path, and `config.toml` may be updated.
    Moved { to: PathBuf, files: usize },
    /// Nothing moved. The source is intact and `config.toml` is untouched.
    Failed(Failure),
}

/// What the worker and the window share.
///
/// An `Arc<Mutex<_>>` rather than a channel for the same reason `export.rs`
/// gives: the module is full of `Rc`s that cannot cross a thread boundary, so
/// the result travels here and the window is only ever *rung*.
#[derive(Debug, Default)]
pub struct Shared {
    pub progress: Progress,
    pub outcome: Option<Outcome>,
}

/// A file to copy, discovered before anything is written.
struct Item {
    relative: PathBuf,
    bytes: u64,
}

/// Everything the move will touch, measured before it starts.
///
/// Surveying first buys two things. The sheet can say how many documents and how
/// many megabytes are about to move, which is what makes the confirmation a
/// decision rather than a dare; and the progress bar can be determinate, because
/// the total is known rather than discovered.
pub struct Survey {
    items: Vec<Item>,
    pub files: usize,
    pub bytes: u64,
}

/// Decide whether a chosen destination can be used at all.
///
/// Every one of these is reachable with a single click of a folder picker, which
/// is why each gets its own answer rather than one generic refusal — "that
/// location cannot be used" tells somebody nothing about which of four quite
/// different mistakes they made.
pub fn vet(from: &Path, to: &Path) -> Result<(), Refusal> {
    if to.to_str().is_none() {
        return Err(Refusal::NotUtf8);
    }

    // Compared after normalising, so `/vault` and `/vault/./` are the same
    // answer. Neither path is canonicalised: `canonicalize` resolves symlinks
    // and requires the path to exist, and a user who has deliberately symlinked
    // their vault should not be told it is the same directory as its target.
    let from_n = normalise(from);
    let to_n = normalise(to);

    if from_n == to_n {
        return Err(Refusal::Same);
    }
    if to_n.starts_with(&from_n) {
        return Err(Refusal::Inside);
    }
    if to.join("products").is_dir() {
        return Err(Refusal::Occupied);
    }
    Ok(())
}

/// Strip `.` components and redundant separators without touching the disk.
fn normalise(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect()
}

/// Measure what a move would copy, without copying any of it.
pub fn survey(from: &Path) -> Result<Survey, Failure> {
    let mut items = Vec::new();
    let mut bytes = 0;
    let products = from.join("products");
    if products.is_dir() {
        walk(&products, &products, &mut items, &mut bytes)?;
    }
    Ok(Survey {
        files: items.len(),
        bytes,
        items,
    })
}

/// Collect every file under `dir`, recording each path relative to `root`.
fn walk(root: &Path, dir: &Path, out: &mut Vec<Item>, bytes: &mut u64) -> Result<(), Failure> {
    let entries = fs::read_dir(dir).map_err(|e| Failure::Read(e.to_string()))?;
    // Sorted, so a move is reproducible and its progress reads in a stable
    // order rather than in whatever order the filesystem hands things back.
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| Failure::Read(e.to_string()))?;
        paths.push(entry.path());
    }
    paths.sort();

    for path in paths {
        // Symlinks are copied as whatever they point at rather than followed
        // into a loop: `symlink_metadata` decides *that* without following, so
        // a symlink to a directory is recorded as a leaf here rather than
        // recursed into forever.
        let link_meta = fs::symlink_metadata(&path).map_err(|e| Failure::Read(e.to_string()))?;
        if link_meta.is_dir() {
            walk(root, &path, out, bytes)?;
            continue;
        }

        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        if link_meta.is_file() {
            let len = link_meta.len();
            *bytes += len;
            out.push(Item {
                relative,
                bytes: len,
            });
            continue;
        }

        // Whatever is left is a symlink to a file (a symlink to a directory
        // was handled above), a dangling symlink, a FIFO, a socket or a device
        // node. A symlink's own `symlink_metadata` length is the byte length of
        // the link's *target text*, not of the file it points at — recording
        // that and then having the copy step follow the link, which it must to
        // actually copy the file the link stands in for, would hand `verify` a
        // number that can never match what was really written. Resolving once,
        // here, is what makes the size this function records the same number
        // the copy step produces; and it is also what turns a symlink into
        // `/dev/zero` or a FIFO that would otherwise hang or fill the disk
        // into a refusal before a single byte moves.
        let real = fs::metadata(&path).map_err(|e| Failure::Read(format!("{}: {e}", path.display())))?;
        if !real.is_file() {
            return Err(Failure::Read(format!(
                "{}: not a plain file or a symlink to one — move it out of the vault by hand first",
                path.display()
            )));
        }
        let len = real.len();
        *bytes += len;
        out.push(Item { relative, bytes: len });
    }
    Ok(())
}

/// Run a move on a thread and ring the window as it goes.
///
/// The window is rung on every file rather than only at the end, which is the
/// one way this differs from `export.rs`'s worker — an export produces a file
/// and says so, a move produces minutes of silence unless it reports itself.
pub fn commit(
    job: Job,
    survey: Survey,
    config_path: PathBuf,
    data_dir: PathBuf,
    shared: Arc<Mutex<Shared>>,
    weak: slint::Weak<AppWindow>,
) {
    std::thread::spawn(move || {
        let outcome = run(job, survey, &config_path, &data_dir, &|progress| {
            if let Ok(mut shared) = shared.lock() {
                shared.progress = progress;
            }
            // Every send ignored: a window that has gone away must not take the
            // move down with it, and the move is the thing holding the user's
            // documents. Same rule as `render.rs`'s sink.
            let _ = weak.upgrade_in_event_loop(|app| app.invoke_relocate_progressed());
        });

        if let Ok(mut shared) = shared.lock() {
            shared.outcome = Some(outcome);
        }
        let _ = weak.upgrade_in_event_loop(|app| app.invoke_relocate_finished());
    });
}

/// Move a vault. A plain function of a [`Job`] plus where `config.toml` lives,
/// so the move itself is testable by handing it two directories — the same
/// shape `export::run` has — while still being able to record the result.
///
/// `report` is called once per file. In a test it collects; in the app it fills
/// the shared slot and rings the window.
pub fn run(job: Job, survey: Survey, config_path: &Path, data_dir: &Path, report: &dyn Fn(Progress)) -> Outcome {
    let Job { from, to } = job;
    let files = survey.files;

    // The fast path, and the one that will almost never be taken for the case
    // this module exists for. `rename` across a filesystem fails with `EXDEV`,
    // and a different filesystem is what "my other drive" means — but within one
    // disk this is instant and atomic, which is strictly better than a copy.
    let source_products = from.join("products");
    let target_products = to.join("products");
    if source_products.is_dir() && fs::rename(&source_products, &target_products).is_ok() {
        report(Progress {
            files_done: files,
            files_total: files,
            bytes_done: survey.bytes,
            bytes_total: survey.bytes,
            current: String::new(),
        });
        persist_vault(config_path, data_dir, &to);
        return Outcome::Moved { to, files };
    }

    match copy_all(&source_products, &target_products, &survey, report) {
        Ok(()) => {}
        Err(failure) => {
            // The destination is torn down and the source is left exactly as it
            // was. `remove_dir_all` failing here is not worth reporting over the
            // failure that caused it — the user needs to know the move did not
            // happen, and that their vault is untouched, which is still true.
            let _ = fs::remove_dir_all(&target_products);
            return Outcome::Failed(failure);
        }
    }

    // The new location is committed to disk now, while the old vault — the
    // only copy that could contradict it — still exists as a fallback if this
    // fails. Doing this after removing the source (the previous order) left a
    // window, sometimes hours wide until the next clean exit, where
    // `config.toml` named a vault whose `products/` had already been deleted.
    persist_vault(config_path, data_dir, &to);

    // Only now. Everything above this line is recoverable by doing nothing.
    if let Err(e) = fs::remove_dir_all(&source_products) {
        // The copy is verified and complete, so the documents are safe in the
        // new location — this is a leftover, not a loss. Reporting it as a
        // failure would be a lie that sends the user looking for missing files.
        eprintln!("parachron: the old vault could not be removed: {e}");
    }

    Outcome::Moved { to, files }
}

/// Record the vault's new location in `config.toml`, right after the copy (or
/// rename) verifies and before the old vault is touched.
///
/// A read-modify-write of the file already on disk, rather than a value
/// threaded down from the live session: everything else `config.toml` holds —
/// theme, language, sort, window size — is only ever saved when the window
/// closes (`main::persist`), so reading what is already there and changing
/// only `vault` matches that design exactly, and does not need the running
/// session's in-memory state — which lives behind `Rc`s a background thread
/// cannot reach — passed down here at all. The window-close save still runs
/// and still writes the fully current session afterwards; this is only the
/// safety net for a crash or a kill in between.
///
/// Best-effort: a failure here is logged, not propagated. The move has
/// already copied and verified the documents into `to` (or renamed them
/// there); refusing to finish it because this one file could not be updated
/// would trade a recoverable inconvenience — `config.toml` catches up at the
/// next clean exit, exactly as it always has — for leaving a good copy of the
/// vault sitting unused while the original is kept past the point it needed
/// to be.
fn persist_vault(config_path: &Path, data_dir: &Path, to: &Path) {
    let mut config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!(
                "parachron: could not read config.toml to record the new vault location ({e}); it will be written when the window closes instead"
            );
            return;
        }
    };
    config.vault = to.to_str().filter(|_| to != data_dir).map(str::to_string);
    if let Err(e) = config.save(config_path) {
        eprintln!(
            "parachron: could not record the new vault location in config.toml ({e}); it will be written when the window closes instead"
        );
    }
}

/// Copy every surveyed file, then check the result before anybody deletes
/// anything.
fn copy_all(
    source: &Path,
    target: &Path,
    survey: &Survey,
    report: &dyn Fn(Progress),
) -> Result<(), Failure> {
    // `target`'s parent — the folder the user chose — must already exist.
    // `create_dir_all` would happily rebuild a chain of missing ancestors, and
    // a drive that came unmounted between the picker and this call presents
    // exactly that way: an ordinary, empty, *creatable* directory at the mount
    // point, on the root filesystem. Building the vault there would put it
    // where its owner never looked, and plugging the drive back in afterwards
    // would hide the lot underneath it — on disk, invisible, and impossible to
    // explain to somebody who did nothing wrong. Requiring the destination to
    // already be there turns that into a loud failure instead.
    let destination = target
        .parent()
        .expect("target is always <destination>/products");
    if !destination.is_dir() {
        return Err(Failure::Write {
            file: destination.display().to_string(),
            detail: "the destination no longer exists".to_string(),
        });
    }
    fs::create_dir(target).map_err(|e| Failure::Write {
        file: target.display().to_string(),
        detail: e.to_string(),
    })?;

    let mut bytes_done = 0;
    for (index, item) in survey.items.iter().enumerate() {
        let from = source.join(&item.relative);
        let to = target.join(&item.relative);
        let name = item.relative.display().to_string();

        if let Some(parent) = item.relative.parent().filter(|p| !p.as_os_str().is_empty()) {
            create_dir_without_following_symlinks(target, parent).map_err(|e| Failure::Write {
                file: name.clone(),
                detail: e.to_string(),
            })?;
        }

        // An exclusive create, not `fs::copy`: `fs::copy` opens the destination
        // for writing however it finds it, symlink included, and a symlink
        // planted at exactly this name by another local user (or process) with
        // write access to the destination — in the moment between this move
        // creating the product folder and reaching this particular file within
        // it — would be written through rather than refused. `create_new`
        // fails on anything already there, symlink or not, and every file this
        // loop writes is one that should not already exist in a destination
        // this move itself is populating.
        let mut writer = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&to)
            .map_err(|e| Failure::Write {
                file: name.clone(),
                detail: e.to_string(),
            })?;
        let mut reader = fs::File::open(&from).map_err(|e| Failure::Write {
            file: name.clone(),
            detail: e.to_string(),
        })?;
        std::io::copy(&mut reader, &mut writer).map_err(|e| Failure::Write {
            file: name.clone(),
            detail: e.to_string(),
        })?;
        // Without this the file can look copied — `verify` reads it straight
        // back through the same page cache the write went into — while the
        // bytes are still only in memory. `write_atomic` in `data.rs` takes
        // this precaution for a single manifest already, for the same
        // reason spelled out there: `run` deletes the only other copy right
        // after this succeeds, and a crash or a pulled drive between "the
        // kernel says this is written" and "the kernel actually wrote it" is
        // exactly what turns a reported-successful move into a vault that
        // is truncated in its new home and already gone from its old one.
        writer.sync_all().map_err(|e| Failure::Write {
            file: name.clone(),
            detail: e.to_string(),
        })?;

        // Best-effort, unlike the file sync above: this is the directory
        // entry, not the document's own bytes, and no test in this module —
        // nor `write_atomic`'s own precedent — treats losing it as the same
        // class of failure as losing data.
        if let Some(parent) = to.parent()
            && let Ok(dir) = fs::File::open(parent)
        {
            let _ = dir.sync_all();
        }

        bytes_done += item.bytes;
        // After the copy, not before: a bar that reaches 100% and then sits
        // there is a bar that has been lying since the last file started.
        report(Progress {
            files_done: index + 1,
            files_total: survey.files,
            bytes_done,
            bytes_total: survey.bytes,
            current: name,
        });
    }

    // Same best-effort directory sync as inside the loop, for the one
    // directory the loop never opens on its own: `target` itself.
    if let Ok(dir) = fs::File::open(target) {
        let _ = dir.sync_all();
    }

    verify(source, target, survey)
}

/// Create `target/relative`, refusing to treat a symlink as though it were one
/// of the directories along the way.
///
/// `fs::create_dir_all` does not distinguish a directory this move created a
/// moment ago (for an earlier file in the same product) from a symlink
/// planted at that exact path since — both simply "already exist", and
/// `create_dir_all` walks straight through either. Checking each level with
/// `symlink_metadata`, which does not follow a link, is what tells them apart.
fn create_dir_without_following_symlinks(target: &Path, relative: &Path) -> std::io::Result<()> {
    let mut built = target.to_path_buf();
    for component in relative.components() {
        built.push(component);
        match fs::create_dir(&built) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&built)?.is_dir() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        format!("{} exists and is not a directory", built.display()),
                    ));
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Confirm the copy before the source is deleted.
///
/// Sizes rather than contents. Hashing every byte of a vault would double the
/// read cost of the move to defend against a filesystem that reported a
/// successful write and did not perform one — which is a real failure mode and a
/// rare one, and the cheaper check catches the common shapes of it: a truncated
/// write, a full disk that reported success, a file that never arrived.
fn verify(source: &Path, target: &Path, survey: &Survey) -> Result<(), Failure> {
    for item in &survey.items {
        let copied = fs::symlink_metadata(target.join(&item.relative))
            .map_err(|e| Failure::Verify(format!("{}: {e}", item.relative.display())))?;
        if copied.len() != item.bytes {
            return Err(Failure::Verify(format!(
                "{}: {} bytes copied, {} expected",
                item.relative.display(),
                copied.len(),
                item.bytes
            )));
        }
    }

    // A file modified, added or removed in the source while the move was
    // running is invisible to a copy already taken. Comparing only *counts*
    // (the previous check) misses the shape a sync client's own replace
    // pattern takes — delete an old name, create the final one — because the
    // count balances while the actual set of names does not, and the file
    // that was never copied is deleted for good the moment this trusts that
    // check. Comparing the full set of names and sizes is what actually proves
    // nothing changed underneath the move. Parachron does not write to the
    // vault while a move is in flight — the UI is blocked on it — but nothing
    // else on the machine is stopped from doing so.
    let after = resurvey(source)?;
    let changed = after.len() != survey.items.len()
        || after.iter().any(|item| {
            !survey
                .items
                .iter()
                .any(|before| before.relative == item.relative && before.bytes == item.bytes)
        });
    if changed {
        return Err(Failure::Verify(
            "the vault changed while it was being moved".to_string(),
        ));
    }
    Ok(())
}

/// Re-walk a directory for the changed-underneath check in [`verify`].
fn resurvey(dir: &Path) -> Result<Vec<Item>, Failure> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    let mut bytes = 0;
    walk(dir, dir, &mut items, &mut bytes)?;
    Ok(items)
}

// ── Wiring ───────────────────────────────────────────────────────────────

/// What the move needs kept between a menu click and a thread landing.
struct State {
    paths: Paths,
    lang: Lang,
    /// The live vault location, which is what `main` writes to `config.toml` on
    /// the way out. `None` means the default — the same thing an absent key
    /// means, so a vault moved back to the default writes no key rather than
    /// writing the default path out longhand.
    vault: Option<String>,
    /// Where the user pointed the picker, once it has been vetted.
    chosen: Option<PathBuf>,
    files: usize,
    bytes: u64,
    /// A move is in flight. Claimed *before* the picker opens, which is the
    /// correction `0445eb3` paid for on the export: a flag claimed afterwards
    /// leaves the action dead for the rest of the session if the dialog is
    /// cancelled.
    running: bool,
    shared: Arc<Mutex<Shared>>,
}

/// The four owners of the products root, so that a move can retarget all of
/// them from one place.
///
/// Chron6 arrived at this arrangement for the language and wrote down why: a
/// shared cell cannot be held, because `viewer::State` lives behind an
/// `Arc<Mutex<_>>` captured into the render worker's `Send` sink. So each owner
/// keeps a plain copy, and the risk of a forgotten copy is answered by there
/// being exactly one caller.
struct Owners {
    vault: Rc<RefCell<Vault>>,
    viewer: Rc<Viewer>,
    editors: Editors,
    exports: Exports,
}

/// What the language switch and `main` reach the move through.
#[derive(Clone)]
pub struct Relocations {
    state: Rc<RefCell<State>>,
}

impl Relocations {
    /// The vault location as the session last knew it, for `Session`.
    pub fn current(&self) -> Option<String> {
        self.state.borrow().vault.clone()
    }

    /// Chron6's switch calls this.
    ///
    /// Only the composed strings need saying again — the sheet's labels are
    /// bound to `Strings` and follow `apply_strings` like the rest of the
    /// window. A *finished* move's status is cleared rather than re-composed,
    /// for the reason `Exports::set_lang` gives: it is a sentence about
    /// something that already happened.
    pub fn set_lang(&self, app: &AppWindow, lang: Lang) {
        self.state.borrow_mut().lang = lang;
        let state = self.state.borrow();
        if state.running {
            push_progress(app, &state);
        } else if state.chosen.is_some() {
            app.set_relocate_summary(summary(&state).into());
            app.set_relocate_status(slint::SharedString::new());
            app.set_relocate_failed(false);
        }
    }
}

/// Point every owner of the products root at the new vault, then re-read it.
fn retarget(app: &AppWindow, owners: &Owners, paths: &Paths) {
    owners
        .vault
        .borrow_mut()
        .set_products_root(paths.products.clone());
    owners.viewer.set_products_root(paths.products.clone());
    owners.editors.set_products_root(paths.products.clone());
    owners.exports.set_products_root(paths.products.clone());
    // Fifth, and not a products root: the About pane names where the vault is,
    // and a pane still naming the old folder is exactly the hidden state CORE §3
    // says there is not.
    crate::about::set_vault(app, Some(paths.vault.as_path()));
    // One pass through the vault re-reads the disk at the new root and rebuilds
    // everything derived from the selection, which is the same single route
    // Chron6 established for a language switch.
    crate::vault::rescan(&owners.vault, app, &owners.viewer, None);
}

/// "5 documents · 24.1 MB", composed here because the string table holds no
/// interpolation.
fn summary(state: &State) -> String {
    format!(
        "{} {} · {}",
        state.files,
        strings::get(state.lang, Key::RelocateDocuments),
        human_bytes(state.bytes)
    )
}

/// Bytes at one decimal place, in the largest unit that leaves a number a person
/// can hold in their head. Not translated: these are SI symbols.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn push_progress(app: &AppWindow, state: &State) {
    let Ok(shared) = state.shared.lock() else {
        return;
    };
    let progress = &shared.progress;
    // By bytes rather than by file count: five files of wildly different sizes
    // make a per-file fraction jump and then sit still.
    let fraction = if progress.bytes_total == 0 {
        0.0
    } else {
        progress.bytes_done as f32 / progress.bytes_total as f32
    };
    app.set_relocate_fraction(fraction);
    app.set_relocate_progress(format!("{} / {}", progress.files_done, progress.files_total).into());
    app.set_relocate_current(progress.current.clone().into());
    app.set_relocate_status(strings::get(state.lang, Key::RelocateMoving).into());
    app.set_relocate_failed(false);
}

/// Render a [`Refusal`] through the string table.
pub fn describe_refusal(lang: Lang, refusal: &Refusal) -> String {
    strings::get(
        lang,
        match refusal {
            Refusal::Same => Key::ErrVaultSame,
            Refusal::Inside => Key::ErrVaultInside,
            Refusal::Occupied => Key::ErrVaultOccupied,
            Refusal::NotUtf8 => Key::ErrVaultNotUtf8,
        },
    )
    .to_string()
}

/// Render a [`Failure`] through the string table.
///
/// The trailing detail is the OS's own message and stays as it is, the same way
/// `vault::describe`, `viewer::describe` and `export::describe` treat theirs.
pub fn describe(lang: Lang, failure: &Failure) -> String {
    let (key, detail) = match failure {
        Failure::Read(detail) => (Key::ErrVaultRead, detail),
        Failure::Write { file, detail } => {
            return format!(
                "{}: {}: {file}: {detail}",
                strings::get(lang, Key::ErrVaultMoveFailed),
                strings::get(lang, Key::ErrVaultWrite)
            );
        }
        Failure::Verify(detail) => (Key::ErrVaultVerify, detail),
    };
    format!(
        "{}: {}: {detail}",
        strings::get(lang, Key::ErrVaultMoveFailed),
        strings::get(lang, key)
    )
}

/// Wire the vault-location entry, its sheet and its worker into the window.
pub fn install(
    app: &AppWindow,
    paths: Paths,
    lang: Lang,
    vault: Rc<RefCell<Vault>>,
    viewer: Rc<Viewer>,
    editors: Editors,
    exports: Exports,
) -> Relocations {
    let state = Rc::new(RefCell::new(State {
        vault: if paths.is_configured() {
            paths.vault.to_str().map(str::to_string)
        } else {
            None
        },
        paths,
        lang,
        chosen: None,
        files: 0,
        bytes: 0,
        running: false,
        shared: Arc::new(Mutex::new(Shared::default())),
    }));
    let owners = Rc::new(Owners {
        vault,
        viewer,
        editors,
        exports,
    });

    app.set_relocate_available(true);

    app.on_choose_vault({
        let state = Rc::clone(&state);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            if state.borrow().running {
                return;
            }
            let from = state.borrow().paths.vault.clone();
            let title = strings::get(state.borrow().lang, Key::RelocateTitle).to_string();

            pick_folder(app.window(), &title, {
                let state = Rc::clone(&state);
                let weak = weak.clone();
                move |chosen| {
                    let Some(app) = weak.upgrade() else { return };
                    let Some(chosen) = chosen else { return };

                    let lang = state.borrow().lang;
                    if let Err(refusal) = vet(&from, &chosen) {
                        // Refused before anything is measured, let alone
                        // copied. The sheet opens holding the reason rather
                        // than a notice appearing somewhere else.
                        let mut state = state.borrow_mut();
                        state.chosen = None;
                        state.files = 0;
                        state.bytes = 0;
                        drop(state);
                        open_sheet(&app, &from, &chosen, "");
                        app.set_relocate_status(describe_refusal(lang, &refusal).into());
                        app.set_relocate_failed(true);
                        return;
                    }

                    match survey(&from) {
                        Ok(found) => {
                            let mut state = state.borrow_mut();
                            state.files = found.files;
                            state.bytes = found.bytes;
                            state.chosen = Some(chosen.clone());
                            let text = summary(&state);
                            drop(state);
                            open_sheet(&app, &from, &chosen, &text);
                        }
                        Err(failure) => {
                            state.borrow_mut().chosen = None;
                            open_sheet(&app, &from, &chosen, "");
                            app.set_relocate_status(describe(lang, &failure).into());
                            app.set_relocate_failed(true);
                        }
                    }
                }
            });
        }
    });

    app.on_relocate_confirm({
        let state = Rc::clone(&state);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut borrowed = state.borrow_mut();
            if borrowed.running {
                return;
            }
            let Some(to) = borrowed.chosen.clone() else {
                return;
            };
            let from = borrowed.paths.vault.clone();

            // Checked again, not only when the folder was picked: the sheet can
            // sit open indefinitely, and a destination that gained its own
            // `products/` in that window — a sync client materialising another
            // machine's vault, a second Parachron instance — must not be
            // silently merged into.
            if let Err(refusal) = vet(&from, &to) {
                let lang = borrowed.lang;
                borrowed.chosen = None;
                drop(borrowed);
                app.set_relocate_status(describe_refusal(lang, &refusal).into());
                app.set_relocate_failed(true);
                return;
            }

            // Surveyed again rather than reused: the sheet may have been sitting
            // open, and the worker's totals are what the progress bar divides by.
            let survey = match survey(&from) {
                Ok(survey) => survey,
                Err(failure) => {
                    let lang = borrowed.lang;
                    drop(borrowed);
                    app.set_relocate_status(describe(lang, &failure).into());
                    app.set_relocate_failed(true);
                    return;
                }
            };

            borrowed.running = true;
            borrowed.files = survey.files;
            borrowed.bytes = survey.bytes;
            if let Ok(mut shared) = borrowed.shared.lock() {
                *shared = Shared::default();
            }
            let shared = Arc::clone(&borrowed.shared);
            let lang = borrowed.lang;
            let config_path = borrowed.paths.config.clone();
            let data_dir = borrowed.paths.data.clone();
            drop(borrowed);

            app.set_relocate_running(true);
            app.set_relocate_fraction(0.0);
            app.set_relocate_progress(slint::SharedString::new());
            app.set_relocate_current(slint::SharedString::new());
            app.set_relocate_status(strings::get(lang, Key::RelocateMoving).into());
            app.set_relocate_failed(false);

            commit(
                Job { from, to },
                survey,
                config_path,
                data_dir,
                shared,
                app.as_weak(),
            );
        }
    });

    app.on_relocate_cancel({
        let state = Rc::clone(&state);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            // A move in flight is not cancellable. There is no half of a copy
            // that is safe to abandon, and a Cancel that quietly did nothing
            // would be worse than one that is not there — the sheet hides the
            // button while it runs, and this is the other half of that.
            if state.borrow().running {
                return;
            }
            state.borrow_mut().chosen = None;
            app.set_relocate_done(false);
            app.set_relocate_open(false);
        }
    });

    app.on_relocate_progressed({
        let state = Rc::clone(&state);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let state = state.borrow();
            if state.running {
                push_progress(&app, &state);
            }
        }
    });

    app.on_relocate_finished({
        let state = Rc::clone(&state);
        let owners = Rc::clone(&owners);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let outcome = {
                let borrowed = state.borrow();
                borrowed.shared.lock().ok().and_then(|s| s.outcome.clone())
            };
            let Some(outcome) = outcome else { return };

            let mut borrowed = state.borrow_mut();
            borrowed.running = false;
            let lang = borrowed.lang;
            app.set_relocate_running(false);

            match outcome {
                Outcome::Moved { to, .. } => {
                    // The session's own copy of the config is updated only now
                    // — `main` reads `Relocations::current` on the way out, and
                    // `run` has separately already written `config.toml` itself
                    // as a crash-safety net. A vault moved back to the default
                    // writes no key at all rather than the default path
                    // longhand, whether it started as the default or was a
                    // configured vault moved back — the destination is what
                    // decides this, not where the vault happened to start.
                    let default = to == borrowed.paths.data;
                    borrowed.paths = borrowed
                        .paths
                        .clone()
                        .with_vault(to.to_str().filter(|_| !default));
                    borrowed.vault = if default {
                        None
                    } else {
                        to.to_str().map(str::to_string)
                    };
                    borrowed.chosen = None;
                    let paths = borrowed.paths.clone();
                    drop(borrowed);

                    retarget(&app, &owners, &paths);
                    // The sheet stays up saying so. A copy that ran for minutes
                    // and then made the window blink leaves the user unable to
                    // tell "it worked" from "it gave up", and the list quietly
                    // repopulating is not an answer to a question that anxious.
                    app.set_relocate_from(paths.vault.display().to_string().into());
                    app.set_relocate_status(strings::get(lang, Key::RelocateDone).into());
                    app.set_relocate_failed(false);
                    app.set_relocate_done(true);
                }
                Outcome::Failed(failure) => {
                    borrowed.chosen = None;
                    drop(borrowed);
                    // The sheet stays open holding the reason, naming the file
                    // it stopped on. The original vault is untouched and the
                    // config still points at it.
                    app.set_relocate_status(describe(lang, &failure).into());
                    app.set_relocate_failed(true);
                }
            }
        }
    });

    Relocations { state }
}

fn open_sheet(app: &AppWindow, from: &Path, to: &Path, summary: &str) {
    app.set_relocate_from(from.display().to_string().into());
    app.set_relocate_to(to.display().to_string().into());
    app.set_relocate_summary(summary.into());
    app.set_relocate_status(slint::SharedString::new());
    app.set_relocate_failed(false);
    app.set_relocate_done(false);
    app.set_relocate_running(false);
    app.set_relocate_fraction(0.0);
    app.set_relocate_open(true);
}

/// Ask for a folder. `done` runs on the UI thread with the chosen path, or with
/// `None` if the dialog was cancelled.
///
/// A thin edge, exactly as `import::pick` and `export::pick_destination` are, and
/// for the reason Chron3 wrote down: a portal dialog is drawn by the desktop's
/// own portal service in the user's session, so it appears on the real display
/// whatever `DISPLAY` says and cannot be driven under `Xvfb`. Everything past it
/// takes a `PathBuf`, so `vet`, `survey` and `run` are all testable by handing
/// them paths and only the click that opens the dialog needs a person.
pub fn pick_folder(
    window: &slint::Window,
    title: &str,
    done: impl FnOnce(Option<PathBuf>) + 'static,
) {
    let handle = window.window_handle();
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_parent(&handle);

    let _ = slint::spawn_local(async move {
        let chosen = dialog.pick_folder().await;
        done(chosen.map(|folder| folder.path().to_path_buf()));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vault with two products, five files between them.
    fn seed(root: &Path) {
        let products = root.join("products");
        for (folder, files) in [
            (
                "sarj-cihazi",
                vec!["product.toml", "invoice.pdf", "warranty.pdf"],
            ),
            ("ironwolf-pro-6tb", vec!["product.toml", "invoice.pdf"]),
        ] {
            let dir = products.join(folder);
            fs::create_dir_all(&dir).unwrap();
            for (n, file) in files.iter().enumerate() {
                // Distinct lengths, so `verify`'s size check is actually
                // checking something rather than comparing zero to zero.
                fs::write(dir.join(file), "x".repeat(n + 1)).unwrap();
            }
        }
    }

    fn collector() -> (RefCell<Vec<Progress>>, impl Fn(Progress) + use<>) {
        let seen = RefCell::new(Vec::new());
        (seen, |_p: Progress| {})
    }

    /// A harmless place for `persist_vault` to write during a test: inside a
    /// tempdir that outlives the move and is never inspected by these tests.
    fn no_config(root: &Path) -> (PathBuf, PathBuf) {
        (root.join("config.toml"), root.join("unused-data-dir"))
    }

    // -- vet -------------------------------------------------------------

    #[test]
    fn the_current_vault_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(vet(dir.path(), dir.path()), Err(Refusal::Same));
    }

    /// `/vault` and `/vault/.` are the same directory and must give the same
    /// answer — a refusal that a trailing dot slips past is not a refusal.
    #[test]
    fn the_current_vault_is_refused_however_it_is_spelled() {
        let dir = tempfile::tempdir().unwrap();
        let dotted = dir.path().join(".");
        assert_eq!(vet(dir.path(), &dotted), Err(Refusal::Same));
    }

    /// The destructive one. Copy-then-remove into a folder under the source
    /// writes the copy underneath the thing it is about to delete.
    #[test]
    fn a_folder_inside_the_current_vault_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let inside = dir.path().join("somewhere/deeper");
        fs::create_dir_all(&inside).unwrap();
        assert_eq!(vet(dir.path(), &inside), Err(Refusal::Inside));
    }

    #[test]
    fn a_folder_that_already_holds_a_vault_is_refused() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        fs::create_dir_all(to.path().join("products")).unwrap();
        assert_eq!(vet(from.path(), to.path()), Err(Refusal::Occupied));
    }

    /// A sibling directory whose name merely *starts with* the vault's name is
    /// not inside it. `starts_with` on a `Path` compares components rather than
    /// characters, and this is the test that says so on purpose.
    #[test]
    fn a_sibling_with_a_similar_name_is_not_inside_the_vault() {
        let base = tempfile::tempdir().unwrap();
        let from = base.path().join("vault");
        let to = base.path().join("vault-two");
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        assert_eq!(vet(&from, &to), Ok(()));
    }

    #[cfg(unix)]
    #[test]
    fn a_destination_that_is_not_utf8_is_refused_rather_than_mangled() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let from = tempfile::tempdir().unwrap();
        let mut name = OsString::from_vec(vec![b'v', b'a', b'u', b'l', b't', 0xff]);
        name.push("");
        let to = from.path().parent().unwrap().join(name);

        // Refused because `config.toml` could not record it, not because of
        // anything about the directory itself.
        assert_eq!(vet(from.path(), &to), Err(Refusal::NotUtf8));
    }

    // -- survey ----------------------------------------------------------

    #[test]
    fn the_survey_counts_every_file_and_every_byte_before_anything_moves() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path());

        let survey = survey(dir.path()).unwrap();

        assert_eq!(survey.files, 5);
        // 1+2+3 for the first product, 1+2 for the second.
        assert_eq!(survey.bytes, 9);
    }

    #[test]
    fn an_empty_vault_surveys_to_nothing_rather_than_failing() {
        let dir = tempfile::tempdir().unwrap();
        let survey = survey(dir.path()).unwrap();
        assert_eq!((survey.files, survey.bytes), (0, 0));
    }

    // -- the move --------------------------------------------------------

    #[test]
    fn a_move_within_one_filesystem_relocates_every_file() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let (_seen, report) = collector();
        let (config_path, data_dir) = no_config(from.path());
        let outcome = run(
            Job {
                from: from.path().to_path_buf(),
                to: to.path().to_path_buf(),
            },
            survey,
            &config_path,
            &data_dir,
            &report,
        );

        assert!(matches!(outcome, Outcome::Moved { files: 5, .. }));
        assert!(
            to.path()
                .join("products/sarj-cihazi/warranty.pdf")
                .is_file()
        );
        assert!(
            to.path()
                .join("products/ironwolf-pro-6tb/invoice.pdf")
                .is_file()
        );
        // The source is gone rather than merely emptied — a leftover `products/`
        // would make the old location look like an empty vault.
        assert!(!from.path().join("products").exists());
    }

    /// The copy path, exercised without needing two filesystems.
    ///
    /// `run` takes `rename` when it can, so the branch this milestone was asked
    /// for — the cross-disk one — is the branch a same-disk test never reaches.
    /// Calling the copy directly is what makes it testable at all; the genuinely
    /// cross-device case is checked by hand against `/dev/shm` and recorded in
    /// the verification section.
    #[test]
    fn the_copy_path_reproduces_the_vault_and_verifies_it() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let (_seen, report) = collector();
        copy_all(
            &from.path().join("products"),
            &to.path().join("products"),
            &survey,
            &report,
        )
        .expect("the copy must succeed and verify");

        for relative in [
            "sarj-cihazi/product.toml",
            "sarj-cihazi/invoice.pdf",
            "sarj-cihazi/warranty.pdf",
            "ironwolf-pro-6tb/product.toml",
            "ironwolf-pro-6tb/invoice.pdf",
        ] {
            let copied = to.path().join("products").join(relative);
            assert!(copied.is_file(), "{relative} did not arrive");
        }
        // The source is untouched by the copy — removal is a separate step, and
        // that separation is the whole safety argument.
        assert!(
            from.path()
                .join("products/sarj-cihazi/invoice.pdf")
                .is_file()
        );
    }

    #[test]
    fn progress_is_reported_once_per_file_and_ends_at_the_total() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let seen = RefCell::new(Vec::new());
        copy_all(
            &from.path().join("products"),
            &to.path().join("products"),
            &survey,
            &|p| seen.borrow_mut().push(p),
        )
        .unwrap();

        let seen = seen.into_inner();
        assert_eq!(seen.len(), 5, "one message per file, not per chunk");
        // Monotonic and finishing at the total, which is what a determinate bar
        // needs and what a bar that jumps backwards does not have.
        for (index, progress) in seen.iter().enumerate() {
            assert_eq!(progress.files_done, index + 1);
            assert_eq!(progress.files_total, 5);
            assert!(!progress.current.is_empty(), "the file name is shown");
        }
        let last = seen.last().unwrap();
        assert_eq!(last.bytes_done, last.bytes_total);
        assert_eq!(last.bytes_done, 9);
    }

    // -- the invariant ---------------------------------------------------

    /// The test this module exists for.
    ///
    /// A move that fails half way must leave the original vault complete. Not
    /// "mostly complete", and not "recoverable from a partial destination" —
    /// complete, because the user's next launch reads `config.toml`, which still
    /// names the old location.
    #[cfg(unix)]
    #[test]
    fn a_move_that_fails_leaves_the_original_vault_untouched() {
        use std::os::unix::fs::PermissionsExt;

        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        // A destination that accepts a directory and refuses a file in it.
        let target = to.path().join("readonly");
        fs::create_dir_all(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o555)).unwrap();

        let (_seen, report) = collector();
        let (config_path, data_dir) = no_config(from.path());
        let outcome = run(
            Job {
                from: from.path().to_path_buf(),
                to: target.clone(),
            },
            survey,
            &config_path,
            &data_dir,
            &report,
        );

        // Restored before any assertion can fail, or the temp dir will not clean
        // up and the failure message will be about that instead.
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            matches!(outcome, Outcome::Failed(Failure::Write { .. })),
            "expected a write failure, got {outcome:?}"
        );

        // Every original file, still there, still the right size.
        let after = super::survey(from.path()).unwrap();
        assert_eq!(after.files, 5, "the source vault lost files");
        assert_eq!(after.bytes, 9, "the source vault lost bytes");
        assert!(
            from.path()
                .join("products/sarj-cihazi/warranty.pdf")
                .is_file()
        );
    }

    /// The partial destination is cleaned up rather than left as a half vault
    /// that a later move would refuse as `Occupied`.
    #[cfg(unix)]
    #[test]
    fn a_failed_move_leaves_no_half_written_vault_behind() {
        use std::os::unix::fs::PermissionsExt;

        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let target = to.path().join("readonly");
        fs::create_dir_all(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o555)).unwrap();

        let (_seen, report) = collector();
        let (config_path, data_dir) = no_config(from.path());
        let _ = run(
            Job {
                from: from.path().to_path_buf(),
                to: target.clone(),
            },
            survey,
            &config_path,
            &data_dir,
            &report,
        );
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            !target.join("products").exists(),
            "a partial vault was left where the next attempt would call it Occupied"
        );
    }

    /// `verify` compares sizes, so a truncated copy is caught before the source
    /// is deleted. Constructed by hand, because producing a short write on
    /// demand is not something a test can ask a filesystem for.
    #[test]
    fn verification_catches_a_file_that_did_not_arrive_whole() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let source = from.path().join("products");
        let target = to.path().join("products");
        let (_seen, report) = collector();
        copy_all(&source, &target, &survey, &report).unwrap();

        // Shorten one file behind the copy's back and re-verify.
        let victim = target.join("sarj-cihazi/warranty.pdf");
        fs::write(&victim, "").unwrap();

        let failure =
            verify(&source, &target, &survey).expect_err("a truncated copy must not verify");
        assert!(matches!(failure, Failure::Verify(_)), "{failure:?}");
    }

    /// A file appearing in the source while the move runs is the one way this
    /// can lose a document with no step having failed, so it is caught rather
    /// than assumed away.
    #[test]
    fn verification_catches_the_vault_changing_underneath_the_move() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let source = from.path().join("products");
        let target = to.path().join("products");
        let (_seen, report) = collector();
        copy_all(&source, &target, &survey, &report).unwrap();

        fs::write(source.join("sarj-cihazi/late-arrival.pdf"), "new").unwrap();

        let failure =
            verify(&source, &target, &survey).expect_err("a source that grew must not verify");
        assert!(matches!(failure, Failure::Verify(_)), "{failure:?}");
    }

    /// The branch this milestone was actually asked for, against two real
    /// filesystems.
    ///
    /// Every other test here runs inside one `/tmp`, where `fs::rename` succeeds
    /// and the copy path is never reached — so on its own the suite would prove
    /// the fast path twice and the slow path never. `/dev/shm` is a tmpfs and is
    /// mounted on any ordinary Linux system, which makes a genuine `EXDEV`
    /// available for the price of a directory.
    ///
    /// It asserts the two really are different devices before it proves
    /// anything, because a machine where they are not would otherwise report a
    /// pass for a path it never ran. If `/dev/shm` is missing or unwritable the
    /// test says so and stops, rather than passing quietly — a skip nobody sees
    /// is how a test rots.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_move_across_two_real_filesystems_takes_the_copy_path() {
        use std::os::linux::fs::MetadataExt;

        let shm = Path::new("/dev/shm");
        if !shm.is_dir() {
            eprintln!("skipped: /dev/shm is not there, so EXDEV cannot be produced");
            return;
        }
        let Ok(to) = tempfile::tempdir_in(shm) else {
            eprintln!("skipped: /dev/shm is not writable");
            return;
        };
        let from = tempfile::tempdir().unwrap();
        seed(from.path());

        let here = fs::metadata(from.path()).unwrap().st_dev();
        let there = fs::metadata(to.path()).unwrap().st_dev();
        if here == there {
            eprintln!("skipped: /tmp and /dev/shm are the same device on this machine");
            return;
        }

        // The premise, proven rather than assumed: a bare rename cannot do this.
        assert!(
            fs::rename(from.path().join("products"), to.path().join("products")).is_err(),
            "these two paths are on one filesystem after all, so this proves nothing"
        );

        let survey = survey(from.path()).unwrap();
        let seen = RefCell::new(Vec::new());
        let (config_path, data_dir) = no_config(from.path());
        let outcome = run(
            Job {
                from: from.path().to_path_buf(),
                to: to.path().to_path_buf(),
            },
            survey,
            &config_path,
            &data_dir,
            &|p| seen.borrow_mut().push(p),
        );

        assert!(
            matches!(outcome, Outcome::Moved { files: 5, .. }),
            "the copy path did not complete: {outcome:?}"
        );
        assert_eq!(
            fs::read_to_string(to.path().join("products/sarj-cihazi/warranty.pdf")).unwrap(),
            "xxx",
            "a file crossed the filesystem boundary with its contents changed"
        );
        assert!(
            !from.path().join("products").exists(),
            "the source survived a move that reported success"
        );
        assert_eq!(
            seen.into_inner().len(),
            5,
            "progress was reported for the copy path too, not only for rename"
        );
    }

    #[test]
    fn moving_an_empty_vault_succeeds_and_reports_nothing_moved() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        fs::create_dir_all(from.path().join("products")).unwrap();
        let survey = survey(from.path()).unwrap();

        let (_seen, report) = collector();
        let (config_path, data_dir) = no_config(from.path());
        let outcome = run(
            Job {
                from: from.path().to_path_buf(),
                to: to.path().to_path_buf(),
            },
            survey,
            &config_path,
            &data_dir,
            &report,
        );

        assert!(matches!(outcome, Outcome::Moved { files: 0, .. }));
        assert!(to.path().join("products").is_dir());
    }

    // -- config persistence -----------------------------------------------

    /// The bug this module used to have: a crash between a successful move and
    /// the next clean exit left `config.toml` naming a vault whose `products/`
    /// had already been deleted. `run` now writes the new location itself,
    /// before the old vault is touched, rather than relying solely on the
    /// window-close save.
    #[test]
    fn a_successful_move_records_the_new_vault_before_the_source_is_removed() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let config_path = from.path().join("config.toml");
        // Anything other than `to` — a real install's data directory does not
        // move, so a distinct path here matches every genuine relocation.
        let data_dir = from.path().to_path_buf();

        let (_seen, report) = collector();
        let outcome = run(
            Job {
                from: from.path().to_path_buf(),
                to: to.path().to_path_buf(),
            },
            survey,
            &config_path,
            &data_dir,
            &report,
        );
        assert!(matches!(outcome, Outcome::Moved { .. }));

        let saved = Config::load(&config_path).unwrap();
        assert_eq!(saved.vault.as_deref(), to.path().to_str());
    }

    /// A move that lands exactly on the platform's data directory writes no
    /// `vault` key at all — the same thing an absent key already means — rather
    /// than the default path spelled out longhand.
    #[test]
    fn a_move_to_the_platform_default_records_no_vault_key() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let config_path = from.path().join("config.toml");

        let (_seen, report) = collector();
        let outcome = run(
            Job {
                from: from.path().to_path_buf(),
                to: to.path().to_path_buf(),
            },
            survey,
            &config_path,
            to.path(),
            &report,
        );
        assert!(matches!(outcome, Outcome::Moved { .. }));

        let saved = Config::load(&config_path).unwrap();
        assert_eq!(saved.vault, None);
    }

    // -- the destination must already exist --------------------------------

    /// `create_dir_all` would rebuild a chain of missing ancestors without
    /// complaint, and a drive that came unmounted between the picker and the
    /// move presents exactly that way: an ordinary, creatable directory at the
    /// mount point. `copy_all` must refuse rather than build the vault there.
    #[test]
    fn copy_all_refuses_a_destination_whose_parent_does_not_exist() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let unmounted = to.path().join("not-actually-mounted");
        let (_seen, report) = collector();
        let failure = copy_all(
            &from.path().join("products"),
            &unmounted.join("products"),
            &survey,
            &report,
        )
        .expect_err("a destination that does not exist must not be created");
        assert!(matches!(failure, Failure::Write { .. }), "{failure:?}");
        assert!(
            !unmounted.exists(),
            "the missing mount point must not have been resurrected as an ordinary directory"
        );
    }

    // -- symlink-safe directory creation ------------------------------------

    #[cfg(unix)]
    #[test]
    fn create_dir_without_following_symlinks_refuses_a_symlink_standing_in_for_a_directory() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        symlink(elsewhere.path(), root.path().join("sarj-cihazi")).unwrap();

        let err = create_dir_without_following_symlinks(root.path(), Path::new("sarj-cihazi"))
            .expect_err("a symlink standing in for the directory must be refused, not trusted");
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }

    /// A product with more than one document calls this twice for the same
    /// folder — the second call must not fail just because the first already
    /// created it.
    #[test]
    fn create_dir_without_following_symlinks_is_idempotent_for_a_real_directory() {
        let root = tempfile::tempdir().unwrap();
        create_dir_without_following_symlinks(root.path(), Path::new("sarj-cihazi")).unwrap();
        create_dir_without_following_symlinks(root.path(), Path::new("sarj-cihazi")).unwrap();
        assert!(root.path().join("sarj-cihazi").is_dir());
    }

    // -- symlinks in the source vault ---------------------------------------

    /// A symlink's own `symlink_metadata` length is the byte length of the
    /// link's target *text*, not of the file it points at. Recording that and
    /// then having the copy follow the link — which it must, to copy the file
    /// the link stands in for — would hand `verify` a number that can never
    /// match what was actually written.
    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_file_surveys_at_the_size_of_what_it_points_to() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(products.join("sarj-cihazi")).unwrap();
        let real = dir.path().join("real-invoice.pdf");
        fs::write(&real, "abcdefghij").unwrap();
        symlink(&real, products.join("sarj-cihazi/invoice.pdf")).unwrap();

        let survey = survey(dir.path()).unwrap();
        assert_eq!(survey.files, 1);
        assert_eq!(
            survey.bytes, 10,
            "the symlink's own path length was recorded instead of its target's size"
        );
    }

    /// A link to nothing cannot be "copied as whatever it points at" — the one
    /// thing this module's own doc comment says a symlink in the vault gets.
    /// Refusing it up front is the same choice this module makes everywhere
    /// else a move cannot proceed safely: loud, and before a byte moves.
    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_in_the_vault_is_refused_rather_than_silently_mis_measured() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        fs::create_dir_all(products.join("sarj-cihazi")).unwrap();
        symlink(
            dir.path().join("nowhere.pdf"),
            products.join("sarj-cihazi/invoice.pdf"),
        )
        .unwrap();

        let Err(failure) = survey(dir.path()) else {
            panic!("a link to nothing must not be silently sized as zero");
        };
        assert!(matches!(failure, Failure::Read(_)));
    }

    // -- verify sees more than a count ---------------------------------------

    /// A file replaced by a same-sized, differently-named one leaves the count
    /// unchanged — exactly what a sync client's own replace pattern (delete an
    /// old temp name, create the final one) looks like. The count-only check
    /// this module used to have would sail straight through it.
    #[test]
    fn verification_catches_a_same_count_replacement_underneath_the_move() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        seed(from.path());
        let survey = survey(from.path()).unwrap();

        let source = from.path().join("products");
        let target = to.path().join("products");
        let (_seen, report) = collector();
        copy_all(&source, &target, &survey, &report).unwrap();

        fs::remove_file(source.join("sarj-cihazi/warranty.pdf")).unwrap();
        fs::write(source.join("sarj-cihazi/warranty-2.pdf"), "xxx").unwrap();

        let failure = verify(&source, &target, &survey)
            .expect_err("a same-sized file under a different name must not verify");
        assert!(matches!(failure, Failure::Verify(_)), "{failure:?}");
    }
}
