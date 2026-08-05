# Chron6 — Localization

**Milestone:** 6 of ~9 (CORE §9)
**Status:** planned
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
| `theme.rs` | the eleven picker rows, two of which translate |

All five are derived from the selection, which means all five are recomputed by the path the vault already owns: `plan(true)` produces a fresh `Update` from the entries, and `push` hands the rows, the details snapshot and the viewer's state over in one pass. So the switch is `apply_strings`, then four `set_lang` calls, then one re-push — not five separate refresh routines that could disagree about what is on screen.

`keep_view: true` is the right argument for that re-push, for the reason Chron3 gave it: changing language is not changing product. Whoever was reading page seven of an invoice is still reading it.

**The form cannot go stale, and that is worth writing down rather than leaving as a gap.** Its heading and its four per-field messages are composed in Rust too, and nothing re-pushes them. It does not need to: the sheet's backdrop fills the window and swallows every click that misses the card, so `Document ▾` is unreachable while the form is up, and the form is always rebuilt from scratch by `open()`. The same holds for Chron5's picker for the same reason. This is a property of the sheet recipe, so if a later milestone ever makes a sheet dismissable by clicking away, or puts a menu above one, this paragraph is the thing that stops being true.

## Files to add and change

```
src/
├── lang.rs           # NEW — the switch: who is told, in what order
├── main.rs           # + install the switch; persist() takes the session's language
├── vault.rs          # + set_lang
├── viewer.rs         # + set_lang
├── editor.rs         # + set_lang; install returns a handle so the switch can reach it
├── theme.rs          # + set_lang
└── strings.rs        # + menu keys; the Turkish review
ui/
├── app.slint         # + the two language rows in `Document ▾`, the active one marked
└── strings.slint     # + menu keys
```

## Tasks

- [ ] `strings.rs`: `MenuLanguage`, `LangEnglish`, `LangTurkish` — language names given in their own language, identical in both tables
- [ ] `strings.rs`: walk all keys in Turkish; fix what is wrong, and comment anything that looks wrong and is not
- [ ] `vault.rs`: `set_lang`, and a `relabel` that re-plans with `keep_view: true`
- [ ] `viewer.rs`: `set_lang` on `State`
- [ ] `editor.rs`: `set_lang`; `install` returns a handle instead of nothing
- [ ] `theme.rs`: `set_lang`, and re-push the picker's rows
- [ ] `lang.rs`: `install` — register `on_language_selected`, ignore a switch to the language already in effect, then `apply_strings` → four `set_lang` → one re-push
- [ ] `app.slint`: `Document ▾` gains a Language section with English and Türkçe, the active one marked
- [ ] `main.rs`: `persist` writes the session's language rather than the loaded one
- [ ] `main.rs`: `apply_strings` gains every key the table has grown since Chron4 and stays exhaustive
- [ ] A test that every key differs between the languages except where it must not, with the exceptions named

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

*(Filled in when the milestone lands.)*

## Done when

All acceptance criteria pass on the laptop. Then: confirm CORE §4's localization paragraph still describes what shipped, mark this file's status `done`, and move on to Chron7.
