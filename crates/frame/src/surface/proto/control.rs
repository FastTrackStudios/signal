//! Controls: what makes a piece of a design *do* something.
//!
//! A control is attached to a design node (a component instance, a frame —
//! anything drawn). It names its kind (knob, fader, footswitch…), what it
//! is bound to in an engine's [`Catalog`](crate::surface::proto::Catalog), and which of its
//! descendant nodes the runtime drives (the needle, the arc, the readout).
//! The look is entirely the design's; the behavior is entirely the control's.

use facet::Facet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::surface::proto::curve::Curve;
use crate::surface::proto::interact::{
    AddRule, ClickAction, Command, Condition, Interact, MenuItem, Placement, Show, Zoom,
};
use crate::surface::proto::param::{
    Catalog, EntryId, ParamAddr, ParamKind, StreamKind, ValueFormat,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ButtonMode {
    /// Press flips the value.
    #[default]
    Toggle,
    /// On while held.
    Momentary,
    /// Fires an action; no state.
    Trigger,
    /// One of a bank of radio buttons: sets the bound (choice) param to
    /// option `index` of `count`; lit while that option is selected.
    Select { index: u32, count: u32 },
    /// Each press steps the bound choice param to its next option,
    /// wrapping (an EQ band's shape, a stereo placement).
    Cycle,
}

impl ButtonMode {
    /// The position (0..1) a `Select` button sets.
    #[must_use]
    pub fn select_position(index: u32, count: u32) -> f64 {
        if count < 2 {
            0.0
        } else {
            f64::from(index.min(count - 1)) / f64::from(count - 1)
        }
    }
}

/// The behavior of a control. Each kind has a fixed interaction contract
/// (see the `surface` spec) and a fixed set of slot roles it drives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
// A handle's two axes make it the big one; a surface holds tens of controls,
// cloned only when a runtime is built.
#[allow(clippy::large_enum_variant)]
pub enum ControlKind {
    /// Rotary; vertical drag, fine with Shift, wheel, double-click reset.
    Knob {
        /// Sweep in degrees (default 270), centered on straight up.
        sweep_degrees: f64,
    },
    /// Linear slider/fader.
    Fader {
        orientation: Orientation,
    },
    Button {
        mode: ButtonMode,
    },
    /// Stage footswitch: tap and hold are different gestures.
    Footswitch {
        /// Hold threshold (default 500 ms).
        hold_ms: u32,
    },
    /// Level / gain-reduction meter, driven by a stream.
    Meter {
        orientation: Orientation,
    },
    /// A needle meter (VU, gain reduction, tuner): the `Needle` slot sweeps
    /// `sweep_degrees` (centered on straight up) across `floor_db..ceiling_db`
    /// of a level stream — or across a bound param's range.
    Gauge {
        sweep_degrees: f64,
        floor_db: f64,
        ceiling_db: f64,
    },
    /// Two parameters at once (x → `binding`, y → `y_binding`).
    XyPad,
    /// Pick one of a choice parameter's options (segmented, dropdown, radio).
    Selector,
    /// Text readout of a bound value or stream.
    Readout,
    /// Draws the latest frame of the bound stream (a spectrum, a history
    /// of gain reduction, a waveform) into its `Trace` / `Area` slots.
    Plot {
        style: PlotStyle,
        x: Axis,
        y: PlotRange,
    },
    /// Draws a function of params — `provider` names a function in the
    /// runtime's curve registry (`eq.response`, `comp.transfer`,
    /// `adsr.envelope`, `filter.response`), `inputs` bind its arguments.
    /// `series` picks one of the provider's series (`"b2"` = EQ band 2's
    /// own curve); `None` = its first (the combined response).
    Curve {
        provider: String,
        inputs: Vec<CurveInput>,
        series: Option<String>,
        style: PlotStyle,
        x: Axis,
        y: PlotRange,
    },
    /// A draggable point inside its parent (the plot area): `x` and `y`
    /// params placed through `x_map` / `y_map`, the wheel turns `wheel`
    /// (an EQ band's Q). Either axis may be unbound (a threshold chip
    /// only slides sideways).
    Handle {
        x: Option<Binding>,
        y: Option<Binding>,
        wheel: Option<Binding>,
        x_map: HandleAxis,
        y_map: HandleAxis,
    },
    /// A template: the runtime draws this node's subtree once per item
    /// `first..first + count` of an indexed family, `{n}` in every address,
    /// name and text inside it resolved to the item's index. An item shows
    /// while its `used` param is on (all show without one): adding an item
    /// is setting `used`, removing it clearing it. The template itself is
    /// never drawn. Instances get stable ids (from the template's and the
    /// index), so live reloads keep their state.
    Repeat {
        first: u32,
        count: u32,
        used: Option<ParamAddr>,
    },
    /// The empty space of a plot area: press-and-drag draws a marquee that
    /// selects the selectable controls inside it, a click clears the
    /// selection, a double-click adds an item there (`add`) — its position
    /// read through `x` / `y` (the area's axes, `y` zoomed by the control's
    /// [`Zoom`]).
    Canvas {
        x: Axis,
        y: Axis,
        add: Option<AddRule>,
    },
    /// A panel that follows its `anchor` node (a handle): placed on its
    /// preferred side `gap` px away, flipped when that side doesn't fit
    /// inside the surface, clamped in, drawn above everything and shown per
    /// `show`. With `context` (a [`Repeat`](Self::Repeat) template node)
    /// there is one popover for all items: its anchor and every `{n}` inside
    /// it follow the active item (the dragged, last selected or hovered one).
    Popover {
        anchor: Option<Uuid>,
        placement: Placement,
        gap: f64,
        show: Show,
        context: Option<Uuid>,
    },
    /// A context menu: rows (`items`, each naming its row node), opened by
    /// a right-click on a control whose [`Interact::menu`] names it, at the
    /// pointer (clamped inside the surface). Arrow keys move the highlight,
    /// Enter runs, Right / Left open and close submenus, Escape closes. With
    /// `context` its `{n}` resolve to the item it was opened on.
    Menu {
        items: Vec<MenuItem>,
        context: Option<Uuid>,
    },
    /// Inline text editing of a text param: click (or a
    /// [`Command::Edit`]) to edit — caret, selection, typing, Backspace /
    /// Delete, arrows, Home / End, Enter to commit, Escape to cancel. With
    /// `click_to_edit` off it only edits when told to (a name label on a
    /// handle, renamed by double-clicking the handle).
    TextField {
        click_to_edit: bool,
        max_len: u32,
    },
}

/// How a plot or curve draws its points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum PlotStyle {
    /// A polyline; `Area` slots fill down to the bottom of the range.
    Line,
    /// A polyline, `Area` slots filled to `baseline` (in y units: 0 dB for
    /// an EQ band, the top for gain reduction hanging down).
    Area { baseline: f64 },
    /// One bar per point (decimated to the pixel width), up from the bottom.
    Bars,
    /// A staircase through the points.
    Steps,
    /// A dot per point.
    Dots,
}

