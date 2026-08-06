# Chron11 — Packaging and CI

**Milestone:** 11 of ~11 (CORE §9)
**Status:** in progress — every file this milestone owes exists and everything checkable on this laptop is checked; what remains is not writing but *observing*, and it needs CI and a person. See **How the criteria were verified**.
**Builds against:** CORE §1 (identity — app id, binary name, icons, licence, repo), §2 (stack — every dependency has to exist on three platforms), §3 (data model — where the vault lives on a target that is not Linux, and where it lives once the user has chosen), §7 (packaging & CI, in full), §8 (conventions & development rules, including who is allowed to be the author of a release), §9 (roadmap — the README lands here)

**Renumbered during Chron9:** this file was Chron9 until the roadmap's last two rows swapped places. A release is the one step that hands artefacts to people who did not build them, and the milestone that lets a user choose which disk the vault lives on changes where their documents are — that cannot land *after* a version has shipped expecting them somewhere else. So the save-location milestone took the 9 slot and packaging moved to 10. Releases are the last step. CORE §9's table records the move; the eight earlier files that say "packaging (Chron9)" in their **Out** lists are left as written, because they were true when written and this project annotates rather than rewrites.

**AUR is out, and this is a withdrawal rather than a decision.** The Arch User Repository is temporarily disabled by its own maintainers following attacks on it. Everything below that named it is struck in place with that reason rather than deleted, because a reader a year from now needs to know the AUR was planned and why it was not shipped — and because if the AUR comes back, what is struck here is the design to un-strike. It costs one of the three human prerequisites, one acceptance criterion, and the only part of this milestone that was going to make a git commit outside this repository.

## Goal

Parachron becomes something a person installs rather than something a person builds. A tag pushed to the repository produces three assets — `.pkg.tar.zst`, `.deb`, `.exe` — built by GitHub Actions, each carrying the binary, the icons, the licence and, on Linux, a desktop entry that puts the app in a menu. ~~An AUR package makes `paru -S parachron` work.~~ And `README.md` is finally written, for the person who landed on the GitHub page and wants the app, not the history.

**Overtaken before this milestone began:** `README.md` was written in commit `9768a07`, ahead of the milestone that was going to write it, and CORE §9's row records why — the page is what a visitor lands on, and having it ready means this milestone cuts a tag rather than also writing a page. So the last sentence of that paragraph describes work already done. What is left of it here is real but smaller: the screenshots the walkthrough has never had, a Windows line for the data-directory section, the AUR route coming back out, and the check that every link resolves and every badge reads a real value once a release exists.

This is the only milestone whose output is not code the app runs, and the only one whose acceptance criteria are largely unverifiable on this laptop. Both facts shape everything below.

## Scope

**In:** a dependency split so that two Linux-only feature sets are not asked for on a target that cannot use them · `packaging/org.parachron.Parachron.desktop` · the icon install map · `[package.metadata.deb]` · a `PKGBUILD` · Windows resources (icon and manifest) through `build.rs` · `[profile.release]` · a CI workflow on push and a release workflow on tag · MuPDF built statically or bundled per target · AGPL compliance in every artefact · ~~the AUR package~~ · `README.md`'s remaining work per `usereadme.md`.

**Out (explicitly):** a Windows installer — the release asset is the executable CORE §7 names, and an MSI or NSIS wrapper is a second packaging system for one of three targets · code signing on Windows, which needs a certificate somebody has to buy and renew, and an unsigned binary with a public build log is the more honest artefact for an AGPL app · Flatpak, Snap and AppImage, none of which CORE §7 lists · macOS, for the same reason · auto-update, which is a network call in an app that makes none (CORE §4) · publishing to crates.io, since this is an application and a vendored MuPDF makes it a hostile dependency for anyone who did pull it in · a changelog, which CORE §8 rule 3 rules out of the README and the repository already keeps · translating the README, which is a document for a GitHub page rather than UI copy.

## The human actions this milestone needs, which no tooling can perform

Named up front because three of them block acceptance and none of them is a task anybody can tick on the user's behalf.

