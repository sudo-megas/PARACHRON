//! Turning PDF pages into pixels, off the UI thread.
//!
//! MuPDF's context is per-thread and none of its handles are `Send` — in this
//! crate's own words, only `BaseContext` carries that marker. So a single
//! worker thread owns every document, page and pixmap for the lifetime of the
//! app, and the only things that cross back to the UI are plain bytes.
//!
//! Nothing here may panic on a bad file. Every failure becomes a [`ViewError`]
//! that `main.rs` renders through the string table.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;

use mupdf::pdf::PdfDocument;
use mupdf::{Colorspace, Document, Matrix};

/// How much decoded page imagery to keep around, in bytes.
const CACHE_BUDGET: usize = 64 * 1024 * 1024;

/// Why a document could not be shown.
///
/// Typed rather than pre-rendered, for the same reason as `data::DataError`:
/// a message built here would be a user-visible English literal living outside
/// `strings.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    /// Listed in `product.toml` but not on disk.
    Missing,
    /// The file exists but could not be read; carries the OS message.
    Unreadable(String),
    /// Not a PDF, or damaged beyond opening.
    NotAPdf(String),
    /// Password-protected. Parachron shows the file's state rather than
    /// prompting — a password dialog would be its own feature.
    Encrypted,
    /// Structurally valid but holding no pages at all.
    NoPages,
    /// The document opened but this page would not rasterize.
    RenderFailed(String),
}

/// One rasterized page, ready to hand to Slint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA, four bytes per pixel.
    pub rgba: Vec<u8>,
}

impl Raster {
    fn bytes(&self) -> usize {
        self.rgba.len()
    }
}

/// A request to show one page of one document at one size.
///
/// There is deliberately only one request kind: the page count comes back with
/// the image, which makes superseding trivial — of any batch waiting in the
/// channel, only the highest token can still matter.
#[derive(Debug, Clone)]
struct Job {
    token: u64,
    path: PathBuf,
    page: usize,
    /// Target box in physical pixels. The page is fitted inside it, preserving
    /// aspect; zoom is already folded in by the caller, so zoom 2× simply asks
    /// for a box twice the pane.
    target_width: u32,
    target_height: u32,
}

enum Message {
    Render(Job),
    /// Forget everything remembered about this path, because the bytes behind
    /// it have changed (Chron3 imports and deletes files).
    Invalidate(PathBuf),
    Shutdown,
}

/// What the worker sends back. Every field is `Send` — no MuPDF type escapes.
#[derive(Debug, Clone)]
pub enum Response {
    Ready {
        token: u64,
        page: usize,
        pages: usize,
        raster: Raster,
    },
    Failed {
        token: u64,
        error: ViewError,
    },
}

/// Handle to the render worker. Dropping it stops the thread.
pub struct Renderer {
    tx: Sender<Message>,
}

impl Renderer {
    /// Start the worker. `sink` is called on the worker thread for every
    /// response — callers are expected to hop to the UI thread inside it.
    pub fn spawn(sink: impl Fn(Response) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();

        thread::Builder::new()
            .name("parachron-render".to_string())
            .spawn(move || {
                let mut open: Option<OpenDocument> = None;
                let mut cache = Cache::default();

                while let Ok(message) = rx.recv() {
                    let mut job = match message {
                        Message::Shutdown => break,
                        Message::Invalidate(path) => {
                            forget(&mut open, &mut cache, &path);
                            continue;
                        }
                        Message::Render(job) => job,
                    };

                    // Anything already queued supersedes this job: the user has
                    // moved on. A window drag emits a burst of resize requests
                    // and only the last one is worth the work.
                    //
                    // Invalidations are never superseded. They are statements
                    // about the disk rather than about what to draw, so every
                    // one that turns up is applied — dropping one would leave
                    // the worker serving bytes that no longer exist.
                    let mut stop = false;
                    for queued in rx.try_iter() {
                        match queued {
                            Message::Shutdown => stop = true,
                            Message::Invalidate(path) => forget(&mut open, &mut cache, &path),
                            Message::Render(newer) => job = newer,
                        }
                    }
                    if stop {
                        break;
                    }

                    sink(serve(&mut open, &mut cache, job));
                }
            })
            .expect("render worker thread must start");

        Self { tx }
    }

