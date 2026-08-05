//! The data layer: Parachron's on-disk vault (CORE §3).
//!
//! One folder per product under `<data>/products/`, each holding a
//! `product.toml` plus that product's PDFs. The app scans the folder at
//! startup and builds its list from what actually exists — the data outlives
//! the app, so everything stays human-readable and rsync-friendly.
//!
//! Nothing in this module may panic on bad input. A folder that fails to parse
//! becomes an [`Entry::Broken`] and stays visible in the list with a readable
//! reason attached.

use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use time::{Date, Month};

/// Longest folder name `folder_slug` will produce.
const SLUG_MAX: usize = 64;
/// Folder name for a product whose name has no usable characters at all.
/// An identifier on disk, not a label anyone reads in the UI.
const SLUG_FALLBACK: &str = "product";

/// Why a product folder could not be turned into a [`Product`].
///
/// The variants are structured rather than pre-rendered strings so the UI can
/// translate them through the string table (CORE §4: no user-visible literals
/// outside `strings.rs`). The `String` payloads are diagnostic detail from the
/// OS or the TOML parser, not UI copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataError {
    /// No home directory could be determined, so the vault has nowhere to live.
    NoHome,
    /// A path could not be read; carries the OS message.
    Unreadable(String),
    /// The folder has no `product.toml`.
    MissingToml,
    /// `product.toml` is not valid TOML, or a required field is absent.
    Malformed(String),
    /// A date field is not a TOML date, or not a real calendar date.
    InvalidDate { field: &'static str, detail: String },
}

/// Where the vault lives.
///
/// CORE §3 fixes this layout: `config.toml` sits beside `products/` inside the
/// data dir, not in a separate config dir.
#[derive(Debug, Clone)]
pub struct Paths {
    pub data: PathBuf,
    pub products: PathBuf,
    pub config: PathBuf,
}

impl Paths {
    /// `~/.local/share/parachron/` on Linux, the platform equivalent elsewhere.
    ///
    /// The project path is pinned literally instead of being derived from a
    /// qualifier/organisation triple, so the directory is named exactly what
    /// CORE §3 documents on every platform.
    pub fn resolve() -> Result<Self, DataError> {
        let dirs = ProjectDirs::from_path(PathBuf::from("parachron")).ok_or(DataError::NoHome)?;
        let data = dirs.data_dir().to_path_buf();
        Ok(Self {
            products: data.join("products"),
            config: data.join("config.toml"),
            data,
        })
    }

    /// Create the vault on first run. Existing directories are left alone.
    pub fn ensure(&self) -> Result<(), DataError> {
        fs::create_dir_all(&self.products).map_err(|e| DataError::Unreadable(e.to_string()))
    }
}

/// A product folder as it appears on disk, before validation.
///
/// Dates arrive as [`toml::value::Datetime`] because that is what native TOML
/// dates deserialize into; [`to_date`] turns them into calendar dates and
/// rejects anything that is not one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub serial: String,
    pub link: String,
    pub purchase_date: toml::value::Datetime,
    pub warranty_start: toml::value::Datetime,
    pub warranty_end: toml::value::Datetime,
    /// A product may legitimately have no documents attached yet.
    #[serde(default)]
    pub pdfs: Vec<String>,
    pub added: toml::value::Datetime,
    /// Everything in the file that Parachron does not have a field for.
    ///
    /// CORE §3 promises the vault outlives the app, which has to mean the app
    /// does not quietly delete what it did not put there: somebody's
    /// hand-added `notes = "..."` survives being edited in the form.
    ///
    /// Declared last on purpose — serde emits fields in declaration order, so
    /// anywhere else would shuffle unknown keys above `name` and rewrite the
    /// documented shape of the file. Never insert a *known* key here: the file
    /// would gain a duplicate and stop parsing.
    #[serde(flatten, default, skip_serializing_if = "toml::Table::is_empty")]
    pub extra: toml::Table,
}

