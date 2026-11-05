//! Design tokens and fonts.
//!
//! Colours are Jolly Roger Office's shared pirate palette: Parchment (light) and Midnight Sea
//! (dark), with a deep-sea ribbon band and brass accents (see [`Tokens::palette`]). Fonts come from egui's
//! bundled faces and the operating system at runtime: the UI uses the platform UI font, cells use
//! the best available match for the workbook font (Calibri/Carlito/Aptos/Arial…), and egui's
//! bundled Ubuntu/Hack/egui-icons and monochrome emoji faces are the last resort. A build with
//! `CRAFT_FONTS_DIR` also embeds the permitted OFL craft-fonts faces (barred Adobe/Source/Noto CJK
//! families never enter it, and a web build keeps only its explicit allowlist). Native builds opt
//! into craft-fonts' OFL LXGW Chinese face, so a host without a Han system face still draws Chinese
//! interface text; no font file is stored in this repo.
//!
//! Chrome colours belong in [`Tokens`], each with a light and a dark value; colours
//! drawn on the sheet keep their light value in both themes, because the sheet stays white. The
//! foreground/background pairs are listed in `contrast_tests`, which holds them to WCAG 2 (4.5:1
//! for text, 3:1 for glyphs) in both themes: a new token goes there too.

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use egui::epaint::text::Blob;
use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};

mod font_fallback;
mod font_setup;

pub use font_fallback::symbol_face_installed;
#[cfg(test)]
use font_fallback::{cjk_han_japanese, cjk_han_simplified, cjk_hangul, fallback_faces, hebrew, lao, symbols_more, thai};
use font_setup::pick;
pub use font_setup::{
    CELL, CELL_BOLD, CELL_BOLD_ITALIC, CELL_ITALIC, CraftFont, HanOrder, MATH, MATH_BOLD, MATH_ITALIC, MONO, SERIF, UI, UI_BOLD, available_text,
    cell_family, cell_text_width, chinese_font_loaded, craft_font_scripts, craft_fonts_cover, craft_fonts_loaded, craft_fonts_required,
    font_definitions, font_definitions_for_language, font_definitions_with, pinned_font_definitions, symbol_fallback, ui_bold, ui_font,
};
#[cfg(test)]
use font_setup::{candidates, craft_font_faces, try_load};
#[cfg(test)]
pub(crate) use font_setup::{drop_unparseable_fonts, font_parses};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub window: Color32,
    pub ribbon: Color32,
    pub ribbon_border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    pub accent_dark: Color32,
    pub accent_soft: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub separator: Color32,
    pub grid_bg: Color32,
    /// Behind the pages in Page Layout view.
    pub desk: Color32,
    pub gridline: Color32,
    pub header_bg: Color32,
    pub header_text: Color32,
    pub header_line: Color32,
    pub header_sel_bg: Color32,
    pub header_sel_text: Color32,
    pub header_all_bg: Color32,
    pub sel_fill: Color32,
    /// The tint over the active cell's row (File ▸ Options ▸ General).
    pub active_row: Color32,
    pub sel_border: Color32,
    pub input_bg: Color32,
    pub input_border: Color32,
    pub tab_bar: Color32,
    pub tab_active: Color32,
    pub status_bar: Color32,
    pub cell_text: Color32,
    pub menu_bg: Color32,
    pub shadow: Color32,
    pub danger: Color32,
    /// Background error checking: the corner triangle, and the warning button's diamond and mark.
    pub error_check_green: Color32,
    pub warning_fill: Color32,
    pub warning_ink: Color32,
    /// Info-bar (notice strip) fills, by severity.
    pub notice_warn_bg: Color32,
    pub notice_warn_border: Color32,
    pub notice_info_bg: Color32,
    pub notice_info_border: Color32,
    /// Text and glyphs drawn on an `accent` fill (primary buttons, switched-on toggles).
    pub on_accent: Color32,
    /// The category palette, indexed by [`Cat`]: the coloured parts of icons, gallery highlights.
    pub cat: [Color32; 8],
    /// The page colour inside icons (documents, cells, clipboards).
    pub paper: Color32,
    /// Reference colours for formulas edited in the window-coloured formula bar.
    pub ref_colors: [Color32; 8],
    /// Reference colours on the sheet (outlines and the in-cell editor). The sheet stays white.
    pub sheet_ref_colors: [Color32; 8],
    /// Code colours in the code editor (and the window-coloured fields it sits in).
    pub syntax: Syntax,
    /// The message box warning disc and its glyph.
    pub warning: Color32,
    pub on_warning: Color32,
    /// Controls drawn on the sheet (filter and validation buttons, object handles). They follow
    /// the sheet, so they are light in both themes.
    pub sheet_control_bg: Color32,
    pub sheet_control_border: Color32,
    pub sheet_control_text: Color32,
    /// A placeholder fill on the sheet (a picture that has not loaded), and a form slider's track.
    pub sheet_track: Color32,
    /// A form slider's thumb.
    pub sheet_thumb: Color32,
    /// A form button or spin arrow under the pointer, and pressed.
    pub sheet_control_hover: Color32,
    pub sheet_control_pressed: Color32,
    /// The filled part of a form slider's track (from the minimum to the thumb).
    pub sheet_fill: Color32,
    /// The ring around the form control that takes the arrow keys.
    pub sheet_focus: Color32,
    /// The macro recording dot (Record Macro, the status bar).
    pub recording: Color32,
    /// A breakpoint dot in the macro step editor's gutter.
    pub breakpoint: Color32,
    /// Macro overlays on the sheet (Check and debugging): the inputs, the cells the macro writes,
    /// the cells it would write over, and the current step's target. Sheet colours.
    pub sheet_macro_input: Color32,
    pub sheet_macro_footprint: Color32,
    pub sheet_macro_conflict: Color32,
    pub sheet_macro_target: Color32,
    /// The ribbon band (Jolly Roger Office's ribbon colour): behind the title bar and the ribbon's
    /// tab row, with its own text, hover and accent colours ([`Tokens::on_band`]).
    pub band: Color32,
    pub band_text: Color32,
    pub band_text_dim: Color32,
    pub band_hover: Color32,
    pub band_pressed: Color32,
    pub band_accent: Color32,
    /// The PY badge in the formula bar (Python mode) and its text.
    pub badge_py: Color32,
    pub on_badge: Color32,
    /// The PY tag on a Python cell and its text. They follow the sheet, so they are the same in
    /// both themes.
    pub sheet_py_tag: Color32,
    pub on_sheet_py_tag: Color32,
    /// Trace arrows on the sheet: precedents and dependents, and the links Trace Error follows
    /// between cells showing an error. Sheet colours.
    pub sheet_trace_arrow: Color32,
    pub sheet_trace_error: Color32,
}

