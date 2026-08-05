# Chron2 — Document viewer

**Milestone:** 2 of ~9 (CORE §9)
**Status:** not started
**Builds against:** CORE §2 (stack — MuPDF), §3 (data model — `pdfs` order), §4 (layout, viewer column, app-wide principles), §8 (conventions & development rules), §10 (serial-strip ratio, open item to close here)

## Goal

Column 2 stops being a placeholder and becomes the document viewer. Selecting a product shows its PDFs as workspace tabs in `pdfs` order; the active tab renders with MuPDF, one page at a time fitted to the pane, with prev/next, a page counter and a zoom slider; a fixed serial-number strip sits below. Rendering never blocks the window and a bad PDF never takes the app down.

## Scope

**In:** `mupdf` dependency on a minimal feature set · background render worker · page cache · tabs built from `pdfs` · page navigation · zoom slider · serial-number strip · error states for missing, corrupt, encrypted and empty PDFs · new string keys · closing CORE §10's serial-strip open item.

**Out (explicitly):** text selection, search and copy from the page · printing · rotation · thumbnails · adding or removing PDFs (Chron3) · the details column (Chron4) · themes beyond the Chron1 palette (Chron5) · Turkish completeness (Chron6) · export (Chron7) · About (Chron8) · packaging MuPDF per target (Chron9).

## Prerequisites

`mupdf-sys` builds a vendored MuPDF from source and runs `bindgen` over its headers. On this machine `make`, `gcc`, `g++`, `python` and `fontconfig` are already present; **`clang` is not** and must be installed before the first build:

```
sudo pacman -S clang
```

Budget several minutes for that first `cargo build` — MuPDF is a large C library. The result is cached in `target/`, so only clean builds pay it again. CI pays it on every run; that is Chron9's problem, and CORE §7 already flags the static-build/bundling requirement.

## Files to add and change

```
Cargo.toml            # + mupdf, default-features = false (see Technical notes)
src/
├── render.rs         # NEW — render worker, page rasterization, cache, ViewError
├── data.rs           # + helper resolving a product's folder to absolute PDF paths
├── strings.rs        # + viewer keys (tabs, page counter, zoom, error states)
└── main.rs           # + wire product selection → tabs → renderer → image
ui/
├── viewer.slint      # NEW — tabs, preview, control row, serial strip
├── app.slint         # column 2 hosts the viewer component
└── strings.slint     # + viewer string properties
tests/fixtures/       # NEW — sample.pdf (1 page), multipage.pdf, corrupt.pdf, encrypted.pdf
```

## Tasks

- [ ] Install `clang`; add `mupdf` to `Cargo.toml` with `default-features = false` and only the features PDF rendering needs (Technical notes); confirm a clean `cargo build` succeeds
- [ ] `render.rs`: a worker thread that owns every MuPDF handle, takes `RenderRequest { path, page, width_px, height_px }` over a channel and returns an RGBA buffer — MuPDF contexts are per-thread, so nothing MuPDF-shaped ever crosses back to the UI thread
- [ ] Results reach Slint through `slint::invoke_from_event_loop`; the UI thread never calls MuPDF and never blocks
- [ ] `ViewError` enum (missing file, unreadable, not a PDF, encrypted, zero pages, render failed) rendered through the string table — same typed-error pattern as `DataError`, for the same criterion-5 reason
- [ ] Open a document once per tab and keep its page count and per-page bounds; bounds are cheap and drive the fit calculation without rasterizing
- [ ] Bounded LRU cache keyed by `(path, page, width_px, height_px)`; stale entries dropped when the pane resizes or zoom changes
- [ ] `data.rs`: helper turning a `Product` plus the products root into absolute PDF paths, reusing the `folder` field already on `Product`
- [ ] `viewer.slint`: workspace-style tab row across the top of column 2, one tab per entry in `pdfs`, in that order; active tab visually distinct; tabs for files flagged in `missing_pdfs` (Chron1) render in the error style and are still selectable
- [ ] Preview area fills the remaining height; the page is fitted whole inside it by default, centred, with a subtle page edge so a white page reads as a page against the panel
- [ ] Control row: `‹` / `›` prev-next, a `2 / 12` counter, and a zoom slider (Technical notes for the semantics); controls disabled, not hidden, when a document has one page
- [ ] Zoom above fit makes the page larger than the pane — wrap the preview in a `Flickable` so it pans, and re-render at the new pixel size rather than upscaling the cached bitmap
- [ ] Serial-number strip pinned below the preview (CORE §4), showing the selected product's `serial`; label from the string table, value beside it
- [ ] Empty and error states: product with no `pdfs`, PDF missing from disk, corrupt file, encrypted file, zero-page file — each a readable message from the string table, no panic
- [ ] Selecting a different product or tab resets to page 1 at fit zoom; selecting a broken product keeps Chron1's reason display
- [ ] Render a real invoice and a real multi-page warranty PDF end to end and confirm the minimal feature set is sufficient (Technical notes)

