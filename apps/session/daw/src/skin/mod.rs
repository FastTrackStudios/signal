//! The mixer drawn from a frame theme instead of the painters here.
//!
//! A theme is frame documents (`.frame`, or the older `.fm`) whose
//! `mcp.strip` page is one channel strip: its tokens are the colours, its
//! bindings read the track ([`schema`]), its `@name` nodes are the
//! controls a click lands on ([`control`]), and the controls it places are
//! components. See `docs/frame-themes.md`.
//!
//! **Layered**: `FTS_THEME` is a list of paths (`:`-separated, the base
//! first), each a file or a folder of them, every one over the ones before
//! it — so a theme built on another is only the files it changes: one
//! control, the strip, a colour (`theme_runtime::layer`).
//!
//! Compiled in `FTS_THEME_MODE` or the documents' first mode, and
//! **hot-reloaded**: [`poll`] notices any of the files changing (or one
//! added to or taken from a folder) and recompiles, and a widget that sees
//! [`version`] move re-records. A theme that fails to compile leaves the
//! last good one on screen and says why once.
//!
//! The Tone rack is still the painters' own — the theme draws the REAPER
//! part of the strip, below it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime};

use daw_proto::Track;
use theme_runtime::{Fields, Rgba, Schema, Theme, Value};

use crate::arrangement::Palette;
use crate::mcp::Control;

pub mod canvas;
pub mod export;

/// The role a theme draws a mixer strip as.
pub const STRIP: &str = "mcp.strip";

/// The role a theme draws behind a strip's rack as, the rack's height
/// tall: the strip's ground, its edge, the selection rule at the top. The
/// strip's own fields.
pub const RACKBACK: &str = "mcp.rackback";

/// The role a theme draws one folder step as: the band under a nested
/// strip for each folder it is inside, `INDENT_STEP` tall. Drawn by the
/// painters when a theme has none.
pub const STEP: &str = "mcp.step";

/// How often [`poll`] looks at the file.
const POLL: Duration = Duration::from_millis(250);

/// What a strip theme can bind, in slot order.
///
/// Live, because the mixer draws them every frame rather than re-record
/// for them: the values the overlay already draws live (mute, solo, arm,
/// the fader, the meter, which control the pointer is on).
#[must_use]
pub fn schema() -> Schema {
    Schema::new()
        .field("track.name")
        .field("track.label")
        .field("track.number")
        .field("track.color")
        .field("track.has_color")
        .field("track.selected")
        .field("track.folder")
        .field("track.collapsed")
        .field("track.depth")
        .field("track.input")
        .field("track.ground")
        .field("track.edge")
        .field("track.has_edge")
        .field("track.tint")
        .field("track.band")
        .field("track.label_size")
        .field("track.parent_send")
        .field("ui.rack")
        .field("folder.tint")
        .field("folder.level")
        .field("button.label")
        .field("button.label_size")
        .field("button.on")
        .field("button.hover")
        .field("button.pressed")
        .field("button.has_icon")
        .field("button.enabled")
        .field("button.readout")
        .field("key.black")
        .field("key.piano")
        .field("key.label")
        .field("key.labelled")
        .field("group.label")
        .field("lane.name")
        .field("lane.active")
        .field("lane.reference")
        .field("lane.tinted")
        .field("lane.tint")
        .field("tick.bar")
        .field("tick.label")
        .field("tick.labelled")
        .field("marker.label")
        .field("strip.label")
        .field("menu.label")
        .field("menu.shortcut")
        .field("menu.enabled")
        .field("menu.hover")
        .field("menu.rule")
        .field("unit.name")
        .field("unit.value")
        .field("unit.bypassed")
        .field("unit.glyph")
        .field("unit.expands")
        .field("unit.zoom_lit")
        .field("phase.name")
        .field("phase.tint")
        .field("phase.blank")
        .field("phase.shut")
        .field("phase.held")
        .field("macro.name")
        .field("macro.value")
        .field("macro.says")
        .field("macro.tint")
        .field("macro.held")
        .field("row.indent")
        .field("row.number")
        .field("row.number_size")
        .field("row.number_x")
        .field("row.number_baseline")
        .field("row.has_number")
        .field("row.icon")
        .field("row.icon_x")
        .field("row.icon_y")
        .field("row.icon_size")
        .field("row.name")
        .field("row.name_size")
        .field("row.input")
        .layout("mixer.h")
        .layout("ui.mixing")
        .layout("ui.recording")
        .live("track.mute")
        .live("track.solo")
        .live("track.armed")
        .live("track.monitor")
        .live("track.phase")
        .live("track.fader")
        .live("track.volume")
        .live("track.pan")
        .live("track.fx")
        .live("track.sends")
        .live("track.receives")
        .live("meter.left")
        .live("meter.right")
        .live("meter.hold_left")
        .live("meter.hold_right")
        .live("meter.clip")
        .live("ui.hover")
        .live("ui.pressed")
        .live("meter.peak")
        .live("track.volume_label")
        .live("track.volume_label_w")
}