1. **The GitHub repository must exist at `https://github.com/sudo-megas/PARACHRON` with Actions enabled.** CORE §1 names it and `Cargo.toml` points at it; nothing in this tree proves it is there.
2. ~~**An AUR account with an SSH public key registered, and that private key added to the repository as a secret.** AUR publication is a `git push` to `ssh://aur@aur.archlinux.org/parachron.git`, and it is a *commit* — which puts it squarely under CORE §8 rule 2, so it must be authored by `sudo-megas` and by nothing else.~~ — struck: the AUR is temporarily disabled by its maintainers. This was the only one of the three that needed a credential, and it is the one that went away.
3. **A tag has to be pushed by a person.** The release workflow triggers on it; nothing triggers the person.

Everything else in this file is work that can be written and reviewed before any of the three happen.

## The spike, and why this one runs before the notes are finished

Chron3 established that a hard question gets spiked rather than assumed, and Chron7 showed what it costs to skip one — a feature that produced a valid file with words silently missing. This milestone's hard question is not about MuPDF, and the first draft of this file got the shape of it wrong in a way worth leaving on the record.

**The alarm that turned out to be a false one, and how it was settled.** `rfd` is configured `default-features = false, features = ["xdg-portal"]` and `arboard` `default-features = false, features = ["wayland-data-control"]`. Both feature sets name a Linux mechanism, and the obvious reading is that a Windows build has default features off, one Linux backend requested, and therefore **no** file dialog and **no** clipboard — which would make `Add Document`, `EXPORT`, the serial strip, the purchase link and both About URLs dead on the target CORE §7 says CI owns. That reading was written into this file as a defect before anyone checked it, which is exactly the move Chron7 was written to discourage.

It is wrong, and settling it needed no Windows machine and about ten seconds:

```
cargo tree --target x86_64-pc-windows-msvc -e features -p rfd
cargo tree --target x86_64-pc-windows-msvc -e features -p arboard
```

`rfd` resolves on that target with `windows-sys` and `Win32_UI_Shell`, `Win32_UI_Shell_Common`, `Win32_System_Com` — the Win32 dialog. `arboard` resolves with `clipboard-win` and `Win32_System_DataExchange` — the Windows clipboard. **The platform backends are gated on `cfg(target_os = …)`, not on cargo features.** `rfd`'s manifest puts every GTK, Wayland and portal dependency behind `[target.'cfg(any(target_os = "linux", …))'.dependencies]`, so `xdg-portal = ["pollster"]` enables a crate that is itself target-gated to Linux and the BSDs, and asking for it on Windows is inert rather than exclusive. The features gate the *Linux* backends specifically; they do not switch a backend on for every platform.

So the split is **hygiene, not a fix**. It is still worth doing — a `[target.'cfg(unix)'.dependencies]` / `[target.'cfg(windows)'.dependencies]` split with the existing five-line `rfd` comment preserved verbatim on the Unix side, because its reasoning about `wayland` and `gtk3` is still the reasoning. What it buys is that the next reader of `Cargo.toml` does not have to run the command above to find out whether the Windows build works, and that a future `rfd` release which *does* make its features exclusive cannot break the Windows target silently.

**1. Confirm on the runner what `cargo tree` says on this laptop.** Resolution is not compilation. The spike's first job is a Windows build that reaches `rfd`'s and `arboard`'s Win32 code, so the claim above is backed by a compiler rather than by a dependency graph.

**2. Confirm the dialog and the clipboard actually work once built** — resolving, compiling and functioning are three different statements, and only the third is criterion 6.

**3. Does `mupdf-sys` build on `windows-latest` at all?** It vendors MuPDF from C source and runs `bindgen` over its headers, which needs `libclang` — Chron2 had to install `clang` on this laptop before the first build would go through. The Windows runner's toolchain is MSVC, and `bindgen` there needs LLVM present. Chron2 measured a full vendored compile at about 1m40s on Linux; the Windows number is unknown and the *success* is unknown, which matters more.

**4. Which Slint renderer the release binaries use.** Slint's default renderer wants a GL context. On a Windows runner, in a VM, or over a remote desktop, that is not always there, and a packaged app that opens to nothing is indistinguishable from one that crashed. The spike should establish whether the software renderer needs to be available as a fallback, and if so whether it is a feature on the release build or a runtime environment variable documented in the README.

The spike's output is four yes-or-no answers and a `Cargo.toml`. It runs in CI, on a branch, before the release workflow is written — because a release workflow built on the assumption that all three targets compile is a workflow whose first real run is its first test.

