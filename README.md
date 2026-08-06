<img src="build/icons/parachron-512.png" alt="" width="96" align="left" hspace="12" vspace="4">

# PARACHRON

**A desktop vault for your purchases.**

<br clear="left">

[![Latest version](https://img.shields.io/github/v/release/sudo-megas/PARACHRON?label=latest&color=6c8cd5)](https://github.com/sudo-megas/PARACHRON/releases/latest)
[![Release date](https://img.shields.io/github/release-date/sudo-megas/PARACHRON?label=released&color=6c8cd5)](https://github.com/sudo-megas/PARACHRON/releases/latest)
[![Download size](https://img.shields.io/github/downloads/sudo-megas/PARACHRON/total?label=downloads&color=6c8cd5)](https://github.com/sudo-megas/PARACHRON/releases/latest)
[![Arch Linux](https://img.shields.io/badge/Arch-.pkg.tar.zst-1793d1?logo=archlinux&logoColor=white)](#arch-linux--cachyos)
[![Debian](https://img.shields.io/badge/Debian-.deb-a80030?logo=debian&logoColor=white)](#debian--ubuntu)
[![Windows](https://img.shields.io/badge/Windows-.exe-0078d4?logo=windows&logoColor=white)](#windows)
[![License](https://img.shields.io/badge/license-AGPL--3.0-green)](LICENSE)

---

## Description

Parachron keeps every product's invoices, warranty PDFs, serial number and purchase details in one place, and counts down the warranty in days.

Everything lives in plain folders on your own disk — one folder per product, human-readable, easy to back up. No account, no cloud, no telemetry.

---

## Dependencies

**To run a packaged release** — nothing. MuPDF is built into the binary.

**To build from source** you need:

- `rust` and `cargo` (stable)
- `clang` — the PDF engine generates its bindings with it
- a C toolchain: `gcc`/`g++`, `make`, `python`
- `fontconfig`

<details>
<summary>Per distribution</summary>

```bash
# Arch / CachyOS
sudo pacman -S --needed rust clang gcc make python fontconfig

# Debian / Ubuntu
sudo apt install build-essential clang python3 libfontconfig-1-dev
# plus rustup, from https://rustup.rs
```

</details>

---

## Installation

> Packaged downloads (`.pkg.tar.zst`, `.deb`, `.exe`) arrive with the first tagged release. Until then, **Build from source** below works today.

### Build from source

Works on every platform Parachron supports.

```bash
git clone https://github.com/sudo-megas/PARACHRON.git
cd PARACHRON
cargo build --release
```

The binary lands at `target/release/parachron`. Run it from there, or copy it somewhere on your `PATH`:

```bash
sudo install -Dm755 target/release/parachron /usr/bin/parachron
```

The first build takes a few minutes — it compiles MuPDF from source. Later builds are quick.

### Arch Linux / CachyOS

**From the AUR**

```bash
paru -S parachron      # or: yay -S parachron
```

**From the repository's PKGBUILD**

```bash
git clone https://github.com/sudo-megas/PARACHRON.git
cd PARACHRON/packaging
makepkg -si
```

**From a release**

Download `parachron-*.pkg.tar.zst` from [Releases](https://github.com/sudo-megas/PARACHRON/releases/latest), then:

```bash
sudo pacman -U parachron-*.pkg.tar.zst
```

### Debian / Ubuntu

Download `parachron_*.deb` from [Releases](https://github.com/sudo-megas/PARACHRON/releases/latest), then:

```bash
sudo apt install ./parachron_*.deb
```

Parachron will appear in your applications menu.

### Windows

Download `parachron.exe` from [Releases](https://github.com/sudo-megas/PARACHRON/releases/latest) and run it. It is a single executable — there is nothing to install.

The Windows build is produced by GitHub Actions from this repository, so its build log is public. It is not code-signed, so Windows may show a SmartScreen warning the first time; choose **More info → Run anyway**.

---

## How to use

### Product list

The left column lists everything in your vault, newest addition at the bottom. Two chips at the top reorder it — **A–Z** alphabetically, **Date** by purchase date, oldest first. Click either again to go back to the order you added things in. A folder Parachron cannot read stays in the list, marked, rather than disappearing.

### Search

The bar above the list narrows it as you type, matching **product names and serial numbers**. Accents and letter case do not matter: typing `sarj` finds `Şarj Cihazı`. Press `Esc` or click the ✕ to clear it. What you are reading stays open even if your search hides its row.

### Document viewer

The centre column shows the selected product's PDFs, one tab per file. Move through pages with `‹` and `›`, and use the zoom slider to get closer — at `1×` the whole page always fits, whatever the window size. Under the page, a strip shows the serial number; click it to copy.

### Details panel

The right column holds the purchase link, the purchase date, the warranty start and end, and the days remaining in large type. Click the link to copy it — Parachron never opens a browser for you. An expired warranty says so instead of counting into negative numbers.

### Add document

**Document ▾ → Add Document** opens a form for a product's name, serial, purchase link and three dates, and lets you attach its PDFs. The files are copied into the vault, so the originals are yours to move or delete. **Edit Document…** reopens the same form for the selected product.

### Export

**EXPORT** builds one PDF for the selected product: a clean summary page — name, serial, dates, days left, purchase link — followed by every one of that product's documents. It is the thing to email to a shop. Anything that could not be included is named on the summary page itself.

### Themes

**THEME** offers eleven built-in colour schemes, light and dark, including Catppuccin, Rosé Pine and a paper-like light theme. The choice applies instantly and is remembered.

### About

At the foot of the left column. Shows the version, the release date, where the source lives, and the full licence text. Parachron is also available in **Turkish** — switch under **Document ▾ → Language**.

---

## Where your data lives

```
~/.local/share/parachron/
├── products/
│   └── qd-oled-monitor/
│       ├── product.toml      # name, serial, link, dates
│       ├── invoice.pdf
│       └── warranty.pdf
└── config.toml               # theme, language, sort order, window size
```

Plain text and plain folders. Copy the directory anywhere to back it up, and Parachron will read it again as it is.

---

## Licence

Parachron is free and open source under the **GNU AGPL-3.0**. You can use it, study it, change it and share it. If you distribute a modified version — including running it as a network service — the source of your version has to stay open under the same licence.

The full text is in [LICENSE](LICENSE), and the app shows it under **About → Read the full license**.
