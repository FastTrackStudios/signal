//! Fluent builder API for constructing REAPER projects programmatically.
//!
//! Provides a convenient, composable API for building RPP structures
//! without manually constructing deeply nested types. Designed for
//! test fixtures, project generation, and programmatic DAW workflows.
//!
//! # Example
//!
//! ```
//! use dawfile_reaper::builder::ReaperProjectBuilder;
//! use dawfile_reaper::RppSerialize;
//!
//! let project = ReaperProjectBuilder::new()
//!     .tempo(120.0)
//!     .sample_rate(48000)
//!     .track("Drums", |t| t
//!         .color(0x112233)
//!         .folder_start()
//!     )
//!     .track("Kick", |t| t
//!         .item(0.0, 4.0, |i| i
//!             .name("Kick Pattern")
//!             .source_wave("kick.wav")
//!         )
//!     )
//!     .track("Snare", |t| t
//!         .item(0.0, 4.0, |i| i.source_wave("snare.wav"))
//!         .folder_end(1)
//!     )
//!     .build();
//!
//! let rpp_text = project.to_rpp_string();
//! assert!(rpp_text.contains("Drums"));
//! ```

use crate::stock_fx::StockFx;
use crate::types::envelope::{Envelope, EnvelopePoint, EnvelopePointShape};
use crate::types::fx_chain::{FxChain, FxChainNode, FxPlugin, PluginType};
use crate::types::item::{
    ChannelMode, FadeCurveType, FadeSettings, Item, MidiEvent, MidiSource, MidiSourceEvent,
    PitchMode, PlayRateSettings, SourceBlock, SourceType, StretchMarker, Take,
};
use crate::types::marker_region::{MarkerRegion, MarkerRegionCollection};
use crate::types::project::{ProjectProperties, ReaperProject};
use crate::types::time_tempo::{TempoTimeEnvelope, TempoTimePoint};
use crate::types::track::{
    FolderSettings, FolderState, MuteSoloSettings, RecordSettings, Track, TrackSoloState,
    VolPanSettings,
};

// ===========================================================================
// FxBuilder
// ===========================================================================

/// Builder for constructing FX plugins within an FX chain.
pub struct FxBuilder {
    name: String,
    plugin_type: PluginType,
    file: String,
    bypassed: bool,
    offline: bool,
    fxid: Option<String>,
    preset_name: Option<String>,
    state_data: Vec<String>,
    raw_block: String,
    wak: Option<[i32; 2]>,
    float_pos: Option<[i32; 4]>,
}

impl FxBuilder {
    fn new(name: impl Into<String>, plugin_type: PluginType, file: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            plugin_type,
            file: file.into(),
            bypassed: false,
            offline: false,
            fxid: None,
            preset_name: None,
            state_data: vec![],
            raw_block: String::new(),
            wak: None,
            float_pos: None,
        }
    }

    /// Set the `WAK` line values (REAPER's wet/auto-bypass flags). REAPER
    /// writes `WAK 0 0` for a normal active FX.
    pub fn wak(mut self, a: i32, b: i32) -> Self {
        self.wak = Some([a, b]);
        self
    }

    /// Set the `FLOATPOS` rectangle (floating FX window position).
    pub fn float_pos(mut self, pos: [i32; 4]) -> Self {
        self.float_pos = Some(pos);
        self
    }

    /// Set the complete raw plugin block (`<VST …> … >`), used for verbatim
    /// state transplant (e.g. a converted Omnisphere chunk). When set, it is
    /// emitted as-is by the serializer instead of a synthesized header.
    pub fn raw_block(mut self, block: impl Into<String>) -> Self {
        self.raw_block = block.into();
        self
    }

    /// Mark this FX as bypassed.
    pub fn bypassed(mut self) -> Self {
        self.bypassed = true;
        self
    }

    /// Mark this FX as offline.
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Set the FX GUID.
    pub fn fxid(mut self, guid: impl Into<String>) -> Self {
        self.fxid = Some(guid.into());
        self
    }

    /// Set the preset name.
    pub fn preset(mut self, name: impl Into<String>) -> Self {
        self.preset_name = Some(name.into());
        self
    }

    /// Add base64-encoded state data lines.
    pub fn state(mut self, data: impl Into<String>) -> Self {
        self.state_data.push(data.into());
        self
    }

    fn build(self) -> FxPlugin {
        FxPlugin {
            name: self.name,
            custom_name: None,
            plugin_type: self.plugin_type,
            file: self.file,
            bypassed: self.bypassed,
            offline: self.offline,
            fxid: self.fxid,
            preset_name: self.preset_name,
            float_pos: self.float_pos,
            wak: self.wak,
            parallel: false,
            state_data: self.state_data,
            raw_block: self.raw_block,
            param_envelopes: vec![],
            params_on_tcp: vec![],
            header_extra: None,
        }
    }
}

// ===========================================================================
// MidiSourceBuilder
// ===========================================================================

/// Builder for constructing MIDI source data within a take.
///
/// Generates REAPER's internal MIDI format: hex-encoded `E` lines with
/// delta-tick timing. No external MIDI crate needed — REAPER uses its own
/// simple text-based encoding.
///
/// # Example
///
/// ```
/// use dawfile_reaper::builder::MidiSourceBuilder;
///
/// let midi = MidiSourceBuilder::new()
///     .ticks_per_qn(960)
///     .note(0, 0, 60, 96, 480)   // C4, velocity 96, half note
///     .note(0, 0, 64, 96, 480)   // E4 at same time
///     .cc(0, 0, 1, 64)           // Mod wheel to 64
///     .build();
///
/// assert_eq!(midi.ticks_per_qn, 960);
/// assert!(!midi.events.is_empty());
/// ```
pub struct MidiSourceBuilder {
    ticks_per_qn: u32,
    /// Events stored as (absolute_tick, MidiEvent) for sorting before build.
    events: Vec<(u64, MidiEvent)>,
    /// Deferred note-offs: (absolute_tick_off, channel, note, velocity)
    pending_note_offs: Vec<(u64, u8, u8, u8)>,
    /// Current absolute tick position for sequential event building.
    cursor: u64,
}

impl MidiSourceBuilder {
    /// Create a new MIDI source builder with default 960 ticks per quarter note.
    pub fn new() -> Self {
        Self {
            ticks_per_qn: 960,
            events: vec![],
            pending_note_offs: vec![],
            cursor: 0,
        }
    }

    /// Set ticks per quarter note (default 960).
    pub fn ticks_per_qn(mut self, tpq: u32) -> Self {
        self.ticks_per_qn = tpq;
        self
    }