## Files to add and change

```
Cargo.toml            # + target-gated rfd and arboard, [package.metadata.deb],
                      #   [profile.release], Windows build-dependencies
build.rs              # + Windows resources: the .ico and an app manifest
src/main.rs           # + slint::set_xdg_app_id, so a Wayland shell can tie
                      #   the window to the desktop entry
README.md             # + screenshots, a Windows data path, no AUR route
build/parachron.manifest              # NEW — DPI awareness, asInvoker
packaging/
├── org.parachron.Parachron.desktop   # NEW — CORE §7 names this file exactly
└── PKGBUILD                          # NEW — the Arch package
.github/
└── workflows/
    ├── ci.yml        # NEW — build and test on push and pull request
    ├── spike.yml     # NEW — the four Windows answers, on a branch, first
    └── release.yml   # NEW — three assets on a tag ~~and the AUR push~~
```

`packaging/` rather than `build/`, which already exists. The distinction is who reads them: `build/icons/` holds assets the *app* reads — `app.slint` references `../build/icons/parachron-256.png` at compile time — while `packaging/` holds files only a packager ever opens. Keeping them apart means a person looking for what ships is not reading past what the binary embeds.

~~`README.md` is listed as new and is genuinely absent, which is worth one more sentence: `Cargo.toml` already says `readme = "README.md"`, so the manifest currently points at a file that is not there. Nothing has noticed because nothing has run `cargo package`.~~

**Overtaken by `9768a07`:** the README exists, and `readme = "README.md"` now points at something. What this milestone owes the page is listed under **The README** in the tasks below, and it is smaller than writing one: screenshots, a Windows line for the data-directory section, the AUR route coming out, and a dependency sentence that currently claims more than it means.

## Tasks

### The dependency split

- [x] Spike the four questions above in CI, on a branch, and record the answers here before the rest of the milestone is written in detail — `.github/workflows/spike.yml`; answers under **What the spike returned**
- [x] `Cargo.toml`: `rfd` split by target — `xdg-portal` on Unix with its comment intact, the default Windows backend on Windows
- [x] `Cargo.toml`: `arboard` split by target — `wayland-data-control` on Unix, the Windows backend on Windows
- [x] `Cargo.toml`: `[target.'cfg(windows)'.build-dependencies]` for the resource compiler — `winresource = "0.1.31"`, with the host-vs-target caveat written at the section
- [x] `build.rs`: on Windows, compile `build/icons/parachron.ico` and an application manifest into the executable; on every other target, do exactly what it does today — gated on `CARGO_CFG_TARGET_OS`, not `cfg!(windows)`; see the correction below
- [x] `Cargo.toml`: `[profile.release]` — LTO, one codegen unit, symbols stripped, panic behaviour chosen deliberately rather than by default (`unwind`, and the reason is written at the line)
- [ ] Confirm `time`'s `local-offset` guard still holds on Windows: `main` reads the offset before anything spawns, and that ordering is a soundness requirement, not a preference (Chron4) — **not confirmed on Windows.** `main.rs` still calls `data::local_offset()` as its first statement and nothing was reordered, but "the ordering is unchanged" is a fact about this diff, not an observation of `time` on a Windows runner

### Linux install layout