/// Where values sit across a plot's width (or height).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Axis {
    /// `min..max` evenly. A stream's values are spread over it by index.
    Linear { min: f64, max: f64 },
    /// Equal ratios take equal room (frequency). A spectrum feed's bins
    /// land where their own band (`Spectrum { f_min, f_max }`) puts them.
    Log { f_min: f64, f_max: f64 },
    /// A history, oldest at the left and the newest at the right edge: a
    /// `History` feed's frame as sent, or — for a scalar feed (a level, a
    /// gain reduction) — the last `len` readings, kept by the runtime.
    Time { len: u32 },
}

impl Axis {
    /// A value → 0..1 across the axis (not clamped).
    #[must_use]
    pub fn to_unit(&self, v: f64) -> f64 {
        match *self {
            Self::Linear { min, max } => {
                if (max - min).abs() < f64::EPSILON {
                    0.0
                } else {
                    (v - min) / (max - min)
                }
            }
            Self::Log { f_min, f_max } => {
                if f_min > 0.0 && f_max > f_min && v > 0.0 {
                    (v / f_min).ln() / (f_max / f_min).ln()
                } else {
                    0.0
                }
            }
            Self::Time { len } => v / f64::from(len.max(2) - 1),
        }
    }

    /// 0..1 across the axis → a value.
    #[must_use]
    pub fn from_unit(&self, u: f64) -> f64 {
        match *self {
            Self::Linear { min, max } => min + (max - min) * u,
            Self::Log { f_min, f_max } if f_min > 0.0 && f_max > f_min => {
                f_min * (f_max / f_min).powf(u)
            }
            Self::Log { f_min, .. } => f_min,
            Self::Time { len } => u * f64::from(len.max(2) - 1),
        }
    }
}

