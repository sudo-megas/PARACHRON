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
| File picker | `rfd` — `xdg-portal` on Unix, no features on Windows | Native dialogs with no GTK development headers, which keeps §7's three targets cheap. `wayland` is deliberately **off**: its window identifier roundtrips a second event queue on Slint's own display from a foreign thread, which risks a deadlock in exchange for cosmetic dialog parenting. Always driven through `AsyncFileDialog` — the blocking call parks the caller in an untimeouted D-Bus read. **Target-split in Chron11** (see below) |
| Clipboard | `arboard` — `wayland-data-control` on Unix, no features on Windows | No image support; Parachron only ever copies text. Target-split in Chron11 |

**The two split rows are hygiene, not a fix, and the distinction is the record
worth keeping.** Both feature sets name a Linux mechanism, and the obvious
reading — that a Windows build therefore has neither a file dialog nor a
clipboard, which would make Add Document, EXPORT, the serial strip, the purchase
link and both About URLs dead on the target §7 says CI owns — was written into
Chron11 as a defect before anybody checked it. It is wrong.
`cargo tree --target x86_64-pc-windows-msvc -e features` resolves `rfd` with
`windows-sys` and `Win32_UI_Shell`, and `arboard` with `clipboard-win`: the
platform backends are gated on `cfg(target_os = …)`, not on cargo features, so
asking for a Linux backend on Windows is inert rather than exclusive. The split
buys two things — the next reader does not have to run that command, and a
future release that *does* make those features exclusive cannot break the
Windows target silently.

Rejected during planning: Python, TypeScript, Qt, Electron (vetoed on sketch); iced, egui, GTK4 (GUI runners-up); pdfium-render, Poppler, pure-Rust PDF (render runners-up); SQLite, JSON, RON (storage runners-up).

## 3. Data model

Data lives under the platform's own data directory. Until Chron11 this section
named one path, because there was one target that anybody could install; a
Windows asset makes it three, and the path is not the same on all of them.