- [x] ~~`main.rs`: `slint::set_xdg_app_id("org.parachron.Parachron")` between `local_offset()` and `AppWindow::new()`~~ — done ahead of this milestone in `167d3bf`, and **placed differently from what this line says**; see the correction below. Without it a Wayland session never associates the window with the entry, and no value in the `.desktop` file can repair that (Technical notes)
- [x] `packaging/org.parachron.Parachron.desktop`: `Name=Parachron`, `Exec=parachron`, `Icon=parachron`, `Categories=Utility;Office;`, a `StartupWMClass` matching the app id above, and a `Comment` — with a `Comment[tr]` beside it, because a desktop entry is the one piece of UI copy that lives outside the string table by necessity. The entry existed from `167d3bf`; this milestone added the `StartupWMClass`, which was the one field missing
- [x] Confirm the app id empirically rather than by reading the source: `xprop WM_CLASS` under the `Xvfb :98` harness the earlier milestones use, before and after the line above — **`"", "org.parachron.Parachron"`**, and the empty first member is a finding rather than a formality; see the correction below
- [x] `desktop-file-validate` the entry, in `ci.yml` rather than in the `PKGBUILD`, so it is checked on every push and not only when somebody packages — passes, with one hint left unactioned and recorded below
- [x] Icon install map: `build/icons/parachron-<n>.png` → `/usr/share/icons/hicolor/<n>x<n>/apps/parachron.png`, for every size from 16 to 512; `parachron-1024.png` stays in the repo as artwork and ships in no package
- [x] `LICENSE` installed to `/usr/share/licenses/parachron/LICENSE` on Arch and to `/usr/share/doc/parachron/copyright` on Debian — the policy path each distribution's own tooling reads, which is not the single path this file originally named for both
- [x] `Cargo.toml`: `[package.metadata.deb]` naming the binary, the icons, the desktop entry and the licence, with the same layout CORE §7 specifies, and a `maintainer` with an address in it — `authors = ["sudo-megas"]` has no `<…>` part and `cargo-deb` would emit a malformed `Maintainer:` field from it
- [x] Both dependency lists carry the ~~twelve~~ **thirteen** `dlopen`ed libraries as well as the ~~eleven~~ **twelve** linked ones (Technical notes), and the two lists are kept identical — the counts in this line were both wrong and the thirteenth is the one that mattered; see the correction below
- [x] `packaging/PKGBUILD`: `pkgname=parachron`, the AGPL licence field, build and package functions, and the same install map — built and its contents listed with `makepkg -f` plus `bsdtar -tf`; the `-i` half needs root and is the maintainer's to run

### CI

- [x] `.github/workflows/ci.yml`: on push and pull request — `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt --check`; **tests in the dev profile**, for the reason in Technical notes. A Linux/Windows matrix with `fail-fast: false`, so a Windows break does not hide the Linux result
- [x] Cache the vendored MuPDF build per target, and confirm the cache is actually hit on a second run rather than assumed to be — the workflow prints `MuPDF cache: HIT`/`MISS` and the matched key, so criterion 9 is read out of the log rather than inferred from a stopwatch
- [x] `.github/workflows/release.yml`: on a `v*` tag — build all three assets and attach them to a GitHub release ~~, and push the AUR package~~
- [x] MuPDF statically linked or bundled per target, so no asset depends on a MuPDF the user has to install (CORE §7) — asserted on the artefact in all three jobs, `ldd` on Linux and `dumpbin /dependents` on Windows, each failing the job if `mupdf` appears. Confirmed locally: the Linux release binary's `ldd` has no MuPDF in it
- [x] The release workflow sets `PARACHRON_BUILD_DATE` from the tag rather than leaving `build.rs` to read the runner's clock — Chron8 stamps it at compile time so that a source build honestly reports its own build day, and a *released* asset should carry the release date CORE §4 asks the About pane for. This is the seam Chron8 hands over. Taken with `git log -1 --format=%cs` on the tag and passed to all three build jobs
- [ ] ~~The AUR push authenticates with the registered key and commits as `sudo-megas` (CORE §8 rule 2)~~ — struck with the AUR
- [x] No AI attribution in any workflow file, comment, commit or release note — CORE §8 rule 1 covers generated YAML exactly as it covers Rust
- [x] **Added, not planned:** three consistency guards this file did not ask for, each closing a way two files could disagree without anybody noticing — the `v*` tag against `Cargo.toml`'s version (a mismatch ships assets whose filenames contradict their own About pane), `PKGBUILD`'s `pkgver` against the same, and `main.rs`'s `APP_ID` against both the `StartupWMClass` and the entry's filename (all three have to be one string or a desktop quietly draws the wrong icon)

### The README

