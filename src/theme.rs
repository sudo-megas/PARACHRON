//! The eleven palettes (CORE §5), and the picker that chooses between them.
//!
//! The palettes are Rust data pushed into the `Palette` global, not conditionals
//! inside it. The Slint answer would be an eleven-way branch on each of twelve
//! colours — a hundred and thirty-two arms the compiler cannot check for
//! completeness and no test can reach. This way the global is a slot, adding a
//! theme is one `const` plus one line in [`Theme::ALL`], and the palettes are
//! testable. That last one is not tidiness: the alternative to a contrast test
//! is looking at eleven screenshots and believing yourself.
//!
//! Every palette walks the same surface ladder — `bg`, `panel`, `raised`,
//! `selection`, `border` — so a theme reads as a ladder rather than as twelve
//! unrelated colours. Where a source palette has fewer than five steps the
//! missing one is interpolated rather than repeated: two roles sharing a value
//! is how an active tab loses its border, or a selected row stops looking
//! selected.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

use crate::strings::{self, Key, Lang};
use crate::{AppWindow, Palette as PaletteGlobal, ThemeItem};

/// One theme's twelve colours, as `0xRRGGBB` — except [`Palette::backdrop`],
/// which is `0xAARRGGBB` because it carries its own alpha.
///
/// Hex integers rather than a colour type so the table reads like the palettes
/// it was copied from, and a value can be checked against an upstream swatch by
/// eye.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: u32,
    pub panel: u32,
    pub raised: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub danger: u32,
    pub selection: u32,
    pub paper: u32,
    pub paper_edge: u32,
    pub backdrop: u32,
}

/// White, in every theme. See `palette.slint` for why this is not a choice.
const PAPER: u32 = 0xffffff;
/// The dim behind a sheet, over a dark theme and over a light one. A light
/// window needs less of it before the card in front reads as in front.
const SCRIM_DARK: u32 = 0x8c000000;
const SCRIM_LIGHT: u32 = 0x73000000;

// ── The eleven ───────────────────────────────────────────────────────────────
//
// Five of these are their projects' published palettes and can be checked
// against them by eye: the four Catppuccin flavours and Rosé Pine Dawn, which is
// what CORE §5's "light/dawn" means. Ubuntu Canonical Aubergine uses Canonical's
// published brand colours. The other four are interpretations pinned here, which
// is what CORE §10 asked this milestone to do — Default Dark was only ever this
// project's own, and Noctalia, Ruby and Paperlike have no single published hex
// set to copy. Saying which is which matters: somebody comparing Mocha against
// the upstream swatch should find it identical, and somebody who thinks Ruby
// should be redder is disagreeing with a choice rather than reporting a bug.

/// Chron1's palette, unchanged — and the initializers in `palette.slint`.
const DEFAULT_DARK: Palette = Palette {
    bg: 0x1b1b1d,
    panel: 0x232326,
    raised: 0x2c2c31,
    selection: 0x34343b,
    border: 0x3a3a40,
    text: 0xe6e6e8,
    muted: 0x9a9aa2,
    accent: 0x6fb2d2,
    danger: 0xe0736d,
    paper: PAPER,
    paper_edge: 0x101012,
    backdrop: SCRIM_DARK,
};

/// The same neutral grey read the other way up, with the accents darkened —
/// which is the step a light theme converted from a dark one always skips.
const DEFAULT_LIGHT: Palette = Palette {
    bg: 0xe9e9ec,
    panel: 0xf7f7f9,
    raised: 0xdedee3,
    selection: 0xd0d0d8,
    border: 0xbcbcc6,
    text: 0x1c1c20,
    muted: 0x5c5c66,
    accent: 0x1f6b8f,
    danger: 0xa52a22,
    paper: PAPER,
    paper_edge: 0xa8a8b2,
    backdrop: SCRIM_LIGHT,
};

/// Interpretation: near-black blue with a lavender accent.
const NOCTALIA: Palette = Palette {
    bg: 0x0d0e14,
    panel: 0x151721,
    raised: 0x1f2230,
    selection: 0x2a2e40,
    border: 0x363b52,
    text: 0xd8dbe8,
    muted: 0x8b90a6,
    accent: 0xa78bfa,
    danger: 0xf0717a,
    paper: PAPER,
    paper_edge: 0x05060a,
    backdrop: SCRIM_DARK,
};