/// A validated product (CORE §3 schema).
///
/// The struct mirrors the schema in full. `link`, `warranty_start` and
/// `warranty_end` have no reader until the details column in Chron4, which is
/// what the allowance below is for; it comes off there.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Product {
    /// Folder name under `products/` — the stable identity of the product.
    pub folder: String,
    pub name: String,
    pub serial: String,
    pub link: String,
    pub purchase_date: Date,
    pub warranty_start: Date,
    pub warranty_end: Date,
    /// Order here is tab order in the viewer (Chron2).
    pub pdfs: Vec<String>,
    pub added: Date,
    /// Files listed in `pdfs` that are not on disk (Chron2 tabs).
    pub missing_pdfs: Vec<String>,
    /// Keys the manifest carried that Parachron has no field for, kept so
    /// editing the product writes them back (see [`Manifest::extra`]).
    pub extra: toml::Table,
}

impl Product {
    /// Absolute path to one of this product's documents.
    ///
    /// Product folders are addressed by `folder`, not by `name` — the display
    /// name is free to change or repeat, the folder is the identity.
    pub fn document_path(&self, products_root: &Path, file: &str) -> PathBuf {
        products_root.join(&self.folder).join(file)
    }
}

/// A product folder, valid or not.
///
/// Broken folders are surfaced, never hidden (CORE §3). This enum carries
/// through every later Chron.
#[derive(Debug, Clone)]
pub enum Entry {
    Ok(Product),
    Broken { folder: String, reason: DataError },
}

impl Entry {
    pub fn folder(&self) -> &str {
        match self {
            Entry::Ok(p) => &p.folder,
            Entry::Broken { folder, .. } => folder,
        }
    }

    /// The insertion-order sort key, absent for folders that failed to parse.
    pub fn added(&self) -> Option<Date> {
        match self {
            Entry::Ok(p) => Some(p.added),
            Entry::Broken { .. } => None,
        }
    }
}

/// Read every product folder under `products`, in whatever order the
/// filesystem hands them over.
///
/// Ordering is not this module's business: `vault` owns list order because it
/// owns the sort mode, and sorting here as well would be work always thrown
/// away and a second answer to "why is this product here".
///
/// Never fails: an unreadable `products/` directory comes back as a single
/// broken entry rather than an error the caller has to handle at startup.
pub fn scan(products: &Path) -> Vec<Entry> {
    let listing = match fs::read_dir(products) {
        Ok(listing) => listing,
        Err(e) => {
            return vec![Entry::Broken {
                folder: products.display().to_string(),
                reason: DataError::Unreadable(e.to_string()),
            }];
        }
    };

    let mut entries: Vec<Entry> = Vec::new();
    for dirent in listing.flatten() {
        let path = dirent.path();
        if !path.is_dir() {
            continue;
        }
        let folder = dirent.file_name().to_string_lossy().into_owned();
        entries.push(match load(&path, &folder) {
            Ok(product) => Entry::Ok(product),
            Err(reason) => Entry::Broken { folder, reason },
        });
    }

    entries
}

/// Parse and validate one product folder.
fn load(dir: &Path, folder: &str) -> Result<Product, DataError> {
    let manifest = dir.join("product.toml");
    if !manifest.is_file() {
        return Err(DataError::MissingToml);
    }

    let text = fs::read_to_string(&manifest).map_err(|e| DataError::Unreadable(e.to_string()))?;
    let raw: Manifest =
        toml::from_str(&text).map_err(|e| DataError::Malformed(first_line(&e.to_string())))?;

    // Files the manifest promises but the folder does not hold.
    let missing_pdfs: Vec<String> = raw
        .pdfs
        .iter()
        .filter(|name| !dir.join(name).is_file())
        .cloned()
        .collect();

    Ok(Product {
        folder: folder.to_string(),
        name: raw.name,
        serial: raw.serial,
        link: raw.link,
        purchase_date: to_date(&raw.purchase_date, "purchase_date")?,
        warranty_start: to_date(&raw.warranty_start, "warranty_start")?,
        warranty_end: to_date(&raw.warranty_end, "warranty_end")?,
        added: to_date(&raw.added, "added")?,
        pdfs: raw.pdfs,
        missing_pdfs,
        extra: raw.extra,
    })
}

