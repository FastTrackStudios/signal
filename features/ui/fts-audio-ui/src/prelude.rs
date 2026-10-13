//! Convenience re-exports — `use fts_audio_ui::prelude::*`.

pub use crate::axis::{DbAxis, FreqAxis};
pub use crate::controls::ValueReadout;
pub use crate::controls::{
    Dropdown, HSlider, Knob, KnobSize, ModRangeInput, Ramp, RampDirection, RangeKnob, RawKnob,
    Segmented, Toggle, VSlider, XYPad, XYValue,
};
pub use crate::drag::{begin_drag, begin_drag_axis, DragAxis, DragProvider, DragState};
pub use crate::gesture::{self, Press};
pub use crate::marks::{TextMark, TextMarkGroup, TickMark, TickMarkGroup, TickTier};
pub use crate::meters::{
    GrMeter, LevelMeter, LevelMeterDb, LevelMeterOrientation, SpectrumAnalyzer,
};
pub use crate::param::ParamHandle;
pub use crate::shell::{
    EditorForm, FloatingPanel, PluginShell, RailButton, ShellItem, EDITOR_FORMS, RAIL_W,
};
pub use crate::theme::{use_init_theme, use_theme, Theme, ThemeProvider, ThemeVariant};
