//! The string table.
//!
//! CORE §4: every user-visible string in Parachron lives here and nowhere
//! else — no literals in `.slint` files, none in the rest of the Rust code.
//! English is complete; Turkish keys are all present and may lag until Chron6.

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

    // Placeholders (the details column is fleshed out in Chron4)
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
        Checking => ("Checking…", "Denetleniyor…"),
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

        BrokenTitle => ("Broken entry", "Bozuk kayıt"),
        MissingFiles => ("Missing files", "Eksik dosyalar"),
        ErrNoHome => (
            "No home directory, so the vault has nowhere to live",
            "Ev dizini bulunamadı, kasa için yer yok",
        ),
        ErrUnreadable => ("Could not be read", "Okunamadı"),
        ErrMissingToml => ("No product.toml in this folder", "Bu klasörde product.toml yok"),
        ErrMalformed => ("product.toml is not valid", "product.toml geçerli değil"),
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

    #[test]
    fn the_key_list_covers_the_whole_enum() {
        // `table()` matches exhaustively, so a key missing from `Key::ALL` is
        // the only way a string can go untested. Catch it by counting.
        let mut seen: Vec<Key> = Key::ALL.to_vec();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "Key::ALL contains a duplicate");
        assert_eq!(
            Key::ALL.len(),
            77,
            "Key::ALL is out of step with the enum — add the new key to it"
        );
    }
}
