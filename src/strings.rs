//! The string table.
//!
//! CORE §4: every user-visible string in Parachron lives here and nowhere
//! else — no literals in `.slint` files, none in the rest of the Rust code.
//! Both languages are complete and have been since Chron6; a key whose two
//! sides are the same string is a deliberate entry in `SAME_IN_BOTH`, not an
//! unfinished translation.
//!
//! One thing on screen is not in this table, by decision rather than omission:
//! the AGPL text the About pane shows. It is a legal instrument quoted verbatim
//! from the repository's own `LICENSE`, not UI copy, and paraphrasing it into an
//! `(en, tr)` pair would be a misrepresentation. See `about.rs`.

/// The two languages Parachron ships (CORE §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Tr,
}

impl Lang {
    /// Resolve the `lang` value from `config.toml`.
    ///
    /// Anything unrecognised falls back to English. CORE §4 is explicit that
    /// the app never consults the system locale — Turkish is only ever reached
    /// by a deliberate user action.
    pub fn from_code(code: &str) -> Self {
        match code {
            "tr" => Lang::Tr,
            _ => Lang::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
        }
    }

    /// Every language, in the order the menu lists them.
    ///
    /// Only the tests walk it, the same way only the tests walk [`Key::ALL`] —
    /// `app.slint` lists the two rows literally because there are two and CORE §4
    /// says there are two, so a model would be more machinery than the thing it
    /// modelled. This exists so a test can state that.
    #[allow(dead_code)]
    pub const ALL: &'static [Lang] = &[Lang::En, Lang::Tr];

    /// Which menu row is marked.
    pub fn index(self) -> i32 {
        match self {
            Lang::En => 0,
            Lang::Tr => 1,
        }
    }

    /// The inverse of [`Lang::index`], for what a click asks for. Anything out of
    /// range falls back to English, the same way `from_code` does.
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Lang::Tr,
            _ => Lang::En,
        }
    }

    /// The menu's label for this language (CORE §4: no literals outside the
    /// table). Pushed by `apply_strings` under its own property, so this is the
    /// mapping a test uses to say which property should hold which key.
    #[allow(dead_code)]
    pub fn name(self) -> Key {
        match self {
            Lang::En => Key::LangEnglish,
            Lang::Tr => Key::LangTurkish,
        }
    }
}

