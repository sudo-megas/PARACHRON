# PARACHRON — CORE

Single source of truth for the Parachron project. Every Chron milestone file builds against what is written here. If reality and CORE.md disagree, update CORE.md first.

**Parachron** (*para* — against/alongside, *chronos* — time): a desktop vault for purchases. It keeps each product's invoices, warranty documents, serial number and purchase details together, and counts down the warranty for you.

---

## 1. Identity

| Key | Value |
|---|---|
| App name | Parachron (wordmark: PARACHRON) |
| Binary name | `parachron` |
| App ID | `org.parachron.Parachron` |
| License | AGPL-3.0 (required by MuPDF linkage; source stays public) |
| Project root | `/home/megas/PARACHRON/` |
| Icons | `/home/megas/PARACHRON/build/icons/` (`parachron-{16,24,32,48,64,96,128,256,512,1024}.png`, `parachron.ico`) |
| Maintainer | sudo-megas (sole commit author — see §8) |
| Repo | `https://github.com/sudo-megas/PARACHRON` (CI: GitHub Actions) |

## 2. Stack (decided, locked)

| Concern | Choice | Notes |
|---|---|---|
| Language | Rust (stable) | edition 2021+ |
| GUI | Slint | UI in `.slint` files; logic in Rust |
| PDF render | MuPDF (`mupdf` crate) | pages rasterized to images for the preview pane; built with `default-features = false` plus `base14-fonts`, `system-fonts`, `brotli`, `img` — no OCR, no ebook formats, and **no PDF JavaScript execution** |
| PDF export | MuPDF (reused) | generates summary page + merges product PDFs; text drawn through `mupdf::shape::Shape` with **composite** font registration (see §6 — a simple Latin encoding silently drops Turkish letters) |
| Data format | TOML (`toml` + `serde`) | one file per product; `preserve_order` so hand-added keys keep the order they were written in |
| Dates | `time` crate | Stored as native TOML dates (ISO `YYYY-MM-DD`); displayed as `DD-MM-YYYY`; days-left computed at runtime. Features `macros`, `formatting`, `parsing`, `local-offset` — and the offset **must** be read at the top of `main`, before any thread exists (Chron4) |
| File picker | `rfd`, `default-features = false` + `xdg-portal` | Native dialogs with no GTK development headers, which keeps §7's three targets cheap. `wayland` is deliberately **off**: its window identifier roundtrips a second event queue on Slint's own display from a foreign thread, which risks a deadlock in exchange for cosmetic dialog parenting. Always driven through `AsyncFileDialog` — the blocking call parks the caller in an untimeouted D-Bus read |
| Clipboard | `arboard`, `wayland-data-control` only | No image support; Parachron only ever copies text |

Rejected during planning: Python, TypeScript, Qt, Electron (vetoed on sketch); iced, egui, GTK4 (GUI runners-up); pdfium-render, Poppler, pure-Rust PDF (render runners-up); SQLite, JSON, RON (storage runners-up).

## 3. Data model

Data lives under the XDG data dir: `~/.local/share/parachron/`.

```
~/.local/share/parachron/
├── products/
│   ├── qd-oled-monitor/
│   │   ├── product.toml
│   │   ├── invoice.pdf
│   │   └── warranty.pdf
│   └── ironwolf-pro-6tb/
│       ├── product.toml
│       └── invoice.pdf
└── config.toml            # app state: chosen theme, language, sort mode, window size
```

One folder per product; the folder holds `product.toml` plus that product's PDFs. The app scans `products/` at startup and builds its list from what actually exists on disk. The data must outlive the app: everything human-readable, rsync-friendly, no hidden state.

### product.toml schema

```toml
name = "QD-OLED Monitor"          # display name (also default sort key)
serial = "ABC123XYZ"              # serial number, shown in the strip under the viewer
link = "https://store.example/p"  # purchase site / product page
purchase_date = 2026-03-14        # TOML date
warranty_start = 2026-03-14       # TOML date
warranty_end = 2029-03-14         # TOML date (entered directly, not computed)
pdfs = ["invoice.pdf", "warranty.pdf"]   # order = tab order in the viewer
added = 2026-08-05                # when the entry was created (insertion order)
```