    /// Ask for a page. Silently ignored once the worker has stopped — a dead
    /// renderer must not take the window down with it.
    pub fn request(&self, token: u64, path: PathBuf, page: usize, target: (u32, u32)) {
        let _ = self.tx.send(Message::Render(Job {
            token,
            path,
            page,
            target_width: target.0,
            target_height: target.1,
        }));
    }

    /// Tell the worker the file at `path` is not what it used to be.
    ///
    /// Nothing here notices a file changing by itself: the cache is keyed by
    /// path and size with no modification time, and `ensure_open` reuses the
    /// open document whenever the path matches. Chron3 writes files into paths
    /// the viewer may already have seen, so it says so.
    pub fn invalidate(&self, path: &Path) {
        let _ = self.tx.send(Message::Invalidate(path.to_path_buf()));
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.tx.send(Message::Shutdown);
    }
}

/// The document the worker currently has open. Keeping one is enough: flipping
/// back to a previous tab is served by the page cache, and reopening costs far
/// less than rasterizing.
struct OpenDocument {
    path: PathBuf,
    document: Document,
    pages: usize,
}

/// Serve one job, from cache when possible.
fn serve(open: &mut Option<OpenDocument>, cache: &mut Cache, job: Job) -> Response {
    // The page count comes first, because it decides which page is even
    // askable for.
    let pages = match ensure_open(open, &job.path) {
        Ok(pages) => pages,
        Err(error) => {
            return Response::Failed {
                token: job.token,
                error,
            };
        }
    };

    // The requested page may be past the end. A file replaced on disk can be
    // shorter than the one whose page number is still on screen — import a
    // two-page warranty over a twelve-page one while the reader is on page
    // eight, and this is that. Landing on the last page beats refusing to draw,
    // and because the response carries the page it actually rendered rather
    // than the one that was asked for, the counter and the arrows come back
    // agreeing with it. `page_count` refuses a document with no pages at all,
    // so there is always one to land on.
    let page = job.page.min(pages.saturating_sub(1));

    let key = CacheKey {
        path: job.path.clone(),
        page,
        width: job.target_width,
        height: job.target_height,
    };

    if let Some(raster) = cache.get(&key) {
        return Response::Ready {
            token: job.token,
            page,
            pages,
            raster,
        };
    }

    let document = &open.as_ref().expect("just opened").document;
    match rasterize(document, page, job.target_width, job.target_height) {
        Ok(raster) => {
            cache.insert(key, raster.clone());
            Response::Ready {
                token: job.token,
                page: job.page,
                pages,
                raster,
            }
        }
        Err(error) => Response::Failed {
            token: job.token,
            error,
        },
    }
}

/// Drop everything the worker remembers about `path`.
///
/// Both halves matter. The cached rasters would keep serving the old pixels,
/// and the open document is a handle onto bytes that have since been replaced —
/// which is worse, because it also carries a stale page count.
fn forget(open: &mut Option<OpenDocument>, cache: &mut Cache, path: &Path) {
    if open.as_ref().is_some_and(|current| current.path == path) {
        *open = None;
    }
    cache.forget(path);
}

/// Make `open` hold `path`, reusing it when it already does. Returns the page
/// count.
fn ensure_open(open: &mut Option<OpenDocument>, path: &Path) -> Result<usize, ViewError> {
    if let Some(current) = open.as_ref()
        && current.path == path
    {
        return Ok(current.pages);
    }

    let document = open_document(path)?;
    let pages = page_count(&document)?;
    *open = Some(OpenDocument {
        path: path.to_path_buf(),
        document,
        pages,
    });
    Ok(pages)
}

