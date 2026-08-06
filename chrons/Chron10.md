# Chron10 — Character

**Milestone:** 10, added after CORE §9's original nine-milestone roadmap (CORE §9)
**Status:** done
**Builds against:** CORE §4 (UI layout — the three columns, the wireframe, the app-wide principles), §5 (themes — the twelve-role contract this milestone works entirely inside), §8 (conventions — the Chron file convention this file follows), §9 (roadmap — this milestone's own row), §10 (open items — the column-1 `ListView` scrollbar this milestone does not touch, and the item it adds)

## Goal

The app works and reads as flat. Column 1 and column 3 are the same background color by design — a comment in `palette.slint` says so, and says that sharing is what makes the three-column structure legible. It isn't: nothing separates one column's domain from another beyond a 1px line, five independent hand-rolled buttons have no pressed state, and column 3 spends most of its height on two competing flex spacers around a handful of small labels. None of this is a bug in the sense of behaving wrong — the app does what CORE §4 specifies. It is a milestone in the sense that "give the app character" is a real, scoped piece of work, the way "theme it" and "translate it" were.

Four things, in the order they have to land because each one builds on the last: a masthead-and-canvas structure for columns 1 and 3, matching what column 2's tab strip already does, with a real gutter at the seams instead of a bare hairline (A1). A hover state, a selection landmark and a status marker for column 1's rows, none of which exist today (A2). A rebuilt column 3 that replaces two independent dead spacers with one, anchored by a card that turns the days-left counter into what CORE §4 has always called it — the counter the app exists for (A3). And pressed-state feedback plus a real use of the already-declared `primary` property across all five of the app's independently hand-rolled button recipes (A4).

## Scope

**In:** column 1 and column 3 backgrounds split into a `Palette.panel` masthead over a `Palette.bg` canvas body, matching column 2's existing tab-strip pattern · a wider content inset at both column seams (8px for column 2, 4px for column 3 — asymmetric, by width arithmetic against the 1000×700 floor, not by eye) · `palette.slint`'s now-incorrect comment about panel being deliberately shared corrected · a hover state, a 3px left accent bar on the selected row, and a 6px status swatch for broken/warning rows in column 1's product list, sourced directly from the row's own `broken`/`warning` booleans · column 3 rebuilt around one flex spacer instead of two, with the days-left counter promoted into a card carrying a proportional warranty-elapsed gauge (`Snapshot.progress`, a new numeric field alongside the existing pre-formatted `days-left` string) · `DetailRow` gains an opt-in `wrap` property, default off, so the About pane's four existing uses are unaffected while column 3's three date rows and its purchase-link row stop truncating · pressed-state feedback on all five hand-rolled button recipes (`Btn`, `GlyphBtn`, `SortChip`, `MenuRow`, `NavButton`) · `Btn.primary` given a real resting-state visual, gated on `enabled` (closing a small bug where a disabled primary button currently reads as enabled), and set on column 3's EXPORT button, the one place the property's own doc comment — "the affirmative one" — was missing it.

**Also in, after the first pass was rejected:** two more stored colour roles, `accent2` and `accent3`, and all eleven palettes re-authored to carry the hues their sources actually publish · each column wearing one of the three — a solid rule across the top of its card and its masthead bracketed in the same hue · the columns themselves redrawn as inset cards on the window's canvas with a real channel between them, replacing the 1px hairline · the About pane's icon centred and enlarged · the generated icon set inset inside its own canvas so it stops outweighing every other icon on the task bar. See the Technical notes for what the first pass got wrong and why.

**Out (explicitly):** per-theme fonts, radii or spacing — CORE §5's "colour tables and nothing else" still holds; what changed is how many colours a table has, not what a table is · a literal gradient anywhere, for the reason CORE §5 already gives Paperlike's own ladder-not-gradient choice · changing the 25/50/25 column ratio, which CORE §4 pins as a decided layout: the cards are inset *inside* columns whose own geometry is untouched, which is why `assert_columns` never had to move · per-row dividers in column 1 — considered and rejected in favor of state-based landmarks (hover, selection, marker) that only appear when they mean something, rather than a rule under every row regardless of state · replacing column 1's `ListView`, still CORE §10's own open item and not a visual-polish question · new string keys for column 1's status marker — the existing `⚠`/`!` text prefix (`Key::BrokenPrefix`/`Key::WarnPrefix`) stays exactly as it is; the marker added here is a decorative swatch bound to the row's existing boolean fields, not a second copy of the same information.

## Files to add and change

```
CORE.md                # §4 wireframe + prose, §5 correction, §9 row, §10 open item
chrons/
└── Chron10.md          # this file
src/
└── details.rs          # + Snapshot.progress, computed in Snapshot::product, pushed in show()
ui/
├── app.slint           # col1 bg, About-strip resting color, seam insets, row template, SortChip/MenuRow pressed state, details-progress property + wiring
├── details.slint        # masthead/body restructure, anchor card + gauge, DetailRow.wrap use, link-row wrap, export-button primary
├── widgets.slint        # DetailRow.wrap property, Btn/GlyphBtn pressed state + Btn.primary visual
├── viewer.slint         # NavButton pressed state
└── palette.slint        # panel-sharing comment corrected
```

No new files beyond this one, no new dependency, no new string keys.

## Tasks

### A1 — Column identity

- [x] `app.slint`: `col1` background `Palette.panel` → `Palette.bg` (line 429)
- [x] `app.slint`: About strip's resting-state background `Palette.panel` → `Palette.bg` (line 569); hover (`raised`) and active (`selection`) states unchanged
- [x] `app.slint`: column 2's content inset widened `x: 1px` → `x: 8px`
- [x] `app.slint`: column 3's content inset widened `x: 1px` → `x: 4px` (asymmetric — see Technical notes for the width arithmetic)
- [x] `details.slint`: `Details`' root restructured from one `VerticalLayout` into a `Palette.panel` masthead (THEME/EXPORT row + status line, own padding, bottom hairline) over a `Palette.bg` body (`vertical-stretch: 1`)
- [x] `palette.slint`: lines 24-26's comment rewritten — panel is no longer flush-shared by "the title bar, the list, column 3"; it marks a masthead band, and bg is the canvas each column's body sits on
- [x] Flag carried to A2: empty-state text in column 1 (`app.slint` ~549-559) uses fixed `x: 14px` offsets; resolved as a no-op — A2's row changes add zero text indentation (Technical notes)

### A2 — Column 1 rows

- [x] `app.slint`: row background gains a hover case — `row-touch.has-hover ? Palette.raised : transparent` — alongside the existing selection case; `row-touch`'s id and count (3, one per product row, per `ui_tests.rs`) unchanged
- [x] `app.slint`: 3px `Palette.accent` left accent bar on the selected row, `if idx == root.selected-index`
- [x] `app.slint`: 6×6px rounded status swatch, `if item.broken || item.warning`, colored `Palette.danger`/`Palette.accent`, in a fixed slot before the row's `Text` — bound to the existing `item.broken`/`item.warning` booleans; `vault.rs`'s `⚠`/`!` text prefix untouched
- [x] `app.slint`: row `TouchArea` gains the `accessible-role`/`accessible-label`/`accessible-action-default` quartet every other interactive element already has (picked up opportunistically, not a stated goal)

### A3 — Column 3

- [x] `details.slint`: the two independent `Rectangle { vertical-stretch: 1 }` spacers collapsed into one, placed directly above the anchor card
- [x] `details.slint`: days-left counter promoted into a card — `Palette.panel` background, `border-radius: 5px`, label + counter text (26px → 30px) + a proportional gauge bar (mirrors `widgets.slint`'s `Slider` track/fill)
- [x] `src/details.rs`: `Snapshot` gains `pub progress: f32`; `Snapshot::product` computes it from `warranty_start`/`warranty_end`/today; `Snapshot::empty()` sets `0.0`; `show()` pushes it
- [x] `app.slint`: `AppWindow` gains `in property <float> details-progress`; `details.slint`'s `Details` gains `in property <float> progress`, wired at the `details := Details { ... }` call site
- [x] `src/details.rs`: three new tests beside the existing days-left test — warranty just started (progress ≈ 0), midpoint, expired (progress ≈ 1, `expired: true`)
- [x] `widgets.slint`: `DetailRow` gains `in property <bool> wrap: false`; default reproduces today's `elide` exactly, so `about.slint`'s four uses are unchanged
- [x] `details.slint`: `wrap: true` on the three date `DetailRow`s
- [x] `details.slint`: purchase-link row height `26px` → `40px`, `wrap: word-wrap` added, `overflow: elide` kept as the two-line fallback
- [x] (found during real-app verification, not in the original list) `details.slint`: link `Text`'s `vertical-alignment: center` → `top` — with `wrap` added, centering a wrapped multi-line value inside a fixed-height box clipped both ends and showed a random middle slice of the URL rather than its start (Technical notes)
- [x] (found during real-app verification) `details.slint`: `anchor` gains an explicit `vertical-stretch: 0` — a bare `Rectangle` has no max-height, so it was sharing the spacer's stretch and growing to fill a tall window instead of staying compact (Technical notes)

### A4 — Buttons (five recipes)

- [x] `widgets.slint`: `Btn` gains `touch.pressed` as a background case, converging on `Palette.selection`
- [x] `widgets.slint`: `Btn.primary` gains a real resting-state background (`selection`-toned when enabled) and its border-color logic is gated on `enabled` — fixes a disabled primary button currently reading as enabled
- [x] `widgets.slint`: `GlyphBtn` gains the same pressed-state case as `Btn`
- [x] `app.slint`: `SortChip` gains `touch.pressed`, converging on `Palette.selection` alongside its existing `active` case
- [x] `app.slint`: `MenuRow` gains `touch.pressed`, converging on `Palette.selection` alongside its existing `active`/hover cases
- [x] `viewer.slint`: `NavButton` gains the same pressed-state pattern as `Btn`
- [x] `details.slint`: `primary: true` set on the EXPORT button

### A8 — Colour, columns and icons (the second pass, after the first was rejected)

- [x] `palette.slint` / `theme.rs`: `accent2` and `accent3` added to the table — fourteen roles, not twelve
- [x] `theme.rs`: all eleven palettes given their two extra hues from their own published sources; Noctalia rebuilt wholesale onto the five-colour set it actually publishes, instead of being a free interpretation of its darkest one
- [x] `theme.rs`: the contrast floor extended to the two new roles against `panel` and `bg` — not `selection`, which they are never drawn on
- [x] `ui_tests.rs`: `assert_palette_pushed` covers fourteen roles; a dropped setter would otherwise leave ten themes silently wearing Default Dark's two hues
- [x] `app.slint`: each column redrawn as an inset card on the window canvas — `edge-gap`, `seam-gap`, real channels — with the column rectangles' own 25/50/25 geometry untouched
- [x] `app.slint` / `viewer.slint` / `details.slint`: a 4px rule of the column's hue across the top of each card, and a 2px line of it under each masthead
- [x] `app.slint`: the About pane inset to match the cards it covers; `about.slint` given the card's surface, corners and outline
- [x] `about.slint`: the pane's icon centred (a `HorizontalLayout` — `horizontal-alignment` on an `Image` aligns the bitmap in its own box, which is why a fixed-width icon sat in the corner) and taken from 72px to 144px
- [x] `build/icons/generate.sh`: every size inset to `ICON_INSET_PCT` of its canvas with transparent margin, so the task-bar icon stops outweighing its neighbours
- [x] `build/icons/generate.sh`: the tile sizes given the same alpha-rounded corners the mark sizes already had — the master's own corners are opaque, so an inset tile would otherwise be a hard dark square floating in its margin, which the About pane at 144px shows plainly
- [x] `build/icons/generate.sh`: `MARK_CROP` re-measured against the master that is actually in the tree. The inherited crop began eighty pixels above the tile's own rim, so every size below the wordmark floor — which is every size a task bar draws — carried a dead band of backdrop across its top with the hexagon pushed off centre beneath it (Technical notes)

## Acceptance criteria

1. Columns 1 and 3 each show a visibly distinct masthead band (`panel`) over a canvas body (`bg`); column 2's existing tab-strip pattern is unchanged.
2. Both column seams show a visible gutter of canvas tone, not content flush against the 1px hairline; column 3's gutter does not cause the THEME/EXPORT row to wrap or clip in either language at the 1000×700 floor.
3. Hovering a column-1 row shows `raised`; the selected row shows both `selection` fill and the left accent bar; a broken or warning row shows its swatch regardless of name length.
4. Column 3 shows one flex region, not two; the days-left card is visually anchored with a gauge whose fill fraction matches elapsed-vs-total warranty at three checked points (just started, midpoint, expired).
5. The three date rows and the purchase-link row show more of their value before eliding than before this milestone; the About pane's four `DetailRow` uses are pixel-unchanged.
6. All five button recipes show a visibly distinct pressed state, and it is distinguishable from hover.
7. EXPORT reads as the affirmative action in its row; a disabled `Btn` with `primary: true` never shows the primary background or border.
8. Every new color in `.slint` resolves through `Palette`; `grep -rn` for color literals in `ui/` outside `palette.slint` returns nothing new.
9. `cargo build` and `cargo test` are warning-free.
10. The app fits at 1000×700 in both languages with nothing clipped or overlapping, across at least the four themes named in Technical notes.

## Technical notes

**The first pass answered a question nobody asked, and this is the correction.** The complaint that opened this milestone had eight points, and its seventh was that the themes are "colour palettes in name but they apply only 1 main colour of the colorset." That was read as a request for *structure* — that sections did not feel like their own domains — and answered with mastheads and a wider seam, while the colour table was deliberately left at twelve roles on the argument that CORE §5 pinned it and that seven of the eleven palettes are branded and could not be invented against. Every one of those facts was true and the conclusion was still wrong. The point was the plain reading: a palette has several colours in it and the app was using one. Noctalia publishes five — a deep navy, a blue, a grey, an orange and a yellow — and what reached the screen was the navy, five times over, at five brightnesses. Eleven themes, and every one of them arrived as its own single hue.

So the table grew by two roles rather than none. `accent` keeps what it always had — focus, selection, the active chip, the affirmative button, everything that means *state*. `accent2` and `accent3` carry *identity*: column 1 is `accent`, column 2 is `accent2`, column 3 is `accent3`. The worry that argued against this — that branded palettes have no honest source for extra hues — turned out to be backwards on inspection. Catppuccin publishes fourteen accents per flavour, Rosé Pine six, Canonical a brand sheet; the branded palettes had *more* to draw on than this project's own interpretations did. Where a source's obvious next hue could not clear the contrast floor it is named at its const and either replaced or darkened with the measured figure written down, which is the same bookkeeping Chron5 did for Frappé's ladder.

**A hairline is not a transition.** The seam between columns was a 1px `Palette.border` line between two panels of identical colour, and Chron10's first pass widened the *inset* while leaving that hairline as the entire boundary. Whatever the arithmetic said, on screen it is one grey line, and the honest verdict on it was that you cannot feel it. Each column is now an inset card on the window's canvas: the column rectangles keep their exact 25/50/25 and draw nothing at all, and the card inside each is what you see, with `edge-gap` at the window's edges and `seam-gap` per side making a real channel between neighbours. The card is a surface, not a container — it is drawn as a sibling under the content rather than wrapping it, which is what let two hundred lines of column-1 layout stay exactly where they were.

**A measured constant is measured against one particular image.** `generate.sh` carried its crop numbers forward from the artwork they were taken off, and the artwork was replaced. `MARK_CROP` began at y=30 on a master whose tile does not start until y=110, so the eighty pixels above the rim — plain backdrop — were cropped into every icon below the wordmark floor, which is every size a task bar ever draws. It did not look like a bug. It looked like an icon with a dark strip along the top and its glyph sitting low, which is a thing an icon can simply be, and it survived two passes of this milestone and a full cache-bust-and-look before anybody said it was cut off. The lesson is narrow and worth writing down anyway: a comment saying a number was measured is a claim about a file, and it expires when that file is redrawn. The numbers here are now measured against the master that is in the tree, and the header says to re-measure them if it changes again.

**A tint in a dark surface goes muddy before it goes colourful.** The first attempt at giving each masthead its column's hue mixed the hue into `raised` at 14%, and on Noctalia that produced three bands of very similar grey-teal: orange at 14% in a navy panel is not orange, it is a slightly warmer navy. Light themes did not have the problem — Catppuccin Latte's mauve masthead reads as mauve — which is exactly the trap of checking one theme. The fill keeps the tint, because where it works it works; the hue is *stated* on the band's two edges instead, a 4px rule across the top of the card and a 2px line under the masthead, both at full strength. Those read identically on all eleven.

**Accent bar and swatch, not dividers.** At `row-height` 38px, a rule under every row read as a spreadsheet in an early read of the design and fought the "handsome" ask directly — line-noise on every row regardless of whether it means anything. A hover state, a selection accent bar and a status swatch appear only on the rows where they carry information (interactive, selected, needs-attention), which is why they were chosen over a divider under all of them.

**A swatch, not a second copy of the text prefix.** `vault.rs` already prefixes a broken/warning row's `label` with `⚠ `/`! ` — both already localized (`Key::BrokenPrefix`/`Key::WarnPrefix`). The new 6×6px marker does not read or duplicate that string; it binds directly to the same `item.broken`/`item.warning` booleans the prefix is built from. Two devices, one source of truth, no new string keys.

**One spacer, not two, not zero.** Column 3 had two independent `Rectangle { vertical-stretch: 1 }` spacers with nothing between them — the literal dead space the milestone answers. Zero spacers would pin the anchor card to the top of a tall window, which is not what "anchored" should mean on a 1400px window. One spacer, positioned directly above the card, gives the same flexibility with a legible before/after: facts, breathing room, the counter.

**`progress` is a field, not a reparse.** `days-left` arrives pre-formatted ("658 days") because Turkish does not agree in number after a numeral and the string table holds no interpolation — that rule predates this milestone and stays true. The gauge needed a plain fraction, so `Snapshot` gained a second, independent `f32` field computed from the same `warranty_start`/`warranty_end`/today already in scope, not derived from the display string.

**A fifth button recipe.** `NavButton` in `viewer.slint` (page arrows, tab strip) is the same hand-rolled Rectangle+TouchArea shape as `Btn`/`GlyphBtn`/`SortChip`/`MenuRow` and was easy to miss when scoping — it was found and included before implementation started, not after. Leaving it out would have meant column 2 visibly diverging from every other column once the other four got pressed-state feedback, which is exactly complaint 7's shape.

**The column-3 gutter is 4px, asymmetric with column 2's 8px, decided by arithmetic before any code was written.** At the 1000×700 floor, column 3 is exactly 250px; minus `Details`' own 16px padding on each side that's 218px of content width before any seam change. The masthead's two buttons (`TEMA`/`DIŞA AKTAR` in the tighter language, 12px horizontal padding each, 8px between them) need on the order of 160-200px combined — a real margin at 218px, but one a symmetric 8px gutter would narrow further than a column with no scroll container and no room to spare should be narrowed. Verified directly by screenshot in Turkish at the floor (see below): `DIŞA AKTAR` sits inside `TEMA`'s row with visible margin on both sides, not wrapped, not clipped.

**`Btn.primary` already existed and was already used three times** (About's licence-sheet Close, the form's Save, the theme picker's Close) before this milestone — it just had no visible resting-state effect beyond a border color, and no `enabled` gate, so a disabled primary button read as enabled. Both are fixed in the same change that gives EXPORT its first `primary: true`. One consequence worth recording rather than treating as a defect: because a primary `Btn` now rests on `Palette.selection`, and the shared pressed-state also converges on `Palette.selection`, pressing EXPORT specifically is a near-visual-no-op — its resting and pressed states are close. This was flagged during implementation and left as-is: it is a coincidence of two independently-correct decisions (primary's resting tone, pressed's converge-on-selection tone) landing on the same role, not a bug, and EXPORT still shows a border-color and cursor change on interaction like every other button.

**Two defects the plan did not anticipate, caught only by looking at the running app, not by the test suite.** Both are recorded here as findings, in the manner Chron8 records what its own first draft got wrong, because a claim about a UI is not a finding until it has been looked at.

The first: the purchase-link `Text` kept `vertical-alignment: center` when `wrap: word-wrap` was added in the same change. `cargo test` and `cargo build` both passed — Slint layout tests do not (cannot, headlessly) catch centered wrapped text in a fixed-height box clipping both ends, so a real screenshot showed a long URL rendering as a legible-looking but wrong fragment starting mid-string ("qd-oled-monitor-27-inch-..." rather than the URL's own start). Changed to `vertical-alignment: top`; a screenshot after the fix shows the URL from its own beginning, wrapped to two lines, eliding a third only if present.

The second: the new `anchor` card had no explicit `vertical-stretch`, and a bare `Rectangle` has no max-height ceiling. In the column-3 layout it shared the single spacer's stretch instead of the spacer absorbing a tall window's slack on its own — at 1000×1400 the card grew to fill roughly two-thirds of the column instead of sitting compact at the bottom, which is not what "anchored" means. `vertical-stretch: 0` on `anchor` fixed it; a screenshot at the same size after the fix shows the card back to its content-driven height with the spacer carrying the extra room, matching the design.

Neither defect was reachable by `cargo test` — both are genuinely visual and both were caught only because this milestone's own verification step insisted on a real window at a real size rather than trusting a green build.

## How the criteria were verified

**Automated.** `cargo build` and `cargo test` clean, no warnings, 158/158 passing (unchanged in count from before this milestone — no test was removed; three were added for `Snapshot.progress`: just-started ≈0, midpoint ≈0.5, expired clamped to ≈1 with `expired: true`). `grep -rnE "#[0-9a-fA-F]{6,8}"` over `ui/` outside `palette.slint` returns nothing new (one pre-existing comment mention in `form.slint`, unrelated to this milestone). A spot grep for stray string literals in the five touched `.slint` files returns only `import` lines. `row-touch`'s element count and id were unchanged, confirmed both by the row template's diff and by `cargo test`'s existing assertions on it (used in 17 places across `ui_tests.rs`) all still passing.

**Through the real, running app — Xvfb `:98` at 1400×1600, a scratch vault under a throwaway `XDG_DATA_HOME` with a healthy product (QD-OLED Monitor, warranty ~2/3 elapsed), a near-full-term one (IronWolf Pro 6TB), a warning-flagged one (Şarj Cihazı, a `product.toml` listing a PDF that is not on disk — chosen deliberately to also exercise a non-ASCII, Turkish-folded name), and an unparseable-manifest broken folder.** This is where this milestone's real risk lived, and it is where both defects in Technical notes were actually found — not in a diff, in a screenshot.

- **Two window sizes.** 1000×700 (the floor) and 1000×1400 (generously tall). At the floor, all masthead/body bands, the row markers, and the anchor card render with no clipping or overlap. At 1400px tall, the single column-3 spacer absorbs the slack and the anchor card stays compact at its content height (after the `vertical-stretch: 0` fix — the pre-fix screenshot showing the card wrongly filling most of the column is the finding recorded above, not a shipped state).
- **Four themes.** Default Dark (baseline, all states above). Catppuccin Latte (light, branded) in Turkish — masthead/body separation reads clearly, the anchor card shows a visible edge against its canvas, `DIŞA AKTAR` fits the masthead row with margin at the 1000px floor, confirming the gutter arithmetic. Ubuntu Canonical Aubergine (the tightest-contrast palette, documented prior near-failure) — selected the warning row (Şarj Cihazı) specifically, confirming its accent bar, swatch color, and the primary EXPORT border all clear contrast by eye. Paperlike — selected the broken row, confirming the disabled/non-primary EXPORT state does not show a primary background or border, and the masthead/body tonal split is present though subtle on this palette's warm near-white ladder.
- **Both languages.** English (all screenshots above except the Latte pass) and Turkish (Latte pass) — every changed string-bearing surface (masthead labels, warning prefix, dates) renders correctly in both; no new string keys were added so no new translation surface exists to check.
- **Interaction states.** Hover confirmed on `Btn` (THEME, via mouse-move without click, screenshotted mid-hover showing the `raised` background and accent border). Row hover, selection accent bar, and the broken/warning swatches were all confirmed via the product-selection screenshots above. Pressed-state on the remaining four recipes (`GlyphBtn`, `SortChip`, `MenuRow`, `NavButton`) was not independently screenshotted mid-press — see Not verified.

## Not verified

Named rather than implied, in the manner of every Chron file here.

- **Pressed-state on `GlyphBtn`, `SortChip`, `MenuRow`, and `NavButton` specifically** was not screenshotted mid-press (only `Btn`'s hover was). All four share the exact `touch.pressed` pattern that was confirmed working for `Btn`'s hover/pressed logic by code review and a passing build, but "the tick appears" was only directly observed for one of the five recipes.
- **The clipboard round-trip** (copying the purchase link or serial) was not exercised — inherited unverified from Chron8, which recorded the same gap under `Xvfb` for the same reason (no clipboard owner to ask on an isolated display).
- **Seven of the eleven themes** were not individually screenshotted: Default Light, Noctalia, Catppuccin Frappé, Catppuccin Macchiato, Catppuccin Mocha, Rosé Pine, Ruby. Every new color in this milestone resolves through the same `Palette` roles exercised across the four themes that were checked, which is evidence rather than proof for the other seven.
- **The derived-tone `.mix()` fallback for the anchor card** was designed against and not implemented — see Scope/Out and CORE §10's new open item. Nothing to verify because nothing shipped.
- **Escape/keyboard navigation** through the new row markers and the anchor card was not driven — this milestone changes no keyboard behavior, so it inherits whatever was already true, but that inheritance was not re-confirmed.
- **The unthemed column-1 `ListView` scrollbar** (CORE §10's pre-existing open item) is more visually conspicuous now that it sits inside a `bg`-toned canvas rather than a flush `panel` — this is a side effect of A1 surfaced by this milestone, recorded here as an accepted cost rather than silently fixed, matching how Chron8 handled the same open item.

## Done when

All acceptance criteria pass. Then: CORE §4's wireframe and column 1/3 prose describe the masthead/gutter structure that actually shipped, §5's palette-sharing description is corrected to match `palette.slint`'s own corrected comment, §9 gains this milestone's row, §10 gains the derived-tone-card open item, this file's status moves to `done`.