/// The eight category colours, in the order of [`Tokens::cat`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cat {
    Blue,
    Green,
    Orange,
    Red,
    Purple,
    Teal,
    Amber,
    Slate,
}

impl Cat {
    pub const ALL: [Cat; 8] = [Cat::Blue, Cat::Green, Cat::Orange, Cat::Red, Cat::Purple, Cat::Teal, Cat::Amber, Cat::Slate];
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Reference colours on the white sheet, the same in both themes (the sheet does not follow the
/// theme). The light theme's formula bar is white too, so it uses them as well.
const SHEET_REF_COLORS: [Color32; 8] =
    [rgb(0x2E6FD8), rgb(0xD03C3C), rgb(0x7A4FC9), rgb(0x23803D), rgb(0xB5531A), rgb(0x16798A), rgb(0xB83B8E), rgb(0x77681A)];
/// Macro overlays on the sheet, the same in both themes (the sheet stays white).
const SHEET_MACRO_INPUT: Color32 = rgb(0x2E6FD8);
const SHEET_MACRO_FOOTPRINT: Color32 = rgb(0x23803D);
const SHEET_MACRO_CONFLICT: Color32 = rgb(0xC42B1C);
const SHEET_MACRO_TARGET: Color32 = rgb(0x6B3FC0);
/// Trace arrows on the sheet, the same in both themes.
const SHEET_TRACE_ARROW: Color32 = rgb(0x1F5FC9);
const SHEET_TRACE_ERROR: Color32 = rgb(0xD13438);

/// Code colours for one background; plain text uses the surface's own text colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Syntax {
    pub keyword: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub builtin: Color32,
}

const LIGHT_SYNTAX: Syntax =
    Syntax { keyword: rgb(0x0B57B3), string: rgb(0xA3361B), number: rgb(0x157A5A), comment: rgb(0x6B7280), builtin: rgb(0x7B3FB5) };
const DARK_SYNTAX: Syntax =
    Syntax { keyword: rgb(0x6FAEFF), string: rgb(0xF09A7A), number: rgb(0x7FD1A8), comment: rgb(0x9AA3AE), builtin: rgb(0xC69BF2) };
const SHEET_CONTROL_BG: Color32 = rgb(0xF3F3F3);
const SHEET_CONTROL_BORDER: Color32 = rgb(0x8A8A8A);
const SHEET_CONTROL_TEXT: Color32 = rgb(0x1F1F1F);
const SHEET_TRACK: Color32 = rgb(0xD6D6D6);
const SHEET_THUMB: Color32 = rgb(0x6B6B6B);
const SHEET_CONTROL_HOVER: Color32 = rgb(0xE4E4E4);
const SHEET_CONTROL_PRESSED: Color32 = rgb(0xCFCFCF);
const SHEET_FILL: Color32 = rgb(0x107C41);
const SHEET_FOCUS: Color32 = rgb(0x107C41);
/// The PY tag on Python cells: a deep green, white text on it.
const SHEET_PY_TAG: Color32 = rgb(0x1E6E4E);

impl Tokens {
    pub fn light() -> Tokens {
        Tokens::palette(false)
    }