- [x] ~~`README.md` per `usereadme.md`'s layout: wordmark and icon, the badge row, description, dependencies, the four installation routes, the app-sections walkthrough, and the licence summary~~ — done ahead of this milestone in `9768a07`
- [x] ~~Written **after** the first release exists, so every download link and every badge points at something real (CORE §9)~~ — deliberately inverted; the cost is stated on the page itself, in a note saying packaged downloads arrive with the first release and build-from-source works today
- [x] ~~Take the AUR route back out of the Arch section~~ — it was never in it. `9768a07` wrote the page after the AUR had already been withdrawn, so the Arch section has only ever named the two routes that work. This task was written from the plan rather than from the file
- [x] Add a Windows line to **Where your data lives**, and a line for a vault the user has relocated (Chron9) — `%APPDATA%\parachron\data\`, read out of `directories`' own source rather than its documentation, plus a **Keeping documents on another disk** subsection covering the move, why `config.toml` stays behind, and what a missing vault does
- [x] Soften "To run a packaged release — nothing", which is true of MuPDF and reads as a claim about everything; both Linux packages declare a dozen runtime dependencies — now says there is no PDF engine to install, which is the true and useful half, and that the Linux packages do depend on the graphics, font and D-Bus libraries a desktop already has
- [ ] Screenshots of the real app for the sections walkthrough, taken on the isolated display the earlier milestones use — **not done**, and the only task on this list left undone for a reason other than needing CI or a person with a Windows machine. See **How the criteria were verified**

## Acceptance criteria

1. A pushed `v*` tag produces a GitHub release carrying exactly three assets: `.pkg.tar.zst`, `.deb`, `.exe`.
2. `pacman -U` on the `.pkg.tar.zst` installs the binary to `/usr/bin/parachron`, the icons under `/usr/share/icons/hicolor/`, the desktop entry to `/usr/share/applications/`, and the licence under `/usr/share/licenses/parachron/`; the app appears in the desktop menu with its icon and launches from it.
3. `makepkg -si` from the repository's own `PKGBUILD` produces the same package from source.
4. `apt install ./parachron_*.deb` on Debian or Ubuntu installs to the same layout and launches from the menu.
5. The Windows `.exe` runs on a clean Windows machine with no MuPDF installed, opens no console window, and shows its own icon in Explorer and the taskbar.
6. On Windows, `Add Document` opens a real file dialog and the serial strip really copies. The dependency graph says both backends are there; this criterion is the difference between a graph and a working application, and it is the one that proves the Windows target rather than any single feature of it.
7. Every package includes the full AGPL text, and the binary's About pane shows it too (Chron8).
8. `cargo test` is green in CI on every target that runs it, in the dev profile, with no warnings from `cargo build` either.
9. A second CI run hits the MuPDF cache and completes materially faster than the first.
10. ~~`paru -S parachron` (or `yay`, or a manual AUR clone) installs a working app from the AUR.~~ — struck with the AUR. It was one of the four criteria that could not be checked on this laptop, and the only one that has stopped being checkable anywhere.
11. `README.md` follows `usereadme.md`'s layout, carries screenshots of the real app, names no install route that does not exist, every download link resolves, and every badge shows a real value.
12. No AI attribution anywhere in the repository, the workflows or the release notes.
13. `git log` shows only `sudo-megas` as author.

## Technical notes

**CI must run the tests in the dev profile, and this is not a preference.** `build.rs` enables Slint's debug info only when Cargo sets `DEBUG=true`, and Slint records element ids only when debug info is emitted. Every headless UI assertion in `ui_tests.rs` finds its elements by id. So `cargo test --release` does not run a faster version of the suite; it runs a suite where the element lookups find nothing and the UI test fails wholesale. The release *binary* stays lean because the release profile still turns it off — that was the point of writing it that way — but a workflow that reaches for `--release` on the test step to save a minute will break the one test that covers the window.

**The vault's path on Windows is a CORE §3 question, not a packaging one.** CORE §3 documents the data directory as `~/.local/share/parachron/`, and `data.rs` pins the project path deliberately "so the directory is named exactly what CORE §3 documents on every platform." On Windows, `directories` resolves that to somewhere under `%APPDATA%`, which is correct behaviour and is not what §3 says. The moment a Windows binary exists, §3 is describing one of three targets. It should gain the other two paths — CORE's own rule is that when reality and CORE differ, CORE is updated first — and the README's Windows section should say where a user's data actually lives, because "everything human-readable, rsync-friendly, no hidden state" is a promise that needs an address.

**A release is not a commit.** CORE §8 rule 2 says every commit, push and pull request is authored by `sudo-megas` and never by a bot. A GitHub release created by a workflow is attributed to the workflow's token, which is not a person and also not a commit — it is an artefact upload against a tag the user pushed, and the tag is the authored thing. Worth stating rather than letting a future reader discover that "no bots" was interpreted loosely somewhere.

~~The AUR is different in kind: publishing there *is* a git commit in a git repository, so it must carry `sudo-megas` as author and must authenticate as them. That is why the SSH key is a human prerequisite rather than a task.~~ — struck with the AUR, and worth leaving legible, because it is the reasoning to restore rather than re-derive if the AUR comes back. With it gone, **this milestone makes no git commit outside this repository at all**, which removes the only place rule 2 was going to be tested against a machine holding a credential.

**AGPL compliance is mostly already satisfied, and the part that is not is an install path.** CORE §1 chose AGPL-3.0 because MuPDF's linkage requires it, and CORE §7 says the public repository satisfies the source obligation — which it does, for a binary built from a public tree by a public workflow whose logs anyone can read. What each package still has to do is ship the licence text itself, at the path that distribution's users and tooling expect. That is one line in the `PKGBUILD`, one entry in `[package.metadata.deb]`, and on Windows the About pane Chron8 built, which is the only place a Windows user would look.

**Static or bundled, but never "the user installs MuPDF".** CORE §7 flags this and it is worth restating as a rule the workflow has to obey rather than an aspiration: `mupdf-sys` vendors and builds MuPDF from source, so the natural outcome is a static link, and the natural failure is a `.deb` that declares a runtime dependency on a system `libmupdf` that Debian does not ship in the version this needs. Every asset is checked with `ldd` — or its Windows equivalent — before it is attached to a release, and what it links against is a criterion rather than a note.

**Corrected before this milestone began: a dependency list built from `ldd` produces a package that installs cleanly and then fails to open a window.** The paragraph above tells a packager to check with `ldd`, and for the question it was answering — *is MuPDF linked in, or is it expecting a system copy* — that is the right instrument. It is the wrong instrument for `depends=()`, and this file did not say so.

`ldd target/release/parachron` reports eleven libraries: fontconfig, freetype, libpng, expat, bzip2, brotli, zlib, libgcc, libc and friends. Slint's winit backend opens twelve more with `dlopen` at runtime — `libX11`, `libX11-xcb`, `libxcb`, `libXcursor`, `libXrender`, `libXi`, `libxkbcommon`, `libxkbcommon-x11`, `libwayland-client`, `libwayland-egl`, `libEGL`, `libGL` — and none of them appears in any `ldd` output, because none of them is a link-time dependency. A `depends=()` or a `Depends:` derived from `ldd` alone therefore installs without complaint, puts the app in the menu, and produces nothing when it is clicked. On the `.deb` side `cargo-deb`'s `$auto` runs `dpkg-shlibdeps`, which reads the same eleven and misses the same twelve.

That is the worst shape a packaging bug can take: criteria 2 and 4 both say "installs … and launches from it", so both would pass their install half and fail their launch half, on a machine that is not this one. The two lists are written out in full in both `PKGBUILD`s and in `[package.metadata.deb]`, and they are the same list and move together.

This is recorded as a correction rather than folded in silently, in the genre this file's own spike section already uses on itself — and for the same reason. The first draft asserted something about `rfd` and `arboard` that ten seconds of `cargo tree` disproved; this one asserted an instrument, and reading what the binary actually opens disproved it. **A claim about code is not a finding until the code has been read at the line the claim is about** (Chron8).

**Corrected during this milestone: the twelve are thirteen, and the thirteenth is the one that would have shipped broken.** The paragraph above names twelve `dlopen`ed libraries and eleven linked ones. Both counts came from a reading rather than a measurement, and when the measurement was taken — `ldd` for the first list, and the soname literals carried in an unstripped release binary for the second — it returned **twelve linked and thirteen opened**. Twelve of the thirteen are the ones already listed. The thirteenth is `libdbus-1.so.3`, and it is not opened by Slint at all:

```
rfd-0.17.2/src/backend/xdg_desktop_portal/portal/ffi.rs:199
  Liblary::open(c"libdbus-1.so.3").or_else(|| Liblary::open(c"libdbus-1.so"))