/// Every user-visible string, by key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    // Identity and chrome
    AppTitle,
    MenuDocument,
    ActionAddDocument,
    ActionTheme,
    ActionExport,
    NavAbout,

    // Product list
    ListEmpty,
    BrokenPrefix,
    WarnPrefix,

    // Shown in place of a column that has nothing to say yet
    SelectPrompt,
    DetailsPlaceholder,

    // Document viewer
    NoDocuments,
    Rendering,
    PrevPage,
    NextPage,
    ZoomLabel,
    SerialLabel,
    Copied,
    // Glyphs. They carry no words, but they are on screen, so they live here
    // with everything else the user can see — same rule as Chron1's ⚠ prefix.
    PrevGlyph,
    NextGlyph,
    CopyGlyph,

    // Add and edit (Chron3)
    ActionEditDocument,
    FormAddTitle,
    FormEditTitle,
    FieldName,
    FieldLink,
    FieldPurchaseDate,
    FieldWarrantyStart,
    FieldWarrantyEnd,
    FieldDocuments,
    DateHint,
    ActionAddPdf,
    ActionSave,
    ActionCancel,
    RemoveGlyph,
    FilterPdf,
    Checking,
    NoDocumentsYet,
    ErrNameRequired,
    ErrDateInvalid,
    ErrWarrantyBackwards,
    ErrSaveFailed,

    // Details column and sorting (Chron4)
    WarrantyLeft,
    DayUnit,
    DaysUnit,
    WarrantyExpired,
    SortName,
    SortPurchase,
    SortByName,
    SortByPurchase,

    // Theming (Chron5). The eleven names are on screen, so they live here with
    // everything else the user can see — proper nouns included, same rule the
    // glyphs follow.
    ThemeTitle,
    ActionClose,
    CheckGlyph,
    ThemeDefaultLight,
    ThemeDefaultDark,
    ThemeNoctalia,
    ThemeCatppuccinLatte,
    ThemeCatppuccinFrappe,
    ThemeCatppuccinMacchiato,
    ThemeCatppuccinMocha,
    ThemeRosePine,
    ThemeRuby,
    ThemeUbuntuAubergine,
    ThemePaperlike,

    // Localization (Chron6)
    MenuLanguage,
    LangEnglish,
    LangTurkish,

    // Export (Chron7). The summary page reuses the field labels the form and the
    // details column already have; these are the ones only the export needs.
    ExportSaveTitle,
    ExportedOn,
    ExportSkipped,
    Exporting,
    ExportDone,
    ErrExportFailed,
    ErrExportWrite,
    ErrExportAssemble,

    // Broken and incomplete entries
    BrokenTitle,
    MissingFiles,
    ErrNoHome,
    ErrUnreadable,
    ErrMissingToml,
    ErrMalformed,
    ErrInvalidDate,
    ErrConfigSave,

    // Documents that will not open
    ErrPdfMissing,
    ErrNotAPdf,
    ErrEncrypted,
    ErrNoPages,
    ErrRenderFailed,

    // Search (Chron8). The query itself is user data and never translates; these
    // three are the chrome around it.
    SearchPlaceholder,
    SearchNoMatches,
    SearchClear,

    // About (Chron8). The wordmark, the maker's name, the two addresses and the
    // glyph are the same in both tables — see `SAME_IN_BOTH` for why each one is.
    // The version, the build date and the licence id are *not* here at all: they
    // come from `Cargo.toml` and `build.rs`, so writing them down again is the
    // one thing that could make the pane disagree with the manifest.
    AboutGlyph,
    AboutWordmark,
    AboutSubtitle,
    AboutMaker,
    AboutMakerName,
    AboutVersion,
    AboutReleased,
    AboutSource,
    AboutSourceUrl,
    AboutDocs,
    AboutDocsUrl,
    AboutNotLinks,
    AboutLicense,
    AboutReadLicense,
    AboutMotto,
}

impl Key {
    /// Every key, in declaration order.
    ///
    /// Single source of truth so the exhaustiveness test below cannot drift
    /// away from the enum — adding a key without adding it here is the one
    /// mistake this table invites. Only the tests walk it; that is the point.
    #[allow(dead_code)]
    pub const ALL: &'static [Key] = &[
        Key::AppTitle,
        Key::MenuDocument,
        Key::ActionAddDocument,
        Key::ActionTheme,
        Key::ActionExport,
        Key::NavAbout,
        Key::ListEmpty,
        Key::BrokenPrefix,
        Key::WarnPrefix,
        Key::SelectPrompt,
        Key::DetailsPlaceholder,
        Key::NoDocuments,
        Key::Rendering,
        Key::PrevPage,
        Key::NextPage,
        Key::ZoomLabel,
        Key::SerialLabel,
        Key::Copied,
        Key::PrevGlyph,
        Key::NextGlyph,
        Key::CopyGlyph,
        Key::ActionEditDocument,
        Key::FormAddTitle,
        Key::FormEditTitle,
        Key::FieldName,
        Key::FieldLink,
        Key::FieldPurchaseDate,
        Key::FieldWarrantyStart,
        Key::FieldWarrantyEnd,
        Key::FieldDocuments,
        Key::DateHint,
        Key::ActionAddPdf,
        Key::ActionSave,
        Key::ActionCancel,
        Key::RemoveGlyph,
        Key::FilterPdf,
        Key::Checking,
        Key::NoDocumentsYet,
        Key::ErrNameRequired,
        Key::ErrDateInvalid,
        Key::ErrWarrantyBackwards,
        Key::ErrSaveFailed,
        Key::WarrantyLeft,
        Key::DayUnit,
        Key::DaysUnit,
        Key::WarrantyExpired,
        Key::SortName,
        Key::SortPurchase,
        Key::SortByName,
        Key::SortByPurchase,
        Key::ThemeTitle,
        Key::ActionClose,
        Key::CheckGlyph,
        Key::ThemeDefaultLight,
        Key::ThemeDefaultDark,
        Key::ThemeNoctalia,
        Key::ThemeCatppuccinLatte,
        Key::ThemeCatppuccinFrappe,
        Key::ThemeCatppuccinMacchiato,
        Key::ThemeCatppuccinMocha,
        Key::ThemeRosePine,
        Key::ThemeRuby,
        Key::ThemeUbuntuAubergine,
        Key::ThemePaperlike,
        Key::MenuLanguage,
        Key::LangEnglish,
        Key::LangTurkish,
        Key::ExportSaveTitle,
        Key::ExportedOn,
        Key::ExportSkipped,
        Key::Exporting,
        Key::ExportDone,
        Key::ErrExportFailed,
        Key::ErrExportWrite,
        Key::ErrExportAssemble,
        Key::BrokenTitle,
        Key::MissingFiles,
        Key::ErrNoHome,
        Key::ErrUnreadable,
        Key::ErrMissingToml,
        Key::ErrMalformed,
        Key::ErrInvalidDate,
        Key::ErrConfigSave,
        Key::ErrPdfMissing,
        Key::ErrNotAPdf,
        Key::ErrEncrypted,
        Key::ErrNoPages,
        Key::ErrRenderFailed,
        Key::SearchPlaceholder,
        Key::SearchNoMatches,
        Key::SearchClear,
        Key::AboutGlyph,
        Key::AboutWordmark,
        Key::AboutSubtitle,
        Key::AboutMaker,
        Key::AboutMakerName,
        Key::AboutVersion,
        Key::AboutReleased,
        Key::AboutSource,
        Key::AboutSourceUrl,
        Key::AboutDocs,
        Key::AboutDocsUrl,
        Key::AboutNotLinks,
        Key::AboutLicense,
        Key::AboutReadLicense,
        Key::AboutMotto,
    ];
}