    pub fn dark() -> Tokens {
        Tokens::palette(true)
    }

    /// One palette for both modes: each colour is written once, with its light and dark values.
    ///
    /// Jolly Roger Office's shared palette: Parchment (light) and Midnight Sea (dark). The spec's
    /// colours are used as given (`brand` roles: background, panel, ribbon band, text, accent,
    /// danger, selection, borders); the other chrome roles (hover, pressed, dim text, header fills)
    /// are tints between them, chosen to pass `contrast_tests`. Where a spec colour is too faint
    /// for text on its surface (brass on parchment), the fill keeps the spec colour and text uses
    /// `accent_dark`, a deeper shade of it.
    fn palette(dark: bool) -> Tokens {
        Tokens {
            dark,
            // Spec: background parchment / midnight.
            window: pick(dark, rgb(0xF4E9D0), rgb(0x0F1B24)),
            // Spec: panel.
            ribbon: pick(dark, rgb(0xEADBB8), rgb(0x16262F)),
            // Spec: borders (weathered wood / sea slate).
            ribbon_border: pick(dark, rgb(0x8C6D46), rgb(0x3A4F5C)),
            // Spec: text (ink / sail).
            text: pick(dark, rgb(0x2B1D0E), rgb(0xE8DCC0)),
            text_dim: pick(dark, rgb(0x5A4530), rgb(0xB5AA92)),
            text_disabled: pick(dark, rgb(0xA38F70), rgb(0x667680)),
            // Spec: accent (brass / gold).
            accent: pick(dark, rgb(0xB8860B), rgb(0xD4A017)),
            accent_dark: pick(dark, rgb(0x6B4E05), rgb(0xD4A017)),
            // Spec: selection #C9A227 at 35 % over the background.
            accent_soft: pick(dark, rgb(0xE5D095), rgb(0x504A25)),
            hover: pick(dark, rgb(0xE6D5AE), rgb(0x1E3340)),
            pressed: pick(dark, rgb(0xDCC799), rgb(0x284252)),
            separator: pick(dark, rgb(0xCDB892), rgb(0x2A3D49)),
            grid_bg: rgb(0xFFFDF8),
            desk: pick(dark, rgb(0xDCCDA8), rgb(0x0A141B)),
            gridline: rgb(0xE4DBC8),
            header_bg: pick(dark, rgb(0xEADBB8), rgb(0x16262F)),
            header_text: pick(dark, rgb(0x4A3826), rgb(0xCFC3A8)),
            header_line: pick(dark, rgb(0xC4AE85), rgb(0x3A4F5C)),
            header_sel_bg: pick(dark, rgb(0xDDC27A), rgb(0x4A4222)),
            header_sel_text: pick(dark, rgb(0x2B1D0E), rgb(0xF2DC96)),
            header_all_bg: pick(dark, rgb(0xD2B76A), rgb(0x5A4E1E)),
            // Spec: selection #C9A227 at 35 %.
            sel_fill: Color32::from_rgba_unmultiplied(0xC9, 0xA2, 0x27, 0x59),
            sel_border: rgb(0xB8860B),
            // The active row band (upstream's option), in the selection's brass.
            active_row: pick(dark, Color32::from_rgba_unmultiplied(0xC9, 0xA2, 0x27, 0x2C), Color32::from_rgba_unmultiplied(0xD4, 0xA0, 0x17, 0x30)),
            input_bg: pick(dark, rgb(0xFFFBF0), rgb(0x0A141B)),
            input_border: pick(dark, rgb(0x8C6D46), rgb(0x3A4F5C)),
            tab_bar: pick(dark, rgb(0xF4E9D0), rgb(0x0F1B24)),
            tab_active: pick(dark, rgb(0xFFFBF0), rgb(0x16262F)),
            status_bar: pick(dark, rgb(0xEADBB8), rgb(0x16262F)),
            cell_text: Color32::BLACK,
            menu_bg: pick(dark, rgb(0xFAF2DF), rgb(0x16262F)),
            shadow: pick(dark, Color32::from_black_alpha(46), Color32::from_black_alpha(110)),
            // Spec: danger (oxblood / signal red). The spec's dark #C0392B is under 4.5:1 for text
            // on Midnight Sea, so dark danger text uses a lighter tint of it.
            danger: pick(dark, rgb(0x8B1E1E), rgb(0xF07A6E)),
            error_check_green: pick(dark, rgb(0x1E7A3A), rgb(0x4CC26B)),
            warning_fill: rgb(0xD4A017),
            warning_ink: rgb(0x2B1D0E),
            notice_warn_bg: pick(dark, rgb(0xF5E2A6), rgb(0x3A3218)),
            notice_warn_border: pick(dark, rgb(0xC9A227), rgb(0x7A6420)),
            notice_info_bg: pick(dark, rgb(0xDCE6EA), rgb(0x17303D)),
            notice_info_border: pick(dark, rgb(0x8FA9B8), rgb(0x2F5568)),
            on_accent: pick(dark, rgb(0x2B1D0E), rgb(0x0F1B24)),
            cat: pick(
                dark,
                [rgb(0x1F5FA8), rgb(0x2C6E3A), rgb(0xA4500F), rgb(0x8B1E1E), rgb(0x6A3D9A), rgb(0x116873), rgb(0x7A5A08), rgb(0x4B5866)],
                [rgb(0x6EA8F7), rgb(0x5CC587), rgb(0xF0965A), rgb(0xF07B6E), rgb(0xB394EE), rgb(0x4CC7CF), rgb(0xF2C14E), rgb(0xA9B6C4)],
            ),
            paper: pick(dark, rgb(0xFFFBF0), rgb(0x2A3D49)),
            ref_colors: pick(
                dark,
                SHEET_REF_COLORS,
                [rgb(0x6FA3F5), rgb(0xF27C7C), rgb(0xB394EE), rgb(0x6CCB85), rgb(0xF0A061), rgb(0x5CC9DA), rgb(0xE77FC4), rgb(0xD4C25A)],
            ),
            sheet_ref_colors: SHEET_REF_COLORS,
            syntax: pick(dark, LIGHT_SYNTAX, DARK_SYNTAX),
            warning: rgb(0xD4A017),
            on_warning: rgb(0x2B1D0E),
            sheet_control_bg: SHEET_CONTROL_BG,
            sheet_control_border: SHEET_CONTROL_BORDER,
            sheet_control_text: SHEET_CONTROL_TEXT,
            sheet_track: SHEET_TRACK,
            sheet_thumb: SHEET_THUMB,
            sheet_control_hover: SHEET_CONTROL_HOVER,
            sheet_control_pressed: SHEET_CONTROL_PRESSED,
            sheet_fill: SHEET_FILL,
            sheet_focus: SHEET_FOCUS,
            recording: pick(dark, rgb(0x8B1E1E), rgb(0xF1705F)),
            breakpoint: pick(dark, rgb(0x8B1E1E), rgb(0xF1707A)),
            sheet_macro_input: SHEET_MACRO_INPUT,
            sheet_macro_footprint: SHEET_MACRO_FOOTPRINT,
            sheet_macro_conflict: SHEET_MACRO_CONFLICT,
            sheet_macro_target: SHEET_MACRO_TARGET,
            band: pick(dark, rgb(0x1F3A4D), rgb(0x0A141B)),
            band_text: pick(dark, rgb(0xF4E9D0), rgb(0xE8DCC0)),
            band_text_dim: pick(dark, rgb(0xCDBF9C), rgb(0xA99E86)),
            band_hover: pick(dark, rgb(0x223F55), rgb(0x16262F)),
            band_pressed: pick(dark, rgb(0x36607A), rgb(0x1E3340)),
            band_accent: rgb(0xD4A017),
            badge_py: pick(dark, SHEET_PY_TAG, rgb(0x5CBF94)),
            on_badge: pick(dark, Color32::WHITE, rgb(0x0B1F17)),
            sheet_py_tag: SHEET_PY_TAG,
            on_sheet_py_tag: Color32::WHITE,
            sheet_trace_arrow: SHEET_TRACE_ARROW,
            sheet_trace_error: SHEET_TRACE_ERROR,
        }
    }