/// Catppuccin Latte. `bg` is mantle and `panel` is base, so panels stay lighter
/// than the canvas the way they do in the dark flavours.
const CATPPUCCIN_LATTE: Palette = Palette {
    bg: 0xe6e9ef,
    panel: 0xeff1f5,
    raised: 0xdce0e8,
    selection: 0xccd0da,
    border: 0xbcc0cc,
    text: 0x4c4f69,
    muted: 0x6c6f85,
    accent: 0x1e66f5,
    danger: 0xd20f39,
    paper: PAPER,
    paper_edge: 0xacb0be,
    backdrop: SCRIM_LIGHT,
};

/// Catppuccin Frappé, whose ladder sits one step lower than its siblings'.
///
/// Frappé is the lightest of the three dark flavours, and putting `selection` on
/// surface1 like Macchiato and Mocha do left its red at 2.70:1 on a selected row
/// — which is what a broken folder's label is drawn in. `selection` takes
/// surface0 instead, surface1 moves out to the hairlines, and the hover step
/// between base and surface0 is interpolated because Catppuccin has no token
/// there. Macchiato and Mocha clear the same floor on surface1 at 3.37 and 3.93,
/// so they are unchanged; this is Frappé's own problem, not the mapping's.
const CATPPUCCIN_FRAPPE: Palette = Palette {
    bg: 0x292c3c,
    panel: 0x303446,
    raised: 0x394050,
    selection: 0x414559,
    border: 0x51576d,
    text: 0xc6d0f5,
    muted: 0xa5adce,
    accent: 0x8caaee,
    danger: 0xe78284,
    paper: PAPER,
    paper_edge: 0x232634,
    backdrop: SCRIM_DARK,
};

const CATPPUCCIN_MACCHIATO: Palette = Palette {
    bg: 0x1e2030,
    panel: 0x24273a,
    raised: 0x363a4f,
    selection: 0x494d64,
    border: 0x5b6078,
    text: 0xcad3f5,
    muted: 0xa5adcb,
    accent: 0x8aadf4,
    danger: 0xed8796,
    paper: PAPER,
    paper_edge: 0x181926,
    backdrop: SCRIM_DARK,
};

const CATPPUCCIN_MOCHA: Palette = Palette {
    bg: 0x181825,
    panel: 0x1e1e2e,
    raised: 0x313244,
    selection: 0x45475a,
    border: 0x585b70,
    text: 0xcdd6f4,
    muted: 0xa6adc8,
    accent: 0x89b4fa,
    danger: 0xf38ba8,
    paper: PAPER,
    paper_edge: 0x11111b,
    backdrop: SCRIM_DARK,
};

/// Rosé Pine Dawn. `accent` is pine and `danger` is love.
///
/// The surfaces walk base, surface, overlay, highlight-med and highlight-high.
/// The page's edge is `muted`, one step beyond the ladder, and it has to be:
/// this is the palette with the least distance between white paper and its
/// canvas — 1.09:1 — so the edge is carrying the page's boundary on its own. The
/// first version reused highlight-high for both `border` and `paper_edge`, which
/// made the page's frame the same colour as every hairline in the window and the
/// faintest of the four light themes' edges.
const ROSE_PINE: Palette = Palette {
    bg: 0xfaf4ed,
    panel: 0xfffaf3,
    raised: 0xf2e9e1,
    selection: 0xdfdad9,
    border: 0xcecacd,
    text: 0x575279,
    muted: 0x797593,
    accent: 0x286983,
    danger: 0xb4637a,
    paper: PAPER,
    paper_edge: 0x9893a5,
    backdrop: SCRIM_LIGHT,
};

/// Interpretation, and the one theme where `danger` is not red.
///
/// In a red-forward theme it cannot be. A broken folder and an expired warranty
/// have to look like something is wrong, and in a window whose accents are
/// already ruby another red is just more of the theme. So the accent is a ruby
/// rose and the error colour is amber — a deliberate departure from "danger is
/// red", and the only palette here where the two roles are different hues on
/// purpose.
const RUBY: Palette = Palette {
    bg: 0x170a0e,
    panel: 0x1f0f14,
    raised: 0x2e171e,
    selection: 0x3d1f28,
    border: 0x4f2a35,
    text: 0xf4e3e7,
    muted: 0xbd979e,
    accent: 0xff6188,
    danger: 0xffa657,
    paper: PAPER,
    // Black, because there is nothing below this canvas. Ruby's `bg` is dark
    // enough that no shadow can be meaningfully darker than it — which does not
    // matter, since a white page against a near-black pane carries its own
    // boundary. See the paper-edge test for the rule that makes this legitimate.
    paper_edge: 0x000000,
    backdrop: SCRIM_DARK,
};

