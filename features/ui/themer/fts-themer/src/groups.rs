//! Semantic grouping of `[color theme]` keys.
//!
//! The palette is ~420 flat keys with names like `col_tr1_bg` and
//! `midi_inline_trackbg2`. That's fine for a parser and useless for a GUI —
//! nobody wants one 420-row list of swatches. This module sorts keys into the
//! areas of REAPER they actually paint, and flags the ones that are *not*
//! colors so an editor never shows a color picker for a blend mode.
//!
//! Classification is prefix-based with an explicit override table, so an
//! unknown key from a future REAPER version still lands somewhere sensible
//! instead of vanishing.

/// A UI area of REAPER that a set of palette keys paints.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Group {
    /// Window chrome, backgrounds, generic text.
    Main,
    /// Arrange view background, grid lines, cursors, markers.
    Arrange,
    /// Media items, take names, waveform peaks, fades.
    Items,
    /// Track control panel (the left-hand strip).
    Tcp,
    /// Mixer control panel.
    Mcp,
    /// Envelopes and automation lanes.
    Envelopes,
    /// Meters and VU scales.
    Meters,
    /// The MIDI editor, piano roll and inline MIDI.
    Midi,
    /// Transport bar and its readouts.
    Transport,
    /// Anything that isn't a color: blend words, flags, mode ints.
    Modes,
    /// Classified nowhere else.
    Other,
}

impl Group {
    /// Human label for a section header.
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "Main window",
            Self::Arrange => "Arrange view",
            Self::Items => "Media items",
            Self::Tcp => "Track panel (TCP)",
            Self::Mcp => "Mixer (MCP)",
            Self::Envelopes => "Envelopes",
            Self::Meters => "Meters",
            Self::Midi => "MIDI editor",
            Self::Transport => "Transport",
            Self::Modes => "Blend modes & flags",
            Self::Other => "Other",
        }
    }

    /// Display order for a GUI — most-edited first, junk drawer last.
    pub fn all() -> [Group; 11] {
        [
            Self::Main,
            Self::Arrange,
            Self::Items,
            Self::Tcp,
            Self::Mcp,
            Self::Meters,
            Self::Envelopes,
            Self::Midi,
            Self::Transport,
            Self::Other,
            Self::Modes,
        ]
    }
}

/// Keys whose prefix would misfile them. Checked before the prefix rules.
const OVERRIDES: &[(&str, Group)] = &[
    ("col_mixerbg", Group::Mcp),
    ("col_arrangebg", Group::Arrange),
    ("col_tl_bg", Group::Arrange),
    ("col_tl_bgsel", Group::Arrange),
    ("col_tl_fg", Group::Arrange),
    ("col_tl_fg2", Group::Arrange),
    ("col_cursor", Group::Arrange),
    ("col_cursor2", Group::Arrange),
    ("col_gridlines", Group::Arrange),
    ("col_gridlines2", Group::Arrange),
    ("col_gridlines3", Group::Arrange),
    ("col_seltrack", Group::Tcp),
    ("col_seltrack2", Group::Mcp),
    ("col_toolbar_frame", Group::Main),
    ("col_offlinetext", Group::Items),
    ("col_fadearm", Group::Items),
    ("col_fadearm2", Group::Items),
    ("col_fadearm3", Group::Items),
    ("playrate_edited", Group::Transport),
    ("timesig_sel", Group::Arrange),
    ("region", Group::Arrange),
    ("region_lane_bg", Group::Arrange),
    ("region_lane_text", Group::Arrange),
    ("marker", Group::Arrange),
    ("marker_lane_bg", Group::Arrange),
    ("marker_lane_text", Group::Arrange),
    ("areasel_fill", Group::Arrange),
    ("areasel_outline", Group::Arrange),
    ("autogroup", Group::Items),
    ("activetake_tag", Group::Items),
    ("selitem_tag", Group::Items),
    ("guideline_color", Group::Arrange),
    ("arrange_vgrid", Group::Arrange),
    ("group_0", Group::Items),
];

