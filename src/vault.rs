//! Who owns the product list.
//!
//! Everything that changes what column 1 shows goes through here: the initial
//! scan, a click, a save from the form (Chron3) and a sort toggle (Chron4).
//! The window's `products` model is written in exactly one place — this one —
//! because two writers would eventually disagree about what row 3 is.
//!
//! Selection is a folder name, never an index. A folder is a product's identity
//! (CORE §3); the display name may change or repeat, and any index into the
//! list stops meaning what it meant the moment the list is re-sorted or
//! re-scanned.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use time::UtcOffset;

use crate::data::{self, DataError, Entry, Product};
use crate::details;
use crate::strings::{self, Key, Lang};
use crate::viewer::{DocSet, Viewer};
use crate::{AppWindow, ProductItem};

/// How the product list is ordered (CORE §4).
///
/// Chron3 builds this because the module that owns list order is built here;
/// the toggles that let anyone choose between them arrive in Chron4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    /// Insertion order, by the manifest's `added` date. CORE §4's default.
    #[default]
    Added,
    Name,
    Purchase,
}

impl SortMode {
    /// Read the value `config.toml` carries, falling back to the default rather
    /// than rejecting a file somebody typed into.
    pub fn from_code(code: &str) -> Self {
        match code {
            "name" => SortMode::Name,
            "purchase" => SortMode::Purchase,
            _ => SortMode::Added,
        }
    }

    /// Which chip is lit for this mode.
    pub fn chip(self) -> i32 {
        match self {
            SortMode::Added => 0,
            SortMode::Name => 1,
            SortMode::Purchase => 2,
        }
    }

    /// The inverse of [`SortMode::chip`], for what a click asks for.
    pub fn from_chip(chip: i32) -> Self {
        match chip {
            1 => SortMode::Name,
            2 => SortMode::Purchase,
            _ => SortMode::Added,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            SortMode::Added => "added",
            SortMode::Name => "name",
            SortMode::Purchase => "purchase",
        }
    }
}

pub struct Vault {
    products_root: PathBuf,
    /// In display order — the same order as the rows on screen, so the index a
    /// click carries indexes this directly.
    entries: Vec<Entry>,
    sort: SortMode,
    /// Folder of the selected product, or `None`.
    selected: Option<String>,
    lang: Lang,
    /// Read once at startup, while the process was still single-threaded.
    offset: UtcOffset,
}

/// Everything the window is told after the list changes.
///
/// Plain data, computed while the vault is borrowed and pushed once the borrow
/// has been dropped.
struct Update {
    rows: Vec<ProductItem>,
    index: i32,
    /// The three `selected-*` values the row click writes optimistically on the
    /// Slint side, which go stale the moment the model is rebuilt.
    name: SharedString,
    detail: SharedString,
    broken: bool,
    doc: Option<DocSet>,
    details: details::Snapshot,
    /// Which chip is lit: 0 as added, 1 alphabetical, 2 purchase date.
    sort: i32,
    keep_view: bool,
}

impl Vault {
    /// Re-read the vault from disk, then show `select` if it is still there.
    fn plan_rescan(&mut self, select: Option<&str>) -> Update {
        self.entries = data::scan(&self.products_root);
        if let Some(folder) = select {
            self.selected = Some(folder.to_string());
        }
        // A save is not a change of product, so the reader stays where they
        // were if the file they were reading is still there.
        self.plan(true)
    }

    /// Point the vault at the product on row `index`.
    fn plan_select(&mut self, index: usize) -> Update {
        self.selected = self
            .entries
            .get(index)
            .map(|entry| entry.folder().to_string());
        self.plan(false)
    }

    /// Sort, rebuild the rows, and work out everything the window needs.
    fn plan(&mut self, keep_view: bool) -> Update {
        sort_entries(&mut self.entries, self.sort);

        let rows: Vec<ProductItem> = self
            .entries
            .iter()
            .map(|entry| row(entry, self.lang))
            .collect();

        let position = self
            .selected
            .as_deref()
            .and_then(|folder| self.entries.iter().position(|e| e.folder() == folder));

        let Some(index) = position else {
            // The selection is gone — deleted outside the app, or renamed. Every
            // part of it goes, `broken` included: a stale `true` would paint the
            // "select a product" prompt in the error colour.
            self.selected = None;
            return Update {
                rows,
                index: -1,
                name: SharedString::new(),
                detail: SharedString::new(),
                broken: false,
                doc: None,
                details: details::Snapshot::empty(),
                sort: self.sort.chip(),
                keep_view: false,
            };
        };

        let item = &rows[index];
        let (name, detail, broken) = (item.name.clone(), item.detail.clone(), item.broken);
        let doc = match &self.entries[index] {
            Entry::Ok(product) => Some(DocSet::of(product)),
            // A folder that will not parse has no documents to show; column 2
            // keeps Chron1's reason display instead of the viewer.
            Entry::Broken { .. } => None,
        };

        // Today is re-read here rather than cached at startup, so a window left
        // open overnight shows the right countdown in the morning.
        let details =
            details::Snapshot::of(self.entries.get(index), self.lang, data::today(self.offset));

        Update {
            rows,
            index: index as i32,
            name,
            detail,
            broken,
            doc,
            details,
            sort: self.sort.chip(),
            keep_view,
        }
    }

