//! fts-audio-proto — RT-safe instruction set for the fts-audio VST plugin.
//!
//! This crate defines the rule tables that integrated extensions upload and the
//! fts-audio VST executes on REAPER's audio thread. All types are flat,
//! fixed-size, and `Copy` — no heap allocation ever touches the audio thread.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐      rule table       ┌─────────────────┐
//! │  Integrated     │ ──────────────────▶  │  fts-audio VST   │
//! │  extension      │   lock-free swap      │  (audio thread)  │
//! │                 │                       │                  │
//! └─────────────────┘                       └─────────────────┘
//! ```
//!
//! Guests write rules to the "back" buffer, then atomically swap the active
//! buffer index. The VST reads from the "front" buffer each audio block.
//!
//! # Rule Types
//!
//! 1. **FX Parameter Mappings** — macro param → target FX param with transform
//! 2. **MIDI Routing Rules** — source event → target (FX param, MIDI out, action)
//! 3. **Automation Schedules** — timestamped parameter values consumed sample-accurately
//! 4. **Sample Triggers** — position-based triggers for click/count-in/guide
//! 5. **MIDI Filter Masks** — which MIDI events to intercept/delete
//! 6. **Event Subscriptions** — request specific events written to guest ring buffer

#![no_std]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ============================================================================
// Capacity Constants
// ============================================================================

/// Maximum FX parameter mappings per rule table.
pub const MAX_FX_MAPPINGS: usize = 256;

/// Maximum MIDI routing rules per rule table.
pub const MAX_MIDI_ROUTES: usize = 128;

/// Maximum scheduled automation events per rule table.
pub const MAX_AUTOMATION_EVENTS: usize = 1024;

/// Maximum sample trigger slots per rule table.
pub const MAX_SAMPLE_TRIGGERS: usize = 32;

/// Maximum MIDI filter entries per rule table.
pub const MAX_MIDI_FILTERS: usize = 64;

/// Maximum event subscription entries per rule table.
pub const MAX_EVENT_SUBSCRIPTIONS: usize = 32;

/// Number of macro parameters (matches fts-macros).
pub const NUM_MACROS: usize = 8;

// ============================================================================
// FX Parameter Mapping Rules
// ============================================================================

/// How to transform a source value before applying to the target parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum MapMode {
    /// Direct passthrough: source 0.0–1.0 → target 0.0–1.0.
    PassThrough = 0,
    /// Scale to range: source 0.0–1.0 → target [min..max].
    ScaleRange = 1,
    /// Relative increment: each change moves target by step.
    Relative = 2,
    /// Toggle: source < 0.5 → 0.0, source ≥ 0.5 → 1.0.
    Toggle = 3,
}

/// A single FX parameter mapping rule.
///
/// Maps a source (macro index or MIDI CC) to a target FX parameter
/// on a specific track, with an optional value transformation.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FxMapping {
    /// Source macro parameter index (0–7) or MIDI CC (0–127).
    pub source_index: u8,
    /// Source type discriminant.
    pub source_kind: SourceKind,
    /// Transformation mode.
    pub mode: MapMode,
    _pad: u8,
    /// Target track index (resolved by guest before upload).
    pub target_track_idx: u32,
    /// Target FX index in the track's FX chain.
    pub target_fx_idx: u32,
    /// Target parameter index within the FX.
    pub target_param_idx: u32,
    /// Mode-specific parameter: ScaleRange min, Relative step.
    pub mode_param_a: f32,
    /// Mode-specific parameter: ScaleRange max.
    pub mode_param_b: f32,
}

/// What drives an FX parameter mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum SourceKind {
    /// Driven by a macro parameter (0–7).
    Macro = 0,
    /// Driven by a MIDI CC value on any channel.
    MidiCc = 1,
    /// Driven by MIDI pitch bend.
    MidiPitchBend = 2,
    /// Driven by MIDI channel pressure (aftertouch).
    MidiChannelPressure = 3,
}

