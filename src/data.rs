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
use std::io::Write;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use time::{Date, Month, OffsetDateTime, UtcOffset};

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
    /// `config.toml` names a vault that is not there (Chron9).
    ///
    /// Carries the configured path, because naming it is the entire point: the
    /// likeliest cause is a drive that has not been mounted, and a user can act
    /// on "`/mnt/ironwolf/parachron` is not there" and cannot act on "no
    /// products found".
    VaultMissing(String),
    /// `config.toml` itself could not be parsed, so the vault it names is
    /// unknown (Chron9).
    ///
    /// Distinct from a config with no `vault` key, which is the ordinary case
    /// and means the default. See `config::Config::load`.
    ConfigUnreadable(String),
}

/// Where the vault lives.
///
/// CORE §3 fixed this layout as one directory holding both `config.toml` and
/// `products/`. Chron9 split it in two, because a `vault` key cannot live inside
/// the vault it names — the app would need the location in order to read the
/// setting that gives it the location. So `data` is the platform's data
/// directory and never moves, and `vault` is wherever the products are, which is
/// `data` until somebody says otherwise.
#[derive(Debug, Clone)]
pub struct Paths {
    /// The platform data directory. `config.toml` lives here, always.
    pub data: PathBuf,
    /// The directory holding `products/`. Equal to `data` by default.
    pub vault: PathBuf,
    /// `vault/products`.
    pub products: PathBuf,
    /// `data/config.toml`.
    pub config: PathBuf,
    /// Whether `vault` came out of `config.toml` rather than being the default.
    ///
    /// This is what decides whether [`Paths::ensure`] creates or checks, and it
    /// is the difference between a first run and a drive that is not mounted.
    configured: bool,
}

impl Paths {
    /// `~/.local/share/parachron/` on Linux, the platform equivalent elsewhere.
    ///
    /// The project path is pinned literally instead of being derived from a
    /// qualifier/organisation triple, so the directory is named exactly what
    /// CORE §3 documents on every platform.
    ///
    /// The vault starts as the data directory. [`Paths::with_vault`] moves it,
    /// and is called once `config.toml` has been read.
    pub fn resolve() -> Result<Self, DataError> {
        let dirs = ProjectDirs::from_path(PathBuf::from("parachron")).ok_or(DataError::NoHome)?;
        let data = dirs.data_dir().to_path_buf();
        Ok(Self::rooted(data))
    }

    /// A `Paths` rooted anywhere, for tests that must not reach a real home.
    ///
    /// `resolve` goes through `ProjectDirs` and therefore through `$HOME`, which
    /// is the one thing a test may not touch — `ui_tests` in particular installs
    /// the whole stack and would otherwise scan the machine's own vault.
    #[cfg(test)]
    pub fn for_test(data: PathBuf) -> Self {
        Self::rooted(data)
    }

    /// The default layout for a given data directory: vault and data are one.
    fn rooted(data: PathBuf) -> Self {
        Self {
            products: data.join("products"),
            config: data.join("config.toml"),
            vault: data.clone(),
            data,
            configured: false,
        }
    }

    /// Point `products/` at a vault the user chose (Chron9).
    ///
    /// `None`, or a path that is empty once trimmed, means the default — which
    /// is the ordinary case and resolves to exactly what every install had
    /// before this existed. `config.toml` does not move whatever is passed here.
    pub fn with_vault(self, vault: Option<&str>) -> Self {
        let Some(vault) = vault.map(str::trim).filter(|v| !v.is_empty()) else {
            return self;
        };
        let vault = PathBuf::from(vault);
        Self {
            products: vault.join("products"),
            vault,
            configured: true,
            ..self
        }
    }

    /// Whether the vault is one the user chose rather than the default.
    pub fn is_configured(&self) -> bool {
        self.configured
    }

