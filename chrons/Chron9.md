# Chron9 — Packaging and CI

**Milestone:** 9 of ~9 (CORE §9)
**Status:** planned
**Builds against:** CORE §1 (identity — app id, binary name, icons, licence, repo), §2 (stack — every dependency has to exist on three platforms), §3 (data model — where the vault lives on a target that is not Linux), §7 (packaging & CI, in full), §8 (conventions & development rules, including who is allowed to be the author of a release), §9 (roadmap — the README lands here)

## Goal

Parachron becomes something a person installs rather than something a person builds. A tag pushed to the repository produces three assets — `.pkg.tar.zst`, `.deb`, `.exe` — built by GitHub Actions, each carrying the binary, the icons, the licence and, on Linux, a desktop entry that puts the app in a menu. An AUR package makes `paru -S parachron` work. And `README.md` is finally written, for the person who landed on the GitHub page and wants the app, not the history.

This is the only milestone whose output is not code the app runs, and the only one whose acceptance criteria are largely unverifiable on this laptop. Both facts shape everything below.

## Scope

**In:** the dependency split that makes a Windows build possible at all · `packaging/org.parachron.Parachron.desktop` · the icon install map · `[package.metadata.deb]` · a `PKGBUILD` · Windows resources (icon and manifest) through `build.rs` · `[profile.release]` · a CI workflow on push and a release workflow on tag · MuPDF built statically or bundled per target · AGPL compliance in every artefact · the AUR package · `README.md` per `usereadme.md`.

**Out (explicitly):** a Windows installer — the release asset is the executable CORE §7 names, and an MSI or NSIS wrapper is a second packaging system for one of three targets · code signing on Windows, which needs a certificate somebody has to buy and renew, and an unsigned binary with a public build log is the more honest artefact for an AGPL app · Flatpak, Snap and AppImage, none of which CORE §7 lists · macOS, for the same reason · auto-update, which is a network call in an app that makes none (CORE §4) · publishing to crates.io, since this is an application and a vendored MuPDF makes it a hostile dependency for anyone who did pull it in · a changelog, which CORE §8 rule 3 rules out of the README and the repository already keeps · translating the README, which is a document for a GitHub page rather than UI copy.

## The human actions this milestone needs, which no tooling can perform

Named up front because three of them block acceptance and none of them is a task anybody can tick on the user's behalf.

1. **The GitHub repository must exist at `https://github.com/sudo-megas/PARACHRON` with Actions enabled.** CORE §1 names it and `Cargo.toml` points at it; nothing in this tree proves it is there.
2. **An AUR account with an SSH public key registered, and that private key added to the repository as a secret.** AUR publication is a `git push` to `ssh://aur@aur.archlinux.org/parachron.git`, and it is a *commit* — which puts it squarely under CORE §8 rule 2, so it must be authored by `sudo-megas` and by nothing else.
3. **A tag has to be pushed by a person.** The release workflow triggers on it; nothing triggers the person.

Everything else in this file is work that can be written and reviewed before any of the three happen.

## The spike, and why this one runs before the notes are finished

Chron3 established that a hard question gets spiked rather than assumed, and Chron7 showed what it costs to skip one — a feature that produced a valid file with words silently missing. This milestone's hard question is not about MuPDF. It is that **two dependencies are currently configured so that a Windows build has no implementation at all**, and neither of them fails at compile time in a way anyone has seen, because nobody has ever compiled this for Windows.

**1. `rfd` has no backend on Windows as configured.** `Cargo.toml` carries `default-features = false, features = ["xdg-portal"]`, with a five-line comment explaining — correctly, and for good reasons that stay — why `wayland` and `gtk3` were both rejected. `xdg-portal` is a Linux backend. A Windows build of `rfd` with default features off and only that one on has nothing to open a dialog with, so `Add Document` and `EXPORT` — the only two ways to get files into or out of the vault — would be dead on the target CORE §7 says CI owns. The fix is a `[target.'cfg(unix)'.dependencies]` / `[target.'cfg(windows)'.dependencies]` split, with the existing comment preserved verbatim on the Unix side because its reasoning is still the reasoning.

**2. `arboard` is the same shape.** `default-features = false, features = ["wayland-data-control"]` is a Linux-only feature set, and the clipboard is how the serial strip, the purchase link and both About URLs do the only thing CORE §4 lets them do. Same split, same reason.

