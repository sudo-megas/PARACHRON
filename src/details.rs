//! Column 3: the product's dates, its link, and the warranty countdown.
//!
//! A pure function of the selected entry and today's date, plus one callback
//! for copying the link. The vault computes the snapshot while it is already
//! deciding what the list looks like, so there is one pass over the selection
//! rather than two that could disagree.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};
use time::Date;

use crate::AppWindow;
use crate::data::{self, Entry, Product};
use crate::strings::{self, Key, Lang};
use crate::vault::{self, Vault};

/// How long the "copied" confirmation stays up. The same 1.5 seconds the serial
/// strip uses — sharing the number is what stops the two drifting apart.
const COPIED_LINGER: Duration = Duration::from_millis(1500);

/// Everything column 3 shows, already formatted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub filled: bool,
    pub link: String,
    pub purchase_date: String,
    pub warranty_start: String,
    pub warranty_end: String,
    pub days_left: String,
    pub expired: bool,
}

impl Snapshot {
    /// Nothing selected, or a folder whose manifest would not parse. Column 2
    /// already shows the reason in that case; column 3 has nothing to add and
    /// says nothing rather than showing empty labels.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn of(entry: Option<&Entry>, lang: Lang, today: Date) -> Self {
        match entry {
            Some(Entry::Ok(product)) => Self::product(product, lang, today),
            _ => Self::empty(),
        }
    }

    fn product(product: &Product, lang: Lang, today: Date) -> Self {
        let remaining = data::days_left(product.warranty_end, today);
        let expired = remaining == 0;

        Self {
            filled: true,
            link: product.link.clone(),
            purchase_date: data::fmt_date(product.purchase_date),
            warranty_start: data::fmt_date(product.warranty_start),
            warranty_end: data::fmt_date(product.warranty_end),
            days_left: countdown(remaining, lang),
            expired,
        }
    }
}

/// The counter, in words.
///
/// Composed here rather than in the `.slint` file because the string table
/// holds no interpolation — the same reason the viewer's page counter is built
/// in Rust.
///
/// Chron7's summary page calls this rather than reimplementing the arithmetic, so
/// the figure on the exported page and the figure in column 3 cannot disagree —
/// which is the whole reason CORE §6 says "days left at time of export".
pub fn countdown(days: i64, lang: Lang) -> String {
    if days == 0 {
        return strings::get(lang, Key::WarrantyExpired).to_string();
    }
    let unit = if days == 1 {
        Key::DayUnit
    } else {
        Key::DaysUnit
    };
    format!("{days} {}", strings::get(lang, unit))
}

/// Push a snapshot to the window.
///
/// Also takes the export's status line down, because this is called on every
/// change of selection and that status is a claim about one product: `Saved — Not
/// included: gone.pdf` left over from the last product, sitting above the next
/// one's details or above a broken folder's placeholder, says something untrue.
/// `export.rs` is the only thing that ever puts a status up; a change of product
/// is the only other thing that can take one down.
pub fn show(app: &AppWindow, snapshot: &Snapshot) {
    crate::export::clear_status(app);
    app.set_details_filled(snapshot.filled);
    app.set_details_link(snapshot.link.as_str().into());
    app.set_details_purchase_date(snapshot.purchase_date.as_str().into());
    app.set_details_warranty_start(snapshot.warranty_start.as_str().into());
    app.set_details_warranty_end(snapshot.warranty_end.as_str().into());
    app.set_details_days_left(snapshot.days_left.as_str().into());
    app.set_details_expired(snapshot.expired);
}

/// Kept alive for the life of the window.
pub struct Details {
    _copied: Rc<Timer>,
}