    /// Make the vault usable, or say why it is not.
    ///
    /// **The default vault is created; a configured one is only ever checked.**
    /// This is the rule Chron9 turns on, and it is about a drive that is not
    /// mounted. If `vault` names a path under a mount point and the drive is not
    /// there, that mount point is an ordinary empty directory on the root
    /// filesystem — `create_dir_all` would succeed against it without complaint,
    /// the app would build a vault on the system disk, and its owner would file
    /// documents there believing they were on the drive they bought for exactly
    /// this. Mounting the drive afterwards hides the lot underneath it: still on
    /// disk, entirely invisible, and impossible to explain to somebody who did
    /// nothing wrong.
    ///
    /// So a missing configured vault is reported and nothing is written. The
    /// default is created because its parent is the platform's own data
    /// directory, which exists on any machine that has a home at all, and
    /// creating it on first run is what Parachron has always done.
    pub fn ensure(&self) -> Result<(), DataError> {
        if self.configured && !self.vault.is_dir() {
            return Err(DataError::VaultMissing(self.vault.display().to_string()));
        }
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
/// The struct mirrors the schema in full. It carried an `#[allow(dead_code)]`
/// from Chron1, for `link`, `warranty_start` and `warranty_end`, whose comment
/// said it would come off in Chron4 when the details column gained readers for
/// them. Chron4 shipped and the allowance did not; it comes off here, which is
/// the compiler confirming those readers really do exist rather than us saying so.
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
    /// A product is addressed by its folder, never by its name — the display
    /// name is free to change or repeat, the folder is the identity.
    pub fn folder(&self) -> &str {
        match self {
            Entry::Ok(p) => &p.folder,
            Entry::Broken { folder, .. } => folder,
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
pub fn first_line(message: &str) -> String {
    message
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Render a date the way Parachron shows dates: `DD-MM-YYYY` (CORE §3).
///
/// Storage stays ISO — this is the display half of that rule and the single
/// place the conversion happens. Called by the details column, the export's
/// summary page, and the About pane's release row.
pub fn fmt_date(date: Date) -> String {
    let format = time::macros::format_description!("[day]-[month]-[year]");
    date.format(&format).unwrap_or_default()
}

/// Read the machine's UTC offset.
///
/// **Call this before anything spawns a thread.** `time` refuses to work the
/// local offset out in a process that has more than one, because the C call
/// underneath is not safe once other threads exist — and Parachron always has
/// the render worker running a moment later. So `main` asks first, while it is
/// still alone, and hands the answer to everything that needs a date.
///
/// A machine that will not say falls back to UTC. For a counter measured in
/// months, being a day out at the edge of a timezone is the right failure.
pub fn local_offset() -> UtcOffset {
    UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
}

/// Today, where the user is.
///
/// Recomputed on every use rather than cached at startup, so an app left open
/// overnight shows the right number in the morning. With the offset already in
/// hand this costs nothing and is safe to ask from any thread.
pub fn today(offset: UtcOffset) -> Date {
    OffsetDateTime::now_utc().to_offset(offset).date()
}

/// Days of warranty left, clamped at zero (CORE §3).
///
/// A warranty that ran out is expired, not negative — nobody wants to read
/// `-412 days`.
pub fn days_left(warranty_end: Date, today: Date) -> i64 {
    (warranty_end - today).whole_days().max(0)
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
    let loose = time::macros::format_description!("[day padding:none]-[month padding:none]-[year]");

    let parsed = Date::parse(&normalised, &padded)
        .or_else(|_| Date::parse(&normalised, &loose))
        .ok()?;

    // `time`'s `[year]` accepts a leading sign, so `14-03--0500` parses as the
    // year −500. That would then be clamped on the way to TOML and written out
    // as year 0 — a silent rewrite of what somebody typed. Refuse it instead;
    // a purchase has never been made before the common era.
    (parsed.year() >= 1).then_some(parsed)
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

    // Leaving a temporary behind would be litter in somebody's product folder,
    // so every failure below clears up after itself.
    let written = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        // Without this the rename can be durable while the bytes are not, and
        // a crash at the wrong moment leaves a zero-length manifest — the exact
        // outcome this function exists to prevent.
        file.sync_all()
    })();

    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(unreadable(e));
    }

    fs::rename(&tmp, path).map_err(|e| {
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
    const RESERVED: [&str; 24] = [
        "con", "prn", "aux", "nul", "com0", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
        "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8",
        "lpt9",
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

/// Fold a string for matching: the same letter mapping `folder_slug` uses, and
/// none of its slugging.
///
/// Chron7 refused `folder_slug` for the export's suggested filename, and this is
/// the same refusal for the same reason. Slugging lowercases, folds to ASCII
/// *and* hyphenates — right for a directory that has to survive being rsynced
/// onto Windows, wrong for anything a person reads or types. Search wants the
/// folding half only, applied to both sides, so `sarj` finds `Şarj Cihazı` and
/// `ŞARJ` finds it too.
///
/// The İ/ı mapping is not a nicety here either. `"İ".to_lowercase()` yields `i`
/// followed by a combining dot above, so a serial like `İST-0042-ĞŞ` would be
/// unmatchable by anything typed on a keyboard. [`fold`] already solves that
/// once, for folder names; this is its second caller.
///
/// Folding happens *before* lowercasing, because `fold` maps both cases of every
/// letter it knows to a single lowercase ASCII one — doing it the other way round
/// would hand `İ` to `to_lowercase` and reintroduce the combining mark.
pub fn search_fold(text: &str) -> String {
    text.chars()
        .map(|ch| fold(ch).unwrap_or(ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
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

    // ── Where the vault is (Chron9) ──────────────────────────────────────

    /// The case that must not have changed: no `vault` key means exactly what
    /// every install had before this key existed.
    #[test]
    fn without_a_vault_key_products_sit_beside_the_config_as_they_always_have() {
        let paths = Paths::for_test(PathBuf::from("/data/parachron"));

        assert_eq!(paths.vault, PathBuf::from("/data/parachron"));
        assert_eq!(paths.products, PathBuf::from("/data/parachron/products"));
        assert_eq!(paths.config, PathBuf::from("/data/parachron/config.toml"));
        assert!(!paths.is_configured());
    }

    /// A configured vault moves `products/` and leaves `config.toml` alone —
    /// which it must, since the key naming the vault is inside it.
    #[test]
    fn a_configured_vault_moves_the_products_and_never_the_config() {
        let paths =
            Paths::for_test(PathBuf::from("/data/parachron")).with_vault(Some("/mnt/ironwolf/pc"));

        assert_eq!(paths.vault, PathBuf::from("/mnt/ironwolf/pc"));
        assert_eq!(paths.products, PathBuf::from("/mnt/ironwolf/pc/products"));
        assert_eq!(
            paths.config,
            PathBuf::from("/data/parachron/config.toml"),
            "the pointer cannot live in the thing it points at"
        );
        assert!(paths.is_configured());
    }

    /// An empty or whitespace-only value is the default, not a vault at the
    /// filesystem root. A hand-edited `vault = ""` is far likelier than somebody
    /// meaning `/`.
    #[test]
    fn an_empty_vault_value_means_the_default() {
        for value in [Some(""), Some("   "), None] {
            let paths = Paths::for_test(PathBuf::from("/data/parachron")).with_vault(value);
            assert_eq!(paths.products, PathBuf::from("/data/parachron/products"));
            assert!(
                !paths.is_configured(),
                "{value:?} is not a configured vault"
            );
        }
    }

    /// The default vault is created on first run, as it always was.
    #[test]
    fn the_default_vault_is_created_on_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(dir.path().join("parachron"));

        paths.ensure().expect("the default vault is created");
        assert!(paths.products.is_dir());
    }

    /// **The test this milestone turns on.**
    ///
    /// If `vault` names a path under a mount point and the drive is not there,
    /// that mount point is an ordinary empty directory on the root filesystem.
    /// `create_dir_all` would succeed against it, Parachron would build a vault
    /// on the system disk, and its owner would file documents there believing
    /// they were on the drive they bought for exactly this — then mount the
    /// drive and watch the lot disappear underneath it.
    ///
    /// So the assertion is not merely that `ensure` fails. It is that **nothing
    /// was written**, which is the part a returned `Err` does not prove.
    #[test]
    fn a_configured_vault_that_is_not_there_is_reported_and_never_created() {
        let dir = tempfile::tempdir().unwrap();
        // A plausible unmounted drive: the parent exists and is writable, which
        // is exactly the shape that makes `create_dir_all` succeed.
        let mount_point = dir.path().join("mnt");
        std::fs::create_dir_all(&mount_point).unwrap();
        let vault = mount_point.join("ironwolf/parachron");

        let paths =
            Paths::for_test(dir.path().join("data")).with_vault(Some(vault.to_str().unwrap()));

        let failure = paths
            .ensure()
            .expect_err("a missing vault must be reported");
        assert!(
            matches!(&failure, DataError::VaultMissing(named) if named == &vault.display().to_string()),
            "the message must name the path so a user can act on it: {failure:?}"
        );
        assert!(!vault.exists(), "a vault was created on the wrong disk");
        assert!(
            !mount_point.join("ironwolf").exists(),
            "the mount point was written into, which is what hides data under a mount"
        );
    }

    /// And it does not quietly fall back to the default, which would look
    /// identical to total data loss to whoever is reading the window.
    #[test]
    fn a_missing_configured_vault_does_not_fall_back_to_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let paths = Paths::for_test(data.clone())
            .with_vault(Some(dir.path().join("gone").to_str().unwrap()));

        assert!(paths.ensure().is_err());
        assert!(
            !data.join("products").exists(),
            "the default vault was created as a consolation prize"
        );
        assert_ne!(paths.products, data.join("products"));
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
        assert_eq!(
            fmt_date(to_date(&raw.added, "added").unwrap()),
            "05-08-2026"
        );
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
            "",
            "tomorrow",
            "2026-03-14", // ISO is the storage format, not the input one
            "31-02-2026", // no such day
            "14-13-2026", // no such month
            "14-03",
            // `time`'s year accepts a sign, and a negative year would be
            // clamped to 0 on the way into TOML — a silent rewrite of what was
            // typed rather than a refusal.
            "14-03--0500",
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
        assert_eq!(
            manifest.extra.len(),
            2,
            "unknown keys are kept, not dropped"
        );

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
            reloaded
                .extra
                .get("last_checked")
                .is_some_and(|v| v.is_datetime()),
            "an unknown date key must stay a TOML date: {:?}",
            reloaded.extra.get("last_checked")
        );
        // Known keys keep their documented order ahead of the extras.
        let name_at = rewritten.find("name =").unwrap();
        let notes_at = rewritten.find("notes =").unwrap();
        assert!(
            name_at < notes_at,
            "extras must not shuffle above the schema"
        );
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
        assert!(
            leftovers.is_empty(),
            "temporary files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn a_write_that_fails_leaves_neither_a_temporary_nor_a_damaged_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("product.toml");
        write_atomic(&path, "the good copy").unwrap();

        // A directory where the file should go: `File::create` cannot succeed,
        // which is the cheapest reachable write failure.
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        assert!(write_atomic(&blocked, "nope").is_err());

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "the good copy",
            "an unrelated failure must not touch what was already written"
        );
        let strays: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(strays.is_empty(), "temporary left behind: {strays:?}");
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

    // ── Search folding (Chron8) ──────────────────────────────────────────

    /// The property the search bar rests on. Query and field go through the same
    /// fold, so any casing or accent combination of the query still reaches the
    /// product. A fold applied to one side only is the bug that passes every
    /// English fixture — `sarj` against a raw `Şarj Cihazı` fails, and so does a
    /// folded query against a raw field — so both directions are asserted here.
    #[test]
    fn a_folded_query_finds_a_folded_field_in_any_casing_the_user_types() {
        // The field as it sits in the vault, typed properly.
        let field = search_fold("Şarj Cihazı");
        for query in ["sarj", "ŞARJ", "Şarj", "SARJ", "şarj", "Cihaz", "cihazı"] {
            let folded = search_fold(query);
            assert!(
                field.contains(&folded),
                "{query:?} folded to {folded:?}, which is not inside {field:?}"
            );
        }

        // The other direction: the field is the half that was typed carelessly,
        // shouting and dotless, and a properly typed query still has to find it.
        let shouted = search_fold("ŞARJ CİHAZI");
        assert_eq!(
            shouted, field,
            "the same words in different cases must fold to the same string"
        );
        for query in ["şarj", "cihazı", "Cihazı"] {
            let folded = search_fold(query);
            assert!(
                shouted.contains(&folded),
                "{query:?} folded to {folded:?}, which is not inside {shouted:?}"
            );
        }
    }

    /// The Turkish dotted capital, pinned on its own, because Rust's
    /// `"İ".to_lowercase()` yields `i` followed by U+0307 COMBINING DOT ABOVE.
    /// A serial folded that way holds a character no keyboard produces, so
    /// `İST-0042-ĞŞ` would be unmatchable by the very person who owns it.
    /// [`fold`] runs *before* the lowercasing precisely so `to_lowercase` never
    /// sees the letter; folding the other way round would reintroduce the mark
    /// and this test is what would notice.
    #[test]
    fn a_dotted_capital_i_folds_to_a_bare_ascii_i_with_no_combining_mark() {
        let dotted = search_fold("İ");
        assert_eq!(dotted, "i");
        assert!(
            !dotted.contains('\u{0307}'),
            "the fold left a combining dot above behind: {:?}",
            dotted.chars().collect::<Vec<_>>()
        );

        let serial = search_fold("İST-0042-ĞŞ");
        assert_eq!(serial, "ist-0042-gs");
        assert!(
            !serial.contains('\u{0307}'),
            "a serial no keyboard can match: {:?}",
            serial.chars().collect::<Vec<_>>()
        );
        // Typed off a keyboard, and typed by someone being careful. Both find it.
        for query in ["ist", "İST", "İst", "0042", "ğş", "ĞŞ"] {
            let folded = search_fold(query);
            assert!(
                serial.contains(&folded),
                "{query:?} folded to {folded:?}, which is not inside {serial:?}"
            );
        }
    }

    /// Folding is not slugging, and the distinction is worth a test of its own
    /// because reaching for `folder_slug` is a recurring mistake: Chron7 refused
    /// it for the export's suggested filename and Chron8 refuses it again here,
    /// one milestone later. Slugging lowercases, folds to ASCII *and*
    /// hyphenates — right for a directory that has to survive being rsynced onto
    /// Windows, wrong for anything a person reads or types. A search that
    /// slugged its input would hyphenate the space and stop matching the moment
    /// somebody typed two words.
    #[test]
    fn folding_keeps_what_a_person_typed_where_slugging_would_rewrite_it() {
        let folded = search_fold("Şarj Cihazı");
        assert_eq!(folded, "sarj cihazi", "the space is part of what was typed");
        assert_eq!(folder_slug("Şarj Cihazı"), "sarj-cihazi");
        assert_ne!(
            folded,
            folder_slug("Şarj Cihazı"),
            "if these two ever agree, one of them has taken on the other's job"
        );
        // The consequence, stated as the search sees it: a two-word query.
        assert!(folded.contains(&search_fold("arj Cih")));

        // Punctuation survives instead of collapsing into hyphens, so a serial
        // can be searched for the way it is printed on the box.
        assert_eq!(search_fold("Dell // U2724D!!"), "dell // u2724d!!");
        assert_eq!(folder_slug("Dell // U2724D!!"), "dell-u2724d");

        // No truncation either. A slug is capped at SLUG_MAX because a path has
        // a length limit; a query has no such thing, and a cap here would mean a
        // long product name stopped matching its own tail.
        let long = "very long name ".repeat(40);
        assert!(long.len() > SLUG_MAX);
        assert_eq!(search_fold(&long), long, "the fold shortened a long field");

        // A script the fold table has no entry for passes through unchanged
        // rather than landing on `SLUG_FALLBACK` — a folder needs *a* name, but
        // a query that turned into "product" would match every product there is.
        assert_eq!(search_fold("日本語"), "日本語");
        assert_eq!(folder_slug("日本語"), SLUG_FALLBACK);
    }

    /// An empty query folds to an empty string rather than to a fallback name.
    /// That is what lets the caller read "nothing typed" as "match everything":
    /// every string contains `""`. `folder_slug` substitutes `SLUG_FALLBACK`
    /// here, which as a query would match only the products whose names happen
    /// to contain the word.
    #[test]
    fn an_empty_string_folds_to_an_empty_string_and_not_to_a_fallback() {
        assert_eq!(search_fold(""), "");
        assert_eq!(folder_slug(""), SLUG_FALLBACK);
        assert!(search_fold("QD-OLED Monitor").contains(&search_fold("")));
    }

    /// The English path, which must not regress while the Turkish one is being
    /// served: an ASCII string comes back with its case lowered and nothing else
    /// touched — spaces, digits and hyphens all still where they were typed.
    #[test]
    fn an_ascii_string_is_unchanged_apart_from_its_case() {
        assert_eq!(search_fold("QD-OLED Monitor"), "qd-oled monitor");
        assert_eq!(search_fold("IronWolf Pro 6TB"), "ironwolf pro 6tb");
        assert_eq!(search_fold("ABC123XYZ"), "abc123xyz");
        assert_eq!(search_fold("  spaced   out  "), "  spaced   out  ");

        // Folding is a fixed point on its own output, which is what makes it
        // safe for a caller to fold a string it is not sure was folded already.
        let once = search_fold("Şarj Cihazı");
        assert_eq!(search_fold(&once), once);
    }
}