| Target | Data directory |
|---|---|
| Arch / CachyOS, Debian / Ubuntu | `$XDG_DATA_HOME/parachron/`, in practice `~/.local/share/parachron/` |
| Windows | `%APPDATA%\parachron\data\`, in practice `C:\Users\<user>\AppData\Roaming\parachron\data\` |

The tree below is written with the Linux path, which is the one the rest of this
document and the README use.

**The Windows path has a `data` segment nobody chose, and it is worth naming
rather than discovering.** `data.rs` calls `ProjectDirs::from_path("parachron")`
— pinned literally rather than built from a qualifier/organisation triple, so
the directory is named `parachron` and not `com.sudo-megas.Parachron` on one
platform and something else on another. On Linux `directories` resolves that to
`$XDG_DATA_HOME/parachron`. On Windows it resolves to
`%APPDATA%\parachron\data`, because `ProjectDirs` reserves the project folder
for a set of siblings — `data`, `config`, `cache` — where XDG already separates
those at the root. Parachron only ever uses `data_dir()`, so `config.toml` lives
inside `data\` on Windows alongside `products\` exactly as it does on Linux;
`config\` is created by nothing and stays absent. Read from the source at
`directories-6.0.0/src/win.rs:77` rather than from its documentation, since this
is a path a user is going to be asked to paste into Explorer.

macOS is not a target (§7) and so has no row. `directories` would resolve it to
`~/Library/Application Support/parachron`, which is recorded here only so that
the absence reads as a decision rather than an oversight.

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

### Where the vault is (Chron9)

The tree above is the default, not the only arrangement. Parachron *copies* documents into the vault rather than referencing them where it found them, so a vault grows with the paperwork put into it — and the default puts that growth on whatever disk `$HOME` sits on. `config.toml` therefore carries an optional `vault` key naming a directory, and `products/` lives under it.

**`config.toml` does not move, and cannot.** It holds the key that says where the vault is, so it cannot live inside the vault: the app would need the location in order to read the setting that gives it the location. The two split accordingly, and only one of them travels:

| Path | Holds | Moves |
|---|---|---|
| `<data dir>/config.toml` | theme, language, sort mode, window size, `vault` | never |
| `<vault>/products/` | one folder per product, with its PDFs | yes |

`vault` absent, or present and empty, means the vault **is** the data directory — which resolves `products/` to `~/.local/share/parachron/products/`, exactly what every install had before the key existed. There is no migration and nothing a user who does not want this has to notice. Moving the vault back to the default writes no key rather than the default path spelled out, so the file matches what a fresh install would have.

**A configured vault is checked, never created.** The default vault is created on first run; its parent is the platform's own data directory and exists on any machine with a home. A configured one is only ever looked for. If `vault` names a path under a mount point and the drive is not mounted, that mount point is an ordinary empty directory on the root filesystem — creating the vault there would put documents on the system disk while their owner believed they were on the drive bought for exactly this, and mounting the drive afterwards would hide the lot underneath it. So a missing configured vault puts its path on screen, creates nothing, and does **not** fall back to the default, because a silent fall back is indistinguishable from total data loss to whoever is reading the window.

The same rule reaches one file further out. A `config.toml` that will not parse used to degrade to the defaults, which cost a theme; with a `vault` key it would cost sight of the vault, so it is now reported as a broken entry naming the file rather than guessed at. "No `vault` key" and "this file did not parse" are different answers.

A relocation is a **move**, not a repointing: `fs::rename` where it works, and copy → verify → remove where it does not, which is the cross-filesystem case the feature exists for. The source is removed only after the copy verifies, so a failure at any point leaves the original vault complete and `config.toml` still naming it.

Two consequences worth stating rather than discovering. "Back up the vault" and "back up everything" stopped being the same sentence once the two can be in different places — settings are small and reproducible, documents are neither — which is why the About pane names the vault's location, as plain text with copy-to-clipboard and nothing that opens. And a vault that cannot be found is not a vault that is empty: the list has never hidden a folder it could not read, and it does not start with the folder that holds all of them.

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

### config.toml schema

```toml
lang = "en"                       # "en" | "tr"
theme = "default-dark"            # one of §5's eleven ids
sort = "added"                    # "added" | "name" | "purchase"
window_width = 1280
window_height = 800
vault = "/mnt/ironwolf/parachron" # optional (Chron9); absent means the data dir
```

A path is bytes on Linux and a TOML string is UTF-8, so a vault path that is not valid UTF-8 cannot be written here at all. It is refused when it is chosen, rather than lossily converted into a similar-looking path that would be wrong every time it was read back.

Rules: dates are stored as **native TOML dates** — RFC 3339 `YYYY-MM-DD`, the only form TOML parses — and rendered in the UI as `DD-MM-YYYY` (e.g. `14-03-2026`). Storage format and display format are separate concerns; never write `DD-MM-YYYY` into a `.toml` file. `warranty_end` is entered by the user together with `warranty_start` (both come from the warranty card). Days left = `warranty_end - today`, clamped at 0, displayed as e.g. `658 days`. Missing or malformed TOML must never crash the app — the product appears in the list flagged as broken, with a readable error.

## 4. UI layout

Window: resizable, minimum **1000×700**. Three fixed columns — **25 / 50 / 25** — the classic list | content | inspector pattern. Chosen over the earlier two-column draft because invoices are portrait A4: a full-height center column shows roughly twice the readable page area from the same window.

```
┌───────────────────────────────────────────────────────────────────┐
│ [icon] Document ▾ | Add Document        PARACHRON          – □ ✕  │
├───────────────┬───────────────────────────────┬───────────────────┤
│ Column 1(25%) │ Column 2 (50%)                │ Column 3 (25%)    │
│ [A–Z] [Date]  │  [Invoice] [Garanti] [.pdf]   │  [THEME] [EXPORT] │  ← masthead (panel)
│ ┌───────────┐ │  ┌─────────────────────────┐  │                   │
│ │ Search… ✕ │ │  │                         │  │  Link:            │
│ └───────────┘ │  │                         │  │  store.example/p  │
├ ─ ─ ─ ─ ─ ─ ─ ┤  │   PDF preview (MuPDF)   │  ├ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤  ← canvas body (bg)
│ Product list  │  │                         │  │  Purchase date    │
│ (hover, sel.  │  │                         │  │  14-03-2026       │
│  accent bar,  │  │   full height,          │  │                   │
│  ⚠/! swatch)  │  │   portrait-friendly     │  │  Warranty start   │
│               │  └─────────────────────────┘  │  14-03-2026       │
│               │  ‹ ›  2 / 12      Zoom ──●──   │  Warranty end     │
│               │  Serial number: ABC123XYZ  ⧉  │  14-03-2026       │
│               │                               │ ┌─────────────────┐
│               │                               │ │ Warranty left   │
│               │                               │ │ 658 days        │
│               │                               │ │ ▬▬▬▬▬▬▬▬░░░░░░  │
│───────────────│                               │ └─────────────────┘
│ [ⓘ About]     │                               │                   │
└───────────────┴───────────────────────────────┴───────────────────┘
```

Every column follows the same two-tone structure column 2 established first (Chron2's tab strip): a `Palette.panel` masthead band at the top — the sort/search row in column 1, the tab strip in column 2, the THEME/EXPORT row in column 3 — over a `Palette.bg` canvas body underneath. The seam between columns is a 1px hairline plus a content gutter (8px for column 2, 4px for column 3 — narrower there by width arithmetic against the 1000×700 floor, not by eye), so a column reads as its own domain rather than being separated from its neighbor by the hairline alone (Chron10).

Column 1 — product list. Default order: as added (`added` field). Two sort toggles: alphabetical (by `name`) and purchase date (oldest first, newest at bottom). Under the toggles and directly above the entries, a **search bar**: full column width, fixed height, matching product **name and serial number** as you type and narrowing the list to what matches. Bottom strip: a fixed **About** entry (JADEITE-style sidebar footer) that opens the About view. Each row shows a hover state, a left accent bar on the selected row, and — for a broken folder or a product missing a listed file — a small fixed-position colour swatch beside its name, in addition to the row's existing `⚠`/`!` text prefix (Chron10; the prefix is unchanged, the swatch is a second, non-textual landmark bound to the same underlying state).

The search bar was added to this layout in Chron8; the original wireframe had column 1 as a list and nothing else, and the sort toggles were never drawn into it either. Both are in the sketch above now. Three rules it follows, so that later work does not have to rediscover them. Matching is **folded** — accent- and case-insensitive in both directions, so `sarj` finds `Şarj Cihazı` and Turkish's dotless `ı` cannot make a product unfindable by its own name. A broken folder is matched on its **folder name**, because that is the only text it has and hiding the entry somebody is looking for is the one thing the list has never done (§3: never crash, never hide). And the query is **session state, not settings** — it is never written to `config.toml`, because a sort order that survives a restart reorders and a filter that survives one *hides*, and an app that opens showing three of your eleven products has lost them as far as you can tell.

Column 2 — document viewer. Workspace-style tabs switch between the selected product's PDFs (`pdfs` order); each tab is labelled with the file-name stem, so `invoice.pdf` reads `Invoice`. The preview takes the full remaining height and shows **one page at a time, fitted whole** inside the pane. Under it a control row carries `‹` / `›`, a `2 / 12` page counter, and a **zoom slider** — zoom is a multiplier of the fit scale (`1×`–`4×`), so `1×` always means the whole page is visible whatever the window size; above `1×` the page pans. Page and zoom reset whenever the tab or product changes. Below that, a fixed **44px** serial-number strip: click it to copy the serial to the clipboard with a brief "copied" confirmation, the same gesture the purchase link uses in column 3.

Column 3 — details + actions. Top: THEME and EXPORT buttons, in the masthead band — EXPORT is the affirmative action of the pair and is styled as this app's one `primary` button (THEME merely opens a picker). Then purchase link (click = **copy to clipboard** with a brief "copied" confirmation — never opens a browser; wraps to two lines before eliding, like the three date rows below it), purchase date, warranty start, warranty end, and — anchored in its own card at the foot of the column — the warranty-left counter in days, bold, largest text in the column, with a thin proportional gauge underneath showing how much of the warranty span has elapsed (Chron10). A warranty that has run out reads as expired rather than as a negative number, in the error colour, and the gauge fills fully in the same colour.

Warranty end was added to this list in Chron4; the original wireframe above omitted it. Hiding the date the countdown is counting toward leaves anyone checking the number with nothing to check it against.

Columns are fixed-ratio; the content structure is identical for every product.

### App-wide principles

**No external opens.** Like JADEITE, Parachron never opens external addresses or launches a browser. Every URL in the app (purchase links, About URLs) is plain text with copy-to-clipboard where useful.

**Localization.** All UI strings exist in **English and Turkish**, switchable at runtime, persisted as `lang = "en" | "tr"` in `config.toml`. Default is always `en` — the app never checks the system locale; switching to Turkish is a deliberate user action via the `Document ▾` menu. No hardcoded strings in `.slint` or Rust — everything goes through the string table.

Two things about "switchable at runtime", settled in Chron6. Refilling the `Strings` global only relabels what is *bound* to it; roughly half the text on screen is composed in Rust and pushed into ordinary properties — list rows, a broken folder's reason, the warranty countdown, a document that will not open, the theme picker's rows — and none of it knows the language changed. All of it derives from the current selection, so one re-plan through the vault recomputes the lot; there is one route rather than several that could disagree. And what is *on disk* is not UI copy and never translates: folder names, file names, the tab labels derived from file stems, and the diagnostic tails of OS and TOML-parser messages.

Language names are written in their own language in both tables (`English`, `Türkçe`), so a reader who has landed in a language they cannot read can still find their own. Turkish maps `i`→`İ` and `ı`→`I`, so any label that shouts is stored shouting and never passed through `to_uppercase` — `EXPORT` is `DIŞA AKTAR`, with a dotless capital.

**The `Document ▾` menu** holds Add Document, Edit Document…, **Vault location…** (Chron9), and the two language rows, in that order with a hairline before each group. Vault location sits with the document actions rather than with the languages because it is a thing done to the vault; it is below both of them because it is done once rather than daily. It opens a folder picker, then a sheet that names the current path, the chosen path, and how many documents and megabytes are about to move — the count is what makes confirming a decision rather than a dare. While the move runs the sheet shows a determinate bar, the file count, the byte total and the name of the file being copied, and it stays up when the move lands rather than vanishing: a copy that ran for minutes and then blinked out leaves no way to tell "it worked" from "it gave up". The bar is drawn from `Palette` like everything else — §10 records what happens when a widget is taken from `std-widgets` instead.

### About view

Anaphored from JADEITE's About. Selecting About in the column-1 footer swaps the content area (columns 2+3) for a single centered pane:

- App icon, large (from `build/icons/`)
- `P A R A C H R O N` wordmark, letter-spaced
- Subtitle line (one-phrase app description — wording open, see §10)
- Maker — `sudo-megas`
- Version — from `Cargo.toml` at build time
- Release date — and it has **two honest sources, not one** (Chron11). `build.rs`
  emits `PARACHRON_BUILD_DATE` at compile time, and an existing value in the
  environment wins over the runner's clock. So a build from source reports the
  day it was compiled, which is the only date such a binary can truthfully
  claim; a tagged release has `release.yml` set the variable from the tag's own
  date, so the asset a person downloads shows the day it was released rather
  than the day a runner happened to pick the job up. The pane renders whichever
  it got through the same `fmt_date` as every other date on screen
- Vault — where the products actually are (Chron9). Plain text with copy-to-clipboard and nothing that opens: §3 promises no hidden state, and a folder chosen through a dialog that cannot be read back afterwards is hidden state. It is the one value in this pane that can change while the window is open, so a move pushes it again
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

Pinned in Chron5, widened in Chron10. The hex sets live in `src/theme.rs` as one `const` per theme and are pushed into the `Palette` global in `ui/palette.slint`, the same way `src/strings.rs` fills `Strings`; the global holds initializers for Default Dark so a default start paints no intermediate frame. **Themes are colour tables and nothing else** — no per-theme fonts, radii or spacing. Fourteen roles: five surfaces (`bg`, `panel`, `raised`, `selection`, `border`, in order of distance from the canvas), `text`, `muted`, `accent`, `accent2`, `accent3`, `danger`, `paper`, `paper-edge`, and `backdrop`.

**Three hues, not one, and this is the correction Chron10 exists for.** The table shipped with a single `accent` from Chron5 until Chron10, and the consequence was that every one of these eleven palettes arrived on screen as five greys and one colour — Noctalia's published set has five colours in it and the app was drawing exactly the darkest, which is what it looked like. `accent` still carries *interactive state*: focus rings, the selected row, an active chip, the affirmative button. `accent2` and `accent3` carry *section identity* — column 1 wears `accent`, column 2 `accent2`, column 3 `accent3`, each as a solid rule across the top of its card and mixed as a tint into its masthead band, so the three columns read as three places rather than as three helpings of the same panel. Where a source palette publishes more hues than three (Catppuccin ships fourteen accents per flavour), the two extra are taken from it; where one had to move to clear a contrast floor, `src/theme.rs` says so at the const and says by how much.

Three things about that table are decisions rather than details. `paper` is white in every theme, because a rendered page arrives opaque and white-backed and the image covers the sheet exactly; only `paper-edge` varies. `backdrop` carries its own alpha and is the dim behind a sheet — it was a literal in `ui/form.slint` until Chron5, and it is what makes a sheet over a light theme dim a light window. And Ruby's `danger` is amber rather than red: in a theme whose accents are already ruby, another red does not read as an error.

**Paperlike is a ladder, not a literal gradient.** A real `@linear-gradient` would mean every themed `background:` in every `.slint` file taking a brush rather than a colour — the whole UI's colour plumbing changed for one theme of eleven. What ships is the warm near-white ladder that gradient implies. A real gradient is a later change to the palette's *type*, not to its values.

Every palette is held to a contrast floor by test: body text at 4.5:1 against `bg` and `panel`, and `muted`, `accent` and `danger` at 3.0:1 against `panel`. `accent2` and `accent3` are held to the same 3.0:1 but against `panel` and **`bg`** rather than `selection` — they are never drawn on a selected row, and the two surfaces a column rule actually touches are the card it caps and the channel beside it. The floor exists to catch a light theme built by inverting a dark one; it is not a design review.

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

Sizes 16 through 512 are installed; `parachron-1024.png` is 1.6MB, is larger than any icon theme will ask for, and stays in the repository as artwork for a README header rather than shipping in a package.

The licence text ships at **the path each distribution's own tooling reads**, which is not one path for both: `/usr/share/licenses/parachron/LICENSE` on Arch, `/usr/share/doc/parachron/copyright` on Debian. On Windows there is no such path at all, so the About pane and the executable's `LegalCopyright` resource are the only places the terms appear.

MuPDF note: AGPL-3.0 obligations are satisfied by the public repo; CI must build MuPDF statically or bundle its library per target. `release.yml` asserts this on the artefact rather than trusting the intention — `ldd` on the two Linux binaries and `dumpbin /dependents` on the `.exe`, each failing the job if anything named `mupdf` appears.

### What the Windows spike settled (Chron11)

**MuPDF per target.** `mupdf-sys` vendors and statically links MuPDF on all three targets with no per-target special-casing. It builds under MSVC on `windows-latest` in **ten to twelve minutes** (484s and 712s on two runs) against roughly 1m40s on Linux, and needs `LIBCLANG_PATH` set explicitly — LLVM is on the image but `bindgen` does not find it unaided. No asset depends on a MuPDF the user installs.

**The renderer, and this one has a user-facing consequence.** Slint's default FemtoVG renderer needs an OpenGL driver with shader support. On a machine that has none — a headless runner, a VM, some remote-desktop sessions — the binary exits immediately with `Failed to initialize OpenGL driver: Could not locate glCreateShader symbol`. `SLINT_BACKEND=winit-software` runs correctly on the same machine.

The decision is **a documented environment variable, not a build feature**. Forcing the software renderer into the release build would make every ordinary install slower to rescue an unusual one, and the failure is loud rather than silent — it names the missing symbol and exits non-zero, which is not the "opens to nothing" case that would have justified changing the default. The README carries the variable in its Windows section.

**The Visual C++ runtime: documented for 1.0, not yet fixed.** The `.exe` imports `MSVCP140.dll`, `VCRUNTIME140.dll` and `VCRUNTIME140_1.dll`, which Windows does not ship. The proper remedy is `-C target-feature=+crt-static` *together with* forcing `mupdf-sys`'s C and C++ build to `/MT`, since mixing runtimes is its own class of bug — a spike of its own rather than a line in a workflow.

For 1.0 the decision was to **say so on the page instead of shipping an unverified build change**. The README's Windows section now names the redistributable, quotes the exact error Windows gives, and links Microsoft's installer. That keeps the page true — "there is nothing to install" was not — without putting an untested CRT switch into the one target with no local machine. The failure it describes is loud and self-diagnosing: Windows names the missing DLL. Static linkage remains the better answer and stays open.

**Arch packaging requires `options=(!lto)`.** Arch's stock `makepkg.conf` enables GCC link-time optimisation for every package on the machine, so `mupdf-sys`'s `cc` build emits GCC LTO bytecode instead of native objects, and `rust-lld` — which cannot read it — reports every MuPDF symbol as undefined. This is a default rather than a local preference, so it would have broken both the documented `makepkg -si` route and the CI-built `.pkg.tar.zst` on their first real run. Rust's own LTO in `[profile.release]` is unaffected and still applies.

### Runtime dependencies are two lists, and the second is invisible (Chron11)

`ldd target/release/parachron` reports twelve libraries. Slint's winit backend and `rfd` open **thirteen more** with `dlopen` at runtime, and a `dlopen`ed library appears in no `ldd` output and is missed by `dpkg-shlibdeps`, which is what `cargo-deb`'s `$auto` runs. A dependency list built from `ldd` alone therefore produces a package that installs cleanly, appears in the menu, and then does nothing when it is clicked.

Both lists are written out in full in `packaging/PKGBUILD` and in `[package.metadata.deb]`, they are the same list, and they move together. Twelve of the thirteen are the X11, Wayland and GL libraries a window needs. The thirteenth is **libdbus**, which `rfd` opens for the portal file dialog and whose absence it *logs rather than raises* — so omitting it yields an app that starts, draws its window, opens its menu, and silently declines to add a document. That is a worse failure than the other twelve, all of which produce an app that visibly does not start.

**And a fourteenth entry that is not a library at all: a font.** `fontconfig` is in the linked list, and `fontconfig` is the machinery that *finds* fonts — it is not a font. On a system carrying fontconfig and no font files, the lookup returns no match and `fontique` unwraps it:

```
fontique/src/backend/fontconfig.rs:685
called `Result::unwrap()` on an `Err` value: NoMatch
```

which is a panic rather than a blank window. Arch depends on `ttf-font`, the virtual package every font provides, so the requirement is "a font" rather than a particular one; Debian has no equivalent virtual and names `fonts-dejavu-core`. Any desktop already satisfies both; a minimal install or a container does not.

Three findings of the same shape in one milestone — libdbus, the font, and `dlopen` versus `ldd` — is enough to state the rule rather than the instances: **a dependency list built from what the linker reports is a list of what the program needs to start, not of what it needs to work.** Everything opened at runtime, and everything that is data rather than code, has to be added by reading the program rather than the binary.

### Windows resources (Chron11)

`build.rs` compiles `build/icons/parachron.ico` and `build/parachron.manifest` into the executable through `winresource`. The manifest carries DPI awareness (`PerMonitorV2`), `asInvoker`, and UTF-8 as the process code page — all three read by Windows before the program's own code runs, so none can be set at runtime.

**The resource step is gated on `CARGO_CFG_TARGET_OS`, not on `cfg!(windows)`.** Inside a build script the `cfg` macros describe the host; the question is the target. `winresource` is declared under `[target.'cfg(windows)'.build-dependencies]`, which Cargo also resolves against the host — so a *cross*-build for Windows from Linux cannot embed resources at all. `release.yml` therefore builds Windows on `windows-latest`, and `build.rs` emits a loud `cargo:warning` rather than silently producing an icon-less `.exe` if anybody takes the cross-build route this table still permits.

### A release cannot be re-published, only added to (Chron11)

`release.yml`'s publish step calls `gh release create`, which refuses a tag that already has a release. So a workflow re-run against an already-released tag fails at the last step even when all three builds succeed, and an asset added after the fact needs `gh release upload` instead. The fix, when someone wants it, is a `create`-or-`upload` fallback in that step; it is recorded here rather than applied because it was found by colliding with a live release and should be changed before a release rather than during one.

The related constraint: **a published tag cannot be moved.** Retagging needs a force-push, and deleting a tag that a release points at endangers the release. So a packaging defect discovered after publication is fixed by a new version, not by rebuilding the old one — which is why 1.0.0 and 1.0.1 exist an hour apart, with an identical binary and a corrected `Depends`.

### The AUR was designed and withdrawn

An AUR package (`paru -S parachron`) was fully designed in Chron11 and then not shipped: the Arch User Repository was temporarily disabled by its own maintainers following attacks on it. The design is kept struck through in that file rather than deleted, so it can be restored rather than re-derived.

**The condition for its return is simply that the AUR reopens.** What comes back with it: an AUR account with an SSH public key registered and that key held as a repository secret; a `release.yml` step pushing to `ssh://aur@aur.archlinux.org/parachron.git`; the README's Arch section regaining an AUR route; and CORE §8 rule 2 applying to that push in full, because publishing to the AUR *is* a git commit in a git repository and must be authored by `sudo-megas`. With it gone, no workflow in this repository makes a commit anywhere.