/// Convert a TOML datetime into a calendar date.
///
/// A bare time, or a datetime whose components do not form a real date, is
/// rejected rather than coerced.
fn to_date(value: &toml::value::Datetime, field: &'static str) -> Result<Date, DataError> {
    let invalid = || DataError::InvalidDate {
        field,
        detail: value.to_string(),
    };
    let date = value.date.ok_or_else(invalid)?;
    let month = Month::try_from(date.month).map_err(|_| invalid())?;
    Date::from_calendar_date(i32::from(date.year), month, date.day).map_err(|_| invalid())
}

/// Parser messages span several lines; a list row only has space for the first.
fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or_default().trim().to_string()
}

/// Render a date the way Parachron shows dates: `DD-MM-YYYY` (CORE §3).
///
/// Storage stays ISO — this is the display half of that rule and the single
/// place the conversion happens. Its first caller in the UI is the details
/// column in Chron4; the tests below pin the behaviour until then.
#[allow(dead_code)]
pub fn fmt_date(date: Date) -> String {
    let format = time::macros::format_description!("[day]-[month]-[year]");
    date.format(&format).unwrap_or_default()
}

/// Read a date typed the way Parachron displays them: `DD-MM-YYYY`.
///
/// The inverse of [`fmt_date`], and the only place typed dates are understood.
/// `.` and `/` are accepted as separators and normalised first, because those
/// are what people actually type; refusing them would be pedantry with no
/// upside. A single-digit day or month is accepted for the same reason.
pub fn parse_date(text: &str) -> Option<Date> {
    let normalised: String = text
        .trim()
        .chars()
        .map(|ch| if ch == '.' || ch == '/' { '-' } else { ch })
        .collect();

    let padded = time::macros::format_description!("[day]-[month]-[year]");
    if let Ok(date) = Date::parse(&normalised, &padded) {
        return Some(date);
    }
    let loose =
        time::macros::format_description!("[day padding:none]-[month padding:none]-[year]");
    Date::parse(&normalised, &loose).ok()
}

/// The inverse of [`to_date`]: a calendar date the way TOML stores one.
///
/// CORE §3 is explicit that storage is ISO `YYYY-MM-DD` and `DD-MM-YYYY` is a
/// display format that must never reach a `.toml` file. Going through
/// [`toml::value::Datetime`] rather than formatting a string by hand is what
/// makes that structural rather than a rule to remember.
pub fn to_datetime(date: Date) -> toml::value::Datetime {
    toml::value::Datetime {
        date: Some(toml::value::Date {
            year: date.year().clamp(0, i32::from(u16::MAX)) as u16,
            month: date.month() as u8,
            day: date.day(),
        }),
        time: None,
        offset: None,
    }
}

/// Replace a file's contents without ever leaving it half-written.
///
/// The temporary lives in the target's own directory because a rename across
/// filesystems fails, and the rename is what makes this atomic: a crash leaves
/// either the whole old file or the whole new one. CORE §3 says a malformed
/// manifest must never crash the app; not writing one in the first place is the
/// better half of that promise.
pub fn write_atomic(path: &Path, contents: &str) -> Result<(), DataError> {
    let unreadable = |e: std::io::Error| DataError::Unreadable(e.to_string());

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.tmp"));

    fs::write(&tmp, contents).map_err(unreadable)?;
    fs::rename(&tmp, path).map_err(|e| {
        // Leaving the temporary behind would look like a product folder's
        // business next time somebody opened the directory.
        let _ = fs::remove_file(&tmp);
        unreadable(e)
    })
}

/// Write one product's manifest.
pub fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<(), DataError> {
    let text = toml::to_string_pretty(manifest)
        .map_err(|e| DataError::Malformed(first_line(&e.to_string())))?;
    write_atomic(&dir.join("product.toml"), &text)
}