    /// Add a complete note (note-on + note-off pair) at the current cursor.
    ///
    /// - `delta_ticks`: offset from current cursor position
    /// - `channel`: MIDI channel (0-15)
    /// - `note`: MIDI note number (0-127)
    /// - `velocity`: Note-on velocity (1-127)
    /// - `duration_ticks`: Length of the note in ticks
    pub fn note(
        mut self,
        delta_ticks: u32,
        channel: u8,
        note: u8,
        velocity: u8,
        duration_ticks: u32,
    ) -> Self {
        let abs_tick = self.cursor + delta_ticks as u64;
        self.events.push((
            abs_tick,
            MidiEvent {
                delta_ticks: 0, // will be computed in build()
                bytes: vec![0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
            },
        ));
        self.pending_note_offs
            .push((abs_tick + duration_ticks as u64, channel, note, 0));
        self.cursor = abs_tick;
        self
    }

    /// Add a raw note-on event.
    pub fn note_on(mut self, delta_ticks: u32, channel: u8, note: u8, velocity: u8) -> Self {
        let abs_tick = self.cursor + delta_ticks as u64;
        self.events.push((
            abs_tick,
            MidiEvent {
                delta_ticks: 0,
                bytes: vec![0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
            },
        ));
        self.cursor = abs_tick;
        self
    }

    /// Add a raw note-off event.
    pub fn note_off(mut self, delta_ticks: u32, channel: u8, note: u8, velocity: u8) -> Self {
        let abs_tick = self.cursor + delta_ticks as u64;
        self.events.push((
            abs_tick,
            MidiEvent {
                delta_ticks: 0,
                bytes: vec![0x80 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
            },
        ));
        self.cursor = abs_tick;
        self
    }

    /// Add a Control Change event.
    pub fn cc(mut self, delta_ticks: u32, channel: u8, controller: u8, value: u8) -> Self {
        let abs_tick = self.cursor + delta_ticks as u64;
        self.events.push((
            abs_tick,
            MidiEvent {
                delta_ticks: 0,
                bytes: vec![0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F],
            },
        ));
        self.cursor = abs_tick;
        self
    }

    /// Add a Program Change event.
    pub fn program_change(mut self, delta_ticks: u32, channel: u8, program: u8) -> Self {
        let abs_tick = self.cursor + delta_ticks as u64;
        self.events.push((
            abs_tick,
            MidiEvent {
                delta_ticks: 0,
                bytes: vec![0xC0 | (channel & 0x0F), program & 0x7F],
            },
        ));
        self.cursor = abs_tick;
        self
    }

    /// Add a Pitch Bend event. `value` is 14-bit (0-16383, 8192 = center).
    pub fn pitch_bend(mut self, delta_ticks: u32, channel: u8, value: u16) -> Self {
        let abs_tick = self.cursor + delta_ticks as u64;
        let lsb = (value & 0x7F) as u8;
        let msb = ((value >> 7) & 0x7F) as u8;
        self.events.push((
            abs_tick,
            MidiEvent {
                delta_ticks: 0,
                bytes: vec![0xE0 | (channel & 0x0F), lsb, msb],
            },
        ));
        self.cursor = abs_tick;
        self
    }

    /// Add a Channel Pressure (aftertouch) event.
    pub fn channel_pressure(mut self, delta_ticks: u32, channel: u8, pressure: u8) -> Self {
        let abs_tick = self.cursor + delta_ticks as u64;
        self.events.push((
            abs_tick,
            MidiEvent {
                delta_ticks: 0,
                bytes: vec![0xD0 | (channel & 0x0F), pressure & 0x7F],
            },
        ));
        self.cursor = abs_tick;
        self
    }

    /// Add a Polyphonic Aftertouch event.
    pub fn aftertouch(mut self, delta_ticks: u32, channel: u8, note: u8, pressure: u8) -> Self {
        let abs_tick = self.cursor + delta_ticks as u64;
        self.events.push((
            abs_tick,
            MidiEvent {
                delta_ticks: 0,
                bytes: vec![0xA0 | (channel & 0x0F), note & 0x7F, pressure & 0x7F],
            },
        ));
        self.cursor = abs_tick;
        self
    }

    /// Move the cursor to an absolute tick position.
    pub fn at(mut self, absolute_tick: u64) -> Self {
        self.cursor = absolute_tick;
        self
    }

    /// Advance the cursor by `ticks` without adding an event.
    pub fn advance(mut self, ticks: u32) -> Self {
        self.cursor += ticks as u64;
        self
    }

    /// Build the `MidiSource`, sorting events by absolute tick and computing deltas.
    pub fn build(mut self) -> MidiSource {
        // Merge pending note-offs into events
        for (abs_tick, ch, note, vel) in self.pending_note_offs.drain(..) {
            self.events.push((
                abs_tick,
                MidiEvent {
                    delta_ticks: 0,
                    bytes: vec![0x80 | (ch & 0x0F), note & 0x7F, vel & 0x7F],
                },
            ));
        }

        // By absolute tick, and at equal ticks note-offs BEFORE
        // note-ons.
        //
        // That second rule is not cosmetic. A reader pairs a note-on to
        // the next note-off of the same pitch, so a chord held exactly
        // to the bar line where the next one begins emits
        //
        //     on  C @ 3840   (the next chord)
        //     off C @ 3840   (the one that just ended)
        //
        // and the reader hands the SECOND chord the FIRST one's
        // note-off and calls it zero length — from MIDI that is correct
        // in every other respect. Offs first makes the pairing
        // unambiguous, and it is what every DAW writes. (daw#28.)
        //
        // Stable, so events that are neither keep the order they were
        // added in.
        self.events
            .sort_by_key(|(tick, event)| (*tick, u8::from(!is_note_off(event))));

        // Convert absolute ticks to delta ticks
        let mut last_tick: u64 = 0;
        let mut events = Vec::with_capacity(self.events.len());
        let mut event_stream = Vec::with_capacity(self.events.len());

        for (abs_tick, mut event) in self.events.drain(..) {
            let delta = (abs_tick - last_tick) as u32;
            event.delta_ticks = delta;
            last_tick = abs_tick;
            event_stream.push(MidiSourceEvent::Midi(event.clone()));
            events.push(event);
        }

        MidiSource {
            has_data: !events.is_empty(),
            ticks_per_qn: self.ticks_per_qn,
            ticks_timebase: Some("QN".to_string()),
            cc_interp: None,
            pooled_evts_guid: None,
            events,
            extended_events: vec![],
            event_stream,
            ignore_tempo: None,
            vel_lanes: vec![],
            bank_program_file: None,
            cfg_edit_view: None,
            cfg_edit: None,
            evt_filter: None,
            guid: None,
            unknown_lines: vec![],
        }
    }
}

impl Default for MidiSourceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// EnvelopeBuilder
// ===========================================================================

/// Builder for constructing track envelopes (volume, pan, etc.).
///
/// # Example
///
/// ```
/// use dawfile_reaper::builder::EnvelopeBuilder;
///
/// let env = EnvelopeBuilder::new("VOLENV2")
///     .active()
///     .visible()
///     .linear(0.0, 1.0)
///     .bezier(2.0, 0.5, -0.3)
///     .linear(4.0, 1.0)
///     .build();
///
/// assert_eq!(env.points.len(), 3);
/// assert!(env.active);
/// ```
pub struct EnvelopeBuilder {
    envelope: Envelope,
}

impl EnvelopeBuilder {
    /// Create a new envelope builder for the given type (e.g. "VOLENV2", "PANENV2").
    pub fn new(envelope_type: impl Into<String>) -> Self {
        Self {
            envelope: Envelope {
                envelope_type: envelope_type.into(),
                guid: String::new(),
                active: false,
                visible: false,
                show_in_lane: false,
                lane_height: 0,
                armed: false,
                default_shape: 0,
                points: vec![],
                automation_items: vec![],
                extension_data: vec![],
            },
        }
    }

    /// Set the envelope GUID.
    pub fn guid(mut self, guid: impl Into<String>) -> Self {
        self.envelope.guid = guid.into();
        self
    }

    /// Mark the envelope as active.
    pub fn active(mut self) -> Self {
        self.envelope.active = true;
        self
    }

    /// Mark the envelope as visible.
    pub fn visible(mut self) -> Self {
        self.envelope.visible = true;
        self
    }

    /// Show the envelope in its own lane.
    pub fn show_in_lane(mut self) -> Self {
        self.envelope.show_in_lane = true;
        self
    }

    /// Set the lane height in pixels.
    pub fn lane_height(mut self, height: i32) -> Self {
        self.envelope.lane_height = height;
        self
    }

    /// Arm the envelope for recording.
    pub fn armed(mut self) -> Self {
        self.envelope.armed = true;
        self
    }

    /// Set the default point shape.
    pub fn default_shape(mut self, shape: EnvelopePointShape) -> Self {
        self.envelope.default_shape = shape as i32;
        self
    }

    /// Add a point with a specific shape.
    pub fn point(mut self, position: f64, value: f64, shape: EnvelopePointShape) -> Self {
        self.envelope.points.push(EnvelopePoint {
            position,
            value,
            shape,
            time_sig: None,
            selected: None,
            unknown_field_6: None,
            bezier_tension: None,
        });
        self
    }

    /// Add a linear-interpolated point.
    pub fn linear(self, position: f64, value: f64) -> Self {
        self.point(position, value, EnvelopePointShape::Linear)
    }

    /// Add a square (step) point — value jumps instantly.
    pub fn square(self, position: f64, value: f64) -> Self {
        self.point(position, value, EnvelopePointShape::Square)
    }

    /// Add a bezier curve point with tension (-1.0 to 1.0).
    pub fn bezier(mut self, position: f64, value: f64, tension: f64) -> Self {
        self.envelope.points.push(EnvelopePoint {
            position,
            value,
            shape: EnvelopePointShape::Bezier,
            time_sig: None,
            selected: None,
            unknown_field_6: None,
            bezier_tension: Some(tension),
        });
        self
    }

    /// Add a slow-start/end curve point.
    pub fn slow_start_end(self, position: f64, value: f64) -> Self {
        self.point(position, value, EnvelopePointShape::SlowStartEnd)
    }

    /// Add a fast-start curve point.
    pub fn fast_start(self, position: f64, value: f64) -> Self {
        self.point(position, value, EnvelopePointShape::FastStart)
    }

    /// Add a fast-end curve point.
    pub fn fast_end(self, position: f64, value: f64) -> Self {
        self.point(position, value, EnvelopePointShape::FastEnd)
    }

    /// Build the final `Envelope`.
    pub fn build(self) -> Envelope {
        self.envelope
    }
}

// ===========================================================================
// ItemBuilder
// ===========================================================================

/// Builder for constructing media items within a track.
pub struct ItemBuilder {
    item: Item,
    takes: Vec<Take>,
}

impl ItemBuilder {
    fn new(position: f64, length: f64) -> Self {
        Self {
            item: Item {
                position,
                length,
                ..Item::default()
            },
            takes: vec![],
        }
    }

    /// Set the item name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.item.name = name.into();
        self
    }

    /// Assign this item to a fixed lane index, REAPER 7+ comping.
    ///
    /// REAPER has no per-item `LANE` token — the lane lives in the item's
    /// vertical geometry, `YPOS <lane/n> <1/n> 2`, and is derived back from
    /// it on read. The lane *count* is not known until the track's
    /// `LANENAME` list is set, so this records the index and
    /// [`TrackBuilder::build`] resolves it into the `YPOS` line.
    pub fn fixed_lane(mut self, lane: i32) -> Self {
        self.item.lane = Some(lane);
        self
    }

    /// Set the item's clip gain (item trim, linear; 1.0 = 0 dB) via VOLPAN.
    pub fn gain(mut self, item_trim: f64) -> Self {
        let vp = self
            .item
            .volpan
            .get_or_insert(crate::types::item::VolPanSettings {
                item_trim: 1.0,
                take_pan: 0.0,
                take_volume: 1.0,
                take_pan_law: -1.0,
            });
        vp.item_trim = item_trim;
        self
    }

    /// Mark this item as selected.
    pub fn selected(mut self) -> Self {
        self.item.selected = true;
        self
    }

    /// Mark this item as muted.
    pub fn muted(mut self) -> Self {
        self.item.mute = Some(crate::types::item::MuteSettings {
            muted: true,
            solo_state: crate::types::item::SoloState::NotSoloed,
        });
        self
    }

    /// Set the item GUID.
    pub fn guid(mut self, guid: impl Into<String>) -> Self {
        self.item.item_guid = Some(guid.into());
        self
    }

    /// Set item color.
    pub fn color(mut self, color: i32) -> Self {
        self.item.color = Some(color);
        self
    }

    /// Enable loop source.
    pub fn looped(mut self) -> Self {
        self.item.loop_source = true;
        self
    }

    /// Set snap offset.
    pub fn snap_offset(mut self, offset: f64) -> Self {
        self.item.snap_offset = offset;
        self
    }

    /// Add a WAVE source take (most common case).
    pub fn source_wave(self, file_path: impl Into<String>) -> Self {
        self.take(file_path, SourceType::Wave)
    }

    /// Set the play rate (1.0 = normal speed).
    pub fn playrate(mut self, rate: f64) -> Self {
        let pr = self.item.playrate.get_or_insert(PlayRateSettings {
            rate: 1.0,
            preserve_pitch: false,
            pitch_adjust: 0.0,
            pitch_mode: PitchMode::ProjectDefault,
            unknown_field_5: 0,
            unknown_field_6: 0.0,
        });
        pr.rate = rate;
        self
    }

    /// Set pitch adjustment in semitones (enables preserve-pitch).
    pub fn pitch(mut self, semitones: f64) -> Self {
        let pr = self.item.playrate.get_or_insert(PlayRateSettings {
            rate: 1.0,
            preserve_pitch: false,
            pitch_adjust: 0.0,
            pitch_mode: PitchMode::ProjectDefault,
            unknown_field_5: 0,
            unknown_field_6: 0.0,
        });
        pr.pitch_adjust = semitones;
        pr.preserve_pitch = true;
        self
    }

    /// Set the pitch/time-stretch mode.
    pub fn pitch_mode(mut self, mode: PitchMode) -> Self {
        let pr = self.item.playrate.get_or_insert(PlayRateSettings {
            rate: 1.0,
            preserve_pitch: false,
            pitch_adjust: 0.0,
            pitch_mode: PitchMode::ProjectDefault,
            unknown_field_5: 0,
            unknown_field_6: 0.0,
        });
        pr.pitch_mode = mode;
        self
    }

    /// Set the channel mode.
    pub fn channel_mode(mut self, mode: ChannelMode) -> Self {
        self.item.channel_mode = mode;
        self
    }

    /// Set the slip offset (source start offset in seconds).
    pub fn slip_offset(mut self, offset: f64) -> Self {
        self.item.slip_offset = offset;
        self
    }

    /// Set fade-in time with a curve type.
    pub fn fade_in(mut self, time: f64, curve: FadeCurveType) -> Self {
        self.item.fade_in = Some(FadeSettings {
            curve_type: curve,
            time,
            unknown_field_3: 0.0,
            unknown_field_4: 1,
            unknown_field_5: 0,
            unknown_field_6: 0,
            unknown_field_7: 0,
        });
        self
    }

    /// Set fade-out time with a curve type.
    pub fn fade_out(mut self, time: f64, curve: FadeCurveType) -> Self {
        self.item.fade_out = Some(FadeSettings {
            curve_type: curve,
            time,
            unknown_field_3: 0.0,
            unknown_field_4: 1,
            unknown_field_5: 0,
            unknown_field_6: 0,
            unknown_field_7: 0,
        });
        self
    }

    /// Add a stretch marker at the given position.
    pub fn stretch_marker(mut self, position: f64, source_position: f64) -> Self {
        self.item.stretch_markers.push(StretchMarker {
            position,
            source_position,
            rate: None,
        });
        self
    }

    /// Add a stretch marker with a specific rate.
    pub fn stretch_marker_with_rate(
        mut self,
        position: f64,
        source_position: f64,
        rate: f64,
    ) -> Self {
        self.item.stretch_markers.push(StretchMarker {
            position,
            source_position,
            rate: Some(rate),
        });
        self
    }

    /// Add an empty MIDI source take.
    pub fn source_midi(self) -> Self {
        self.take("", SourceType::Midi)
    }

    /// Add a MIDI source take with events constructed via a builder closure.
    ///
    /// # Example
    ///
    /// ```
    /// use dawfile_reaper::builder::{TrackBuilder, MidiSourceBuilder};
    ///
    /// let track = TrackBuilder::new("Piano")
    ///     .item(0.0, 4.0, |i| i
    ///         .name("Piano MIDI")
    ///         .midi(|m| m
    ///             .note(0, 0, 60, 96, 480)    // C4
    ///             .note(0, 0, 64, 96, 480)    // E4
    ///             .note(0, 0, 67, 96, 480)    // G4
    ///         )
    ///     )
    ///     .build();
    /// ```
    pub fn midi(mut self, f: impl FnOnce(MidiSourceBuilder) -> MidiSourceBuilder) -> Self {
        let builder = f(MidiSourceBuilder::new());
        let midi_source = builder.build();
        self.takes.push(Take {
            is_selected: self.takes.is_empty(),
            name: String::new(),
            source: Some(SourceBlock {
                source_type: SourceType::Midi,
                file_path: String::new(),
                midi_data: Some(midi_source),
                raw_content: String::new(),
            }),
            ..Take::default()
        });
        self
    }

    /// Add a FLAC source take.
    pub fn source_flac(self, file_path: impl Into<String>) -> Self {
        self.take(file_path, SourceType::Flac)
    }

    /// Add a MP3 source take.
    pub fn source_mp3(self, file_path: impl Into<String>) -> Self {
        self.take(file_path, SourceType::Mp3)
    }

    /// Add a take with a specific source type and file.
    pub fn take(mut self, file_path: impl Into<String>, source_type: SourceType) -> Self {
        let file = file_path.into();
        let name = file.clone();
        self.takes.push(Take {
            is_selected: self.takes.is_empty(), // First take is selected
            name,
            source: Some(SourceBlock {
                source_type,
                file_path: file,
                midi_data: None,
                raw_content: String::new(),
            }),
            ..Take::default()
        });
        self
    }

    /// Override the name of the last-pushed take. Useful when the take's
    /// display label should differ from the source file path (e.g. show a
    /// PT region name instead of the long absolute path).
    pub fn take_name(mut self, name: impl Into<String>) -> Self {
        if let Some(last) = self.takes.last_mut() {
            last.name = name.into();
        }
        self
    }

    fn build(mut self) -> Item {
        self.item.takes = self.takes;
        self.item
    }
}

// ===========================================================================
// TrackBuilder
// ===========================================================================

/// Builder for constructing REAPER tracks.
///
/// # Example
///
/// ```
/// use dawfile_reaper::builder::TrackBuilder;
///
/// let track = TrackBuilder::new("Guitar")
///     .volume(0.8)
///     .pan(-0.25)
///     .color(0xFF0000)
///     .muted()
///     .item(0.0, 4.0, |i| i
///         .name("Guitar DI")
///         .source_wave("guitar.wav")
///     )
///     .build();
///
/// assert_eq!(track.name, "Guitar");
/// ```
pub struct TrackBuilder {
    track: Track,
    fx_nodes: Vec<FxChainNode>,
    input_fx_nodes: Vec<FxChainNode>,
}

impl TrackBuilder {
    /// Create a new track builder with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            track: Track {
                name: name.into(),
                ..Track::default()
            },
            fx_nodes: vec![],
            input_fx_nodes: vec![],
        }
    }

    /// Set track volume (linear, 1.0 = 0dB).
    pub fn volume(mut self, vol: f64) -> Self {
        self.ensure_volpan().volume = vol;
        self
    }

    /// Set track pan (-1.0 = full left, 0.0 = center, 1.0 = full right).
    pub fn pan(mut self, pan: f64) -> Self {
        self.ensure_volpan().pan = pan;
        self
    }

    /// Set track color as RGB integer.
    pub fn color(mut self, color: u32) -> Self {
        self.track.peak_color = Some(color as i32);
        self
    }

    /// Put the track into fixed-lane (comp) mode (`FIXEDLANES`).
    pub fn fixed_lanes(mut self, settings: crate::types::track::FixedLanesSettings) -> Self {
        self.track.fixed_lanes = Some(settings);
        self
    }

    /// Set the per-lane display names (`LANENAME`).
    pub fn lane_names(mut self, settings: crate::types::track::LaneNameSettings) -> Self {
        self.track.lane_names = Some(settings);
        self
    }

    /// Set the lane record / comping state (`LANEREC`).
    pub fn lane_record(mut self, settings: crate::types::track::LaneRecordSettings) -> Self {
        self.track.lane_record = Some(settings);
        self
    }

    /// Set track GUID.
    pub fn guid(mut self, guid: impl Into<String>) -> Self {
        self.track.track_id = Some(guid.into());
        self
    }

    /// Set the track's TCP height in pixels (REAPER `TRACKHEIGHT`).
    pub fn height(mut self, px: i32) -> Self {
        self.track.track_height = Some(crate::types::track::TrackHeightSettings {
            height: px,
            folder_override: false,
        });
        self
    }

    /// Set the track's automation mode (corresponds to REAPER's `AUTOMODE`
    /// line). PT-imported tracks default to [`AutomationMode::Read`] so
    /// REAPER plays back any imported automation envelopes; standalone
    /// REAPER projects authored from scratch typically leave this at the
    /// builder default (`TrimRead`).
    pub fn automation_mode(mut self, mode: crate::types::track::AutomationMode) -> Self {
        self.track.automation_mode = mode;
        self
    }
    /// Shown in the mixer and not in the track panel (`SHOWINMIX` with
    /// the track list off): a track for mixing, out of the arrangement.
    pub fn mixer_only(mut self) -> Self {
        self.track.show_in_mixer = Some(crate::types::track::ShowInMixerSettings {
            show_in_mixer: true,
            unknown_field_2: 0.6667,
            unknown_field_3: 0.5,
            show_in_track_list: false,
            unknown_field_5: 0.5,
            unknown_field_6: 0,
            unknown_field_7: 0,
            unknown_field_8: 0,
        });
        self
    }

    /// Mark track as muted.
    pub fn muted(mut self) -> Self {
        self.ensure_mutesolo().mute = true;
        self
    }

    /// Mark track as soloed.
    pub fn soloed(mut self) -> Self {
        self.ensure_mutesolo().solo = TrackSoloState::Solo;
        self
    }

    /// Set the solo-defeat flag (this track ignores others' solo state).
    pub fn solo_defeated(mut self) -> Self {
        self.ensure_mutesolo().solo_defeat = true;
        self
    }

    /// Mark track as selected.
    pub fn selected(mut self) -> Self {
        self.track.selected = true;
        self
    }

    /// Arm the track for recording.
    pub fn armed(mut self) -> Self {
        let rec = self.track.record.get_or_insert(RecordSettings {
            armed: false,
            input: 0,
            monitor: crate::types::track::MonitorMode::Off,
            record_mode: crate::types::track::RecordMode::Input,
            monitor_track_media: false,
            preserve_pdc_delayed: false,
            record_path: 0,
        });
        rec.armed = true;
        self
    }

    /// Set the number of channels (default is 2).
    pub fn channels(mut self, n: u32) -> Self {
        self.track.channel_count = n;
        self
    }

    /// Make this track a folder parent (opens a folder level).
    pub fn folder_start(mut self) -> Self {
        self.track.folder = Some(FolderSettings {
            folder_state: FolderState::FolderParent,
            indentation: 1,
        });
        self
    }

    /// Close `levels` folder levels on this track.
    ///
    /// Use `folder_end(1)` to close one folder, `folder_end(2)` for two, etc.
    pub fn folder_end(mut self, levels: i32) -> Self {
        self.track.folder = Some(FolderSettings {
            folder_state: FolderState::LastInFolder,
            indentation: -levels,
        });
        self
    }

    /// Add a media item to this track.
    pub fn item(
        mut self,
        position: f64,
        length: f64,
        f: impl FnOnce(ItemBuilder) -> ItemBuilder,
    ) -> Self {
        let builder = f(ItemBuilder::new(position, length));
        self.track.items.push(builder.build());
        self
    }

    /// Add a simple wave item (shorthand for `.item()` with just a source file).
    pub fn wave_item(self, position: f64, length: f64, file: impl Into<String>) -> Self {
        let file = file.into();
        self.item(position, length, |i| i.source_wave(file))
    }

    /// Add a VST plugin to the track's FX chain.
    pub fn vst(self, name: impl Into<String>, file: impl Into<String>) -> Self {
        self.fx(|fx| {
            FxBuilder::new(
                format!("VST: {}", name.into()),
                PluginType::Vst,
                file.into(),
            )
        })
    }

    /// Add a VST3 plugin to the track's FX chain.
    pub fn vst3(self, name: impl Into<String>, file: impl Into<String>) -> Self {
        self.fx(|_| {
            FxBuilder::new(
                format!("VST3: {}", name.into()),
                PluginType::Vst3,
                file.into(),
            )
        })
    }

    /// Add a CLAP plugin to the track's FX chain.
    pub fn clap(self, name: impl Into<String>, file: impl Into<String>) -> Self {
        self.fx(|_| {
            FxBuilder::new(
                format!("CLAP: {}", name.into()),
                PluginType::Clap,
                file.into(),
            )
        })
    }

    /// Add a JS plugin to the track's FX chain.
    pub fn js(self, name: impl Into<String>) -> Self {
        self.fx(|_| {
            FxBuilder::new(
                format!("JS: {}", name.into()),
                PluginType::Js,
                String::new(),
            )
        })
    }

    /// Add an FX plugin to the track's FX chain using a builder closure.
    pub fn fx(mut self, f: impl FnOnce(FxBuilder) -> FxBuilder) -> Self {
        let dummy = FxBuilder::new("", PluginType::Vst, "");
        let builder = f(dummy);
        self.fx_nodes.push(FxChainNode::Plugin(builder.build()));
        self
    }

    /// Add a stock REAPER FX plugin to the track's FX chain.
    ///
    /// Accepts any type implementing [`StockFx`],
    /// such as `ReaComp`, `ReaEq`, `ReaGate`, etc.
    ///
    /// # Example
    ///
    /// ```
    /// use dawfile_reaper::builder::TrackBuilder;
    /// use dawfile_reaper::stock_fx::{ReaComp, ReaEq};
    ///
    /// let track = TrackBuilder::new("Vocals")
    ///     .stock_fx(ReaComp::new().threshold_db(-18.0).ratio(4.0))
    ///     .stock_fx(ReaEq::new().high_pass(0, 80.0, 0.7))
    ///     .build();
    /// ```
    pub fn stock_fx(mut self, fx: impl StockFx) -> Self {
        self.fx_nodes.push(fx.into_fx_node());
        self
    }

    /// Add a stock REAPER FX plugin to the track's input FX chain.
    pub fn input_stock_fx(mut self, fx: impl StockFx) -> Self {
        self.input_fx_nodes.push(fx.into_fx_node());
        self
    }

    /// Add an FX plugin to the track's input FX chain.
    pub fn input_fx(mut self, f: impl FnOnce(FxBuilder) -> FxBuilder) -> Self {
        let dummy = FxBuilder::new("", PluginType::Vst, "");
        let builder = f(dummy);
        self.input_fx_nodes
            .push(FxChainNode::Plugin(builder.build()));
        self
    }

    /// Add a track envelope (volume, pan, etc.) using a builder closure.
    ///
    /// # Example
    ///
    /// ```
    /// use dawfile_reaper::builder::TrackBuilder;
    ///
    /// let track = TrackBuilder::new("Vocals")
    ///     .envelope("VOLENV2", |e| e
    ///         .active()
    ///         .visible()
    ///         .linear(0.0, 1.0)
    ///         .bezier(2.0, 0.5, -0.3)
    ///         .linear(4.0, 1.0)
    ///     )
    ///     .build();
    ///
    /// assert_eq!(track.envelopes.len(), 1);
    /// ```
    pub fn envelope(
        mut self,
        envelope_type: impl Into<String>,
        f: impl FnOnce(EnvelopeBuilder) -> EnvelopeBuilder,
    ) -> Self {
        let builder = f(EnvelopeBuilder::new(envelope_type));
        self.track.envelopes.push(builder.build());
        self
    }

    /// Add a pre-built envelope directly.
    pub fn add_envelope(mut self, env: Envelope) -> Self {
        self.track.envelopes.push(env);
        self
    }

    /// Lock the track controls.
    pub fn locked(mut self) -> Self {
        self.track.locked = true;
        self
    }

    /// Disable FX on this track.
    pub fn fx_disabled(mut self) -> Self {
        self.track.fx_enabled = false;
        self
    }

    /// Add a receive from another track by index.
    pub fn receive(mut self, source_track_index: i32) -> Self {
        self.track
            .receives
            .push(crate::types::track::ReceiveSettings {
                source_track_index,
                mode: 0,
                volume: 1.0,
                pan: 0.0,
                mute: false,
                mono_sum: false,
                invert_polarity: false,
                source_audio_channels: 0,
                dest_audio_channels: 0,
                pan_law: -1.0,
                midi_channels: 0,
                automation_mode: -1,
            });
        self
    }

    /// Build the final Track.
    pub fn build(mut self) -> Track {
        if !self.fx_nodes.is_empty() {
            self.track.fx_chain = Some(FxChain {
                window_rect: None,
                show: 0,
                last_sel: 0,
                docked: false,
                nodes: self.fx_nodes,
                raw_content: String::new(),
            });
        }
        if !self.input_fx_nodes.is_empty() {
            self.track.input_fx = Some(FxChain {
                window_rect: None,
                show: 0,
                last_sel: 0,
                docked: false,
                nodes: self.input_fx_nodes,
                raw_content: String::new(),
            });
        }
        Self::resolve_fixed_lane_geometry(&mut self.track);
        self.track
    }

    /// Turn each item's fixed-lane index into the `YPOS` line REAPER
    /// actually reads.
    ///
    /// `ItemBuilder::fixed_lane` records the index only; there is no `LANE`
    /// token in an `<ITEM>` at any REAPER version (emitting one made REAPER
    /// reject the whole project — see the note in `Item::write_rpp`). A
    /// fixed-lanes track divides its height evenly, so lane `l` of `n` is
    /// `YPOS l/n 1/n 2`, which is exactly what `Item::from_block` derives
    /// the lane back from.
    ///
    /// The count comes from the track's `LANENAME` list when there is one,
    /// and otherwise from the highest lane any item claims — a track with
    /// lanes but no names still has to place its items.
    fn resolve_fixed_lane_geometry(track: &mut Track) {
        let named = track
            .lane_names
            .as_ref()
            .map_or(0, |ln| ln.lane_names.len());
        let claimed = track
            .items
            .iter()
            .filter_map(|i| i.lane)
            .max()
            .map_or(0, |max| (max + 1).max(0) as usize);
        let lanes = named.max(claimed);
        if lanes == 0 {
            return;
        }
        let height = 1.0 / lanes as f64;
        for item in &mut track.items {
            let Some(lane) = item.lane else { continue };
            if item.y_pos.is_some() {
                continue;
            }
            item.y_pos = Some(crate::types::item::ItemYPos {
                y: f64::from(lane) * height,
                height,
                mode: 2,
            });
        }
    }

    fn ensure_volpan(&mut self) -> &mut VolPanSettings {
        self.track.volpan.get_or_insert(VolPanSettings {
            volume: 1.0,
            pan: 0.0,
            pan_law: -1.0,
        })
    }

    fn ensure_mutesolo(&mut self) -> &mut MuteSoloSettings {
        self.track.mutesolo.get_or_insert(MuteSoloSettings {
            mute: false,
            solo: TrackSoloState::NoSolo,
            solo_defeat: false,
        })
    }
}

