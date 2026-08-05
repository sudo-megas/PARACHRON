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

    // Placeholders (fleshed out in Chron2 and Chron4)
    SelectPrompt,
    DetailsPlaceholder,

    // Broken and incomplete entries
    BrokenTitle,
    MissingFiles,
    ErrNoHome,
    ErrUnreadable,
    ErrMissingToml,
    ErrMalformed,
    ErrInvalidDate,
    ErrConfigSave,
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
        let keys = [
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
            Key::BrokenTitle,
            Key::MissingFiles,
            Key::ErrNoHome,
            Key::ErrUnreadable,
            Key::ErrMissingToml,
            Key::ErrMalformed,
            Key::ErrInvalidDate,
            Key::ErrConfigSave,
        ];
        for key in keys {
            assert!(!get(Lang::En, key).is_empty(), "empty EN string for {key:?}");
            assert!(!get(Lang::Tr, key).is_empty(), "empty TR string for {key:?}");
        }
    }
}
