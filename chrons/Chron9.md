# Chron9 — Vault location

**Milestone:** 9 of ~10 (CORE §9)
**Status:** planned
**Builds against:** CORE §3 (data model, in full — where the vault is, what is in it, and the promise that it outlives the app), §4 (the `Document ▾` menu, the About view, app-wide principles), §5 (a new surface is themed like every other one), §7 (packaging — Chron10 has to describe a data directory that is no longer one path), §8 (conventions & development rules), §9 (roadmap — this milestone and Chron10 swapped places)

## Goal

The vault goes where its owner wants it. Parachron *copies* documents into its vault — `import.rs` has done that since Chron3, deliberately, so that the originals stay the user's to move or delete — which means a vault grows in proportion to the paperwork put into it, and today it can only grow in one place: `~/.local/share/parachron/`, on whatever disk `$HOME` happens to sit on. A laptop with a small root partition and a large second drive has nowhere to put it.

So `config.toml` gains a `vault` key, the `Document ▾` menu gains a folder picker, and picking a folder moves what is already there onto it — with the move visible while it runs, because moving several gigabytes of PDFs across disks takes long enough that a still window reads as a crash.

Two things this milestone is really about, underneath the feature. The first is that **a wrong answer here loses documents**, which no previous milestone could do: Chron3 writes files, but into a directory the app created and knows; this one deletes a source directory after copying it somewhere else. The second is that **a vault the app cannot find must never look like a vault that is empty** — an app that opens showing none of your eleven products has lost them as far as you can tell, which is the reasoning CORE §4 already gives for keeping the search query out of `config.toml`, arriving here by a different route.

## Scope

**In:** a `vault` key in `config.toml` · `Paths` resolving `products/` under it while `config.toml` stays put · a `Vault location…` entry in `Document ▾` · a folder picker · a confirmation sheet naming both paths and what is about to move · a worker that moves the vault, reporting progress per file · `fs::rename` with a copy-verify-remove fallback for the cross-filesystem case that is the whole point · refusal of the three destinations a folder picker makes reachable and that would destroy data · a broken state for a configured vault that is not there · a broken state for a `config.toml` that will not parse · the current location shown in the About pane · EN and TR strings for all of it.

**Out (explicitly):** a `PARACHRON_VAULT` environment variable — the app reads no environment variable at runtime today, and a one-corner override scheme is worse than none because it is the kind of thing that gets documented once and then contradicts the UI · more than one vault, or switching between them, which is a different feature with a concept of "current" that has to be visible everywhere the product list is · syncing, mirroring or a second copy kept in step — CORE §3's promise is that the data is rsync-friendly, which is a statement about the format and an invitation to use somebody else's sync tool, not a feature request · moving `config.toml`, which cannot move (see below) · per-product locations, which would make "where is my vault" a question with several answers · symlinking, which is a thing the user can already do to the default path without the app knowing and is not improved by the app knowing · a settings or preferences screen — Chron8 refused one and nothing here changes the argument: this is one entry in a menu that already holds Add, Edit and Language · deleting the old vault when the user declines the move, or any other tidy-up of a directory the app no longer points at · undo.

## Prerequisites

Chron3 complete: `rfd` is in the tree and gains one call, `AsyncFileDialog::pick_folder`, alongside the two `pick_files`/`save_file` calls it already has. Chron5 complete: a new surface reads its colours from `Palette` like every other one. Chron6 complete: every string here exists in both tables from the day it is written, rather than being retrofitted. Chron7 complete: its worker is the shape this milestone's worker copies, including two corrections it has already paid for. **No new dependency** — the move is `std::fs`, the picker is `rfd`, the progress is Slint.

## The pointer cannot live in the thing it points at

This is the constraint that decides the whole shape, and it is worth stating before the file list rather than after it.

`config.toml` holds the theme, the language, the sort mode and the window size. It is the obvious place for a `vault` key and it is the only place, because anything else needs a *second* configuration file to say where the first one is. But it cannot then live inside the vault: the app would have to know the vault's location in order to read the setting that tells it the vault's location.

So the two split, and only one of them moves:

| Path | Holds | Moves |
|---|---|---|
| `<XDG data dir>/config.toml` | theme, language, sort, window size, **`vault`** | never |
| `<vault>/products/` | one folder per product, with its PDFs | yes — this is the point |

