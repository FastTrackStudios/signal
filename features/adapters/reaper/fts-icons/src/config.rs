use anyhow::{Context, Result};
use serde::Deserialize;

use crate::color;
use crate::render::StateStyle;

/// Partial style — every field optional, layered over a base.
/// Colors are hex strings (`#rgb`/`#rrggbb`/`#rrggbbaa`) or `"none"` to clear.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct StyleOverride {
    pub icon: Option<String>,
    pub bg: Option<String>,
    pub border: Option<String>,
    pub border_width: Option<f32>,
    pub icon_size: Option<f32>,
    pub bg_size: Option<f32>,
    pub corner_radius: Option<f32>,
}

impl StyleOverride {
    pub fn is_empty(&self) -> bool {
        self.icon.is_none()
            && self.bg.is_none()
            && self.border.is_none()
            && self.border_width.is_none()
            && self.icon_size.is_none()
            && self.bg_size.is_none()
            && self.corner_radius.is_none()
    }

    fn apply(&self, st: &mut StateStyle) -> Result<()> {
        if let Some(c) = &self.icon {
            st.icon = color::parse(c).context("style field `icon`")?;
        }
        if let Some(c) = &self.bg {
            st.bg = parse_optional(c).context("style field `bg`")?;
        }
        if let Some(c) = &self.border {
            st.border = parse_optional(c).context("style field `border`")?;
        }
        if let Some(v) = self.border_width {
            st.border_width = v;
        }
        if let Some(v) = self.icon_size {
            st.icon_size = v;
        }
        if let Some(v) = self.bg_size {
            st.bg_size = v;
        }
        if let Some(v) = self.corner_radius {
            st.corner_radius = v;
        }
        Ok(())
    }
}

fn parse_optional(s: &str) -> Result<Option<resvg::tiny_skia::Color>> {
    if s.eq_ignore_ascii_case("none") {
        Ok(None)
    } else {
        color::parse(s).map(Some)
    }
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct StateSet {
    pub all: Option<StyleOverride>,
    pub normal: Option<StyleOverride>,
    pub hover: Option<StyleOverride>,
    pub clicked: Option<StyleOverride>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct IconEntry {
    /// Output filename (no extension, no `_150` suffix — sizes go in subfolders).
    pub file: String,
    /// Iconify id (`mdi:eye-outline`) or generated text (`text:2/4`).
    pub source: String,
    /// Toolbar command id to point at this icon in reaper-menu.ini
    /// (e.g. `_FTS_TEMPO_INSERT_TIMESIG_4_4` or `40252`). Applied on --install.
    pub assign: Option<String>,
    /// Cell width in px at 100% (default 30; 60 = double-wide button).
    pub width: Option<f32>,
    pub all: Option<StyleOverride>,
    pub normal: Option<StyleOverride>,
    pub hover: Option<StyleOverride>,
    pub clicked: Option<StyleOverride>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// REAPER resource paths to install into (else auto-detected).
    pub resource_paths: Option<Vec<String>>,
    /// Plain output dir for non-install builds.
    pub out_dir: Option<String>,
    /// Default cell width for every icon in this file (px at 100%).
    pub width: Option<f32>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub defaults: StateSet,
    #[serde(default, rename = "icon")]
    pub icons: Vec<IconEntry>,
}

pub fn load(path: &std::path::Path) -> Result<ConfigFile> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse config {}", path.display()))
}

/// Resolve the 3 concrete states for one icon.
///
/// Layering per state S (later wins):
///   builtin → defaults.all → defaults.normal → defaults.S
///           → icon.all → icon.normal → icon.S
/// so an unspecified hover/clicked inherits the normal look.
pub fn resolve(defaults: &StateSet, icon: &IconEntry) -> Result<[StateStyle; 3]> {
    let mut out = Vec::with_capacity(3);
    for state in ["normal", "hover", "clicked"] {
        let mut st = StateStyle::default();
        for ov in [
            &defaults.all,
            &defaults.normal,
            pick(defaults, state),
            &icon.all,
            &icon.normal,
            pick_icon(icon, state),
        ]
        .into_iter()
        .flatten()
        {
            ov.apply(&mut st)
                .with_context(|| format!("icon {:?}, state {state}", icon.file))?;
        }
        out.push(st);
    }
    Ok(out.try_into().expect("3 states"))
}

fn pick<'a>(s: &'a StateSet, state: &str) -> &'a Option<StyleOverride> {
    match state {
        "hover" => &s.hover,
        "clicked" => &s.clicked,
        _ => &NONE,
    }
}

fn pick_icon<'a>(i: &'a IconEntry, state: &str) -> &'a Option<StyleOverride> {
    match state {
        "hover" => &i.hover,
        "clicked" => &i.clicked,
        _ => &NONE,
    }
}

static NONE: Option<StyleOverride> = None;

pub const EXAMPLE: &str = r##"# fts-icons config — `fts-icons build icons.toml --install`
#
# Colors: #rgb / #rrggbb / #rrggbbaa, or "none" to clear an inherited one.
# Sizes are px at 100% (inside a 30x30 cell); 150%/200% scale automatically.
# Unspecified hover/clicked states inherit the normal look.

[settings]
# resource_paths = ["~/.fts-dev"]   # else auto-detected
# out_dir = "out"                   # used without --install

[defaults.normal]
icon = "#e6e6e6"

[defaults.hover]
icon = "#ffd75e"

[defaults.clicked]
icon = "#ffffff"
bg = "#2e7d32aa"
border = "#69f0ae"

[[icon]]
file = "fts_automation"
source = "mdi:eye-outline"

[[icon]]
file = "fts_record_mode"
source = "ph:record-fill"
  [icon.normal]
  icon = "#ff5252"
"##;
