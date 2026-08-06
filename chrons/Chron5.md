# Chron5 — Theming

**Milestone:** 5 of ~9 (CORE §9)
**Status:** done
**Builds against:** CORE §4 (layout, column 3's THEME button, app-wide principles), §5 (the eleven themes), §7 (packaging — palettes are baked in, nothing to install), §8 (conventions & development rules), §10 (open items — this is where the hex sets get pinned)

## Goal

The THEME button stops being a disabled stub. All eleven palettes from CORE §5 ship inside the binary, a picker lists them, and choosing one repaints the window at once and is still in effect after a restart.

Chron1 put every colour behind the `Palette` global precisely so this milestone would be a contained change, and Chron2 lifted that global into its own file for the same reason. Chron5 is the milestone that collects on both — with one correction and one honest exception, both below.

## Scope

**In:** the eleven palettes as a table in Rust · `Palette` becomes a pushed global rather than a hardcoded one · the theme picker as a sheet · instant switching · persistence through `config.toml` · the one colour literal still outside the palette · theme names through the string table · a contrast floor every palette has to clear.

**Out (explicitly):** Turkish completeness for the theme names (Chron6, which is where the switch that makes them change lands) · EXPORT (Chron7) · About (Chron8) · packaging (Chron9) · a twelfth theme, or a theme read from a file on disk — CORE §5 says the themes are baked into the binary, and a user-supplied palette is a different feature with a file format and a validation story of its own · following the desktop's light/dark preference, for the same reason CORE §4 refuses to read the system locale: the choice is the user's and it is deliberate · per-theme fonts, radii or spacing — a theme is a colour table and nothing else · a literal gradient for the Paperlike theme (see the note below).

## Prerequisites

None new to install. No new dependency: the palettes are `const` data in Rust and eleven of them cost nothing that matters.

## A correction to Chron1 and Chron2

Both said every colour lives in the `Palette` global. That was true when each wrote it and is not true now. Chron3's sheet needed a dim backdrop and `form.slint` grew

```slint
background: #000000.with-alpha(0.55);
```

which is the only colour literal in `ui/` outside `palette.slint`. It is one line, and it is exactly the line that would have made "switch the palette and the whole window follows" quietly false — a sheet open over a light theme would dim it with the same 55% black that suits a dark one. `backdrop` becomes a palette role like any other, and the sweep for colour literals becomes an acceptance criterion so the claim stops drifting away from the code (the same job criterion 12 does for strings).

`Palette` is also not exported from `app.slint`, which lists only `Strings`, `DocTab` and `FormDoc`. `slint::include_modules!` generates Rust bindings for exported globals only, so `app.global::<Palette>()` does not compile until the export list grows by one word. Verified before anything else in this milestone was written, because the whole approach depends on it.

## Three more defects, found by building it

Chron2 fixed a Chron1 defect it tripped over. Chron3 fixed two of Chron2's. The pattern holds: making the palette real turned up three things that were wrong before this milestone started, and all three are in scope because all three are a themed colour that did not reach the screen.

**1. `paper-edge` was never drawn at all.** `viewer.slint` set it as `border-color` on the rectangle holding the page, and put the `Image` at that rectangle's origin at the full page size. A Slint border is inset *within* the element's own bounds and children paint after it, so the image covered the border completely — every time a page was loaded, which is every time there is anything to look at. The colour had been in the palette since Chron2 and had never once been visible.

The fix draws the edge as its own rectangle one pixel outside the page rather than as a border inside it. That also keeps Chron2's other rule intact: the image stays at exactly the pixels it was rasterized for. Insetting the image by a pixel would have been the obvious fix and would have traded this defect for resampling every page.

This was found by asserting, in a test, that `paper-edge` was distinguishable from the pane behind it — and then asking why the assertion mattered.

**2. The zoom slider could not be themed.** `widgets.slint` opens by explaining that nothing from `std-widgets` is used, because those follow the Slint style rather than the `Palette` global and "would leave them the only things on screen that Chron5 could not theme". `viewer.slint` then imported `Slider` from `std-widgets`. On Default Dark it passes for a muted control; on Catppuccin Latte and Paperlike it was a bright blue-orange object that stayed exactly the same whatever theme was chosen — the one thing on screen that visibly did not belong. No test would ever have caught it: a test can only check a colour it pushed, and nothing pushed this one. A screenshot caught it immediately.

`Slider` is now hand-rolled beside `Btn`, `GlyphBtn` and `Field`, which is what the file said all along.

**3. The picker hid its eleventh theme, and the test agreed that it did not.** The first version put the rows in a `ListView`. A `ListView` virtualizes, so it takes a height rather than giving one, and the height it was given left Paperlike below the fold behind a scrollbar. The headless test asked whether all eleven rows were *realised* and got yes — because a `ListView` realises a row just past the edge of its viewport. The test was checking something adjacent to what it claimed.

Eleven rows of 32px is 352px, and CORE §4 puts a hard 700px floor under the window, which leaves 640px of card: there was never anything to virtualize. The rows are a plain layout now, and the test asks where each row actually *is* relative to the card rather than whether it exists.

## Files to add and change

```
src/
├── theme.rs          # NEW — the eleven palettes, the picker's state, pushing colours
├── main.rs           # + install the theme; persist() takes the session's theme
├── config.rs         # theme is a real setting now, not a placeholder
└── strings.rs        # + eleven theme names and the picker's chrome
ui/
├── palette.slint     # every colour `out` → `in`, filled from Rust, defaults kept
├── theme.slint       # NEW — the picker sheet
├── sheet.slint       # NEW — the backdrop/card recipe, lifted out of form.slint
├── app.slint         # + export Palette; + host the picker
├── details.slint     # THEME becomes live
├── form.slint        # the sheet recipe and the backdrop colour both come from elsewhere
├── widgets.slint     # + Slider, hand-rolled (defect 2)
├── viewer.slint      # the page's edge is drawn (defect 1); the new Slider
└── strings.slint     # + theme names and picker chrome
```

`theme.rs` is a module rather than a table inside `main.rs` for the reason `details.rs` is one: it is a pure function of a theme id plus one callback, and putting it beside `main`'s wire-up would mean `main.rs` held eleven palettes of data and stopped being wire-up only.

`sheet.slint` exists because Chron5 adds the second sheet. The form's backdrop-and-card recipe is a dozen lines of layout with three decisions baked into it — the backdrop swallows clicks without dismissing, Escape cancels, the card is centred and capped against the window — and two copies of that would drift the way Chron1's and Chron2's two button copies drifted before Chron3 lifted `Btn`. Lifting it on the second use rather than the third is the cheap moment.

## Tasks

- [x] `app.slint`: add `Palette` to the export list, and confirm `app.global::<Palette>()` compiles
- [x] `palette.slint`: every property `out` → `in`, keeping its current value as the initializer; `+ backdrop`
- [x] `theme.rs`: `Theme` — the eleven ids, their display-name keys, their mode, and `from_code`/`code` with a fallback for a config somebody has typed into
- [x] `theme.rs`: `Palette` as plain Rust data, one `const` per theme, and `push(app, palette)`
- [x] `theme.rs`: the contrast floor, as a test over all eleven — no palette ships unreadable
- [x] `sheet.slint`: `Sheet` — dim backdrop that swallows clicks without dismissing, Escape, centred card
- [x] `form.slint`: rebuilt on `Sheet`; the hardcoded backdrop colour goes
- [x] `theme.slint`: the picker — eleven rows, the active one marked, all of them visible at the 700px floor
- [x] `widgets.slint`: `Slider`, hand-rolled, because the `std-widgets` one cannot be themed
- [x] `viewer.slint`: draw the page's edge outside the page, so the palette's colour actually appears
- [x] `app.slint`: host the picker, and keep whether it is open on the Slint side
- [x] `details.slint`: THEME loses `enabled: false` and calls out — the only route to the picker, because CORE §4 gives the theme a button in column 3 and a second entry in `Document ▾` would be a route the specification does not describe
- [x] `theme.rs`: choosing a theme pushes the palette immediately and leaves the sheet open
- [x] `main.rs`: `persist` writes the session's theme rather than carrying the loaded one through
- [x] `strings.slint` / `strings.rs`: eleven theme names and the picker's chrome through the table
- [x] `grep` sweep: no colour literal anywhere in `ui/` outside `palette.slint`

## Acceptance criteria

1. The THEME button opens a picker listing all eleven themes from CORE §5, with the active one marked.
2. Choosing a theme repaints the window immediately — list, viewer chrome, details column, sheet, hairlines, buttons and chips — with no restart and no flicker of the previous colours.
3. The chosen theme is still in effect after quitting and reopening the app.
4. A `config.toml` naming a theme that does not exist falls back to Default Dark rather than refusing to start.
5. Every one of the eleven palettes clears the contrast floor for body text on its background, muted text on a panel, and the error colour on a panel.
6. The add/edit sheet's backdrop follows the theme: over a light theme it dims a light window, not a dark one.
7. The rendered page keeps a white sheet under it in every theme, and its edge stays visible against the pane behind it.
8. At the 1000×700 floor the picker shows every theme, scrolling if it must, and never overflows the window.
9. `grep -rn` for colour literals in `ui/` finds none outside `palette.slint`.
10. `grep -rn` for user-visible literals in `.slint`/`.rs` still finds none outside `strings.rs` — the eleven theme names included.
11. `git log` shows only `sudo-megas` as author and no AI attribution anywhere.

## Technical notes

**The palettes live in Rust, not in Slint.** The obvious Slint answer is an `in property <int> theme` on the global and eleven-way conditionals on each of twelve colours — a hundred and thirty-two branches that the compiler cannot check for completeness and no test can reach. The palettes are Rust data instead, pushed through twelve setters exactly the way `apply_strings` pushes the string table. Three things fall out of that. The `Palette` global stops holding knowledge and becomes a slot, which is what it was always pretending to be. Adding a theme is one `const` and one line in a list, in a file that is already `#[cfg(test)]`-covered. And the palettes become testable — which matters more than it sounds, because the alternative to a contrast test is looking at eleven screenshots and believing yourself.

**Defaults stay in `palette.slint`.** Turning `out property <color> bg: #1b1b1d;` into an `in` property is a one-word change and the initializer is worth keeping. Without it the window is built with unset colours and paints once in whatever `#000000` means before Rust's push lands, which is a visible flash on every start. It also keeps `ui_tests.rs` working: that harness calls `apply_strings` and nothing else, and a headless window rendering with no colours at all is a different thing from the one the user sees. The initializers are Default Dark, which is `config.toml`'s default, so the pre-push frame and the post-push frame are identical for the common case and the flash is not merely hidden but absent.

**Paper stays white in every theme.** `render.rs` rasterizes with `alpha: false`, so a page arrives already opaque and white-backed, and the `Image` covers the paper `Rectangle` exactly. A themed `paper` would therefore be invisible except for the instant before the first page lands, where it would flash a colour the page is not. Only `paper-edge` varies — it is the line that stops a white page reading as a hole punched in a dark pane, and on a light theme it has to get lighter or it reads as a black frame. This is stated here so that nobody later "finishes the job" by theming the paper.

**Light themes invert the hover step, not the panel step.** Every palette keeps `panel` lighter than `bg` — the title bar, the list and column 3 sit above the canvas in both modes, which is what makes the three-column structure legible without borders doing all the work. What flips is `raised`: on a dark theme a hovered button gets lighter than its panel, on a light theme it gets darker. Both are "more contrast than at rest", which is the thing the role actually means; taking it literally as "lighter" is what makes hand-converted light themes look washed out.

**The ladder each palette follows.** Five surface roles, in order of distance from the canvas: `bg`, `panel`, `raised`, `selection`, `border`. Default Dark already had them in that order and the other ten follow it, so a theme can be read as a ladder rather than as twelve unrelated colours, and a new one is built by walking a source palette's surface tokens in order. Where a source palette has fewer steps than five, the missing one is interpolated rather than repeated — two roles sharing a value is how an active tab loses its border or a selected row stops looking selected.

**Which palettes are upstream and which are interpretations.** CORE §10 asks for the exact hex sets to be pinned in the first UI milestone, and this is it. Five are taken from their projects' published palettes and can be checked against them: Catppuccin Latte, Frappé, Macchiato and Mocha, and Rosé Pine — Dawn, which is what CORE §5's "light/dawn" means. Ubuntu Canonical Aubergine uses Canonical's published brand colours: Aubergine `#2c001e`, Ubuntu Orange `#e95420`, Warm Grey `#aea79f` and the aubergine ladder between them. The remaining four — Default Light, Default Dark, Noctalia, Ruby and Paperlike — are interpretations pinned here, because Default Dark was only ever this project's own and the others have no single published hex set to copy. Saying which is which matters: somebody comparing Mocha against the upstream swatch should find it identical, and somebody who thinks Ruby should be redder is disagreeing with a choice rather than reporting a bug.

**Ruby's error colour leaves the red family.** In a red-forward theme, `danger` cannot also be red — a broken folder and an expired warranty have to look like something is wrong, and in a window whose accents are already ruby, another red is just more of the theme. Ruby's accent is a ruby rose and its `danger` is amber. That is a deliberate departure from "danger is red" and the only theme where the two roles are different hues on purpose.

**Paperlike is a ladder, not a gradient.** CORE §5 calls it a gradient theme, and the palette is a table of flat colours. Rendering an actual `@linear-gradient` would mean every themed `background:` in every `.slint` file taking a brush rather than a colour, in ten places that currently take `Palette.something` — a change to the whole UI's colour plumbing bought for one theme out of eleven. What ships is the warm near-white ladder that gradient implies, applied to the surface roles. CORE §5's name is kept because the theme is recognisably that theme; this note is the honest footnote on it, and a real gradient is a later change to the palette's *type*, not to its values.

**The picker is a sheet, not a dropdown.** Column 3 is a quarter of a window whose floor is 1000px, so about 250px wide, and eleven rows of thirty pixels do not hang under a button in it. A centred sheet is also the shape the project already has for "a modal thing with rows in it", and the reason Chron3 hand-rolled its menu rather than using `PopupWindow` applies here unchanged: elements inside a popup are not reliably realised for the testing backend's element lookup, which would put the entire picker outside the headless tests.

**Choosing applies at once, and the sheet stays open.** CORE §5 says switching is instant, so a row click pushes the palette immediately — including repainting the sheet the click happened in, which is the most direct demonstration the theme took. The sheet then stays up so eleven themes can be compared without reopening the picker eleven times, and there is a Close but no Cancel: there is nothing to cancel, because the change is already in effect and undoing it is one more click on the row above. A picker that previewed on hover and committed on click would be nicer still and is a different feature — it needs a "what was I on before" to restore, which is state this milestone would rather not own.

**Writing the theme when it changes, or not.** `config.toml` is written once, at shutdown, and Chron4 settled that policy for the sort mode. The theme follows it: `persist` takes the session's theme instead of the loaded one. This is the same defect Chron4 found and fixed one field over — `persist` spreads `..settings`, so before this milestone the theme survived from load and a change made in the picker would have been thrown away at exit. `main.rs`'s own test asserts that an unrelated setting survives a save, using `theme` as the unrelated one; that assertion has to change, and it changing is the evidence the plumbing landed.

**The contrast floor, and what it is not.** Each palette is checked for a relative-luminance ratio against WCAG's formula: body text on `bg` and on `panel`, `muted` on `panel`, and `danger` on `panel`. `muted` is held to the large-text floor rather than the body-text one, because it is used for labels and secondary lines and holding a deliberately quiet colour to 4.5:1 would mean it was not quiet. This is a floor, not a design review: it catches a light theme built by inverting a dark one and forgetting the accents, which is the failure this table invites, and it says nothing about whether Frappé is pretty.

## How the criteria were verified

106 tests pass (`cargo test`), up from Chron4's 94, with no warnings.

**Automated, and this is where the palettes earn their place in Rust.** Twelve tests walk all eleven themes. Body text clears 4.5:1 against both `bg` and `panel`; `muted`, `accent` and `danger` clear 3.0:1 against `panel`. The contrast function is checked against WCAG's own reference points first — black on white is 21:1, white on white is 1:1, and the ratio is symmetric — because an unverified metric would let every palette pass for the wrong reason. The five surface roles are all distinct and step monotonically away from the canvas in whichever direction the theme runs, which is what catches a light theme converted from a dark one with `raised` left on the wrong side. `panel` is lighter than `bg` everywhere. `paper` is white everywhere. Every backdrop is translucent and none is opaque. No two themes are the same twelve values, no two share a config id or a name key, `Theme::ALL` has eleven entries with no duplicates, ids round-trip through `config.toml` and an unknown id falls back to Default Dark. Default Dark is asserted to still be Chron1's exact palette, because that is what makes a default start flash-free — if it drifts from `palette.slint`'s initializers the app flashes on every launch and nothing else would notice.

**The contrast floor found a real defect in a palette I had just written.** Ruby's `paper-edge` was a near-black one step off its near-black `bg`, which reads as no edge at all. Fixing it properly meant admitting the rule was wrong rather than the value: on a dark theme no shadow can be meaningfully darker than a canvas that is already nearly black, and it does not need to be, because white paper against a near-black pane is already a hard boundary. The rule is now that a page must be distinguishable from its pane *either* by its own brightness *or* by its edge — which is a statement about the design rather than a threshold fitted to eleven data points, and it is what made defect 1 above visible.

**Headless, through the real element tree.** `ui_tests.rs` clicks the THEME button, counts the picker's rows against `Theme::ALL`, checks that exactly the active theme is marked, clicks Catppuccin Latte, and then asserts the `Palette` global's `bg`, `text` and `backdrop` alpha are the values `theme.rs` holds for it — so the palette was pushed, not merely recorded. It checks the tick moved, that the sheet stays up after a choice, that re-choosing the active theme is a no-op, and that Close closes. At the 1000×700 floor it asserts the card lies inside the window and that every row's own top and bottom lie inside the card, which is the assertion that replaced the one defect 3 slipped through.

**By real clicks, on an isolated display, against a scratch vault.** `Xvfb :98` at exactly 1000×700 — CORE §4's floor, the hardest case for the picker and the size the headless test measured, so the click coordinates are measured rather than guessed. `XDG_DATA_HOME` points at a temporary directory. The vault is seeded with a product named `Şarj Cihazı` whose serial is `İST-0042-ĞŞ`, a product with an expired warranty, and a folder with no manifest, so every state column 1 and column 3 can be in is on screen at once.

All eleven themes were applied by clicking their rows, each confirmed against a screenshot:

| Theme | Result |
|---|---|
| Default Dark → Default Light | Whole window inverts; accents darken rather than staying dark-theme bright |
| Catppuccin Latte | Panels, list, viewer chrome, details column, hairlines and both buttons follow; the zoom slider is Latte's blue |
| Catppuccin Frappé, Macchiato, Mocha | Each distinctly its own flavour, matching the upstream swatches |
| Rosé Pine Dawn | Warm light surfaces, pine accent, love as the error colour |
| Ruby Theme | Wine surfaces, ruby-rose accent, `Expired` in amber — the one theme where the error colour is not red, working as designed |
| Ubuntu Canonical Aubergine | Aubergine ladder, Ubuntu Orange accent, Warm Grey for the quiet text |
| Paperlike | Warm near-white ladder, slate-blue ink, iron-gall red |
| Noctalia | Near-black blue with the lavender accent |

**Criteria 2 and 6 were checked numerically rather than by eye,** because "looks about right" is exactly the standard a scrim is easy to get wrong against. Sampling the same two points in column 1 and column 3 with the sheet closed and open: on Catppuccin Latte, `#EFF1F5` closed — byte-identical to the palette's `panel` — and `#838487` open, against `#83848B` predicted for a 45% scrim. On Default Dark, `#232326` closed, again byte-identical, and `#101011` open against `#0F0F11` predicted for 55%. So the palette reaches the screen exactly, and a light theme is dimmed by a light theme's scrim.

**Criterion 8** is verified twice, which it needed: the headless test measures every row against the card at the floor, and the screenshot at that same size shows all eleven names with `Paperlike gradient theme` fully inside the card above the Close button.

**Three corrections to Chron3's click harness,** all of which cost a wasted run before being noticed. The window size has to be pinned in `config.toml` before launch, or the app opens at its 1280×800 default and every coordinate computed for another size misses — silently, because a click on nothing looks the same as a click that worked. `cargo test` does not necessarily relink `target/debug/parachron`, so the script builds first; two runs produced byte-identical screenshots from a stale binary before that was understood. And every screenshot is md5'd against the previous one, so a tap that changed nothing says so instead of being read as a theme that happens to look similar — which is how the off-by-one row alignment was caught rather than mislabelled.

**Not verified end to end: criterion 3.** `xdotool windowclose` does not make the app exit under `Xvfb` — the window advertises `WM_DELETE_WINDOW` in `WM_PROTOCOLS` and the event does not arrive, and there is no window manager to route it. `persist` runs after `run_event_loop` returns, so it never ran and `config.toml` was never rewritten. What *is* verified is every link of the chain: the headless test asserts a click updates `Themes::current`; `main.rs`'s test asserts `persist` writes the session's theme over a file already holding a different one; `config.rs`'s tests assert the value reloads; and `theme.rs` asserts the id round-trips and that an unknown one falls back. The single unverified step is the three lines in `main` that read `themes.borrow().current()` into `Session`. That is an honest gap rather than a claim, and it is the same shape as Chron1's: the 1000×700 floor is enforced by a window manager the harness does not have either.

**Still not themed, and known.** `app.slint` uses `std-widgets`' `ListView` for the product list, and its scrollbar is drawn from the Slint style rather than the palette. It appears only when the list overflows column 1, which is at about seventeen products, so it is invisible in every screenshot above. Replacing it is not the same job as replacing a slider — the list virtualizes and should keep doing so — and it is recorded here rather than left to be rediscovered.

## Done when

All acceptance criteria pass on the laptop. Then: record in CORE §5 that the palettes are pinned and where, close CORE §10's theme-palette item, note the Paperlike interpretation and the `backdrop` role in CORE §5, mark this file's status `done`, and move on to Chron6.
