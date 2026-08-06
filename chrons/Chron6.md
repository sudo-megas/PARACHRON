# Chron6 — Localization

**Milestone:** 6 of ~9 (CORE §9)
**Status:** done
**Builds against:** CORE §4 (localization, `Document ▾`, app-wide principles), §3 (`lang` in `config.toml`), §8 (conventions & development rules)

## Goal

Both languages become real. `Document ▾` gains English and Türkçe, choosing one relabels the entire window without a restart, and the choice survives into the next session. Every one of the string table's keys is reviewed in Turkish rather than merely present.

Like Chron4, this is a small milestone because an earlier one built the seam. Chron1 put every string behind the table and wrote `apply_strings` with "called again whenever the language changes (Chron6)" over it. Chron3 turned `lang` from a value captured into seven closures into state on each of the three owners, and said in the code why: Slint permits one handler per callback, and registering one from inside that callback's own handler is a panic rather than a no-op. Chron6 is where that groundwork gets used.

## Scope

**In:** the language switch in `Document ▾` · `set_lang` on each of the four owners and the one function that drives them · re-pushing everything Rust composed rather than merely refilling the `Strings` global · the Turkish review, key by key · persistence through `config.toml` · new keys for the menu itself.

**Out (explicitly):** a third language — the table is a pair and CORE §4 says two · reading the system locale, which CORE §4 forbids outright · translating what is on disk: folder names, file names, tab labels derived from file stems, and the diagnostic tails of OS and TOML parser messages are not UI copy and stay as they are · right-to-left layout, which neither language needs · EXPORT (Chron7), whose own strings are written there and are Turkish-complete when they land · About (Chron8) · packaging (Chron9) · locale-aware collation for the alphabetical sort, which `vault.rs` already declines with its reasons.

## Prerequisites

Chron5 complete: the theme picker's eleven rows are labelled through the string table, so the switch has to relabel them and the two milestones are tested together. Nothing new to install.

## What actually goes stale

`apply_strings` refills the `Strings` global, and the `.slint` files bound to it follow immediately. That is about half the text on screen. The other half was composed in Rust, pushed into ordinary window properties, and does not know the language changed:

| Where | What |
|---|---|
| `vault::row` | the `!` warning prefix, `Missing files: …`, `⚠`, `Broken entry: <folder>` |
| `vault::describe` | every `DataError` — the reason under a broken folder |
| `details::countdown` | `658 days` / `658 gün`, and `Expired` |
| `viewer::describe` | every `ViewError` — the message in place of a page |
| `theme.rs` | the eleven picker rows, four of which translate |

All five are derived from the selection, which means all five are recomputed by the path the vault already owns: `plan(true)` produces a fresh `Update` from the entries, and `push` hands the rows, the details snapshot and the viewer's state over in one pass. So the switch is `apply_strings`, then four `set_lang` calls, then one re-push — not five separate refresh routines that could disagree about what is on screen.

`keep_view: true` is the right argument for that re-push, for the reason Chron3 gave it: changing language is not changing product. Whoever was reading page seven of an invoice is still reading it.

**The form cannot go stale, and that is worth writing down rather than leaving as a gap.** Its heading and its four per-field messages are composed in Rust too, and nothing re-pushes them. It does not need to: the sheet's backdrop fills the window and swallows every click that misses the card, so `Document ▾` is unreachable while the form is up, and the form is always rebuilt from scratch by `open()`. The same holds for Chron5's picker for the same reason. This is a property of the sheet recipe, so if a later milestone ever makes a sheet dismissable by clicking away, or puts a menu above one, this paragraph is the thing that stops being true.

## Files to add and change

```
src/
├── lang.rs           # NEW — the switch: who is told, in what order
├── main.rs           # + install the switch; persist() reads every owner
├── vault.rs          # + set_lang
├── viewer.rs         # + set_lang
├── editor.rs         # + set_lang; install returns a handle so the switch can reach it
├── theme.rs          # + set_lang
└── strings.rs        # + menu keys; the Turkish review
ui/
├── app.slint         # + the two language rows in `Document ▾`; MenuRow gains `active`
└── strings.slint     # + menu keys
```

## Tasks

- [x] `strings.rs`: `MenuLanguage`, `LangEnglish`, `LangTurkish` — language names given in their own language, identical in both tables
- [x] `strings.rs`: walk all keys in Turkish; fix what is wrong, and comment anything that looks wrong and is not
- [x] `vault.rs`: `set_lang`, and a `relabel` that re-plans with `keep_view: true`
- [x] `viewer.rs`: `set_lang` on `State`
- [x] `editor.rs`: `set_lang`; `install` returns a handle instead of nothing
- [x] `theme.rs`: `set_lang`, and re-push the picker's rows
- [x] `lang.rs`: `install` — register `on_language_selected`, ignore a switch to the language already in effect, then `apply_strings` → four `set_lang` → one re-push
- [x] `app.slint`: `Document ▾` gains a Language section with English and Türkçe, the active one marked
- [x] `main.rs`: `persist` writes the session's language rather than the loaded one
- [x] `main.rs`: `apply_strings` gains every key the table has grown since Chron4 and stays exhaustive
- [x] A test that every key differs between the languages except where it must not, with the exceptions named

