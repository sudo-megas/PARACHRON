# Chron4 — Details column

**Milestone:** 4 of ~9 (CORE §9)
**Status:** planned
**Builds against:** CORE §3 (data model — the fields column 3 reads), §4 (layout, column 3, sort toggles, app-wide principles), §8 (conventions & development rules), §10 (open items)

## Goal

Column 3 stops being a placeholder. The selected product's purchase link, purchase date, warranty start and warranty end appear, and under them the number the app exists for: how many days of warranty are left, bold and the largest text in the column. The link copies to the clipboard on click, the same gesture as the serial strip, and never opens a browser. Column 1 gains the two sort toggles CORE §4 describes.

This is a small milestone by design, because Chron3 built the machinery. `Product` has carried every field this column needs since Chron1, annotated as dead code while it waited. `fmt_date` was written and tested in Chron1 for exactly this caller. `SortMode` and its comparators shipped in Chron3 with the vault that owns them. Chron4 is where all of it becomes visible.

## Scope

**In:** column 3 as a real component · purchase link with copy-to-clipboard · purchase date, warranty start, warranty end · days-left counter with its expired state · the two sort toggles and their persistence · the details view for a broken or unselected product · new string keys.

**Out (explicitly):** THEME (Chron5) and EXPORT (Chron7), which stay stubs in the button row · themes beyond the Chron1 palette (Chron5) · Turkish completeness (Chron6) · About (Chron8) · packaging (Chron9) · a third sort mode beyond CORE §4's two toggles · editing anything from column 3 (the form owns editing) · notifications or reminders when a warranty is close to expiring.

## Prerequisites

Chron3 complete. Specifically, the details column depends on `vault.rs` owning selection by folder, and the sort toggles depend on `SortMode`, its comparators and the `config.sort` plumbing, all of which shipped in Chron3.

## Files to add and change

```
Cargo.toml            # time gains "local-offset"
src/
├── details.rs        # NEW — the column-3 snapshot and the copy-link callback
├── vault.rs          # + on_sort_toggled
├── data.rs           # + days_left; the two #[allow(dead_code)] come off
├── main.rs           # + capture the UTC offset first thing; persist() takes the sort
└── strings.rs        # + details and sort keys
ui/
├── details.slint     # NEW — column 3
├── app.slint         # + sort chips above the list; column 3 hosts Details
└── strings.slint     # + details and sort properties
```

## Tasks

- [ ] `main.rs`: read the local UTC offset at the very top of `main`, before any thread exists, and keep it for the life of the app
- [ ] `data.rs`: `days_left(warranty_end, today)` clamped at zero; drop the `#[allow(dead_code)]` on `Product` and on `fmt_date`, which now have callers
- [ ] `details.rs`: build the column's snapshot from the selected `Entry` — dates through `fmt_date`, days-left composed through the string table
- [ ] `details.slint`: the component, following the `Viewer` boundary — data in as properties, intent out as callbacks, no text of its own
- [ ] `details.slint`: the days-left counter as the column's visual anchor, in the error colour once it reaches zero
- [ ] `details.rs`: `on_copy_link`, mirroring `on_copy_serial` — clipboard, silent failure, a `link-copied` flag and its own single-shot timer
- [ ] `app.slint`: column 3 hosts `Details`; THEME and EXPORT stay as they are
- [ ] Broken entry, no selection, and a product with an empty link each render something readable rather than an empty panel
- [ ] `app.slint`: two sort chips in a strip above the product list, the active one marked
- [ ] `vault.rs`: `on_sort_toggled` — clicking the active chip returns to insertion order; selection and the open page survive the re-sort
- [ ] `main.rs`: `persist` writes the session's sort mode instead of carrying the loaded one through
- [ ] `strings.slint` / `strings.rs`: every new label, unit and chip through the table

## Acceptance criteria

