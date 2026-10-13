//! [`Theme`] → REAPER `[color theme]` keys.
//!
//! The inverse of `daw_ui::theming::reaper_import`, and deliberately built
//! from that module's documented choices so a theme survives a round trip:
//! `col_main_bg2` is the app surface, `col_tr1_bg` the raised strip,
//! `col_main_3dsh` the border, `col_cursor` the accent, and
//! `col_vutop`/`col_vumid`/`col_vubot`/`col_vuclip` the meter ramp.
//!
//! This is what makes the FTS theme canonical rather than derived: REAPER
//! becomes an output, so "theme REAPER to match the editor" is one call.
//!
//! **The palette is small and this file is large, on purpose.** REAPER has
//! ~400 colour keys; authoring 400 colours is not a design, it is data
//! entry, and it guarantees the parts nobody thought about drift grey. So
//! roughly twenty authored colours are *derived* here into the full set —
//! every list, docker, dialog and wiring view is a shade or mix of the same
//! surfaces, text and accent. Change `ACCENT` and all of it moves together.
//!
//! Two things are deliberately never emitted:
//!
//! - **Anything that isn't a colour.** `*_drawmode` / `*dm` words are blend
//!   modes and `*_mode` are flags; writing a colour over one silently
//!   changes how a layer composites.
//! - **Keys whose value REAPER computes.** Left alone rather than guessed.
//!
//! See `.claude/skills/reaper-theme-colors/SKILL.md` for what each family
//! actually paints.

use crate::color::Color;
use crate::palette::Theme;

/// One REAPER palette assignment.
#[derive(Clone, PartialEq, Debug)]
pub struct Assignment {
    /// `[color theme]` key.
    ///
    /// Borrowed for the derived set, which is entirely `&'static str`, and
    /// owned for [`Theme::overrides`], whose keys come from a file.
    pub key: std::borrow::Cow<'static, str>,
    pub color: Color,
}

const fn a(key: &'static str, color: Color) -> Assignment {
    Assignment {
        key: std::borrow::Cow::Borrowed(key),
        color,
    }
}

