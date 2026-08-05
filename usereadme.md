# README.md for guiding the user who landed github page to install the app

 Readme must be pretty much more user-friendly.
 Avoid writing tons of unrelated data and changelogs for casual users.
 I will give you the Layout, you can improvise using it as a scaffold
 use CaskaydiaCove Nerd Font globally if applicable
 use colors nature of reading things easy.
 you can use mini images - icons under the text like the other users. Icons can show us "Latest Version" "Release Date" "Package in MB" "Arch Linux amblem" "Debian amblem" "Windows amblem"
 the following maximums doesnt apply for "image-descriptions" we can embed as many image as we needed.
 RULE: no AI attribution anywhere in the README. No "made with Claude", no "Claude Code", no generated-by trailers. Banned.
 App icon lives at build/icons/ — use parachron-512.png (or smaller sizes) for the header image.



            PAGE
-----------------------------
<write the text top-left "PARACHRON"> 36px
<insert the app icon near the wordmark>
<you can insert here the mini-images/icons descripted above>

1. "DESCRIPTION" 26px
<give short description here what is app about: a desktop vault for your purchases — keeps every product's invoices, warranty PDFs, serial number and purchase details in one place, and counts down the warranty in days> <maximum of 3 lines> 18px

2. DEPENDENCIES 26px
<list needed dependencies in fuzzy list format here> 18px
<runtime deps are minimal (MuPDF is bundled/static); build-from-source needs: rust/cargo, clang, and MuPDF build requirements — keep the list short and per-distro where needed>

3. INSTALLATION 26px
    3.A "Build From Source" 22px (yes, the app builds from source with cargo — keep this section.)
    <building and compiling instructions: git clone, cargo build --release, where the binary ends up> <dont overflow it just give enough for everyday user installs it without any burden> 18px

    3.B "Arch Linux / CachyOS" 22px
    <Install via git clone method instructions (makepkg -si with the repo PKGBUILD)> 18px
    <Install via AUR> 18px
    <Install via downloading the .pkg.tar.zst from Releases and pacman -U> 18px

    3.C "Debian / Ubuntu" 22px
    <the app IS Debian-compatible — .deb is an official release asset. Include this section.> 18px
    <Install via downloading the .deb from Releases and installing it (apt install ./parachron_*.deb or dpkg -i)> 18px

    3.D "Windows" 22px
    <Windows installation instructions and steps: download the .exe from Releases, run it. Note it is built by CI.> 18px

4. HOW TO USE? WHAT IS THE APPLICATION SECTIONS?
<give explanation about every section in the app><maximum of 5 lines for each section> 22px.
<sections to cover: Product List (left column: your products, sortable alphabetically or by purchase date) — Document Viewer (center: PDF preview with tabs to switch between a product's invoice/warranty files, serial number strip below) — Details Panel (right column: THEME and EXPORT buttons, purchase link, purchase date, warranty start, and the days-left counter) — Add Document (title bar menu: adding a new product with its PDFs and dates) — Export (one combined PDF: summary page + all the product's documents) — Themes (11 built-in themes) — About (bottom of the left column)>

5. LICENCE SUMMARY
<AGPL-3.0-only. one short friendly paragraph: free and open source, you can use/modify/share it, and if you distribute a modified version its source must stay open under the same license. link to the LICENSE file for the full text>
-----------------------------
            PAGE
