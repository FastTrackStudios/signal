//! Per-revision render snapshot: everything `render_block` reads,
//! copied out of `ProjectState` once per project revision so the
//! audio thread never holds the project lock through its inner loops.
//!
//! Cross-track references (folder parent, send destination) are
//! resolved to *track indices* at build time, so the hot path indexes
//! flat `Vec`s instead of hashing guids.

use std::collections::HashMap;
use std::sync::Arc;

use daw_proto::RouteType;
use daw_proto::automation::{EnvelopePoint, EnvelopeType, SendEnvelopeKind, TakeEnvelopeKind};
use daw_proto::primitives::AutomationMode;

use super::graph;

#[path = "stretch_render.rs"]
mod stretch_render;
use crate::audio_engine::source::AudioSource;
use crate::sync::{EnvelopeData, EnvelopeKey, ProjectState};

/// Honor `EnvelopeData.automation_mode` at snapshot time: return the
/// curve only when the mode is something other than `Off` — its points
/// with its automation items laid over them ([`daw_proto::automation::played`]),
/// flattened here, off the audio thread.
/// `Touch` / `Latch` / `Write` apply at playback like `Read` —
/// recording is a separate concern (see touch state on `Standalone`).
fn active_envelope(data: Option<&EnvelopeData>) -> Option<Vec<EnvelopePoint>> {
    let d = data?;
    match d.automation_mode {
        AutomationMode::Off => None,
        _ => Some(daw_proto::automation::played(
            &d.points,
            &d.automation_items,
        )),
    }
}

/// Everything `render_block` reads, snapshotted once per project
/// revision: the per-track graph, the processing order, the solo
/// routing mask, and the master section.
pub(crate) struct RenderSnapshot {
    pub(crate) tracks: Vec<TrackSnapshot>,
    /// Topological processing order over the ROUTING GRAPH: a track is
    /// fully processed before anything it feeds — its folder parent
    /// (children sum upward) and every send destination (busses
    /// receive post-fader signal). See [`graph::topo_order`].
    pub(crate) order: Vec<usize>,
    /// Solo routing mask — `Some` iff any track is soloed. With folder
    /// summing, a soloed track's signal flows THROUGH its ancestors,
    /// so they (and its descendants) keep passing audio even though
    /// they aren't soloed themselves. See [`graph::solo_pass`].
    pub(crate) solo_pass: Option<Vec<graph::Pass>>,
    /// Tempo map kept for beat-grid consumers (metronome click).
    pub(crate) tempo_map: TempoMap,
    /// Time-signature numerator (beats per measure) for click accents.
    pub(crate) beats_per_measure: u32,
    /// Time-signature denominator, carried for the aux-render clock.
    pub(crate) time_sig_den: u32,
    /// Master fader gain (linear).
    pub(crate) master_volume: f64,
    /// Master pan (−1…1).
    pub(crate) master_pan: f64,
    pub(crate) master_muted: bool,
    /// Live hardware-input channel per track, indexed like `tracks`.
    /// `Some(ch)` when the track's `RecordInput::Audio { channel }` taps
    /// input-device channel `ch`; `None` otherwise. Read by render
    /// stage 0 to route live input into the track bus.
    pub(crate) input_channels: Vec<Option<u32>>,
}

impl RenderSnapshot {
    pub(crate) fn build(p: &ProjectState) -> Arc<Self> {
        let tempo_map = TempoMap::from_state(p);
        let idx_by_guid: HashMap<&str, usize> = p
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.guid.as_str(), i))
            .collect();
        let tracks: Vec<TrackSnapshot> = p
            .tracks
            .iter()
            .map(|t| snapshot_track(p, t, &tempo_map, &idx_by_guid))
            .collect();
        let order = graph::topo_order(&tracks);
        let solo_pass = graph::solo_pass(&tracks);
        let beats_per_measure = p.transport.time_signature.numerator().max(1);
        let time_sig_den = p.transport.time_signature.denominator().max(1);
        let input_channels = tracks.iter().map(|t| t.input_channel).collect();
        Arc::new(Self {
            tracks,
            order,
            solo_pass,
            tempo_map,
            beats_per_measure,
            time_sig_den,
            master_volume: p.master_volume,
            master_pan: p.master_pan,
            master_muted: p.master_muted,
            input_channels,
        })
    }
}

