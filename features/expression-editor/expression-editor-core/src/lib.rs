//! `expression-editor-core` — the portable engine behind the
//! expression editor.
//!
//! One editor, two products:
//!
//! - **an MPE editor**, where per-note pitch bend, channel pressure,
//!   and CC74 are edited as properties *of a note* instead of in raw
//!   controller lanes;
//! - **a Melodyne competitor**, where an analyzed audio clip's notes
//!   carry a tracked f0 contour that edits by exactly the same
//!   gestures.
//!
//! They unify because a note in either domain is the same thing: an
//! integer pitch row plus a continuous curve measured in semitones
//! relative to it. See [`doc`] for why that framing is the whole trick,
//! and [`blob`] for the center/drift/vibrato decomposition that makes
//! Melodyne's two headline sliders work on hand-drawn MIDI too.
//!
//! This crate holds no UI, no DSP, and no DAW. It is deliberately
//! portable so it compiles for wasm, Blitz-native, and embedded
//! builds alike, and so the interaction model can be tested headlessly
//! — the split MPElodyne got right, kept.
//!
//! The Dioxus surface lives in `expression-editor-ui`; domain adapters
//! (MIDI takes, `tune_dsp::PitchDoc`) live with their domains.

pub mod actions;
pub mod blob;
pub mod camera;
pub mod cc;
pub mod chord;
pub mod clipboard;
pub mod cursor;
pub mod doc;
pub mod draft;
pub mod drum;
pub mod edit;
pub mod fills;
pub mod flam;
pub mod handles;
pub mod harmony;
pub mod kit;
pub mod memagic;
pub mod menu;
pub mod mode;
pub mod modulation;
pub mod mouse;
pub mod multitool;
pub mod razor;
pub mod reference;
pub mod rows;
pub mod shape;
pub mod timing;
pub mod tools;
pub mod tracks;
pub mod tuning;
pub mod zoom;

pub use camera::{Bounds, Camera, Content, VerticalCamera, Viewport};
pub use cc::{CcDisplay, CcLane, CcSet};
pub use chord::Chord;
pub use cursor::{Aim, Cursor};
pub use doc::{
    Curve, CurveShape, Dimension, ExpressionDoc, Marker, Note, NoteId, Point, Target, TimeBase,
};
pub use draft::PitchDraft;
pub use edit::{Edit, History};
pub use handles::{Handle, Scope};
pub use mode::{Mode, ModeFamily};
pub use mouse::{Action, MouseMap};
pub use multitool::{Bend, Steepness, Zone};
pub use razor::{RazorArea, RazorSet};
pub use reference::{MidiReference, RefNote, SnapSource};
pub use rows::{Articulation, DrumMap, NoteShape, RowSpace, StringTuning};
pub use shape::Shape;
pub use timing::{Separator, StretchLaw};
pub use tools::{Grid, Hit, Mods, Selection, Tool};
pub use tracks::{RefColor, Track, Workspace};
pub use tuning::{Temperament, Tuning};
pub use zoom::{HorizontalMode, SmartZoom, VerticalMode, ZoomModes};

mod editor;
pub use editor::{CUSHION, Editor, PAD, StripLane, content_of};