`vault` absent, or present and empty, means the vault **is** the XDG data dir. That resolves `products/` to `~/.local/share/parachron/products/`, which is byte-identical to what every existing install already has. There is no migration, no first-run prompt, and no version of this feature that a user who does not want it has to notice.

The cost, stated rather than discovered later: a user who copies their whole `~/.local/share/parachron/` to a new machine carries their settings and their products together, as they do today. A user who *relocates* the vault and then copies only the vault carries their products and not their settings. That is the right trade — settings are small and reproducible, documents are neither — but it means "back up the vault" and "back up everything" stopped being the same sentence, and the About pane showing both paths is what makes that legible.

## Files to add and change

```
src/data.rs           # + Paths::vault; resolution split from creation
src/config.rs         # + vault key; load() learns to fail rather than default
src/relocate.rs       # NEW — the worker that moves a vault, with progress
src/main.rs           # startup order inverts: config before scan
src/about.rs          # + the current location row
src/strings.rs        # + the new keys, EN and TR
ui/app.slint          # + the Document ▾ entry
ui/sheet.slint        # + the confirmation and progress sheet
ui/palette.slint      # (unchanged — the bar reads roles that already exist)
CORE.md               # §3 gains the vault key and the resolution rule
```

`relocate.rs` rather than a function in `data.rs`: it owns a thread, a channel and a progress protocol, which is the same reason `render.rs` and `export.rs` are their own modules rather than functions in the files that call them.

## Tasks

### The pointer

- [ ] `config.rs`: `Config` gains `vault: Option<String>`, and `config.rs`'s existing "no query field" test grows `vault` in the list of keys that *are* settings, so a missing one fails as loudly as an added one
- [ ] `config.rs`: `Config::load` stops being infallible. A file that will not parse is distinguishable from a file with no `vault` key, for the reason in Technical notes
- [ ] `data.rs`: `Paths` gains `vault`, and `products` is derived from it rather than from `data`
- [ ] `data.rs`: split resolution from creation — the default vault is created on first run, a configured one is never created, only checked
- [ ] `main.rs`: invert the startup order so the config loads before the scan, with `local_offset()` still the first statement in `main` (Chron4, and it is a soundness requirement rather than a preference)
- [ ] A path is bytes on Linux and a TOML string is UTF-8: a vault path that is not valid UTF-8 is refused with a message rather than mangled into one that nearly works

### The move

- [ ] `relocate.rs`: a worker on the shape of `export.rs`'s, with the busy flag claimed before the dialog rather than after it — the correction `0445eb3` already paid for once
- [ ] `fs::rename` first; on failure fall back to copy → verify → remove. Crossing a filesystem is the case this milestone exists for, and it is exactly the case `rename` cannot do
- [ ] The invariant, which is what its tests are for: a failure at any point leaves the **original** intact and `config.toml` unchanged. The partial destination is cleaned up; the source is removed only after the copy verifies
- [ ] Refuse the current vault, a folder inside the current vault, and a folder that already holds a `products/` — each with its own message, not one generic error
- [ ] Progress reported **per file, not per chunk**: files done, files total, bytes done, bytes total, and the name of the file being copied
- [ ] Write the new location to `config.toml` only after the move has succeeded

### The UI

- [ ] `Document ▾` gains `Vault location…` / `Kasa Konumu…`, opening `rfd::AsyncFileDialog::pick_folder` through `slint::spawn_local` the way `import::pick` does
- [ ] A confirmation sheet naming the current path, the chosen path, and how many documents and how many megabytes are about to move
- [ ] A progress bar, `N of M`, a byte count and the current file name — the bar drawn from `Palette` and not taken from `std-widgets`, for the reason in Technical notes
- [ ] A failure leaves the sheet open with its reason and the file it stopped on, rather than closing and leaving a notice that is gone when the app closes
- [ ] The About pane gains the current vault path: plain text, copy-to-clipboard, opening nothing — the one gesture that pane has
- [ ] Every string in both tables, and any that shouts stored shouting rather than passed through `to_uppercase` (CORE §4)

### The failures