1. Selecting a product fills column 3 with its link, purchase date, warranty start and warranty end, all dates reading `DD-MM-YYYY`.
2. The days-left counter equals `warranty_end - today` and is the largest, boldest text in the column.
3. A warranty that ended reads as expired rather than as a negative number, in the error colour.
4. Clicking the link copies it to the system clipboard and shows the same brief confirmation the serial strip uses. No browser opens, ever.
5. A product with no link, a broken folder, and no selection at all each render a readable column rather than a blank one.
6. The `A–Z` chip sorts the list alphabetically; the date chip sorts by purchase date, oldest first; clicking the active chip returns to insertion order.
7. Broken entries stay at the end of the list under every sort mode.
8. Toggling a sort while a product is open keeps that product selected, keeps its row visible, and leaves the open page and zoom untouched.
9. The chosen sort mode is still in effect after quitting and reopening the app.
10. `grep -rn` for user-visible literals in `.slint`/`.rs` still finds none outside `strings.rs`.
11. `git log` shows only `sudo-megas` as author and no AI attribution anywhere.

## Technical notes

**Getting today's date right is the only hard part of this milestone.** The `time` crate refuses to determine the local UTC offset in a process that has more than one thread, on Unix, because the underlying C call is not safe to make once other threads exist. Parachron always has a second thread — the render worker starts with the window. So the offset is read at the very top of `main`, before the window is created and before anything is spawned, and kept. Every later "what is today" is `now_utc().to_offset(offset).date()`, which needs no further system call and is safe from any thread.

Recomputing on every push rather than caching a `Date` at startup is deliberate: an app left open overnight should show the right number in the morning. If the offset cannot be read at all, the app falls back to UTC and is at worst a day out at the edges of a timezone, which is the right failure for a counter measured in months. There is a test for the fallback, because an untested fallback is a guess.

**Days-left is composed in Rust, not in Slint.** The string table holds no interpolation, by design — the unit words and the expired message are keys, and Rust formats the number in front of the unit, exactly as the page counter does in the viewer. Turkish takes no plural agreement after a numeral, so the singular and plural keys carry the same Turkish word; that is not a mistake in the table and is worth a comment so nobody "fixes" it.

**The link is text.** CORE §4's app-wide principle is that Parachron never opens an external address. The link renders as plain text with a copy affordance and the same confirmation the serial strip uses, driven the same way: a boolean pushed from Rust and a single-shot timer that clears it. Sharing the mechanism means the two confirmations cannot drift apart, and clipboard failure stays a silent no-op — the text is on screen either way.

**Warranty end is shown, which CORE §4's wireframe did not.** The wireframe lists link, purchase date, warranty start and the counter. Hiding the date the countdown is counting toward makes the column harder to trust: a user checking "is this right?" has nothing to check it against. CORE §4 is amended to include it rather than the column quietly disagreeing with the specification — CORE's own rule is that when reality and CORE differ, CORE is updated first.

**Two toggles, not three.** CORE §4 says two sort toggles over a default of insertion order, and that is what this is: two chips, either of which can be turned off by clicking it again to return to `added`. A three-way segmented control including an explicit "Added" chip would be clearer in isolation and would also be a different specification, so it is not what got built. Broken entries sink to the end under every mode, tie-broken by folder, which keeps the order stable and keeps unreadable folders out of the middle of an alphabetical list.

**The sort is not saved when it changes.** `config.toml` is written once, at shutdown, and this milestone keeps it that way — `persist` simply takes the session's sort mode rather than the one that was loaded. Writing the file on every toggle would mean touching the disk for a preference whose worst-case loss is one click after a crash.

**Column 3 has no room to spare.** It is a quarter of a window whose floor is 1000×700, it clips its contents, and it has no scroll container. The layout is built to fit at the floor rather than to scroll, which means long links elide rather than wrap and the counter keeps its size whatever is above it.

## How the criteria were verified

*(filled in when the milestone is done)*

## Done when

All acceptance criteria pass on the laptop. Then: amend CORE §4 to show warranty end in column 3, close CORE §10's remaining details-column questions if any are left open, mark this file's status `done`, and ask user permission to start Chron5.
