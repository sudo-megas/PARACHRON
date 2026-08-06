# Chron8 — About view and polish

**Milestone:** 8 of ~9 (CORE §9)
**Status:** planned
**Builds against:** CORE §1 (identity — the icon, the repo, the maintainer, the licence the About pane names), §4 (the About view in full, layout, app-wide principles), §5 (themes — the pane is themed like everything else), §7 (packaging — Chron9 supplies the release date this milestone renders), §8 (conventions & development rules), §10 (open items — the subtitle and the motto are closed here)

## Goal

The About strip at the bottom of column 1 stops being inert. Selecting it swaps columns 2+3 for a single centred pane carrying the icon, the wordmark, the subtitle, the maker, the version, the release date, both URLs as plain text, the licence and its full bundled text, and the footer motto. Everything CORE §4 lists, nothing it does not.

And the seven milestones behind this one get their loose ends collected in one place. Chron7 said it was the last milestone that adds a feature; this is the one that makes the app finished rather than merely complete — the floor that has never been enforced in code, the stale allowances whose own comments said they would come off, the copy that says it succeeded when it did not, and the doc comments that still describe a milestone as forthcoming when it shipped three ago.

Chron9 then packages what this milestone leaves.

## Scope

**In:** the About pane, swapped into the content area · the full licence text, bundled and readable in the app · `build.rs` stamping a build date · version and licence id read from the manifest at compile time · about twenty new string keys in both languages · CORE §10's two wording items closed · the 1000×700 floor enforced where a test can see it · the stale `#[allow(dead_code)]` allowances and stale doc comments cleared · the copy confirmation told the truth · the exhaustiveness test's duplicate hole closed · the `Paths::resolve` failure row given a folder to name.

**Out (explicitly):** packaging, CI, the `.desktop` file, the `README.md` and everything that ships an artefact (Chron9) · a settings or preferences screen — theme and language have their routes already and CORE §4 describes no third one · a changelog or release-notes pane, which is what the repo is for · checking for updates, which is a network call in an app that has none · opening either URL, which CORE §4 forbids outright · a twelfth theme, a third language, deleting a product, warranty reminders, drag-and-drop — all refused by earlier milestones with their reasons, and none of them become polish by being listed under it · replacing column 1's `ListView` (see the open question below, which is the one thing in this file that is a question rather than a plan).

## The inherited debt this milestone is made of

Seven milestones of honest bookkeeping, collected. Each item names where it was recorded, because none of it is invented here.

**1. The 1000×700 floor has never been enforced anywhere a test can see it.** Chron1 verified it against a real window and said so; Chron5 said "it is the same shape as Chron1's: the 1000×700 floor is enforced by a window manager the harness does not have either"; `ui_tests.rs` says it in a comment beside the columns assertion. That is all true and it is not the whole problem. `Config::load` defaults a field that is *absent or unparseable* — `window_width = 300` parses as a `u32` perfectly well and goes straight into `set_size` unclamped. So the app's floor is entirely a request made to a window manager, and on a session that honours it loosely, or through a hand-edited config, there is no floor at all. Chron1's own note records opening at "~1280×700 logical" from a stored 400×300, which is the window manager being asked politely and answering in its own words.

**2. `Product` still carries an allowance whose comment says it comes off in Chron4.** `data.rs:114` reads "`link`, `warranty_start` and `warranty_end` have no reader until the details column in Chron4, which is what the allowance below is for; it comes off there." Chron4 shipped. The allowance did not come off. Five more `#[allow(dead_code)]` sit in `strings.rs`, `data.rs` and `theme.rs`, and at least one of them — `Themes::set_lang` — has had a real caller since Chron6.

**3. Four doc comments describe shipped milestones as forthcoming.** `strings.rs:5` still says Turkish "may lag until Chron6"; `config.rs:24` still says "the switch arrives in Chron6"; `app.slint:5` still calls the viewer and details panes "placeholders that later Chrons fill in"; `strings.rs:89` and `strings.slint:22` still head a group with "the details column is fleshed out in Chron4". Every one of them is now false, and a false comment is worse than none because it is read as current.

**4. The copy confirmation fires whether or not anything was copied.** Chron4 decided clipboard failure is "a silent no-op — the text is on screen either way", and that decision is right about not raising a dialog. It is not right about the tick. `arboard` returns a `Result`, the confirmation is a boolean pushed on a timer regardless, and a user on a session where the clipboard is unavailable gets told their serial was copied and then pastes the last thing they actually copied. Saying nothing is a defensible silence; saying "copied" is a false statement.