- [ ] A configured vault that does not exist at startup: the app opens, names the path, creates nothing, and does not fall back to the default
- [ ] A `config.toml` that will not parse: the app opens, names the file, and does not point at the default vault
- [ ] Neither of those two states is silent, and neither is a crash (CORE §3)

## Acceptance criteria

1. With no `vault` key, `products/` resolves exactly where it does today and an existing vault opens unchanged, with nothing asked of the user.
2. Setting `vault` moves where `products/` is read from and leaves `config.toml` where it was.
3. `Document ▾ → Vault location…` opens a folder picker, and the window keeps repainting while it is open.
4. Confirming a move relocates every product folder with its PDFs intact, and the list shows all of them afterwards, in the same order.
5. A move onto a **different filesystem** succeeds — the case `fs::rename` cannot serve, and the one this milestone was asked for.
6. The move is watched rather than waited out: files done of total, a byte count and the current file name are all on screen while it runs.
7. A move interrupted by an error leaves the original vault complete and `config.toml` unchanged, and says which file it stopped on.
8. Choosing the current vault, a folder inside it, or a folder that already holds a `products/` is refused, each with its own message.
9. A configured vault that is not present at startup — an unmounted drive — leaves the app open, names the path on screen, creates nothing anywhere, and does not silently revert to the default.
10. A `config.toml` that will not parse leaves the app open and names the file, and does not point the app at the default vault.
11. The About pane shows where the vault currently is; the path can be copied and nothing about it opens anything.
12. Every string added by this milestone exists in English and Turkish, and the window is fully labelled in both.
13. `cargo test` is green with no warnings from `cargo build` either, and `cargo fmt --check` is clean.

## Technical notes

**A configured vault is never created, and this is the rule the whole feature turns on.** If `vault = "/mnt/ironwolf/parachron"` and the ironwolf is not mounted, `/mnt/ironwolf` is an ordinary empty directory on the root filesystem. `create_dir_all` would succeed against it without complaint, Parachron would create a vault there, and its owner would file invoices onto the system disk believing they were on the drive they bought for exactly this. Then the drive gets mounted and the whole lot disappears underneath the mount point — still on disk, entirely invisible, and impossible to explain to somebody who did nothing wrong.

So creation and resolution are different operations with different rules. The **default** vault is created on first run: its parent is `~/.local/share`, which exists on any machine that has a home directory, and creating it is what Parachron has always done. A **configured** vault is only ever checked. Missing means missing — the path goes on screen, nothing is created, and the app does not fall back to the default, because a silent fall back is indistinguishable from total data loss to the person looking at the window.

**A `config.toml` that will not parse used to cost a theme. It could now cost a vault.** `Config::load` is infallible today: anything it cannot read degrades to `Config::default()`, the app opens on Default Dark, and the user notices and shrugs. Add a `vault` key and that same fallback yields `vault: None` — the app points at the *default* vault, finds it empty or finds an old one, and shows that. The real vault is on the other disk, and nothing on screen mentions it, because as far as the app is concerned the user never configured one.

This is the same failure as the unmounted drive and it is not covered by the same rule, because the vault is not missing — the **pointer** is. So `load` has to distinguish "this file has no `vault` key", which is the ordinary case and means the default, from "this file did not parse", which means the app does not know what it was told and must say so rather than guess. The second becomes a broken state naming the file, in the shape CORE §3 already requires for a manifest that will not parse. That a product with a bad `product.toml` has been handled this way since Chron1 while the app's own config was not is worth noticing; this milestone is where the asymmetry starts to matter.

**Move, and never a bare rename.** `fs::rename` across a filesystem boundary fails with `EXDEV`, and a different filesystem is precisely what "put it on my other drive" means — so the fast path is the one that will almost never be taken for the use case this was asked for. It is still worth having: relocating within one disk is instant and atomic, and an atomic move is strictly better than a copy when it is available.

The fallback is copy → verify → remove, in that order, and the order is the whole safety argument. Copying first means a failure at any point has damaged nothing: the source is untouched, the partial destination is removed, and `config.toml` still names the old location, so the next launch opens the vault that still exists. Removing the source only after the copy verifies means there is no window in which the documents exist in neither place. The tests are written against that invariant rather than against the happy path, because the happy path is the one that will get exercised by hand and the failure path is the one that will not.