## 8. Conventions

| Typical | Parachron uses |
|---|---|
| SPEC.md | **CORE.md** (this file) |
| Milestone 1, 2, 3… | **Chron1.md, Chron2.md, Chron3.md…** |

Chron files live in `chrons/` at the project root (`/home/megas/PARACHRON/chrons/`), are written before coding a milestone, and reference CORE.md sections by number.

### Development rules (binding for all tooling, including Claude Code)

1. **No AI attribution anywhere.** Commits, code comments, README, release notes and UI must never contain trailers like "Made by Claude", "Claude Session", "Claude Code", "Co-Authored-By: Claude" or similar. Banned outright.
2. **Single author.** All commits, pushes and pull requests are authored by the `sudo-megas` GitHub account — never a bot or AI account identity.

   **How this applies to a release (Chron11).** A GitHub release created by
   `release.yml` is attributed to the workflow's token, which is neither a
   person nor a commit — it is an artefact upload against a tag, and *the tag is
   the authored thing*. Rule 2 binds the tag, which a person pushes by hand and
   nothing in this repository can push for them. Stated here rather than left
   for a future reader to discover, so that "no bots" is not quietly read as
   having been interpreted loosely. Nothing in any workflow makes a commit; the
   one step that was going to — publishing to the AUR, which is a `git push` to
   a git repository and would have needed a credential — went away with the AUR
   itself (§7).