**5. `Key::ALL`'s duplicate check only catches adjacent duplicates.** The test dedups a `Vec` and compares lengths, and `Vec::dedup` removes *consecutive* repeats only. A key listed twice with anything between the two copies passes, and the count assertion beside it passes too because the count is bumped by hand to whatever the list is. The test's own comment calls the missing key "the one mistake this table invites" — this is the other one.

**6. A `Paths::resolve` failure produces a broken row with an empty folder name.** `main.rs` synthesises an `Entry::Broken` when the data directory cannot be resolved, and it has nothing to put in `folder`, so the list renders a broken entry labelled with nothing. It is the rarest state in the app and the only one with no name on it, and it has no test.

**7. `ErrConfigSave` is printed to a stream that does not exist on one of the three targets.** It goes to `stderr`, after `app.hide()`, so on Linux it lands in a terminal the user probably did not launch from — and `main.rs:7` sets `windows_subsystem = "windows"` in release, so on Windows there is no stderr at all. The string is written, translated and unreachable. "Report, never fatal" is the right stance; reporting into a closed pipe is not reporting.

**8. Two things nobody has watched happen.** Chron4: the countdown across midnight — "nobody left the app running overnight to watch it change." Chron2 criterion 7: the window staying responsive under a genuinely large render — "follows from the architecture … but was not measured under load." Both are named here so that Chron9 does not inherit them silently. Only one of them is cheap to close, and this milestone closes that one.

**9. Persistence has never been verified end to end.** Chron5's criterion 3 and Chron6's criteria 6 and 7 all failed to close for the same mechanical reason: `xdotool windowclose` does not make the app exit under `Xvfb`, because `WM_DELETE_WINDOW` needs a window manager to route it, so `persist` never ran and `config.toml` was never rewritten. Three milestones have now written "an honest gap rather than a claim" about the same three lines in `main`. Clamping the loaded size (item 1) puts a second reader on that path, which makes it worth closing properly rather than for a fourth time.

## Prerequisites

Nothing to install and no new dependency. The About pane is `Text`, `Image` and the project's own `Btn`; the licence is a file already in the repo; the version and the licence id are `env!` on variables Cargo already sets; the release date is one line `build.rs` emits. Chron5's palette and Chron6's table are both complete, which is what lets a pane this text-heavy be written without inventing either.

`build.rs` gains its first `cargo:rustc-env=`, which is worth noting because it is also the first thing in this project that makes the binary depend on *when* it was built rather than only on what is in the tree.

## Files to add and change

```
build.rs              # + stamp PARACHRON_BUILD_DATE
src/
├── about.rs          # NEW — what the pane says, and where each value comes from
├── main.rs           # + install about; + clamp the loaded window size
├── strings.rs        # + the About keys, their SAME_IN_BOTH entries, the count
├── details.rs        # copy confirmations tell the truth
├── config.rs         # the floor as a constant the loader can reach
├── data.rs           # the stale allowance off; Product's readers exist now
└── ui_tests.rs       # + the About sections
ui/
├── about.slint       # NEW — the pane, and the licence sheet
├── app.slint         # content := wraps col2+col3; the About strip becomes live
└── strings.slint     # + the About string properties
```

`about.rs` is a module rather than twenty lines in `main.rs` for the reason `details.rs` and `theme.rs` are modules: `main.rs` has been wire-up only since Chron1, and the About pane owns a build date, a version, a licence blob and an open/closed state. Chron5 wrote the rule down — "putting it beside `main`'s wire-up would mean `main.rs` held eleven palettes of data and stopped being wire-up only" — and this is the same shape with different data.

`ui/about.slint` holds both the pane and the licence sheet. The sheet uses `Sheet` from `ui/sheet.slint` with its `min-card-height`, which Chron5 added for exactly this ("a list that should be scrollable rather than short"), and the `Flickable` recipe `ui/viewer.slint` already uses.

## Tasks

### The swap

- [ ] `app.slint`: wrap `col2` and `col3` in a `content := Rectangle { x: col1.width; width: body.width - col1.width; }` and move the `if` inside it. The two columns keep referencing each other by id — which is the whole reason for the wrapper, see Technical notes — and `assert_columns` keeps finding `col1`/`col2`/`col3` unchanged
- [ ] `app.slint`: `about-open` as a private property beside `menu-open` and `theme-open`; opening a pane is a UI gesture and Rust only needs to hear what is chosen
- [ ] `app.slint`: the About strip gains an `id`, a `TouchArea`, hover, the accessible quartet the other rows carry (`accessible-role`, `accessible-label`, `accessible-action-default`), and `Palette.text` when it is the active view rather than `Palette.muted` always
- [ ] `app.slint`: Escape closes About, through the same `KeyBinding` route the form uses
- [ ] Selecting a product while About is open closes About and shows that product (Technical notes)