/// One strip's values, in [`schema`]'s order.
pub struct StripFields(Vec<Value>);

impl Fields for StripFields {
    fn field(&self, slot: u32) -> Value {
        self.0.field(slot)
    }
}

/// What a strip shows beyond the track itself.
#[derive(Clone, Copy, Default)]
pub struct Shown<'a> {
    /// The name as fitted to the plate, and the size it fits at.
    pub label: Option<(&'a str, f32)>,
    pub depth: usize,
    pub collapsed: bool,
    pub level: Option<daw_proto::TrackLevels>,
    pub clipped: bool,
    /// The control the pointer is over, and the one it is pressing.
    pub hover: Option<Control>,
    pub pressed: Option<Control>,
    /// The strip's width (the record input is elided to it).
    pub width: f64,
    /// The height the mixer's sections are resolved against: the whole
    /// panel's, less the rack — the same for every strip, however deep.
    pub mixer_h: f64,
    /// Whether a rack sits over the strip (which then draws the
    /// selection rule itself, at the top of the rack).
    pub rack: bool,
    /// The face text is measured in, for what has to be sized to it.
    pub font: Option<&'a crate::text::Font>,
    /// The mode the strip is laid out for — see `Settings::mixing`.
    pub mixing: bool,
    pub recording: bool,
}

static SCHEMA: std::sync::LazyLock<Schema> = std::sync::LazyLock::new(schema);

/// One folder step's values, for [`STEP`]: the colour of the folder it
/// stands for and how many folders out it is (1, the strip's own).
#[must_use]
pub fn step_fields(tint: vello::peniko::Color, level: usize) -> StripFields {
    let [r, g, b, a] = tint.to_rgba8().to_u8_array();
    values(&[
        ("folder.tint", Value::Color(Rgba::from_rgba8(r, g, b, a))),
        ("folder.level", Value::Num(crate::num::coord(level))),
    ])
}

/// The role a theme draws one track panel row as.
pub const ROW: &str = export::tcp::ROW;

/// What a track panel row shows beyond its track.
#[derive(Clone, Copy, Default)]
pub struct RowShown<'a> {
    /// How deep in folders the track is.
    pub depth: i32,
    /// The row's height, less the divider under it.
    pub height: f64,
    /// The face its text is measured in.
    pub font: Option<&'a crate::text::Font>,
    /// The control the pointer is over, and the one it is pressing.
    pub hover: Option<crate::row::Control>,
    pub pressed: Option<crate::row::Control>,
    /// Not recording: the window is in a mode other than Record, and the
    /// row has no arm (`ui.recording` false). Off by default — a row
    /// asked for with nothing said is a recording one, as it always was.
    pub idle: bool,
    /// A folder whose children are folded away (`track.collapsed`).
    pub collapsed: bool,
}

/// A row control's name in a theme: its `@name`, its `ui.hover`.
#[must_use]
pub const fn row_name(control: crate::row::Control) -> &'static str {
    use crate::row::Control as C;
    match control {
        C::Mute => "mute",
        C::Solo => "solo",
        C::RecArm => "arm",
        C::Volume => "volume",
        C::Pan => "pan",
        C::Routing => "routing",
        C::Fx => "fx",
        C::Name => "name",
        C::Folder => "folder",
        C::Phase => "phase",
        C::LanePrev => "lane-prev",
        C::LaneNext => "lane-next",
        C::LaneName => "lane-name",
        C::LaneShow => "lane-show",
        C::Automation => "automation",
    }
}

