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
    /// Every entry on disk, in sort order.
    ///
    /// **Not** the same order as the rows on screen since Chron8: the search bar
    /// filters, so a row index and an entry index are two different numbers.
    /// [`Vault::visible`] is the map between them, and every index handed to or
    /// taken from the window goes through it.
    entries: Vec<Entry>,
    sort: SortMode,
    /// What the search bar holds. Session state: never written to `config.toml`,
    /// because a sort that survives a restart reorders and a filter that survives
    /// one *hides*.
    query: String,
    /// Folder of the selected product, or `None`.
    ///
    /// Independent of the filter. A query that excludes the open product narrows
    /// the list, not the app — the invoice stays on screen.
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
    /// Which row is highlighted, or `-1` for none.
    ///
    /// Since Chron8 this is a row index rather than an entry index, and `-1` no
    /// longer means "nothing is selected" — a filter can hide the selected
    /// product's row while the product is still open. `open` is that question.
    index: i32,
    /// Whether a product is selected at all, whatever the filter is showing.
    ///
    /// This gates the viewer, and `index` no longer can. Chron3 recorded that
    /// `selected-index` passing through `-1` tears the viewer down and rebuilds
    /// it at the cost of the resize debounce; before the search bar, `-1` and
    /// "no selection" were the same state, and now they are not.
    open: bool,
    /// Whether the viewer needs to hear about this update at all.
    ///
    /// False for a query change. Typing cannot change which product is selected,
    /// so the document, its page and its zoom are all still right — and Chron6
    /// found that a re-plan bumps the viewer's generation token and issues a
    /// fresh render, "a visible blink for no reason" on a large invoice. At the
    /// rate a query is typed that would be one blink per keystroke.
    view: bool,
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
    ///
    /// `index` is a row on screen. Since Chron8 that is not an index into
    /// `entries` — the filter sits between them — so it is mapped back through
    /// the same visible set the rows were built from. Getting this wrong does not
    /// crash; it selects a different product than the one clicked, which is why
    /// there is a test that clicks a row in a filtered list and asserts on the
    /// folder that comes back.
    fn plan_select(&mut self, index: usize) -> Update {
        self.selected = self
            .visible()
            .get(index)
            .map(|&entry| self.entries[entry].folder().to_string());
        self.plan(false)
    }

    /// The entries the query lets through, as indices into `entries`, in the
    /// order they appear on screen.
    ///
    /// Does not sort: `plan` sorts before it calls this, and `plan_select` is
    /// looking up a row that the last `plan` already laid out, so the order is
    /// the one the user clicked on.
    fn visible(&self) -> Vec<usize> {
        let needle = data::search_fold(&self.query);
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches(entry, &needle))
            .map(|(index, _)| index)
            .collect()
    }

    /// Sort, filter, rebuild the rows, and work out everything the window needs.
    fn plan(&mut self, keep_view: bool) -> Update {
        self.plan_with(keep_view, true)
    }

    /// [`Vault::plan`], with a say in whether the viewer is told.
    fn plan_with(&mut self, keep_view: bool, view: bool) -> Update {
        sort_entries(&mut self.entries, self.sort);

        let visible = self.visible();
        let rows: Vec<ProductItem> = visible
            .iter()
            .map(|&entry| row(&self.entries[entry], self.lang))
            .collect();

        // Two different questions, and only the first one clears the selection.
        // Is the selected folder still on disk? Then the product stays open even
        // if the query is hiding its row. Is it gone — deleted outside the app,
        // or renamed? Then everything about it goes, `broken` included: a stale
        // `true` would paint the "select a product" prompt in the error colour.
        let entry = self
            .selected
            .as_deref()
            .and_then(|folder| self.entries.iter().position(|e| e.folder() == folder));

        let Some(entry) = entry else {
            self.selected = None;
            return Update {
                rows,
                index: -1,
                open: false,
                view,
                name: SharedString::new(),
                detail: SharedString::new(),
                broken: false,
                doc: None,
                details: details::Snapshot::empty(),
                sort: self.sort.chip(),
                keep_view: false,
            };
        };

        // Where that entry sits among the rows, if the query is showing it at
        // all. `None` means the product is open and its row is filtered out.
        let index = visible
            .iter()
            .position(|&candidate| candidate == entry)
            .map(|row| row as i32)
            .unwrap_or(-1);

        // Built from the entry rather than read out of `rows`, because `rows` may
        // legitimately not contain it.
        let item = row(&self.entries[entry], self.lang);
        let (name, detail, broken) = (item.name.clone(), item.detail.clone(), item.broken);
        let doc = match &self.entries[entry] {
            Entry::Ok(product) => Some(DocSet::of(product)),
            // A folder that will not parse has no documents to show; column 2
            // keeps Chron1's reason display instead of the viewer.
            Entry::Broken { .. } => None,
        };

        // Today is re-read here rather than cached at startup, so a window left
        // open overnight shows the right countdown in the morning.
        let details =
            details::Snapshot::of(self.entries.get(entry), self.lang, data::today(self.offset));

        Update {
            rows,
            index,
            open: true,
            view,
            name,
            detail,
            broken,
            doc,
            details,
            sort: self.sort.chip(),
            keep_view,
        }
    }

    /// Narrow the list.
    ///
    /// Not a change of product, and deliberately not a change to the viewer: the
    /// document on screen stays open at the same page and zoom even when the
    /// query hides its row.
    fn plan_query(&mut self, query: String) -> Update {
        self.query = query;
        self.plan_with(true, false)
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

    /// Chron6. Every row's text — the `!` prefix, `Missing files: …`, a broken
    /// folder's heading and its reason — is composed here, so the language has to
    /// arrive before the rows are rebuilt.
    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
    }

    /// The selected product's folder, which is its identity (CORE §3) and is on
    /// disk rather than on screen — so nothing translates it.
    ///
    /// Added in Chron6 for the test that says so, and given a real caller in
    /// Chron7: an export that lands after the user has moved on compares the folder
    /// it exported against this one, and withholds its status line if they differ.
    /// The allowance came off there, the way Chron1's allowance on `Product` came
    /// off in Chron4.
    pub fn selected_folder(&self) -> Option<String> {
        self.selected.clone()
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
        // Always empty at startup. The query is not persisted, so the app never
        // opens showing three of eleven products with a filter the user has
        // forgotten they typed.
        query: String::new(),
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

    app.on_search_changed({
        let vault = Rc::clone(&vault);
        let viewer = Rc::clone(&viewer);
        let weak = app.as_weak();
        move |query| {
            let Some(app) = weak.upgrade() else { return };
            // Same two-phase shape as every other handler: compute while
            // borrowed, push once the borrow is gone.
            let update = vault.borrow_mut().plan_query(query.to_string());
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

/// Rebuild everything derived from the selection, without re-reading the disk.
///
/// What the language switch calls (Chron6). One pass through `plan` recomputes
/// the rows, the details snapshot and the viewer's state, which between them are
/// every string Rust composed rather than pushed through the `Strings` global —
/// so there is one route rather than five refresh routines that could disagree
/// about what is on screen.
///
/// `keep_view: true` for Chron3's reason: this is not a change of product.
/// Whoever was reading page seven of an invoice is still reading it.
pub fn relabel(vault: &Rc<RefCell<Vault>>, app: &AppWindow, viewer: &Viewer) {
    let update = vault.borrow_mut().plan(true);
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
    // Before the index too, and this is the one that hosts the viewer. Chron3's
    // warning — that `selected-index` passing through -1 tears the viewer down
    // and pays the resize debounce to build it again — is why the gate moved off
    // the index in Chron8: a filter can legitimately send the index to -1 while
    // the product stays open, and the document must not flicker when it does.
    app.set_selected_open(update.open);
    app.set_selected_index(update.index);
    app.set_sort_mode(update.sort);
    details::show(app, &update.details);
    if update.view {
        viewer.show(app, update.doc, update.keep_view);
    }

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

/// Whether `entry` survives the folded query `needle`.
///
/// A product matches on its **name** or its **serial number** — the two things a
/// person has in front of them when they go looking for a receipt. The purchase
/// link is deliberately not matched: a row matching on text column 1 cannot show
/// is a row the user cannot explain.
///
/// A broken folder matches on its **folder name**, which is the only text its row
/// has. Hiding the entry somebody is hunting for would be the one thing this list
/// has never done — Chron1 made a folder that will not parse visible on purpose,
/// and a filter is not a licence to take that back.
///
/// An empty query matches everything, which is the case that has to be cheap: it
/// is what the list is in for all but a few seconds of its life.
fn matches(entry: &Entry, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    match entry {
        Entry::Ok(product) => {
            data::search_fold(&product.name).contains(needle)
                || data::search_fold(&product.serial).contains(needle)
        }
        Entry::Broken { folder, .. } => data::search_fold(folder).contains(needle),
    }
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

    /// A product with a serial, for the half of the search that column 1 never
    /// shows. `product` leaves the serial empty because ordering never reads it.
    fn serialled(folder: &str, name: &str, serial: &str) -> Entry {
        let Entry::Ok(mut p) = product(folder, name, 1, 1) else {
            unreachable!("product() builds an Ok entry")
        };
        p.serial = serial.to_string();
        Entry::Ok(p)
    }

    fn a_vault(entries: Vec<Entry>) -> Vault {
        Vault {
            products_root: PathBuf::from("/nonexistent"),
            entries,
            sort: SortMode::Added,
            query: String::new(),
            selected: None,
            lang: Lang::En,
            offset: UtcOffset::UTC,
        }
    }

    /// The folders behind the rows an update carries, in the order they appear.
    fn shown(vault: &mut Vault) -> Vec<String> {
        vault
            .visible()
            .into_iter()
            .map(|entry| vault.entries[entry].folder().to_string())
            .collect()
    }

    /// The rarest row in the app: `Paths::resolve` failed, so `main` synthesises
    /// a broken entry with no folder name to put on it.
    ///
    /// It has always fallen back to the generic heading rather than rendering
    /// blank — `row` branches on exactly this. Pinned here because an empty
    /// `folder` in `main.rs` reads like a defect until you follow it, and this is
    /// the assertion that answers the question without anybody having to.
    #[test]
    fn a_failure_with_no_folder_behind_it_still_reads() {
        let entry = Entry::Broken {
            folder: String::new(),
            reason: DataError::NoHome,
        };
        let item = row(&entry, Lang::En);
        let heading = strings::get(Lang::En, Key::BrokenTitle);

        assert!(!item.label.is_empty());
        assert!(item.label.as_str().contains(heading));
        assert_eq!(item.name.as_str(), heading);
        // The reason is the half that says what actually went wrong.
        assert_eq!(item.detail.as_str(), strings::get(Lang::En, Key::ErrNoHome));
        assert!(item.broken);
        // And no orphaned separator where the folder name would have gone.
        assert!(!item.name.as_str().ends_with(": "));
    }

    #[test]
    fn an_empty_query_shows_everything() {
        let mut vault = a_vault(vault());
        let update = vault.plan(false);
        assert_eq!(update.rows.len(), 4);
    }

    #[test]
    fn a_query_narrows_the_list_to_matching_names() {
        let mut vault = a_vault(vault());
        let update = vault.plan_query("iron".to_string());
        assert_eq!(update.rows.len(), 1);
        assert_eq!(shown(&mut vault), ["drive"]);
    }

    /// The serial is the other half of CORE §4's "name and serial", and it is the
    /// half that matters when somebody is holding a warranty card: the number
    /// printed on it is not shown in column 1 at all, so a search that only
    /// matched what is visible would miss the one field they can read out.
    #[test]
    fn a_query_matches_a_serial_the_list_never_shows() {
        let mut vault = a_vault(vec![
            serialled("mouse", "Wireless Mouse", "MX-9182-QT"),
            serialled("dock", "Thunderbolt Dock", "TB-4471-KL"),
        ]);
        let update = vault.plan_query("4471".to_string());
        assert_eq!(update.rows.len(), 1);
        assert_eq!(shown(&mut vault), ["dock"]);
    }

    /// Chron1 made a folder that will not parse visible on purpose, and a filter
    /// is not a licence to take that back. Its folder name is the only text its
    /// row carries, so that is what it matches on.
    #[test]
    fn a_broken_folder_matches_on_its_folder_name() {
        let mut vault = a_vault(vault());
        let update = vault.plan_query("broken".to_string());
        assert_eq!(update.rows.len(), 1);
        assert_eq!(shown(&mut vault), ["test-broken"]);
    }

    /// Both sides are folded, so any casing or accenting of the query finds the
    /// product. The dotless-ı case is the one that would otherwise be
    /// unreachable from a keyboard.
    #[test]
    fn matching_is_folded_on_both_sides() {
        let mut vault = a_vault(vec![serialled("sarj", "Şarj Cihazı", "İST-0042-ĞŞ")]);

        for query in ["sarj", "ŞARJ", "Şarj", "cihazi", "CIHAZI"] {
            let update = vault.plan_query(query.to_string());
            assert_eq!(update.rows.len(), 1, "{query:?} did not find Şarj Cihazı");
        }

        // And the serial, whose İ would become `i` plus a combining dot under a
        // plain `to_lowercase` and stop matching anything typeable.
        for query in ["ist", "İST", "0042"] {
            let update = vault.plan_query(query.to_string());
            assert_eq!(update.rows.len(), 1, "{query:?} did not find the serial");
        }
    }

    /// The defect a filter introduces that does not crash.
    ///
    /// With the query on, row 0 is entry 2. Selecting "the first row" has to
    /// resolve to `drive`; an implementation that indexed `entries` directly
    /// would quietly select `keyboard` instead, and nothing would look wrong
    /// until the user noticed they were reading the wrong invoice.
    #[test]
    fn a_row_index_is_not_an_entry_index_once_a_filter_is_on() {
        let mut vault = a_vault(vault());
        vault.plan_query("iron".to_string());

        // The unfiltered order, so the test states the trap rather than assuming
        // the reader will spot it.
        assert_eq!(
            folders(&vault.entries),
            ["keyboard", "monitor", "drive", "test-broken"]
        );

        let update = vault.plan_select(0);
        assert_eq!(vault.selected.as_deref(), Some("drive"));
        assert_eq!(update.index, 0);
        assert!(update.open);
    }

    /// A query that excludes the open product narrows the *list*, not the app.
    ///
    /// `index` goes to -1 because no row is highlighted, and `open` stays true
    /// because the invoice is still on screen. Before Chron8 those were one flag,
    /// and Chron3 recorded what conflating them costs: the viewer is torn down
    /// and rebuilt through the resize debounce.
    #[test]
    fn a_filter_that_hides_the_selection_keeps_the_product_open() {
        let mut vault = a_vault(vault());
        vault.plan_query(String::new());
        vault.plan_select(1);
        assert_eq!(vault.selected.as_deref(), Some("monitor"));

        let update = vault.plan_query("iron".to_string());
        assert_eq!(update.index, -1, "no row should be highlighted");
        assert!(update.open, "the product is still open behind the filter");
        assert_eq!(vault.selected.as_deref(), Some("monitor"));
        // And the pane still describes the product it is showing, not nothing.
        assert!(update.name.contains("Monitor"));

        // Clearing the query brings its row back, and the selection never moved.
        let update = vault.plan_query(String::new());
        assert_eq!(vault.selected.as_deref(), Some("monitor"));
        assert!(update.index >= 0, "the row should be back");
        assert!(update.open);
    }

    /// The other reason `index` can be -1, which has to keep behaving as it did:
    /// the folder is gone from disk, so the selection really is nothing.
    #[test]
    fn a_selection_that_left_the_vault_is_cleared_rather_than_kept_open() {
        let mut vault = a_vault(vault());
        // `plan_select` maps a row through the *current* order, and only `plan`
        // establishes one — so sort before clicking, exactly as the window does.
        vault.plan(false);
        vault.plan_select(1);
        assert_eq!(vault.selected.as_deref(), Some("monitor"));

        vault.entries.retain(|entry| entry.folder() != "monitor");
        let update = vault.plan(false);

        assert_eq!(update.index, -1);
        assert!(!update.open, "nothing is selected any more");
        assert_eq!(vault.selected, None);
        assert!(update.name.is_empty());
        // `broken` has to go with it, or the "select a product" prompt renders in
        // the error colour.
        assert!(!update.broken);
    }

    /// Typing cannot change which product is selected, so the viewer is not told.
    ///
    /// Chron6 found that a re-plan bumps the viewer's generation token and issues
    /// a fresh render — "a visible blink for no reason" on a large invoice. At the
    /// rate a query is typed that would be one blink per keystroke. Every other
    /// route still tells the viewer, which is what the second half asserts: this
    /// is a narrow exemption, not the viewer being cut out of the loop.
    #[test]
    fn a_query_change_leaves_the_viewer_alone_and_nothing_else_does() {
        let mut vault = a_vault(vault());

        assert!(!vault.plan_query("iron".to_string()).view);
        assert!(!vault.plan_query(String::new()).view);

        assert!(vault.plan_select(0).view);
        assert!(vault.plan_sort(SortMode::Name).view);
        assert!(vault.plan(true).view);
    }

    /// A query nothing matches empties the list rather than falling back to
    /// showing everything, which is the tempting way to "helpfully" recover and
    /// would leave the user unable to tell that their query did anything at all.
    #[test]
    fn a_query_nothing_matches_shows_no_rows() {
        let mut vault = a_vault(vault());
        let update = vault.plan_query("zzzznothing".to_string());
        assert!(update.rows.is_empty());
    }

    /// The filter and the sort compose: narrowing does not reshuffle what is
    /// left, and re-sorting does not un-narrow it.
    #[test]
    fn the_filter_and_the_sort_do_not_interfere() {
        let mut vault = a_vault(vault());
        vault.plan_sort(SortMode::Name);
        let update = vault.plan_query("o".to_string());

        let rows = shown(&mut vault);
        assert_eq!(update.rows.len(), rows.len());

        // Alphabetical by name — Alice Keyboard, IronWolf Pro, QD-OLED Monitor —
        // with the broken folder still last. It survives the query on its folder
        // name (`test-broken` has an `o` in it), which is the point: filtering
        // does not lift the rule that an unreadable folder sinks to the end
        // rather than being buried halfway down an alphabetical list.
        assert_eq!(rows, ["keyboard", "drive", "monitor", "test-broken"]);
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