### The pane

- [ ] `about.slint`: the pane — icon, letter-spaced wordmark, subtitle, then the label/value rows, the not-a-link note, the licence row, and the italic motto — centred, `no text of its own`, following the `Viewer` and `Details` boundary
- [ ] `about.slint`: label/value rows reuse the `DetailRow` shape from `details.slint` rather than a second one
- [ ] `about.slint`: the two URLs render as plain text with the copy affordance and the same single-shot confirmation the serial strip and the purchase link share — no browser, ever (CORE §4)
- [ ] `about.slint`: `read the full license` opens a `Sheet` holding the bundled text in a `Flickable`, with a Close button; the backdrop does not dismiss, as in every other sheet
- [ ] `about.rs`: version from `env!("CARGO_PKG_VERSION")`, licence id from `env!("CARGO_PKG_LICENSE")`, build date from `env!("PARACHRON_BUILD_DATE")`, licence text from `include_str!("../LICENSE")`
- [ ] `build.rs`: emit `cargo:rustc-env=PARACHRON_BUILD_DATE=<YYYY-MM-DD>`, formatted for display through the same `DD-MM-YYYY` rule CORE §3 sets for every other date on screen
- [ ] `about.rs`: install — push the values once at startup; they are language-independent and therefore survive a language switch without a `set_lang` (Technical notes)

### Strings

- [ ] `strings.rs` / `strings.slint`: the About keys in both languages, in one `// About (Chron8)` group — subtitle, maker label, version label, release-date label, source label, docs label, the not-a-link note, licence label, the read-the-licence entry, the licence sheet's title, the motto, the wordmark, the `ⓘ` glyph, the maker name, and both URLs
- [ ] `strings.rs`: `Key::ALL` gains every one of them, and the count assertion moves off 88
- [ ] `strings.rs`: `SAME_IN_BOTH` gains the keys that are identical by nature — the wordmark, `sudo-megas`, both URLs and the glyph — each with its reason, in the style of the nineteen already there
- [ ] `strings.rs`: the duplicate check sorts before it dedups, so a non-adjacent duplicate is caught
- [ ] `main.rs`: `apply_strings` gains every new key and stays exhaustive
- [ ] CORE §10: the subtitle is `Paper Vault` / `Belge Kasası` and the motto is `Built with Reason and Passion` / `Akıl ve Tutkuyla`; strike the open item

### Polish

- [ ] `config.rs`: `MIN_WIDTH` / `MIN_HEIGHT` constants beside the defaults, and `load` clamps `window_width`/`window_height` up to them
- [ ] `main.rs`: size the shown window from the clamped values, so the floor holds without a window manager's help
- [ ] `details.rs`: the copy confirmation is shown only when the clipboard write succeeded; a failure stays silent, as Chron4 decided, but stops claiming otherwise
- [ ] `data.rs`: `#[allow(dead_code)]` off `Product`; audit the other five and remove each one that no longer allows anything
- [ ] `strings.rs`, `config.rs`, `app.slint`, `strings.slint`: the four stale doc comments corrected to describe what shipped
- [ ] `main.rs`: the `Paths::resolve` broken row carries a name, through the string table, and gains a test
- [ ] `main.rs`: `ErrConfigSave` reaches somewhere a user could see it, or the code says plainly why it cannot (Technical notes)
- [ ] Close the persistence gap: a test that exercises the load → clamp → size → read-back → `persist` path without needing a window manager to deliver a close event

## Acceptance criteria

