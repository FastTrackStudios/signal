//! [`Theme`] → `libSwell.colortheme`, REAPER's *other* theme file.
//!
//! On Linux a large share of what you look at is not painted by the
//! `.ReaperTheme` palette at all: the menu bar, every dialog, buttons,
//! combo boxes, list/tree views, trackbars and scrollbars are drawn by
//! SWELL, REAPER's Win32 compatibility layer, from a separate file called
//! `libSwell.colortheme` in the **resource** directory.
//!
//! It ships light — `_3dface #B3B3B3` — so a dark `.ReaperTheme` still
//! leaves a grey bar across the top and grey dialogs everywhere, and no
//! amount of palette work fixes it.
//!
//! # Only the keys SWELL actually reads
//!
//! SWELL **silently ignores** names it does not know, so an invented key
//! looks exactly like a key that had no effect. The first version of this
//! module guessed 24 of them — `menu_bg`, `menubar_bg`, `menu_text` and
//! friends — all plausible, all ignored. [`SUPPORTED`] is the real
//! vocabulary, taken from the `libSwell.colortheme` REAPER ships, and a
//! test asserts every emitted key is in it.
//!
//! The absent names are the interesting part: there is no menu background
//! or menu text key. SWELL paints menus and the menu bar from `_3dface`
//! and picks text by contrast against it — so **the menu bar is themed by
//! getting the 3D bevel right**, not by naming it.

use std::fmt::Write as _;

use crate::palette::Theme;

/// Every setting SWELL understands, from REAPER's shipped file.
///
/// Anything not in here is ignored at load, which is why it is worth
/// keeping exact rather than approximately right.
pub const SUPPORTED: [&str; 66] = [
    "_3ddkshadow",
    "_3dface",
    "_3dhilight",
    "_3dshadow",
    "button_hilight",
    "button_shadow",
    "button_text",
    "button_text_disabled",
    "checkbox_bg",
    "checkbox_fg",
    "checkbox_inter",
    "checkbox_text",
    "checkbox_text_disabled",
    "combo_arrow",
    "combo_bg",
    "combo_bg2",
    "combo_height",
    "combo_shadow",
    "default_font_face",
    "default_font_size",
    "edit_bg",
    "edit_bg_disabled",
    "edit_bg_sel",
    "edit_cursor",
    "edit_text",
    "edit_text_disabled",
    "edit_text_sel",
    "focus_hilight",
    "focusrect",
    "group_text",
    "info_bk",
    "info_text",
    "listview_bg",
    "listview_bg_sel",
    "listview_bg_sel_inactive",
    "listview_hdr_arrow",
    "listview_text",
    "listview_text_sel",
    "listview_text_sel_inactive",
    "menu_scroll",
    "menu_submenu_arrow",
    "menu_text_disabled",
    "menubar_font_size",
    "menubar_height",
    "menubar_margin_width",
    "menubar_spacing_width",
    "menubar_text_disabled",
    "progress",
    "scrollbar",
    "scrollbar_bg",
    "scrollbar_fg",
    "scrollbar_min_thumb_height",
    "scrollbar_width",
    "tab_hilight",
    "tab_shadow",
    "tab_text",
    "trackbar_knob",
    "trackbar_mark",
    "trackbar_track",
    "treeview_arrow",
    "treeview_bg",
    "treeview_bg_sel",
    "treeview_bg_sel_inactive",
    "treeview_text",
    "treeview_text_sel",
    "treeview_text_sel_inactive",
];

/// One SWELL setting: a name and an already-formatted value.
#[derive(Clone, Debug, PartialEq)]
pub struct SwellSetting {
    pub key: &'static str,
    pub value: String,
}