/// The row control a theme's `@name` is.
#[must_use]
pub fn row_control(name: &str) -> Option<crate::row::Control> {
    use crate::row::Control as C;
    [
        C::Mute,
        C::Solo,
        C::RecArm,
        C::Volume,
        C::Pan,
        C::Routing,
        C::Fx,
        C::Name,
        C::Folder,
        C::Phase,
    ]
    .into_iter()
    .find(|c| row_name(*c) == name)
}

/// The values a row's targets are laid out from — its indent, at
/// `depth` — for hit testing, which needs nothing else.
#[must_use]
pub fn row_hit_fields(depth: i32) -> StripFields {
    let tcp = crate::tcp::Tcp::FULL;
    values(&[(
        "row.indent",
        Value::Num((f64::from(depth.max(0)) * tcp.indent()).min(tcp.max_indent())),
    )])
}

/// `track`'s values for a track panel row: the strip's, and what the row
/// shows beyond them — the indent, the number and icon as the rail fits
/// them, the name as the field elides it, the record input.
#[must_use]
pub fn row_fields(palette: &Palette, track: &Track, shown: RowShown<'_>) -> StripFields {
    let tcp = crate::tcp::Tcp::FULL;
    let indent = (f64::from(shown.depth.max(0)) * tcp.indent()).min(tcp.max_indent());
    let mut out = fields(
        palette,
        track,
        Shown {
            collapsed: shown.collapsed,
            ..Shown::default()
        },
    );
    let mut set = |name: &str, v: Value| {
        if let Some(at) = SCHEMA
            .slot(name)
            .and_then(|s| usize::try_from(s).ok())
            .and_then(|s| out.0.get_mut(s))
        {
            *at = v;
        }
    };
    set("row.indent", Value::Num(indent));
    let named = |c: Option<crate::row::Control>| Value::text(c.map_or("", row_name));
    set("ui.hover", named(shown.hover));
    set("ui.pressed", named(shown.pressed));
    set("ui.recording", Value::Bool(!shown.idle));
    let Some(font) = shown.font else {
        return out;
    };
    let rail = crate::tcp::rail_layout(font, track, indent, 0.0, shown.height);
    set("row.has_number", Value::Bool(rail.number.is_some()));
    if let Some(number) = rail.number {
        set("row.number", Value::text(&number.text));
        set("row.number_size", Value::Num(f64::from(number.size)));
        set("row.number_x", Value::Num(number.x));
        set("row.number_baseline", Value::Num(number.baseline));
    }
    set("row.icon", Value::Bool(rail.icon.is_some()));
    if let Some(icon) = rail.icon {
        set("row.icon_x", Value::Num(icon.x0));
        set("row.icon_y", Value::Num(icon.y0));
        set("row.icon_size", Value::Num(icon.width()));
    }
    let (_, field_h) = crate::tcp::band(0.0, shown.height, crate::tcp::Density::at(shown.height));
    let size = tcp.name_size(field_h);
    let name_w = (tcp.volume_x() - tcp.name_x() - indent).max(0.0);
    set(
        "row.name",
        Value::text(&font.elide(&track.name, size, name_w)),
    );
    set("row.name_size", Value::Num(f64::from(size)));
    set(
        "row.input",
        Value::text(&font.elide(&crate::tcp::record_input(track), 11.0, 180.0)),
    );
    out
}