/// Open a document, mapping every way it can fail onto [`ViewError`].
pub fn open_document(path: &Path) -> Result<Document, ViewError> {
    if !path.is_file() {
        return Err(ViewError::Missing);
    }

    let document = Document::open(path).map_err(|e| match e {
        mupdf::Error::Io(io) => ViewError::Unreadable(io.to_string()),
        other => ViewError::NotAPdf(other.to_string()),
    })?;

    // `needs_password` is the honest test: a document that wants one has not
    // really opened, and every later call would fail obscurely.
    match document.needs_password() {
        Ok(true) => return Err(ViewError::Encrypted),
        Ok(false) => {}
        Err(e) => return Err(ViewError::NotAPdf(e.to_string())),
    }

    Ok(document)
}

/// Open a document as a PDF, for export (Chron7).
///
/// The same three checks as [`open_document`], in the same order, mapped onto the
/// same [`ViewError`] — because the *verdicts* have to be phrased in one
/// vocabulary, and `import.rs` already argued that. It lives here rather than in
/// `export.rs` for that reason.
///
/// The two are not quite interchangeable, and the difference is a real one rather
/// than an oversight. CORE §2 builds MuPDF with `img`, so `Document::open`
/// recognises image formats that `PdfDocument::open` cannot: a scan saved as
/// `invoice.pdf` that is really a PNG opens in the viewer and is refused here. That
/// is correct — a PNG cannot be grafted into a PDF's page tree — and the export
/// names it among the documents it could not include, which is what CORE §6 asks
/// for. It is written down because "one answer in the app" would otherwise read as
/// a stronger promise than the code makes.
///
/// The third check is why this function exists at all. `PdfDocument::open` does
/// **not** refuse a password-protected file the way `Document::open` plus
/// `needs_password` does: it returns `Ok`, and only then admits it needs one.
/// Merging that would append a page whose content stream cannot be decrypted.
/// Verified against the `encrypted.pdf` fixture rather than assumed.
pub fn open_pdf(path: &Path) -> Result<PdfDocument, ViewError> {
    if !path.is_file() {
        return Err(ViewError::Missing);
    }

    let document = PdfDocument::open(path).map_err(|e| match e {
        mupdf::Error::Io(io) => ViewError::Unreadable(io.to_string()),
        other => ViewError::NotAPdf(other.to_string()),
    })?;

    match document.needs_password() {
        Ok(true) => return Err(ViewError::Encrypted),
        Ok(false) => {}
        Err(e) => return Err(ViewError::NotAPdf(e.to_string())),
    }

    // `PdfDocument` dereferences to `Document`, so the page-count check is the
    // same code the viewer runs rather than a second opinion about emptiness.
    page_count(&document)?;
    Ok(document)
}

/// Page count, rejecting documents that have none.
pub fn page_count(document: &Document) -> Result<usize, ViewError> {
    let pages = document
        .page_count()
        .map_err(|e| ViewError::NotAPdf(e.to_string()))?;

    if pages <= 0 {
        return Err(ViewError::NoPages);
    }
    Ok(pages as usize)
}

/// Rasterize one page, fitted inside `target_width × target_height` with its
/// aspect preserved.
pub fn rasterize(
    document: &Document,
    page: usize,
    target_width: u32,
    target_height: u32,
) -> Result<Raster, ViewError> {
    let page_no = i32::try_from(page).map_err(|e| ViewError::RenderFailed(e.to_string()))?;
    let loaded = document
        .load_page(page_no)
        .map_err(|e| ViewError::RenderFailed(e.to_string()))?;

    let bounds = loaded
        .bounds()
        .map_err(|e| ViewError::RenderFailed(e.to_string()))?;

    // Degenerate page boxes would divide by zero on the way to a scale factor.
    let (page_width, page_height) = (bounds.width(), bounds.height());
    if page_width <= 0.0 || page_height <= 0.0 {
        return Err(ViewError::RenderFailed(String::new()));
    }

    let scale = (target_width as f32 / page_width).min(target_height as f32 / page_height);
    let scale = scale.max(f32::MIN_POSITIVE);
    let matrix = Matrix::new_scale(scale, scale);

    // `alpha: false` gives an opaque, white-backed page — a transparent one
    // would let the dark panel show through the paper. `show_extras: true`
    // keeps annotations, which is where stamps and signatures live on invoices.
    let pixmap = loaded
        .to_pixmap(&matrix, &Colorspace::device_rgb(), false, true)
        .map_err(|e| ViewError::RenderFailed(e.to_string()))?;

    Ok(to_rgba(&pixmap))
}