```

That is the file dialog. What makes it worth its own correction rather than a thirteenth row is how it fails. Every other library in the list is needed to put a window on screen, so omitting one produces an app that does not start — loud, and found by the first person who clicks the launcher. libdbus is needed to open a *dialog*, and `rfd` degrades rather than panics; `portal/libdbus.rs:77` logs `Can't connect to a portal: libdbus-1.so not found` and carries on. A package missing it installs cleanly, appears in the menu, opens its window, draws all three columns, and then does nothing whatsoever when `Add Document` or `EXPORT` is clicked, with the only trace in a log nobody is reading.

So the paragraph above was right about the *shape* of the bug — "installs cleanly and then fails to open a window" — and its own list would have shipped a worse version of it: installs cleanly, opens a window, and cannot add a document. An app that cannot add a document is not an app. This was found by doing the thing that paragraph ends by recommending, one level further down: not reading what the toolkit opens, but reading what *everything* the binary links opens, and then reading the line.

**Corrected during this milestone: `set_xdg_app_id` goes *after* `AppWindow::new`, and this file says twice that it goes before.** The task list says "between `local_offset()` and `AppWindow::new()`" and the technical note below repeats it as "one line in `main.rs`, between Chron4's ordering requirement and before any window exists, which is what `set_xdg_app_id` requires". The shipped code, written in `167d3bf`, puts it after `AppWindow::new()` and before `app.show`, and `main.rs:63-69` explains why: the call reaches for a platform that only exists once a window has asked for one, and returns `NoPlatform` before that.

