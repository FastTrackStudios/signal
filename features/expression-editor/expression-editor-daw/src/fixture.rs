//! A synthesized MPE performance.
//!
//! MPE is the one mode with no real material: a survey of 12,186
//! candidate files found three MPE-shaped ones, all trial demo content,
//! none carrying an MPE Configuration Message (#159). So the fixture is
//! **generated** rather than stored — it cannot drift with a format
//! change, it costs the repo no bytes, and every property a test wants
//! to assert is stated here as a constant rather than buried in a blob.
//!
//! What makes this a *real* MPE stream rather than multi-channel MIDI:
//!
//! - Notes overlap and are spread across member channels, one voice per
//!   channel, which is the whole reason MPE exists. A channel is only
//!   reused once its previous note has ended, so no note is ambiguous
//!   and every bend has exactly one owner.
//! - Each note carries all three expression dimensions — per-note pitch
//!   bend, channel pressure and CC74 timbre — as a stream of samples
//!   across its life, not a single value at its start.
//! - The stream opens with an **MPE Configuration Message** (RPN 6),
//!   which is exactly what the files found in the wild were missing,
//!   followed by an explicit per-note bend range (RPN 0).
//!
//! The bend range is stated on the wire on purpose. A document's
//! `bend_range` is the only thing that makes a 14-bit bend word and a
//! semitone offset comparable, so a reader that assumes 2 where the
//! writer meant 48 gets every pitch curve wrong by a factor of 24 —
//! silently, with plausible-looking output. [`parse_config`] reads the
//! range back out of the stream so a consumer can check rather than
//! assume.
//!
//! Channel pressure lives in [`MidiTakeSnapshot::channel_pressures`],
//! which is the read side of the facade. The write side has no channel
//! pressure in `MidiTakeContent`, so [`crate::to_content`] emits CC11
//! instead; a fixture round-tripped through a take comes back with its
//! pressure in a controller lane. That is a property of the write path,
//! not of this fixture.

use daw::service::midi::{MidiCC, MidiChannelPressure, MidiNote, MidiPitchBend, MidiTakeSnapshot};
use expression_editor_core::doc::ExpressionDoc;
use expression_editor_core::tuning;

/// Ticks per quarter note the fixture is authored at.
pub const PPQ: f64 = 960.0;

/// Lower-zone master channel, on the wire (0-based).
///
/// MPE defines a zone as a master channel plus the members above it.
/// The lower zone's master is channel 1 to a musician, 0 on the wire.
pub const MASTER_CHANNEL: u8 = 0;

/// How many member channels the zone declares — the RPN 6 value.
pub const MEMBER_CHANNELS: u8 = 8;

/// Per-note bend range the fixture declares, in semitones.
///
/// The same constant the editor loads takes with, so a fixture read at
/// the default never disagrees with what it was written at.
pub const PER_NOTE_BEND_RANGE: f64 = crate::DEFAULT_BEND_RANGE;

/// Master-channel bend range, in semitones. MPE's default is 2, and it
/// is a different quantity from the per-note range — a reader that
/// conflates them scales member bends by 2 instead of 48.
pub const MASTER_BEND_RANGE: f64 = 2.0;

/// Expression samples written across each note's life.
///
/// Endpoints alone are not enough: several instruments ignore sparse
/// data, and a curve that only exists at note-on is indistinguishable
/// from no gesture at all.
pub const EXPRESSION_STEPS: usize = 16;

/// CC74 — timbre, MPE's third dimension.
pub const TIMBRE_CC: u8 = 74;

/// The first member channel on the wire.
pub const FIRST_MEMBER_CHANNEL: u8 = MASTER_CHANNEL + 1;

/// What a note's pitch curve does over its life.
///
/// Named gestures rather than arbitrary numbers so a test can assert
/// the shape it expects — "this note bends up two semitones" is
/// checkable, "this note has some bend data" is not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bend {
    /// Sits at its written pitch.
    Steady,
    /// Vibrato of `depth` semitones, `cycles` full cycles over the note.
    Vibrato { depth: f64, cycles: f64 },
    /// Glides from the written pitch to `semitones` away by the end.
    Slide { semitones: f64 },
    /// Starts `semitones` away and arrives at the written pitch — a
    /// scoop, the gesture that most needs a wide bend range.
    Scoop { semitones: f64 },
}