/// `track`'s values for a strip theme.
#[must_use]
pub fn fields(palette: &Palette, track: &Track, shown: Shown<'_>) -> StripFields {
    let mut values = vec![Value::Missing; SCHEMA.names().count()];
    let mut set = |name: &str, v: Value| {
        if let Some(slot) = SCHEMA.slot(name).and_then(|s| usize::try_from(s).ok())
            && let Some(at) = values.get_mut(slot)
        {
            *at = v;
        }
    };
    let color = |c: vello::peniko::Color| {
        let [r, g, b, a] = c.to_rgba8().to_u8_array();
        Value::Color(Rgba::from_rgba8(r, g, b, a))
    };
    let fraction = |v: f32| crate::engine::meter_fraction(v);
    let level = shown.level.unwrap_or_default();
    let named = |c: Option<Control>| c.map_or(Value::text(""), |c| Value::text(name(c)));

    set("track.name", Value::text(&track.name));
    let (label, label_size) = shown.label.unwrap_or((&track.name, 11.0));
    set("track.label", Value::text(label));
    set("track.label_size", Value::Num(f64::from(label_size)));
    set("track.number", Value::Num(f64::from(track.index) + 1.0));
    set(
        "track.color",
        color(crate::tcp::track_color(palette, track)),
    );
    set("track.has_color", Value::Bool(track.color.is_some()));
    set("track.selected", Value::Bool(track.selected));
    set("track.folder", Value::Bool(track.is_folder));
    set("track.collapsed", Value::Bool(shown.collapsed));
    set("track.depth", Value::Num(crate::num::coord(shown.depth)));
    let input = crate::tcp::record_input(track);
    let input = match shown.font {
        // As the painters elide it: to the field, less its caret.
        Some(font) => font.elide(&input, 10.0, (shown.width - 12.0).min(75.0) - 14.0),
        None => input,
    };
    set("track.input", Value::text(&input));
    set(
        "track.ground",
        color(crate::mcp::strip_ground(palette, track)),
    );
    let edge = crate::mcp::strip_edge(palette, track);
    set("track.has_edge", Value::Bool(edge.is_some()));
    set("track.edge", edge.map_or(Value::Missing, color));
    set("track.tint", color(crate::tcp::row_tint(palette, track)));
    set("track.band", color(crate::tcp::folder_band(palette, track)));
    set("track.parent_send", Value::Bool(track.parent_send));
    set("ui.rack", Value::Bool(shown.rack));
    set("mixer.h", Value::Num(shown.mixer_h));
    set("ui.mixing", Value::Bool(shown.mixing));
    set("ui.recording", Value::Bool(shown.recording));

    set("track.mute", Value::Bool(track.muted));
    set("track.solo", Value::Bool(track.soloed));
    set("track.armed", Value::Bool(track.armed));
    set(
        "track.monitor",
        Value::Num(match track.input_monitor {
            daw_proto::InputMonitoringMode::Off => 0.0,
            daw_proto::InputMonitoringMode::Normal => 1.0,
            daw_proto::InputMonitoringMode::NotWhenPlaying => 2.0,
        }),
    );
    set("track.phase", Value::Bool(track.phase_inverted));
    set(
        "track.fader",
        Value::Num(crate::tcp::volume_fraction(track.volume)),
    );
    set("track.volume", Value::Num(track.volume));
    set("track.pan", Value::Num(track.pan.clamp(-1.0, 1.0)));
    set("track.fx", Value::Num(f64::from(track.fx_count)));
    set("track.sends", Value::Num(f64::from(track.send_count)));
    set("track.receives", Value::Num(f64::from(track.receive_count)));
    let (left, right) = (fraction(level.peak_left), fraction(level.peak_right));
    set("meter.left", Value::Num(left));
    set("meter.right", Value::Num(right));
    set("meter.peak", Value::Num(left.max(right)));
    set("meter.hold_left", Value::Num(fraction(level.hold_left)));
    set("meter.hold_right", Value::Num(fraction(level.hold_right)));
    set("meter.clip", Value::Bool(shown.clipped));
    set("ui.hover", named(shown.hover));
    set("ui.pressed", named(shown.pressed));
    let readout = crate::overlay::volume_label(track.volume);
    if let Some(font) = shown.font {
        set(
            "track.volume_label_w",
            Value::Num(font.width(&readout, 9.0) + 6.0),
        );
    }
    set("track.volume_label", Value::text(&readout));
    StripFields(values)
}

/// A control's name in a theme: the `x` of a node named `@x`.
#[must_use]
pub const fn name(control: Control) -> &'static str {
    match control {
        Control::Fx => "fx",
        Control::Pan => "pan",
        Control::RecArm => "arm",
        Control::Mute => "mute",
        Control::Solo => "solo",
        Control::Routing => "routing",
        Control::Volume => "fader",
        Control::Name => "name",
        Control::Monitor => "monitor",
        Control::Clip => "clip",
        Control::Folder => "folder",
    }
}

/// The control a theme's `@name` is.
#[must_use]
pub fn control(name: &str) -> Option<Control> {
    Some(match name {
        "fx" => Control::Fx,
        "pan" => Control::Pan,
        "arm" => Control::RecArm,
        "mute" => Control::Mute,
        "solo" => Control::Solo,
        "routing" => Control::Routing,
        "fader" | "volume" => Control::Volume,
        "name" => Control::Name,
        "monitor" => Control::Monitor,
        "clip" => Control::Clip,
        "folder" => Control::Folder,
        _ => return None,
    })
}