1. Clicking the About strip replaces columns 2+3 with the About pane; column 1 stays live and the strip reads as the active view.
2. The pane carries every row CORE §4 lists: the icon, `P A R A C H R O N` letter-spaced, the subtitle, maker `sudo-megas`, the version, the release date, the source URL, the docs URL, the not-a-link note, `AGPL-3.0-only`, the read-the-licence entry, and the italic motto.
3. The version equals `Cargo.toml`'s `version` and the licence equals its `license`, both without either being written a second time anywhere.
4. The release date is the date the binary was built, rendered `DD-MM-YYYY` like every other date in the app.
5. `read the full license` shows the bundled AGPL text, scrolled, in full — the first line and the last line are both reachable — and closes without disturbing the pane behind it.
6. Clicking either URL copies it and shows the same confirmation the serial strip and the purchase link show. No browser opens, ever.
7. Switching to Turkish while About is open relabels the pane at once; the version, the release date, the URLs, the maker and the licence id are unchanged, because none of them is UI copy.
8. Escape closes the pane; clicking the strip again closes it; selecting a product closes it and shows that product.
9. The pane fits inside the content area at the 1000×700 floor in both languages, with nothing clipped and nothing overlapping — every row's own top and bottom inside the pane, the assertion Chron5's third defect taught.
10. The pane is themed: it reads every colour from `Palette` and looks correct in all eleven themes, the licence sheet included.
11. A `config.toml` holding `window_width = 300`, `window_height = 200` opens a window of at least 1000×700, and the file is rewritten with what was actually used.
12. A clipboard write that fails shows no confirmation, and the app carries on.
13. The data directory failing to resolve produces a broken entry with a readable name and a readable reason, not a blank row.
14. `cargo build` and `cargo test` are both warning-free — the distinction Chron6 paid for, and worth re-checking in a milestone that removes allowances.
15. `grep -rn` for user-visible literals in `.slint`/`.rs` finds none outside `strings.rs`, with the bundled licence text the single stated exception (Technical notes).
16. `grep -rn` for colour literals in `ui/` still finds none outside `palette.slint`.
17. `git log` shows only `sudo-megas` as author and no AI attribution anywhere.

## Technical notes

**The swap has one real obstacle, and it is an id.** `col3`'s geometry is written as `col1.width + col2.width` and `body.width - col1.width - col2.width`, and the comment above it explains why: column 3 takes the remainder so rounding can never leave a seam. That means `col3` references `col2` by id — and an element inside an `if` cannot be referenced by id from a sibling, so the obvious `if !about-open: col2 := …` does not compile. Re-expressing the arithmetic off `body.width` alone would compile and would quietly give up the seam guarantee that `assert_columns` pins to half a pixel. Wrapping both columns in one `content` element and putting the `if` inside keeps the ids adjacent, keeps the remainder arithmetic, keeps the three assertions passing unchanged, and is the smaller diff. Making the columns `visible: false` instead is the third option and the wrong one: an invisible element is still realised, so an element-id lookup would find two content trees at once and the headless tests would stop meaning what they say.

**About is a pane, not a sheet, and that is the whole language-switch question.** Chron6 wrote down why the form and the theme picker cannot go stale: their backdrops fill the window, so `Document ▾` is unreachable while they are up, and they are rebuilt from scratch every time they open. `lang.rs` carries the same sentence as a trip-wire — "if a later milestone makes a sheet dismissable by clicking away, or puts a menu above one, that is the sentence that stops being true." An About *pane* leaves column 1 and the title bar live, so the language can change while it is on screen. The trip-wire is not tripped, because the answer is to not compose anything: every label in the pane is bound to `Strings` and follows `apply_strings` immediately, and every *value* — version, build date, URLs, maker, licence id — is language-independent and pushed once at startup. So About needs no `set_lang`, no fifth owner, and no row in `lang.rs`'s table of composed sites. That is the cheapest correct design and the reason to take it is that the alternative adds a sixth thing to a list whose only safeguard is that it has exactly one caller.

**The bundled licence is not UI copy, and this needs saying out loud.** Every milestone since Chron1 has defended its literals against the same sweep, and this one embeds 34,020 bytes of verbatim English legal text with `include_str!`. It does not go through the string table and it must not: the AGPL is a legal instrument whose text is the thing, translating it would be a misrepresentation, and paraphrasing it in a `(en, tr)` tuple would be worse. The *entry that opens it* is UI copy and is keyed in both languages; the text behind it is a file in the repository, quoted exactly, which is also what CORE §1 means by the source staying public. The sweep's allow-list gains one entry with that reason attached, the way Chron3's gained the on-disk identifier fallbacks.

**The floor becomes code rather than a request.** Slint's `min-width`/`min-height` are constraints handed to the window manager, and a window manager is free to be approximate — Chron1 asked for 1000×700 and got 1280×700. That is not a bug to fix in Slint; it is a reason not to have the floor exist in only one place. `Config::load` learns the floor and clamps what it read, so a hand-edited or corrupted config cannot open a window narrower than the layout was designed for, and the clamp is a pure function of two numbers, which means it is testable without a display at all. The `.slint` constraint stays exactly as it is — the two are belt and braces, and CORE §4's number is written once in each of the two languages that need it.

**Column 3 has no room to spare, and About is not column 3.** Chron4 recorded that column 3 clips and has no scroll container, deliberately: it is built to fit at the floor. The About pane inherits none of that, because it gets columns 2+3 together — at least 750 logical pixels at the floor — which is why it can afford a large icon and a full-width motto. The licence sheet is the one part that genuinely needs to scroll, and it is a sheet precisely so that it can.