// ===========================================================================
// MarkerBuilder
// ===========================================================================

/// Builder for constructing markers and regions.
///
/// # Example
///
/// ```
/// use dawfile_reaper::builder::MarkerBuilder;
///
/// // Simple marker
/// let marker = MarkerBuilder::marker(1, 4.0, "Verse 1").build();
///
/// // Region with start and end
/// let region = MarkerBuilder::region(2, 4.0, 12.0, "Verse 1").lane(1).build();
/// ```
pub struct MarkerBuilder {
    marker: MarkerRegion,
}

impl MarkerBuilder {
    /// Create a marker (point) at the given position.
    pub fn marker(id: i32, position: f64, name: impl Into<String>) -> Self {
        Self {
            marker: MarkerRegion {
                id,
                position,
                name: name.into(),
                color: 0,
                flags: 0,
                locked: 0,
                guid: String::new(),
                additional: 0,
                end_position: None,
                lane: None,
                beat_position: None,
            },
        }
    }

    /// Create a region spanning from `start` to `end`.
    pub fn region(id: i32, start: f64, end: f64, name: impl Into<String>) -> Self {
        Self {
            marker: MarkerRegion {
                id,
                position: start,
                name: name.into(),
                color: 0,
                flags: 1, // region flag
                locked: 0,
                guid: String::new(),
                additional: 0,
                end_position: Some(end),
                lane: None,
                beat_position: None,
            },
        }
    }