// ── the loaded theme ──────────────────────────────────────────────────

struct Loaded {
    /// The layers as given, base first: files and folders.
    paths: Vec<PathBuf>,
    mode: Option<String>,
    /// Every file they came to and when it last changed.
    stamp: Vec<(PathBuf, Option<SystemTime>)>,
    checked: Instant,
    /// The last error reported, so a broken file is reported once.
    reported: Option<String>,
}

#[cfg(not(test))]
static THEME: RwLock<Option<Arc<Theme>>> = RwLock::new(None);

// Each test its own: a test that sets a theme must not reach the
// painters' tests running beside it.
#[cfg(test)]
thread_local! {
    static THEME: RwLock<Option<Arc<Theme>>> = const { RwLock::new(None) };
}
static VERSION: AtomicU64 = AtomicU64::new(0);
#[cfg(not(test))]
static SOURCE: Mutex<Option<Loaded>> = Mutex::new(None);

#[cfg(test)]
thread_local! {
    static SOURCE: Mutex<Option<Loaded>> = const { Mutex::new(None) };
}

/// The files being watched, for `f` to look at or change.
fn with_source<R>(f: impl FnOnce(&mut Option<Loaded>) -> R) -> R {
    #[cfg(not(test))]
    return f(&mut SOURCE.lock().unwrap_or_else(PoisonError::into_inner));
    #[cfg(test)]
    SOURCE.with(|s| f(&mut s.lock().unwrap_or_else(PoisonError::into_inner)))
}

/// The theme the mixer draws with, when there is one with a strip.
#[must_use]
pub fn current() -> Option<Arc<Theme>> {
    loaded().filter(|t| t.has_role(STRIP))
}

/// The theme loaded, whatever it draws — for a view that asks the theme
/// for its own role (the rails) and draws itself when there is none.
#[must_use]
pub fn loaded() -> Option<Arc<Theme>> {
    #[cfg(not(test))]
    return THEME.read().unwrap_or_else(PoisonError::into_inner).clone();
    #[cfg(test)]
    THEME.with(|t| t.read().unwrap_or_else(PoisonError::into_inner).clone())
}

/// `base` recoloured by the loaded theme: every colour the painters draw
/// with that the theme names as a token is the theme's.
///
/// So what is still painted rather than themed — the rack's curves and
/// knobs, the live strips — wears the theme's colours as soon as it has
/// them: a theme that sets `$mute` reddens every mute the painters draw
/// too, not only the ones it draws itself.
#[must_use]
pub fn palette(base: &Palette) -> Palette {
    let mut p = base.clone();
    let Some(theme) = loaded() else {
        return p;
    };
    let colour = |name: &str| {
        theme.token(name).and_then(Value::color).map(|c| {
            let [r, g, b, a] = c.to_rgba8();
            vello::peniko::Color::from_rgba8(r, g, b, a)
        })
    };
    for (name, field) in [
        ("text", &mut p.text),
        ("text.dim", &mut p.text_dim),
        ("text.faint", &mut p.text_faint),
        ("accent", &mut p.accent),
        ("mute", &mut p.mute),
        ("solo", &mut p.solo),
        ("rec", &mut p.rec),
        ("pan", &mut p.pan),
        ("meter.safe", &mut p.meter_safe),
        ("meter.warn", &mut p.meter_warn),
        ("meter.danger", &mut p.meter_danger),
        ("field", &mut p.tcp_field),
        ("combo", &mut p.tcp_combo),
        ("rack.well", &mut p.tcp_meter_well),
        ("rail.ground", &mut p.tcp_gutter),
        ("rail.rule", &mut p.tcp_rule),
        ("rail.button", &mut p.tcp_button),
    ] {
        if let Some(c) = colour(name) {
            *field = c;
        }
    }
    let chrome = &mut p.chrome;
    for (name, field) in [
        ("hardware", &mut chrome.hardware),
        ("hardware.edge", &mut chrome.hardware_edge),
        ("hardware.mark", &mut chrome.hardware_mark),
        ("surface", &mut chrome.surface),
        ("surface.sunken", &mut chrome.surface_sunken),
        ("border", &mut chrome.border),
        ("text", &mut chrome.text),
        ("text.dim", &mut chrome.text_dim),
        ("text.faint", &mut chrome.text_faint),
        ("accent", &mut chrome.accent),
    ] {
        if let Some(c) = colour(name) {
            *field = crate::tcp::to_theme(c);
        }
    }
    p
}