/// A plot's vertical range: `min` at the bottom, `max` at the top, shaped
/// by `curve` (a meter-style law; `Linear` for dB).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct PlotRange {
    pub min: f64,
    pub max: f64,
    pub curve: Curve,
}

impl PlotRange {
    #[must_use]
    pub fn linear(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            curve: Curve::Linear,
        }
    }

    /// A value → 0..1 up the plot (clamped; NaN stays NaN).
    #[must_use]
    pub fn to_unit(&self, v: f64) -> f64 {
        if v.is_nan() {
            return f64::NAN;
        }
        let span = self.max - self.min;
        if span.abs() < f64::EPSILON {
            return 0.0;
        }
        self.curve.apply(((v - self.min) / span).clamp(0.0, 1.0))
    }

    /// 0..1 up the plot → a value.
    #[must_use]
    pub fn from_unit(&self, u: f64) -> f64 {
        self.min + (self.max - self.min) * self.curve.invert(u.clamp(0.0, 1.0))
    }
}

/// One argument of a curve provider and where its value comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct CurveInput {
    /// The provider's argument (`threshold`, `b2_freq`). The name `marker`
    /// is the curve's own: `Marker` slots ride the curve at its value (a
    /// compressor's input level) and the provider never sees it.
    pub name: String,
    pub source: InputSource,
}

impl CurveInput {
    #[must_use]
    pub fn param(name: impl Into<String>, addr: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: InputSource::Param {
                addr: ParamAddr::new(addr),
            },
        }
    }

    #[must_use]
    pub fn stream(name: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: InputSource::Stream {
                id: EntryId::new(id),
                channel: None,
            },
        }
    }

    #[must_use]
    pub fn constant(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            source: InputSource::Const { value },
        }
    }
}

/// A curve argument's value: a param in engineering units (a choice
/// param's option index, a toggle's 0/1), a stream's latest reading, or a
/// constant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum InputSource {
    Param { addr: ParamAddr },
    Stream { id: EntryId, channel: Option<u8> },
    Const { value: f64 },
}

/// How a handle's param lands on one axis of its parent area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct HandleAxis {
    pub axis: Axis,
    /// Added to the bound value (engineering units) before it is placed:
    /// these params' values plus `shift`. An ADSR's decay point sits after
    /// its attack (`after: [attack]`); its release point after attack,
    /// decay and a drawn sustain (`shift`). With no binding on this axis,
    /// the handle still follows them.
    pub after: Vec<ParamAddr>,
    pub shift: f64,
}

impl HandleAxis {
    #[must_use]
    pub fn new(axis: Axis) -> Self {
        Self {
            axis,
            after: Vec::new(),
            shift: 0.0,
        }
    }

    #[must_use]
    pub fn after(mut self, addrs: &[&str], shift: f64) -> Self {
        self.after = addrs.iter().map(|a| ParamAddr::new(*a)).collect();
        self.shift = shift;
        self
    }

    /// Whether this axis moves at all (bound, or offset by params).
    #[must_use]
    pub fn follows(&self, bound: bool) -> bool {
        bound || !self.after.is_empty() || self.shift != 0.0
    }
}

/// The curve providers frame ships (`crate::surface::runtime::curves`), each with
/// the inputs it can't do without. Engines may register more, or their own
/// implementation under the same name.
pub const BUILTIN_CURVES: &[(&str, &[&str])] = &[
    ("eq.response", &[]),
    ("comp.transfer", &["threshold", "ratio"]),
    ("adsr.envelope", &["attack", "decay", "sustain", "release"]),
    ("filter.response", &["freq"]),
];

impl ControlKind {
    #[must_use]
    pub const fn knob() -> Self {
        Self::Knob {
            sweep_degrees: 270.0,
        }
    }

    #[must_use]
    pub const fn fader() -> Self {
        Self::Fader {
            orientation: Orientation::Vertical,
        }
    }

    #[must_use]
    pub const fn footswitch() -> Self {
        Self::Footswitch { hold_ms: 500 }
    }