/// Snapshot of routing-relevant track data.
pub(crate) struct TrackSnapshot {
    /// Project track guid. Carried so the live-MIDI drain can resolve a
    /// programmatic event's target track guid → snapshot index.
    pub(crate) guid: String,
    pub(crate) name: String,
    pub(crate) volume: f64,
    pub(crate) pan: f64,
    /// Polarity invert: flip the signal's sign at the gain stage.
    pub(crate) phase_inverted: bool,
    pub(crate) muted: bool,
    pub(crate) soloed: bool,
    pub(crate) parent_send: bool,
    /// Live hardware-input channel this track records/monitors from
    /// (`RecordInput::Audio { channel }`). `None` for non-audio inputs.
    pub(crate) input_channel: Option<u32>,
    /// Fixed-lane play mask (bit n = lane n audible). `0` when the
    /// track has no fixed lanes — every item plays.
    pub(crate) lane_play_mask: u64,
    /// VCA groups this track LEADS (its fader/mute scale followers).
    pub(crate) vca_lead: u128,
    /// VCA groups this track FOLLOWS: effective gain = own fader ×
    /// every shared-group lead's fader; a muted lead mutes it.
    pub(crate) vca_follow: u128,
    /// Track has hardware-output routes. v0 monitoring model: hw outs
    /// sum to master like a parent send (we only render one stereo
    /// device), so a `MAINSEND 0` track feeding the interface
    /// directly is still audible.
    pub(crate) hw_out: bool,
    /// Folder parent track index — children sum into the parent's
    /// bus. `None` when top-level OR the parent guid didn't resolve
    /// (the track then sums straight to master, like REAPER).
    pub(crate) parent_idx: Option<usize>,
    /// Raw parent guid presence, kept for debug tracing only: a
    /// `Some` here with `parent_idx == None` flags a dangling parent.
    pub(crate) has_parent_guid: bool,
    pub(crate) sends: Vec<SendSnapshot>,
    pub(crate) items: Vec<ItemSnapshot>,
    /// FX chain — list of `fx_guid`s in chain order. The renderer
    /// resolves each one in `Standalone.plugin_instances` and pipes
    /// the track bus through them in order. `None`/missing entries
    /// (e.g. synthetic-only FX) are skipped.
    pub(crate) fx_chain: Vec<String>,
    /// Per-FX enabled flag from `Fx.enabled`. Matched 1:1 with
    /// `fx_chain` by index.
    pub(crate) fx_enabled: Vec<bool>,
    /// Stored normalized parameter values per FX. Matched 1:1 with
    /// `fx_chain` by index and sent to hosted plugins every block so
    /// service-side parameter edits affect DSP.
    pub(crate) fx_params: Vec<Vec<(u32, f64)>>,
    /// Per-FX automation, matched 1:1 with `fx_chain`: each automated
    /// parameter's index and its envelope's points (an FX-param envelope
    /// not Off). Evaluated at each block's start ([`super::envelope::params_at`]).
    pub(crate) fx_envs: Vec<Vec<super::envelope::Automated>>,
    /// A send takes one of its plugins' aux outputs (3/4, 5/6), so the
    /// renderer collects them.
    pub(crate) aux_taps: bool,
    /// Volume envelope points, sorted by time. Multiplies the
    /// static `volume` field when present.
    pub(crate) volume_env: Option<Vec<EnvelopePoint>>,
    /// Pan envelope points. `0..=1` value range, where 0.5 = center.
    pub(crate) pan_env: Option<Vec<EnvelopePoint>>,
    /// Mute envelope. Values > 0.5 mute the track at that time.
    pub(crate) mute_env: Option<Vec<EnvelopePoint>>,
    /// Pre-FX volume envelope. Without an FX chain to insert between
    /// pre/post, this acts as an additional multiplier alongside the
    /// main volume envelope.
    pub(crate) volume_prefx_env: Option<Vec<EnvelopePoint>>,
    /// Pre-FX pan envelope. Blended additively with the static + main
    /// pan envelope, then clamped.
    pub(crate) pan_prefx_env: Option<Vec<EnvelopePoint>>,
    /// The volume envelope IS the volume — its mode reads or writes it
    /// (Read, Touch, Latch, Write), as REAPER plays those: the fader is
    /// not applied on top. Trim/Read leaves the fader as a trim.
    pub(crate) volume_replaces: bool,
    /// The same for pan: the envelope's pan, not added to the knob's.
    pub(crate) pan_replaces: bool,
}