## Acceptance criteria

1. Selecting a product with two PDFs shows two tabs in `pdfs` order, the first active, its first page rendered fitted whole inside the pane.
2. Switching tabs switches documents; the page counter and zoom reset, and the previously viewed tab renders from cache on return.
3. `‹` / `›` move through a multi-page PDF, the counter tracks, and both controls are disabled at the ends.
4. The zoom slider enlarges the page, the enlarged page pans, and the result is re-rendered rather than a blurred upscale.
5. Resizing the window re-renders the page at the new size; the page stays fitted and stays crisp.
6. A missing, corrupt, encrypted and zero-page PDF each show a readable message and leave the app running — verified with the committed fixtures.
7. The window stays responsive while a large PDF renders: the list still scrolls and tabs still respond.
8. `grep -rn` for user-visible literals in `.slint`/`.rs` still finds none outside `strings.rs`.
9. `git log` shows only `sudo-megas` as author and no AI attribution anywhere.

## Technical notes

**Crate and features.** `mupdf` 0.8, itself AGPL-3.0 — which is exactly why CORE §1 fixes Parachron's licence. Its defaults pull `js`, `epub`, `xps`, `cbz`, `html`, `img`, `svg`, `docx-output`, `tesseract` and `brotli`, none of which a purchase vault needs; `tesseract` alone drags in OCR. Start from `default-features = false` and add back only `base14-fonts` (the 14 standard PDF fonts, needed whenever a PDF does not embed its own) and `system-fonts` (fontconfig fallback, already installed). Dropping `js` also means PDF-embedded JavaScript never executes — a security gain for files that arrive as email attachments. Note that `img` governs opening image *files* as documents, not images embedded inside a PDF, which the always-built codecs handle. That reasoning is sound but unproven here, hence the last task: if a real invoice fails to render, re-enable features one at a time and record what was needed.

**Threading.** MuPDF's `fz_context` is per-thread and its handles are not `Send`. One worker thread owns every context, document and page for the whole app; the UI thread only ever sends a `RenderRequest` and receives pixels. Requests are coalesced — a window drag produces a burst of resize requests and only the last matters, so drop superseded ones instead of queueing them.

**Zoom semantics.** Zoom is a multiplier of the fit scale, not of the PDF's natural size: `1.0×` means the whole page is visible (the default), up to `4.0×`. This keeps the slider meaningful at any window size, since fit itself changes as the pane changes. Re-render on change rather than scaling the bitmap; debounce while the slider is dragged. Ctrl+scroll-wheel over the preview is a nice-to-have, not required.

**HiDPI.** Rasterize at `pane_size × window.scale_factor() × zoom` physical pixels and hand Slint an image of that size. Chron1 already established the logical/physical distinction the hard way — getting this wrong shows up as a soft, blurry page on a scaled display.

**Tab labels.** CORE §4's wireframe shows `[Invoice] [Garanti] [.pdf]`, so the label is the file-name stem rather than a fixed English word — `invoice.pdf` → `Invoice`, `garanti.pdf` → `Garanti`. That keeps the tab honest about what is on disk and needs no translation, which is right for user-supplied file names. Elide long stems.

**Serial strip.** CORE §10 leaves the ratio open and Chron2 is where it closes. Propose a fixed height (~44px) rather than a proportion: it is one line of text, and a proportional strip would grow absurd on a tall window. Pin the final number in CORE §10 and strike the open item.

**Fixtures.** `tests/fixtures/` gets four small committed PDFs — one page, multi-page, deliberately corrupt, and encrypted — so criterion 6 is a test rather than a story. Keep them tiny; they live in git forever.

**Error pattern.** `ViewError` follows `DataError`: a typed enum in `render.rs`, rendered to text by `main.rs` through the string table. Building the message in `render.rs` would plant a user-visible English literal outside `strings.rs` and fail criterion 8 — the same trap Chron1 hit.

## Done when

All acceptance criteria pass on the laptop. Then: record the serial-strip height in CORE §10 and remove that open item, note the final `mupdf` feature set in CORE §2, mark this file's status `done`, and ask user permission to start writing Chron3.