**Selecting a product closes About.** The alternative is that the click lands, the selection changes, and nothing visible happens because the columns that would show it are covered — a list that looks broken. Closing on selection also means there is exactly one way to be looking at a product and one way to be looking at About, rather than a hidden third state where a product is selected underneath. The theme picker and the form are sheets and behave differently on purpose: they are modal, About is a view.

**An export in flight survives About being opened.** The export status line lives in column 3, so it is not drawn while About is up, and its property is untouched — `details::show` clears the status on a change of *product*, and opening a pane is not one. Closing About shows whatever the line says by then. This is stated because Chron7 spent a commit on two fixes that cancelled each other out in exactly this area, and the next person to touch it should not add a third clear for symmetry.

**The `ⓘ` glyph goes in the string table.** CORE §4's wireframe draws the strip as `[ⓘ About]`, and Chron2 established that a glyph on screen is a literal like any other — `‹`, `›` and `⧉` are all keys. It is identical in both languages, so it joins `SAME_IN_BOTH` with the other glyphs, whose comment already says a key whose two sides are equal is not an unfinished translation.

**Turkish shouts without a dot.** Nothing in the pane shouts today, and if a heading ever does, it is stored shouting in both tables and never passed through `to_uppercase` — `HAKKINDA`, dotless, not `HAKKİNDA`. The rule and its test already exist for `TEMA` and `DIŞA AKTAR`; a new shouting label joins that assertion rather than getting its own.

**The confirmation is a claim, so it needs to be true.** Chron4 chose silence for a clipboard failure and gave a good reason: the text is on screen either way, and a dialog for a failed copy would be noise. The tick is a different thing from a dialog — it is the app saying the clipboard now holds this. `arboard` already returns a `Result` and the call site already discards it; showing the confirmation only on `Ok` costs one branch and changes a false statement into no statement, which is what Chron4 actually decided.

**Removing an allowance is a test, not a tidy-up.** `#[allow(dead_code)]` on `Product` was correct when written and has been wrong since Chron4; taking it off asks the compiler whether the fields really do have readers now. If one of them does not, that is a finding and not a reason to put the allowance back. The same applies to the other five: an allowance that no longer allows anything is a comment claiming a state of affairs that ended.

**`ErrConfigSave` has no honest home, and the milestone says which.** It fires after the window is hidden, so there is nowhere on screen to put it, and on a release Windows build there is no stderr either. Three options: write it before hiding, which means checking a save that has not happened yet; keep a window alive to show it, which turns "the app is closing" into "the app is asking about something"; or leave it and write down that on Windows a failed config save is silent. The third is the honest one and it is what this milestone records — with the `eprintln!` kept, because on Linux it does reach a terminal and on a target where it does not, printing costs nothing. What changes is that the limitation is written down instead of being a surprise.

**Closing the persistence gap without a window manager.** Three milestones have written the same paragraph about `xdotool windowclose` not making the app exit under `Xvfb`. The part that is genuinely untestable is the event delivery; the part that has been going untested with it is the arithmetic — load, clamp, size, read back, persist. Clamping puts a second reader on that path, so it is worth extracting the read-back-and-persist step into something a test can call with a size and a config path, and asserting the round trip. The remaining gap is then the three lines in `main` that call it, which is a genuinely smaller claim than the one Chron5 and Chron6 both had to make.

**The one open question in this file.** Column 1's product list is a `std-widgets` `ListView`, and its scrollbar is the last thing on screen that Chron5 could not theme. It appears at about seventeen products. CORE §10 records it and frames replacing it as "a different job from replacing a slider", because a `ListView` virtualizes and a hand-rolled list would not. Both answers are defensible: leaving it means shipping one element drawn in the Slint style in a themed app, and replacing it means betting that nobody's vault is large enough for a non-virtualizing list to matter — which is a bet about somebody else's data, and the sort of bet this project has consistently declined to make on their behalf. It is left out of this milestone's scope and out of its criteria deliberately, and it stays in CORE §10 as an open item rather than being closed by silence. **This one is the user's call, not the milestone's.**

## How the criteria were verified

Written when the milestone is done, as in Chron1–7.

## Done when

All acceptance criteria pass on the laptop. Then: strike CORE §10's About-subtitle-and-motto item and record the chosen wording, note in CORE §4 that the release date is the build date and where it comes from, note in CORE §7 that `build.rs` now stamps a value CI will want to control, record whatever the `ListView` question is answered with, mark this file's status `done`, and move on to Chron9.