    /// Reorder the list. `Added` is CORE §4's default and what an active chip
    /// clears back to.
    fn plan_sort(&mut self, sort: SortMode) -> Update {
        self.sort = sort;
        // A re-sort is not a change of product: whoever was reading page seven
        // of an invoice is still reading it.
        self.plan(true)
    }

    /// What to write back to `config.toml` on the way out.
    pub fn sort(&self) -> SortMode {
        self.sort
    }
}

/// Wire the list into the window and fill it with what is on disk.
pub fn install(
    app: &AppWindow,
    products_root: PathBuf,
    entries: Vec<Entry>,
    sort: SortMode,
    lang: Lang,
    offset: UtcOffset,
    viewer: Rc<Viewer>,
) -> Rc<RefCell<Vault>> {
    let vault = Rc::new(RefCell::new(Vault {
        products_root,
        entries,
        sort,
        selected: None,
        lang,
        offset,
    }));

    app.on_product_selected({
        let vault = Rc::clone(&vault);
        let viewer = Rc::clone(&viewer);
        let weak = app.as_weak();
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            if index < 0 {
                return;
            }
            // The borrow ends with this statement, before anything touches the
            // window. Slint setters can run bindings that call straight back
            // into this callback, and a `RefCell` borrowed twice is a panic.
            let update = vault.borrow_mut().plan_select(index as usize);
            push(&app, &viewer, update);
        }
    });

    app.on_sort_toggled({
        let vault = Rc::clone(&vault);
        let viewer = Rc::clone(&viewer);
        let weak = app.as_weak();
        move |mode| {
            let Some(app) = weak.upgrade() else { return };
            let update = vault.borrow_mut().plan_sort(SortMode::from_chip(mode));
            push(&app, &viewer, update);
        }
    });

    let update = vault.borrow_mut().plan(false);
    push(app, &viewer, update);

    vault
}

/// Re-read the vault and show `select` if it is still on disk.
///
/// What the form calls after a save. The borrow is scoped to the first
/// statement, before anything touches the window.
pub fn rescan(vault: &Rc<RefCell<Vault>>, app: &AppWindow, viewer: &Viewer, select: Option<&str>) {
    let update = vault.borrow_mut().plan_rescan(select);
    push(app, viewer, update);
}

/// A copy of the selected product, when one is selected and its manifest
/// parsed. The form pre-fills from this.
pub fn selected_product(vault: &Rc<RefCell<Vault>>) -> Option<Product> {
    let vault = vault.borrow();
    let folder = vault.selected.as_deref()?;
    vault.entries.iter().find_map(|entry| match entry {
        Entry::Ok(product) if product.folder == folder => Some(product.clone()),
        _ => None,
    })
}

/// Hand an update to the window. No borrow of the vault is alive here.
fn push(app: &AppWindow, viewer: &Viewer, update: Update) {
    let index = update.index;

    // Model first: the index written below indexes the rows now on screen.
    app.set_products(ModelRc::new(VecModel::from(update.rows)));
    app.set_selected_name(update.name);
    app.set_selected_detail(update.detail);
    // Before the index, so the broken pane can never compose an old name with
    // a new reason.
    app.set_selected_broken(update.broken);
    // Straight to its final value. Passing through -1 on the way would gate off
    // the conditional that hosts the viewer, tearing it down and paying the
    // resize debounce to build it again.
    app.set_selected_index(update.index);
    app.set_sort_mode(update.sort);
    details::show(app, &update.details);
    viewer.show(app, update.doc, update.keep_view);

    // Scroll last, and only after a layout pass. The list still reports the
    // previous model's height until it has laid out again, so scrolling here
    // and now would clamp against a stale number — and a list that just got
    // shorter would be left showing its last row alone above empty space.
    let weak = app.as_weak();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            app.invoke_scroll_row_into_view(index);
        }
    });
}