3. **User-facing README.** `README.md` follows the layout prompt in `usereadme.md` (anaphored from JADEITE): written for users landing on the GitHub page, friendly, minimal — no changelogs, no developer oceans of info.

## 9. Chron roadmap

One line of planned scope per milestone. Each Chron file is written in detail only when its milestone begins — earlier ones reshape later ones, so this table is the map, not the terrain. Merging, splitting **and reordering** milestones is allowed; update this table when it happens.

**Packaging has moved twice, and this paragraph is the index that resolves its number.** It was Chron9 from the first draft of this section until its milestone was about to start. A release is the one step that hands artefacts to people who did not build them, and choosing which disk the vault lives on moves a user's documents — so shipping a version that expects them in one place and then moving them is the wrong order to do two things in. The save-location milestone took the 9 slot and packaging became Chron10. Then the character milestone was asked for and finished while packaging was still `planned`, and the same rule applied a second time for the same reason: it is not a release, so it goes first. Packaging is **Chron11**.

Everything written before each move is left alone. Chron1 through Chron8 each list "packaging (Chron9)" in their **Out** sections, Chron9's own file hands work to "Chron10" and reports a finding for it, and both were true when written — this project has always preferred an annotation over a rewrite, and a milestone file is a record of what was known on the day it was written, not a document that gets edited to stay convenient. Read those references as "the packaging milestone" and come back here for its current number. What does get corrected is anything a reader would act on today: this table, and code comments that name a milestone as still forthcoming.

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
| Chron9 | The vault's location, chosen by the user: a `vault` key in `config.toml`, a folder picker behind `Document ▾`, and a worker that moves an existing vault onto the disk it names. Asked for after Chron8 closed, because the app *copies* documents into the vault and a vault therefore grows on whatever disk `$HOME` happens to sit on. It takes the 9 slot rather than the last one because releases have to be last — see the paragraph above the table |
| Chron10 | Character: a masthead-over-canvas structure for every column, each wearing one of three palette hues, with the columns drawn as inset cards on the window's canvas rather than divided by a hairline; hover, selection and status landmarks for column-1 rows; a column-3 anchor card with a warranty-elapsed gauge replacing two dead flex spacers; pressed-state and a real `primary` treatment across all five hand-rolled button recipes. Widened the colour table from twelve roles to fourteen, which is the milestone's real subject: eleven palettes had been arriving as one hue each. Also the icon-identity fix — app id, desktop entry and `generate.sh`, ported off a worktree that had built them and never merged, alongside corrected artwork. Takes the 10 slot for the same reason Chron9 does: it is not a release, and releases go last |
| Chron11 | **Done. Parachron 1.0.1 released with all three assets, built by `release.yml` end to end.** Packaging & CI: PKGBUILD, .deb, Windows .exe, GitHub Actions. ~~AUR~~ — designed and then withdrawn, because the Arch User Repository was disabled by its own maintainers after the attacks on it; Chron11 keeps the design struck through rather than deleted, so it can be restored rather than re-derived if the AUR returns. `README.md` was written per `usereadme.md` (§8 rule 3) *before* this milestone rather than after it — the page is what a visitor lands on, and having it ready means this milestone only has to cut a tag rather than write a page as well. The cost is stated on the page itself: the download links point at a Releases page that is empty until the first tag, and build-from-source is what works until then |

