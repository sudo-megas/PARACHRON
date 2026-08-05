# Chron2 — Document viewer

**Milestone:** 2 of ~9 (CORE §9)
**Status:** done
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
Cargo.toml            # + mupdf and arboard, both default-features = false
src/
├── render.rs         # NEW — render worker, rasterization, LRU cache, ViewError
├── viewer.rs         # NEW — which document, which page, at what size
├── data.rs           # + Product::document_path
├── strings.rs        # + viewer keys, and Key::ALL so the tests cannot drift
└── main.rs           # + install the viewer; still wire-up only
ui/
├── viewer.slint      # NEW — tabs, preview, control row, serial strip
├── palette.slint     # NEW — Palette lifted out of app.slint so both can use it
├── app.slint         # column 2 hosts the viewer component
└── strings.slint     # + viewer string properties
tests/fixtures/       # NEW — six PDFs, see below
```

`viewer.rs` was not in the original plan: the state machine (selection → tabs → page → zoom → request) is real logic and does not belong in a file Chron1 defined as wire-up only. Keeping it separate also makes it unit-testable without a window, which is how criteria 1–4 are verified below.

## Tasks

- [x] Install `clang`; add `mupdf` to `Cargo.toml` with `default-features = false` and only the features PDF rendering needs (Technical notes); confirm a clean `cargo build` succeeds
- [x] `render.rs`: a worker thread that owns every MuPDF handle, takes `RenderRequest { path, page, width_px, height_px }` over a channel and returns an RGBA buffer — MuPDF contexts are per-thread, so nothing MuPDF-shaped ever crosses back to the UI thread
- [x] Results reach Slint through `slint::invoke_from_event_loop`; the UI thread never calls MuPDF and never blocks
- [x] `ViewError` enum (missing file, unreadable, not a PDF, encrypted, zero pages, render failed) rendered through the string table — same typed-error pattern as `DataError`, for the same criterion-5 reason
- [x] Open a document once per tab and keep its page count and per-page bounds; bounds are cheap and drive the fit calculation without rasterizing
- [x] Bounded LRU cache keyed by `(path, page, width_px, height_px)`; stale entries dropped when the pane resizes or zoom changes
- [x] `data.rs`: helper turning a `Product` plus the products root into absolute PDF paths, reusing the `folder` field already on `Product`
- [x] `viewer.slint`: workspace-style tab row across the top of column 2, one tab per entry in `pdfs`, in that order; active tab visually distinct; tabs for files flagged in `missing_pdfs` (Chron1) render in the error style and are still selectable
- [x] Preview area fills the remaining height; the page is fitted whole inside it by default, centred, with a subtle page edge so a white page reads as a page against the panel
- [x] Control row: `‹` / `›` prev-next, a `2 / 12` counter, and a zoom slider (Technical notes for the semantics); controls disabled, not hidden, when a document has one page
- [x] Zoom above fit makes the page larger than the pane — wrap the preview in a `Flickable` so it pans, and re-render at the new pixel size rather than upscaling the cached bitmap
- [x] Serial-number strip pinned below the preview (CORE §4), showing the selected product's `serial`; label from the string table, value beside it
- [x] Empty and error states: product with no `pdfs`, PDF missing from disk, corrupt file, encrypted file, zero-page file — each a readable message from the string table, no panic
- [x] Selecting a different product or tab resets to page 1 at fit zoom; selecting a broken product keeps Chron1's reason display
- [x] Render a real invoice and a real multi-page warranty PDF end to end and confirm the minimal feature set is sufficient (Technical notes)

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

**Crate and features.** `mupdf` 0.8, itself AGPL-3.0 — which is exactly why CORE §1 fixes Parachron's licence. Its defaults pull `js`, `epub`, `xps`, `cbz`, `html`, `img`, `svg`, `docx-output`, `tesseract` and `brotli`; `tesseract` alone drags in an OCR engine. Settled on `default-features = false` plus `base14-fonts` (the 14 standard PDF fonts, needed whenever a PDF does not embed its own), `system-fonts` (fontconfig fallback), `brotli` and `img`. Dropping `js` also means PDF-embedded JavaScript never executes — a security gain for files that arrive as email attachments.

That set is **proven, not assumed**: a real 19-page illustrated PDF with embedded fonts, colour fills, vector diagrams and code blocks renders correctly, as does the Helvetica fixture. The trimmed build also turned out cheap — a full vendored MuPDF compile lands in about **1m40s**, not the many minutes feared.

**`arboard` for the clipboard.** Slint exposes no public clipboard API. Added with `default-features = false` plus `wayland-data-control` — the defaults drag in the `image` crate for clipboard *images*, which Parachron never copies. Verified end to end on Wayland: clicking the serial strip puts the serial on the system clipboard.

**Threading.** MuPDF's `fz_context` is per-thread and its handles are not `Send`. One worker thread owns every context, document and page for the whole app; the UI thread only ever sends a `RenderRequest` and receives pixels. Requests are coalesced — a window drag produces a burst of resize requests and only the last matters, so drop superseded ones instead of queueing them.

**Zoom semantics.** Zoom is a multiplier of the fit scale, not of the PDF's natural size: `1.0×` means the whole page is visible (the default), up to `4.0×`. This keeps the slider meaningful at any window size, since fit itself changes as the pane changes. Re-render on change rather than scaling the bitmap; debounce while the slider is dragged. Ctrl+scroll-wheel over the preview is a nice-to-have, not required.

**HiDPI.** Rasterize at `pane_size × window.scale_factor() × zoom` physical pixels and hand Slint an image of that size. Chron1 already established the logical/physical distinction the hard way — getting this wrong shows up as a soft, blurry page on a scaled display.

**Tab labels.** CORE §4's wireframe shows `[Invoice] [Garanti] [.pdf]`, so the label is the file-name stem rather than a fixed English word — `invoice.pdf` → `Invoice`, `garanti.pdf` → `Garanti`. That keeps the tab honest about what is on disk and needs no translation, which is right for user-supplied file names. Elide long stems.

**Serial strip.** CORE §10 left the ratio open; Chron2 closed it at a fixed **44px** rather than a proportion — it is one line of text, and a proportional strip would grow absurd on a tall window. Recorded in CORE §10, open item struck.

**Fixtures.** `tests/fixtures/` holds six small committed PDFs, all generated deterministically (`sample.pdf` and `zero-page.pdf` hand-written, the rest via `qpdf`), so criterion 6 is a test rather than a story:

| Fixture | Bytes | What it proves |
|---|---|---|
| `sample.pdf` | 677 | One page, Helvetica — exercises `base14-fonts` |
| `multipage.pdf` | 1066 | Three pages, for the counter and prev/next |
| `encrypted.pdf` | 1427 | `needs_password` → `ViewError::Encrypted` |
| `zero-page.pdf` | 239 | Valid PDF, zero pages → `ViewError::NoPages` |
| `corrupt.pdf` | 538 | No `%PDF` header at all → `ViewError::NotAPdf` |
| `truncated.pdf` | 338 | Half a PDF — and MuPDF *repairs* it (see below) |

**MuPDF repairs damaged files.** The first `corrupt.pdf` was `sample.pdf` truncated in half, and it did not fail — MuPDF rebuilds a broken cross-reference table by scanning for objects, so it opened the file, found the page and rendered it. That is better behaviour than an error (a half-downloaded invoice still shows), but it meant the fixture was testing nothing. Split into two: `corrupt.pdf` is now bytes with no PDF header, which genuinely fails, and `truncated.pdf` pins the repair so losing it would be caught as a regression.

**Error pattern.** `ViewError` follows `DataError`: a typed enum in `render.rs`, rendered to text by `viewer.rs` through the string table. Building the message in `render.rs` would plant a user-visible English literal outside `strings.rs` and fail criterion 8 — the same trap Chron1 hit.

**Glyphs go through the string table too.** `‹`, `›` and the copy `⧉` are literals on screen, so by Chron1's own rule (which routes the `⚠` prefix) they belong in `strings.rs`. They are identical in both languages, but the criterion-8 sweep should come back genuinely clean rather than clean-with-exceptions.

## How the criteria were verified

46 tests pass (`cargo test`), up from Chron1's 19.

**Automated.** `render.rs` tests drive every fixture: page counts, a raster fitted inside its target box and touching one side, an opaque mostly-white page that still contains dark pixels (so an empty render cannot pass), doubling the box doubles the page, and each failure fixture mapping to its `ViewError`. `viewer.rs` tests drive the state machine without a window: tabs built in `pdfs` order, a missing file flagged rather than dropped, a broken folder offering no documents, the render target equal to `viewport × display scale × zoom`, tokens incrementing so every request supersedes the last, and page/zoom resetting on tab or product change. The LRU cache is tested for both retrieval and least-recently-used eviction.

**By eye, in the running app.** No Wayland input-injection tool exists on this machine (`ydotool`, `xdotool`, `wtype`, `kdotool` all absent), so clicking was driven by a temporary scaffold that invoked the same callbacks the UI does, then removed. Captured: a two-tab product with its page fitted and `1 / 1` with both nav buttons correctly disabled; the 19-page guide at `10 / 19` with both enabled and zoom at 2.2×, the page enlarged, panning, and re-rendered sharp rather than upscaled; and the error product showing "This PDF is password-protected" on its first tab.

**Clipboard.** Verified through Klipper's D-Bus interface: after invoking the serial strip's copy, `getClipboardContents` returned `ABC123XYZ`.

**Criterion 8.** The sweep over `src/` and `ui/` returns only icon resource paths, `""` emptiness comparisons, the `"monospace"` font-family identifier, format punctuation, and text inside comments. No user-visible words outside `strings.rs`.

**Not verified by machine.** Criterion 7 (the window stays responsive during a long render) follows from the architecture — the UI thread never calls MuPDF — but was not measured under load. Worth a look with a very large scanned PDF when one turns up.

## Done when

All acceptance criteria pass on the laptop. Then: record the serial-strip height in CORE §10 and remove that open item, note the final `mupdf` feature set in CORE §2, mark this file's status `done`, and ask user permission to start writing Chron3.