/// Order the list.
///
/// Broken folders sink to the end under every mode. They have no name and no
/// dates to sort by, and burying an unreadable folder halfway down an
/// alphabetical list is how it stops getting noticed. Folder name breaks every
/// tie, so the order is total and does not shuffle between runs.
fn sort_entries(entries: &mut [Entry], mode: SortMode) {
    entries.sort_by(|a, b| match (a, b) {
        (Entry::Ok(x), Entry::Ok(y)) => key(mode, x, y).then_with(|| x.folder.cmp(&y.folder)),
        (Entry::Ok(_), Entry::Broken { .. }) => Ordering::Less,
        (Entry::Broken { .. }, Entry::Ok(_)) => Ordering::Greater,
        (Entry::Broken { .. }, Entry::Broken { .. }) => a.folder().cmp(b.folder()),
    });
}

fn key(mode: SortMode, x: &Product, y: &Product) -> Ordering {
    match mode {
        SortMode::Added => x.added.cmp(&y.added),
        // Case-folded, so `iPhone` does not sort away from `Ipad` on its
        // capital. This is not a locale-aware collation — Turkish puts ç after
        // c and ı before i, which `to_lowercase` knows nothing about. Worth
        // doing properly if it ever bothers somebody; not worth a collation
        // dependency for a list of a few dozen products.
        SortMode::Name => x.name.to_lowercase().cmp(&y.name.to_lowercase()),
        SortMode::Purchase => x.purchase_date.cmp(&y.purchase_date),
    }
}

/// Turn one vault entry into a list row.
///
/// Every string a row carries — prefixes included — is assembled from the
/// string table, so the `.slint` side never holds text of its own.
fn row(entry: &Entry, lang: Lang) -> ProductItem {
    match entry {
        Entry::Ok(product) => {
            let incomplete = !product.missing_pdfs.is_empty();
            let prefix = if incomplete {
                strings::get(lang, Key::WarnPrefix)
            } else {
                ""
            };
            let detail = if incomplete {
                format!(
                    "{}: {}",
                    strings::get(lang, Key::MissingFiles),
                    product.missing_pdfs.join(", ")
                )
            } else {
                String::new()
            };

            ProductItem {
                label: format!("{prefix}{}", product.name).into(),
                name: product.name.clone().into(),
                detail: detail.into(),
                broken: false,
                warning: incomplete,
            }
        }
        Entry::Broken { folder, reason } => {
            // A failure with no folder behind it (no home directory) falls back
            // to the generic heading.
            let heading = strings::get(lang, Key::BrokenTitle);
            let label = if folder.is_empty() {
                format!("{}{heading}", strings::get(lang, Key::BrokenPrefix))
            } else {
                format!("{}{folder}", strings::get(lang, Key::BrokenPrefix))
            };
            let name = if folder.is_empty() {
                heading.to_string()
            } else {
                format!("{heading}: {folder}")
            };

            ProductItem {
                label: label.into(),
                name: name.into(),
                detail: describe(lang, reason).into(),
                broken: true,
                warning: false,
            }
        }
    }
}