    /// Set the marker/region color (REAPER color integer).
    pub fn color(mut self, color: i32) -> Self {
        self.marker.color = color;
        self
    }

    /// Assign to a ruler lane (v7.62+). 0 = default lane.
    pub fn lane(mut self, lane: i32) -> Self {
        self.marker.lane = Some(lane);
        self
    }

    /// Set the GUID for this marker/region.
    pub fn guid(mut self, guid: impl Into<String>) -> Self {
        self.marker.guid = guid.into();
        self
    }

    /// Lock this marker/region.
    pub fn locked(mut self) -> Self {
        self.marker.locked = 1;
        self
    }

    /// Set flags bitfield.
    pub fn flags(mut self, flags: i32) -> Self {
        self.marker.flags = flags;
        self
    }

    /// Build the final `MarkerRegion`.
    pub fn build(self) -> MarkerRegion {
        self.marker
    }
}

// ===========================================================================
// TempoEnvelopeBuilder
// ===========================================================================

/// Builder for constructing tempo/time-signature envelopes.
///
/// # Example
///
/// ```
/// use dawfile_reaper::builder::TempoEnvelopeBuilder;
///
/// let envelope = TempoEnvelopeBuilder::new(120.0, 4, 4)
///     .point(0.0, 120.0)
///     .point_with_time_sig(8.0, 140.0, 3, 4)
///     .ramp(16.0, 100.0)  // linear ramp to 100 BPM
///     .build();
///
/// assert_eq!(envelope.points.len(), 3);
/// ```
pub struct TempoEnvelopeBuilder {
    envelope: TempoTimeEnvelope,
}

