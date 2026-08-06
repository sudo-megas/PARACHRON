# Chron7 — Export

**Milestone:** 7 of ~9 (CORE §9)
**Status:** done
**Builds against:** CORE §6 (export, in full), §2 (MuPDF is reused for this — no second PDF library), §3 (data model — the fields the summary page reads; dates), §4 (column 3's EXPORT button, app-wide principles), §7 (packaging — no new build dependency on any target), §8 (conventions & development rules)

## Goal

EXPORT stops being a disabled stub. Selecting a product and pressing it produces one PDF containing a generated summary page followed by every one of that product's documents, in tab order, saved wherever the user chooses. The summary page is real text — searchable, selectable, printable — not a picture of text, and it carries the warranty countdown as it stood at the moment of export.

This is the last milestone that adds a feature; Chron8 is polish and Chron9 is packaging. It is also the only one that makes Parachron *write* a PDF rather than read one.

## Scope

**In:** the summary page, generated from the product's data · appending the product's PDFs in tab order · one output file, at a path the user picks · export off the UI thread · documents that cannot be included, skipped and named on the summary page itself · EXPORT gated on a selection that can actually be exported · new string keys.

**Out (explicitly):** exporting the whole vault at once — CORE §6 says this is a product-level action, and a vault-wide export is a different feature with a different summary page · exporting a subset of a product's documents · page ranges · a print dialog: Parachron writes a file and the user's PDF viewer prints it, which is also the only reading of CORE §4's no-external-opens rule that holds · encryption or password-protecting the output · embedding the product's images or the app's icon · a progress bar (see the note on why) · About (Chron8) · packaging (Chron9).

## Prerequisites

Chron5 and Chron6 complete: the summary page's labels come from the string table and are written in the language the app is in, so a Turkish session exports a Turkish summary. Nothing new to install, and **no new dependency** — CORE §2 says MuPDF is reused for export and it is. `rfd` is already in the tree from Chron3 and gains one more call.

## The spike, and what it settled

Chron3 established that a hard question gets spiked before the milestone's notes are written. This one had four, and all four are now answered against the real crate rather than assumed. The spike verified by *reading text back out of the saved file* with `Page::search`, never by absence of an error — a glyph the font cannot supply renders as nothing at all and raises nothing.

**1. A base-14 font in its default encoding silently drops Turkish.** `TextOptions` defaults to `simple: true` with `SimpleFontEncoding::Latin`, and the three encodings the crate offers are Latin, Greek and Cyrillic. `ğ ş ı İ` are Latin Extended-A and are not in WinAnsi. Drawn that way, `Şarj Cihazı`, `Öğrenci` and `İphone` came back from the saved PDF with **zero** search hits each. `Ürün` came back with one, because `Ü` *is* in Latin-1 — which is exactly the trap: a careless check on a word like `Ürün` passes and the feature looks fine.

**This was never a Turkish-mode question.** Product names and serial numbers are user data. Somebody running the app in English with a product called `Şarj Cihazı` — the app's own folder-slug test fixture — has to get a correct summary page. There was no version of this that could be deferred to a language setting.

**2. `simple: false` fixes it completely, and costs nothing.** Registering the same `helv` as a composite font instead of a simple one gave **all five** words back, one hit each. The bundled base-14 face has the glyphs; only the encoding was in the way. So the fix is one field on `TextOptions`, and neither of the fallbacks the spike was prepared to fall back to is needed: no `bundled-fonts-noto` (binary size on all three of CORE §7's targets) and no `font-kit` system-font lookup (fragile on the Windows target there is no local machine to test). Every text run on the summary page is composite, unconditionally — not "when the string looks non-Latin", because deciding that per string is how the `Ürün` trap gets reintroduced.

**3. The whole export is one in-memory document, with no temporary file.** `PdfDocument::new()`, a `new_page(Size::A4)`, a `Shape` committed onto it, then `insert_pdf` once per source, then write. The spike built a five-page file this way — summary plus a one-page and a three-page source — and the summary page's Turkish text was still searchable in the reopened output. An earlier sketch wrote the summary to a temporary file and reopened it to merge into; that step does not exist.

**4. `PdfDocument::open` does *not* refuse an encrypted file, and this one matters.** `render::open_document` catches encryption because it explicitly asks `needs_password()` after opening — the comment there calls that "the honest test". `PdfDocument::open` on the same `encrypted.pdf` fixture returned `Ok`, and the document then reported `needs_password() == Ok(true)` and `page_count() == Ok(1)`. Merging that would append a page whose content stream cannot be decrypted. So export asks the same question in the same order, and it asks it through `render.rs` rather than in `export.rs`: a new `render::open_pdf` mirrors `open_document`'s three checks and returns a `PdfDocument`. `PdfDocument` dereferences to `Document`, so `render::page_count` is reused unchanged and there is still exactly one place in the app that decides what is wrong with a PDF and what it is called. A file that is not a PDF at all came back as an error already (`no objects found`), which maps onto `NotAPdf` like any other.

## Files to add and change

```
src/
├── export.rs         # NEW — the summary page, the merge, the thread
├── render.rs         # + open_pdf, beside open_document and sharing its checks
├── details.rs        # EXPORT becomes live; the status line
├── main.rs           # + install export
└── strings.rs        # + summary-page and export keys
ui/
├── details.slint     # EXPORT loses `enabled: false`; + a fixed-height status line
└── app.slint         # + the export properties and its two callbacks
```

`strings.slint` is untouched, which is worth a word: every string the export uses
is either drawn onto the PDF or composed into the status line, so all of it is
looked up in Rust and none of it needs a property on the Slint side.

`export.rs` mirrors `import.rs` deliberately, down to the shape of its types: a `Job` that can cross to a thread, an `Outcome` that comes back, a `commit` that spawns and rings the window, and a `run` that is a plain function of a `Job` and is where all the tests point. The two modules are the same kind of thing — file I/O plus MuPDF, off the UI thread, reporting to a slot — and making the second one look like the first is cheaper to read than a second invention.

## Tasks

- [x] `render.rs`: `open_pdf(path) -> Result<PdfDocument, ViewError>` — `is_file`, open, `needs_password`, mapped exactly as `open_document` maps them
- [x] `export.rs`: `Job` — the product's folder, its `Draft`-shaped data, the documents in tab order, the language, today's date, the output path
- [x] `export.rs`: inspect every source *before* drawing anything, so the summary page can name what it had to leave out
- [x] `export.rs`: `summary(doc, job)` — the page, laid out top-down on A4, every run composite
- [x] `export.rs`: labels and values through the string table, dates through `fmt_date`, the counter through the same `countdown` the details column uses
- [x] `export.rs`: append each usable source with `insert_pdf`, in tab order
- [x] `export.rs`: write with `write_to` into a `File`, not `save(&str)`
- [x] `export.rs`: `suggested_name(product, today)` — `Parachron-<name>-<DD-MM-YYYY>.pdf`, sanitised but not slugged
- [x] `export.rs`: `pick_destination` — `rfd` save dialog behind `spawn_local`, the same thin edge `import::pick` is
- [x] `export.rs`: `commit` — thread, slot, `invoke_export_finished`
- [x] `details.rs`: `on_export`, gated on a selection with a parsed manifest; busy, done and failed states
- [x] `details.slint`: EXPORT live, with a status line that does not reflow the column when it appears
- [x] `strings.slint` / `strings.rs`: every summary-page label, notice and error through the table, both languages
- [x] Delete `tests/glyph_spike.rs`; its four findings live on as tests in `export.rs` and `render.rs`

## Acceptance criteria

1. Exporting a product with two documents produces one PDF whose first page is the summary and whose remaining pages are those two documents' pages, in tab order.
2. The summary page carries the product's name, serial number, purchase date, warranty start, warranty end, days left at the moment of export, and the purchase link — every one of CORE §6's fields.
3. The summary page's text is selectable and searchable in an external PDF viewer, not an image: searching the output for the product's name finds it on page one.
4. A product named `Şarj Cihazı` with a serial containing `İ` exports with those characters intact and findable, with the app in **English**.
5. With the app in Turkish, the summary page's labels are Turkish and the countdown reads `658 gün`.
6. The days-left figure on the page equals what column 3 showed at the moment EXPORT was pressed, and reads as expired rather than negative for a warranty that has run out.
7. A product with a document listed in `product.toml` but absent from disk exports the rest, and the summary page names the one it could not include.
8. A product with an encrypted or unreadable PDF does the same: the output is a valid PDF, the bad file is named on the summary page, and nothing crashes.
9. A product with no documents at all exports a one-page PDF that is just the summary.
10. The save dialog opens with `Parachron-<product-name>-<date>.pdf` suggested, and a product name containing `/` or a control character does not produce an unwritable suggestion.
11. Cancelling the save dialog writes nothing and leaves the window as it was.
12. The window keeps repainting while the dialog is open and while a large export is being written; the UI thread never calls MuPDF.
13. EXPORT does nothing and looks inert when nothing is selected or the selection is a folder whose manifest will not parse.
14. Exporting does not disturb the viewer: the same product stays selected, on the same page, at the same zoom.
15. `grep -rn` for user-visible literals in `.slint`/`.rs` still finds none outside `strings.rs`.
16. `git log` shows only `sudo-megas` as author and no AI attribution anywhere.

## Five defects an adversarial review of this milestone found

All five were in code that passed fourteen tests and a clean build, and all five sat just outside where those tests aimed: every one called `run` directly, with a short name, a short link, at most three skipped files and a writable destination.

**1. Nothing on the page wrapped, and nothing clipped.** `insert_text` lays a single line out from the point it is given and keeps going past the paper's edge. A product called `Samsung Odyssey OLED G8 34-inch Ultrawide Curved Gaming Monitor` with an ordinary store URL under it put ink in **column 594 of a 595-point page** — the right margin is at 539 — so the end of the name and most of the link were off the sheet. In an artefact whose purpose is to be kept and forwarded, that is data loss.

No test could see it. Every test searches the written file, and `search` reads the content stream rather than the visible area, so text drawn past the media box is still found; the one test that rasterizes the page checked only where the ink was *vertically*. Every value on the page now goes through `wrapped`, which uses `insert_textbox` and bounds each value's height so one long field cannot push the rest of the page off the bottom. A test measures the leftmost and rightmost dark columns.

That fix immediately exposed a second thing: **`insert_textbox` draws nothing at all if the box is not taller than its own line.** A one-line box at the counter's 19pt silently produced an empty region — caught only because the search tests were looking for that exact string. `wrapped` adds half a size of headroom and advances `y` by what was actually consumed.

**2. The "Not included" block ran off the paper.** It sat at a fixed 0.62 of the page height and grew downwards with no stop: 13 skipped documents straddled the footer rule, 14 overprinted the footer date, and 18 or more were drawn *below the paper* — invisible in every viewer and still findable by `search`, which is precisely the failure the rasterizing test was written for and precisely the case it did not run. The block now starts below whatever the fields actually consumed, so a wrapped name pushes it down instead of being written over, and it stops at the footer rule and says `+N` for what it could not list.

**3. A failed export told the user their `product.toml` was invalid.** Every MuPDF error was mapped onto `DataError::Malformed`, which the string table renders as "product.toml is not valid" — so an export onto a full disk sent somebody to repair a manifest that was perfectly fine, and a `File::create` failure came back as `Unreadable`, "Could not be read", for a *write*. The old test asserted only that the message was non-empty, so it passed while naming the wrong file. Export has its own `Failure` type now, `Write` or `Assemble`, and a test asserts neither message mentions the manifest.

**4. The status line outlived the product it was about.** Nothing in the vault's push touched it, and the line is a sibling of both branches of column 3, so `Saved — Not included: gone.pdf` from product A sat above product B's details — or above a broken folder's "Details appear here", claiming a folder with no documents had failed to include one. `details::show` clears it on every selection change: `export.rs` is the only thing that ever puts a status up, and a change of product is the only other thing that can take one down.

**5. A language switch blanked "Exporting…" mid-export.** `set_lang` cleared the status unconditionally, and the justification for clearing — that it is a sentence about something that already happened — is only true of a *finished* export. Switching language while a 200-page manual was being written left the line blank, the EXPORT button live, and clicking it silently doing nothing because the export was busy: an app that looked idle while it worked. A running export is re-said in the new language; a finished one is still cleared.

**And fixes 3 and 5 then collided, in the same commit.** `details::show` clears the status, and it runs from every `vault::push` — including the one `lang::switch` ends with. So the unconditional clear that fixed 3 erased the line that fixing 5 had just re-said, one call earlier in the same sequence, and the original symptom came straight back. It was wider than the language switch, too: a sort toggle or a form save during an export blanked the line for the same reason, because only a change of *product* was ever meant to.

The clear is now conditional on `export-running`, a flag on the window rather than only in Rust, because `details::show` is the thing that has to consult it. Making it a window property also gave the two paths **one** predicate for "an export is in flight" — the first attempt had `set_lang` asking `State.exporting` while `clear_status` asked the window, and a test that faked one of them immediately showed the two could disagree. `State.exporting` now exists only to carry *which* folder.

The headless test covers all three transitions: a selection change, a sort toggle and a language switch during a running export, then the same selection change once it is finished. Neither of the earlier tests could have caught this — one sets the status by hand and clicks a row, the other never reaches `set_lang` with an export in flight.

**A residual that fix 3 narrowed rather than closed.** The window stays live while an export runs, so the user can select another product before it lands, and `Saved` would then appear above the wrong one. The export compares the folder it was given against `vault::selected_folder()` when it finishes and withholds the line if they differ — the file is still written, only the sentence about it is dropped, because there is nowhere honest to put it. That is what put a real caller on `selected_folder`, whose Chron6 allowance comes off here.

Two smaller ones, from the same review. `suggested_name` had no length bound, so a pasted marketplace title produced a filename over `NAME_MAX` — which is 255 **bytes**, and a Turkish letter is two of them, so a name well inside any character count can be over it; it is capped now, on a character boundary. And the Turkish export strings were internally inconsistent: *dışa aktarmak* is "export" and bare *aktarmak* is "transfer", and three of the five had dropped the particle, so a user pressed `DIŞA AKTAR`, got a dialog titled *PDF Olarak Aktar*, watched *Aktarılıyor…* and read *Dışa aktarılamadı* — four wordings for one action.

One claim was also softened rather than fixed. `render::open_pdf`'s comment said "is this a readable PDF" has exactly one answer in the app; CORE §2 builds MuPDF with `img`, so `Document::open` accepts image formats `PdfDocument::open` cannot, and a scan saved as `invoice.pdf` that is really a PNG opens in the viewer and is skipped by the export. That behaviour is correct — a PNG cannot be grafted into a page tree, and the summary page names it — but the comment promised more than the code delivers, and now says so.

## Technical notes

**The summary page is drawn last of the decisions and first of the pages.** Sources are inspected before anything is drawn, because the page has to be able to say `Not included: warranty.pdf`, and it cannot say that if it was drawn before anybody tried to open the file. So the order is: open every source and sort them into usable and refused; draw the summary, refusals included; append the usable ones; write. This is also why a refused document does not fail the export — CORE §6 says the output covers the product, and a product with one broken invoice still has a warranty worth carrying. Refusing to export at all would be the app deciding the user does not get their summary because one file is bad.

**Skipping is recorded in the artefact, not just in the window.** A notice in column 3 is gone the moment the app closes; the exported PDF is the thing that gets emailed to a shop six months later. Naming the missing file on the page itself means the file explains its own gaps, which is the same principle that keeps broken folders visible in the list with a readable reason instead of hiding them.

**Why `write_to` and not `save`.** `PdfDocument::save` and `save_with_options` both take `&str`, so a destination path that is not valid UTF-8 cannot be expressed — and on Linux a path is bytes, so that is a real file the user could have picked, not a hypothetical. `write_to` takes any `io::Write`, so the output goes into a `File` opened from the `PathBuf` the dialog returned and the question never arises. The spike confirmed it returns the byte count and produces a file that reopens.

**Not the render worker.** Export runs on a thread of its own, spawned per export, for the same two reasons `import.rs` gives. MuPDF contexts are per-thread and Chron2's rule that the UI thread never calls MuPDF is worth keeping; and the render worker's queue deliberately drops all but the newest job, which is right for pixels and would be silent data loss for a file the user asked to be written. The outcome travels in an `Arc<Mutex<Option<Outcome>>>` slot rather than in the closure, because `details`/`vault` are full of `Rc`s and cannot cross a thread boundary — the same reason `import::commit` does it that way.

**Nothing is invalidated.** The output goes to a path the user chose, outside the vault, and export reads the product's files without changing them. So no `Renderer::invalidate` call belongs anywhere in this milestone. Chron3 added that message because imports write over paths the viewer may already have cached; export writes nowhere the render worker has ever heard of. Stated so that a later reading of "export touches PDFs" does not add one for symmetry.

**The counter is the one in the window, computed the same way.** `days_left` and `countdown` already exist and already handle the expired case and Turkish's lack of plural agreement after a numeral. Export calls them rather than reimplementing the arithmetic, so the number on the page and the number in column 3 cannot disagree — which is the whole reason CORE §6 says "days left at time of export" rather than "days left". `today` is read at the moment the job is built, on the UI thread, from the offset `main` captured before any thread existed.

**The suggested filename is sanitised, not slugged.** `data::folder_slug` exists and is the wrong tool: it lowercases and folds to ASCII, so `Şarj Cihazı` would be suggested as `sarj-cihazi` — correct for a directory that has to survive being rsynced to Windows, and a downgrade for a filename a person is about to read in a save dialog. The suggestion keeps the name as written and strips only what makes a filename invalid, which is what `import::destination_name` already does for picked files. It is only a suggestion in any case: the dialog lets the user type whatever they want, and the app writes where it is told.

**No progress bar.** The window stays responsive because the work is on another thread, and the status line says the export is running. A percentage would need `insert_pdf` to report progress per page, which it does not, so it would be a number invented from the source count — and for an export that is typically a summary page plus a dozen invoice pages, the honest answer is that it finishes before a progress bar would have finished animating in. If a hundred-page manual ever makes this feel slow, the fix is a real page-level callback, not a fake bar now.

**The dialog is a thin edge, again.** `rfd::AsyncFileDialog::save_file` behind `slint::spawn_local`, exactly as `import::pick` wraps `pick_files`, and for exactly the reason Chron3 wrote down: a portal dialog is drawn by the desktop's own portal service in the user's session, so it appears on the real display whatever `DISPLAY` says and cannot be driven under `Xvfb`. Everything past the dialog takes a `PathBuf`, so the whole of `export::run` is testable by handing it a path and only the click that opens the dialog needs a person. The blocking `FileDialog` is not an option here either — it parks the calling thread inside a D-Bus read with no timeout.

**The busy flag is claimed before the dialog opens, not after it closes.** The first version set it in the dialog's callback, which reads naturally and is wrong: the event loop keeps running while a portal dialog is up — that is the entire point of `spawn_local` — so the window stays interactive and EXPORT can be pressed again. A second press passed the `busy` check, opened a *second* save dialog, and its chosen path was then silently dropped by the check in its own callback. Two dialogs, one of them a lie.

Claiming the flag on the way in means the cancel path has to give it back, so `pick_destination` reports cancellation rather than simply not calling back. That is worth having anyway: it makes "cancelling writes nothing" an explicit branch with a comment on it instead of an absence.

**And the product is taken by value at the moment EXPORT is pressed.** The selection can change while the dialog is open, for the same reason a second click could happen. Capturing the product up front means the export is of the product the user pressed the button on, which is what they meant, and it also means nothing has to be re-read from a vault that may have been re-scanned in the meantime. It is read *before* the state borrow, too: `selected_product` borrows the vault, and holding two borrows across a dialog is how a re-entrant callback becomes a panic.

**`Shape` borrows its page, and `commit` wants the document.** `Shape::new` takes `&mut PdfPage` and holds it; `commit` takes `&mut PdfDocument`. Since `new_page` returns an owned `PdfPage`, the borrow of the page and the borrow of the document do not overlap in the crate's own example order — but the shape has to be dropped or the page scoped before the document is used again, which is why the summary-page code puts the drawing in a block. Worth stating because the compiler error it produces otherwise points at the wrong line.

**A4, not the source page size.** The summary page is A4 (`595×842`) regardless of what the appended documents are, because it is a document Parachron authored and CORE §6 asks for it to be print-friendly. Appended pages keep their own sizes — `insert_pdf` grafts them as they are, and rescaling somebody's invoice to match a summary page would be the export quietly altering their evidence.

## How the criteria were verified

124 tests pass (`cargo test`), up from Chron6's 110, with no warnings from `cargo test` or `cargo build`. Fourteen of the new ones are `export.rs`'s.

**Every test reads the written file back.** `run` is a plain function of a `Job`, so each test builds a product folder in a temporary directory, exports it, reopens the output with `render::open_pdf`, and then asks the document what it says. That matters more here than anywhere else in the project, because the failure this milestone was built around produces a valid PDF that is simply missing words: a check for "did it write without erroring" would pass every time.

**Automated.** A two-document product exports as five pages — summary, then one, then three — in tab order. Every field CORE §6 lists is found on page one by searching for it: the name, the serial, both warranty dates, the purchase date, the link, the countdown, and the wordmark. The counter is asserted to be `951 days`, checked against the calendar by hand, and to come from the same `countdown` column 3 calls. An expired warranty reads as expired and the page is asserted to contain no negative number. A Turkish session's page carries Turkish labels *and* is asserted not to also contain the English ones — which is what would catch a page half-composed from a stale language. A product with an encrypted PDF, a corrupt one, and one listed in the manifest but absent from disk exports the rest: three names come back in `skipped` with the right `ViewError` each, the output is two pages, and all three names are found on the summary page. A product with no documents at all exports one page. The summary page is A4 and the appended page is asserted to keep the size it already had, rather than being rescaled to match. A destination that cannot be written is reported rather than panicking, in both languages. And the suggested filename keeps `Şarj Cihazı` as written while stripping a slash and a control character, and still produces something writable from a name of nothing but dots.

**The glyph test is the one this milestone exists for**, and it is written to be hard to fool. Each of the four letters a Latin encoding cannot carry is searched for inside a real word — `Şarj`, `Cihazı`, `İST`, `ĞŞ` — with the app in **English**, because product names are user data. Then `Ürün` is exported separately and pinned as the *near-miss*: `Ü` is in Latin-1 and survives a wrong encoding, so a test written around a word like that one would pass while the feature was broken. Both are in the file so nobody trusts the wrong one.

**A test that rasterizes the page, because searchable text can be off the page.** The layout is hand-placed arithmetic, and `search` finds text in a content stream whether or not it falls inside the media box — so every other test here would pass a page whose footer had been drawn a hundred points below the paper. The page is rendered and inspected: it has ink in the top third, ink in the middle, ink in the bottom sixth, and it is more than 90% paper. That is the assertion that would catch a margin constant being changed by someone who then only ran the search tests.

**`write_to` is verified on a path `save` could not have expressed.** A destination containing a lone `0xFF` byte — valid in a Unix filename, not valid UTF-8, and therefore inexpressible as the `&str` that `PdfDocument::save` takes — is exported to and the file is asserted to exist.

**`render::open_pdf` is pinned against all five cases,** including the one that is the reason it exists: `encrypted.pdf` comes back as `Encrypted` rather than as a document that opens and only then admits it needs a password.

**Headless, through the real element tree.** Criterion 13: with a healthy product selected the EXPORT button reports itself enabled; with the unparseable folder selected it reports itself disabled, `details-filled` is false, and clicking it anyway produces no status line and no panic. The button reads `filled` rather than a flag of its own, so this is testing that binding.

**By eye, which no assertion replaces.** The summary page was rendered to an image and looked at, in both languages, for a product with a Turkish name and serial and two documents that could not be included. English: the wordmark quiet above `Şarj Cihazı` at full size, a rule, then serial `İST-0042-ĞŞ`, purchase date, warranty start and end, the link, `Warranty left / 951 days` as the largest thing on the page, then `Not included` with `gone.pdf — This file is not in the product folder` and `locked.pdf — This PDF is password-protected`, then a rule and `Exported 06-08-2026`. Turkish: the same page reading `Seri numarası`, `Satın alma tarihi`, `Garanti başlangıcı`, `Garanti bitişi`, `Satın alma bağlantısı`, `Kalan garanti / 951 gün`, `Eklenmeyen belgeler` and `Aktarma tarihi`. Both are black on white with no colour from any theme, and the two file names stay as they are on disk in both — they are not UI copy.

**Not verified by machine: the save dialog itself,** and this is the same boundary Chron3 documented for the file picker. A portal dialog is drawn by the desktop's own portal service in the user's session, so it appears on the real display whatever `DISPLAY` says and cannot be driven under `Xvfb`. Everything behind it takes a `PathBuf` and is tested that way; the click that opens it needs a person. That also means criteria 11 and 12 — cancelling writes nothing, and the window keeps repainting while the dialog is open — rest on the dialog being wired exactly as `import::pick` is, which has been driven by hand since Chron3, plus the fact that `run` is never reached without a path.

The `busy` flag's two transitions are in that same untested region: they live inside the dialog's callback, which no test can reach. What can be said is that the flag is now claimed on the way *in* rather than on the way out, and that the cancel path is an explicit branch rather than an absence — the defect it fixes was found by reading the sequence rather than by a test, and a test for it would need a drivable dialog.

**A whole-app pass over all three milestones together,** on the isolated display: select a product, switch to Rosé Pine Dawn, switch to Turkish on top of it, walk a live warranty, an expired one and a broken folder, click EXPORT on the broken folder and confirm the frame does not change at all, then return to the first product and confirm the frame is byte-identical to where it was. The vault's manifests are checked afterwards for the hand-added `notes` key, because a session that only reads must write nothing, and `stderr` is checked for being empty.

## Done when

All acceptance criteria pass on the laptop. Then: note in CORE §2 that composite font registration is required for export and why, confirm CORE §6 still describes what shipped, mark this file's status `done`, and move on to Chron8.