/// Canonical's brand colours: Dark Aubergine as the canvas, Mid and Canonical
/// Aubergine up the ladder, Warm Grey for the quiet text, Ubuntu Orange as the
/// accent. `danger` is a plain red, which orange leaves room for.
///
/// This is the tightest palette of the eleven, and the reason is Canonical's own
/// combination rather than anything here: Ubuntu Orange on aubergine is a
/// genuinely low-contrast pairing. The first version used Canonical Aubergine
/// `#772953` for `selection`, which put the orange at 2.58:1 against the one
/// background a selected row and the picker's tick are ever drawn on. Mid
/// Aubergine takes that slot instead and Canonical Aubergine moves out to the
/// hairlines, which clears the floor without changing a brand colour.
const UBUNTU_AUBERGINE: Palette = Palette {
    bg: 0x2c001e,
    // No Canonical colour sits between Dark and Mid Aubergine, so the two middle
    // steps of the ladder are interpolated.
    panel: 0x3b0d29,
    raised: 0x4a1a3c,
    selection: 0x5e2750,
    border: 0x772953,
    text: 0xf7f2f4,
    muted: 0xaea79f,
    accent: 0xe95420,
    danger: 0xff6b6b,
    paper: PAPER,
    paper_edge: 0x1a0011,
    backdrop: SCRIM_DARK,
};

/// Interpretation, and an honest partial one.
///
/// CORE §5 calls this a gradient theme; a palette is a table of flat colours.
/// A real `@linear-gradient` would mean every themed `background:` in every
/// `.slint` file taking a brush rather than a colour — the whole UI's colour
/// plumbing changed for one theme out of eleven. What ships is the warm
/// near-white ladder that gradient implies, with slate-blue ink and an
/// iron-gall red. A real gradient is a later change to the palette's *type*,
/// not to its values.
const PAPERLIKE: Palette = Palette {
    bg: 0xece6dc,
    panel: 0xf7f3ea,
    raised: 0xe0d9cc,
    selection: 0xd3cabb,
    border: 0xbfb5a4,
    text: 0x2b2721,
    muted: 0x6a6257,
    accent: 0x46687f,
    danger: 0x9c3a34,
    paper: PAPER,
    paper_edge: 0xb5aa98,
    backdrop: SCRIM_LIGHT,
};

/// A theme, by id.
///
/// Order is CORE §5's table order, so the picker can be checked against the
/// specification by reading down it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    Light,
    #[default]
    Dark,
    Noctalia,
    Latte,
    Frappe,
    Macchiato,
    Mocha,
    RosePine,
    Ruby,
    UbuntuAubergine,
    Paperlike,
}