Rules: dates are stored as **native TOML dates** — RFC 3339 `YYYY-MM-DD`, the only form TOML parses — and rendered in the UI as `DD-MM-YYYY` (e.g. `14-03-2026`). Storage format and display format are separate concerns; never write `DD-MM-YYYY` into a `.toml` file. `warranty_end` is entered by the user together with `warranty_start` (both come from the warranty card). Days left = `warranty_end - today`, clamped at 0, displayed as e.g. `658 days`. Missing or malformed TOML must never crash the app — the product appears in the list flagged as broken, with a readable error.

## 4. UI layout

Window: resizable, minimum **1000×700**. Three fixed columns — **25 / 50 / 25** — the classic list | content | inspector pattern. Chosen over the earlier two-column draft because invoices are portrait A4: a full-height center column shows roughly twice the readable page area from the same window.

```
┌───────────────────────────────────────────────────────────────────┐
│ [icon] Document ▾ | Add Document        PARACHRON          – □ ✕  │
├───────────────┬───────────────────────────────┬───────────────────┤
│ Column 1(25%) │ Column 2 (50%)                │ Column 3 (25%)    │
│ [A–Z] [Date]  │  [Invoice] [Garanti] [.pdf]   │  [THEME] [EXPORT] │
│ ┌───────────┐ │  ┌─────────────────────────┐  │                   │
│ │ Search… ✕ │ │  │                         │  │  Link:            │
│ └───────────┘ │  │                         │  │  store.example/p  │
│ Product list  │  │   PDF preview (MuPDF)   │  │                   │
│ (insertion    │  │                         │  │                   │
│  order,       │  │                         │  │                   │
│  sortable)    │  │                         │  │                   │
│               │  │   full height,          │  │  Purchase date    │
│               │  │   portrait-friendly     │  │  14-03-2026       │
│               │  │                         │  │                   │
│               │  │                         │  │  Warranty start   │
│               │  └─────────────────────────┘  │  14-03-2026       │
│               │  ‹ ›  2 / 12      Zoom ──●──   │                   │
│───────────────│  Serial number: ABC123XYZ  ⧉  │                   │
│ [ⓘ About]     │                               │  **658 days**     │
└───────────────┴───────────────────────────────┴───────────────────┘
```

Column 1 — product list. Default order: as added (`added` field). Two sort toggles: alphabetical (by `name`) and purchase date (oldest first, newest at bottom). Under the toggles and directly above the entries, a **search bar**: full column width, fixed height, matching product **name and serial number** as you type and narrowing the list to what matches. Bottom strip: a fixed **About** entry (JADEITE-style sidebar footer) that opens the About view.

The search bar was added to this layout in Chron8; the original wireframe had column 1 as a list and nothing else, and the sort toggles were never drawn into it either. Both are in the sketch above now. Three rules it follows, so that later work does not have to rediscover them. Matching is **folded** — accent- and case-insensitive in both directions, so `sarj` finds `Şarj Cihazı` and Turkish's dotless `ı` cannot make a product unfindable by its own name. A broken folder is matched on its **folder name**, because that is the only text it has and hiding the entry somebody is looking for is the one thing the list has never done (§3: never crash, never hide). And the query is **session state, not settings** — it is never written to `config.toml`, because a sort order that survives a restart reorders and a filter that survives one *hides*, and an app that opens showing three of your eleven products has lost them as far as you can tell.

Column 2 — document viewer. Workspace-style tabs switch between the selected product's PDFs (`pdfs` order); each tab is labelled with the file-name stem, so `invoice.pdf` reads `Invoice`. The preview takes the full remaining height and shows **one page at a time, fitted whole** inside the pane. Under it a control row carries `‹` / `›`, a `2 / 12` page counter, and a **zoom slider** — zoom is a multiplier of the fit scale (`1×`–`4×`), so `1×` always means the whole page is visible whatever the window size; above `1×` the page pans. Page and zoom reset whenever the tab or product changes. Below that, a fixed **44px** serial-number strip: click it to copy the serial to the clipboard with a brief "copied" confirmation, the same gesture the purchase link uses in column 3.