impl Theme {
    /// Every REAPER palette key this theme determines.
    ///
    /// Apply with `fts_themer::ThemeIni::set_color`, which preserves
    /// REAPER's flag byte and leaves untouched keys alone.
    pub fn reaper_palette(&self) -> Vec<Assignment> {
        let c = &self.chrome;
        let s = &self.signal;
        let e = &self.editor;

        // Derived steps used across several families. Naming them here keeps
        // the sections below readable and the relationships explicit.
        let strip_a = c.surface_raised;
        let strip_b = c.surface_raised.shade(0.04);
        let control = c.surface_raised.shade(0.06);
        let bar = c.surface.mix(c.surface_raised, 0.5);
        let list_alt = c.surface_raised.shade(0.03);

        let mut out = vec![
            // ── main window ─────────────────────────────────────────────
            // col_main_bg is the window behind docked panes; col_main_bg2
            // is the app surface. Leaving bg grey is what makes an
            // otherwise-dark theme show grey gutters around the dockers.
            a("col_main_bg", c.surface),
            a("col_main_bg2", c.surface),
            a("col_main_text", c.text),
            a("col_main_text2", c.text_dim),
            a("col_main_textshadow", c.surface.shade(-0.3)),
            a("col_main_3dsh", c.border),
            a("col_main_3dhl", c.border.shade(0.15)),
            a("col_main_editbk", c.surface_sunken),
            a("col_main_resize", c.surface.mix(c.border, 0.5)),
            a("col_main_resize2", c.border),
            a("col_buttonbg", control),
            a("col_toolbar_frame", c.border),
            a("col_toolbar_text", c.text_dim),
            a("col_toolbar_text_on", c.accent),
            a("col_offlinetext", s.mute),
            // ── dockers ─────────────────────────────────────────────────
            // The tab strip around every docked pane — a large grey area in
            // any theme that forgets it.
            a("docker_bg", c.surface),
            a("docker_shadow", c.surface.shade(-0.3)),
            a("docker_selface", c.surface_raised),
            a("docker_unselface", bar),
            a("docker_text", c.text_dim),
            a("docker_text_sel", c.text),
            // ── arrange ─────────────────────────────────────────────────
            a("col_arrangebg", c.surface_sunken),
            a("col_tracklistbg", c.surface),
            a("col_mixerbg", c.surface),
            a("col_cursor", s.playhead),
            a("col_cursor2", c.accent),
            a("col_gridlines", e.grid_beat),
            a("col_gridlines2", e.grid_sub),
            a("col_gridlines3", e.octave_line),
            a("guideline_color", c.accent),
            a("col_fadearm", c.accent),
            a("col_fadearm2", s.meter_safe),
            a("col_fadearm3", s.meter_warn),
            // ── timeline / ruler ────────────────────────────────────────
            a("col_tl_bg", bar),
            a("col_tl_bgsel", c.accent),
            a("col_tl_bgsel2", c.accent.shade(-0.2)),
            a("col_tl_fg", c.text),
            a("col_tl_fg2", c.text_dim),
            a("col_tsigmark", c.accent),
            a("timesig_sel", c.accent),
            a("playrate_edited", s.meter_warn),
            // ── markers and regions ─────────────────────────────────────
            a("marker", c.accent),
            a("marker_edge", c.accent.shade(0.2)),
            a("marker_edge_sel", c.selected),
            a("marker_lane_bg", bar),
            a("marker_lane_text", c.text),
            a("region", s.meter_warn),
            a("region_lane_bg", bar),
            a("region_lane_text", c.text),
            // ── selection rectangles ────────────────────────────────────
            a("marquee_fill", c.accent),
            a("marquee_outline", c.accent.shade(0.3)),
            a("marqueezoom_fill", s.meter_warn),
            a("marqueezoom_outline", s.meter_warn.shade(0.3)),
            a("areasel_fill", c.accent),
            a("areasel_outline", c.accent.shade(0.3)),
            a("linkedlane_fill", e.razor),
            a("linkedlane_outline", e.razor.shade(0.2)),
            a("linkedlane_unsynced", s.mute),
            // ── track panels ────────────────────────────────────────────
            a("col_tr1_bg", strip_a),
            a("col_tr2_bg", strip_b),
            a("col_tr1_divline", c.border),
            a("col_tr2_divline", c.border),
            a("col_tr1_peaks", s.peaks),
            a("col_tr2_peaks", s.peaks),
            a("col_tr1_ps2", s.peaks.shade(-0.2)),
            a("col_tr2_ps2", s.peaks.shade(-0.2)),
            a("col_seltrack", c.accent),
            a("col_seltrack2", c.accent),
            a("col_tr1_itembgsel", strip_a.mix(c.accent, 0.25)),
            a("col_tr2_itembgsel", strip_b.mix(c.accent, 0.25)),
            // The thin pill inside the FX button strip is this, not the FX
            // colours and not artwork — it is the FX list's scrollbar
            // thumb, and it is the only lit-looking thing on an empty
            // strip. Deriving it from the border was blue-cast, so it read as a status
            // indicator sitting on a neutral grey button.
            a("tcp_list_scrollbar", c.hardware_mark.shade(-0.45)),
            a("tcp_list_scrollbar_mouseover", c.accent),
            // ── mixer panels ────────────────────────────────────────────
            // The strip chrome REAPER paints itself — the rest of the mixer
            // is image art and cannot be reached from here.
            // The FX list and its bypass toggle sit *inside* the FX button
            // strip, and REAPER draws them from the palette rather than
            // from artwork — so they are the one part of that control a
            // component cannot reach. They still have to match the
            // component beside them, which means the neutral hardware
            // family, not the blue-cast chrome text ramp: `c.text` here
            // put a faintly blue pill against a grey button.
            a("mcp_fx_normal", c.hardware_mark.shade(0.35)),
            a("mcp_fx_bypassed", s.mute),
            a("mcp_fx_offlined", c.hardware_mark.shade(-0.33)),
            a("mcp_fxparm_normal", c.hardware_mark.shade(0.35)),
            a("mcp_fxparm_bypassed", s.mute),
            a("mcp_fxparm_offlined", c.hardware_mark.shade(-0.33)),
            a("mcp_sends_normal", s.meter_warn),
            a("mcp_sends_muted", s.mute),
            a("mcp_sends_levels", c.text_dim),
            a("mcp_send_midihw", c.accent),
            a("mcp_list_scrollbar", c.hardware_mark.shade(-0.45)),
            a("mcp_list_scrollbar_mouseover", c.accent),
            // ── routing / IO dialogs ────────────────────────────────────
            a("io_text", c.text),
            a("io_3dhl", c.border.shade(0.15)),
            a("io_3dsh", c.border),
            a("col_routingact", c.accent),
            // ── media items ─────────────────────────────────────────────
            a("col_mi_bg", strip_a),
            a("col_mi_bg2", strip_b),
            a("col_mi_label", c.text),
            a("col_mi_label_sel", c.selected),
            a("col_mi_label_float", c.text),
            a("col_mi_label_float_sel", c.selected),
            a("col_mi_fades", c.accent),
            a("col_mi_fade2", c.accent.shade(0.2)),
            a("col_mi_left", c.border),
            a("col_mi_leftsel", c.accent),
            a("col_mi_note", e.row_white.shade(0.4)),
            a("col_mi_note_sel", c.selected),
            a("col_peaksedge", s.peaks.shade(0.2)),
            a("col_peaksedge2", s.peaks.shade(0.2)),
            a("col_peaksedgesel", c.selected),
            a("col_peaksedgesel2", c.selected),
            a("col_peaksfade1", s.peaks.shade(0.3)),
            a("col_peaksfade2", s.peaks.shade(-0.3)),
            // Stretch markers: a handle family, so they follow the handle
            // and accent rather than inventing a colour.
            a("col_stretchmarker", c.accent),
            a("col_stretchmarker_h0", c.accent.shade(0.2)),
            a("col_stretchmarker_h1", c.accent.shade(0.1)),
            a("col_stretchmarker_h2", c.accent.shade(-0.1)),
            a("col_stretchmarker_b", c.border),
            a("col_stretchmarkerm", c.accent.shade(-0.25)),
            a("col_stretchmarker_text", c.text),
            a("col_stretchmarker_tm", c.text_dim),
            // ── meters ──────────────────────────────────────────────────
            a("col_vubot", s.meter_safe),
            a("col_vumid", s.meter_warn),
            a("col_vutop", s.meter_danger),
            a("col_vuclip", s.meter_danger.shade(0.25)),
            a("col_vuintcol", c.surface_sunken),
            a("mcp_vu_bg", c.surface_sunken),
            a("tcp_vu_bg", c.surface_sunken),
            // ── transport ───────────────────────────────────────────────
            a("col_trans_bg", bar),
            a("col_trans_fg", c.text),
            // ── envelopes ───────────────────────────────────────────────
            // Per-type envelopes get meanings, not arbitrary hues: volume
            // is the peak colour, pan the accent, mute the mute colour.
            a("col_envlane1_divline", c.border),
            a("col_envlane2_divline", c.border),
            a("env_trim_vol", s.peaks.shade(0.2)),
            a("env_track_mute", s.mute),
            a("env_sends_mute", s.mute.shade(-0.15)),
            a("env_item_vol", s.peaks),
            a("env_item_pan", c.accent),
            a("env_item_mute", s.mute),
            a("env_item_pitch", e.microtonal),
            a("env_item_sampleedit", e.razor),
            a("envcp_bg", strip_a),
            a("envcp_fg", c.text),
            a("envcp_fg_sel", c.selected),
            // ── generic lists (media explorer, FX browser, managers) ────
            // Big surfaces that stay grey in most themes because nobody
            // remembers they exist.
            a("genlist_bg", c.surface_raised),
            a("genlist_fg", c.text),
            a("genlist_grid", c.border),
            a("genlist_selbg", c.accent.shade(-0.45)),
            a("genlist_selfg", c.selected),
            a("genlist_seliabg", list_alt),
            a("genlist_seliafg", c.text_dim),
            a("genlist_hilite", c.accent),
            a("genlist_hilite_sel", c.selected),
            // ── MIDI editor ─────────────────────────────────────────────
            // The one place REAPER and the expression editor draw the same
            // thing, so they must agree exactly.
            a("midi_rulerbg", bar),
            a("midi_rulerfg", c.text),
            a("midi_grid1", e.grid_beat),
            a("midi_grid2", e.grid_sub),
            a("midi_grid3", e.octave_line),
            a("midi_griddi", e.grid_sub),
            a("midi_trackbg1", e.row_white),
            a("midi_trackbg2", e.row_black),
            a("midi_trackbg_outer1", e.row_white.shade(-0.25)),
            a("midi_trackbg_outer2", e.row_black.shade(-0.25)),
            a("midi_selpitch1", e.row_white.mix(c.accent, 0.2)),
            a("midi_selpitch2", e.row_black.mix(c.accent, 0.2)),
            a("midi_selbg", c.accent.shade(-0.4)),
            a("midi_gridhc", e.octave_line),
            a("midi_pkey1", e.key_white),
            a("midi_pkey2", e.key_black),
            a("midi_pkey3", c.accent),
            a("midi_noteon_flash", c.selected),
            a("midi_leftbg", c.surface),
            a("midi_notebg", e.row_white),
            a("midi_notefg", c.text),
            a("midi_editcurs", s.playhead),
            a("midi_ofsn", e.row_black.shade(-0.2)),
            a("midi_ofsnsel", c.accent.shade(-0.3)),
            a("midi_endpt", s.mute),
            a("midi_ccbut", control),
            a("midi_ccbut_text", c.text),
            a("midi_ccbut_arrow", c.text_dim),
            a("midifont_col_light", c.surface),
            a("midifont_col_dark", c.text),
            a("midifont_col_light_unsel", c.surface.shade(0.25)),
            a("midifont_col_dark_unsel", c.text_dim),
            // ── MIDI editor track list ──────────────────────────────────
            a("midieditorlist_bg", c.surface_raised),
            a("midieditorlist_bg2", list_alt),
            a("midieditorlist_fg", c.text),
            a("midieditorlist_fg2", c.text_dim),
            a("midieditorlist_grid", c.border),
            a("midieditorlist_selbg", c.accent.shade(-0.45)),
            a("midieditorlist_selbg2", c.accent.shade(-0.5)),
            a("midieditorlist_selfg", c.selected),
            a("midieditorlist_selfg2", c.text),
            a("midieditorlist_seliabg", list_alt),
            a("midieditorlist_seliafg", c.text_dim),
            // ── notation ────────────────────────────────────────────────
            a("score_bg", e.key_white),
            a("score_fg", c.surface),
            a("score_sel", c.accent),
            a("score_timesel", c.accent.shade(-0.3)),
            a("score_loop", s.meter_warn),
            // ── wiring view ─────────────────────────────────────────────
            a("wiring_grid", e.grid_sub),
            a("wiring_grid2", e.grid_beat),
            a("wiring_border", c.border),
            a("wiring_tbg", strip_a),
            a("wiring_ticon", c.text_dim),
            a("wiring_media", s.peaks),
            a("wiring_recv", s.meter_safe),
            a("wiring_send", c.accent),
            a("wiring_sendwire", c.accent.shade(-0.2)),
            a("wiring_fader", c.text_dim),
            a("wiring_parent", c.text_faint),
            a("wiring_parentwire_border", c.border),
            a("wiring_parentwire_folder", c.accent.shade(-0.2)),
            a("wiring_parentwire_master", c.text_dim),
            a("wiring_hwout", e.microtonal),
            a("wiring_hwoutwire", e.microtonal.shade(-0.2)),
            a("wiring_recinput", s.rec),
            a("wiring_recinputwire", s.rec.shade(-0.2)),
            a("wiring_activity", s.meter_safe),
            a("wiring_pin_normal", c.text_dim),
            a("wiring_pin_connected", c.accent),
            a("wiring_pin_disconnected", c.text_faint),
            a("wiring_horz_col", c.border),
            a("wiring_recbg", c.surface_sunken),
            a("wiring_recitem", s.rec.shade(-0.35)),
            // ── media explorer ──────────────────────────────────────────
            a("col_explorer_sel", c.accent.shade(-0.45)),
            a("col_explorer_seledge", c.accent),
        ];

        // Envelope lane colours come from the pitch-class wheel: the same
        // problem (N series that must stay tellable apart), and reusing it
        // keeps automation lanes in the theme's hue family.
        for (i, color) in e.pitch_classes.iter().take(16).enumerate() {
            out.push(a(ENV_KEYS[i], *color));
        }

        // Track groups, likewise — 32 groups that only need to be mutually
        // distinguishable, so the wheel is cycled with a lightness step so
        // the second pass isn't identical to the first.
        if !e.pitch_classes.is_empty() {
            for (i, key) in GROUP_KEYS.iter().enumerate() {
                let hue = e.pitch_classes[i % e.pitch_classes.len()];
                let pass = i / e.pitch_classes.len();
                out.push(a(key, hue.shade(-0.12 * pass as f32)));
            }
        }

        // Last, so an explicit key always wins: replace a derived
        // assignment in place, or append one the derivation never emits.
        // Replacing in place rather than appending keeps the output free
        // of duplicate keys, which `emits_no_duplicate_keys` pins.
        for (key, color) in &self.overrides {
            match out.iter_mut().find(|x| x.key == key.as_str()) {
                Some(existing) => existing.color = *color,
                None => out.push(Assignment {
                    key: std::borrow::Cow::Owned(key.clone()),
                    color: *color,
                }),
            }
        }

        out
    }
}