impl Bend {
    /// Offset in semitones at `f`, the note's normalized position.
    pub fn at(&self, f: f64) -> f64 {
        match *self {
            Bend::Steady => 0.0,
            Bend::Vibrato { depth, cycles } => {
                // Fade the vibrato in, as a player does; a vibrato that
                // is at full depth on the attack reads as detuning.
                depth * f * (std::f64::consts::TAU * cycles * f).sin()
            }
            Bend::Slide { semitones } => semitones * f,
            Bend::Scoop { semitones } => semitones * (1.0 - f),
        }
    }
}

/// One synthesized voice, before channels are assigned.
#[derive(Clone, Copy, Debug)]
pub struct Voice {
    pub pitch: u8,
    pub velocity: u8,
    /// Start, in ticks at [`PPQ`].
    pub start: f64,
    /// Length, in ticks at [`PPQ`].
    pub len: f64,
    pub bend: Bend,
    /// Channel pressure at the start and peak of the swell, 0..=127.
    pub pressure: (u8, u8),
    /// CC74 at the note's start and end, 0..=127.
    pub timbre: (u8, u8),
}

/// The performance itself: a spread chord that fills the whole zone and
/// releases one voice at a time, with a line entering as the first
/// voice lets go.
///
/// Two things are deliberate. The chord takes every member channel, so
/// the line has to reuse released ones — which is where a channel
/// allocator goes wrong and a bend lands on the wrong note. And the
/// chord's shortest voice releases exactly where the line's first note
/// begins, so the fixture contains the boundary case rather than
/// avoiding it.
pub fn voices() -> Vec<Voice> {
    let q = PPQ;
    let v = |pitch: u8,
             velocity: u8,
             start: f64,
             len: f64,
             bend: Bend,
             pressure: (u8, u8),
             timbre: (u8, u8)| Voice {
        pitch,
        velocity,
        start: q * start,
        len: q * len,
        bend,
        pressure,
        timbre,
    };
    let vib = |depth: f64, cycles: f64| Bend::Vibrato { depth, cycles };
    let slide = |semitones: f64| Bend::Slide { semitones };
    let scoop = |semitones: f64| Bend::Scoop { semitones };

    vec![
        // The chord: eight voices from one downbeat, releasing one at a
        // time, so the whole zone is in use and the line above has to
        // take channels back.
        v(40, 60, 0.0, 4.0, vib(0.20, 3.0), (16, 80), (30, 70)),
        v(47, 64, 0.0, 4.5, vib(0.25, 4.0), (18, 84), (34, 78)),
        v(52, 68, 0.0, 5.0, vib(0.30, 2.0), (20, 88), (40, 92)),
        // One voice sits still. A fixture where every note moves would
        // hide a reader that attributes bends by position rather than
        // by channel.
        v(55, 66, 0.0, 5.5, Bend::Steady, (22, 76), (36, 66)),
        v(59, 70, 0.0, 6.0, vib(0.40, 5.0), (24, 90), (44, 96)),
        v(64, 74, 0.0, 3.0, scoop(-1.0), (26, 96), (64, 20)),
        v(67, 72, 0.0, 3.5, slide(0.75), (24, 92), (52, 110)),
        // First to release, at 2.5 quarters — the line takes this
        // channel back on the same tick.
        v(71, 78, 0.0, 2.5, slide(-0.5), (28, 98), (58, 26)),
        // The line. Its first note is the boundary case; its wide scoop
        // is the gesture that only reads correctly at the declared
        // 48-semitone range.
        v(76, 104, 2.5, 0.5, scoop(12.0), (30, 110), (24, 110)),
        v(78, 100, 3.0, 0.5, vib(0.75, 2.0), (32, 112), (70, 30)),
        v(79, 108, 3.5, 1.0, slide(2.0), (36, 120), (52, 118)),
        v(77, 96, 4.5, 1.5, vib(0.90, 4.0), (34, 116), (60, 24)),
        v(74, 88, 5.5, 2.0, slide(-3.0), (28, 104), (30, 100)),
    ]
}

/// The MPE zone a stream declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MpeConfig {
    /// Master channel, on the wire.
    pub master_channel: u8,
    /// Member channels the zone claims. Zero turns a zone off, which is
    /// how MPE says "this is ordinary MIDI again".
    pub member_channels: u8,
    /// Bend range declared on the member channels, in semitones, if the
    /// stream stated one.
    pub per_note_bend_range: Option<f64>,
}

impl MpeConfig {
    /// Wire channels a note may sound on.
    pub fn member_range(&self) -> std::ops::RangeInclusive<u8> {
        let first = self.master_channel + 1;
        first..=(first + self.member_channels.saturating_sub(1))
    }
}