**3. Does `mupdf-sys` build on `windows-latest` at all?** It vendors MuPDF from C source and runs `bindgen` over its headers, which needs `libclang` — Chron2 had to install `clang` on this laptop before the first build would go through. The Windows runner's toolchain is MSVC, and `bindgen` there needs LLVM present. Chron2 measured a full vendored compile at about 1m40s on Linux; the Windows number is unknown and the *success* is unknown, which matters more.

**4. Which Slint renderer the release binaries use.** Slint's default renderer wants a GL context. On a Windows runner, in a VM, or over a remote desktop, that is not always there, and a packaged app that opens to nothing is indistinguishable from one that crashed. The spike should establish whether the software renderer needs to be available as a fallback, and if so whether it is a feature on the release build or a runtime environment variable documented in the README.

The spike's output is four yes-or-no answers and a `Cargo.toml`. It runs in CI, on a branch, before the release workflow is written — because a release workflow built on the assumption that all three targets compile is a workflow whose first real run is its first test.

## Files to add and change

```
Cargo.toml            # + target-gated rfd and arboard, [package.metadata.deb],
                      #   [profile.release], Windows build-dependencies
build.rs              # + Windows resources: the .ico and an app manifest
README.md             # NEW — per usereadme.md (CORE §8 rule 3)
packaging/
├── org.parachron.Parachron.desktop   # NEW — CORE §7 names this file exactly
└── PKGBUILD                          # NEW — the Arch package
.github/
└── workflows/
    ├── ci.yml        # NEW — build and test on push and pull request
    └── release.yml   # NEW — three assets on a tag, and the AUR push
```

`packaging/` rather than `build/`, which already exists. The distinction is who reads them: `build/icons/` holds assets the *app* reads — `app.slint` references `../build/icons/parachron-256.png` at compile time — while `packaging/` holds files only a packager ever opens. Keeping them apart means a person looking for what ships is not reading past what the binary embeds.

`README.md` is listed as new and is genuinely absent, which is worth one more sentence: `Cargo.toml` already says `readme = "README.md"`, so the manifest currently points at a file that is not there. Nothing has noticed because nothing has run `cargo package`.

## Tasks

### The dependency split

- [ ] Spike the four questions above in CI, on a branch, and record the answers here before the rest of the milestone is written in detail
- [ ] `Cargo.toml`: `rfd` split by target — `xdg-portal` on Unix with its comment intact, the default Windows backend on Windows
- [ ] `Cargo.toml`: `arboard` split by target — `wayland-data-control` on Unix, the Windows backend on Windows
- [ ] `Cargo.toml`: `[target.'cfg(windows)'.build-dependencies]` for the resource compiler
- [ ] `build.rs`: on Windows, compile `build/icons/parachron.ico` and an application manifest into the executable; on every other target, do exactly what it does today
- [ ] `Cargo.toml`: `[profile.release]` — LTO, one codegen unit, symbols stripped, panic behaviour chosen deliberately rather than by default
- [ ] Confirm `time`'s `local-offset` guard still holds on Windows: `main` reads the offset before anything spawns, and that ordering is a soundness requirement, not a preference (Chron4)

### Linux install layout

- [ ] `packaging/org.parachron.Parachron.desktop`: `Name=Parachron`, `Exec=parachron`, `Icon=parachron`, `Categories=Utility;Office;`, and a `Comment` — with a `Comment[tr]` beside it, because a desktop entry is the one piece of UI copy that lives outside the string table by necessity
- [ ] Icon install map: `build/icons/parachron-<n>.png` → `/usr/share/icons/hicolor/<n>x<n>/apps/parachron.png`, for every size from 16 to 512; `parachron-1024.png` stays in the repo as artwork and ships in no package
- [ ] `LICENSE` installed to `/usr/share/licenses/parachron/LICENSE` in both Linux packages
- [ ] `Cargo.toml`: `[package.metadata.deb]` naming the binary, the icons, the desktop entry and the licence, with the same layout CORE §7 specifies
- [ ] `packaging/PKGBUILD`: `pkgname=parachron`, the AGPL licence field, build and package functions, and the same install map — verified with `makepkg -si` on this machine, which is the one Linux target that can be tested locally