/// Fold a letter to its ASCII shape before it is lowercased.
///
/// Rust's `to_lowercase` turns `İ` into `i` followed by a combining dot above.
/// A combining mark inside a directory name is mojibake in a file manager and
/// normalises differently on other platforms, so the Turkish letters are mapped
/// explicitly — the language is first-class here (CORE §4) and its users should
/// not get worse folder names than English ones. The common Latin-1 accents
/// come along for the same price.
fn fold(ch: char) -> Option<char> {
    Some(match ch {
        'ç' | 'Ç' => 'c',
        'ğ' | 'Ğ' => 'g',
        'ı' | 'İ' => 'i',
        'ö' | 'Ö' => 'o',
        'ş' | 'Ş' => 's',
        'ü' | 'Ü' => 'u',
        'á' | 'Á' | 'à' | 'À' | 'â' | 'Â' | 'ä' | 'Ä' | 'å' | 'Å' | 'ã' | 'Ã' => 'a',
        'é' | 'É' | 'è' | 'È' | 'ê' | 'Ê' | 'ë' | 'Ë' => 'e',
        'í' | 'Í' | 'ì' | 'Ì' | 'î' | 'Î' | 'ï' | 'Ï' => 'i',
        'ó' | 'Ó' | 'ò' | 'Ò' | 'ô' | 'Ô' | 'õ' | 'Õ' => 'o',
        'ú' | 'Ú' | 'ù' | 'Ù' | 'û' | 'Û' => 'u',
        'ñ' | 'Ñ' => 'n',
        'ß' => 's',
        _ => return None,
    })
}

/// Windows reserves these as device names and refuses to create a directory
/// using one. CORE §7 ships a Windows binary, so a vault that syncs onto one
/// must not contain a folder it cannot open.
fn is_reserved(slug: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    RESERVED.contains(&slug)
}