Column 3 — details + actions. Top: THEME and EXPORT buttons. Then purchase link (click = **copy to clipboard** with a brief "copied" confirmation — never opens a browser), purchase date, warranty start, warranty end, and the warranty-left counter in days — bold, largest text in the column, its visual anchor. A warranty that has run out reads as expired rather than as a negative number, in the error colour.

Warranty end was added to this list in Chron4; the original wireframe above omitted it. Hiding the date the countdown is counting toward leaves anyone checking the number with nothing to check it against.

Columns are fixed-ratio; the content structure is identical for every product.

### App-wide principles

**No external opens.** Like JADEITE, Parachron never opens external addresses or launches a browser. Every URL in the app (purchase links, About URLs) is plain text with copy-to-clipboard where useful.

**Localization.** All UI strings exist in **English and Turkish**, switchable at runtime, persisted as `lang = "en" | "tr"` in `config.toml`. Default is always `en` — the app never checks the system locale; switching to Turkish is a deliberate user action via the `Document ▾` menu. No hardcoded strings in `.slint` or Rust — everything goes through the string table.

Two things about "switchable at runtime", settled in Chron6. Refilling the `Strings` global only relabels what is *bound* to it; roughly half the text on screen is composed in Rust and pushed into ordinary properties — list rows, a broken folder's reason, the warranty countdown, a document that will not open, the theme picker's rows — and none of it knows the language changed. All of it derives from the current selection, so one re-plan through the vault recomputes the lot; there is one route rather than several that could disagree. And what is *on disk* is not UI copy and never translates: folder names, file names, the tab labels derived from file stems, and the diagnostic tails of OS and TOML-parser messages.

Language names are written in their own language in both tables (`English`, `Türkçe`), so a reader who has landed in a language they cannot read can still find their own. Turkish maps `i`→`İ` and `ı`→`I`, so any label that shouts is stored shouting and never passed through `to_uppercase` — `EXPORT` is `DIŞA AKTAR`, with a dotless capital.

### About view

Anaphored from JADEITE's About. Selecting About in the column-1 footer swaps the content area (columns 2+3) for a single centered pane:

- App icon, large (from `build/icons/`)
- `P A R A C H R O N` wordmark, letter-spaced
- Subtitle line (one-phrase app description — wording open, see §10)
- Maker — `sudo-megas`
- Version — from `Cargo.toml` at build time
- Release date
- Source code — `https://github.com/sudo-megas/PARACHRON` (plain text)
- Docs — `https://github.com/sudo-megas/PARACHRON#readme` (plain text)
- Note under the URLs: these addresses are not links; Parachron never opens external addresses — copy them into your browser (see App-wide principles)
- License — `AGPL-3.0-only`, with a "read the full license" entry that shows the bundled license text
- Footer motto, italic (wording open, see §10)

## 5. Themes

All themes are baked into the binary. Switching is instant and persisted in `config.toml`.