impl TempoEnvelopeBuilder {
    /// Create a new tempo envelope with the given default tempo and time signature.
    pub fn new(default_tempo: f64, numerator: i32, denominator: i32) -> Self {
        Self {
            envelope: TempoTimeEnvelope::new(default_tempo, (numerator, denominator)),
        }
    }

    /// Add a tempo point at the given position (seconds) with square shape (instant jump).
    pub fn point(mut self, position: f64, tempo: f64) -> Self {
        self.envelope.points.push(TempoTimePoint {
            position,
            tempo,
            shape: 1, // square = instant
            ..TempoTimePoint::default()
        });
        self
    }

    /// Add a tempo point with a time signature change.
    ///
    /// Time signature is encoded as `65536 * denominator + numerator`.
    pub fn point_with_time_sig(
        mut self,
        position: f64,
        tempo: f64,
        numerator: i32,
        denominator: i32,
    ) -> Self {
        self.envelope.points.push(TempoTimePoint {
            position,
            tempo,
            shape: 1,
            time_signature_encoded: Some(65536 * denominator + numerator),
            ..TempoTimePoint::default()
        });
        self
    }

    /// Add a linear ramp point — tempo ramps linearly from the previous point to this one.
    pub fn ramp(mut self, position: f64, tempo: f64) -> Self {
        self.envelope.points.push(TempoTimePoint {
            position,
            tempo,
            shape: 0, // linear
            ..TempoTimePoint::default()
        });
        self
    }

    /// Add a bezier curve tempo point with the given tension (-1.0 to 1.0).
    pub fn bezier(mut self, position: f64, tempo: f64, tension: f64) -> Self {
        self.envelope.points.push(TempoTimePoint {
            position,
            tempo,
            shape: 5, // bezier
            bezier_tension: tension,
            ..TempoTimePoint::default()
        });
        self
    }

    /// Add a raw `TempoTimePoint` directly.
    pub fn raw_point(mut self, point: TempoTimePoint) -> Self {
        self.envelope.points.push(point);
        self
    }

    /// Build the final `TempoTimeEnvelope`.
    pub fn build(self) -> TempoTimeEnvelope {
        self.envelope
    }
}

// ===========================================================================
// ReaperProjectBuilder
// ===========================================================================

/// Fluent builder for constructing complete REAPER projects.
///
/// # Example
///
/// ```
/// use dawfile_reaper::builder::ReaperProjectBuilder;
/// use dawfile_reaper::RppSerialize;
///
/// let project = ReaperProjectBuilder::new()
///     .tempo(120.0)
///     .sample_rate(48000)
///     .track("Guitar", |t| t
///         .volume(0.8)
///         .item(0.0, 8.0, |i| i.source_wave("guitar.wav"))
///     )
///     .build();
///
/// let text = project.to_rpp_string();
/// assert!(text.contains("Guitar"));
/// ```
pub struct ReaperProjectBuilder {
    version: f64,
    version_string: String,
    timestamp: i64,
    properties: ProjectProperties,
    tracks: Vec<Track>,
    markers_regions: MarkerRegionCollection,
    tempo_envelope: Option<TempoTimeEnvelope>,
}

impl ReaperProjectBuilder {
    /// Create a new project builder with REAPER defaults.
    pub fn new() -> Self {
        Self {
            version: 0.1,
            version_string: "7.0/x64".to_string(),
            timestamp: 0,
            properties: ProjectProperties::new(),
            tracks: vec![],
            markers_regions: MarkerRegionCollection::new(),
            tempo_envelope: None,
        }
    }

    /// Set the REAPER version string (e.g. "7.0/linux-x86_64").
    pub fn version_string(mut self, v: impl Into<String>) -> Self {
        self.version_string = v.into();
        self
    }

    /// Set the project tempo in BPM.
    pub fn tempo(mut self, bpm: f64) -> Self {
        self.properties.tempo = Some((bpm, 4, 4, 0));
        self
    }

    /// Set the project tempo with a specific time signature.
    pub fn tempo_with_time_sig(mut self, bpm: f64, num: i32, den: i32) -> Self {
        self.properties.tempo = Some((bpm, num, den, 0));
        self
    }

    /// Set the project sample rate.
    pub fn sample_rate(mut self, rate: i32) -> Self {
        self.properties.sample_rate = Some((rate, 0, 0));
        self
    }

    /// Set the project timestamp.
    pub fn timestamp(mut self, ts: i64) -> Self {
        self.timestamp = ts;
        self
    }

    /// Add a track using a builder closure.
    pub fn track(
        mut self,
        name: impl Into<String>,
        f: impl FnOnce(TrackBuilder) -> TrackBuilder,
    ) -> Self {
        let builder = f(TrackBuilder::new(name));
        self.tracks.push(builder.build());
        self
    }

    /// Add a pre-built track directly.
    pub fn add_track(mut self, track: Track) -> Self {
        self.tracks.push(track);
        self
    }

    /// Add an empty track with just a name.
    pub fn empty_track(mut self, name: impl Into<String>) -> Self {
        self.tracks.push(Track {
            name: name.into(),
            ..Track::default()
        });
        self
    }

    /// Add a marker at the given position.
    pub fn marker(mut self, id: i32, position: f64, name: impl Into<String>) -> Self {
        self.markers_regions
            .add(MarkerBuilder::marker(id, position, name).build());
        self
    }

    /// Add a marker with a REAPER color int (`0x01000000 | (b<<16)|(g<<8)|r`;
    /// `0` = no custom color).
    pub fn marker_with_color(
        mut self,
        id: i32,
        position: f64,
        name: impl Into<String>,
        color: i32,
    ) -> Self {
        self.markers_regions.add(
            MarkerBuilder::marker(id, position, name)
                .color(color)
                .build(),
        );
        self
    }

    /// Add a region spanning from `start` to `end`.
    pub fn region(mut self, id: i32, start: f64, end: f64, name: impl Into<String>) -> Self {
        self.markers_regions
            .add(MarkerBuilder::region(id, start, end, name).build());
        self
    }

    /// Add a pre-built `MarkerRegion` directly.
    pub fn add_marker(mut self, marker: MarkerRegion) -> Self {
        self.markers_regions.add(marker);
        self
    }

    /// Set the tempo envelope using a builder closure.
    ///
    /// # Example
    ///
    /// ```
    /// use dawfile_reaper::builder::ReaperProjectBuilder;
    /// use dawfile_reaper::RppSerialize;
    ///
    /// let project = ReaperProjectBuilder::new()
    ///     .tempo(120.0)
    ///     .tempo_envelope(|e| e
    ///         .point(0.0, 120.0)
    ///         .ramp(8.0, 140.0)
    ///         .point(16.0, 120.0)
    ///     )
    ///     .build();
    ///
    /// let rpp = project.to_rpp_string();
    /// assert!(rpp.contains("TEMPOENVEX"));
    /// ```
    pub fn tempo_envelope(
        mut self,
        f: impl FnOnce(TempoEnvelopeBuilder) -> TempoEnvelopeBuilder,
    ) -> Self {
        let default_tempo = self.properties.tempo.map(|t| t.0 as f64).unwrap_or(120.0);
        let (num, den) = self.properties.tempo.map(|t| (t.1, t.2)).unwrap_or((4, 4));
        let builder = f(TempoEnvelopeBuilder::new(default_tempo, num, den));
        self.tempo_envelope = Some(builder.build());
        self
    }

    /// Set a pre-built tempo envelope directly.
    pub fn set_tempo_envelope(mut self, envelope: TempoTimeEnvelope) -> Self {
        self.tempo_envelope = Some(envelope);
        self
    }

    /// Build the final ReaperProject.
    pub fn build(self) -> ReaperProject {
        ReaperProject {
            version: self.version,
            version_string: self.version_string,
            timestamp: self.timestamp,
            properties: self.properties,
            tracks: self.tracks,
            items: vec![],
            envelopes: vec![],
            fx_chains: vec![],
            markers_regions: self.markers_regions,
            tempo_envelope: self.tempo_envelope,
            ruler_lanes: vec![],
            ruler_height: None,
        }
    }
}

impl Default for ReaperProjectBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