### CI

- [ ] `.github/workflows/ci.yml`: on push and pull request — `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt --check`; **tests in the dev profile**, for the reason in Technical notes
- [ ] Cache the vendored MuPDF build per target, and confirm the cache is actually hit on a second run rather than assumed to be
- [ ] `.github/workflows/release.yml`: on a `v*` tag — build all three assets, attach them to a GitHub release, and push the AUR package
- [ ] MuPDF statically linked or bundled per target, so no asset depends on a MuPDF the user has to install (CORE §7)
- [ ] The AUR push authenticates with the registered key and commits as `sudo-megas` (CORE §8 rule 2)
- [ ] No AI attribution in any workflow file, comment, commit or release note — CORE §8 rule 1 covers generated YAML exactly as it covers Rust

### The README

- [ ] `README.md` per `usereadme.md`'s layout: wordmark and icon, the badge row, description, dependencies, the four installation routes, the app-sections walkthrough, and the licence summary
- [ ] Written **after** the first release exists, so every download link and every badge points at something real (CORE §9)
- [ ] Screenshots of the real app for the sections walkthrough, taken on the isolated display the earlier milestones use

## Acceptance criteria

1. A pushed `v*` tag produces a GitHub release carrying exactly three assets: `.pkg.tar.zst`, `.deb`, `.exe`.
2. `pacman -U` on the `.pkg.tar.zst` installs the binary to `/usr/bin/parachron`, the icons under `/usr/share/icons/hicolor/`, the desktop entry to `/usr/share/applications/`, and the licence under `/usr/share/licenses/parachron/`; the app appears in the desktop menu with its icon and launches from it.
3. `makepkg -si` from the repository's own `PKGBUILD` produces the same package from source.
4. `apt install ./parachron_*.deb` on Debian or Ubuntu installs to the same layout and launches from the menu.
5. The Windows `.exe` runs on a clean Windows machine with no MuPDF installed, opens no console window, and shows its own icon in Explorer and the taskbar.
6. On Windows, `Add Document` opens a real file dialog and the serial strip really copies — the two things the current dependency configuration cannot do there.
7. Every package includes the full AGPL text, and the binary's About pane shows it too (Chron8).
8. `cargo test` is green in CI on every target that runs it, in the dev profile, with no warnings from `cargo build` either.
9. A second CI run hits the MuPDF cache and completes materially faster than the first.
10. `paru -S parachron` (or `yay`, or a manual AUR clone) installs a working app from the AUR.
11. `README.md` follows `usereadme.md`'s layout, every download link resolves, and every badge shows a real value.
12. No AI attribution anywhere in the repository, the workflows, the release notes or the AUR package.
13. `git log` shows only `sudo-megas` as author, in this repository and in the AUR one.

## Technical notes

**CI must run the tests in the dev profile, and this is not a preference.** `build.rs` enables Slint's debug info only when Cargo sets `DEBUG=true`, and Slint records element ids only when debug info is emitted. Every headless UI assertion in `ui_tests.rs` finds its elements by id. So `cargo test --release` does not run a faster version of the suite; it runs a suite where the element lookups find nothing and the UI test fails wholesale. The release *binary* stays lean because the release profile still turns it off — that was the point of writing it that way — but a workflow that reaches for `--release` on the test step to save a minute will break the one test that covers the window.

**The vault's path on Windows is a CORE §3 question, not a packaging one.** CORE §3 documents the data directory as `~/.local/share/parachron/`, and `data.rs` pins the project path deliberately "so the directory is named exactly what CORE §3 documents on every platform." On Windows, `directories` resolves that to somewhere under `%APPDATA%`, which is correct behaviour and is not what §3 says. The moment a Windows binary exists, §3 is describing one of three targets. It should gain the other two paths — CORE's own rule is that when reality and CORE differ, CORE is updated first — and the README's Windows section should say where a user's data actually lives, because "everything human-readable, rsync-friendly, no hidden state" is a promise that needs an address.