/// Turn a product name into a folder name.
///
/// The result is ASCII alphanumerics and single hyphens, which sidesteps
/// Windows' illegal characters and its refusal of trailing dots and spaces by
/// construction rather than by checking for them.
pub fn folder_slug(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for ch in name.chars() {
        let mapped = fold(ch).unwrap_or(ch);
        if mapped.is_ascii_alphanumeric() {
            slug.push(mapped.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }

    // ASCII by construction, so truncating cannot split a character.
    slug.truncate(SLUG_MAX);
    let slug = slug.trim_matches('-');

    if slug.is_empty() {
        SLUG_FALLBACK.to_string()
    } else if is_reserved(slug) {
        format!("{slug}-{SLUG_FALLBACK}")
    } else {
        slug.to_string()
    }
}

/// A folder name under `products_root` that nothing else has taken.
///
/// Two products may legitimately share a name — a spare of the same monitor —
/// so a collision is numbered rather than refused.
pub fn unique_folder(products_root: &Path, slug: &str) -> String {
    if !products_root.join(slug).exists() {
        return slug.to_string();
    }
    for n in 2..=9999 {
        let candidate = format!("{slug}-{n}");
        if !products_root.join(&candidate).exists() {
            return candidate;
        }
    }
    // Ten thousand products of the same name. Hand back the last candidate and
    // let the failure to create it speak for itself, in the OS's own words.
    format!("{slug}-9999")
}

/// A product that has passed validation and is ready to be written.
///
/// The form produces one of these, so nothing half-typed ever reaches the
/// writer, and the writer needs no opinion about what a valid date looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub name: String,
    pub serial: String,
    pub link: String,
    pub purchase_date: Date,
    pub warranty_start: Date,
    pub warranty_end: Date,
    pub pdfs: Vec<String>,
    pub added: Date,
    /// Carried through from the manifest that was read, so editing a product
    /// does not drop keys the app has no field for.
    pub extra: toml::Table,
}

impl Draft {
    /// The on-disk shape of this draft.
    pub fn manifest(&self) -> Manifest {
        Manifest {
            name: self.name.clone(),
            serial: self.serial.clone(),
            link: self.link.clone(),
            purchase_date: to_datetime(self.purchase_date),
            warranty_start: to_datetime(self.warranty_start),
            warranty_end: to_datetime(self.warranty_end),
            pdfs: self.pdfs.clone(),
            added: to_datetime(self.added),
            extra: self.extra.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    #[test]
    fn dates_display_as_day_month_year() {
        let date = Date::from_calendar_date(2026, Month::March, 14).unwrap();
        assert_eq!(fmt_date(date), "14-03-2026");
    }

    #[test]
    fn display_pads_single_digit_day_and_month() {
        let date = Date::from_calendar_date(2026, Month::August, 5).unwrap();
        assert_eq!(fmt_date(date), "05-08-2026");
    }

    #[test]
    fn iso_dates_from_toml_round_trip_to_the_display_format() {
        // The exact schema CORE §3 documents.
        let raw: Manifest = toml::from_str(
            r#"
            name = "QD-OLED Monitor"
            serial = "ABC123XYZ"
            link = "https://store.example/p"
            purchase_date = 2026-03-14
            warranty_start = 2026-03-14
            warranty_end = 2029-03-14
            pdfs = ["invoice.pdf", "warranty.pdf"]
            added = 2026-08-05
        "#,
        )
        .expect("CORE §3 schema must parse");

        let purchase = to_date(&raw.purchase_date, "purchase_date").unwrap();
        assert_eq!(fmt_date(purchase), "14-03-2026");
        assert_eq!(fmt_date(to_date(&raw.added, "added").unwrap()), "05-08-2026");
    }

    #[test]
    fn impossible_calendar_dates_never_reach_a_product() {
        // The TOML layer validates calendar dates itself — 30 February is
        // refused while parsing — so a manifest carrying one surfaces as
        // `Malformed` rather than `InvalidDate`. Either way the folder is
        // flagged and the app keeps running; the calendar check in `to_date`
        // stays as the second line of defence.
        assert!("2026-02-30".parse::<toml::value::Datetime>().is_err());

        let manifest = toml::from_str::<Manifest>(
            r#"
            name = "Broken"
            serial = "X"
            link = ""
            purchase_date = 2026-02-30
            warranty_start = 2026-03-14
            warranty_end = 2029-03-14
            added = 2026-08-05
        "#,
        );
        assert!(manifest.is_err());
    }

    #[test]
    fn a_folder_without_a_manifest_is_broken_not_hidden() {
        let dir = Path::new("/nonexistent/parachron/products/no-manifest");
        assert!(matches!(
            load(dir, "no-manifest"),
            Err(DataError::MissingToml)
        ));
    }

    #[test]
    fn a_bare_time_is_not_a_date() {
        let raw: toml::value::Datetime = "07:32:00".parse().unwrap();
        assert!(matches!(
            to_date(&raw, "added"),
            Err(DataError::InvalidDate { field: "added", .. })
        ));
    }

    #[test]
    fn scanning_a_missing_directory_yields_one_broken_entry_not_a_panic() {
        let entries = scan(Path::new("/nonexistent/parachron/products"));
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0],
            Entry::Broken {
                reason: DataError::Unreadable(_),
                ..
            }
        ));
    }

    // ── The write half (Chron3) ──────────────────────────────────────────

    fn draft(name: &str) -> Draft {
        let date = Date::from_calendar_date(2026, Month::March, 14).unwrap();
        Draft {
            name: name.to_string(),
            serial: "ABC123XYZ".to_string(),
            link: "https://store.example/p".to_string(),
            purchase_date: date,
            warranty_start: date,
            warranty_end: Date::from_calendar_date(2029, Month::March, 14).unwrap(),
            pdfs: vec!["invoice.pdf".to_string()],
            added: Date::from_calendar_date(2026, Month::August, 5).unwrap(),
            extra: Default::default(),
        }
    }

    #[test]
    fn a_slug_is_lowercase_ascii_with_single_hyphens() {
        assert_eq!(folder_slug("QD-OLED Monitor"), "qd-oled-monitor");
        assert_eq!(folder_slug("IronWolf Pro 6TB"), "ironwolf-pro-6tb");
        assert_eq!(folder_slug("  spaced   out  "), "spaced-out");
        assert_eq!(folder_slug("Dell // U2724D!!"), "dell-u2724d");
    }

    /// Turkish is a first-class language here (CORE §4), so its letters have to
    /// produce folder names as good as English ones.
    #[test]
    fn turkish_letters_fold_to_ascii_without_combining_marks() {
        assert_eq!(folder_slug("Şarj Cihazı"), "sarj-cihazi");
        assert_eq!(folder_slug("Ürün Güncesi"), "urun-guncesi");
        assert_eq!(folder_slug("Öğrenci"), "ogrenci");

        // The trap: `"İ".to_lowercase()` is `i` plus U+0307, a combining dot
        // that would end up inside a directory name.
        let slug = folder_slug("İphone");
        assert_eq!(slug, "iphone");
        assert!(
            slug.is_ascii(),
            "a slug must be pure ASCII, got {slug:?} which contains {:?}",
            slug.chars().filter(|c| !c.is_ascii()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_name_with_nothing_usable_still_yields_a_folder() {
        assert_eq!(folder_slug(""), SLUG_FALLBACK);
        assert_eq!(folder_slug("!!! ???"), SLUG_FALLBACK);
        assert_eq!(folder_slug("---"), SLUG_FALLBACK);
        // Non-Latin scripts have no ASCII fold here, so they land on the
        // fallback rather than on an empty path.
        assert_eq!(folder_slug("日本語"), SLUG_FALLBACK);
    }

    /// CORE §7 ships a Windows binary, and Windows will not create a directory
    /// named after one of its devices.
    #[test]
    fn windows_reserved_names_are_not_used_as_folders() {
        for reserved in ["CON", "nul", "COM1", "lpt9", "AUX", "prn"] {
            let slug = folder_slug(reserved);
            assert!(
                !is_reserved(&slug),
                "{reserved} slugged to {slug}, which Windows reserves"
            );
        }
        assert_eq!(folder_slug("CON"), "con-product");
    }

    #[test]
    fn a_slug_is_bounded_and_never_ends_in_a_hyphen() {
        let slug = folder_slug(&"very long name ".repeat(40));
        assert!(slug.len() <= SLUG_MAX, "{} chars", slug.len());
        assert!(!slug.ends_with('-'));
        assert!(!slug.starts_with('-'));
        // Trailing dots and spaces are impossible by construction, which is
        // the other thing Windows refuses.
        assert!(slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }

    #[test]
    fn colliding_folder_names_are_numbered_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        assert_eq!(unique_folder(root, "monitor"), "monitor");
        fs::create_dir(root.join("monitor")).unwrap();
        assert_eq!(unique_folder(root, "monitor"), "monitor-2");
        fs::create_dir(root.join("monitor-2")).unwrap();
        assert_eq!(unique_folder(root, "monitor-3"), "monitor-3");
        assert_eq!(unique_folder(root, "monitor"), "monitor-3");
    }

    #[test]
    fn typed_dates_round_trip_through_the_display_format() {
        let date = Date::from_calendar_date(2026, Month::March, 14).unwrap();
        assert_eq!(parse_date("14-03-2026"), Some(date));
        assert_eq!(parse_date(&fmt_date(date)), Some(date));

        // What people actually type.
        assert_eq!(parse_date("14.03.2026"), Some(date));
        assert_eq!(parse_date("14/03/2026"), Some(date));
        assert_eq!(parse_date("  14-03-2026 "), Some(date));
        assert_eq!(parse_date("1-3-2026"), parse_date("01-03-2026"));
    }

    #[test]
    fn a_date_that_is_not_a_date_is_refused_rather_than_guessed_at() {
        for text in [
            "", "tomorrow", "2026-03-14", // ISO is the storage format, not the input one
            "31-02-2026", // no such day
            "14-13-2026", // no such month
            "14-03",
        ] {
            assert_eq!(parse_date(text), None, "{text:?} should not parse");
        }
    }

    #[test]
    fn manifests_are_written_as_iso_dates_never_as_display_dates() {
        let text = toml::to_string_pretty(&draft("QD-OLED Monitor").manifest()).unwrap();
        assert!(text.contains("purchase_date = 2026-03-14"), "{text}");
        assert!(text.contains("warranty_end = 2029-03-14"), "{text}");
        assert!(
            !text.contains("14-03-2026"),
            "CORE §3: a display date must never reach a .toml file\n{text}"
        );
    }

    /// CORE §3 promises the vault outlives the app, so a key somebody added by
    /// hand has to survive the app rewriting the file.
    #[test]
    fn unknown_keys_survive_a_rewrite() {
        let original = r#"
name = "QD-OLED Monitor"
serial = "ABC123XYZ"
link = "https://store.example/p"
purchase_date = 2026-03-14
warranty_start = 2026-03-14
warranty_end = 2029-03-14
pdfs = ["invoice.pdf"]
added = 2026-08-05
notes = "extended warranty claim ref #4471"
last_checked = 2026-07-01
"#;
        let mut manifest: Manifest = toml::from_str(original).unwrap();
        assert_eq!(manifest.extra.len(), 2, "unknown keys are kept, not dropped");

        // Edit something the app does own, then write it back.
        manifest.pdfs.push("warranty.pdf".to_string());
        let rewritten = toml::to_string_pretty(&manifest).unwrap();

        let reloaded: Manifest = toml::from_str(&rewritten).unwrap();
        assert_eq!(reloaded.pdfs.len(), 2);
        assert_eq!(
            reloaded.extra.get("notes").and_then(|v| v.as_str()),
            Some("extended warranty claim ref #4471"),
        );
        // A date among the unknown keys is still a date, not a stringified one.
        assert!(
            reloaded.extra.get("last_checked").is_some_and(|v| v.is_datetime()),
            "an unknown date key must stay a TOML date: {:?}",
            reloaded.extra.get("last_checked")
        );
        // Known keys keep their documented order ahead of the extras.
        let name_at = rewritten.find("name =").unwrap();
        let notes_at = rewritten.find("notes =").unwrap();
        assert!(name_at < notes_at, "extras must not shuffle above the schema");
    }

    #[test]
    fn an_atomic_write_replaces_the_file_and_leaves_no_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("product.toml");

        write_atomic(&path, "first").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");

        write_atomic(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "product.toml")
            .collect();
        assert!(leftovers.is_empty(), "temporary files left behind: {leftovers:?}");
    }

    /// The round trip that matters: what the form writes is what the list reads.
    #[test]
    fn a_written_product_scans_back_as_the_same_product() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        let folder = folder_slug("QD-OLED Monitor");
        let home = products.join(&folder);
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("invoice.pdf"), b"%PDF-1.4\n").unwrap();

        let draft = draft("QD-OLED Monitor");
        write_manifest(&home, &draft.manifest()).unwrap();

        let entries = scan(&products);
        assert_eq!(entries.len(), 1);
        let Entry::Ok(product) = &entries[0] else {
            panic!("a manifest we just wrote must parse: {:?}", entries[0]);
        };

        assert_eq!(product.folder, folder);
        assert_eq!(product.name, draft.name);
        assert_eq!(product.serial, draft.serial);
        assert_eq!(product.link, draft.link);
        assert_eq!(product.purchase_date, draft.purchase_date);
        assert_eq!(product.warranty_end, draft.warranty_end);
        assert_eq!(product.added, draft.added);
        assert_eq!(product.pdfs, draft.pdfs);
        assert!(
            product.missing_pdfs.is_empty(),
            "the file is on disk, so nothing is missing"
        );
    }

    /// A crash between copying the PDFs and writing the manifest has to leave
    /// something visible and repairable, not a silent half-product.
    #[test]
    fn a_folder_written_without_its_manifest_shows_up_broken() {
        let dir = tempfile::tempdir().unwrap();
        let products = dir.path().join("products");
        let home = products.join("interrupted");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("invoice.pdf"), b"%PDF-1.4\n").unwrap();

        let entries = scan(&products);
        assert!(matches!(
            entries.as_slice(),
            [Entry::Broken {
                reason: DataError::MissingToml,
                ..
            }]
        ));
    }
}
