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

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::Deserialize;
use time::{Date, Month};

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
#[derive(Debug, Deserialize)]
struct RawProduct {
    name: String,
    serial: String,
    link: String,
    purchase_date: toml::value::Datetime,
    warranty_start: toml::value::Datetime,
    warranty_end: toml::value::Datetime,
    /// A product may legitimately have no documents attached yet.
    #[serde(default)]
    pdfs: Vec<String>,
    added: toml::value::Datetime,
}

/// A validated product (CORE §3 schema).
///
/// The struct mirrors the schema in full even though this milestone only
/// renders `name`: the viewer reads `pdfs` in Chron2 and the details column
/// reads the rest in Chron4.
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
    /// Files listed in `pdfs` that are not on disk — groundwork for Chron2 tabs.
    pub missing_pdfs: Vec<String>,
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

/// Read every product folder under `products`.
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

    sort_by_added(&mut entries);
    entries
}

/// CORE §4 default order: as added. Folders that failed to parse have no
/// readable `added`, so they settle at the end, ordered by folder name.
fn sort_by_added(entries: &mut [Entry]) {
    entries.sort_by(|a, b| match (a.added(), b.added()) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.folder().cmp(b.folder())),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.folder().cmp(b.folder()),
    });
}

/// Parse and validate one product folder.
fn load(dir: &Path, folder: &str) -> Result<Product, DataError> {
    let manifest = dir.join("product.toml");
    if !manifest.is_file() {
        return Err(DataError::MissingToml);
    }

    let text = fs::read_to_string(&manifest).map_err(|e| DataError::Unreadable(e.to_string()))?;
    let raw: RawProduct =
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
        let raw: RawProduct = toml::from_str(
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

        let manifest = toml::from_str::<RawProduct>(
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
}