    /// Short lowercase name, as used in markup and lint messages.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Knob { .. } => "knob",
            Self::Fader { .. } => "fader",
            Self::Button { .. } => "button",
            Self::Footswitch { .. } => "footswitch",
            Self::Meter { .. } => "meter",
            Self::Gauge { .. } => "gauge",
            Self::XyPad => "xy-pad",
            Self::Selector => "selector",
            Self::Readout => "readout",
            Self::Plot { .. } => "plot",
            Self::Curve { .. } => "curve",
            Self::Handle { .. } => "handle",
            Self::Repeat { .. } => "repeat",
            Self::Canvas { .. } => "canvas",
            Self::Popover { .. } => "popover",
            Self::Menu { .. } => "menu",
            Self::TextField { .. } => "text-field",
        }
    }

    /// Whether the control writes a value (vs. only displaying one).
    #[must_use]
    pub const fn is_input(&self) -> bool {
        !matches!(
            self,
            Self::Meter { .. }
                | Self::Gauge { .. }
                | Self::Readout
                | Self::Plot { .. }
                | Self::Curve { .. }
                | Self::Repeat { .. }
        )
    }

    /// Kinds that bind nothing themselves (their children and rules do).
    #[must_use]
    pub const fn is_container(&self) -> bool {
        matches!(
            self,
            Self::Repeat { .. } | Self::Canvas { .. } | Self::Popover { .. } | Self::Menu { .. }
        )
    }
}

/// What a control is wired to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Source {
    /// Read and write an engine parameter.
    Param { addr: ParamAddr },
    /// Fire an engine action (buttons, footswitch taps).
    Action { id: EntryId },
    /// Display a telemetry stream (meters, readouts). `channel` picks one
    /// channel of a multi-channel level stream.
    Stream { id: EntryId, channel: Option<u8> },
    /// Select within a collection (songs, patches).
    Collection { id: EntryId },
}

/// A control ⇄ engine edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Binding {
    pub source: Source,
    /// Shapes control position → value sent (and back for display).
    pub curve: Curve,
    /// Sub-range of the target a full control sweep covers (normalized).
    pub min: f64,
    pub max: f64,
}

impl Binding {
    #[must_use]
    pub fn param(addr: impl Into<String>) -> Self {
        Self {
            source: Source::Param {
                addr: ParamAddr::new(addr),
            },
            curve: Curve::Linear,
            min: 0.0,
            max: 1.0,
        }
    }

    #[must_use]
    pub fn action(id: impl Into<String>) -> Self {
        Self {
            source: Source::Action {
                id: EntryId::new(id),
            },
            curve: Curve::Linear,
            min: 0.0,
            max: 1.0,
        }
    }

    #[must_use]
    pub fn stream(id: impl Into<String>) -> Self {
        Self {
            source: Source::Stream {
                id: EntryId::new(id),
                channel: None,
            },
            curve: Curve::Linear,
            min: 0.0,
            max: 1.0,
        }
    }

    /// Control position (0..1) → target value (normalized).
    #[must_use]
    pub fn to_target(&self, position: f64) -> f64 {
        self.min + (self.max - self.min) * self.curve.apply(position)
    }

    /// Target value (normalized) → control position (0..1).
    #[must_use]
    pub fn to_position(&self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span == 0.0 {
            return 0.0;
        }
        self.curve
            .invert(((value - self.min) / span).clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum MidiMessage {
    Cc { number: u8 },
    Note { number: u8 },
    PitchBend,
    Aftertouch,
}

/// A hardware control driving this control.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MidiMapping {
    /// 1..=16; `None` = omni.
    pub channel: Option<u8>,
    pub message: MidiMessage,
    pub curve: Curve,
}

/// Which part of a control's drawing a descendant node is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet,
)]
#[repr(u8)]
pub enum SlotRole {
    /// Rotates with the value (knob pointer).
    Needle,
    /// Its outline is replaced by an arc from the start angle to the value.
    ValueArc,
    /// Arc showing the modulation range.
    ModArc,
    /// Grows along the control's axis with the value (fader fill, meter bar).
    Fill,
    /// Moves along the axis with the value (fader cap).
    Thumb,
    /// Its text becomes the formatted value.
    Value,
    /// Its text becomes the bound parameter's name (unless authored).
    Label,
    /// Visible (full opacity) when on, dimmed when off.
    Led,
    /// Peak-hold marker on meters.
    PeakHold,
    /// Its opacity follows the value (0 = dark, 1 = full): a lamp whose
    /// brightness is a stream (an LFO rate lamp). Emission scales with it.
    Glow,
    /// A plot's or curve's line: its geometry becomes the points (stroke it).
    Trace,
    /// The region between a plot's or curve's line and its baseline (fill it).
    Area,
    /// Centred on the curve at its `marker` input (a compressor's input
    /// ball on the transfer curve); hidden without a reading.
    Marker,
    /// Shown only while the control is selected (a ring round a handle).
    Selected,
    /// Shown only while the pointer is over the control (or dragging it).
    Hovered,
    /// A text field's insertion point: moved to the caret while editing,
    /// hidden otherwise.
    Caret,
    /// A text field's selection highlight: sized over the selected text.
    TextSelection,
    /// Shown only while the value's text is empty (a "Name…" hint).
    Placeholder,
    /// A canvas's rubber band: sized to the marquee while one is drawn.
    Marquee,
    /// A menu's highlight: moved onto the active row.
    Highlight,
    /// Its solid fills (and stroke) take the colour of the control's
    /// [`Control::tint`] ramp at its position — a handle at its x.
    Tint,
    /// Shown only while the control reads on (≥ 0.5).
    On,
    /// Shown only while the control reads off.
    Off,
}