#[derive(Default)]
struct FxChainSnapshot {
    envs: Vec<Vec<super::envelope::Automated>>,
    chain: Vec<String>,
    enabled: Vec<bool>,
    params: Vec<Vec<(u32, f64)>>,
}

pub(crate) struct SendSnapshot {
    /// Destination track index. `None` when the dest guid didn't
    /// resolve to a project track — the send is then a no-op.
    pub(crate) dest_idx: Option<usize>,
    /// The send lands on the destination's channels 3/4 — its
    /// sidechain, which keys the destination's plugins
    /// ([`crate::plugin::PluginInstance::set_sidechain`]) rather than
    /// being heard.
    pub(crate) to_sidechain: bool,
    /// Which of the source's pairs it takes: 0 its main output (1/2), 1
    /// its channels 3/4, 2 its 5/6 — a plugin's
    /// [`aux_out`](crate::plugin::PluginInstance::aux_out).
    pub(crate) from_pair: usize,
    pub(crate) volume: f64,
    pub(crate) pan: f64,
    pub(crate) muted: bool,
    /// Where the send taps the chain: post-fader (default), pre-FX,
    /// or post-FX/pre-fader.
    pub(crate) mode: daw_proto::routing::SendMode,
    /// Optional per-send envelopes. Block-evaluated alongside track
    /// envelopes; see `effective_send_*` helpers.
    pub(crate) volume_env: Option<Vec<EnvelopePoint>>,
    pub(crate) pan_env: Option<Vec<EnvelopePoint>>,
    pub(crate) mute_env: Option<Vec<EnvelopePoint>>,
}

pub(crate) struct ItemSnapshot {
    pub(crate) audio: Option<Arc<AudioSource>>,
    /// Fixed lane this item sits on (lane-enabled tracks only).
    pub(crate) fixed_lane: Option<u32>,
    pub(crate) position_seconds: f64,
    pub(crate) length_seconds: f64,
    pub(crate) fade_in_seconds: f64,
    pub(crate) fade_out_seconds: f64,
    pub(crate) fade_in_shape: daw_proto::item::FadeShape,
    pub(crate) fade_out_shape: daw_proto::item::FadeShape,
    /// Loop the source when the item outlasts it (RPP `LOOP 1`).
    pub(crate) loop_source: bool,
    pub(crate) muted: bool,
    pub(crate) item_volume: f64,
    pub(crate) take_volume: f64,
    pub(crate) play_rate: f64,
    pub(crate) start_offset_seconds: f64,
    /// The active take's stretch markers, sorted by position. Empty =
    /// unwarped (offset + rate only).
    pub(crate) stretch_markers: Vec<daw_proto::StretchMarker>,
    /// REAPER `CHANMODE` on the active take (0 = normal stereo).
    pub(crate) channel_mode: u32,
    /// Take pitch in semitones (added to envelope-driven pitch).
    pub(crate) take_pitch_semitones: f64,
    /// Per-take envelopes. All in item-relative time (0 at item
    /// start). Evaluated at block midpoint.
    pub(crate) take_volume_env: Option<Vec<EnvelopePoint>>,
    pub(crate) take_pan_env: Option<Vec<EnvelopePoint>>,
    pub(crate) take_mute_env: Option<Vec<EnvelopePoint>>,
    pub(crate) take_pitch_env: Option<Vec<EnvelopePoint>>,
    /// Sorted absolute-time MIDI notes on this item (only populated
    /// for MIDI takes — empty Vec for audio items). Each entry is
    /// `(start_seconds, length_seconds, channel, pitch, velocity)`
    /// in project time, already converted from PPQ at snapshot time
    /// using the project's static tempo. Per-block note events fall
    /// out by intersecting these against the render window.
    pub(crate) midi_notes: Vec<MidiNoteSnapshot>,
    /// Sorted absolute-time CC / pitch-bend / program-change /
    /// sysex events. Emitted to the plugin per-block as raw MIDI
    /// messages, in the same stream as the note events.
    pub(crate) midi_other: Vec<MidiOtherSnapshot>,
    /// Per-note expression points (MPE-style). Translated to
    /// `PluginNoteExpression` per block.
    pub(crate) note_expressions: Vec<NoteExpressionSnapshot>,
}

#[derive(Clone, Copy)]
pub(crate) struct NoteExpressionSnapshot {
    pub(crate) time_seconds: f64,
    pub(crate) channel: u8,
    pub(crate) note: u8,
    pub(crate) dimension: daw_proto::midi::NoteExpressionDim,
    pub(crate) value: f64,
}