/// `col_env1..16`.
const ENV_KEYS: [&str; 16] = [
    "col_env1",
    "col_env2",
    "col_env3",
    "col_env4",
    "col_env5",
    "col_env6",
    "col_env7",
    "col_env8",
    "col_env9",
    "col_env10",
    "col_env11",
    "col_env12",
    "col_env13",
    "col_env14",
    "col_env15",
    "col_env16",
];

/// `group_0..31` — REAPER's track-group tint slots.
const GROUP_KEYS: [&str; 32] = [
    "group_0", "group_1", "group_2", "group_3", "group_4", "group_5", "group_6", "group_7",
    "group_8", "group_9", "group_10", "group_11", "group_12", "group_13", "group_14", "group_15",
    "group_16", "group_17", "group_18", "group_19", "group_20", "group_21", "group_22", "group_23",
    "group_24", "group_25", "group_26", "group_27", "group_28", "group_29", "group_30", "group_31",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Vec<Assignment> {
        Theme::default().reaper_palette()
    }

    fn get(key: &str) -> Color {
        palette()
            .into_iter()
            .find(|x| x.key == key)
            .unwrap_or_else(|| panic!("no assignment for {key}"))
            .color
    }

    #[test]
    fn maps_the_keys_reaper_import_reads() {
        // The import side documents these as its anchors; if the export
        // skips one, a round trip loses that part of the theme.
        for key in [
            "col_main_bg2",
            "col_tr1_bg",
            "col_main_3dsh",
            "col_main_text",
            "col_main_text2",
            "col_cursor",
            "col_vutop",
            "col_vumid",
            "col_vubot",
            "col_vuclip",
            "col_arrangebg",
        ] {
            let _ = get(key);
        }
    }

    #[test]
    fn anchors_carry_the_authored_colour_unchanged() {
        let t = Theme::default();
        assert_eq!(get("col_main_bg2"), t.chrome.surface);
        assert_eq!(get("col_main_text"), t.chrome.text);
        assert_eq!(get("col_main_3dsh"), t.chrome.border);
        assert_eq!(get("col_vubot"), t.signal.meter_safe);
        assert_eq!(get("col_cursor"), t.signal.playhead);
    }

    #[test]
    fn covers_the_chrome_that_otherwise_stays_grey() {
        // These are the families a theme forgets, and forgetting them is
        // exactly what leaves grey gutters around an otherwise dark theme.
        for key in [
            "col_main_bg",
            "docker_bg",
            "docker_selface",
            "docker_unselface",
            "docker_text",
            "genlist_bg",
            "genlist_fg",
            "midieditorlist_bg",
            "io_text",
            "col_toolbar_text",
            "mcp_fx_normal",
            "wiring_grid",
        ] {
            let _ = get(key);
        }
    }

    #[test]
    fn never_emits_a_drawmode_or_other_non_colour() {
        // Writing a colour over a blend word silently changes compositing.
        for x in palette() {
            let k = x.key;
            assert!(
                !k.contains("drawmode") && !k.ends_with("_mode") && !k.ends_with("dm"),
                "export would clobber the non-colour key {k}"
            );
        }
    }

    /// An override replaces the derived value for a key.
    #[test]
    fn an_override_wins_over_the_derivation() {
        let derived = palette()
            .into_iter()
            .find(|x| x.key == "col_arrangebg")
            .expect("col_arrangebg is derived");

        let mut theme = Theme::default();
        let want = Color::rgb(0x42, 0x42, 0x42);
        theme.overrides.insert("col_arrangebg".into(), want);

        let got = theme
            .reaper_palette()
            .into_iter()
            .find(|x| x.key == "col_arrangebg")
            .expect("still emitted");
        assert_eq!(got.color, want);
        assert_ne!(got.color, derived.color, "the test colour must differ");
    }

    /// And reaches keys the derivation never emits — the point of it.
    #[test]
    fn an_override_can_add_a_key_the_derivation_never_touches() {
        let key = "col_vugrid";
        assert!(
            !palette().iter().any(|x| x.key == key),
            "{key} is derived after all — pick another for this test",
        );

        let mut theme = Theme::default();
        theme.overrides.insert(key.into(), Color::rgb(1, 2, 3));
        let got = theme.reaper_palette();
        assert_eq!(
            got.iter().filter(|x| x.key == key).count(),
            1,
            "expected exactly one {key}",
        );
    }

    /// Overriding must not leave two lines for one key: REAPER reads the
    /// last, so a duplicate is a value that silently depends on order.
    #[test]
    fn overriding_does_not_duplicate_a_key() {
        let mut theme = Theme::default();
        theme
            .overrides
            .insert("col_arrangebg".into(), Color::rgb(9, 9, 9));
        let got = theme.reaper_palette();
        let n = got.iter().filter(|x| x.key == "col_arrangebg").count();
        assert_eq!(n, 1, "col_arrangebg emitted {n} times");
    }

    #[test]
    fn emits_no_duplicate_keys() {
        // A duplicate means one assignment silently wins — usually not the
        // one the author expected.
        let mut keys: Vec<String> = palette().iter().map(|x| x.key.to_string()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate keys in the export");
    }

    #[test]
    fn alternating_rows_and_lists_differ() {
        // If banding collapses, the track list and every list control read
        // as one flat slab.
        assert_ne!(get("col_tr1_bg"), get("col_tr2_bg"));
        assert_ne!(get("col_mi_bg"), get("col_mi_bg2"));
        assert_ne!(get("genlist_bg"), get("genlist_seliabg"));
    }

    #[test]
    fn midi_rows_match_the_editor_rows_exactly() {
        // The entire point of the exercise: REAPER's piano roll and the
        // expression editor draw the same rows in the same colours.
        let t = Theme::default();
        assert_eq!(get("midi_trackbg1"), t.editor.row_white);
        assert_eq!(get("midi_trackbg2"), t.editor.row_black);
        assert_eq!(get("midi_pkey1"), t.editor.key_white);
        assert_eq!(get("midi_editcurs"), t.signal.playhead);
    }

    #[test]
    fn every_surface_key_is_dark_in_a_dark_theme() {
        // The failure this whole file exists to prevent: one forgotten
        // background left at REAPER's default grey, showing as a bright
        // gutter in an otherwise dark theme.
        for key in [
            "col_main_bg",
            "col_main_bg2",
            "docker_bg",
            "docker_selface",
            "docker_unselface",
            "col_tracklistbg",
            "col_mixerbg",
            "col_arrangebg",
            "genlist_bg",
            "midieditorlist_bg",
            "col_trans_bg",
            "col_tl_bg",
        ] {
            let l = get(key).luminance();
            assert!(l < 0.25, "{key} is too light for a dark theme ({l:.3})");
        }
    }

    #[test]
    fn envelope_lanes_come_from_the_pitch_wheel() {
        let t = Theme::default();
        assert_eq!(get("col_env1"), t.editor.pitch_classes[0]);
        assert_eq!(get("col_env12"), t.editor.pitch_classes[11]);
        // Only 12 hues exist, so 13-16 must be absent rather than indexing
        // past the end.
        assert!(palette().iter().all(|x| x.key != "col_env13"));
    }

    #[test]
    fn track_groups_cycle_the_wheel_without_repeating_exactly() {
        // 32 slots from 12 hues: the second pass must not be pixel-identical
        // to the first, or groups 1 and 13 are indistinguishable.
        assert_ne!(get("group_0"), get("group_12"));
        assert_eq!(
            palette()
                .iter()
                .filter(|x| x.key.starts_with("group_"))
                .count(),
            32
        );
    }

    #[test]
    fn a_short_pitch_list_does_not_panic_the_export() {
        let mut t = Theme::default();
        t.editor.pitch_classes.truncate(3);
        let out = t.reaper_palette();
        assert!(out.iter().any(|x| x.key == "col_env3"));
        assert!(out.iter().all(|x| x.key != "col_env4"));
        // Groups still fill all 32 slots by cycling the short list.
        assert_eq!(
            out.iter().filter(|x| x.key.starts_with("group_")).count(),
            32
        );
    }

    #[test]
    fn an_empty_pitch_list_does_not_panic_the_export() {
        let mut t = Theme::default();
        t.editor.pitch_classes.clear();
        let out = t.reaper_palette();
        assert!(out.iter().all(|x| !x.key.starts_with("group_")));
    }
}