| Theme | Mode | `config.toml` id | Palette source |
|---|---|---|---|
| Default Light | light | `default-light` | this project |
| Default Dark | dark | `default-dark` | this project (Chron1's) |
| Noctalia | dark | `noctalia` | interpretation |
| Catppuccin Latte | light | `catppuccin-latte` | upstream |
| Catppuccin Frappé | dark | `catppuccin-frappe` | upstream |
| Catppuccin Macchiato | dark | `catppuccin-macchiato` | upstream |
| Catppuccin Mocha | dark | `catppuccin-mocha` | upstream |
| Rosé Pine | light/dawn | `rose-pine` | upstream (Dawn) |
| Ruby Theme | dark | `ruby` | interpretation |
| Ubuntu Canonical Aubergine | dark | `ubuntu-aubergine` | Canonical brand colours |
| Paperlike gradient theme | light | `paperlike` | interpretation (see below) |

Pinned in Chron5. The hex sets live in `src/theme.rs` as one `const` per theme and are pushed into the `Palette` global in `ui/palette.slint`, the same way `src/strings.rs` fills `Strings`; the global holds initializers for Default Dark so a default start paints no intermediate frame. **Themes are colour tables and nothing else** — no per-theme fonts, radii or spacing. Twelve roles: five surfaces (`bg`, `panel`, `raised`, `selection`, `border`, in order of distance from the canvas), `text`, `muted`, `accent`, `danger`, `paper`, `paper-edge`, and `backdrop`.

Three things about that table are decisions rather than details. `paper` is white in every theme, because a rendered page arrives opaque and white-backed and the image covers the sheet exactly; only `paper-edge` varies. `backdrop` carries its own alpha and is the dim behind a sheet — it was a literal in `ui/form.slint` until Chron5, and it is what makes a sheet over a light theme dim a light window. And Ruby's `danger` is amber rather than red: in a theme whose accents are already ruby, another red does not read as an error.

**Paperlike is a ladder, not a literal gradient.** A real `@linear-gradient` would mean every themed `background:` in every `.slint` file taking a brush rather than a colour — the whole UI's colour plumbing changed for one theme of eleven. What ships is the warm near-white ladder that gradient implies. A real gradient is a later change to the palette's *type*, not to its values.

Every palette is held to a contrast floor by test: body text at 4.5:1 against `bg` and `panel`, and `muted`, `accent` and `danger` at 3.0:1 against `panel`. The floor exists to catch a light theme built by inverting a dark one; it is not a design review.

## 6. Export

Label in UI: **EXPORT**. Product-level action producing one all-covering PDF:

1. MuPDF generates a clean summary page from the product's data — name, serial number, purchase date, warranty start/end, days left at time of export, purchase link. Searchable text, print-friendly, theme-independent.
2. All of the product's PDFs are appended in tab order.
3. Output: a single `Parachron-<product-name>-<date>.pdf`, save location chosen by the user via file dialog.

Settled in Chron7. The summary page is A4 whatever the appended documents are, and appended pages keep their own sizes — rescaling somebody's invoice to match would be the export altering their evidence. The page is drawn in black on white and reads nothing from the theme, because a printed page is not a window. The countdown goes through the same `days_left` and `countdown` the details column uses, so the figure on the page and the figure on screen cannot disagree.

**A document that cannot be included is skipped, not fatal.** A file listed in `product.toml` but absent, encrypted, or unreadable cannot be appended; the export still produces the summary and everything that could be read, and names what it left out **on the summary page itself**. A notice in the window is gone when the app closes; the exported file is what gets emailed to a shop six months later, so it carries its own gaps — the same principle that keeps broken folders visible in the list with a readable reason.

**Every text run on the page is registered as a composite font, not a simple one.** This is not a detail. A base-14 font in its default Latin encoding silently drops `ğ ş ı İ` — no error, the words simply are not in the file — and product names and serial numbers are user data, so an English session has to export `Şarj Cihazı` correctly. `Ü` survives a Latin encoding because it is in Latin-1, which makes the bug easy to test around by accident. Composite everywhere, unconditionally.

The output is written through `write_to` rather than `save`, which takes a `&str` and therefore cannot express a destination path that is not valid UTF-8 — on Linux a path is bytes, so that is a real file a user could pick. Nothing about export invalidates the render worker's cache: the output goes outside the vault and the product's own files are only read.

## 7. Packaging & CI

Targets, built by GitHub Actions on tagged releases:

| Asset | Platform | Tooling |
|---|---|---|
| `.pkg.tar.zst` | Arch / CachyOS (pacman) | PKGBUILD, `makepkg` in CI |
| `.deb` | Debian / Ubuntu | `cargo-deb` |
| `.exe` | Windows | cross-build or `windows-latest` runner (no local Windows machine — CI owns this target) |

Install layout (Linux): binary to `/usr/bin/parachron`; icons from `build/icons/` into `/usr/share/icons/hicolor/<size>/apps/parachron.png`; desktop entry `org.parachron.Parachron.desktop` to `/usr/share/applications/`. The `.ico` feeds the Windows build.

MuPDF note: AGPL-3.0 obligations are satisfied by the public repo; CI must build MuPDF statically or bundle its library per target.

## 8. Conventions

| Typical | Parachron uses |
|---|---|
| SPEC.md | **CORE.md** (this file) |
| Milestone 1, 2, 3… | **Chron1.md, Chron2.md, Chron3.md…** |

Chron files live in `chrons/` at the project root (`/home/megas/PARACHRON/chrons/`), are written before coding a milestone, and reference CORE.md sections by number.

### Development rules (binding for all tooling, including Claude Code)

1. **No AI attribution anywhere.** Commits, code comments, README, release notes and UI must never contain trailers like "Made by Claude", "Claude Session", "Claude Code", "Co-Authored-By: Claude" or similar. Banned outright.
2. **Single author.** All commits, pushes and pull requests are authored by the `sudo-megas` GitHub account — never a bot or AI account identity.
3. **User-facing README.** `README.md` follows the layout prompt in `usereadme.md` (anaphored from JADEITE): written for users landing on the GitHub page, friendly, minimal — no changelogs, no developer oceans of info.

## 9. Chron roadmap

One line of planned scope per milestone. Each Chron file is written in detail only when its milestone begins — earlier ones reshape later ones, so this table is the map, not the terrain. Merging or splitting milestones is allowed; update this table when it happens.

| Chron | Scope |
|---|---|
| Chron1 | Scaffold: cargo + Slint build, data layer (folder scan, TOML parse, broken-file handling), bare three-column window with product list, string-table plumbing |
| Chron2 | Document viewer: MuPDF page rendering, workspace tabs, serial strip |
| Chron3 | Add/edit products: Document menu, input forms, PDF import into the data dir. Also built the vault seam that owns the product list, and with it `SortMode` and its comparators — the module exists to own list order, and shipping it with a hardcoded order would have meant rewriting its centre a milestone later. The toggles anyone can *see* still arrived in Chron4 |
| Chron4 | Details column: dates, copy-link, days-left counter, sort toggles |
| Chron5 | Theming: all 11 palettes, THEME picker |
| Chron6 | Localization: full EN/TR string tables, language switch |
| Chron7 | Export: summary page generation + PDF merge |
| Chron8 | About view + column-1 search bar + polish: error states, min-size behavior, edge cases. The search bar was asked for after Chron7 closed and folded in here rather than becoming a milestone of its own — it lands in column 1, which is where this milestone's other layout work already is. It does mean Chron7's line about being the last milestone to add a feature stopped being true one milestone later; §9 is the map, and the map changed |
| Chron9 | Packaging & CI: PKGBUILD, .deb, Windows .exe, GitHub Actions, AUR; `README.md` written per `usereadme.md` (§8 rule 3) once release assets exist |

## 10. Open items

- ~~About subtitle and footer motto: wording to be chosen~~ — chosen while writing Chron8. Subtitle: **"Paper Vault"** / **"Belge Kasası"**. Footer motto: **"Built with Reason and Passion"** / **"Akıl ve Tutkuyla"** — JADEITE's own motto, carried across as a maker's signature rather than a second description of the app. Both are keys in the string table like everything else; Chron8 is where they reach the screen.
- ~~Serial-number strip exact size ratio~~ — settled in Chron2: a fixed **44px**, not a proportion. It holds one line of text, and a proportional strip would grow absurd on a tall window.
- ~~Theme palettes: exact hex sets per theme~~ — pinned in Chron5; see §5 for where they live and which are upstream. The prediction that this would be a contained change was *nearly* right: every colour did live behind the `Palette` global except the sheet backdrop, which Chron3 had added as a literal, and two colours that were in the global but never reached the screen — the page's edge, drawn as a border the page image painted over, and the zoom slider, which came from `std-widgets` and so read the Slint style rather than the palette.
- Remaining unthemed: the `std-widgets` `ListView` scrollbar in column 1, which appears only when the product list overflows (about seventeen products). Replacing it means replacing a virtualizing list, which is a different job from replacing a slider. Chron8 leaves this open on purpose rather than closing it by silence: replacing the list means betting that no vault is large enough for virtualization to matter, which is a bet about somebody else's data. See Chron8's technical notes for what each answer costs.
- Release date in the About view (§4): stamped by `build.rs` at compile time, so a source build honestly reports the day it was built. Decided while writing Chron8; Chron9 will want to control the value for tagged release builds.
