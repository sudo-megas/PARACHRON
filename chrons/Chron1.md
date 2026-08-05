# Chron1 — Scaffold

**Milestone:** 1 of ~9 (CORE §9)
**Status:** done
**Builds against:** CORE §1 (identity), §2 (stack), §3 (data model), §4 (layout skeleton, principles), §8 (conventions & development rules)

## Goal

A running Parachron: `cargo run` opens a resizable three-column window (25/50/25, min 1000×700) that lists real products read from disk. No PDF rendering, no editing, no themes yet — this milestone is the skeleton every later Chron hangs flesh on: the repo structure, the data layer, and the string-table plumbing.

## Scope

**In:** repo skeleton · Cargo + Slint build working · data layer (scan, parse, validate, broken-file flagging) · config load/save scaffolding · three-column window with product list · selection state · string table (EN filled, TR keyed but allowed to lag until Chron6) · AGPL license file.

**Out (explicitly):** PDF viewing (Chron2) · adding/editing products (Chron3) · sorting toggles and details data (Chron4) · themes beyond one hardcoded default palette (Chron5) · language switch UI (Chron6) · export (Chron7) · About (Chron8) · packaging (Chron9).

## Repo skeleton to create

```
/home/megas/PARACHRON/
├── CORE.md
├── usereadme.md
├── LICENSE                  # AGPL-3.0-only full text
├── .gitignore               # target/, *.pkg.tar.zst, *.deb, *.exe
├── Cargo.toml               # name = "parachron", license = "AGPL-3.0-only"
├── build.rs                 # slint-build compile step
├── chrons/
│   └── Chron1.md            # this file
├── build/
│   └── icons/               # already populated (CORE §1)
├── ui/
│   ├── app.slint            # window, three columns, product list
│   └── strings.slint        # string-table global (see Tasks)
└── src/
    ├── main.rs              # wire-up only: load data, feed UI, run
    ├── data.rs              # Product struct, scan(), parse, validation
    ├── config.rs            # config.toml load/save (theme, lang, sort, window)
    └── strings.rs           # EN/TR tables, lookup by key + lang
```

## Tasks

- [x] `cargo init`, pin Slint, set `license = "AGPL-3.0-only"`, commit as `sudo-megas` (CORE §8 rules apply from commit one — no AI trailers, ever)
- [x] `build.rs` + minimal `app.slint` compiling and opening an empty window titled `PARACHRON`
- [x] Window: resizable, `min-width: 1000px`, `min-height: 700px`; three fixed-ratio columns 25/50/25 with visible placeholder panels
- [x] Title bar row per CORE §4 wireframe: app icon, `Document ▾` (non-functional stub menu), `Add Document` (disabled stub), centered wordmark; native window controls are fine for now
- [x] `data.rs`: `Product` struct mirroring CORE §3 schema exactly; `DataError` for broken entries
- [x] `scan()`: read `~/.local/share/parachron/products/` via the `directories` crate (XDG); create the tree on first run if absent; each subfolder → parse `product.toml`; malformed/missing TOML → product still appears, flagged broken with a readable reason (CORE §3 rule: never crash)
- [x] Also verify each file listed in `pdfs = [...]` exists; missing file → warning flag on the product (groundwork for Chron2 tabs)
- [x] `config.rs`: load `config.toml` or create with defaults (`lang = "en"` per CORE §4 — never read system locale; `theme = "default-dark"`, `sort = "added"`); save on exit (window size persistence optional, nice-to-have)
- [x] `strings.rs` + `strings.slint`: every user-visible string goes through key lookup — zero hardcoded UI strings from day one (CORE §4 principle); EN complete, TR keys present
- [x] Column 1: product list bound to scan results, insertion order (`added`), broken entries visually marked (e.g. ⚠ prefix); clicking selects
- [x] Selection: column 2 placeholder shows the selected product's `name` (proves the state pipeline end-to-end); column 3 placeholder static
- [x] Column 1 footer: About strip present but inert (opens nothing — Chron8)

## Acceptance criteria

1. Fresh machine, `cargo run`: window opens, data dir auto-created, list shows an empty state message (from the string table).
2. With three product folders on disk — two valid, one with broken TOML — the list shows all three; the broken one is marked and the app neither crashes nor hides it.
3. Window cannot shrink below 1000×700; columns keep 25/50/25 at any size.
4. Clicking a product updates the center placeholder with its name.
5. `grep -rn` for user-visible literals in `.slint`/`.rs` finds none outside `strings.rs` tables.
6. `git log` shows only `sudo-megas` as author and no AI attribution anywhere.

## Technical notes

Crates: `slint` (+ `slint-build`), `serde` + `toml`, `directories` (XDG paths), `time` (chosen — lighter than `chrono`; recorded in CORE §2). MuPDF is deliberately **not** a dependency yet — keep Chron1 compiling fast and clean; it enters in Chron2.

Broken-product pattern: `enum Entry { Ok(Product), Broken { folder: String, reason: DataError } }` — the list renders both variants. This same enum carries through every later Chron. `reason` is a typed `DataError` rather than a `String` because a pre-rendered message would be a user-visible English literal built inside `data.rs`, which acceptance criterion 5 forbids; `main.rs` renders the variant through the string table instead.

Manual test data: create `~/.local/share/parachron/products/test-monitor/` and `test-drive/` with valid `product.toml` files (CORE §3 schema), plus `test-broken/` containing a `product.toml` with a deliberate syntax error.

## How the criteria were verified

`cargo test` covers criteria 2, 3 (ratios) and 4 headlessly via the Slint testing backend (`src/ui_tests.rs`): column widths are asserted to land exactly on 25/50/25 at 1000×700, 1400×900 and 1920×1080, and simulated clicks assert the selection reaches the centre column. `build.rs` emits Slint debug info in debug profiles only, which the element-id lookups require.

The 1000×700 floor is the one part the headless backend cannot judge — it has no window manager and adopts whatever size it is given. Verified against a real window instead: with `window_width = 400`, `window_height = 300` in `config.toml`, the app still opened at ~1280×700 logical.

Criteria 1, 5 and 6 were checked directly: a fresh run with the data dir deleted, a literal sweep of `src/` and `ui/`, and `git log`.

## Done when

All acceptance criteria pass on the laptop. Then: update CORE §2 with the chosen date crate, mark this file's status `done`, and ask user permission to start writing Chron2.