#[derive(Clone, Copy)]
pub(crate) struct MidiNoteSnapshot {
    pub(crate) start_seconds: f64,
    pub(crate) length_seconds: f64,
    pub(crate) channel: u8,
    pub(crate) pitch: u8,
    pub(crate) velocity: u8,
}

#[derive(Clone)]
pub(crate) struct MidiOtherSnapshot {
    pub(crate) time_seconds: f64,
    pub(crate) message: daw_proto::MidiEvent,
}

/// Cumulative tempo-map: sorted list of `(time_seconds, beat, bpm)`
/// segment starts. Each segment runs at constant BPM until the next
/// entry; the final segment extends forever. Built once per
/// [`RenderSnapshot::build`] call from `p.tempo_points`.
///
/// The `beat` field is the project-time beat (quarter notes from the
/// origin) at which the segment begins — i.e. the PPQ of the segment
/// boundary. PPQ values on `MidiNote.start_ppq` are *project-time*
/// quarter notes when the source is a tempo-locked take.
pub(crate) struct TempoMap {
    segments: Vec<TempoSegment>,
}

#[derive(Clone, Copy)]
struct TempoSegment {
    /// Time in seconds at the segment start.
    start_seconds: f64,
    /// Beat (quarter notes from origin) at the segment start.
    start_beat: f64,
    /// Tempo within this segment (constant BPM).
    bpm: f64,
}

impl TempoMap {
    /// Build from `ProjectState`. Always returns at least one
    /// segment — a fallback `(0, 0, project_bpm)` covers projects
    /// with no tempo points.
    pub(crate) fn from_state(p: &ProjectState) -> Self {
        let default_bpm = p.transport.tempo.bpm().max(1.0);
        let mut pts: Vec<(f64, f64)> = p
            .tempo_points
            .iter()
            .map(|tp| (tp.position_seconds(), tp.bpm.max(1.0)))
            .collect();
        pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        // Drop duplicates at the same time (keep the last write).
        pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);

        let mut segments: Vec<TempoSegment> = Vec::new();
        // Always seed with a segment at time=0 so any PPQ before
        // the first tempo point still resolves to a number.
        if pts.first().map(|p| p.0 > 1e-9).unwrap_or(true) {
            segments.push(TempoSegment {
                start_seconds: 0.0,
                start_beat: 0.0,
                bpm: pts.first().map(|p| p.1).unwrap_or(default_bpm),
            });
        }
        for (time_seconds, bpm) in pts {
            // Beat at this segment start = previous beat + (time delta) * prev_bpm / 60.
            let start_beat = segments
                .last()
                .map(|prev| prev.start_beat + (time_seconds - prev.start_seconds) * prev.bpm / 60.0)
                .unwrap_or(0.0);
            segments.push(TempoSegment {
                start_seconds: time_seconds,
                start_beat,
                bpm,
            });
        }
        Self { segments }
    }

    /// Convert seconds to project-time beats (inverse of `beat_to_seconds`).
    pub(crate) fn seconds_to_beat(&self, seconds: f64) -> f64 {
        let mut idx = 0usize;
        for (i, s) in self.segments.iter().enumerate() {
            if s.start_seconds <= seconds {
                idx = i;
            } else {
                break;
            }
        }
        let seg = self.segments[idx];
        seg.start_beat + (seconds - seg.start_seconds) * seg.bpm / 60.0
    }

    /// Tempo (BPM) of the segment containing `seconds`.
    pub(crate) fn tempo_at(&self, seconds: f64) -> f64 {
        let mut idx = 0usize;
        for (i, s) in self.segments.iter().enumerate() {
            if s.start_seconds <= seconds {
                idx = i;
            } else {
                break;
            }
        }
        self.segments[idx].bpm
    }

    /// Convert project-time beats (= PPQ in quarter notes) to seconds.
    pub(crate) fn beat_to_seconds(&self, beat: f64) -> f64 {
        // Find the segment whose start_beat ≤ beat.
        let mut idx = 0usize;
        for (i, s) in self.segments.iter().enumerate() {
            if s.start_beat <= beat {
                idx = i;
            } else {
                break;
            }
        }
        let seg = self.segments[idx];
        seg.start_seconds + (beat - seg.start_beat) * 60.0 / seg.bpm
    }
}