impl FxMapping {
    /// Create a new macro → FX parameter mapping.
    pub const fn macro_passthrough(
        source_macro: u8,
        track_idx: u32,
        fx_idx: u32,
        param_idx: u32,
    ) -> Self {
        Self {
            source_index: source_macro,
            source_kind: SourceKind::Macro,
            mode: MapMode::PassThrough,
            _pad: 0,
            target_track_idx: track_idx,
            target_fx_idx: fx_idx,
            target_param_idx: param_idx,
            mode_param_a: 0.0,
            mode_param_b: 1.0,
        }
    }

    /// Create a scale-range mapping.
    pub const fn macro_scale_range(
        source_macro: u8,
        track_idx: u32,
        fx_idx: u32,
        param_idx: u32,
        min: f32,
        max: f32,
    ) -> Self {
        Self {
            source_index: source_macro,
            source_kind: SourceKind::Macro,
            mode: MapMode::ScaleRange,
            _pad: 0,
            target_track_idx: track_idx,
            target_fx_idx: fx_idx,
            target_param_idx: param_idx,
            mode_param_a: min,
            mode_param_b: max,
        }
    }

    /// Apply the mode transformation to a source value.
    pub fn apply(&self, source_value: f32) -> f32 {
        let v = clamp(source_value, 0.0, 1.0);
        match self.mode {
            MapMode::PassThrough => v,
            MapMode::ScaleRange => self.mode_param_a + v * (self.mode_param_b - self.mode_param_a),
            MapMode::Relative => v * abs_f32(self.mode_param_a),
            MapMode::Toggle => {
                if v >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

// ============================================================================
// MIDI Routing Rules
// ============================================================================

/// What a MIDI routing rule targets.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum MidiRouteTarget {
    /// Route to an FX parameter (track_idx, fx_idx, param_idx stored in target fields).
    FxParam = 0,
    /// Route to a MIDI output channel/CC.
    MidiOut = 1,
    /// Trigger an action by command ID.
    Action = 2,
}

/// Source filter for a MIDI routing rule.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MidiSourceFilter {
    /// MIDI channel (0–15, or 255 for any).
    pub channel: u8,
    /// MIDI status type.
    pub msg_type: MidiMsgType,
    /// CC number / note number (meaning depends on msg_type). 255 = any.
    pub data1: u8,
    _pad: u8,
}

/// MIDI message type for filtering.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum MidiMsgType {
    NoteOn = 0,
    NoteOff = 1,
    ControlChange = 2,
    PitchBend = 3,
    ChannelPressure = 4,
    PolyPressure = 5,
    ProgramChange = 6,
    Any = 255,
}

/// A single MIDI routing rule.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MidiRoute {
    /// Source filter — which MIDI events match this rule.
    pub source: MidiSourceFilter,
    /// Target type.
    pub target_kind: MidiRouteTarget,
    _pad: [u8; 3],
    /// Target track index (for FxParam target).
    pub target_track_idx: u32,
    /// Target FX index (for FxParam) or output channel (for MidiOut).
    pub target_fx_idx: u32,
    /// Target param index (for FxParam) or CC number (for MidiOut) or command ID (for Action).
    pub target_param_idx: u32,
    /// Value curve/range transform: min.
    pub curve_min: f32,
    /// Value curve/range transform: max.
    pub curve_max: f32,
}

// ============================================================================
// Automation Schedules
// ============================================================================

/// A single timestamped parameter value for sample-accurate automation.
///
/// Consumed in order by the VST; once the transport passes `sample_position`,
/// the value is applied and the slot is marked consumed.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct AutomationEvent {
    /// Absolute sample position in the project timeline.
    pub sample_position: u64,
    /// Target track index.
    pub target_track_idx: u32,
    /// Target FX index.
    pub target_fx_idx: u32,
    /// Target parameter index.
    pub target_param_idx: u32,
    /// Normalized parameter value (0.0–1.0).
    pub value: f32,
    /// Flags (reserved).
    pub flags: u32,
    _pad: u32,
}

// ============================================================================
// Sample Triggers
// ============================================================================

