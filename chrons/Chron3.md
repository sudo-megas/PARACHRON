# Chron3 — Add and edit products

**Milestone:** 3 of ~9 (CORE §9)
**Status:** in progress
**Builds against:** CORE §3 (data model — the write half), §4 (layout, `Document ▾`, app-wide principles), §7 (packaging — the picker must not add a build dependency), §8 (conventions & development rules), §9 (roadmap — see the note on sorting below)

## Goal

Parachron stops being a read-only viewer of hand-written TOML. `Add Document` opens a form that writes a real product folder — manifest, dates, and PDFs copied in from wherever the user keeps them. `Document ▾` reopens that form on the selected product so a date can be fixed, a name corrected, or **another PDF attached months later**. Everything appears in the list immediately, with no restart.

The milestone is also where the product list gains an owner. Until now the list was built once at startup and handed away; nothing could change it afterwards. Chron3 builds that seam, and Chron4 sorts through the same door.

## Scope

**In:** the vault seam (runtime list ownership, selection by identity, re-scan) · the data write half (manifest serialization, atomic writes, folder naming) · `Document ▾` menu · add/edit form as an in-window sheet · PDF import with validation · removing an attached PDF · two Chron2 defects the seam exposes · `SortMode` and its comparators (see §9 note) · new string keys.