/// Look up `key` in `lang`.
pub fn get(lang: Lang, key: Key) -> &'static str {
    let (en, tr) = table(key);
    match lang {
        Lang::En => en,
        Lang::Tr => tr,
    }
}

/// The table itself: one `(english, turkish)` pair per key.
fn table(key: Key) -> (&'static str, &'static str) {
    use Key::*;
    match key {
        AppTitle => ("PARACHRON", "PARACHRON"),
        MenuDocument => ("Document", "Belge"),
        ActionAddDocument => ("Add Document", "Belge Ekle"),
        ActionTheme => ("THEME", "TEMA"),
        ActionExport => ("EXPORT", "DIŞA AKTAR"),
        NavAbout => ("About", "Hakkında"),

        ListEmpty => ("No products yet.", "Henüz ürün yok."),
        BrokenPrefix => ("⚠ ", "⚠ "),
        WarnPrefix => ("! ", "! "),

        SelectPrompt => (
            "Select a product from the list.",
            "Listeden bir ürün seçin.",
        ),
        DetailsPlaceholder => ("Details appear here.", "Detaylar burada görünür."),

        NoDocuments => (
            "This product has no documents yet.",
            "Bu ürüne henüz belge eklenmemiş.",
        ),
        Rendering => ("Opening…", "Açılıyor…"),
        PrevPage => ("Previous page", "Önceki sayfa"),
        NextPage => ("Next page", "Sonraki sayfa"),
        ZoomLabel => ("Zoom", "Yakınlaştırma"),
        SerialLabel => ("Serial number", "Seri numarası"),
        Copied => ("copied", "kopyalandı"),
        PrevGlyph => ("‹", "‹"),
        NextGlyph => ("›", "›"),
        CopyGlyph => ("⧉", "⧉"),

        ActionEditDocument => ("Edit Document…", "Belgeyi Düzenle…"),
        FormAddTitle => ("Add Document", "Belge Ekle"),
        FormEditTitle => ("Edit Document", "Belgeyi Düzenle"),
        FieldName => ("Name", "Ad"),
        FieldLink => ("Purchase link", "Satın alma bağlantısı"),
        FieldPurchaseDate => ("Purchase date", "Satın alma tarihi"),
        FieldWarrantyStart => ("Warranty start", "Garanti başlangıcı"),
        FieldWarrantyEnd => ("Warranty end", "Garanti bitişi"),
        FieldDocuments => ("Documents", "Belgeler"),
        // The date format, spelled the way it is typed. Translated because the
        // letters stand for words: day/month/year, gün/ay/yıl.
        DateHint => ("DD-MM-YYYY", "GG-AA-YYYY"),
        ActionAddPdf => ("Add PDF", "PDF Ekle"),
        ActionSave => ("Save", "Kaydet"),
        ActionCancel => ("Cancel", "İptal"),
        RemoveGlyph => ("✕", "✕"),
        // Shown in the file dialog's type filter, so it is on screen like
        // anything else — same rule as the glyphs above.
        FilterPdf => ("PDF", "PDF"),
        // `Denetleniyor` is what this said, and it means "being audited" — the
        // register of an inspection rather than of a program looking at a file.
        Checking => ("Checking…", "Kontrol ediliyor…"),
        NoDocumentsYet => (
            "No documents attached yet.",
            "Henüz belge eklenmedi.",
        ),
        ErrNameRequired => ("A name is required.", "Bir ad gerekli."),
        ErrDateInvalid => ("Not a real date.", "Geçerli bir tarih değil."),
        ErrWarrantyBackwards => (
            "The warranty ends before it starts.",
            "Garanti, başlamadan bitiyor.",
        ),
        ErrSaveFailed => ("Could not save", "Kaydedilemedi"),

        WarrantyLeft => ("Warranty left", "Kalan garanti"),
        // Turkish takes no plural agreement after a numeral — "658 gün", not
        // "658 günler" — so both units are the same word here. That is correct,
        // not a table entry somebody forgot to finish.
        DayUnit => ("day", "gün"),
        DaysUnit => ("days", "gün"),
        WarrantyExpired => ("Expired", "Süresi doldu"),
        SortName => ("A–Z", "A–Z"),
        SortPurchase => ("Date", "Tarih"),
        SortByName => ("Sort alphabetically", "Alfabetik sırala"),
        SortByPurchase => (
            "Sort by purchase date",
            "Satın alma tarihine göre sırala",
        ),

        // Title case, unlike `ActionTheme`'s shouting THEME. Both are stored as
        // they appear and never passed through `to_uppercase`: Turkish maps `i`
        // to `İ` and `ı` to `I`, so upper-casing in code gets `DIŞA AKTAR`
        // wrong. `data::fold` documents the same trap in the other direction.
        ThemeTitle => ("Theme", "Tema"),
        ActionClose => ("Close", "Kapat"),
        CheckGlyph => ("✓", "✓"),
        // The eleven names, in CORE §5's order. Only the two Default entries and
        // the two that carry an English common noun translate; the rest are
        // proper nouns and are the same in both tables on purpose.
        ThemeDefaultLight => ("Default Light", "Varsayılan Açık"),
        ThemeDefaultDark => ("Default Dark", "Varsayılan Koyu"),
        ThemeNoctalia => ("Noctalia", "Noctalia"),
        ThemeCatppuccinLatte => ("Catppuccin Latte", "Catppuccin Latte"),
        ThemeCatppuccinFrappe => ("Catppuccin Frappé", "Catppuccin Frappé"),
        ThemeCatppuccinMacchiato => ("Catppuccin Macchiato", "Catppuccin Macchiato"),
        ThemeCatppuccinMocha => ("Catppuccin Mocha", "Catppuccin Mocha"),
        ThemeRosePine => ("Rosé Pine", "Rosé Pine"),
        ThemeRuby => ("Ruby Theme", "Ruby Teması"),
        ThemeUbuntuAubergine => ("Ubuntu Canonical Aubergine", "Ubuntu Canonical Aubergine"),
        ThemePaperlike => (
            "Paperlike gradient theme",
            "Kâğıt görünümlü gradyan tema",
        ),

        MenuLanguage => ("Language", "Dil"),
        // Each language named in its own language, identically in both tables.
        // Somebody who has landed in a language they cannot read needs to find
        // their own name in the list, and `İngilizce` is no help to a reader of
        // English. A key whose two sides are equal is not an unfinished
        // translation — the glyph keys work the same way.
        LangEnglish => ("English", "English"),
        LangTurkish => ("Türkçe", "Türkçe"),

        // "Export" in Turkish is *dışa aktarmak*; bare *aktarmak* is "transfer".
        // These five all keep the particle, and that is a correction: three of
        // them dropped it, so a user pressed `DIŞA AKTAR`, got a dialog titled
        // *PDF Olarak Aktar*, watched *Aktarılıyor…* and on failure read *Dışa
        // aktarılamadı* — four wordings for one action.
        ExportSaveTitle => ("Export PDF", "PDF Olarak Dışa Aktar"),
        // On the summary page's footer, in front of the date. English reads as a
        // sentence, `Exported 06-08-2026`; Turkish reads as a label, so it takes
        // the colon English does not want. One of the few places the two languages
        // need different punctuation rather than different words.
        ExportedOn => ("Exported", "Dışa aktarma tarihi:"),
        ExportSkipped => ("Not included", "Eklenmeyen belgeler"),
        Exporting => ("Exporting…", "Dışa aktarılıyor…"),
        // The status line after a successful export. Deliberately not the same
        // word as `ExportedOn`, which sits on the page's footer in front of a date
        // — that one is a label on an artefact, this one is a thing that just
        // happened, and English would otherwise use "Exported" for both.
        ExportDone => ("Saved", "Kaydedildi"),
        ErrExportFailed => ("Could not export", "Dışa aktarılamadı"),
        // Which half of the export failed. Writing is the file; assembling is the
        // PDF. Neither is a fault in the user's own data, which is what the first
        // version of this said — every MuPDF error was reported as
        // `product.toml is not valid`.
        ErrExportWrite => ("the file could not be written", "dosya yazılamadı"),
        ErrExportAssemble => ("the PDF could not be built", "PDF oluşturulamadı"),

        BrokenTitle => ("Broken entry", "Bozuk kayıt"),
        MissingFiles => ("Missing files", "Eksik dosyalar"),
        ErrNoHome => (
            "No home directory, so the vault has nowhere to live",
            "Ev dizini bulunamadı, kasa için yer yok",
        ),
        ErrUnreadable => ("Could not be read", "Okunamadı"),
        ErrMissingToml => ("No product.toml in this folder", "Bu klasörde product.toml yok"),
        ErrMalformed => ("product.toml is not valid", "product.toml geçerli değil"),
        // Deliberately almost the same as `ErrDateInvalid`, and identical to it in
        // Turkish. They say the same thing to the user because the same thing is
        // wrong; they are two keys because one is a form refusing what was typed
        // (a sentence, with a full stop) and the other is a manifest field being
        // reported (a fragment, which gets the field name and the offending value
        // appended). Not a duplicate to collapse.
        ErrInvalidDate => ("Not a valid date", "Geçerli bir tarih değil"),
        ErrConfigSave => ("Could not save config.toml", "config.toml kaydedilemedi"),

        ErrPdfMissing => (
            "This file is not in the product folder",
            "Bu dosya ürün klasöründe yok",
        ),
        ErrNotAPdf => ("Not a readable PDF", "Okunabilir bir PDF değil"),
        ErrEncrypted => (
            "This PDF is password-protected",
            "Bu PDF parola korumalı",
        ),
        ErrNoPages => ("This PDF has no pages", "Bu PDF hiç sayfa içermiyor"),
        ErrRenderFailed => ("This page could not be shown", "Bu sayfa görüntülenemedi"),

        // An imperative in both, because it sits inside the box it describes and
        // is an instruction rather than a title. `ara` is the imperative of
        // *aramak*; the noun would be `arama`.
        SearchPlaceholder => ("Search products", "Ürünlerde ara"),
        // Deliberately not `ListEmpty`. That one means "there is nothing in your
        // vault", and telling somebody their vault is empty because they mistyped
        // four characters is the most alarming sentence this app could produce.
        SearchNoMatches => (
            "No products match your search.",
            "Aramanızla eşleşen ürün yok.",
        ),
        SearchClear => ("Clear search", "Aramayı temizle"),

        // The strip's glyph in CORE §4's wireframe: `[ⓘ About]`. A glyph, so the
        // same in both, like `‹` and `⧉`.
        AboutGlyph => ("ⓘ", "ⓘ"),
        // The wordmark, letter-spaced by the pane rather than by the string, so
        // the spacing is a layout decision and not something a translator could
        // accidentally change.
        AboutWordmark => ("P A R A C H R O N", "P A R A C H R O N"),
        // CORE §10, settled in Chron8.
        AboutSubtitle => ("Paper Vault", "Belge Kasası"),
        AboutMaker => ("Maker", "Yapımcı"),
        // A GitHub account name (CORE §1). Not a word in either language.
        AboutMakerName => ("sudo-megas", "sudo-megas"),
        AboutVersion => ("Version", "Sürüm"),
        AboutReleased => ("Release date", "Yayın tarihi"),
        AboutSource => ("Source code", "Kaynak kodu"),
        AboutSourceUrl => (
            "https://github.com/sudo-megas/PARACHRON",
            "https://github.com/sudo-megas/PARACHRON",
        ),
        // `Dokümantasyon` rather than `Belgeler`: `Belgeler` is already what
        // `FieldDocuments` calls a product's PDFs, and using it for a link to the
        // project's documentation would make one word mean two things in one app.
        AboutDocs => ("Docs", "Dokümantasyon"),
        AboutDocsUrl => (
            "https://github.com/sudo-megas/PARACHRON#readme",
            "https://github.com/sudo-megas/PARACHRON#readme",
        ),
        // CORE §4's app-wide principle, said to the user rather than only in the
        // specification: Parachron never opens an external address.
        AboutNotLinks => (
            "These addresses are not links. Parachron never opens external \
             addresses — copy one and paste it into your browser.",
            "Bu adresler bağlantı değildir. Parachron hiçbir zaman dış adres \
             açmaz — birini kopyalayıp tarayıcınıza yapıştırın.",
        ),
        AboutLicense => ("License", "Lisans"),
        AboutReadLicense => ("Read the full license", "Tam lisansı oku"),
        // CORE §10, settled in Chron8. JADEITE's motto, carried across as a
        // maker's signature rather than a second description of the app.
        AboutMotto => ("Built with Reason and Passion", "Akıl ve Tutkuyla"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_english_and_never_guesses() {
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("tr"), Lang::Tr);
        // Unknown, empty and locale-shaped values all land on English.
        assert_eq!(Lang::from_code("tr_TR.UTF-8"), Lang::En);
        assert_eq!(Lang::from_code(""), Lang::En);
    }

    #[test]
    fn every_key_has_both_languages() {
        for &key in Key::ALL {
            assert!(!get(Lang::En, key).is_empty(), "empty EN string for {key:?}");
            assert!(!get(Lang::Tr, key).is_empty(), "empty TR string for {key:?}");
        }
    }

    /// The keys whose two sides are meant to be the same string.
    ///
    /// Everything else has to differ, and the test below enforces it. Without a
    /// list like this, "the Turkish is missing" and "the Turkish is a proper noun"
    /// look identical from outside — which is how a table ends up half-translated
    /// with nothing to show for it.
    const SAME_IN_BOTH: &[Key] = &[
        // The wordmark is the wordmark (CORE §1).
        Key::AppTitle,
        // Glyphs carry no words. They are in the table because they are on screen.
        Key::BrokenPrefix,
        Key::WarnPrefix,
        Key::PrevGlyph,
        Key::NextGlyph,
        Key::CopyGlyph,
        Key::RemoveGlyph,
        Key::CheckGlyph,
        // A file type and a two-letter range, neither of which is a word.
        Key::FilterPdf,
        Key::SortName,
        // Theme names that are proper nouns (CORE §5). The two Default entries and
        // the two carrying an English common noun are not in this list, because
        // those do translate.
        Key::ThemeNoctalia,
        Key::ThemeCatppuccinLatte,
        Key::ThemeCatppuccinFrappe,
        Key::ThemeCatppuccinMacchiato,
        Key::ThemeCatppuccinMocha,
        Key::ThemeRosePine,
        Key::ThemeUbuntuAubergine,
        // Each language named in its own language, so a reader stranded in the
        // wrong one can recognise their own.
        Key::LangEnglish,
        Key::LangTurkish,
        // About (Chron8). The wordmark is the wordmark; `sudo-megas` is a GitHub
        // account name; the two addresses are addresses, and CORE §4 has the app
        // never open one, so they are shown as text and copied as text; and the
        // strip's glyph carries no word, like every other glyph above.
        Key::AboutWordmark,
        Key::AboutMakerName,
        Key::AboutSourceUrl,
        Key::AboutDocsUrl,
        Key::AboutGlyph,
    ];

    #[test]
    fn every_key_that_should_translate_does() {
        for &key in Key::ALL {
            let (en, tr) = (get(Lang::En, key), get(Lang::Tr, key));
            if SAME_IN_BOTH.contains(&key) {
                assert_eq!(en, tr, "{key:?} is listed as identical but is not");
            } else {
                assert_ne!(
                    en, tr,
                    "{key:?} has the same string in both languages — either translate \
                     it or add it to SAME_IN_BOTH with a reason"
                );
            }
        }
    }

    /// Turkish uppercases `i` to `İ` and `ı` to `I`, so a label that shouts has to
    /// be stored shouting. Passing these through `to_uppercase` in code would give
    /// `DIŞA AKTAR` a dotted capital and be wrong in a way English never is.
    #[test]
    fn the_shouting_labels_are_stored_shouting() {
        for key in [Key::ActionTheme, Key::ActionExport] {
            for lang in [Lang::En, Lang::Tr] {
                let text = get(lang, key);
                assert_eq!(
                    text,
                    text.to_uppercase(),
                    "{key:?} in {lang:?} is not stored in the case it is shown in"
                );
            }
        }
        // The specific trap, pinned: dotless I, because the stem is `dış`.
        assert_eq!(get(Lang::Tr, Key::ActionExport), "DIŞA AKTAR");
        assert!(!get(Lang::Tr, Key::ActionExport).contains('İ'));
    }

    /// Whether `keys` lists anything twice, adjacent or not.
    fn has_duplicate(keys: &[Key]) -> bool {
        let mut seen: Vec<Key> = keys.to_vec();
        let before = seen.len();
        seen.sort_by_key(|key| format!("{key:?}"));
        seen.dedup();
        before != seen.len()
    }

    /// The duplicate check has to catch a repeat that is not adjacent, which is
    /// the case the first version of it silently passed: `Vec::dedup` collapses
    /// consecutive equal elements only, so `[A, B, A]` survived it untouched.
    #[test]
    fn a_duplicate_is_caught_wherever_it_sits() {
        assert!(!has_duplicate(&[Key::AppTitle, Key::NavAbout, Key::ListEmpty]));
        // Adjacent — caught by the old version too.
        assert!(has_duplicate(&[Key::AppTitle, Key::AppTitle]));
        // Separated, which is the one that used to get through.
        assert!(has_duplicate(&[
            Key::AppTitle,
            Key::NavAbout,
            Key::ListEmpty,
            Key::AppTitle,
        ]));
    }

    #[test]
    fn the_key_list_covers_the_whole_enum() {
        // `table()` matches exhaustively, so a key missing from `Key::ALL` is
        // the only way a string can go untested. Catch it by counting.
        let mut seen: Vec<Key> = Key::ALL.to_vec();
        let before = seen.len();
        // Sorted before the dedup, which is the whole point of this line.
        // `Vec::dedup` removes *consecutive* repeats only, so the first version
        // of this test — a bare `dedup()` on the declaration-ordered list — would
        // have passed a key listed twice with anything at all between the two
        // copies. `Key` is not `Ord`, so sort by the discriminant's debug name,
        // which is stable and unique per variant.
        seen.sort_by_key(|key| format!("{key:?}"));
        seen.dedup();
        assert_eq!(before, seen.len(), "Key::ALL contains a duplicate");
        assert_eq!(
            Key::ALL.len(),
            106,
            "Key::ALL is out of step with the enum — add the new key to it"
        );
    }
}