impl Theme {
    /// Every theme, in the order CORE §5 lists them — which is the order the
    /// picker shows and the order the tests walk.
    pub const ALL: &'static [Theme] = &[
        Theme::Light,
        Theme::Dark,
        Theme::Noctalia,
        Theme::Latte,
        Theme::Frappe,
        Theme::Macchiato,
        Theme::Mocha,
        Theme::RosePine,
        Theme::Ruby,
        Theme::UbuntuAubergine,
        Theme::Paperlike,
    ];

    /// Read the value `config.toml` carries, falling back to the default rather
    /// than rejecting a file somebody has typed into.
    pub fn from_code(code: &str) -> Self {
        Theme::ALL
            .iter()
            .copied()
            .find(|theme| theme.code() == code)
            .unwrap_or_default()
    }

    pub fn code(self) -> &'static str {
        match self {
            Theme::Light => "default-light",
            Theme::Dark => "default-dark",
            Theme::Noctalia => "noctalia",
            Theme::Latte => "catppuccin-latte",
            Theme::Frappe => "catppuccin-frappe",
            Theme::Macchiato => "catppuccin-macchiato",
            Theme::Mocha => "catppuccin-mocha",
            Theme::RosePine => "rose-pine",
            Theme::Ruby => "ruby",
            Theme::UbuntuAubergine => "ubuntu-aubergine",
            Theme::Paperlike => "paperlike",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Theme::Light => DEFAULT_LIGHT,
            Theme::Dark => DEFAULT_DARK,
            Theme::Noctalia => NOCTALIA,
            Theme::Latte => CATPPUCCIN_LATTE,
            Theme::Frappe => CATPPUCCIN_FRAPPE,
            Theme::Macchiato => CATPPUCCIN_MACCHIATO,
            Theme::Mocha => CATPPUCCIN_MOCHA,
            Theme::RosePine => ROSE_PINE,
            Theme::Ruby => RUBY,
            Theme::UbuntuAubergine => UBUNTU_AUBERGINE,
            Theme::Paperlike => PAPERLIKE,
        }
    }

    /// The picker's label for this theme (CORE §4: no literals outside the
    /// table, proper nouns included).
    pub fn name(self) -> Key {
        match self {
            Theme::Light => Key::ThemeDefaultLight,
            Theme::Dark => Key::ThemeDefaultDark,
            Theme::Noctalia => Key::ThemeNoctalia,
            Theme::Latte => Key::ThemeCatppuccinLatte,
            Theme::Frappe => Key::ThemeCatppuccinFrappe,
            Theme::Macchiato => Key::ThemeCatppuccinMacchiato,
            Theme::Mocha => Key::ThemeCatppuccinMocha,
            Theme::RosePine => Key::ThemeRosePine,
            Theme::Ruby => Key::ThemeRuby,
            Theme::UbuntuAubergine => Key::ThemeUbuntuAubergine,
            Theme::Paperlike => Key::ThemePaperlike,
        }
    }
}