**Out (explicitly):** deleting a product (not in CORE §9's line for this milestone, and a vault that quietly eats folders is worse than one that never does) · the details column and the sort *toggles* (Chron4) · themes (Chron5) · Turkish completeness (Chron6) · export (Chron7) · About (Chron8) · packaging (Chron9) · drag-and-drop of PDFs onto the window · reordering a product's PDFs · editing a broken folder through the form (a manifest that will not parse is repaired in a text editor, not guessed at by a form).

## A note on CORE §9

CORE §9 puts "sort toggles" in Chron4. This milestone builds `SortMode`, its three comparators and the `config.sort` plumbing anyway, because the module that owns them — `vault.rs` — exists only to own list order, and shipping it with a hardcoded order would mean rewriting its centre in the next milestone. **What the user can see still lands where CORE §9 says it does:** there are no toggles on screen until Chron4. The table is unchanged; only the seam moved.

## Prerequisites

None new to install. `rfd` is added as a dependency, and its `xdg-portal` backend was chosen partly because it needs no GTK development headers — CORE §7 has to build this on three targets in CI, and a GTK build dependency would be a tax on every one of them. Verified before committing to it: `default-features = false, features = ["xdg-portal"]` pulls five crates (`libc`, `log`, `percent-encoding`, `pollster`, `raw-window-handle`) and checks in about two seconds.

## Two Chron2 defects fixed here

Chron2 fixed a Chron1 defect it tripped over and wrote down why. The same thing happens again, twice.

**1. Stale render responses can paint over the wrong document.** `State::plan` bumps `self.token` only on the path that actually asks for pixels. Every early return — no tabs, no product, viewport not yet measured, or a tab whose file is missing — leaves the token where it was, while `receive` decides whether a response is stale purely by comparing tokens. So: open a product with a slow PDF, then select one whose first tab is in `missing_pdfs`. The new state shows "file missing" without claiming a new token, the old document's response arrives still matching the current token, and it is applied — page image, page count, and a cleared error message, over the top of the missing-file state.

The fix is one line in the right place: bump the token at the *top* of `plan()`, unconditionally, so it is a generation counter for the view rather than a serial number for requests. Chron3 needs this regardless of the bug, because swapping the viewed document underneath an in-flight request is the same mistake with a wider blast radius.

**2. The render worker cannot be told a file changed.** `ensure_open` reuses the currently open document whenever the path matches, and the page cache is keyed by `(path, page, width, height)`. Neither consults the file's modification time. Remove `invoice.pdf` from a product and import a different file under that name in the same session, and the viewer serves the old pixels out of the cache — from a document handle that is still open on bytes that no longer exist — until the app restarts.

The fix is a second message on the render worker's channel: `Message::Invalidate(PathBuf)`, which drops matching cache entries and closes the open document if it is the one named. Every import and every delete sends one. This is a correctness prerequisite for the milestone, not polish.

## Files to add and change

```
Cargo.toml            # + rfd (xdg-portal), + tempfile (dev); time gains "parsing"
src/
├── vault.rs          # NEW — list ownership, selection identity, SortMode, rows
├── editor.rs         # NEW — the form's state machine, validation, callbacks
├── import.rs         # NEW — the picker edge, PDF validation, copying files in
├── data.rs           # + write half; scan stops sorting
├── render.rs         # + Message::Invalidate
├── viewer.rs         # token fix; DocSet replaces the entry list; lang out of captures
├── strings.rs        # + form, menu and error keys
└── main.rs           # + install vault and editor; row()/describe() move to vault.rs
ui/
├── widgets.slint     # NEW — shared Btn and Field, lifted from the two duplicates
├── form.slint        # NEW — the add/edit sheet
├── app.slint         # + menu panel, sheet host, named list, live title-bar buttons
└── strings.slint     # + form and menu properties
```

`vault.rs`, `editor.rs` and `import.rs` are three modules rather than one for the same reason Chron2 pulled `viewer.rs` out of `main.rs`: they are three different kinds of thing. `vault.rs` is state, `editor.rs` is a state machine over user input, `import.rs` is I/O that must not run on the UI thread. Only the last one needs a thread, and only the first two need the window.

## Tasks

### The seam

- [ ] `viewer.rs`: bump `token` at the top of `plan()` so it counts view generations, not requests; add the regression test that fails on today's code
- [ ] `render.rs`: `Message::Invalidate(PathBuf)` — drop matching cache entries, close the open document if it matches; `Renderer::invalidate(path)` to send it
- [ ] `viewer.rs`: replace `State.entries: Vec<Entry>` + `State.selected: usize` with a `DocSet { folder, serial, pdfs, missing }` handed in from outside, and expose `Viewer::show(&self, app, doc: Option<DocSet>, keep_view: bool)`
- [ ] `viewer.rs`: stop registering `on_product_selected` — Slint permits one handler per callback, and the vault needs it
- [ ] `viewer.rs`: `lang` becomes state rather than a value captured into seven closures, so Chron6 can change it without re-registering handlers (which is a panic, not a no-op, from inside a handler)
- [ ] `vault.rs`: `Vault { products_root, entries, sort, selected: Option<String>, lang }`, holding only a `Weak` handle to the window
- [ ] `vault.rs`: `SortMode { Added, Name, Purchase }` and its three comparators, broken entries last under every mode, tie-broken by folder
- [ ] `data.rs`: `scan` stops sorting; `sort_by_added` moves into `vault.rs` as one comparator among three and finally gets a test
- [ ] `vault.rs`: `row()` and `describe()` move here from `main.rs`, which stays wire-up only
- [ ] `vault.rs`: two-phase update — compute an `Update` under a scoped borrow, then push it with no borrow held across any Slint setter
- [ ] `vault.rs`: `rescan(select: Option<&str>)` re-reads the vault and restores the selection by folder
- [ ] `app.slint`: name the list (`list := ListView`) and expose enough of its scroll state that a re-sorted or newly added selection can be brought back into view

### The write half

- [ ] `data.rs`: `write_atomic(path, contents)` — temp file in the target's own directory, then rename; `Config::save` is pointed at it too
- [ ] `data.rs`: manifest serialization, including `Date → toml::value::Datetime`, the inverse `to_date` never had
- [ ] `data.rs`: unknown keys in `product.toml` survive a rewrite (CORE §3: "no hidden state" cuts both ways)
- [ ] `data.rs`: `folder_slug(name)` — Turkish letters mapped before lowercasing, Windows-illegal and reserved names rejected, empty result replaced, collisions suffixed
- [ ] `data.rs`: `Draft`, the validated shape the form produces and the writer consumes

### Import

- [ ] Spike the picker before writing `import.rs`: confirm the window keeps repainting while the dialog is open
- [ ] `import.rs`: picked files validated as real PDFs by reusing `render::open_document` and `render::page_count`, off the UI thread
- [ ] `import.rs`: destination names sanitised and de-duplicated (`invoice.pdf` twice → `invoice-2.pdf`)
- [ ] `import.rs`: copy files in, then `Renderer::invalidate` each destination
- [ ] Add: create folder → copy PDFs → write manifest last. Edit: copy new PDFs → write manifest → delete removed copies last

### The form

- [ ] `widgets.slint`: `Btn` (lifted from `NavButton`, which is the better of the two duplicated recipes — it has the callback and the accessibility attributes) and `Field`, a hand-rolled `TextInput`
- [ ] `form.slint`: the sheet — dim backdrop that swallows clicks without dismissing, centred card, no text of its own
- [ ] `editor.rs`: open in add mode (empty) or edit mode (pre-filled from the selected product)
- [ ] `editor.rs`: validation on Save, then live once a Save has failed, so errors clear as they are fixed
- [ ] `editor.rs`: attach and remove PDFs inside the sheet, including on an existing product
- [ ] `app.slint`: `Document ▾` opens a menu with `Add Document…` and `Edit Document…`; `Edit` is disabled unless a valid product is selected
- [ ] `strings.slint` / `strings.rs`: every new label, button, error and glyph through the table

## Acceptance criteria

1. `Add Document` with a name, three dates and two PDFs writes `~/.local/share/parachron/products/<slug>/` containing `product.toml` and both files; the product appears in the list, selected and scrolled into view, its first PDF rendering — without a restart.
2. Dates typed as `14-03-2026` land in `product.toml` as `2026-03-14` and read back on screen as `14-03-2026`. A `DD-MM-YYYY` string never appears in a `.toml` file.
3. Reopening a product through `Document ▾ → Edit Document…`, attaching a third PDF and saving gives a third tab that renders, and a manifest listing three files in tab order.
4. Removing a PDF in the form deletes Parachron's copy from the product folder; the original file at the path it was imported from is untouched.
5. Removing `invoice.pdf` and importing a *different* file under the same name in the same session renders the new document, not the old one.
6. Renaming a product re-labels the list row and leaves the folder name on disk unchanged.
7. A manifest with a hand-added key the app does not know about still has that key after the product is edited and saved.
8. Saving is refused, with a readable per-field message, for: an empty name, a date that is not a real calendar date, and a warranty that ends before it starts.
9. A picked file that is not a readable PDF is rejected in the form with a readable reason, and nothing is copied.
10. Selecting a product whose first tab is missing while a large PDF is still rendering leaves the missing-file message on screen — the older page never appears.
11. Killing the app between copying PDFs and writing the manifest leaves a folder that shows in the list as broken with a readable reason, not a crash and not a silent half-product.
12. `grep -rn` for user-visible literals in `.slint`/`.rs` still finds none outside `strings.rs`.
13. `git log` shows only `sudo-megas` as author and no AI attribution anywhere.

## Technical notes

**The seam, and why the viewer stopped holding the list.** `viewer::install` took `entries: Vec<Entry>` by value into a private `Arc<Mutex<State>>` it never returned, so nothing could change the list after startup. The obvious repair — keep the entries there and add a way to replace them — was rejected. The viewer reads exactly five fields out of that whole list: the selected product's `serial`, its `pdfs`, its `missing_pdfs`, and its `folder` to build paths. Passing those five in as a `DocSet` removes the second copy of the list entirely, and with it the question of how to re-index a selection after a re-sort. There is no index into a list the viewer does not own.

**Selection is a folder, never an index.** `folder` is already the product's identity — the display name may change or repeat, the folder does not. After a re-scan or a re-sort, the vault finds the selected folder's new position and pushes that. This is also what makes "save an edit and stay on the product you were editing" fall out for free.

**Ordering the pushes.** The list row's click handler writes `selected-index`, `selected-name`, `selected-detail` and `selected-broken` on the Slint side before Rust ever sees the click, and `set_products` does not touch any of them. So all four go stale on a rebuild and all four must be re-pushed: model first, then name and detail, then broken, then index. Two rules on that sequence. `selected-index` must never transit through `-1` on the way to its new value — it gates the conditional that hosts the viewer, and a momentary `-1` tears the viewer down and rebuilds it, which costs the resize debounce before the page comes back. And when the selection really is gone, `selected-broken` has to be cleared with it, or the empty-state prompt renders in the error colour.

**Borrows and re-entrancy.** `Vault` lives behind a `RefCell` and its callbacks call Slint setters, which may run bindings that call back into callbacks. Holding a borrow across a setter is how that becomes a panic. The pattern is the one `apply` already uses in `viewer.rs`: take the borrow, compute a plain-data `Update`, drop the borrow, then push. Not `try_borrow_mut` and not a re-entrancy flag — both of those turn a would-be panic into a silently dropped update, which is worse, because the list is then wrong with nothing to show for it. The vault also holds only a `Weak` window handle; a strong one makes the callback graph a cycle, and the render thread then outlives `main` because `Renderer`'s destructor never runs.

**Atomic writes, and the order things are written in.** A manifest is written to a temporary file in the same directory and renamed over the target — same directory because a cross-device rename fails, and rename because a half-written manifest is the one failure mode CORE §3 says must never happen. The order of operations is chosen for what a crash leaves behind. Adding writes the manifest *last*, so an interrupted add leaves a folder full of PDFs with no manifest, which scans as broken, is visible in the list, and can be finished by hand. Editing writes the manifest *before* deleting removed files, so an interrupted edit leaves an unlisted orphan file rather than a manifest pointing at something that is gone. In both cases the failure is visible and recoverable rather than silent.

**Unknown keys.** The manifest struct is deserialize-only today with no `deny_unknown_fields`, so anything the app does not recognise is dropped on read. That is harmless while nothing writes. Once the form writes, a `notes = "..."` somebody added by hand would disappear on the first edit, which breaks CORE §3's promise that the data outlives the app and holds no hidden state. Unknown keys are carried through the round-trip. Comments and original key order are not preserved — TOML serializers do not keep them — and that limitation is worth stating plainly rather than discovering later.

**Folder names.** `folder_slug` maps Turkish letters *before* lowercasing, because `"İ".to_lowercase()` in Rust yields `i` followed by a combining dot above — a combining mark inside a directory name, which is mojibake in a file manager and normalises differently on other platforms. After the mapping: non-alphanumerics collapse to single hyphens, the result is trimmed and capped, an empty result becomes a fallback name, Windows reserved device names (`CON`, `NUL`, `COM1`…) and trailing dots or spaces are rejected because CORE §7 ships a Windows binary, and collisions get a numeric suffix. Renaming a product never renames its folder.

**The picker is a thin edge.** `rfd` returns a `Vec<PathBuf>` and everything behind that boundary is ordinary code that takes paths. This is not tidiness for its own sake: a portal dialog is drawn by the desktop's own portal service on the real session, so it cannot be driven inside the isolated `Xvfb` display Chron2 established for click-testing. Keeping the dialog at the edge means the whole import path is testable by handing it paths, and only the click that opens the dialog needs a human.

**Validating before copying.** A picked file is opened with the same `open_document` and `page_count` the viewer uses, so "is this really a PDF" has exactly one answer in the app, and encrypted or zero-page files are rejected at the point the user can still do something about it. That work happens off the UI thread, because MuPDF contexts are per-thread and the rule from Chron2 — the UI thread never calls MuPDF — is worth keeping. The render worker is deliberately *not* reused for it: that queue drops all but the newest job, which is correct for pixels and would be silent data loss for a file copy.

**Hand-rolled fields, not `LineEdit`.** The project builds its own buttons on `Rectangle` + `TouchArea` and reads every colour from the `Palette` global. Widgets from `std-widgets` follow the Slint style instead, which means Chron5 could not theme them. `Field` wraps the builtin `TextInput`, which supplies what the form needs — `accepted`, `edited`, `single-line`, `input-type` — and `FocusScope` supplies tab traversal and a `KeyBinding` child for Escape, so the form needs no custom keyboard machinery.

**A menu, not a `PopupWindow`.** `Document ▾` opens a panel positioned under the button with a full-window transparent layer behind it to catch the dismissing click. `PopupWindow` would be the obvious choice, but elements inside one are not reliably realised for the testing backend's element lookup, which would put the entire menu — and the only route to the edit form — outside the headless tests. The hand-rolled panel is a comparable amount of code and stays testable.

**The backdrop does not dismiss.** Clicking outside the sheet does nothing. A half-filled form with three dates in it should not be destroyed by a stray click; Cancel and Escape are deliberate and are enough.

## How the criteria were verified

*(filled in when the milestone is done)*

## Done when

All acceptance criteria pass on the laptop. Then: note the `rfd` choice and its feature set in CORE §2, record in CORE §9 that `SortMode` shipped early while the toggles did not, mark this file's status `done`, and ask user permission to start Chron4.
