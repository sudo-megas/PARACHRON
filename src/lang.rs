//! The language switch: who gets told, and in what order.
//!
//! `apply_strings` refills the `Strings` global, and everything bound to it in
//! the `.slint` files follows immediately. That is about half the text on screen.
//! The other half was composed in Rust and pushed into ordinary window
//! properties, and does not know anything happened:
//!
//! | Where | What |
//! |---|---|
//! | `vault::row` | the `!` warning prefix, `Missing files: …`, `⚠`, `Broken entry: <folder>` |
//! | `vault::describe` | every `DataError` — the reason under a broken folder |
//! | `details::countdown` | `658 days` / `658 gün`, and `Expired` |
//! | `viewer::describe` | every `ViewError` — the message shown in place of a page |
//! | `theme.rs` | the picker's eleven rows, four of which translate |
//! | `export.rs` | the status line under EXPORT, which is cleared rather than re-composed |
//!
//! The first four are all derived from the current selection, so all four are
//! recomputed by the path the vault already owns: one `plan` produces the rows,
//! the details snapshot and the viewer's state together. So the switch is five
//! `set_lang` calls, then `apply_strings`, then one re-push — not a refresh
//! routine per row of that table, which could disagree about what is on screen.
//!
//! The last two are pushed rather than derived and say so themselves: the picker's
//! rows come from a list Rust holds, and the export's status line is a sentence
//! about something that already happened, so it is cleared rather than translated
//! after the fact.
//!
//! The form is the one thing deliberately not in that table, and its absence is
//! load-bearing rather than an oversight. Its heading and per-field messages are
//! composed in Rust too, but a sheet's backdrop covers the whole window, so
//! `Document ▾` is unreachable while the form is up and the language cannot change
//! underneath it. `open()` composes them afresh every time. If a later milestone
//! makes a sheet dismissable by clicking away, or puts a menu above one, that is
//! the sentence that stops being true.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::ComponentHandle;

use crate::AppWindow;
use crate::editor::Editors;
use crate::export::Exports;
use crate::relocate::Relocations;
use crate::strings::Lang;
use crate::theme::{self, Themes};
use crate::vault::{self, Vault};
use crate::viewer::Viewer;

/// Everything the switch has to reach, gathered once so the callback does not
/// take seven arguments.
struct Owners {
    vault: Rc<RefCell<Vault>>,
    viewer: Rc<Viewer>,
    editors: Editors,
    themes: Rc<RefCell<Themes>>,
    exports: Exports,
    /// `None` when there is no vault to move — see `relocate::install`'s caller.
    relocations: Option<Relocations>,
}

/// Change the language and repaint every string on screen.
fn switch(app: &AppWindow, owners: &Owners, lang: Lang) {
    // Each owner keeps its own copy rather than sharing one cell, and that is
    // forced rather than chosen: `viewer::State` lives behind an `Arc<Mutex<_>>`
    // captured into the render worker's response sink, and `Renderer::spawn`
    // requires that closure to be `Send`. An `Rc` inside `State` makes `State`
    // not `Send` and the bound fails. The language is only ever read on the UI
    // thread so the sharing would be sound; the bound cannot know that. The risk
    // of a forgotten copy is answered by there being exactly one caller.
    owners.vault.borrow_mut().set_lang(lang);
    owners.viewer.set_lang(lang);
    owners.editors.set_lang(lang);
    owners.themes.borrow_mut().set_lang(lang);
    owners.exports.set_lang(app, lang);
    // Chron9. Only the sheet's *composed* strings need saying again — its labels
    // are bound to `Strings` and follow `apply_strings` like the rest.
    if let Some(relocations) = &owners.relocations {
        relocations.set_lang(app, lang);
    }

    crate::apply_strings(app, lang);
    // The picker's rows are looked up in Rust, so they are pushed rather than
    // bound and need saying again.
    theme::show(app, &owners.themes);
    // And one pass over the selection for everything else.
    vault::relabel(&owners.vault, app, &owners.viewer);
}

/// Wire the switch into the window.
///
/// Returns the cell holding the session's language, which is what `main` writes
/// to `config.toml` on the way out — the same shape `Themes::current` has, and
/// for the same reason: reading it from the owner is what stops a stale copy
/// being written.
/// Eight arguments, and clippy is right that that is a lot.
///
/// It is a wire-up function and they are the window, the starting language and
/// the six owners the switch has to reach; bundling them into a struct would put
/// the identical list in `main` one line earlier and add a type whose only
/// purpose is to be built and immediately taken apart. `Owners` below already is
/// that struct — it exists because the *switch* needs the bundle, not because
/// the constructor does.
#[allow(clippy::too_many_arguments)]
pub fn install(
    app: &AppWindow,
    lang: Lang,
    vault: Rc<RefCell<Vault>>,
    viewer: Rc<Viewer>,
    editors: Editors,
    themes: Rc<RefCell<Themes>>,
    exports: Exports,
    relocations: Option<Relocations>,
) -> Rc<Cell<Lang>> {
    let current = Rc::new(Cell::new(lang));
    let owners = Rc::new(Owners {
        vault,
        viewer,
        editors,
        themes,
        exports,
        relocations,
    });

    app.set_lang_mode(lang.index());

    app.on_language_selected({
        let current = Rc::clone(&current);
        let owners = Rc::clone(&owners);
        let weak = app.as_weak();
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            let chosen = Lang::from_index(index);

            // Switching to the language already in effect is a no-op, and
            // deliberately so rather than harmlessly: the re-push runs `plan`,
            // which bumps the viewer's generation token and issues a fresh render
            // request. That is right when something changed and pure waste when
            // nothing did — on a large invoice it is a visible blink for no
            // reason.
            if chosen == current.get() {
                return;
            }
            current.set(chosen);
            app.set_lang_mode(chosen.index());
            switch(&app, &owners, chosen);
        }
    });

    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strings::{self, Key};

    #[test]
    fn languages_round_trip_through_the_menu_index() {
        for &lang in Lang::ALL {
            assert_eq!(Lang::from_index(lang.index()), lang);
        }
        // A click carrying anything else lands on English, like `from_code`.
        assert_eq!(Lang::from_index(-1), Lang::En);
        assert_eq!(Lang::from_index(7), Lang::En);
    }

    #[test]
    fn the_menu_lists_both_languages_in_their_own_language() {
        assert_eq!(Lang::ALL.len(), 2, "CORE §4 ships exactly two");
        // Both tables give the same name for each, which is the point: a reader
        // stranded in the wrong language has to recognise their own.
        for &lang in Lang::ALL {
            let en = strings::get(Lang::En, lang.name());
            let tr = strings::get(Lang::Tr, lang.name());
            assert_eq!(en, tr, "{:?} must read the same in both tables", lang);
            assert!(!en.is_empty());
        }
        assert_eq!(strings::get(Lang::En, Key::LangTurkish), "Türkçe");
        assert_eq!(strings::get(Lang::Tr, Key::LangEnglish), "English");
    }
}