/// A sample playback trigger for click/count-in/guide.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SampleTrigger {
    /// Sample position at which to start playback.
    pub trigger_position: u64,
    /// Index into a pre-loaded sample bank.
    pub sample_bank_slot: u16,
    /// Output channel pair (0 = ch 1-2, 1 = ch 3-4, etc.).
    pub output_pair: u8,
    /// Flags: bit 0 = one-shot, bit 1 = looping.
    pub flags: u8,
    /// Gain applied to this trigger (0.0–1.0).
    pub gain: f32,
}

// ============================================================================
// MIDI Filter Masks
// ============================================================================

/// A MIDI filter entry: events matching this pattern are intercepted (eaten)
/// by fts-audio and NOT passed to REAPER's downstream FX chain.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MidiFilter {
    /// Channel (0–15, or 255 for any).
    pub channel: u8,
    /// Message type to filter.
    pub msg_type: MidiMsgType,
    /// Data1 filter (CC#, note#, or 255 for any).
    pub data1: u8,
    /// Flags: bit 0 = active.
    pub flags: u8,
}

// ============================================================================
// Event Subscriptions
// ============================================================================

/// An event subscription: tells fts-audio to write matching events to the
/// guest's ring buffer for async consumption.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct EventSubscription {
    /// Channel (0–15, or 255 for any).
    pub channel: u8,
    /// Message type to subscribe to.
    pub msg_type: MidiMsgType,
    /// Data1 filter (CC#, note#, or 255 for any).
    pub data1: u8,
    /// Flags: bit 0 = active, bit 1 = include velocity/value.
    pub flags: u8,
}

// ============================================================================
// Rule Table (Double-Buffered)
// ============================================================================

/// A single rule table buffer. Guests write here; the VST reads.
///
/// All arrays are pre-allocated at their maximum capacity.
/// The `_count` fields indicate how many entries are active.
#[repr(C)]
pub struct RuleBuffer {
    /// Active FX parameter mappings.
    pub fx_mappings: [FxMapping; MAX_FX_MAPPINGS],
    pub fx_mapping_count: u32,

    /// Active MIDI routing rules.
    pub midi_routes: [MidiRoute; MAX_MIDI_ROUTES],
    pub midi_route_count: u32,

    /// Scheduled automation events (consumed in order).
    pub automation_events: [AutomationEvent; MAX_AUTOMATION_EVENTS],
    pub automation_event_count: u32,
    /// Index of the next unconsumed automation event.
    pub automation_cursor: u32,

    /// Sample triggers.
    pub sample_triggers: [SampleTrigger; MAX_SAMPLE_TRIGGERS],
    pub sample_trigger_count: u32,

    /// MIDI filter masks.
    pub midi_filters: [MidiFilter; MAX_MIDI_FILTERS],
    pub midi_filter_count: u32,

    /// Event subscriptions.
    pub event_subscriptions: [EventSubscription; MAX_EVENT_SUBSCRIPTIONS],
    pub event_subscription_count: u32,

    _pad: u32,
}

/// Double-buffered rule table for lock-free guest → VST communication.
///
/// Guests write to the back buffer, then atomically swap the active index.
/// The VST always reads from the front buffer.
#[repr(C)]
pub struct RuleTable {
    /// Two rule buffers for double-buffering.
    pub buffers: [RuleBuffer; 2],
    /// Index of the currently active (front) buffer (0 or 1).
    /// The VST reads from `buffers[active_index]`.
    /// Guests write to `buffers[1 - active_index]` then swap.
    pub active_index: AtomicU32,
    /// Monotonically increasing sequence number. Guests increment after each swap.
    /// The VST uses this for change detection (skip re-processing if unchanged).
    pub sequence: AtomicU64,
}

impl RuleTable {
    /// Get the index of the front (active, read-only for VST) buffer.
    pub fn front_index(&self) -> usize {
        (self.active_index.load(Ordering::Acquire) & 1) as usize
    }

    /// Get a reference to the front buffer (for the VST to read).
    pub fn front(&self) -> &RuleBuffer {
        &self.buffers[self.front_index()]
    }