/// "Node `node` plays `role`." `node` is a design node id (uuid).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Slot {
    pub role: SlotRole,
    pub node: Uuid,
}

/// Everything behavioral about a design node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Control {
    pub kind: ControlKind,
    pub binding: Option<Binding>,
    /// The second axis of an [`ControlKind::XyPad`].
    pub y_binding: Option<Binding>,
    pub midi: Option<MidiMapping>,
    /// Position (0..1) a reset returns to; `None` = the parameter default.
    pub default: Option<f64>,
    pub slots: Vec<Slot>,
    /// How `Value` slots print the value (decimals, unit, note name, one
    /// label per step); `None` = the parameter's (or stream's) own display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub format: Option<ValueFormat>,
    /// A meter whose source follows a choice param — the 1176's meter
    /// switch showing GR, +4 or +8 output, or nothing. Overrides the
    /// binding's stream while its param selects a tap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub select: Option<SourceSelect>,
    /// The dBFS a level stream must read to sit at the meter scale's 0 — a
    /// VU's operating level ([`VU_REFERENCE_DBFS`]). Level readings are
    /// shifted by it before the scale reads them; gain reduction (already in
    /// dB relative to unity) is not. `None` = the scale is in dBFS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub reference_dbfs: Option<f64>,
    /// Selection, chord clicks and drags, wheel targets, double-click,
    /// context menu, removal ([`Interact`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub interact: Option<Interact>,
    /// The node is drawn (and hit) only while this holds (an analyser layer
    /// behind a toggle; a band's grey disc while it is bypassed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub show_if: Option<Condition>,
    /// Its vertical range follows a choice param (curves, plots, handles'
    /// y axes, a canvas's y).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub zoom: Option<Zoom>,
    /// A colour ramp (`#rrggbb` stops, evenly spaced) its `Tint` slots take
    /// at the control's position.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub tint: Vec<String>,
}

/// 0 VU = −18 dBFS: the digital operating level every VU meter is
/// calibrated to (a sine at −18 dBFS reads 0 VU; +4/+8 ranges offset from it).
pub const VU_REFERENCE_DBFS: f64 = -18.0;

/// One stream a switched meter can read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MeterTap {
    pub stream: EntryId,
    pub channel: Option<u8>,
    /// Added to the stream's dB before the meter reads it: a VU on its
    /// `+4` range reads the level with `offset_db = -4`.
    pub offset_db: f64,
}

impl MeterTap {
    #[must_use]
    pub fn new(stream: impl Into<String>) -> Self {
        Self {
            stream: EntryId::new(stream),
            channel: None,
            offset_db: 0.0,
        }
    }

    #[must_use]
    pub fn channel(mut self, ch: u8) -> Self {
        self.channel = Some(ch);
        self
    }

    #[must_use]
    pub fn offset(mut self, db: f64) -> Self {
        self.offset_db = db;
        self
    }
}

/// A meter's source switched by a choice (or toggle) param: option `i`
/// reads `taps[i]`; `None` (or an option past the end) rests the meter —
/// the switch's OFF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SourceSelect {
    pub param: ParamAddr,
    pub taps: Vec<Option<MeterTap>>,
}

impl SourceSelect {
    #[must_use]
    pub fn new(param: impl Into<String>, taps: Vec<Option<MeterTap>>) -> Self {
        Self {
            param: ParamAddr::new(param),
            taps,
        }
    }