/// Whether an event ends a note.
///
/// A note-on with velocity zero is a note-off — the running-status
/// idiom, and a reader that missed it would pair the wrong events for
/// exactly the reason the sort exists.
fn is_note_off(event: &MidiEvent) -> bool {
    match event.bytes.first() {
        Some(status) if status & 0xF0 == 0x80 => true,
        Some(status) if status & 0xF0 == 0x90 => event.bytes.get(2).is_some_and(|vel| *vel == 0),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::serialize::RppSerialize;
    use crate::types::track::FolderState;

    #[test]
    fn test_minimal_project() {
        let project = ReaperProjectBuilder::new().build();
        let rpp = project.to_rpp_string();
        assert!(rpp.starts_with("<REAPER_PROJECT 0.1"));
        assert!(rpp.contains("RIPPLE 0"));
    }

    #[test]
    fn test_project_with_tempo_and_sample_rate() {
        let project = ReaperProjectBuilder::new()
            .tempo(140.0)
            .sample_rate(96000)
            .build();

        let rpp = project.to_rpp_string();
        assert!(rpp.contains("TEMPO 140"));
        assert!(rpp.contains("SAMPLERATE 96000"));
    }

    #[test]
    fn test_track_builder_basic() {
        let track = TrackBuilder::new("Vocals")
            .volume(0.75)
            .pan(0.1)
            .color(0xFF8800)
            .selected()
            .build();

        assert_eq!(track.name, "Vocals");
        assert_eq!(track.volpan.as_ref().unwrap().volume, 0.75);
        assert_eq!(track.volpan.as_ref().unwrap().pan, 0.1);
        assert_eq!(track.peak_color, Some(0xFF8800_u32 as i32));
        assert!(track.selected);
    }

    #[test]
    fn test_track_builder_muted_soloed_armed() {
        let track = TrackBuilder::new("Lead").muted().armed().build();

        assert!(track.mutesolo.as_ref().unwrap().mute);
        assert!(track.record.as_ref().unwrap().armed);
    }

    #[test]
    fn test_track_builder_with_items() {
        let track = TrackBuilder::new("Drums")
            .item(0.0, 4.0, |i| i.name("Kick").source_wave("kick.wav"))
            .item(4.0, 4.0, |i| i.name("Snare").source_wave("snare.wav"))
            .build();

        assert_eq!(track.items.len(), 2);
        assert_eq!(track.items[0].name, "Kick");
        assert_eq!(track.items[0].position, 0.0);
        assert_eq!(track.items[0].length, 4.0);
        assert_eq!(track.items[1].name, "Snare");
        assert_eq!(track.items[1].position, 4.0);
    }

    #[test]
    fn test_track_builder_with_fx() {
        let track = TrackBuilder::new("Guitar")
            .vst("ReaEQ (Cockos)", "reaeq.dll")
            .fx(|_| {
                FxBuilder::new("VST: ReaComp (Cockos)", PluginType::Vst, "reacomp.dll")
                    .bypassed()
                    .fxid("{COMP-GUID}")
            })
            .build();

        let chain = track.fx_chain.as_ref().unwrap();
        assert_eq!(chain.nodes.len(), 2);

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert!(p.name.contains("ReaEQ"));
            assert!(!p.bypassed);
        } else {
            panic!("Expected plugin");
        }

        if let FxChainNode::Plugin(p) = &chain.nodes[1] {
            assert!(p.name.contains("ReaComp"));
            assert!(p.bypassed);
            assert_eq!(p.fxid.as_deref(), Some("{COMP-GUID}"));
        } else {
            panic!("Expected plugin");
        }
    }

    #[test]
    fn test_track_folder_hierarchy() {
        let project = ReaperProjectBuilder::new()
            .track("Drums", |t| t.folder_start())
            .track("Kick", |t| t.item(0.0, 4.0, |i| i.source_wave("kick.wav")))
            .track("Snare", |t| {
                t.item(0.0, 4.0, |i| i.source_wave("snare.wav"))
                    .folder_end(1)
            })
            .track("Bass", |t| t.volume(0.7))
            .build();

        assert_eq!(project.tracks.len(), 4);

        // Drums is folder parent
        let drums = &project.tracks[0];
        assert_eq!(
            drums.folder.as_ref().unwrap().folder_state,
            FolderState::FolderParent
        );
        assert_eq!(drums.folder.as_ref().unwrap().indentation, 1);

        // Snare closes the folder
        let snare = &project.tracks[2];
        assert_eq!(
            snare.folder.as_ref().unwrap().folder_state,
            FolderState::LastInFolder
        );
        assert_eq!(snare.folder.as_ref().unwrap().indentation, -1);
    }

    #[test]
    fn test_wave_item_shorthand() {
        let track = TrackBuilder::new("Bass")
            .wave_item(0.0, 8.0, "bass_di.wav")
            .build();

        assert_eq!(track.items.len(), 1);
        let take = &track.items[0].takes[0];
        assert!(take.is_selected);
        assert_eq!(take.source.as_ref().unwrap().source_type, SourceType::Wave);
        assert_eq!(take.source.as_ref().unwrap().file_path, "bass_di.wav");
    }

    #[test]
    fn test_full_project_serialize_roundtrip() {
        let project = ReaperProjectBuilder::new()
            .tempo(120.0)
            .sample_rate(48000)
            .track("Drums", |t| {
                t.color(0x112233)
                    .folder_start()
                    .vst("ReaEQ (Cockos)", "reaeq.dll")
            })
            .track("Kick", |t| {
                t.item(0.0, 4.0, |i| i.name("Kick Pattern").source_wave("kick.wav"))
            })
            .track("Snare", |t| {
                t.item(0.0, 4.0, |i| i.source_wave("snare.wav"))
                    .folder_end(1)
            })
            .track("Bass", |t| {
                t.volume(0.7)
                    .armed()
                    .item(0.0, 8.0, |i| i.source_wave("bass.wav"))
            })
            .build();

        let rpp = project.to_rpp_string();

        // Verify it parses back
        let parsed = crate::io::parse_project_text(&rpp).expect("should parse builder output");
        assert_eq!(parsed.tracks.len(), 4);
        assert_eq!(parsed.tracks[0].name, "Drums");
        assert_eq!(parsed.tracks[1].name, "Kick");
        assert_eq!(parsed.tracks[2].name, "Snare");
        assert_eq!(parsed.tracks[3].name, "Bass");
    }

    #[test]
    fn test_input_fx() {
        let track = TrackBuilder::new("Vocals")
            .input_fx(|_| FxBuilder::new("VST: ReaTune (Cockos)", PluginType::Vst, "reatune.dll"))
            .build();

        assert!(track.input_fx.is_some());
        assert_eq!(track.input_fx.as_ref().unwrap().nodes.len(), 1);
        assert!(track.fx_chain.is_none());
    }

    #[test]
    fn test_empty_track() {
        let project = ReaperProjectBuilder::new()
            .empty_track("Track 1")
            .empty_track("Track 2")
            .build();

        assert_eq!(project.tracks.len(), 2);
        assert_eq!(project.tracks[0].name, "Track 1");
        assert_eq!(project.tracks[1].name, "Track 2");
    }

    #[test]
    fn test_add_prebuilt_track() {
        let track = TrackBuilder::new("Custom").volume(0.5).channels(4).build();

        let project = ReaperProjectBuilder::new().add_track(track).build();

        assert_eq!(project.tracks.len(), 1);
        assert_eq!(project.tracks[0].channel_count, 4);
    }

    #[test]
    fn test_item_builder_features() {
        let track = TrackBuilder::new("Test")
            .item(1.0, 2.0, |i| {
                i.name("Test Item")
                    .selected()
                    .muted()
                    .looped()
                    .color(0x334455)
                    .guid("{ITEM-GUID}")
                    .snap_offset(0.5)
                    .source_wave("test.wav")
            })
            .build();

        let item = &track.items[0];
        assert_eq!(item.name, "Test Item");
        assert!(item.selected);
        assert!(item.mute.as_ref().unwrap().muted);
        assert!(item.loop_source);
        assert_eq!(item.color, Some(0x334455));
        assert_eq!(item.item_guid.as_deref(), Some("{ITEM-GUID}"));
        assert_eq!(item.snap_offset, 0.5);
    }

    #[test]
    fn test_multiple_takes() {
        let track = TrackBuilder::new("Multitrack")
            .item(0.0, 4.0, |i| {
                i.take("take1.wav", SourceType::Wave)
                    .take("take2.wav", SourceType::Wave)
            })
            .build();

        assert_eq!(track.items[0].takes.len(), 2);
        assert!(track.items[0].takes[0].is_selected);
        assert!(!track.items[0].takes[1].is_selected);
    }

    #[test]
    fn test_project_with_markers() {
        let project = ReaperProjectBuilder::new()
            .tempo(120.0)
            .marker(1, 0.0, "Intro")
            .marker(2, 4.0, "Verse 1")
            .region(3, 4.0, 12.0, "Verse Section")
            .marker(4, 12.0, "Chorus")
            .empty_track("Track 1")
            .build();

        assert_eq!(project.markers_regions.markers.len(), 3);
        assert_eq!(project.markers_regions.regions.len(), 1);
        assert_eq!(project.markers_regions.all.len(), 4);

        let rpp = project.to_rpp_string();
        assert!(rpp.contains("MARKER 1 0 \"Intro\""));
        assert!(rpp.contains("MARKER 2 4 \"Verse 1\""));
        assert!(rpp.contains("MARKER 3 4 \"Verse Section\""));
        assert!(rpp.contains("MARKER 4 12 \"Chorus\""));
    }

    #[test]
    fn test_marker_with_lane() {
        let marker = MarkerBuilder::marker(1, 0.0, "=START")
            .lane(2)
            .color(0xFF0000)
            .locked()
            .guid("{TEST-GUID}")
            .build();

        assert_eq!(marker.lane, Some(2));
        assert_eq!(marker.color, 0xFF0000);
        assert_eq!(marker.locked, 1);
        assert_eq!(marker.guid, "{TEST-GUID}");
        assert!(marker.is_marker());
    }

    #[test]
    fn test_region_builder() {
        let region = MarkerBuilder::region(5, 4.0, 12.0, "Verse 1")
            .lane(1)
            .build();

        assert!(region.is_region());
        assert_eq!(region.duration(), Some(8.0));
        assert_eq!(region.lane, Some(1));
    }

    #[test]
    fn test_tempo_envelope_builder() {
        let project = ReaperProjectBuilder::new()
            .tempo(120.0)
            .tempo_envelope(|e| e.point(0.0, 120.0).ramp(8.0, 140.0).point(16.0, 120.0))
            .empty_track("Track 1")
            .build();

        let env = project.tempo_envelope.as_ref().unwrap();
        assert_eq!(env.points.len(), 3);
        assert_eq!(env.points[0].tempo, 120.0);
        assert_eq!(env.points[0].shape, 1); // square
        assert_eq!(env.points[1].tempo, 140.0);
        assert_eq!(env.points[1].shape, 0); // linear ramp
        assert_eq!(env.points[2].tempo, 120.0);

        let rpp = project.to_rpp_string();
        assert!(rpp.contains("<TEMPOENVEX"));
        assert!(rpp.contains("PT"));
    }

    #[test]
    fn test_tempo_envelope_with_time_sig() {
        let env = TempoEnvelopeBuilder::new(120.0, 4, 4)
            .point_with_time_sig(0.0, 120.0, 4, 4)
            .point_with_time_sig(8.0, 100.0, 3, 4)
            .build();

        assert_eq!(env.points.len(), 2);
        assert_eq!(
            env.points[0].time_signature_encoded,
            Some(65536 * 4 + 4) // 262148
        );
        assert_eq!(env.points[0].time_signature(), Some((4, 4)));
        assert_eq!(
            env.points[1].time_signature_encoded,
            Some(65536 * 4 + 3) // 262147
        );
        assert_eq!(env.points[1].time_signature(), Some((3, 4)));
    }

    #[test]
    fn test_tempo_envelope_bezier() {
        let env = TempoEnvelopeBuilder::new(120.0, 4, 4)
            .point(0.0, 120.0)
            .bezier(8.0, 160.0, 0.5)
            .build();

        assert_eq!(env.points[1].shape, 5);
        assert_eq!(env.points[1].bezier_tension, 0.5);
    }

    #[test]
    fn test_project_markers_and_tempo_serialize_roundtrip() {
        let project = ReaperProjectBuilder::new()
            .tempo(120.0)
            .sample_rate(48000)
            .marker(1, 0.0, "Intro")
            .marker(2, 4.0, "Verse")
            .tempo_envelope(|e| e.point(0.0, 120.0).ramp(8.0, 140.0))
            .track("Guitar", |t| {
                t.item(0.0, 8.0, |i| i.source_wave("guitar.wav"))
            })
            .build();

        let rpp = project.to_rpp_string();

        // Verify it parses back
        let parsed = crate::io::parse_project_text(&rpp).expect("should parse builder output");
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].name, "Guitar");

        // Markers should be in the output
        assert!(rpp.contains("MARKER 1 0 \"Intro\""));
        assert!(rpp.contains("MARKER 2 4 \"Verse\""));

        // Tempo envelope
        assert!(rpp.contains("<TEMPOENVEX"));
    }

    #[test]
    fn test_add_prebuilt_marker() {
        let marker = MarkerBuilder::marker(1, 5.0, "Custom")
            .color(0x00FF00)
            .lane(3)
            .build();

        let project = ReaperProjectBuilder::new().add_marker(marker).build();

        assert_eq!(project.markers_regions.all.len(), 1);
        assert_eq!(project.markers_regions.all[0].name, "Custom");
        assert_eq!(project.markers_regions.all[0].lane, Some(3));
    }

    #[test]
    fn test_nested_folders() {
        let project = ReaperProjectBuilder::new()
            .track("Instruments", |t| t.folder_start())
            .track("Drums", |t| t.folder_start())
            .track("Kick", |t| t)
            .track("Snare", |t| t.folder_end(1))
            .track("Bass", |t| t.folder_end(1))
            .build();

        assert_eq!(project.tracks.len(), 5);

        // Snare closes Drums folder
        assert_eq!(project.tracks[3].folder.as_ref().unwrap().indentation, -1);
        // Bass closes Instruments folder
        assert_eq!(project.tracks[4].folder.as_ref().unwrap().indentation, -1);
    }

    // ===================================================================
    // MidiSourceBuilder tests
    // ===================================================================

    #[test]
    fn test_midi_builder_single_note() {
        let midi = MidiSourceBuilder::new()
            .ticks_per_qn(960)
            .note(0, 0, 60, 96, 480)
            .build();

        assert_eq!(midi.ticks_per_qn, 960);
        assert!(midi.has_data);
        // Should have note-on + note-off
        assert_eq!(midi.events.len(), 2);

        // Note-on at tick 0
        assert_eq!(midi.events[0].delta_ticks, 0);
        assert_eq!(midi.events[0].bytes, vec![0x90, 60, 96]);

        // Note-off at tick 480
        assert_eq!(midi.events[1].delta_ticks, 480);
        assert_eq!(midi.events[1].bytes, vec![0x80, 60, 0]);
    }

    /// A note that ends exactly where the next begins: the off comes
    /// first.
    ///
    /// A reader pairs a note-on to the next note-off of the same pitch,
    /// so an on-before-off at one tick hands the SECOND note the
    /// FIRST's ending and calls it zero length. Every other note in a
    /// held progression comes out with no length, from MIDI that is
    /// correct in every other respect. (daw#28.)
    #[test]
    fn a_note_off_comes_before_the_note_on_that_shares_its_tick() {
        let midi = MidiSourceBuilder::new()
            .ticks_per_qn(960)
            .note(0, 0, 60, 96, 960)
            .at(960)
            .note(0, 0, 60, 96, 960)
            .build();

        let kinds: Vec<(u8, u8)> = midi
            .events
            .iter()
            .map(|event| (event.bytes[0] & 0xF0, event.bytes[1]))
            .collect();
        assert_eq!(
            kinds,
            vec![(0x90, 60), (0x80, 60), (0x90, 60), (0x80, 60)],
            "the second note-on arrived before the first note-off"
        );
        // And the deltas still add up to where the notes are.
        let mut at = 0;
        let ticks: Vec<u64> = midi
            .events
            .iter()
            .map(|event| {
                at += u64::from(event.delta_ticks);
                at
            })
            .collect();
        assert_eq!(ticks, vec![0, 960, 960, 1920]);
    }

    /// A note-on with velocity zero is a note-off, and sorts like one.
    #[test]
    fn a_zero_velocity_note_on_is_an_ending() {
        let midi = MidiSourceBuilder::new()
            .ticks_per_qn(960)
            .note_on(0, 0, 60, 96)
            .note_on(960, 0, 62, 96)
            .at(960)
            .note_on(0, 0, 60, 0)
            .build();

        let at_960: Vec<Vec<u8>> = {
            let mut at = 0;
            midi.events
                .iter()
                .filter_map(|event| {
                    at += u64::from(event.delta_ticks);
                    (at == 960).then(|| event.bytes.clone())
                })
                .collect()
        };
        assert_eq!(
            at_960.first().map(|bytes| bytes[1]),
            Some(60),
            "the ending did not come first: {at_960:?}"
        );
    }

    #[test]
    fn test_midi_builder_chord() {
        let midi = MidiSourceBuilder::new()
            .note(0, 0, 60, 96, 960) // C4
            .note(0, 0, 64, 96, 960) // E4
            .note(0, 0, 67, 96, 960) // G4
            .build();

        // 3 note-ons at tick 0 + 3 note-offs at tick 960
        assert_eq!(midi.events.len(), 6);

        // All note-ons should be at delta 0
        assert_eq!(midi.events[0].delta_ticks, 0);
        assert_eq!(midi.events[1].delta_ticks, 0);
        assert_eq!(midi.events[2].delta_ticks, 0);

        // First note-off at delta 960
        assert_eq!(midi.events[3].delta_ticks, 960);
        // Subsequent note-offs at delta 0 (same tick)
        assert_eq!(midi.events[4].delta_ticks, 0);
        assert_eq!(midi.events[5].delta_ticks, 0);
    }

    #[test]
    fn test_midi_builder_sequential_notes() {
        let midi = MidiSourceBuilder::new()
            .ticks_per_qn(960)
            .note(0, 0, 60, 96, 960) // C4 at tick 0, 1 beat
            .note(960, 0, 62, 96, 960) // D4 at tick 960, 1 beat
            .build();

        assert_eq!(midi.events.len(), 4);
        // Note-on C4 at tick 0
        assert_eq!(midi.events[0].delta_ticks, 0);
        assert_eq!(midi.events[0].bytes[1], 60);
        // Note-off C4 and Note-on D4 both at tick 960
        assert_eq!(midi.events[1].delta_ticks, 960);
    }

    #[test]
    fn test_midi_builder_cc_and_pitch_bend() {
        let midi = MidiSourceBuilder::new()
            .cc(0, 0, 1, 64) // Mod wheel to 64
            .cc(480, 0, 7, 100) // Volume to 100
            .pitch_bend(0, 0, 8192) // Center
            .pitch_bend(480, 0, 16383) // Max bend up
            .build();

        assert_eq!(midi.events.len(), 4);
        // CC1 at tick 0
        assert_eq!(midi.events[0].bytes, vec![0xB0, 1, 64]);
        // CC7 at tick 480
        assert_eq!(midi.events[1].bytes, vec![0xB0, 7, 100]);
        // Pitch bend center at tick 480
        assert_eq!(midi.events[2].bytes, vec![0xE0, 0, 64]); // 8192 = 0x2000
        // Pitch bend max at tick 960
        assert_eq!(midi.events[3].bytes, vec![0xE0, 127, 127]); // 16383 = 0x3FFF
    }

    #[test]
    fn test_midi_builder_program_change() {
        let midi = MidiSourceBuilder::new()
            .program_change(0, 0, 0) // Piano
            .program_change(0, 9, 25) // Drum kit on channel 10
            .build();

        assert_eq!(midi.events.len(), 2);
        assert_eq!(midi.events[0].bytes, vec![0xC0, 0]);
        assert_eq!(midi.events[1].bytes, vec![0xC9, 25]);
    }

    #[test]
    fn test_midi_builder_channel_pressure_and_aftertouch() {
        let midi = MidiSourceBuilder::new()
            .channel_pressure(0, 0, 100)
            .aftertouch(480, 0, 60, 80)
            .build();

        assert_eq!(midi.events.len(), 2);
        assert_eq!(midi.events[0].bytes, vec![0xD0, 100]);
        assert_eq!(midi.events[1].bytes, vec![0xA0, 60, 80]);
    }

    #[test]
    fn test_midi_builder_absolute_positioning() {
        let midi = MidiSourceBuilder::new()
            .at(0)
            .note_on(0, 0, 60, 96)
            .at(960)
            .note_off(0, 0, 60, 0)
            .build();

        assert_eq!(midi.events.len(), 2);
        assert_eq!(midi.events[0].delta_ticks, 0);
        assert_eq!(midi.events[1].delta_ticks, 960);
    }

    #[test]
    fn test_midi_builder_serializes_to_rpp() {
        use crate::types::serialize::RppSerialize;

        let project = ReaperProjectBuilder::new()
            .tempo(120.0)
            .track("Piano", |t| {
                t.item(0.0, 4.0, |i| {
                    i.name("Piano MIDI")
                        .midi(|m| m.ticks_per_qn(960).note(0, 0, 60, 96, 480).cc(0, 0, 1, 64))
                })
            })
            .build();

        let rpp = project.to_rpp_string();

        // Should contain MIDI source block
        assert!(rpp.contains("<SOURCE MIDI"));
        assert!(rpp.contains("HASDATA 1 960 QN"));
        // Should contain E lines with hex-encoded MIDI bytes
        assert!(rpp.contains("E 0 90 3c 60")); // Note-on C4 vel 96
        assert!(rpp.contains("E 0 b0 01 40")); // CC1 = 64
        assert!(rpp.contains("E 480 80 3c 00")); // Note-off C4
    }

    #[test]
    fn test_midi_builder_event_stream_matches_events() {
        let midi = MidiSourceBuilder::new()
            .note(0, 0, 60, 96, 480)
            .cc(240, 0, 1, 64)
            .build();

        // event_stream should contain the same events in order
        assert_eq!(midi.event_stream.len(), midi.events.len());
        for (stream_evt, evt) in midi.event_stream.iter().zip(midi.events.iter()) {
            if let crate::types::item::MidiSourceEvent::Midi(e) = stream_evt {
                assert_eq!(e.delta_ticks, evt.delta_ticks);
                assert_eq!(e.bytes, evt.bytes);
            } else {
                panic!("Expected Midi event in stream");
            }
        }
    }

    // ===================================================================
    // EnvelopeBuilder tests
    // ===================================================================

    #[test]
    fn test_envelope_builder_basic() {
        let env = EnvelopeBuilder::new("VOLENV2")
            .active()
            .visible()
            .guid("{VOL-GUID}")
            .linear(0.0, 1.0)
            .linear(4.0, 0.5)
            .build();

        assert_eq!(env.envelope_type, "VOLENV2");
        assert!(env.active);
        assert!(env.visible);
        assert_eq!(env.guid, "{VOL-GUID}");
        assert_eq!(env.points.len(), 2);
        assert_eq!(env.points[0].value, 1.0);
        assert_eq!(env.points[0].shape, EnvelopePointShape::Linear);
        assert_eq!(env.points[1].value, 0.5);
    }

    #[test]
    fn test_envelope_builder_shapes() {
        let env = EnvelopeBuilder::new("PANENV2")
            .linear(0.0, 0.0)
            .square(1.0, -0.5)
            .bezier(2.0, 0.5, 0.3)
            .slow_start_end(3.0, 0.0)
            .fast_start(4.0, 1.0)
            .fast_end(5.0, -1.0)
            .build();

        assert_eq!(env.points.len(), 6);
        assert_eq!(env.points[0].shape, EnvelopePointShape::Linear);
        assert_eq!(env.points[1].shape, EnvelopePointShape::Square);
        assert_eq!(env.points[2].shape, EnvelopePointShape::Bezier);
        assert_eq!(env.points[2].bezier_tension, Some(0.3));
        assert_eq!(env.points[3].shape, EnvelopePointShape::SlowStartEnd);
        assert_eq!(env.points[4].shape, EnvelopePointShape::FastStart);
        assert_eq!(env.points[5].shape, EnvelopePointShape::FastEnd);
    }

    #[test]
    fn test_track_with_envelope() {
        let track = TrackBuilder::new("Vocals")
            .envelope("VOLENV2", |e| {
                e.active()
                    .visible()
                    .linear(0.0, 1.0)
                    .bezier(2.0, 0.5, -0.3)
                    .linear(4.0, 1.0)
            })
            .build();

        assert_eq!(track.envelopes.len(), 1);
        assert_eq!(track.envelopes[0].envelope_type, "VOLENV2");
        assert!(track.envelopes[0].active);
        assert_eq!(track.envelopes[0].points.len(), 3);
    }

    #[test]
    fn test_track_with_multiple_envelopes() {
        let track = TrackBuilder::new("Synth")
            .envelope("VOLENV2", |e| e.active().linear(0.0, 1.0))
            .envelope("PANENV2", |e| e.active().linear(0.0, 0.0).linear(4.0, 0.5))
            .build();

        assert_eq!(track.envelopes.len(), 2);
        assert_eq!(track.envelopes[0].envelope_type, "VOLENV2");
        assert_eq!(track.envelopes[1].envelope_type, "PANENV2");
    }

    #[test]
    fn test_envelope_serializes_to_rpp() {
        use crate::types::serialize::RppSerialize;

        let project = ReaperProjectBuilder::new()
            .tempo(120.0)
            .track("Vocal", |t| {
                t.envelope("VOLENV2", |e| {
                    e.active()
                        .visible()
                        .guid("{ABC-123}")
                        .linear(0.0, 1.0)
                        .bezier(2.0, 0.5, -0.3)
                })
            })
            .build();

        let rpp = project.to_rpp_string();
        assert!(rpp.contains("<VOLENV2"));
        assert!(rpp.contains("EGUID {ABC-123}"));
        assert!(rpp.contains("ACT 1 -1"));
        assert!(rpp.contains("VIS 1"));
        assert!(rpp.contains("PT 0.000000 1.000000 0"));
    }

    // ===================================================================
    // Extended ItemBuilder tests
    // ===================================================================

    #[test]
    fn test_item_playrate_and_pitch() {
        let track = TrackBuilder::new("Guitar")
            .item(0.0, 4.0, |i| {
                i.name("Guitar DI")
                    .playrate(0.5) // half speed
                    .pitch(2.0) // up 2 semitones
                    .pitch_mode(PitchMode::ElastiquePro(0))
                    .source_wave("guitar.wav")
            })
            .build();

        let pr = track.items[0].playrate.as_ref().unwrap();
        assert_eq!(pr.rate, 0.5);
        assert_eq!(pr.pitch_adjust, 2.0);
        assert!(pr.preserve_pitch);
        assert_eq!(pr.pitch_mode, PitchMode::ElastiquePro(0));
    }

    #[test]
    fn test_item_channel_mode() {
        let track = TrackBuilder::new("Test")
            .item(0.0, 4.0, |i| {
                i.channel_mode(ChannelMode::MonoDownmix)
                    .source_wave("test.wav")
            })
            .build();

        assert_eq!(track.items[0].channel_mode, ChannelMode::MonoDownmix);
    }

    #[test]
    fn test_item_fades() {
        use crate::types::item::FadeCurveType;

        let track = TrackBuilder::new("Test")
            .item(0.0, 4.0, |i| {
                i.fade_in(0.1, FadeCurveType::Linear)
                    .fade_out(0.5, FadeCurveType::Bezier)
                    .source_wave("test.wav")
            })
            .build();

        let fi = track.items[0].fade_in.as_ref().unwrap();
        assert_eq!(fi.time, 0.1);
        assert_eq!(fi.curve_type, FadeCurveType::Linear);

        let fo = track.items[0].fade_out.as_ref().unwrap();
        assert_eq!(fo.time, 0.5);
        assert_eq!(fo.curve_type, FadeCurveType::Bezier);
    }

    #[test]
    fn test_item_stretch_markers() {
        let track = TrackBuilder::new("Test")
            .item(0.0, 4.0, |i| {
                i.stretch_marker(0.0, 0.0)
                    .stretch_marker(2.0, 1.8)
                    .stretch_marker_with_rate(4.0, 3.6, 1.1)
                    .source_wave("test.wav")
            })
            .build();

        assert_eq!(track.items[0].stretch_markers.len(), 3);
        assert_eq!(track.items[0].stretch_markers[1].position, 2.0);
        assert_eq!(track.items[0].stretch_markers[1].source_position, 1.8);
        assert_eq!(track.items[0].stretch_markers[2].rate, Some(1.1));
    }

    #[test]
    fn test_item_slip_offset() {
        let track = TrackBuilder::new("Test")
            .item(0.0, 4.0, |i| i.slip_offset(1.5).source_wave("test.wav"))
            .build();

        assert_eq!(track.items[0].slip_offset, 1.5);
    }

    #[test]
    fn test_extended_item_serializes_to_rpp() {
        use crate::types::serialize::RppSerialize;

        let project = ReaperProjectBuilder::new()
            .tempo(120.0)
            .track("Guitar", |t| {
                t.item(0.0, 4.0, |i| {
                    i.name("Guitar DI")
                        .playrate(0.5)
                        .pitch(2.0)
                        .fade_in(0.1, FadeCurveType::Linear)
                        .fade_out(0.2, FadeCurveType::Linear)
                        .channel_mode(ChannelMode::MonoDownmix)
                        .slip_offset(0.5)
                        .stretch_marker(0.0, 0.0)
                        .stretch_marker(2.0, 1.8)
                        .source_wave("guitar.wav")
                })
            })
            .build();

        let rpp = project.to_rpp_string();
        assert!(rpp.contains("PLAYRATE 0.5 1 2"));
        assert!(rpp.contains("FADEIN 0"));
        assert!(rpp.contains("FADEOUT 0"));
        assert!(rpp.contains("SOFFS 0.5"));
        assert!(rpp.contains("SM 0 0"));
        assert!(rpp.contains("SM 2 1.8"));
    }

    /// A lane index handed to `fixed_lane` has to survive serialization.
    ///
    /// It used to be written as a per-item `LANE <n>` line, which REAPER
    /// rejects ("Project tokens not recognized: LANE"); removing that line
    /// left `fixed_lane` with nowhere to go, so every Pro Tools comp track
    /// came out as a FIXEDLANES track whose items were all stacked on lane
    /// 0. The lane belongs in the item's vertical geometry.
    #[test]
    fn fixed_lanes_reach_the_rpp_as_ypos_geometry() {
        use crate::types::track::{FixedLanesSettings, LaneNameSettings};

        let project = ReaperProjectBuilder::new()
            .track("Vocal", |mut t| {
                for lane in 0..3 {
                    t = t.item(0.0, 1.0, |i| {
                        i.name(format!("Take {lane}")).fixed_lane(lane)
                    });
                }
                t.fixed_lanes(FixedLanesSettings {
                    bitfield: 9,
                    allow_editing: false,
                    show_play_only_lane: false,
                    mask_playback: false,
                    recording_behavior: 0,
                })
                .lane_names(LaneNameSettings {
                    lane_count: 3,
                    lane_names: vec!["Vocal".into(), "Vocal.01".into(), "Vocal.02".into()],
                })
            })
            .build();

        let rpp = project.to_rpp_string();
        assert!(
            !rpp.lines().any(|l| l.trim_start().starts_with("LANE ")),
            "no item may carry a LANE token — REAPER has none:\n{rpp}"
        );

        // Three lanes: each item claims a third of the track height, at an
        // offset of its own lane index.
        let lanes: Vec<i32> = rpp
            .lines()
            .filter_map(|l| l.trim_start().strip_prefix("YPOS "))
            .filter_map(|rest| {
                let mut f = rest.split_whitespace();
                let y: f64 = f.next()?.parse().ok()?;
                let height: f64 = f.next()?.parse().ok()?;
                assert!(
                    (height - 1.0 / 3.0).abs() < 1e-9,
                    "three lanes means a third of the height each, got {height}"
                );
                Some((y / height).round() as i32)
            })
            .collect();
        assert_eq!(lanes, vec![0, 1, 2], "one item per lane, in order:\n{rpp}");

        // And it reads back: the parser derives the lane from that geometry.
        let reparsed = crate::io::parse_project_text(&rpp).expect("re-parse");
        let mut back: Vec<i32> = reparsed.tracks[0]
            .items
            .iter()
            .map(|i| i.lane.expect("lane derived from YPOS"))
            .collect();
        back.sort_unstable();
        assert_eq!(back, vec![0, 1, 2]);
    }
}