/// Recover the zone declaration from a CC stream.
///
/// The point of reading it back rather than assuming it: an MPE file
/// without a Configuration Message is not an MPE file, and a file whose
/// declared bend range differs from the reader's default produces pitch
/// curves that are wrong by a constant factor with nothing to show for
/// it. A consumer should check this against the range it is about to
/// load with.
///
/// RPN state is tracked per channel, as the spec requires — a data
/// entry belongs to whichever parameter that channel last selected.
pub fn parse_config(ccs: &[MidiCC]) -> Option<MpeConfig> {
    let mut ordered: Vec<&MidiCC> = ccs.iter().collect();
    ordered.sort_by(|a, b| {
        a.position_ppq
            .partial_cmp(&b.position_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.index.cmp(&b.index))
    });

    // Selected RPN per channel, as (msb, lsb).
    let mut selected: [Option<(u8, u8)>; 16] = [None; 16];
    // Bend range each channel declared, whether or not a zone was seen
    // first — the two messages are independent and a writer may emit
    // them in either order.
    let mut bend_range: [Option<f64>; 16] = [None; 16];
    let mut zone: Option<(u8, u8)> = None;

    for cc in ordered {
        let ch = (cc.channel & 0x0f) as usize;
        match cc.controller {
            101 => {
                let lsb = selected[ch].map(|(_, l)| l).unwrap_or(0x7f);
                selected[ch] = Some((cc.value, lsb));
            }
            100 => {
                let msb = selected[ch].map(|(m, _)| m).unwrap_or(0x7f);
                selected[ch] = Some((msb, cc.value));
            }
            // Data entry MSB: means whatever the channel last selected.
            6 => match selected[ch] {
                // RPN 6 — the MPE Configuration Message.
                Some((0, 6)) => zone = Some((cc.channel & 0x0f, cc.value)),
                // RPN 0 — bend range, per channel.
                Some((0, 0)) => bend_range[ch] = Some(cc.value as f64),
                _ => {}
            },
            _ => {}
        }
    }

    let (master_channel, member_channels) = zone?;
    let config = MpeConfig {
        master_channel,
        member_channels,
        per_note_bend_range: None,
    };
    // Only a member channel's range is the per-note range; the master's
    // is its own, smaller quantity, and reading it as the per-note range
    // is how a scoop ends up a twenty-fourth of its written depth.
    let per_note_bend_range = config
        .member_range()
        .find_map(|ch| bend_range[(ch & 0x0f) as usize]);
    Some(MpeConfig {
        per_note_bend_range,
        ..config
    })
}

/// The Configuration Message and bend-range declarations, as CC events.
///
/// Written at tick 0, before any note. The RPN is closed with a null
/// select (127/127) after each parameter so a following data entry
/// cannot be misread as belonging to it — the failure that makes a
/// stream look valid and behave wrongly.
pub fn config_messages() -> Vec<MidiCC> {
    let mut out = Vec::new();
    let mut push = |channel: u8, controller: u8, value: u8| {
        out.push(MidiCC {
            index: 0,
            channel,
            controller,
            value,
            position_ppq: 0.0,
            selected: false,
        });
    };

    // RPN 6 on the master: this is a lower zone of MEMBER_CHANNELS.
    push(MASTER_CHANNEL, 101, 0);
    push(MASTER_CHANNEL, 100, 6);
    push(MASTER_CHANNEL, 6, MEMBER_CHANNELS);
    push(MASTER_CHANNEL, 101, 127);
    push(MASTER_CHANNEL, 100, 127);

    // RPN 0 on the master: the zone's own bend range.
    push(MASTER_CHANNEL, 101, 0);
    push(MASTER_CHANNEL, 100, 0);
    push(MASTER_CHANNEL, 6, MASTER_BEND_RANGE as u8);
    push(MASTER_CHANNEL, 38, 0);
    push(MASTER_CHANNEL, 101, 127);
    push(MASTER_CHANNEL, 100, 127);

    // RPN 0 on every member: the per-note bend range. The spec lets one
    // member speak for the zone; stating it on each is what a reader
    // that never saw the first message still sees.
    for ch in member_channels() {
        push(ch, 101, 0);
        push(ch, 100, 0);
        push(ch, 6, PER_NOTE_BEND_RANGE as u8);
        push(ch, 38, 0);
        push(ch, 101, 127);
        push(ch, 100, 127);
    }

    for (i, cc) in out.iter_mut().enumerate() {
        cc.index = i as u32;
    }
    out
}

/// Wire channels notes may sound on.
pub fn member_channels() -> impl Iterator<Item = u8> {
    FIRST_MEMBER_CHANNEL..(FIRST_MEMBER_CHANNEL + MEMBER_CHANNELS)
}