    /// Get the index of the back (writable for guest) buffer.
    pub fn back_index(&self) -> usize {
        1 - self.front_index()
    }

    /// Get a mutable reference to the back buffer (for guests to write).
    ///
    /// Get a mutable pointer to the back buffer (for guests to write).
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive write access to the back buffer.
    /// Only one guest should write at a time. The returned pointer is valid
    /// for the lifetime of the RuleTable.
    pub fn back_ptr(&self) -> *mut RuleBuffer {
        let idx = self.back_index();
        // Use addr_of! to avoid creating an intermediate reference
        core::ptr::addr_of!(self.buffers[idx]) as *mut RuleBuffer
    }

    /// Swap front and back buffers. Call after finishing writes to the back buffer.
    ///
    /// Increments the sequence number for change detection.
    pub fn swap(&self) {
        let old = self.active_index.load(Ordering::Acquire);
        self.active_index.store(1 - (old & 1), Ordering::Release);
        self.sequence.fetch_add(1, Ordering::Release);
    }

    /// Current sequence number (for change detection by the VST).
    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }
}

// ============================================================================
// no_std helpers
// ============================================================================

fn clamp(v: f32, min: f32, max: f32) -> f32 {
    if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

fn abs_f32(v: f32) -> f32 {
    if v < 0.0 { -v } else { v }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fx_mapping_passthrough() {
        let m = FxMapping::macro_passthrough(0, 1, 2, 3);
        assert_eq!(m.apply(0.0), 0.0);
        assert_eq!(m.apply(0.5), 0.5);
        assert_eq!(m.apply(1.0), 1.0);
    }

    #[test]
    fn fx_mapping_scale_range() {
        let m = FxMapping::macro_scale_range(0, 1, 2, 3, 0.2, 0.8);
        assert!((m.apply(0.0) - 0.2).abs() < 1e-6);
        assert!((m.apply(0.5) - 0.5).abs() < 1e-6);
        assert!((m.apply(1.0) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn fx_mapping_toggle() {
        let m = FxMapping {
            mode: MapMode::Toggle,
            ..FxMapping::macro_passthrough(0, 0, 0, 0)
        };
        assert_eq!(m.apply(0.0), 0.0);
        assert_eq!(m.apply(0.49), 0.0);
        assert_eq!(m.apply(0.5), 1.0);
        assert_eq!(m.apply(1.0), 1.0);
    }

    #[test]
    fn fx_mapping_clamps_input() {
        let m = FxMapping::macro_passthrough(0, 0, 0, 0);
        assert_eq!(m.apply(-0.5), 0.0);
        assert_eq!(m.apply(1.5), 1.0);
    }

    #[test]
    fn rule_buffer_is_copy_friendly() {
        // Ensure all rule types are Copy (required for fixed-size arrays)
        fn assert_copy<T: Copy>() {}
        assert_copy::<FxMapping>();
        assert_copy::<MidiRoute>();
        assert_copy::<AutomationEvent>();
        assert_copy::<SampleTrigger>();
        assert_copy::<MidiFilter>();
        assert_copy::<EventSubscription>();
    }

    #[test]
    fn source_kind_variants() {
        assert_eq!(SourceKind::Macro as u8, 0);
        assert_eq!(SourceKind::MidiCc as u8, 1);
        assert_eq!(SourceKind::MidiPitchBend as u8, 2);
    }

    #[test]
    fn map_mode_variants() {
        assert_eq!(MapMode::PassThrough as u8, 0);
        assert_eq!(MapMode::ScaleRange as u8, 1);
        assert_eq!(MapMode::Relative as u8, 2);
        assert_eq!(MapMode::Toggle as u8, 3);
    }

    #[test]
    fn midi_route_target_variants() {
        assert_eq!(MidiRouteTarget::FxParam as u8, 0);
        assert_eq!(MidiRouteTarget::MidiOut as u8, 1);
        assert_eq!(MidiRouteTarget::Action as u8, 2);
    }
}