    /// The tokens for controls drawn on the ribbon band (the title bar and the ribbon's tab row):
    /// the spec's ribbon colour behind ribbon text, with the gold accent.
    pub fn on_band(&self) -> Tokens {
        Tokens {
            window: self.band,
            text: self.band_text,
            text_dim: self.band_text_dim,
            text_disabled: self.band_hover.lerp_to_gamma(self.band_text_dim, 0.55),
            hover: self.band_hover,
            pressed: self.band_pressed,
            accent: self.band_accent,
            accent_dark: self.band_accent,
            on_accent: rgb(0x1F3A4D),
            ..*self
        }
    }

    pub fn get(ctx: &egui::Context) -> Tokens {
        if ctx.global_style().visuals.dark_mode { Tokens::dark() } else { Tokens::light() }
    }

    /// One colour of the category palette.
    pub fn cat(&self, c: Cat) -> Color32 {
        self.cat.get(c as usize).copied().unwrap_or(self.text)
    }

    /// The reference colour for a formula's `index`-th reference, on the bar or on the sheet.
    pub fn ref_color(&self, index: usize, on_sheet: bool) -> Color32 {
        let set = if on_sheet { &self.sheet_ref_colors } else { &self.ref_colors };
        set.get(index % set.len()).copied().unwrap_or(self.text)
    }
}

/// WCAG 2 relative luminance of an opaque colour.
pub fn luminance(c: Color32) -> f32 {
    let lin = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG 2 contrast ratio between two opaque colours (1 to 21).
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Installs light and dark token visuals in both theme slots, keeps the 2-3 px widget and 8-10 px window/menu corner radii from 0.36, and sets spacing, text styles and the active theme.
pub fn apply(ctx: &egui::Context, dark: bool) {
    // Install both palettes before System can switch between the style slots.
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let dark = theme == egui::Theme::Dark;
        let t = if dark { Tokens::dark() } else { Tokens::light() };
        let mut v = if dark { Visuals::dark() } else { Visuals::light() };
        v.panel_fill = t.window;
        v.window_fill = t.menu_bg;
        v.extreme_bg_color = t.input_bg;
        v.selection.bg_fill = t.accent_soft;
        v.selection.stroke = Stroke::new(1.0, t.accent_dark);
        v.hyperlink_color = t.accent_dark;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.hovered.weak_bg_fill = t.hover;
        v.widgets.active.weak_bg_fill = t.pressed;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.input_border);
        // 0.37 raises the default widget corner radii; GridCraft keeps its 0.36 shape (#8617).
        v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(2);
        v.widgets.inactive.corner_radius = egui::CornerRadius::same(2);
        v.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
        v.widgets.active.corner_radius = egui::CornerRadius::same(2);
        v.widgets.open.corner_radius = egui::CornerRadius::same(2);
        v.window_corner_radius = egui::CornerRadius::same(10);
        v.menu_corner_radius = egui::CornerRadius::same(8);
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: t.shadow };
        ctx.set_visuals_of(theme, v);
    }
    ctx.options_mut(|o| o.fallback_theme = egui::Theme::Light);
    ctx.set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.text_styles.insert(egui::TextStyle::Body, ui_font(13.0));
        s.text_styles.insert(egui::TextStyle::Button, ui_font(13.0));
        s.text_styles.insert(egui::TextStyle::Small, ui_font(11.0));
        s.text_styles.insert(egui::TextStyle::Heading, ui_bold(18.0));
        s.text_styles.insert(egui::TextStyle::Monospace, FontId::new(12.0, FontFamily::Name(MONO.into())));
    });
}