/// Values for a view's own fields (`name`, value), every other field
/// missing — for the roles that are not a strip.
#[must_use]
pub fn values(set: &[(&str, Value)]) -> StripFields {
    let mut values = vec![Value::Missing; SCHEMA.names().count()];
    for (name, v) in set {
        if let Some(at) = SCHEMA
            .slot(name)
            .and_then(|s| usize::try_from(s).ok())
            .and_then(|s| values.get_mut(s))
        {
            *at = v.clone();
        }
    }
    StripFields(values)
}

/// Bumped whenever [`current`] changes: what a recording compares to
/// know it is stale.
#[must_use]
pub fn version() -> u64 {
    VERSION.load(Ordering::Acquire)
}

/// Use `theme` (or none) from now on.
pub fn set(theme: Option<Arc<Theme>>) {
    #[cfg(not(test))]
    {
        *THEME.write().unwrap_or_else(PoisonError::into_inner) = theme;
    }
    #[cfg(test)]
    THEME.with(|t| *t.write().unwrap_or_else(PoisonError::into_inner) = theme);
    VERSION.fetch_add(1, Ordering::AcqRel);
}

/// Load the theme layered from `paths` (base first; files, or folders of
/// them) in `mode`, and keep watching them.
///
/// # Errors
/// Why it did not compile; the previous theme stays.
pub fn load(paths: Vec<PathBuf>, mode: Option<String>) -> Result<(), String> {
    let stamp = stamp(&paths);
    let result = compile(&stamp, mode.as_deref());
    let loaded = Loaded {
        paths,
        mode,
        stamp,
        checked: Instant::now(),
        reported: result.as_ref().err().cloned(),
    };
    with_source(|source| *source = Some(loaded));
    result.map(|theme| set(Some(Arc::new(theme))))
}

/// Load `FTS_THEME`, once, if it is set.
pub fn from_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Some(paths) = std::env::var_os("FTS_THEME") else {
            return;
        };
        let mode = std::env::var("FTS_THEME_MODE").ok();
        if let Err(why) = load(std::env::split_paths(&paths).collect(), mode) {
            tracing::warn!(theme.error = %why, "theme did not load");
        }
    });
}

/// Recompile the theme if any of its files changed since the last look,
/// or a folder gained or lost one (looked at no more than every
/// [`POLL`]). True when [`current`] changed.
pub fn poll() -> bool {
    let fresh = with_source(|source| {
        let loaded = source.as_mut()?;
        if loaded.checked.elapsed() < POLL {
            return None;
        }
        loaded.checked = Instant::now();
        let now = stamp(&loaded.paths);
        if now == loaded.stamp {
            return None;
        }
        loaded.stamp = now;
        match compile(&loaded.stamp, loaded.mode.as_deref()) {
            Ok(theme) => {
                loaded.reported = None;
                Some(theme)
            }
            Err(why) => {
                if loaded.reported.as_deref() != Some(why.as_str()) {
                    tracing::warn!(theme.error = %why, "theme did not reload; keeping the last good one");
                    loaded.reported = Some(why);
                }
                None
            }
        }
    });
    fresh.map(|theme| set(Some(Arc::new(theme)))).is_some()
}

/// Whether `path` is a frame document: `.frame`, or the older `.fm`.
fn is_theme_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "frame" || e == "fm")
}

/// The files `paths` come to, in layering order — a folder's by name — each
/// with when it last changed.
fn stamp(paths: &[PathBuf]) -> Vec<(PathBuf, Option<SystemTime>)> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| is_theme_file(p))
                .collect();
            inside.sort();
            files.extend(inside);
        } else {
            files.push(path.clone());
        }
    }
    files
        .into_iter()
        .map(|f| {
            let modified = std::fs::metadata(&f).and_then(|m| m.modified()).ok();
            (f, modified)
        })
        .collect()
}