    /// The tap option `index` selects.
    #[must_use]
    pub fn tap(&self, index: usize) -> Option<&MeterTap> {
        self.taps.get(index).and_then(Option::as_ref)
    }
}

impl Control {
    #[must_use]
    pub fn new(kind: ControlKind) -> Self {
        Self {
            kind,
            binding: None,
            y_binding: None,
            midi: None,
            default: None,
            slots: Vec::new(),
            format: None,
            select: None,
            reference_dbfs: None,
            interact: None,
            show_if: None,
            zoom: None,
            tint: Vec::new(),
        }
    }

    #[must_use]
    pub fn interacting(mut self, interact: Interact) -> Self {
        self.interact = Some(interact);
        self
    }

    #[must_use]
    pub fn shown_if(mut self, condition: Condition) -> Self {
        self.show_if = Some(condition);
        self
    }

    /// This control with `map` applied to its serialized form (to remap
    /// node ids) and then every `{n}` resolved to `n`.
    #[must_use]
    pub fn instantiate(&self, n: u32, map: impl Fn(String) -> String) -> Self {
        let Ok(json) = serde_json::to_string(self) else {
            return self.clone();
        };
        let json = map(json).replace(crate::surface::proto::interact::INDEX, &n.to_string());
        serde_json::from_str(&json).unwrap_or_else(|_| self.clone())
    }

    /// Whether any address in it is templated (`{n}`).
    #[must_use]
    pub fn is_templated(&self) -> bool {
        serde_json::to_string(self)
            .is_ok_and(|j| j.contains(crate::surface::proto::interact::INDEX))
    }

    /// Every param this control reads or writes besides its bindings: curve
    /// inputs, handle axes, a repeat's `used`, add rules, menu commands and
    /// checks, interaction rules, its `show_if` and zoom.
    #[must_use]
    pub fn extra_params(&self) -> Vec<&ParamAddr> {
        fn cmd<'a>(c: &'a Command, out: &mut Vec<&'a ParamAddr>) {
            if let Command::Set { addr, .. }
            | Command::Toggle { addr }
            | Command::Cycle { addr }
            | Command::Invert { addr }
            | Command::Reset { addr } = c
            {
                out.push(addr);
            }
        }
        let mut out: Vec<&ParamAddr> = Vec::new();
        match &self.kind {
            ControlKind::Curve { inputs, .. } => {
                for i in inputs {
                    if let InputSource::Param { addr } = &i.source {
                        out.push(addr);
                    }
                }
            }
            ControlKind::Handle { x_map, y_map, .. } => {
                out.extend(x_map.after.iter().chain(&y_map.after))
            }
            ControlKind::Repeat { used: Some(u), .. } => out.push(u),
            ControlKind::Canvas {
                add: Some(rule), ..
            } => {
                out.extend(rule.x.iter().chain(&rule.y));
                out.extend(rule.set.iter().map(|a| &a.addr));
                for z in &rule.zones {
                    out.extend(z.set.iter().map(|a| &a.addr));
                }
            }
            ControlKind::Menu { items, .. } => {
                for item in items {
                    for c in &item.commands {
                        cmd(c, &mut out);
                    }
                    if let Some(c) = &item.checked {
                        out.push(&c.addr);
                    }
                }
            }
            _ => {}
        }
        if let Some(i) = &self.interact {
            for r in &i.wheel {
                out.push(&r.target);
                if let Some(c) = &r.when {
                    out.push(&c.addr);
                }
            }
            for r in &i.click {
                if let ClickAction::Run { commands } = &r.action {
                    for c in commands {
                        cmd(c, &mut out);
                    }
                }
            }
            for c in i
                .double_click
                .iter()
                .chain(&i.press)
                .chain(i.keys.iter().flat_map(|k| &k.commands))
            {
                cmd(c, &mut out);
            }
        }
        if let Some(c) = &self.show_if {
            out.push(&c.addr);
        }
        if let Some(z) = &self.zoom {
            out.push(&z.param);
        }
        out
    }

    #[must_use]
    pub fn bound(mut self, binding: Binding) -> Self {
        self.binding = Some(binding);
        self
    }

    #[must_use]
    pub fn slot(mut self, role: SlotRole, node: Uuid) -> Self {
        self.slots.push(Slot { role, node });
        self
    }

    #[must_use]
    pub fn formatted(mut self, format: ValueFormat) -> Self {
        self.format = Some(format);
        self
    }

    pub fn slots_of(&self, role: SlotRole) -> impl Iterator<Item = Uuid> + '_ {
        self.slots
            .iter()
            .filter(move |s| s.role == role)
            .map(|s| s.node)
    }
}