**Three destinations a folder picker makes reachable in one click, and each has to be refused separately.** Choosing the current vault is a no-op that would still run a whole copy. Choosing a folder *inside* the current vault means copy-then-delete-source deletes what was just written — the destination is under the source, so removing the source removes the copy. Choosing a folder that already holds a `products/` merges two vaults silently, with name collisions resolved by whichever file was written last. None of these is exotic; a file dialog opens somewhere and a person clicks. Each gets its own message, because "that location cannot be used" tells somebody nothing about which of three quite different mistakes they made.

**The progress bar is drawn from `Palette`, and this is the third time this has come up.** Chron5 shipped believing every colour in the app lived behind the `Palette` global, and it did not: the sheet backdrop was a literal, the page edge was painted over, and the zoom slider came from `std-widgets` and therefore read the Slint style rather than the theme. CORE §10 still carries the `ListView` scrollbar as the surviving instance of the same thing. A `std-widgets` `ProgressIndicator` would make it three, on a surface that is new in this milestone and has no history to excuse it. The bar is two rectangles and a fraction.

**The startup order inverts, and Chron4's requirement survives it intact.** Today `main` resolves the vault and scans it, then loads the config. It cannot any more: the vault's location comes *out* of the config, so the config has to load first. The new order is `local_offset()`, resolve the XDG directory, load the config, apply the vault override, then ensure and scan.

`local_offset()` stays the first statement in `main`. Chron4 put it there because `time` will not work out a local UTC offset once a process has more than one thread and the render worker starts with the window; that reasoning is unchanged and the new order does not disturb it, because everything inserted before the scan is single-threaded file I/O. Worth stating explicitly because "load the config earlier" reads like a free reordering, and there is exactly one line in `main` that is not free to move.

**A path is bytes and a TOML string is UTF-8.** On Linux a filename is a sequence of bytes with no encoding guarantee, so a folder picker can legitimately return a `PathBuf` that is not valid UTF-8 — and TOML has no way to write it down. Chron7 met the same wall from the other side, which is why the export writes through `write_to` rather than `save`. Here the answer is different because the destination has to be *persisted* rather than just used: the path is refused, with a message saying why, rather than lossily converted into a similar-looking path that would then be wrong every time it was read back. Rare, and the kind of thing that is much cheaper to refuse deliberately than to discover.

**What the About pane gains, and what it deliberately does not.** CORE §3 promises data that is human-readable and rsync-friendly, with no hidden state. A location the user chose through a dialog and cannot read back afterwards is hidden state — they would have to open `config.toml` to answer "where are my documents". So the pane shows it, next to the version and the source URL, as plain text with copy-to-clipboard.

It does not get a button that opens the folder. CORE §4's no-external-opens rule is written about addresses, and a filesystem path is not a URL, so this is not the rule forbidding it — it is that the pane has exactly one gesture, copy, applied to every address in it, and a folder-opening button would be the first exception in a surface whose whole argument is that it has none. The clipboard is enough to paste into a file manager.

## How the criteria were verified

Written when the milestone is done, in the manner of Chron1–8, with Chron8's **Not verified** list as the model for whatever this one cannot check.

Two of these criteria are worth flagging now as likely to need care rather than a line. Criterion 5 needs a genuinely different filesystem rather than a second temporary directory, or it verifies the `rename` path twice and the copy path never — `/dev/shm` is mounted and writable and is the cheapest real one available here. And criterion 9 needs a configured path whose parent exists but whose drive does not, which is a state that has to be constructed deliberately; the failure it guards against is invisible when it happens, so a test that merely fails to find a vault is not the same test.

## Done when

All acceptance criteria pass on the laptop. Then: amend CORE §3 with the `vault` key, the split between what moves and what does not, and the rule that a configured vault is checked rather than created; note in CORE §4 that `Document ▾` has a fourth entry and that the About pane shows the location; record in CORE §9 that this milestone and Chron10 swapped places, which its table already carries; hand Chron10 the fact that "where the vault lives" now has a user-supplied answer as well as a per-platform one, so the README's data-directory section and CORE §3's Windows row both have to describe two things rather than one; mark this file's status `done`, and move on to Chron10.