fn compile(files: &[(PathBuf, Option<SystemTime>)], mode: Option<&str>) -> Result<Theme, String> {
    if files.is_empty() {
        return Err("no theme files".into());
    }
    let mut sources = Vec::new();
    for (path, _) in files {
        let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        sources.push((path.display().to_string(), src));
    }
    let sources: Vec<(&str, &str)> = sources
        .iter()
        .map(|(l, s)| (l.as_str(), s.as_str()))
        .collect();
    Theme::from_sources(&sources, schema(), mode).map_err(|errors| {
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEME: &str = r##"<Document>
  <Variables modes="dark"><Var name="ground" value="#202020" /></Variables>
  <Page name="mcp.strip">
    <Frame name="Strip" w=86 h=400 bind:fill="$ground">
      <Frame name="@mute" x=55 y=127 w=21 h=20 bind:fill="track.mute ? #ff0000 : #000000" />
    </Frame>
  </Page>
</Document>"##;

    /// The file is the theme: saved, it reloads; saved broken, the last
    /// good one stays.
    #[test]
    fn a_saved_theme_reloads_and_a_broken_one_does_not() {
        let dir = std::env::temp_dir().join(format!("fts-skin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("strip.frame");
        std::fs::write(&path, THEME).expect("write");
        load(vec![path.clone()], None).expect("loads");
        assert!(current().is_some_and(|t| t.token("ground").is_some()));

        let wait = || std::thread::sleep(POLL + Duration::from_millis(20));
        wait();
        std::fs::write(&path, THEME.replace("track.mute", "track.nonsense")).expect("write");
        assert!(!poll(), "a broken theme is not taken");
        let ground = current()
            .and_then(|t| t.token("ground").and_then(Value::color))
            .map(Rgba::to_hex);
        assert_eq!(
            ground.as_deref(),
            Some("#202020"),
            "the last good one stays"
        );

        wait();
        std::fs::write(&path, THEME.replace("#202020", "#303030")).expect("write");
        assert!(poll(), "a fixed theme is taken");
        let ground = current()
            .and_then(|t| t.token("ground").and_then(Value::color))
            .map(Rgba::to_hex);
        assert_eq!(ground.as_deref(), Some("#303030"));

        // And a mute lands where the theme put it.
        let theme = current().expect("theme");
        let track = Track {
            muted: true,
            ..Track::default()
        };
        let fields = fields(
            &Palette::from_theme(&daw_ui::theming::Theme::dark()),
            &track,
            Shown::default(),
        );
        let hit = theme.hit(
            STRIP,
            (86.0, 400.0),
            &fields,
            vello::kurbo::Point::new(60.0, 130.0),
        );
        assert_eq!(hit.as_deref().and_then(control), Some(Control::Mute));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A theme over another: the base's folder, then a folder of what it
    /// changes. A file dropped into the second is taken up as it lands.
    #[test]
    fn a_layer_dropped_into_a_folder_is_taken_up() {
        let dir = std::env::temp_dir().join(format!("fts-skin-layers-{}", std::process::id()));
        let (base, mine) = (dir.join("base"), dir.join("mine"));
        std::fs::create_dir_all(&base).expect("temp dir");
        std::fs::create_dir_all(&mine).expect("temp dir");
        std::fs::write(base.join("strip.frame"), THEME).expect("write");
        // Not a theme file: ignored.
        std::fs::write(base.join("notes.txt"), "not a theme").expect("write");
        load(vec![base.clone(), mine.clone()], None).expect("loads");
        let ground = || {
            current()
                .and_then(|t| t.token("ground").and_then(Value::color))
                .map(Rgba::to_hex)
        };
        assert_eq!(ground().as_deref(), Some("#202020"));

        std::thread::sleep(POLL + Duration::from_millis(20));
        std::fs::write(
            mine.join("colours.fm"),
            r##"<Document><Variables modes="dark"><Var name="ground" value="#405060" /></Variables></Document>"##,
        )
        .expect("write");
        assert!(poll(), "the new layer is taken up");
        assert_eq!(
            ground().as_deref(),
            Some("#405060"),
            "and it is over the base"
        );
        // The base's strip is still the strip.
        assert!(current().is_some());

        std::thread::sleep(POLL + Duration::from_millis(20));
        std::fs::remove_file(mine.join("colours.fm")).expect("remove");
        assert!(poll(), "a layer taken away is noticed");
        assert_eq!(ground().as_deref(), Some("#202020"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