/// `0xRRGGBB` as an opaque Slint colour.
fn rgb(value: u32) -> Color {
    Color::from_argb_u8(
        0xff,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}

/// `0xAARRGGBB`, for the one role that carries its own alpha.
fn argb(value: u32) -> Color {
    Color::from_argb_u8(
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}

/// Fill the Slint colour table. The counterpart of `apply_strings`.
pub fn apply(app: &AppWindow, theme: Theme) {
    let p = theme.palette();
    let table = app.global::<PaletteGlobal>();
    table.set_bg(rgb(p.bg));
    table.set_panel(rgb(p.panel));
    table.set_raised(rgb(p.raised));
    table.set_border(rgb(p.border));
    table.set_text(rgb(p.text));
    table.set_muted(rgb(p.muted));
    table.set_accent(rgb(p.accent));
    table.set_danger(rgb(p.danger));
    table.set_selection(rgb(p.selection));
    table.set_paper(rgb(p.paper));
    table.set_paper_edge(rgb(p.paper_edge));
    table.set_backdrop(argb(p.backdrop));
}

/// Which theme is in effect, and what the picker is showing.
pub struct Themes {
    current: Theme,
    lang: Lang,
}

impl Themes {
    /// What to write back to `config.toml` on the way out.
    pub fn current(&self) -> Theme {
        self.current
    }

    /// Called by the language switch: two of the eleven names translate, and two
    /// more carry an English common noun.
    ///
    /// Carried an `#[allow(dead_code)]` whose comment said it would come off in
    /// Chron6 when `lang::switch` gained a call to it. Chron6 shipped, the call
    /// exists, and the allowance stayed on until Chron8 audited the four that
    /// were left — the same way `Product`'s outlived its own note by four
    /// milestones. An allowance nobody revisits is a claim about the code that
    /// quietly stops being true.
    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
    }

    fn rows(&self) -> Vec<ThemeItem> {
        Theme::ALL
            .iter()
            .map(|theme| ThemeItem {
                label: SharedString::from(strings::get(self.lang, theme.name())),
                active: *theme == self.current,
            })
            .collect()
    }
}

/// Push the picker's rows. Called on install, on a change, and on a language
/// switch — one place, so the rows and `current` cannot disagree.
pub fn show(app: &AppWindow, themes: &Rc<RefCell<Themes>>) {
    let rows = themes.borrow().rows();
    app.set_theme_rows(ModelRc::new(VecModel::from(rows)));
}

/// Wire the picker into the window and paint the window with `theme`.
pub fn install(app: &AppWindow, theme: Theme, lang: Lang) -> Rc<RefCell<Themes>> {
    let themes = Rc::new(RefCell::new(Themes {
        current: theme,
        lang,
    }));

    apply(app, theme);

    app.on_theme_selected({
        let themes = Rc::clone(&themes);
        let weak = app.as_weak();
        move |index| {
            let Some(app) = weak.upgrade() else { return };

            // The borrow ends with this statement. Slint setters can run
            // bindings that call straight back into this callback, and a
            // `RefCell` borrowed twice is a panic — the rule `vault.rs` follows.
            let chosen = {
                let mut themes = themes.borrow_mut();
                let Some(&chosen) = usize::try_from(index)
                    .ok()
                    .and_then(|index| Theme::ALL.get(index))
                else {
                    return;
                };
                if chosen == themes.current {
                    return;
                }
                themes.current = chosen;
                chosen
            };

            // CORE §5: switching is instant. This repaints the sheet the click
            // happened in, which is the most direct demonstration it took.
            apply(&app, chosen);
            show(&app, &themes);
        }
    });

    show(app, &themes);
    themes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One channel of an `0xRRGGBB` value, linearized for the luminance sum.
    fn channel(byte: u8) -> f64 {
        let c = f64::from(byte) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    /// WCAG relative luminance.
    fn luminance(value: u32) -> f64 {
        let r = channel((value >> 16) as u8);
        let g = channel((value >> 8) as u8);
        let b = channel(value as u8);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// WCAG contrast ratio, 1.0 (identical) to 21.0 (black on white).
    fn contrast(a: u32, b: u32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        let (light, dark) = if a > b { (a, b) } else { (b, a) };
        (light + 0.05) / (dark + 0.05)
    }

    /// Body text has to clear this wherever it is drawn.
    const BODY_FLOOR: f64 = 4.5;
    /// `muted`, `accent` and `danger` are held to the large-text floor instead.
    ///
    /// `muted` is deliberately quiet — holding a colour that means "secondary"
    /// to the body-text ratio would mean it was not secondary. `accent` and
    /// `danger` are the same story: their largest uses are the 26px counter and
    /// a chip's outline, and their small uses never rely on colour alone — an
    /// error line comes with a red field outline, a broken row with a `⚠`
    /// prefix, so the colour is a second signal rather than the only one.
    ///
    /// This is a floor, not a design review. Its job is catching a light theme
    /// built by inverting a dark one and forgetting the accents, which is the
    /// failure a table of eleven invites. It says nothing about whether Frappé
    /// is pretty.
    const QUIET_FLOOR: f64 = 3.0;

    #[test]
    fn the_contrast_check_agrees_with_its_own_reference_points() {
        // Black on white is WCAG's maximum, white on white its minimum.
        assert!((contrast(0x000000, 0xffffff) - 21.0).abs() < 0.01);
        assert!((contrast(0xffffff, 0xffffff) - 1.0).abs() < 0.001);
        // Symmetric, so the argument order cannot matter.
        assert!((contrast(0x1b1b1d, 0xe6e6e8) - contrast(0xe6e6e8, 0x1b1b1d)).abs() < 1e-9);
    }

    #[test]
    fn every_palette_is_readable() {
        for &theme in Theme::ALL {
            let p = theme.palette();
            let code = theme.code();

            for (surface, name) in [(p.bg, "bg"), (p.panel, "panel")] {
                let ratio = contrast(p.text, surface);
                assert!(
                    ratio >= BODY_FLOOR,
                    "{code}: text on {name} is {ratio:.2}:1, below the {BODY_FLOOR}:1 floor"
                );
            }

            // `selection` is a background too, and until this assertion existed
            // nothing said so. A selected product row is drawn on it, and its
            // label is `text`, or `accent` when files are missing, or `danger`
            // when the folder will not parse; an active sort chip is `selection`
            // filled with an `accent` outline; and the picker's tick is `accent`
            // on the active row, which is the only background that tick ever has.
            //
            // Ubuntu Canonical Aubergine failed this when it was written, at
            // 2.58:1 — Ubuntu Orange on Canonical Aubergine. The floor that was
            // here measured `accent` against `panel`, a surface some of these are
            // never drawn on, so it passed a palette whose most visible accent
            // pairing was unreadable.
            for surface in [p.panel, p.selection] {
                let ratio = contrast(p.text, surface);
                assert!(
                    ratio >= BODY_FLOOR,
                    "{code}: text on {surface:#08x} is {ratio:.2}:1, below the \
                     {BODY_FLOOR}:1 floor"
                );
            }

            for (colour, name) in [(p.muted, "muted"), (p.accent, "accent"), (p.danger, "danger")] {
                for (surface, where_) in [(p.panel, "panel"), (p.selection, "selection")] {
                    let ratio = contrast(colour, surface);
                    assert!(
                        ratio >= QUIET_FLOOR,
                        "{code}: {name} on {where_} is {ratio:.2}:1, below the \
                         {QUIET_FLOOR}:1 floor"
                    );
                }
            }

            // A page has to be distinguishable from the pane behind it. It gets
            // that either from its own brightness or from its edge, and which
            // one carries it depends on the theme rather than being a choice.
            //
            // On a dark theme white paper against a near-black pane is already a
            // hard boundary, and no shadow can be meaningfully darker than a
            // canvas that is nearly black — there is no room below it. On a light
            // theme white paper against a light canvas is barely a boundary at
            // all, and the edge is the only thing doing the work, so it has to be
            // clearly darker than the canvas.
            //
            // Ruby is what taught this rule: its edge was a near-black one step
            // off its near-black canvas, which read as no edge, and the version of
            // this assertion that only checked the edge failed it while having
            // nothing useful to suggest.
            let by_paper = contrast(p.paper, p.bg);
            let by_edge = contrast(p.paper_edge, p.bg);
            assert!(
                by_paper >= 3.0 || by_edge >= 1.3,
                "{code}: a page has no boundary — paper is {by_paper:.2}:1 against \
                 the pane and its edge only {by_edge:.2}:1"
            );

            // And the edge has to be darker than the page, or it is a highlight
            // around white paper rather than a shadow under it.
            assert!(
                luminance(p.paper_edge) < luminance(p.paper),
                "{code}: paper-edge is lighter than the page it frames"
            );
        }
    }

    /// The ladder is what makes a theme readable as a theme rather than as
    /// twelve unrelated colours, and a repeated step is a real defect: an active
    /// tab whose border matches its fill has no border, and a selected row the
    /// colour of a hover does not look selected.
    #[test]
    fn every_palette_walks_five_distinct_surfaces() {
        for &theme in Theme::ALL {
            let p = theme.palette();

            // `paper_edge` is checked for distinctness with the surfaces but is
            // deliberately *not* part of the ladder below: on a dark theme it sits
            // beyond `bg` rather than between the steps, so including it in the
            // monotonic check would fail every dark palette. It still must not be
            // a copy of one of them — Rosé Pine's was a second `border`, which
            // made the page's frame the same colour as every hairline in the
            // window.
            let distinct = [p.bg, p.panel, p.raised, p.selection, p.border, p.paper_edge];
            let names = ["bg", "panel", "raised", "selection", "border", "paper-edge"];
            for (i, &a) in distinct.iter().enumerate() {
                for (j, &b) in distinct.iter().enumerate().skip(i + 1) {
                    assert_ne!(
                        a, b,
                        "{}: {} and {} are both {a:#08x}",
                        theme.code(),
                        names[i],
                        names[j]
                    );
                }
            }

            // Monotonic away from the canvas, in whichever direction the theme
            // runs. `raised` being on the wrong side of `panel` is exactly the
            // mistake a light theme converted from a dark one makes.
            let steps = [
                luminance(p.panel),
                luminance(p.raised),
                luminance(p.selection),
                luminance(p.border),
            ];
            let rising = steps[1] > steps[0];
            for pair in steps.windows(2) {
                assert_eq!(
                    pair[1] > pair[0],
                    rising,
                    "{}: the surface ladder changes direction at {:.4} → {:.4}",
                    theme.code(),
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    /// A panel that sits above the canvas has to be lighter than it, in light
    /// themes as well as dark ones — it is what makes the three columns legible
    /// without the hairlines doing all the work.
    #[test]
    fn panels_sit_above_the_canvas_in_every_theme() {
        for &theme in Theme::ALL {
            let p = theme.palette();
            assert!(
                luminance(p.panel) > luminance(p.bg),
                "{}: panel is darker than the canvas",
                theme.code()
            );
        }
    }

    #[test]
    fn paper_is_white_everywhere() {
        for &theme in Theme::ALL {
            assert_eq!(
                theme.palette().paper,
                PAPER,
                "{}: a themed paper would flash a colour the page is not",
                theme.code()
            );
        }
    }

    #[test]
    fn a_sheet_is_dimmed_over_every_theme_and_never_opaque() {
        for &theme in Theme::ALL {
            let alpha = theme.palette().backdrop >> 24;
            assert!(
                (0x40..0xf0).contains(&alpha),
                "{}: backdrop alpha {alpha:#04x} either hides the window or does not dim it",
                theme.code()
            );
        }
    }

    #[test]
    fn themes_round_trip_through_the_config_value() {
        for &theme in Theme::ALL {
            assert_eq!(Theme::from_code(theme.code()), theme);
        }
        // A config somebody has typed into falls back rather than failing.
        assert_eq!(Theme::from_code("sunset"), Theme::Dark);
        assert_eq!(Theme::from_code(""), Theme::Dark);
        assert_eq!(Theme::default(), Theme::Dark, "CORE §3's config default");
    }

    #[test]
    fn the_theme_list_covers_the_whole_enum_and_has_no_duplicates() {
        // `palette()`, `code()` and `name()` all match exhaustively, so a theme
        // missing from `ALL` is the only way one can go untested.
        assert_eq!(Theme::ALL.len(), 11, "CORE §5 lists eleven themes");

        let mut codes: Vec<&str> = Theme::ALL.iter().map(|t| t.code()).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(before, codes.len(), "two themes share a config id");

        let mut names: Vec<Key> = Theme::ALL.iter().map(|t| t.name()).collect();
        names.sort_unstable_by_key(|key| format!("{key:?}"));
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "two themes share a name key");
    }

    #[test]
    fn no_two_themes_are_the_same_palette() {
        for (i, &a) in Theme::ALL.iter().enumerate() {
            for &b in &Theme::ALL[i + 1..] {
                assert_ne!(
                    a.palette(),
                    b.palette(),
                    "{} and {} are the same eleven-of-eleven",
                    a.code(),
                    b.code()
                );
            }
        }
    }

    /// The initializers in `palette.slint` are Default Dark, which is what makes
    /// the pre-push frame identical to the post-push one on a default start. If
    /// this drifts, the app flashes on every launch.
    #[test]
    fn default_dark_is_still_chron1s_palette() {
        let p = Theme::Dark.palette();
        assert_eq!(p.bg, 0x1b1b1d);
        assert_eq!(p.panel, 0x232326);
        assert_eq!(p.raised, 0x2c2c31);
        assert_eq!(p.border, 0x3a3a40);
        assert_eq!(p.text, 0xe6e6e8);
        assert_eq!(p.muted, 0x9a9aa2);
        assert_eq!(p.accent, 0x6fb2d2);
        assert_eq!(p.danger, 0xe0736d);
        assert_eq!(p.selection, 0x34343b);
        assert_eq!(p.paper_edge, 0x101012);
    }

    /// Ten of eleven. Ruby is the exception and says why at its definition.
    #[test]
    fn danger_is_a_red_everywhere_but_ruby() {
        for &theme in Theme::ALL {
            let d = theme.palette().danger;
            let (r, g, b) = ((d >> 16) & 0xff, (d >> 8) & 0xff, d & 0xff);
            if theme == Theme::Ruby {
                assert!(g > b, "Ruby's danger is amber, which is warmer than red");
                continue;
            }
            assert!(
                r > g && r > b,
                "{}: danger {d:#08x} is not a red",
                theme.code()
            );
        }
    }

    /// Every colour a picker row shows exists in both languages. The names
    /// themselves are reviewed in Chron6; this only pins that none is missing.
    #[test]
    fn every_theme_has_a_name_in_both_languages() {
        for &theme in Theme::ALL {
            for lang in [Lang::En, Lang::Tr] {
                assert!(
                    !strings::get(lang, theme.name()).is_empty(),
                    "{} has no name in {lang:?}",
                    theme.code()
                );
            }
        }
    }
}