/// Assign each voice a member channel.
///
/// Least-recently-freed first, which is what MPE controllers do and
/// what keeps a released note's trailing bend from landing on the next
/// note to take its channel. A voice only takes a channel whose
/// previous note has already ended, so no two sounding notes ever share
/// one — the condition under which per-note expression is attributable
/// at all.
fn assign_channels(voices: &[Voice]) -> Vec<u8> {
    let members: Vec<u8> = member_channels().collect();
    // When each channel last went free. Negative so every channel is
    // free before the first note.
    let mut free_at = vec![f64::NEG_INFINITY; members.len()];
    let mut out = Vec::with_capacity(voices.len());

    for v in voices {
        let pick = free_at
            .iter()
            .enumerate()
            .filter(|&(_, &t)| t <= v.start)
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            // More simultaneous voices than member channels. The
            // fixture never hits this, and a caller that edits the
            // voices into that state should see the ambiguity rather
            // than have it hidden.
            .unwrap_or(0);
        free_at[pick] = v.start + v.len;
        out.push(members[pick]);
    }
    out
}

/// The fixture, as the snapshot every reader in the tree consumes.
///
/// Read it with [`PER_NOTE_BEND_RANGE`]; reading it with anything else
/// is the mistake the declared range exists to catch.
pub fn snapshot() -> MidiTakeSnapshot {
    let voices = voices();
    let channels = assign_channels(&voices);

    let mut notes = Vec::with_capacity(voices.len());
    let mut ccs = config_messages();
    let mut bends = Vec::new();
    let mut pressures = Vec::new();

    for (v, &channel) in voices.iter().zip(&channels) {
        notes.push(MidiNote {
            index: notes.len() as u32,
            channel,
            pitch: v.pitch,
            velocity: v.velocity,
            start_ppq: v.start,
            length_ppq: v.len,
            selected: false,
            muted: false,
        });

        // Samples are placed one tick inside each edge of the note.
        // Ownership is inclusive at both ends, so an event exactly on a
        // boundary belongs to two notes wherever a channel is reused
        // there — and an ambiguous owner means the expression is
        // dropped rather than misattributed. One tick at 960 PPQ costs
        // the gesture nothing and makes every sample unambiguous.
        let inner = (v.len - 2.0).max(0.0);
        for k in 0..=EXPRESSION_STEPS {
            let f = k as f64 / EXPRESSION_STEPS as f64;
            let t = v.start + 1.0 + inner * f;

            bends.push(MidiPitchBend {
                index: bends.len() as u32,
                channel,
                value: (tuning::semitones_to_bend14(v.bend.at(f), PER_NOTE_BEND_RANGE) as i32
                    - 8192) as i16,
                position_ppq: t,
                selected: false,
            });

            // A swell: up to the peak by a third of the note, then
            // easing back. Flat pressure would pass every test here
            // while being exactly the data a player never produces.
            let shape = if f < 1.0 / 3.0 {
                f * 3.0
            } else {
                1.0 - (f - 1.0 / 3.0) * 0.75
            };
            let (p0, peak) = v.pressure;
            let pressure = p0 as f64 + (peak as f64 - p0 as f64) * shape.clamp(0.0, 1.0);
            pressures.push(MidiChannelPressure {
                index: pressures.len() as u32,
                channel,
                pressure: pressure.round().clamp(0.0, 127.0) as u8,
                position_ppq: t,
                selected: false,
            });

            let (t0, t1) = v.timbre;
            let timbre = t0 as f64 + (t1 as f64 - t0 as f64) * f;
            ccs.push(MidiCC {
                index: ccs.len() as u32,
                channel,
                controller: TIMBRE_CC,
                value: timbre.round().clamp(0.0, 127.0) as u8,
                position_ppq: t,
                selected: false,
            });
        }
    }

    let end = notes
        .iter()
        .map(|n| n.start_ppq + n.length_ppq)
        .fold(0.0, f64::max);

    MidiTakeSnapshot {
        // A bar of air after the last release, so the take is not
        // exactly its own content and a view has somewhere to end.
        length_ppq: end + PPQ * 4.0,
        notes,
        ccs,
        pitch_bends: bends,
        channel_pressures: pressures,
        poly_pressures: Vec::new(),
        note_expressions: Vec::new(),
        ppq: PPQ,
    }
}

/// The fixture as an editable document, read at its declared range.
pub fn doc() -> ExpressionDoc {
    crate::to_doc(&snapshot(), PER_NOTE_BEND_RANGE)
}