/// A problem with a control's wiring, relative to a catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum BindingIssue {
    Unbound,
    UnknownParam {
        addr: ParamAddr,
    },
    UnknownAction {
        id: EntryId,
    },
    UnknownStream {
        id: EntryId,
    },
    UnknownCollection {
        id: EntryId,
    },
    /// The source type doesn't suit the control (a meter bound to an action…).
    Mismatch {
        detail: String,
    },
}

impl std::fmt::Display for BindingIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unbound => f.write_str("control is not bound to anything"),
            Self::UnknownParam { addr } => write!(f, "no parameter `{addr}` in the catalog"),
            Self::UnknownAction { id } => write!(f, "no action `{}` in the catalog", id.0),
            Self::UnknownStream { id } => write!(f, "no stream `{}` in the catalog", id.0),
            Self::UnknownCollection { id } => write!(f, "no collection `{}` in the catalog", id.0),
            Self::Mismatch { detail } => f.write_str(detail),
        }
    }
}

impl Control {
    /// Check this control's bindings against `catalog`.
    #[must_use]
    pub fn validate(&self, catalog: &Catalog) -> Vec<BindingIssue> {
        // A template checks as its first item would (`{n}` = 1).
        if self.is_templated() {
            return self.instantiate(1, |s| s).validate(catalog);
        }
        let mut out: Vec<BindingIssue> = Vec::new();
        for issue in self.validate_all(catalog) {
            if !out.contains(&issue) {
                out.push(issue);
            }
        }
        out
    }