**A release is not a commit, and an AUR push is.** CORE §8 rule 2 says every commit, push and pull request is authored by `sudo-megas` and never by a bot. A GitHub release created by a workflow is attributed to the workflow's token, which is not a person and also not a commit — it is an artefact upload against a tag the user pushed, and the tag is the authored thing. The AUR is different in kind: publishing there *is* a git commit in a git repository, so it must carry `sudo-megas` as author and must authenticate as them. That is why the SSH key is a human prerequisite rather than a task, and why it is worth stating the distinction rather than letting a future reader discover that half of "no bots" was interpreted loosely.

**AGPL compliance is mostly already satisfied, and the part that is not is an install path.** CORE §1 chose AGPL-3.0 because MuPDF's linkage requires it, and CORE §7 says the public repository satisfies the source obligation — which it does, for a binary built from a public tree by a public workflow whose logs anyone can read. What each package still has to do is ship the licence text itself, at the path that distribution's users and tooling expect. That is one line in the `PKGBUILD`, one entry in `[package.metadata.deb]`, and on Windows the About pane Chron8 built, which is the only place a Windows user would look.

**Static or bundled, but never "the user installs MuPDF".** CORE §7 flags this and it is worth restating as a rule the workflow has to obey rather than an aspiration: `mupdf-sys` vendors and builds MuPDF from source, so the natural outcome is a static link, and the natural failure is a `.deb` that declares a runtime dependency on a system `libmupdf` that Debian does not ship in the version this needs. Every asset is checked with `ldd` — or its Windows equivalent — before it is attached to a release, and what it links against is a criterion rather than a note.

**`96` and `1024` are not the same kind of leftover.** The icon set has ten PNG sizes and a `.ico`. The hicolor theme takes whatever sizes are installed into it, so 16 through 512 all go in and cost a few hundred kilobytes between them. `parachron-1024.png` is 1.6MB on its own, is larger than any icon theme will ever ask for, and exists for artwork — a README header, a store listing, a future website. It stays in the repository and ships in nothing.

**The `.ico` has never been used by anything.** CORE §7 says it feeds the Windows build, and today nothing references it: the window icon and the title-bar mark both point at PNGs. Wiring it means a resource compiler in `build.rs` under a Windows-only build dependency, alongside an application manifest — which is also where the DPI-awareness declaration belongs, and getting that wrong on Windows is exactly the soft, blurry result Chron2 documented for HiDPI on Linux.

**`README.md` is written last, on purpose.** CORE §9's line for this milestone says it is written "once release assets exist", and the reason is in `usereadme.md`: the page is for a user who wants to install the app, and it carries download instructions, version and release-date badges, and a package size in megabytes. Every one of those is a fact about a release. Writing it first means writing placeholders, and a README with a dead download link is worse than no README, because it looks maintained.

**One thing `usereadme.md` asks for that a README cannot do.** It says to use CaskaydiaCove Nerd Font globally "if applicable". GitHub renders Markdown in its own font stack and ignores anything a repository says about typography, so the instruction cannot apply to the page itself. Where it *can* apply is any image the README embeds — a header wordmark, a screenshot, a diagram — and that is how it should be read. Saying so here means nobody later tries to force it with HTML that GitHub strips.

**Windows is the honest risk, and this file does not pretend otherwise.** There is no Windows machine attached to this project. `mupdf-sys` vendoring a C library through `bindgen` under MSVC, a resource-compiled icon, a clipboard backend nobody has run, a file dialog nobody has opened, and a renderer that may want a GL context the runner does not have — five unknowns, all on one target, all of them CI's to answer. CORE §7 already said "no local Windows machine — CI owns this target." What this milestone adds is that CI owning it means the first green run is the first evidence, so the spike runs before the release workflow is written rather than after.

## How the criteria were verified

Written when the milestone is done, as in Chron1–7. It will have a section this project has not needed before — **what was verified only by CI, and what was verified only by one person on one machine** — because criteria 4, 5, 6 and 10 cannot be checked on a CachyOS laptop, and criteria 2 and 3 are the only two that can be checked locally end to end.

## Done when

All acceptance criteria pass, which for the first time means "pass in CI and on a machine that is not this one". Then: amend CORE §3 with the data directory on all three targets, amend CORE §7 with whatever the spike settles about MuPDF per target and the renderer, record the AUR package name and its repository, note in CORE §8 how rule 2 applies to a release as against an AUR commit, mark this file's status `done` — and with it, the roadmap in CORE §9.