## Acceptance criteria

1. `Document ▾` lists English and Türkçe with the language in effect marked, and each is written in its own language in both tables.
2. Choosing Türkçe relabels every visible string at once: title bar, menu, sort chips, tab row, control row, serial strip, details column, About strip and the theme picker's two translatable rows — with no restart.
3. A broken folder's reason, a product's `Missing files` warning, the days-left counter and a document that will not open all follow the switch, without the product being reselected.
4. Switching while a product is open keeps that product selected, keeps its row visible, and leaves the open page and zoom untouched.
5. Switching to the language already in effect changes nothing and does not re-request the page being read.
6. The chosen language is still in effect after quitting and reopening the app.
7. A `config.toml` naming an unknown language falls back to English, and is rewritten as `en` on exit.
8. `1 day` is not `1 days`, and Turkish reads `1 gün` and `658 gün` — no plural after a numeral.
9. Every key has a non-empty string in both languages, and the keys that are deliberately identical across the two are listed and justified.
10. Folder names, file names and tab labels are unchanged by the switch — they are what is on disk, not UI copy.
11. `grep -rn` for user-visible literals in `.slint`/`.rs` still finds none outside `strings.rs`.
12. `git log` shows only `sudo-megas` as author and no AI attribution anywhere.

## Technical notes

**Why the language is copied four times instead of shared once.** The tidy answer is one `Rc<Cell<Lang>>` cloned into each owner, so there is no second copy to go stale. It does not compile. `viewer::State` lives behind an `Arc<Mutex<State>>` that is captured into the render worker's response sink, and `Renderer::spawn` requires that closure to be `Send`; an `Rc` inside `State` makes `State` not `Send` and the bound fails. The language is only ever read on the UI thread, so the sharing would be sound — the bound cannot know that. So each owner keeps a plain `Lang` and gains a setter, one function calls all four, and the risk of a forgotten copy is answered by there being exactly one caller rather than by the type system. The four setters existing at all is Chron3's doing; it made `lang` mutable state in `Vault`, `viewer::State` and `Editor` for this milestone specifically.

**`install` is never called twice.** The alternative to setters is re-registering the callbacks with a new `lang` captured, which looks like it would work and is a panic: Slint holds one handler per callback, and setting a callback from inside that same callback's handler — which is where a language switch necessarily runs — is not a silent replacement. `viewer.rs` says so in a comment at the point the temptation arises. `editor::install` currently returns nothing, which is the one signature this milestone has to widen; it returns a handle so the switch can reach the editor's `lang` without going back through the window.

**Language names are written in their own language.** Both tables give `English` and `Türkçe`, identically. A user who has landed in a language they cannot read needs to find their own name in the list, and `İngilizce` is no help to somebody who reads only English. This is the same rule the glyph keys follow — a key whose two sides are equal is not an unfinished translation — and it goes in a comment beside the entries so the exhaustiveness test's "these are deliberately identical" list has something to point at.

**Switching to the current language is a no-op, deliberately.** The re-push runs `plan`, which bumps the viewer's generation token and issues a fresh render request. That is correct when something changed and is pure waste when nothing did — a page that is already on screen is torn down and asked for again, and on a large invoice that is a visible blink for no reason. The switch returns early instead.

**Turkish uppercase is a real trap here, and the table already walks into it once.** `EXPORT` is `DIŞA AKTAR`: dotless `I` in `DIŞA`, because Turkish uppercases `ı` to `I` and `i` to `İ`. Anything that upper-cases a Turkish string in code rather than storing it upper-cased will get this wrong, which is why the two shouting labels — `THEME`/`TEMA` and `EXPORT`/`DIŞA AKTAR` — are stored as they appear and never passed through `to_uppercase`. `data::fold` already handles the mirror-image problem for folder names, and its comment explains the combining-dot trap in the other direction.

**What the review is for, given that nothing is blank.** All of Chron1 through Chron5's keys already carry Turkish, so this is not filling gaps — it is reading them as a Turkish speaker would. The things worth looking for: a label translated as a noun where the UI needs an imperative, `Belge` used for both "document" and "file" where the distinction matters, error messages that read as accusations rather than descriptions, and anything long enough to elide in a 250px column that has a shorter honest form. Where a translation looks like an oversight and is not, it gets a comment — `DayUnit` and `DaysUnit` both being `gün` already has one, and it is the pattern.

**Persisting the language, the third time this bug appears.** `persist` spreads `..settings`, so before Chron4 the sort mode was carried through from load, before Chron5 the theme was, and here the language is: `main` passes the `lang` it computed at startup. With the language now mutable, that value is stale by the time the window closes. Same fix, one field over, and the same policy: `config.toml` is written once, at shutdown, not on every switch.

## How the criteria were verified

110 tests pass (`cargo test`), up from Chron5's 106, with no warnings from either `cargo test` **or** `cargo build`. The distinction matters and cost a moment here: two helpers only the tests use warn in the ordinary build and not in the test build, so checking one is not checking the other.