/// Model colour → egui colour.
pub fn color32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod contrast_tests {
    use super::*;

    /// Body text needs 4.5:1 (WCAG AA); glyphs, icons and other graphics need 3:1.
    const TEXT: f32 = 4.5;
    const GLYPH: f32 = 3.0;

    /// Every foreground/background pair the chrome draws: ribbon, popups, dialogs, panes, formula
    /// bar, status bar, plus the colours that sit on the white sheet.
    fn pairs(t: &Tokens) -> Vec<(String, Color32, Color32, f32)> {
        let mut v: Vec<(String, Color32, Color32, f32)> = Vec::new();
        let mut add = |name: &str, fg: Color32, bg: Color32, min: f32| v.push((name.to_string(), fg, bg, min));
        let surfaces = [
            ("window", t.window),
            ("ribbon", t.ribbon),
            ("menu", t.menu_bg),
            ("input", t.input_bg),
            ("status bar", t.status_bar),
            ("tab bar", t.tab_bar),
            ("active tab", t.tab_active),
            ("hover", t.hover),
        ];
        for (name, bg) in surfaces {
            add(&format!("text on {name}"), t.text, bg, TEXT);
            add(&format!("dim text on {name}"), t.text_dim, bg, TEXT);
            add(&format!("accent glyph on {name}"), t.accent_dark, bg, GLYPH);
            add(&format!("danger on {name}"), t.danger, bg, TEXT);
        }
        add("text on pressed", t.text, t.pressed, TEXT);
        add("accent text (File, contextual tabs) on window", t.accent_dark, t.window, TEXT);
        add("on_accent on accent fill", t.on_accent, t.accent, TEXT);
        // The ribbon band (title bar and ribbon tabs).
        let b = t.on_band();
        for (name, fg, min) in [("text", b.text, TEXT), ("dim text", b.text_dim, TEXT), ("accent", b.accent, TEXT)] {
            add(&format!("band {name}"), fg, b.window, min);
            add(&format!("band {name} on hover"), fg, b.hover, min);
        }
        for (name, bg) in [("ribbon", t.ribbon), ("menu", t.menu_bg), ("hover", t.hover), ("pressed", t.pressed)] {
            for c in Cat::ALL {
                add(&format!("{c:?} icon colour on {name}"), t.cat(c), bg, GLYPH);
            }
        }
        add("icon stroke on paper", t.text, t.paper, GLYPH);
        add("on_accent on accent", t.on_accent, t.accent, TEXT);
        add("warning glyph", t.on_warning, t.warning, TEXT);
        add("notice text (warning)", t.text, t.notice_warn_bg, TEXT);
        add("notice text (info)", t.text, t.notice_info_bg, TEXT);
        add("header text", t.header_text, t.header_bg, TEXT);
        add("selected header text", t.header_sel_text, t.header_sel_bg, TEXT);
        for (i, c) in t.ref_colors.iter().enumerate() {
            add(&format!("formula bar reference {i}"), *c, t.input_bg, TEXT);
        }
        let s = t.syntax;
        for (name, c) in [("keyword", s.keyword), ("string", s.string), ("number", s.number), ("comment", s.comment), ("builtin", s.builtin)] {
            add(&format!("code {name} on input"), c, t.input_bg, TEXT);
        }
        // On the sheet.
        add("cell text", t.cell_text, t.grid_bg, TEXT);
        for (i, c) in t.sheet_ref_colors.iter().enumerate() {
            add(&format!("sheet reference {i}"), *c, t.grid_bg, TEXT);
        }
        add("selection border on sheet", t.sel_border, t.grid_bg, GLYPH);
        add("sheet control text", t.sheet_control_text, t.sheet_control_bg, TEXT);
        add("sheet control border on sheet", t.sheet_control_border, t.grid_bg, GLYPH);
        add("slider thumb on track", t.sheet_thumb, t.sheet_track, GLYPH);
        add("sheet control text on hover", t.sheet_control_text, t.sheet_control_hover, TEXT);
        add("sheet control text when pressed", t.sheet_control_text, t.sheet_control_pressed, TEXT);
        add("slider fill on sheet", t.sheet_fill, t.grid_bg, GLYPH);
        add("slider fill on track", t.sheet_fill, t.sheet_track, GLYPH);
        add("control focus ring on sheet", t.sheet_focus, t.grid_bg, GLYPH);
        add("recording dot on ribbon", t.recording, t.ribbon, GLYPH);
        add("recording dot on status bar", t.recording, t.status_bar, GLYPH);
        for (name, c) in [
            ("macro input", t.sheet_macro_input),
            ("macro footprint", t.sheet_macro_footprint),
            ("macro conflict", t.sheet_macro_conflict),
            ("macro target", t.sheet_macro_target),
        ] {
            add(&format!("{name} outline on sheet"), c, t.grid_bg, GLYPH);
        }
        add("trace arrow on sheet", t.sheet_trace_arrow, t.grid_bg, GLYPH);
        add("trace error arrow on sheet", t.sheet_trace_error, t.grid_bg, GLYPH);
        // The macro step editor: breakpoint dots in the gutter, error markers, the current step.
        add("breakpoint on window", t.breakpoint, t.window, GLYPH);
        add("breakpoint on current step", t.breakpoint, t.accent_soft, GLYPH);
        add("text on current step", t.text, t.accent_soft, TEXT);
        add("current step arrow", t.accent_dark, t.accent_soft, GLYPH);
        add("error marker on window", t.danger, t.window, GLYPH);
        add("PY badge text", t.on_badge, t.badge_py, TEXT);
        add("PY badge on window", t.badge_py, t.window, GLYPH);
        add("PY badge on input", t.badge_py, t.input_bg, GLYPH);
        add("PY tag text", t.on_sheet_py_tag, t.sheet_py_tag, TEXT);
        add("PY tag on sheet", t.sheet_py_tag, t.grid_bg, GLYPH);
        v
    }

    #[test]
    fn token_pairs_meet_wcag_contrast_in_both_themes() {
        let mut failures = Vec::new();
        for t in [Tokens::light(), Tokens::dark()] {
            for (name, fg, bg, min) in pairs(&t) {
                let ratio = contrast(fg, bg);
                if ratio < min {
                    failures.push(format!("{} theme: {name} is {ratio:.2}:1, needs {min}:1", if t.dark { "dark" } else { "light" }));
                }
            }
        }
        assert!(failures.is_empty(), "contrast failures:\n{}", failures.join("\n"));
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.001);
        // #767676 on white is the classic 4.54:1 grey.
        assert!((contrast(rgb(0x767676), Color32::WHITE) - 4.54).abs() < 0.01);
    }

    #[test]
    fn sheet_tokens_follow_the_sheet_not_the_theme() {
        let (l, d) = (Tokens::light(), Tokens::dark());
        assert_eq!(l.grid_bg, d.grid_bg);
        assert_eq!(l.sheet_ref_colors, d.sheet_ref_colors);
        assert_eq!(
            (l.sheet_control_bg, l.sheet_control_border, l.sheet_control_text, l.sheet_track),
            (d.sheet_control_bg, d.sheet_control_border, d.sheet_control_text, d.sheet_track)
        );
        assert_eq!(
            (l.sheet_thumb, l.sheet_control_hover, l.sheet_control_pressed, l.sheet_fill, l.sheet_focus),
            (d.sheet_thumb, d.sheet_control_hover, d.sheet_control_pressed, d.sheet_fill, d.sheet_focus)
        );
        assert_eq!(
            (l.sheet_macro_input, l.sheet_macro_footprint, l.sheet_macro_conflict, l.sheet_macro_target),
            (d.sheet_macro_input, d.sheet_macro_footprint, d.sheet_macro_conflict, d.sheet_macro_target)
        );
        assert_eq!((l.sheet_trace_arrow, l.sheet_trace_error), (d.sheet_trace_arrow, d.sheet_trace_error));
        for c in Cat::ALL {
            assert_ne!(l.cat(c), d.cat(c), "{c:?} has a dark-mode value of its own");
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