/// Render a [`DataError`] as readable text in the chosen language. The trailing
/// detail is diagnostic payload from the OS or the TOML parser and stays as-is.
pub fn describe(lang: Lang, error: &DataError) -> String {
    match error {
        DataError::NoHome => strings::get(lang, Key::ErrNoHome).to_string(),
        DataError::MissingToml => strings::get(lang, Key::ErrMissingToml).to_string(),
        DataError::Unreadable(detail) => {
            format!("{}: {detail}", strings::get(lang, Key::ErrUnreadable))
        }
        DataError::Malformed(detail) => {
            format!("{}: {detail}", strings::get(lang, Key::ErrMalformed))
        }
        DataError::InvalidDate { field, detail } => {
            format!(
                "{} ({field}): {detail}",
                strings::get(lang, Key::ErrInvalidDate)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month};

    fn product(folder: &str, name: &str, added: i32, purchased: i32) -> Entry {
        let day = |n: i32| Date::from_calendar_date(2026, Month::March, n as u8).unwrap();
        Entry::Ok(Product {
            folder: folder.to_string(),
            name: name.to_string(),
            serial: String::new(),
            link: String::new(),
            purchase_date: day(purchased),
            warranty_start: day(purchased),
            warranty_end: day(purchased),
            pdfs: Vec::new(),
            added: day(added),
            missing_pdfs: Vec::new(),
            extra: Default::default(),
        })
    }

    fn broken(folder: &str) -> Entry {
        Entry::Broken {
            folder: folder.to_string(),
            reason: DataError::MissingToml,
        }
    }

    /// Added third, alphabetically first, bought last — so the three modes
    /// cannot accidentally agree.
    fn vault() -> Vec<Entry> {
        vec![
            product("monitor", "QD-OLED Monitor", 2, 5),
            broken("test-broken"),
            product("drive", "IronWolf Pro", 3, 1),
            product("keyboard", "Alice Keyboard", 1, 9),
        ]
    }

    fn folders(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.folder()).collect()
    }

    #[test]
    fn insertion_order_is_the_default_and_reads_the_added_date() {
        let mut entries = vault();
        sort_entries(&mut entries, SortMode::Added);
        assert_eq!(folders(&entries), ["keyboard", "monitor", "drive", "test-broken"]);
    }

    #[test]
    fn alphabetical_order_ignores_case() {
        let mut entries = vault();
        sort_entries(&mut entries, SortMode::Name);
        assert_eq!(folders(&entries), ["keyboard", "drive", "monitor", "test-broken"]);
    }

    #[test]
    fn purchase_order_puts_the_oldest_first() {
        let mut entries = vault();
        sort_entries(&mut entries, SortMode::Purchase);
        assert_eq!(folders(&entries), ["drive", "monitor", "keyboard", "test-broken"]);
    }

    #[test]
    fn broken_folders_sink_to_the_end_under_every_mode() {
        for mode in [SortMode::Added, SortMode::Name, SortMode::Purchase] {
            let mut entries = vault();
            entries.push(broken("aaa-broken"));
            sort_entries(&mut entries, mode);

            let tail = &entries[entries.len() - 2..];
            assert!(
                tail.iter().all(|e| matches!(e, Entry::Broken { .. })),
                "{mode:?} left a broken folder in the middle of the list"
            );
            // Tie-broken by folder, so the order is stable rather than whatever
            // the filesystem happened to hand over.
            assert_eq!(folders(tail), ["aaa-broken", "test-broken"]);
        }
    }

    #[test]
    fn sorting_is_stable_across_repeated_runs() {
        let mut once = vault();
        sort_entries(&mut once, SortMode::Name);
        let mut twice = once.clone();
        sort_entries(&mut twice, SortMode::Name);
        assert_eq!(folders(&once), folders(&twice));
    }

    #[test]
    fn sort_modes_round_trip_through_the_config_value() {
        for mode in [SortMode::Added, SortMode::Name, SortMode::Purchase] {
            assert_eq!(SortMode::from_code(mode.code()), mode);
        }
        // A config somebody has typed into falls back rather than failing.
        assert_eq!(SortMode::from_code("sideways"), SortMode::Added);
        assert_eq!(SortMode::from_code(""), SortMode::Added);
    }

    #[test]
    fn a_healthy_product_row_carries_no_prefix_and_no_detail() {
        let item = row(&product("monitor", "Monitor", 1, 1), Lang::En);
        assert_eq!(item.label, "Monitor");
        assert_eq!(item.name, "Monitor");
        assert!(item.detail.is_empty());
        assert!(!item.broken);
        assert!(!item.warning);
    }

    #[test]
    fn rows_translate() {
        assert_eq!(
            row(&broken("bozuk"), Lang::Tr).detail,
            strings::get(Lang::Tr, Key::ErrMissingToml)
        );
    }

    #[test]
    fn a_broken_entry_still_gets_a_row_with_a_readable_reason() {
        let item = row(&broken("test-broken"), Lang::En);
        assert!(item.broken);
        assert!(item.label.contains("test-broken"));
        assert!(!item.detail.is_empty(), "the reason is what makes it fixable");
    }

    #[test]
    fn a_product_missing_a_file_is_flagged_without_being_called_broken() {
        let Entry::Ok(mut p) = product("drive", "IronWolf Pro", 1, 1) else {
            unreachable!()
        };
        p.pdfs = vec!["invoice.pdf".to_string()];
        p.missing_pdfs = vec!["invoice.pdf".to_string()];

        let item = row(&Entry::Ok(p), Lang::En);
        assert!(item.warning);
        assert!(!item.broken);
        assert!(item.detail.contains("invoice.pdf"));
    }
}