fn snapshot_track(
    p: &ProjectState,
    t: &daw_proto::Track,
    tempo_map: &TempoMap,
    idx_by_guid: &HashMap<&str, usize>,
) -> TrackSnapshot {
    let parent_send = p
        .track_ext
        .get(&t.guid)
        .map(|e| e.parent_send_enabled)
        .unwrap_or(true);

    // Live hardware input: a track whose record input is
    // `RecordInput::Audio { channel }` hears that input-device channel —
    // when it is armed and monitoring its input, as REAPER has it. A
    // record input alone is only where a take would come from: a project
    // naming input 1 on every track (most do) put the one microphone
    // through every track to the speakers, and a phone playing it fed
    // back without end.
    let monitoring = t.armed && t.input_monitor != daw_proto::track::InputMonitoringMode::Off;
    let input_channel = match p.track_ext.get(&t.guid).map(|e| e.record_input) {
        Some(daw_proto::track::RecordInput::Audio { channel }) if monitoring => Some(channel),
        _ => None,
    };

    // Sends: only the Send variant matters (Receives mirror; HW out
    // for v0 also routes to master via parent_send equivalent).
    // `send_index` is the position in the source track's send list
    // — also the addressing key for `EnvelopeKey::Send`.
    let sends: Vec<SendSnapshot> = p
        .sends
        .get(&t.guid)
        .map(|v| {
            v.iter()
                .filter(|r| r.route_type == RouteType::Send)
                .enumerate()
                .filter_map(|(idx, r)| {
                    let dest_guid = r.dest_track_guid.as_deref()?;
                    let key = |kind| EnvelopeKey::Send {
                        send_index: idx as u32,
                        kind,
                    };
                    let volume_env = active_envelope(
                        p.envelopes
                            .get(&(t.guid.clone(), key(SendEnvelopeKind::Volume))),
                    );
                    let pan_env = active_envelope(
                        p.envelopes
                            .get(&(t.guid.clone(), key(SendEnvelopeKind::Pan))),
                    );
                    let mute_env = active_envelope(
                        p.envelopes
                            .get(&(t.guid.clone(), key(SendEnvelopeKind::Mute))),
                    );
                    Some(SendSnapshot {
                        dest_idx: idx_by_guid.get(dest_guid).copied(),
                        to_sidechain: r.dest_channels.start_channel >= 2,
                        from_pair: usize::try_from(r.source_channels.start_channel / 2)
                            .unwrap_or(0),
                        volume: r.volume,
                        pan: r.pan,
                        muted: r.muted,
                        mode: r.send_mode,
                        volume_env,
                        pan_env,
                        mute_env,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Items on this track.
    let mut items = Vec::new();
    if let Some(item_guids) = p.items_by_track.get(&t.guid) {
        for ig in item_guids {
            let Some(ie) = p.items.get(ig) else { continue };
            let item = &ie.item;
            // Active take.
            let take_guid_opt = p
                .takes
                .get(ig)
                .and_then(|tl| tl.takes.get(tl.active_idx as usize).map(|t| t.guid.clone()));
            let audio = take_guid_opt
                .as_ref()
                .and_then(|tg| p.audio_sources.get(tg).cloned());
            let stretch_markers = take_guid_opt
                .as_ref()
                .and_then(|tg| p.stretch_markers.get(tg).cloned())
                .unwrap_or_default();
            let (take_volume, play_rate, start_offset, take_pitch, channel_mode) = p
                .takes
                .get(ig)
                .and_then(|tl| tl.takes.get(tl.active_idx as usize))
                .map(|tk| {
                    (
                        tk.volume,
                        tk.play_rate,
                        tk.start_offset.as_seconds(),
                        tk.pitch,
                        tk.channel_mode,
                    )
                })
                .unwrap_or((1.0, 1.0, 0.0, 0.0, 0));
            // Look up each take envelope by `(owner="", EnvelopeKey::Take{..})`
            // — matches the storage convention in automation.rs.
            // Capture take_guid first so the closure can reuse it
            // without moving from `take_guid_opt`.
            let tg_for_env = take_guid_opt.clone();
            let take_env = |kind: TakeEnvelopeKind| -> Option<Vec<EnvelopePoint>> {
                let tg = tg_for_env.as_ref()?;
                active_envelope(p.envelopes.get(&(
                    String::new(),
                    EnvelopeKey::Take {
                        item_guid: ig.clone(),
                        take_guid: tg.clone(),
                        kind,
                    },
                )))
            };
            let take_volume_env = take_env(TakeEnvelopeKind::Volume);
            let take_pan_env = take_env(TakeEnvelopeKind::Pan);
            let take_mute_env = take_env(TakeEnvelopeKind::Mute);
            let take_pitch_env = take_env(TakeEnvelopeKind::Pitch);

            // PPQ → seconds via the project tempo map. PPQ on a take
            // is *relative* to the item's start (REAPER stores notes
            // local to the source). Convert by:
            //   1. Locate the item start as an absolute beat via
            //      the inverse tempo-map (seconds→beat).
            //   2. Compose: absolute beat = item_start_beat + note_ppq.
            //   3. Tempo-map that absolute beat back to seconds.
            // For constant-tempo projects this collapses to the old
            // formula `item_pos + ppq / qn_per_sec`.
            let item_pos = item.position.as_seconds();
            let item_start_beat = tempo_map.seconds_to_beat(item_pos);
            let to_seconds = |ppq: f64| tempo_map.beat_to_seconds(item_start_beat + ppq);

            let midi_other: Vec<MidiOtherSnapshot> = take_guid_opt
                .as_ref()
                .map(|tg| {
                    use daw_proto::{
                        Channel, ControllerNumber, ControllerValue, KeyNumber, MidiEvent,
                        PitchBend, Pressure, ProgramNumber,
                    };
                    let mut out: Vec<MidiOtherSnapshot> = Vec::new();
                    if let Some(ccs) = p.midi_ccs.get(tg) {
                        for cc in ccs {
                            out.push(MidiOtherSnapshot {
                                time_seconds: to_seconds(cc.position_ppq),
                                message: MidiEvent::ControlChange {
                                    channel: Channel::new(cc.channel),
                                    controller: ControllerNumber::new(cc.controller),
                                    value: ControllerValue::new(cc.value),
                                },
                            });
                        }
                    }
                    if let Some(pbs) = p.midi_pitch_bends.get(tg) {
                        for pb in pbs {
                            out.push(MidiOtherSnapshot {
                                time_seconds: to_seconds(pb.position_ppq),
                                message: MidiEvent::PitchBend {
                                    channel: Channel::new(pb.channel),
                                    // Stored as signed −8192..8191; midicore's
                                    // PitchBend is 14-bit unsigned centered at 8192.
                                    bend: PitchBend::new(
                                        (pb.value as i32 + 8192).clamp(0, 16383) as u16
                                    ),
                                },
                            });
                        }
                    }
                    if let Some(pcs) = p.midi_program_changes.get(tg) {
                        for pc in pcs {
                            out.push(MidiOtherSnapshot {
                                time_seconds: to_seconds(pc.position_ppq),
                                message: MidiEvent::ProgramChange {
                                    channel: Channel::new(pc.channel),
                                    program: ProgramNumber::new(pc.program),
                                },
                            });
                        }
                    }
                    if let Some(sx) = p.midi_sysex.get(tg) {
                        for s in sx {
                            out.push(MidiOtherSnapshot {
                                time_seconds: to_seconds(s.position_ppq),
                                message: MidiEvent::SysEx(s.data.clone()),
                            });
                        }
                    }
                    if let Some(cps) = p.midi_channel_pressures.get(tg) {
                        for cp in cps {
                            out.push(MidiOtherSnapshot {
                                time_seconds: to_seconds(cp.position_ppq),
                                message: MidiEvent::ChannelPressure {
                                    channel: Channel::new(cp.channel),
                                    pressure: Pressure::new(cp.pressure),
                                },
                            });
                        }
                    }
                    if let Some(pps) = p.midi_poly_pressures.get(tg) {
                        for pp in pps {
                            out.push(MidiOtherSnapshot {
                                time_seconds: to_seconds(pp.position_ppq),
                                message: MidiEvent::PolyAftertouch {
                                    channel: Channel::new(pp.channel),
                                    key: KeyNumber::new(pp.note),
                                    pressure: Pressure::new(pp.pressure),
                                },
                            });
                        }
                    }
                    out.sort_by(|a, b| {
                        a.time_seconds
                            .partial_cmp(&b.time_seconds)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    out
                })
                .unwrap_or_default();

            let note_expressions_snap: Vec<NoteExpressionSnapshot> = take_guid_opt
                .as_ref()
                .and_then(|tg| p.midi_note_expressions.get(tg))
                .map(|ne_list| {
                    let mut out: Vec<NoteExpressionSnapshot> = ne_list
                        .iter()
                        .map(|n| NoteExpressionSnapshot {
                            time_seconds: to_seconds(n.position_ppq),
                            channel: n.channel,
                            note: n.note,
                            dimension: n.dimension,
                            value: n.value,
                        })
                        .collect();
                    out.sort_by(|a, b| {
                        a.time_seconds
                            .partial_cmp(&b.time_seconds)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    out
                })
                .unwrap_or_default();

            let midi_notes: Vec<MidiNoteSnapshot> = take_guid_opt
                .as_ref()
                .and_then(|tg| p.midi_notes.get(tg))
                .map(|notes| {
                    let mut out: Vec<MidiNoteSnapshot> = notes
                        .iter()
                        .filter(|n| !n.muted && n.velocity > 0)
                        .map(|n| {
                            // PPQ length is also tempo-dependent —
                            // compute end via the tempo map and
                            // subtract.
                            let start = to_seconds(n.start_ppq);
                            let end = to_seconds(n.start_ppq + n.length_ppq);
                            MidiNoteSnapshot {
                                start_seconds: start,
                                length_seconds: (end - start).max(0.0),
                                channel: n.channel,
                                pitch: n.pitch,
                                velocity: n.velocity,
                            }
                        })
                        .collect();
                    out.sort_by(|a, b| {
                        a.start_seconds
                            .partial_cmp(&b.start_seconds)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    out
                })
                .unwrap_or_default();

            // A stretched take plays a render at its own pitch, as an
            // unstretched one: through the map, the mixer's varispeed
            // pitched every drawn-out tail down. Only a take played as
            // recorded otherwise — no rate, pitch or pitch envelope.
            let plain = (play_rate - 1.0).abs() < 1e-9
                && take_pitch.abs() < 1e-9
                && take_pitch_env.is_none();
            let (audio, start_offset, stretch_markers) = match audio
                .as_ref()
                .filter(|_| plain)
                .and_then(|a| {
                    stretch_render::rendered(a, &stretch_markers, item.length.as_seconds())
                }) {
                Some(render) => (Some(render), 0.0, Vec::new()),
                None => (audio, start_offset, stretch_markers),
            };
            items.push(ItemSnapshot {
                audio,
                fixed_lane: item.fixed_lane,
                position_seconds: item.position.as_seconds(),
                length_seconds: item.length.as_seconds(),
                fade_in_seconds: item.fade_in_length.as_seconds(),
                fade_out_seconds: item.fade_out_length.as_seconds(),
                fade_in_shape: item.fade_in_shape,
                fade_out_shape: item.fade_out_shape,
                loop_source: item.loop_source,
                muted: item.muted,
                item_volume: item.volume,
                take_volume,
                play_rate: if play_rate.abs() < 1e-9 {
                    1.0
                } else {
                    play_rate
                },
                start_offset_seconds: start_offset,
                stretch_markers,
                channel_mode,
                take_pitch_semitones: take_pitch,
                take_volume_env,
                take_pan_env,
                take_mute_env,
                take_pitch_env,
                midi_notes,
                midi_other,
                note_expressions: note_expressions_snap,
            });
        }
    }

    let track_env = |ty: EnvelopeType| {
        active_envelope(p.envelopes.get(&(t.guid.clone(), EnvelopeKey::Track(ty))))
    };
    // A track envelope with points, in a mode that reads it as the value
    // itself rather than as a trim.
    let replaces = |ty: EnvelopeType| {
        p.envelopes
            .get(&(t.guid.clone(), EnvelopeKey::Track(ty)))
            .is_some_and(|d| {
                !d.points.is_empty()
                    && !matches!(
                        crate::automation_touch::effective_mode(
                            Some(d.automation_mode),
                            Some(t.automation_mode)
                        ),
                        AutomationMode::TrimRead | AutomationMode::Off
                    )
            })
    };

    // FX chain (track-side only — input FX is a future addition).
    let fx = p
        .fx_chains
        .get(&crate::sync::FxChainKey::Track(t.guid.clone()))
        .map(|chain| {
            let mut guids = Vec::with_capacity(chain.len());
            let mut enabled = Vec::with_capacity(chain.len());
            let mut params = Vec::with_capacity(chain.len());
            let mut envs = Vec::with_capacity(chain.len());
            for e in chain {
                guids.push(e.fx.guid.clone());
                enabled.push(e.fx.enabled);
                params.push(e.params.iter().map(|(&id, &value)| (id, value)).collect());
                // Its parameters' envelopes, those that play.
                let mut automated: Vec<super::envelope::Automated> = p
                    .envelopes
                    .iter()
                    .filter_map(|((owner, key), data)| match key {
                        EnvelopeKey::FxParam {
                            fx_guid,
                            param_index,
                        } if owner == &t.guid && fx_guid == &e.fx.guid => {
                            active_envelope(Some(data))
                                .filter(|points| !points.is_empty())
                                .map(|points| {
                                    // The slot's id and range, from the
                                    // live instance; a synthetic FX's slot
                                    // is its id, 0..1.
                                    let (id, min, max) = e
                                        .ranges
                                        .get(*param_index as usize)
                                        .copied()
                                        .unwrap_or((*param_index, 0.0, 1.0));
                                    super::envelope::Automated {
                                        id,
                                        min,
                                        max,
                                        points,
                                    }
                                })
                        }
                        _ => None,
                    })
                    .collect();
                automated.sort_by_key(|a| a.id);
                envs.push(automated);
            }
            FxChainSnapshot {
                chain: guids,
                enabled,
                params,
                envs,
            }
        })
        .unwrap_or_default();

    let aux_taps = sends.iter().any(|s| s.from_pair > 0);
    TrackSnapshot {
        guid: t.guid.clone(),
        name: t.name.clone(),
        volume: t.volume,
        pan: t.pan,
        phase_inverted: t.phase_inverted,
        muted: t.muted,
        soloed: t.soloed,
        parent_send,
        input_channel,
        lane_play_mask: if t.lane_count > 0 {
            t.lane_play_mask
        } else {
            0
        },
        vca_lead: t.grouping.vca_lead,
        // Pre-FX VCA follow approximated at the fader for v0.
        vca_follow: t.grouping.vca_follow | t.grouping.vca_prefx_follow,
        hw_out: p
            .hw_outputs
            .get(&t.guid)
            .map(|v| !v.is_empty())
            .unwrap_or(false),
        parent_idx: t
            .parent_guid
            .as_deref()
            .and_then(|g| idx_by_guid.get(g).copied()),
        has_parent_guid: t.parent_guid.is_some(),
        sends,
        items,
        fx_chain: fx.chain,
        fx_enabled: fx.enabled,
        fx_params: fx.params,
        fx_envs: fx.envs,
        aux_taps,
        volume_env: track_env(EnvelopeType::Volume),
        pan_env: track_env(EnvelopeType::Pan),
        mute_env: track_env(EnvelopeType::Mute),
        volume_prefx_env: track_env(EnvelopeType::VolumePrefx),
        pan_prefx_env: track_env(EnvelopeType::PanPrefx),
        volume_replaces: replaces(EnvelopeType::Volume),
        pan_replaces: replaces(EnvelopeType::Pan),
    }
}

#[cfg(test)]
mod input_monitoring_tests {
    use daw_proto::track::{InputMonitoringMode, RecordInput};

    use super::RenderSnapshot;
    use crate::sync::{ProjectState, TrackExt};

    /// One track recording from input 1, armed or not, monitoring as given.
    fn project(armed: bool, monitor: InputMonitoringMode) -> ProjectState {
        let mut p = ProjectState::new(daw_proto::project::ProjectInfo {
            guid: "p".into(),
            name: "p".into(),
            path: String::new(),
        });
        let mut track = daw_proto::Track::new("t".into(), 0, "Vocal".into());
        track.armed = armed;
        track.input_monitor = monitor;
        p.tracks.push(track);
        p.track_ext.insert(
            "t".into(),
            TrackExt {
                record_input: RecordInput::Audio { channel: 0 },
                ..TrackExt::default()
            },
        );
        p
    }

    #[test]
    fn a_track_hears_its_input_only_armed_and_monitoring() {
        let hears =
            |armed, monitor| RenderSnapshot::build(&project(armed, monitor)).input_channels[0];
        assert_eq!(
            hears(false, InputMonitoringMode::Normal),
            None,
            "a record input alone is not monitoring: a project naming input 1 on every track fed the microphone through all of them"
        );
        assert_eq!(hears(true, InputMonitoringMode::Off), None);
        assert_eq!(hears(true, InputMonitoringMode::Normal), Some(0));
        assert_eq!(hears(true, InputMonitoringMode::NotWhenPlaying), Some(0));
    }
}