/// Wire up copying the purchase link.
pub fn install(app: &AppWindow, vault: Rc<RefCell<Vault>>) -> Details {
    let copied = Rc::new(Timer::default());

    app.on_copy_link({
        let vault = Rc::clone(&vault);
        let copied = Rc::clone(&copied);
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            // Read the link from the vault rather than back off the window, so
            // what lands on the clipboard is the product's, not whatever the UI
            // happens to be showing.
            let Some(link) = vault::selected_product(&vault).map(|p| p.link) else {
                return;
            };
            if link.is_empty() {
                return;
            }

            // CORE §4: this copies, it never opens. And a clipboard that will
            // not open is not worth taking the app down for — the link is on
            // screen either way.
            let ok = arboard::Clipboard::new()
                .and_then(|mut clipboard| clipboard.set_text(link))
                .is_ok();
            if !ok {
                return;
            }

            app.set_link_copied(true);
            let weak = app.as_weak();
            copied.start(TimerMode::SingleShot, COPIED_LINGER, move || {
                if let Some(app) = weak.upgrade() {
                    app.set_link_copied(false);
                }
            });
        }
    });

    Details { _copied: copied }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::DataError;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap()
    }

    fn product(warranty_end: Date) -> Entry {
        Entry::Ok(Product {
            folder: "monitor".to_string(),
            name: "QD-OLED Monitor".to_string(),
            serial: "ABC123XYZ".to_string(),
            link: "https://store.example/p".to_string(),
            purchase_date: day(2026, Month::March, 14),
            warranty_start: day(2026, Month::March, 14),
            warranty_end,
            pdfs: Vec::new(),
            added: day(2026, Month::August, 5),
            missing_pdfs: Vec::new(),
            extra: Default::default(),
        })
    }

    #[test]
    fn dates_reach_the_column_in_the_display_format() {
        let entry = product(day(2029, Month::March, 14));
        let snapshot = Snapshot::of(Some(&entry), Lang::En, day(2026, Month::August, 5));

        assert!(snapshot.filled);
        assert_eq!(snapshot.purchase_date, "14-03-2026");
        assert_eq!(snapshot.warranty_start, "14-03-2026");
        assert_eq!(snapshot.warranty_end, "14-03-2029");
        assert_eq!(snapshot.link, "https://store.example/p");
    }

    #[test]
    fn the_counter_is_the_days_between_today_and_the_warranty_end() {
        let entry = product(day(2026, Month::August, 15));
        let snapshot = Snapshot::of(Some(&entry), Lang::En, day(2026, Month::August, 5));
        assert_eq!(snapshot.days_left, "10 days");
        assert!(!snapshot.expired);
    }

    #[test]
    fn one_day_left_is_not_one_days() {
        assert_eq!(countdown(1, Lang::En), "1 day");
        assert_eq!(countdown(2, Lang::En), "2 days");
    }

    /// Turkish takes no plural after a numeral, so both forms are `gün`.
    #[test]
    fn turkish_does_not_pluralise_after_a_number() {
        assert_eq!(countdown(1, Lang::Tr), "1 gün");
        assert_eq!(countdown(658, Lang::Tr), "658 gün");
    }

    #[test]
    fn a_warranty_that_has_run_out_reads_as_expired_not_as_a_negative() {
        let entry = product(day(2025, Month::January, 1));
        let snapshot = Snapshot::of(Some(&entry), Lang::En, day(2026, Month::August, 5));

        assert!(snapshot.expired);
        assert_eq!(
            snapshot.days_left,
            strings::get(Lang::En, Key::WarrantyExpired)
        );
        assert!(!snapshot.days_left.contains('-'), "never a negative count");
    }

    #[test]
    fn the_last_day_of_a_warranty_is_expired_rather_than_zero_days() {
        let today = day(2026, Month::August, 5);
        let entry = product(today);
        let snapshot = Snapshot::of(Some(&entry), Lang::En, today);
        assert!(snapshot.expired);
    }

    #[test]
    fn nothing_selected_and_a_broken_folder_both_show_an_empty_column() {
        assert_eq!(
            Snapshot::of(None, Lang::En, day(2026, Month::August, 5)),
            Snapshot::empty()
        );

        let broken = Entry::Broken {
            folder: "test-broken".to_string(),
            reason: DataError::MissingToml,
        };
        let snapshot = Snapshot::of(Some(&broken), Lang::En, day(2026, Month::August, 5));
        assert!(!snapshot.filled, "column 2 already explains what is wrong");
        assert!(snapshot.days_left.is_empty());
    }

    #[test]
    fn the_countdown_counts_down_as_the_days_pass() {
        let entry = product(day(2026, Month::August, 10));
        let counts: Vec<String> = [3, 5, 9, 10]
            .into_iter()
            .map(|d| Snapshot::of(Some(&entry), Lang::En, day(2026, Month::August, d)).days_left)
            .collect();
        assert_eq!(counts, ["7 days", "5 days", "1 day", "Expired"]);
    }
}