impl Theme {
    /// The SWELL settings this theme determines.
    pub fn swell_settings(&self) -> Vec<SwellSetting> {
        let c = &self.chrome;

        // SWELL's chrome is a Win95 bevel: a face colour with a highlight
        // above and two shadows below. Deriving all four from one surface
        // keeps it coherent — and since menus have no keys of their own,
        // `_3dface` IS the menu bar.
        let face = c.surface_raised;
        let hilight = face.shade(0.10);
        let shadow = face.shade(-0.18);
        let dkshadow = face.shade(-0.35);
        let field = c.surface_sunken;
        let sel_bg = c.accent.shade(-0.45);

        let col = |key: &'static str, color: crate::Color| SwellSetting {
            key,
            value: color.to_hex(),
        };
        let num = |key: &'static str, n: i32| SwellSetting {
            key,
            value: n.to_string(),
        };

        vec![
            // ── the bevel — also the menu bar and every menu ─────────────
            col("_3dface", face),
            col("_3dshadow", shadow),
            col("_3dhilight", hilight),
            col("_3ddkshadow", dkshadow),
            col("info_bk", c.surface_raised),
            col("info_text", c.text),
            // ── buttons ─────────────────────────────────────────────────
            // No `button_bg` exists; buttons take `_3dface`.
            col("button_text", c.text),
            col("button_text_disabled", c.text_faint),
            col("button_shadow", shadow),
            col("button_hilight", hilight),
            // ── checkboxes ──────────────────────────────────────────────
            col("checkbox_text", c.text),
            col("checkbox_text_disabled", c.text_faint),
            col("checkbox_fg", c.text),
            col("checkbox_inter", c.border),
            col("checkbox_bg", field),
            // ── text entry ──────────────────────────────────────────────
            col("edit_bg", field),
            col("edit_bg_disabled", c.surface),
            col("edit_bg_sel", sel_bg),
            col("edit_text", c.text),
            col("edit_text_disabled", c.text_faint),
            col("edit_text_sel", c.selected),
            col("edit_cursor", c.accent),
            // ── scrollbars ──────────────────────────────────────────────
            col("scrollbar", c.border.mix(c.text_dim, 0.5)),
            col("scrollbar_fg", c.text_dim),
            col("scrollbar_bg", c.surface),
            // ── combo boxes ─────────────────────────────────────────────
            col("combo_bg", field),
            col("combo_bg2", face),
            col("combo_arrow", c.text_dim),
            col("combo_shadow", shadow),
            // ── menus ───────────────────────────────────────────────────
            // Only the disabled/decoration bits are nameable; enabled menu
            // text and background follow `_3dface`.
            col("menu_text_disabled", c.text_faint),
            col("menubar_text_disabled", c.text_faint),
            col("menu_scroll", c.text_dim),
            col("menu_submenu_arrow", c.text_dim),
            // ── list and tree views ─────────────────────────────────────
            col("listview_bg", c.surface_raised),
            col("listview_bg_sel", sel_bg),
            col("listview_bg_sel_inactive", c.surface_raised.shade(0.06)),
            col("listview_text", c.text),
            col("listview_text_sel", c.selected),
            col("listview_text_sel_inactive", c.text_dim),
            col("listview_hdr_arrow", c.text_dim),
            col("treeview_bg", c.surface_raised),
            col("treeview_bg_sel", sel_bg),
            col("treeview_bg_sel_inactive", c.surface_raised.shade(0.06)),
            col("treeview_text", c.text),
            col("treeview_text_sel", c.selected),
            col("treeview_text_sel_inactive", c.text_dim),
            col("treeview_arrow", c.text_dim),
            // ── tabs ────────────────────────────────────────────────────
            col("tab_shadow", shadow),
            col("tab_hilight", hilight),
            col("tab_text", c.text),
            // ── trackbars (sliders in dialogs) ──────────────────────────
            col("trackbar_track", c.surface_sunken),
            col("trackbar_knob", c.text_dim),
            col("trackbar_mark", c.border),
            // ── misc ────────────────────────────────────────────────────
            col("group_text", c.text_dim),
            col("focusrect", c.accent),
            col("focus_hilight", c.accent),
            col("progress", c.accent),
            // ── metrics ─────────────────────────────────────────────────
            // Not colours, but SWELL reads them here and the shipped
            // defaults are cramped.
            num("menubar_height", 20),
            num("menubar_font_size", 13),
            num("default_font_size", 13),
            num("scrollbar_width", 14),
            num("combo_height", 22),
        ]
    }

    /// Render a complete `libSwell.colortheme`.
    pub fn swell_colortheme(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "; libSwell.colortheme — generated from the FastTrackStudio theme \"{}\".\n\
             ;\n\
             ; Themes what the .ReaperTheme palette cannot: REAPER's menu bar,\n\
             ; dialogs, buttons, combo boxes, lists and scrollbars, all of which\n\
             ; SWELL draws on Linux. Put it in the REAPER resource directory\n\
             ; (beside reaper.ini) and RESTART REAPER — unlike the palette, this\n\
             ; is not reloadable.\n\
             ;\n\
             ; Generated — edit the palette in daw-theme, not this file.",
            self.name
        );
        let _ = writeln!(out, "default_font_face Liberation Sans");
        for setting in self.swell_settings() {
            let _ = writeln!(out, "{} {}", setting.key, setting.value);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(key: &str) -> String {
        Theme::default()
            .swell_settings()
            .into_iter()
            .find(|s| s.key == key)
            .unwrap_or_else(|| panic!("no SWELL setting for {key}"))
            .value
    }

    #[test]
    fn every_emitted_key_is_one_swell_understands() {
        // The regression that motivated this list: 24 invented keys —
        // menu_bg, menubar_bg, menu_text and friends — every one plausible,
        // every one silently ignored, so the file looked complete and the
        // menu bar stayed grey.
        for s in Theme::default().swell_settings() {
            assert!(
                SUPPORTED.contains(&s.key),
                "`{}` is not a SWELL setting — it will be ignored silently",
                s.key
            );
        }
    }

    #[test]
    fn the_bevel_is_ordered() {
        // Win95 bevel: highlight above the face, shadow below, dark shadow
        // below that. Out of order and every dialog looks inverted. This
        // matters more than it sounds: menus have no keys of their own, so
        // `_3dface` is what themes the menu bar.
        let l = |k: &str| crate::Color::hex(&setting(k)).unwrap().luminance();
        assert!(l("_3dhilight") > l("_3dface"));
        assert!(l("_3dface") > l("_3dshadow"));
        assert!(l("_3dshadow") > l("_3ddkshadow"));
    }

    #[test]
    fn the_grey_chrome_is_actually_covered() {
        // `_3dface` ships at #B3B3B3 and is why an otherwise-dark REAPER
        // keeps a light bar across the top and grey dialogs.
        for key in [
            "_3dface",
            "listview_bg",
            "treeview_bg",
            "edit_bg",
            "combo_bg",
        ] {
            let hex = setting(key);
            let l = crate::Color::hex(&hex).expect("valid hex").luminance();
            assert!(l < 0.25, "{key} = {hex} is too light for a dark theme");
        }
    }

    #[test]
    fn text_reads_against_its_own_background() {
        for (fg, bg) in [
            ("listview_text", "listview_bg"),
            ("treeview_text", "treeview_bg"),
            ("edit_text", "edit_bg"),
            ("button_text", "_3dface"),
            ("info_text", "info_bk"),
        ] {
            let f = crate::Color::hex(&setting(fg)).unwrap().luminance();
            let b = crate::Color::hex(&setting(bg)).unwrap().luminance();
            assert!(f - b > 0.2, "{fg} on {bg} has too little contrast");
        }
    }

    #[test]
    fn renders_in_swells_format_not_ini() {
        // Space-separated `name value`, `;` comments — an `=` makes SWELL
        // skip the line silently.
        let text = Theme::default().swell_colortheme();
        for line in text.lines() {
            if line.trim().is_empty() || line.starts_with(';') {
                continue;
            }
            assert!(!line.contains('='), "ini syntax in a SWELL file: {line}");
            assert!(
                line.split_whitespace().count() >= 2,
                "malformed setting line: {line}"
            );
        }
        assert!(text.contains("_3dface #"));
    }

    #[test]
    fn numeric_settings_are_not_written_as_colours() {
        // menubar_height is a pixel count; "#000014" would look accepted
        // and be wrong.
        for key in ["menubar_height", "scrollbar_width", "combo_height"] {
            let v = setting(key);
            assert!(!v.starts_with('#'), "{key} emitted as a colour: {v}");
            assert!(v.parse::<i32>().is_ok(), "{key} is not numeric: {v}");
        }
    }
}