**The code is right and this file is wrong.** The requirement is not "before any window exists" but "before the window is *mapped*" — winit reads the id when the window is first shown and never looks again, so the whole span between `AppWindow::new` and `app.show` is available and the span before `AppWindow::new` is not. This is corrected rather than left as written, because CORE §9's rule is that anything a reader would act on today gets fixed: a reader following the two lines above would move a working call to where it returns an error, and lose the icon this milestone's criterion 2 is about.

**Measured: `WM_CLASS` is `"", "org.parachron.Parachron"`, and the empty half is worth a sentence.** Chron9 ran this check before `set_xdg_app_id` existed and got `"parachron", "parachron"` — winit's fallback to `argv[0]`. The same check now returns an empty instance name and the app id as the class. `StartupWMClass` is matched against either member, so `StartupWMClass=org.parachron.Parachron` is correct and is what the entry now carries. Worth recording rather than rounding off, because "the app id is in `WM_CLASS`" and "`WM_CLASS` is the app id twice" are different facts, and a future reader debugging an X11 association will want the one that is true. This is the X11 half only; Wayland has no `WM_CLASS` and matches the entry by filename against the app id, which is why the reverse-DNS name on the `.desktop` file is load-bearing rather than decorative.

**The harness lesson recurred, for the fourth milestone running.** Chron3 wrote it down: this machine's session is Plasma Wayland, Slint's winit backend prefers Wayland whenever `WAYLAND_DISPLAY` is set, so `DISPLAY=:98` alone is not isolation — the app opens on the real desktop while the script watches an empty Xvfb, with no error and no window to find. Chron9's closing commit was about the same lesson recurring. The first `xprop` run in this milestone did exactly that, and reported no window rather than a wrong one, which is the failure mode that makes it cost a debugging round each time. Every launch goes through `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE`. Recorded a fourth time because four is the number of times it has now cost something.

Two smaller things found the same way and not worth their own paragraphs. `xwininfo` is not installed on this machine, so a harness that reaches for it fails silently and looks like "no window"; `xdotool search --pid` is what the earlier milestones actually used and what works. And `desktop-file-validate` passes the entry with one hint — `Categories=Utility;Office;` names two main categories, so the app "might appear more than once in the application menu". That is left as written: both categories are true of this app, the consequence is a duplicate menu entry rather than a missing one, and the value is the one this file specified. It is recorded so the hint is a decision rather than something nobody read.

**Nothing calls `slint::set_xdg_app_id`, and criterion 2 fails on the maintainer's own desktop because of it.** The desktop entry gets a `StartupWMClass` so a shell can tie a running window to the launcher that started it, and the value has to match what the toolkit actually reports. Slint sets a window's app id only when asked: `i-slint-backend-winit`'s window adapter calls winit's `with_name` only if `xdg_app_id()` returns `Some`, and `set_xdg_app_id` is the only thing that makes it. Nothing in `src/` or `ui/` calls it.