**Automated.** The switch's own module tests that both languages round-trip through the menu index, that an index out of range lands on English the way an unknown `lang` code does, and that each language's name reads identically in both tables. `strings.rs` gained two tests that are the real content work of this milestone. The first walks every key and asserts the two languages **differ**, against an explicit list of the nineteen that are deliberately identical — the wordmark, eight glyphs, `PDF`, `A–Z`, the seven proper-noun theme names, and the two language names. Without that list, "the Turkish is missing" and "the Turkish is a proper noun" look the same from outside, which is how a table ends up half-finished with nothing to show for it. The second pins the Turkish uppercase trap: the two shouting labels are asserted to equal their own `to_uppercase`, and `DIŞA AKTAR` is asserted to contain no dotted `İ` — because Turkish maps `i` to `İ` and `ı` to `I`, so upper-casing that string in code would get it wrong in a way English never is.

**Headless, through the real element tree.** `ui_tests.rs` now installs the owners `main` installs, in `main`'s order, and drives the switch by clicking. It selects the product whose file is missing, records what every Rust-composed string reads, opens `Document ▾`, clicks Türkçe, and asserts: the session's language changed, the tick moved, the bound strings followed, `Missing files` became `Eksik dosyalar`, the countdown's unit became `gün` **while the number in front of it stayed the same** — that last one is what would catch a switch that recomputed the date instead of just re-rendering it. Then that the selection, the row and the open page are untouched; that a broken folder's `DataError` and an expired warranty both follow; that the picker's `Default Dark` row became `Varsayılan Koyu` while `Catppuccin Mocha` stayed itself; that switching to Turkish again changes nothing; and that the selected product's folder is still `drive`, because a folder is an identity and not a label.

**One thing the headless test taught about the menu.** Selecting a product has to happen with the menu closed. An open menu lays a full-window `TouchArea` over everything to catch the dismissing click, so a row click while it is up dismisses the menu rather than selecting anything. The first version of the test opened the menu first and got an empty `selected-name`, which looked like the vault failing to push and was the menu working exactly as designed.

**By real clicks, on an isolated display, against a scratch vault** seeded with a live warranty, an expired one, a product whose file is not on disk, and a folder with no manifest — so every string that has to follow the switch is on screen at once. Confirmed against screenshots:

| Action | Result |
|---|---|
| `Document ▾` in English | `Add Document`, `Edit Document…`, a rule, `Language`, then `English` ticked and `Türkçe` |
| Click `Türkçe` | Title bar `Belge` / `Belge Ekle`; chips `A–Z` / `Tarih`; column 3 `Satın alma bağlantısı`, `Satın alma tarihi`, `Garanti başlangıcı`, `Garanti bitişi`, `Kalan garanti`; `TEMA` / `DIŞA AKTAR`; `Seri numarası`; `Yakınlaştırma`; `Hakkında` |
| The same product, unchanged underneath | The viewer's `This file is not in the product folder` became `Bu dosya ürün klasöründe yok` — a `ViewError` composed in Rust, following the switch without the product being reselected |
| The countdown | `26518 gün` — no plural after the numeral, and the number identical to what English showed |
| The list | `! QD-OLED Monitor` keeps its warning prefix; `test-broken` keeps its folder name |
| The tab | Still reads `Invoice`, because that is a file stem on disk and not UI copy (criterion 10, visible) |
| `Document ▾` in Turkish | `Dil` as the heading, `English` and `Türkçe` both still in their own language, `Türkçe` ticked |
| Back to `English` | Every one of the above returns to what it started as |

`Yakınlaştırma` is four times the length of `Zoom` and was the label most likely to break a layout; it fits the control row at the 1000px floor with the slider intact, which the screenshot shows.

**What the Turkish review changed.** Nothing was blank — all of Chron1 through Chron5's keys already carried Turkish — so this was reading them rather than filling them. One wording fix: `Checking…` was `Denetleniyor…`, which means "being audited", the register of an inspection rather than of a program looking at a file; it is `Kontrol ediliyor…`. One near-duplicate documented rather than collapsed: `ErrDateInvalid` and `ErrInvalidDate` are the same words in Turkish and nearly the same in English, and they stay two keys because one is a form refusing what was typed and the other is a manifest field being reported with its name and value appended. Everything else read correctly, including the two entries that look like oversights and are not — `DayUnit` and `DaysUnit` are both `gün`, and both language names are the same in both tables.

**Not verified end to end: criteria 6 and 7,** for exactly the reason Chron5's criterion 3 was not. `xdotool windowclose` does not make the app exit under `Xvfb`, so `persist` never runs there and `config.toml` is never rewritten. Every link is tested separately: the headless test asserts a click updates the cell `lang::install` returns; `main.rs`'s test asserts `persist` writes the session's values over a file holding different ones and normalises an unrecognised `lang` to `en`; `config.rs` asserts the value reloads; `strings.rs` asserts an unknown code falls back to English. The gap is the same three lines in `main` that read the owners into `Session`.

## Done when

All acceptance criteria pass on the laptop. Then: confirm CORE §4's localization paragraph still describes what shipped, mark this file's status `done`, and move on to Chron7.