/// Copy a pixmap into tightly packed RGBA, honouring its row stride.
fn to_rgba(pixmap: &mupdf::Pixmap) -> Raster {
    let width = pixmap.width();
    let height = pixmap.height();
    let components = pixmap.n() as usize;
    let stride = pixmap.stride().unsigned_abs();
    let samples = pixmap.samples();

    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        let row_start = y * stride;
        let row = &samples[row_start..row_start + width as usize * components];
        for pixel in row.chunks_exact(components) {
            let alpha = if components >= 4 { pixel[3] } else { 255 };
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], alpha]);
        }
    }

    Raster {
        width,
        height,
        rgba,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheKey {
    path: PathBuf,
    page: usize,
    width: u32,
    height: u32,
}

/// Most-recently-used-first list of rendered pages, bounded by total bytes.
#[derive(Default)]
struct Cache {
    entries: Vec<(CacheKey, Raster)>,
    bytes: usize,
}

impl Cache {
    fn get(&mut self, key: &CacheKey) -> Option<Raster> {
        let index = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(index);
        let raster = entry.1.clone();
        self.entries.insert(0, entry);
        Some(raster)
    }

    fn insert(&mut self, key: CacheKey, raster: Raster) {
        // A single page larger than the whole budget is not worth evicting
        // everything else for.
        if raster.bytes() > CACHE_BUDGET {
            return;
        }
        self.bytes += raster.bytes();
        self.entries.insert(0, (key, raster));

        while self.bytes > CACHE_BUDGET {
            match self.entries.pop() {
                Some((_, evicted)) => self.bytes -= evicted.bytes(),
                None => break,
            }
        }
    }

    /// Drop every page cached for one document.
    fn forget(&mut self, path: &Path) {
        let mut freed = 0;
        self.entries.retain(|(key, raster)| {
            let stale = key.path == path;
            if stale {
                freed += raster.bytes();
            }
            !stale
        });
        self.bytes = self.bytes.saturating_sub(freed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn a_single_page_document_reports_one_page() {
        let doc = open_document(&fixture("sample.pdf")).expect("sample must open");
        assert_eq!(page_count(&doc).unwrap(), 1);
    }

    #[test]
    fn a_multi_page_document_reports_every_page() {
        let doc = open_document(&fixture("multipage.pdf")).expect("multipage must open");
        assert_eq!(page_count(&doc).unwrap(), 3);
    }

    #[test]
    fn a_page_rasterizes_fitted_inside_the_target_box() {
        let doc = open_document(&fixture("sample.pdf")).unwrap();
        let raster = rasterize(&doc, 0, 600, 800).expect("page must render");

        // Fitted, not stretched: inside the box, and touching one of its sides.
        assert!(raster.width <= 600 && raster.height <= 800);
        assert!(raster.width == 600 || raster.height == 800);

        // A4 is taller than it is wide, so height is the binding constraint.
        assert_eq!(raster.height, 800);
        assert_eq!(
            raster.rgba.len(),
            raster.width as usize * raster.height as usize * 4
        );
    }

    #[test]
    fn a_rendered_page_is_opaque_and_mostly_paper() {
        let doc = open_document(&fixture("sample.pdf")).unwrap();
        let raster = rasterize(&doc, 0, 400, 400).unwrap();

        assert!(
            raster.rgba.chunks_exact(4).all(|px| px[3] == 255),
            "the page must be opaque, or the panel shows through the paper"
        );

        let white = raster.rgba.chunks_exact(4).filter(|px| px[0] > 200).count();
        let total = raster.rgba.len() / 4;
        assert!(
            white * 2 > total,
            "a mostly blank invoice should be mostly white"
        );

        // The drawn rule and text have to leave *some* dark pixels behind,
        // otherwise we are rendering an empty page and would not know.
        assert!(raster.rgba.chunks_exact(4).any(|px| px[0] < 128));
    }

    #[test]
    fn zoom_asks_for_a_bigger_box_and_gets_a_bigger_page() {
        let doc = open_document(&fixture("sample.pdf")).unwrap();
        let fit = rasterize(&doc, 0, 300, 400).unwrap();
        let zoomed = rasterize(&doc, 0, 600, 800).unwrap();

        assert_eq!(zoomed.height, fit.height * 2);
        assert_eq!(zoomed.width, fit.width * 2);
    }

    #[test]
    fn every_page_of_a_multi_page_document_renders() {
        let doc = open_document(&fixture("multipage.pdf")).unwrap();
        for page in 0..3 {
            let raster = rasterize(&doc, page, 200, 280).unwrap();
            assert!(raster.width > 0 && raster.height > 0);
        }
    }

    #[test]
    fn a_missing_file_is_missing_not_a_crash() {
        assert!(matches!(
            open_document(&fixture("no-such-file.pdf")),
            Err(ViewError::Missing)
        ));
    }

    #[test]
    fn a_file_that_is_not_a_pdf_is_rejected() {
        assert!(matches!(
            open_document(&fixture("corrupt.pdf")),
            Err(ViewError::NotAPdf(_))
        ));
    }

    #[test]
    fn a_truncated_pdf_is_repaired_rather_than_refused() {
        // MuPDF rebuilds a broken cross-reference table by scanning for
        // objects, so a half-downloaded invoice still shows its page instead
        // of an error. Pinned because it is surprising, and because losing it
        // would be a real regression for the user.
        let doc = open_document(&fixture("truncated.pdf")).expect("mupdf repairs this");
        assert_eq!(page_count(&doc).unwrap(), 1);
        assert!(rasterize(&doc, 0, 100, 100).is_ok());
    }

    #[test]
    fn a_password_protected_file_reports_itself_as_encrypted() {
        assert!(matches!(
            open_document(&fixture("encrypted.pdf")),
            Err(ViewError::Encrypted)
        ));
    }

    #[test]
    fn a_zero_page_file_never_reaches_a_page_index() {
        let doc = open_document(&fixture("zero-page.pdf")).expect("it is a valid pdf");
        assert_eq!(page_count(&doc), Err(ViewError::NoPages));
    }

    #[test]
    fn the_cache_returns_what_it_stored_and_stays_within_budget() {
        let mut cache = Cache::default();
        let key = |page| CacheKey {
            path: PathBuf::from("a.pdf"),
            page,
            width: 10,
            height: 10,
        };
        let raster = |byte| Raster {
            width: 1,
            height: 1,
            rgba: vec![byte; 4],
        };

        cache.insert(key(0), raster(1));
        cache.insert(key(1), raster(2));

        assert_eq!(cache.get(&key(0)), Some(raster(1)));
        assert_eq!(cache.get(&key(1)), Some(raster(2)));
        assert_eq!(cache.get(&key(9)), None);
        assert!(cache.bytes <= CACHE_BUDGET);
    }

    #[test]
    fn the_cache_evicts_least_recently_used_first() {
        let mut cache = Cache::default();
        let key = |page| CacheKey {
            path: PathBuf::from("a.pdf"),
            page,
            width: 1,
            height: 1,
        };
        // Two pages, each half the budget, then a third that forces one out.
        // Touching page 0 first should make page 1 the victim.
        let big = || Raster {
            width: 1,
            height: 1,
            rgba: vec![0; CACHE_BUDGET / 2],
        };

        cache.insert(key(0), big());
        cache.insert(key(1), big());
        assert!(
            cache.get(&key(0)).is_some(),
            "touch page 0 so page 1 is oldest"
        );
        cache.insert(key(2), big());

        assert!(
            cache.get(&key(1)).is_none(),
            "page 1 was least recently used"
        );
        assert!(cache.get(&key(0)).is_some());
        assert!(cache.get(&key(2)).is_some());
    }
}