Without it, winit falls back to `argv[0]` on X11 — so `WM_CLASS` reads `"parachron", "parachron"`, which a `StartupWMClass` could at least be written against — and on Wayland it does not set an app id **at all**. Wayland has no `WM_CLASS` and no `StartupWMClass` to fall back on, so on a Plasma or GNOME Wayland session the entry and the window are never associated: wrong icon in the task bar, a second launcher entry, and no value in the `.desktop` file that could fix it. This project's own desktop is Wayland, which makes criterion 2 fail where it is most likely to be checked first.

The repair is one line in `main.rs`, between `local_offset()` and `AppWindow::new()` — after Chron4's ordering requirement and before any window exists, which is what `set_xdg_app_id` requires. It is not on this milestone's task list because the task list was written believing the `.desktop` file was the whole of the problem.

**`96` and `1024` are not the same kind of leftover.** The icon set has ten PNG sizes and a `.ico`. The hicolor theme takes whatever sizes are installed into it, so 16 through 512 all go in and cost a few hundred kilobytes between them. `parachron-1024.png` is 1.6MB on its own, is larger than any icon theme will ever ask for, and exists for artwork — a README header, a store listing, a future website. It stays in the repository and ships in nothing.

**The `.ico` has never been used by anything.** CORE §7 says it feeds the Windows build, and today nothing references it: the window icon and the title-bar mark both point at PNGs. Wiring it means a resource compiler in `build.rs` under a Windows-only build dependency, alongside an application manifest — which is also where the DPI-awareness declaration belongs, and getting that wrong on Windows is exactly the soft, blurry result Chron2 documented for HiDPI on Linux.

**`README.md` is written last, on purpose.** CORE §9's line for this milestone says it is written "once release assets exist", and the reason is in `usereadme.md`: the page is for a user who wants to install the app, and it carries download instructions, version and release-date badges, and a package size in megabytes. Every one of those is a fact about a release. Writing it first means writing placeholders, and a README with a dead download link is worse than no README, because it looks maintained.

**One thing `usereadme.md` asks for that a README cannot do.** It says to use CaskaydiaCove Nerd Font globally "if applicable". GitHub renders Markdown in its own font stack and ignores anything a repository says about typography, so the instruction cannot apply to the page itself. Where it *can* apply is any image the README embeds — a header wordmark, a screenshot, a diagram — and that is how it should be read. Saying so here means nobody later tries to force it with HTML that GitHub strips.

**Windows is the honest risk, and this file does not pretend otherwise.** There is no Windows machine attached to this project. `mupdf-sys` vendoring a C library through `bindgen` under MSVC, a resource-compiled icon, a clipboard backend nobody has run, a file dialog nobody has opened, and a renderer that may want a GL context the runner does not have — five unknowns, all on one target, all of them CI's to answer. CORE §7 already said "no local Windows machine — CI owns this target." What this milestone adds is that CI owning it means the first green run is the first evidence, so the spike runs before the release workflow is written rather than after.

## How the criteria were verified

Written when the milestone is done. Chron8 is the model rather than Chron1–7, which this line originally named: its **Not verified.** list — a bolded noun phrase, what was done instead, the argument standing in for the observation, then a flat sentence conceding the argument is not the observation — is the exact form this milestone needs.

It will have a section this project has not needed before: **what was verified only by CI, and what was verified only by one person on one machine.** Criteria 4, 5 and 6 cannot be checked on this laptop, and criteria 2 and 3 are the only two that can be checked locally end to end. Criterion 10 used to be in that first list and is now struck, which does not improve the ratio — it removes a row rather than answering it.

## Done when

All acceptance criteria pass, which for the first time means "pass in CI and on a machine that is not this one". Then: amend CORE §3 with the data directory on all three targets, amend CORE §7 with whatever the spike settles about MuPDF per target and the renderer, record in CORE §4 that a released binary's About date comes from its tag while a source build's comes from its clock, ~~record the AUR package name and its repository~~, note in CORE §8 how rule 2 applies to a release, record in CORE §7 that the AUR route was designed and withdrawn and under what condition it returns, mark this file's status `done` — and with it, the roadmap in CORE §9.