    fn validate_all(&self, catalog: &Catalog) -> Vec<BindingIssue> {
        let mut issues = Vec::new();
        // Rules, commands, conditions and zoom name params too.
        let unknown = |addr: &&ParamAddr| catalog.param(addr).is_none();
        for addr in self.extra_params().into_iter().filter(unknown) {
            issues.push(BindingIssue::UnknownParam { addr: addr.clone() });
        }
        if let Some(z) = &self.zoom
            && let Some(p) = catalog.param(&z.param)
            && !matches!(p.kind, ParamKind::Choice { .. })
        {
            issues.push(BindingIssue::Mismatch {
                detail: format!("zoom follows `{}`, which is not a choice", z.param),
            });
        }
        let has_commands = self.interact.as_ref().is_some_and(|i| !i.press.is_empty());
        match &self.kind {
            ControlKind::Repeat { .. }
            | ControlKind::Canvas { .. }
            | ControlKind::Popover { .. }
            | ControlKind::Menu { .. } => return issues,
            ControlKind::TextField { .. } => {
                match self.binding.as_ref().map(|b| &b.source) {
                    Some(Source::Param { addr }) => match catalog.param(addr) {
                        None => issues.push(BindingIssue::UnknownParam { addr: addr.clone() }),
                        Some(p) if p.kind != ParamKind::Text => {
                            issues.push(BindingIssue::Mismatch {
                                detail: format!(
                                    "text field bound to `{addr}`, which is not a text param"
                                ),
                            });
                        }
                        Some(_) => {}
                    },
                    Some(_) => issues.push(BindingIssue::Mismatch {
                        detail: "a text field edits a text param".into(),
                    }),
                    None => issues.push(BindingIssue::Unbound),
                }
                return issues;
            }
            ControlKind::Button { .. } if self.binding.is_none() && has_commands => return issues,
            _ => {}
        }
        // Curves and handles carry their own bindings.
        match &self.kind {
            ControlKind::Curve { inputs, .. } => {
                for input in inputs {
                    match &input.source {
                        InputSource::Param { addr } if catalog.param(addr).is_none() => {
                            issues.push(BindingIssue::UnknownParam { addr: addr.clone() });
                        }
                        InputSource::Stream { id, .. } if catalog.stream(id).is_none() => {
                            issues.push(BindingIssue::UnknownStream { id: id.clone() });
                        }
                        _ => {}
                    }
                }
                return issues;
            }
            ControlKind::Handle {
                x,
                y,
                wheel,
                x_map,
                y_map,
            } => {
                let bound: Vec<&Binding> = [x, y, wheel].into_iter().flatten().collect();
                if bound.is_empty() {
                    issues.push(BindingIssue::Unbound);
                }
                for b in bound {
                    match &b.source {
                        Source::Param { addr } if catalog.param(addr).is_none() => {
                            issues.push(BindingIssue::UnknownParam { addr: addr.clone() });
                        }
                        Source::Param { .. } => {}
                        _ => issues.push(BindingIssue::Mismatch {
                            detail: "a handle drags params; bind its axes to params".into(),
                        }),
                    }
                }
                for addr in x_map.after.iter().chain(&y_map.after) {
                    if catalog.param(addr).is_none() {
                        issues.push(BindingIssue::UnknownParam { addr: addr.clone() });
                    }
                }
                return issues;
            }
            _ => {}
        }
        let Some(b) = &self.binding else {
            issues.push(BindingIssue::Unbound);
            return issues;
        };
        let mut check = |b: &Binding| match &b.source {
            Source::Param { addr } => match catalog.param(addr) {
                None => issues.push(BindingIssue::UnknownParam { addr: addr.clone() }),
                Some(p) => {
                    if matches!(
                        self.kind,
                        ControlKind::Meter { .. } | ControlKind::Plot { .. }
                    ) {
                        issues.push(BindingIssue::Mismatch {
                            detail: format!(
                                "{} bound to parameter `{addr}`; it displays streams",
                                self.kind.name()
                            ),
                        });
                    }
                    if matches!(self.kind, ControlKind::Selector)
                        && !matches!(p.kind, ParamKind::Choice { .. } | ParamKind::Toggle)
                    {
                        issues.push(BindingIssue::Mismatch {
                            detail: format!("selector bound to continuous parameter `{addr}`"),
                        });
                    }
                }
            },
            Source::Action { id } => {
                if catalog.action(id).is_none() {
                    issues.push(BindingIssue::UnknownAction { id: id.clone() });
                } else if !matches!(
                    self.kind,
                    ControlKind::Button { .. } | ControlKind::Footswitch { .. }
                ) {
                    issues.push(BindingIssue::Mismatch {
                        detail: format!(
                            "{} bound to action `{}`; only buttons and footswitches fire actions",
                            self.kind.name(),
                            id.0
                        ),
                    });
                }
            }
            Source::Stream { id, .. } => match catalog.stream(id) {
                None => issues.push(BindingIssue::UnknownStream { id: id.clone() }),
                Some(s) => {
                    if self.kind.is_input() {
                        issues.push(BindingIssue::Mismatch {
                            detail: format!(
                                "{} bound to read-only stream `{}`",
                                self.kind.name(),
                                id.0
                            ),
                        });
                    }
                    if matches!(self.kind, ControlKind::Meter { .. })
                        && matches!(s.kind, StreamKind::Spectrum { .. } | StreamKind::Tuner)
                    {
                        issues.push(BindingIssue::Mismatch {
                            detail: format!("meter bound to non-level stream `{}`", id.0),
                        });
                    }
                    if matches!(self.kind, ControlKind::Plot { .. })
                        && matches!(s.kind, StreamKind::Tuner | StreamKind::Envelope { .. })
                    {
                        issues.push(BindingIssue::Mismatch {
                            detail: format!("plot bound to `{}`, which is not a series or a reading (bind a spectrum, history, waveform, level or scalar)", id.0),
                        });
                    }
                }
            },
            Source::Collection { id } => {
                if catalog.collection(id).is_none() {
                    issues.push(BindingIssue::UnknownCollection { id: id.clone() });
                }
            }
        };
        check(b);
        if let Some(y) = &self.y_binding {
            check(y);
        }
        if let Some(sel) = &self.select {
            match catalog.param(&sel.param) {
                None => issues.push(BindingIssue::UnknownParam {
                    addr: sel.param.clone(),
                }),
                Some(p) if !matches!(p.kind, ParamKind::Choice { .. } | ParamKind::Toggle) => {
                    issues.push(BindingIssue::Mismatch {
                        detail: format!(
                            "meter source switched by continuous parameter `{}`",
                            sel.param
                        ),
                    });
                }
                Some(_) => {}
            }
            for tap in sel.taps.iter().flatten() {
                if catalog.stream(&tap.stream).is_none() {
                    issues.push(BindingIssue::UnknownStream {
                        id: tap.stream.clone(),
                    });
                }
            }
        }
        issues
    }
}
