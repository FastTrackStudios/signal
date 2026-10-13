//! Click track, count-in, and section guide types.
//!
//! This module defines the data types used by the guide generation system:
//! - MIDI note mappings for click, count, and section cue events
//! - Configuration for click, count-in, and guide behavior
//! - Event types produced by the scheduling algorithms
//! - Count-in state for real-time playback tracking

pub mod config;
pub mod count_in;
pub mod event;
pub mod midi_map;

pub use config::{ClickConfig, CountInConfig, GuideConfig};
pub use count_in::CountInState;
pub use event::{ClickEvent, ClickType, CountEvent, GuideEvent, SectionCueEvent};