### The roadmap is complete

**Chron11 closed the last row.** Eleven milestones took Parachron from a cargo scaffold to three packaged assets a stranger can download, and §9's table is now a history rather than a plan.

Two of Chron11's thirteen acceptance criteria — that the Windows `.exe` runs and shows its icon on a real machine, and that its file dialog opens — were **accepted by the maintainer rather than observed**, because this project has never had a Windows machine and CI cannot answer a modal dialog. `chrons/Chron11.md` records exactly what was established and what was not, under **The two criteria the maintainer closed**. That distinction is deliberate: a reader who later finds something wrong on Windows should be able to see that those two rows were a decision, not a measurement.

**What a twelfth milestone would start from**, if there is one — all recorded in §7 above rather than left in somebody's memory: the `create`-or-`upload` fallback in `publish`; `+crt-static` with `mupdf-sys` forced to `/MT`; the `ubuntu-22.04` runner's deprecation and why its successor is a `debian:bookworm` container rather than `ubuntu-24.04`; and the AUR, if it reopens.

## 10. Open items

- ~~About subtitle and footer motto: wording to be chosen~~ — settled in Chron8. Subtitle: **"Paper Vault"** / **"Belge Kasası"**. Footer motto: **"Built with Reason and Passion"** / **"Akıl ve Tutkuyla"** — JADEITE's own motto, carried across as a maker's signature rather than a second description of the app. Both are keys in the string table like everything else, and both are on screen.
- ~~Serial-number strip exact size ratio~~ — settled in Chron2: a fixed **44px**, not a proportion. It holds one line of text, and a proportional strip would grow absurd on a tall window.
- ~~Theme palettes: exact hex sets per theme~~ — pinned in Chron5; see §5 for where they live and which are upstream. The prediction that this would be a contained change was *nearly* right: every colour did live behind the `Palette` global except the sheet backdrop, which Chron3 had added as a literal, and two colours that were in the global but never reached the screen — the page's edge, drawn as a border the page image painted over, and the zoom slider, which came from `std-widgets` and so read the Slint style rather than the palette.
- Remaining unthemed: the `std-widgets` `ListView` scrollbar in column 1, which appears only when the product list overflows (about seventeen products). Replacing it means replacing a virtualizing list, which is a different job from replacing a slider. Chron8 leaves this open on purpose rather than closing it by silence: replacing the list means betting that no vault is large enough for virtualization to matter, which is a bet about somebody else's data. See Chron8's technical notes for what each answer costs. Chron10 made this more visible without changing it: the scrollbar now sits against column 1's card rather than a flush `panel`, which surfaces the same unthemed element more than before.
- A derived/mixed tone for column 3's anchor card (`Palette.panel.mix(Palette.raised, ...)` or similar), considered in Chron10 and deliberately not shipped — a few-percent tone delta is likely invisible on at least four of the eleven palettes (their `panel`/`raised` values sit close together), and picking one by eye mid-implementation was judged the wrong way to decide it. Plain `Palette.panel` on the card was verified sufficient across the four themes Chron10 screenshotted; if a future milestone finds a palette where the card doesn't read as separated, this is where that work starts.
- ~~Release date in the About view (§4)~~ — settled in Chron8: `build.rs` emits `PARACHRON_BUILD_DATE` as ISO at compile time and the pane renders it through the same `fmt_date` every other date uses, so a source build honestly reports the day it was built. An existing `PARACHRON_BUILD_DATE` in the environment wins, which is the seam Chron11 uses to stamp a tagged release with its tag's date instead of a runner's clock.