/// Prefix rules, first match wins. Order matters: `col_vu` must beat `col_`.
const PREFIXES: &[(&str, Group)] = &[
    ("midi_", Group::Midi),
    ("midioct", Group::Midi),
    ("col_mi_", Group::Items),
    ("col_vu", Group::Meters),
    ("col_peaks", Group::Items),
    ("col_env", Group::Envelopes),
    ("env_", Group::Envelopes),
    ("col_tcp", Group::Tcp),
    ("col_tr", Group::Tcp),
    ("col_mcp", Group::Mcp),
    ("mcp_", Group::Mcp),
    ("tcp_", Group::Tcp),
    ("col_trans", Group::Transport),
    ("trans_", Group::Transport),
    ("col_toolbar", Group::Main),
    ("col_main", Group::Main),
    ("col_buttonbg", Group::Main),
    ("col_tl_", Group::Arrange),
    ("col_explorer", Group::Main),
    ("col_routing", Group::Tcp),
    ("item_", Group::Items),
    ("take_", Group::Items),
    ("marker", Group::Arrange),
    ("region", Group::Arrange),
    ("meter", Group::Meters),
    ("vu", Group::Meters),
    ("envcp", Group::Envelopes),
    ("group_", Group::Items),
    ("col_", Group::Other),
];

/// Suffixes that mean "this int is not a color".
const NON_COLOR_SUFFIXES: &[&str] = &["_drawmode", "drawmode", "_mode", "_flags", "_blend"];

/// Whole keys that are ints but not colors.
const NON_COLOR_KEYS: &[&str] = &[
    "activetake_tag",
    "autogroup",
    "selitem_tag",
    "peaksedges",
    "col_nodarkmodemiscwnd",
];

/// Is this key a color, or a blend/flag word that must not get a color picker?
pub fn is_color(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    if NON_COLOR_KEYS.contains(&k.as_str()) {
        return false;
    }
    !NON_COLOR_SUFFIXES.iter().any(|s| k.ends_with(s))
}

/// Which area of REAPER a key paints.
pub fn classify(key: &str) -> Group {
    let k = key.to_ascii_lowercase();
    if !is_color(&k) {
        return Group::Modes;
    }
    if let Some((_, g)) = OVERRIDES.iter().find(|(name, _)| *name == k) {
        return *g;
    }
    PREFIXES
        .iter()
        .find(|(p, _)| k.starts_with(p))
        .map(|(_, g)| *g)
        .unwrap_or(Group::Other)
}

/// Group `keys` into display order, dropping empty groups.
pub fn group_all<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<(Group, Vec<&'a str>)> {
    let keys: Vec<&str> = keys.into_iter().collect();
    Group::all()
        .into_iter()
        .filter_map(|g| {
            let members: Vec<&str> = keys.iter().copied().filter(|k| classify(k) == g).collect();
            (!members.is_empty()).then_some((g, members))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawmodes_are_never_colors() {
        assert!(!is_color("marquee_drawmode"));
        assert!(!is_color("areasel_drawmode"));
        assert!(!is_color("col_mi_fade2_drawmode"));
        assert_eq!(classify("marquee_drawmode"), Group::Modes);
        assert!(is_color("col_main_bg"));
    }

    #[test]
    fn prefix_order_puts_specific_before_general() {
        // col_vu* would be swallowed by a naive col_ rule.
        assert_eq!(classify("col_vubot"), Group::Meters);
        assert_eq!(classify("col_mi_bg"), Group::Items);
        assert_eq!(classify("col_env1"), Group::Envelopes);
        assert_eq!(classify("col_main_bg"), Group::Main);
    }

    #[test]
    fn overrides_beat_prefixes() {
        // col_mixerbg starts with col_ but belongs to the mixer.
        assert_eq!(classify("col_mixerbg"), Group::Mcp);
        assert_eq!(classify("col_cursor"), Group::Arrange);
    }

    #[test]
    fn unknown_keys_land_in_other_not_lost() {
        assert_eq!(classify("some_future_reaper_key"), Group::Other);
    }

    #[test]
    fn grouping_keeps_every_key_exactly_once() {
        let keys = [
            "col_main_bg",
            "col_arrangebg",
            "col_mi_bg",
            "col_tcp_text",
            "col_vubot",
            "marquee_drawmode",
            "mystery_key",
        ];
        let grouped = group_all(keys);
        let total: usize = grouped.iter().map(|(_, m)| m.len()).sum();
        assert_eq!(total, keys.len());
    }
}
