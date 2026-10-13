//! Performed media: the song sung and played, for the tracks whose audio
//! has to MEAN something.
//!
//! Most of the fixture's audio is a second of tone (`media`): enough to
//! open, to draw, to compare. A take you line a double up against needs
//! more — a line that moves, a double that is not quite on it — so the
//! lead vocals, the BGVs, the choir and the guitars play the song
//! (`golden_session::song`): the singers its melody, at their part's
//! interval, a double late, a little sharp, drifting; the
//! guitars its chords, strummed. One file per performance, shared by
//! the tracks that captured it (a guitar's mics, a singer's languages),
//! the length of the song, and each item reads its own stretch of it.

use num_traits::ToPrimitive;

use super::shape::{Section, Shape};
use super::song::{chord_at, melody_notes};

/// The performed media's sample rate: a voice's harmonics and a guitar's
/// strings with room to spare, at a quarter of the size.
pub const RATE: u32 = 11_025;

/// What a track performs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Perf {
    /// The melody sung `shift` semitones off; a double sits late, sharp
    /// and drifting against its lead.
    Sung { shift: f64, double: bool },
    /// The chords strummed, `shift` semitones up; `late` the second side
    /// of a doubled part.
    Strum { shift: f64, late: bool },
    /// The kit's one groove as a mic hears it: how loud each piece is in
    /// it (`tom` the one tom it is on, if any), and whether it is a
    /// reverb return, every hit rung out.
    Kit(Mic),
}

/// A kit mic: each piece's level in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mic {
    pub kick: f64,
    pub snare: f64,
    /// Which tom (1–4) the mic is on; every other tom bleeds at `toms`.
    pub tom: Option<u8>,
    pub toms: f64,
    pub hat: f64,
    pub cymbal: f64,
    /// A reverb return: the pieces rung out for most of a second.
    pub verb: bool,
}

impl Mic {
    const fn new(kick: f64, snare: f64, toms: f64, hat: f64, cymbal: f64) -> Self {
        Self {
            kick,
            snare,
            tom: None,
            toms,
            hat,
            cymbal,
            verb: false,
        }
    }

    /// The mic at `path` under the kit (`Kick/Sum/In`, `Toms/Tom 2/T2`,
    /// `Cymbals/OH`), `None` for a track that is not a mic.
    fn of(parts: &[&str]) -> Option<Self> {
        let verb = parts.contains(&"Verb");
        let mic = match parts {
            ["Kick", .., "Sub"] => Self::new(1.0, 0.0, 0.0, 0.0, 0.0),
            ["Kick", .., "Out"] => Self::new(1.0, 0.15, 0.08, 0.05, 0.06),
            ["Kick", ..] => Self::new(1.0, 0.08, 0.05, 0.02, 0.02),
            ["Snare", .., "Fund"] => Self::new(0.05, 1.0, 0.05, 0.0, 0.0),
            ["Snare", .., "Bottom"] => Self::new(0.12, 1.0, 0.06, 0.12, 0.05),
            ["Snare", ..] => Self::new(0.1, 1.0, 0.1, 0.3, 0.1),
            ["Toms", t, ..] if t.starts_with("Tom ") => Self {
                tom: t.trim_start_matches("Tom ").parse().ok(),
                ..Self::new(0.06, 0.12, 0.1, 0.05, 0.1)
            },
            ["Toms", ..] => Self::new(0.05, 0.1, 1.0, 0.0, 0.05),
            ["Cymbals", "Hi-Hat", ..] => Self::new(0.05, 0.3, 0.05, 1.0, 0.3),
            ["Cymbals", "Ride", ..] => Self::new(0.05, 0.2, 0.1, 0.2, 1.0),
            ["Cymbals", ..] => Self::new(0.3, 0.6, 0.5, 0.6, 1.0),
            ["Rooms", ..] => Self::new(0.5, 0.8, 0.6, 0.3, 0.6),
            _ => return None,
        };
        Some(Self { verb, ..mic })
    }
}

/// The performance a track's audio is, and the file it shares with the
/// tracks that captured the same one: `None` for a track whose audio is
/// only a tone.
#[must_use]
pub fn performance(path: &str) -> Option<(String, Perf)> {
    let parts: Vec<&str> = path.split('/').collect();
    let key = |n: usize| {
        parts
            .iter()
            .take(n)
            .map(|p| p.to_ascii_lowercase().replace(' ', "-"))
            .collect::<Vec<_>>()
            .join("-")
    };
    match parts.as_slice() {
        ["Vocals", _singer, take @ ("Main" | "DBL"), ..] => Some((
            format!("perf-{}", key(3)),
            Perf::Sung {
                shift: 0.0,
                double: *take == "DBL",
            },
        )),
        ["BGVs", part, ..] => {
            let shift = match *part {
                "Octave Up" => 12.0,
                "Octave Down" => -12.0,
                "Higher Harmony" => 4.0,
                "Lower Harmony" => -3.0,
                _ => 7.0,
            };
            Some((
                format!("perf-{}", key(2)),
                Perf::Sung {
                    shift,
                    double: false,
                },
            ))
        }
        ["Choir", part, ..] => {
            let shift = match *part {
                "Alto" => -5.0,
                "Tenor" => -12.0,
                "Bass" => -19.0,
                _ => 0.0,
            };
            Some((
                format!("perf-{}", key(2)),
                Perf::Sung {
                    shift,
                    double: false,
                },
            ))
        }
        // The kit's pieces — not its Process, which is mixing, not a take.
        ["Drums", rest @ ..] if rest.first() != Some(&"Process") => {
            let mic = Mic::of(rest)?;
            Some((format!("perf-{}", key(parts.len())), Perf::Kit(mic)))
        }
        ["Electric" | "Acoustic", ..] => {
            // Up to the side (`L`/`R`) a doubled part was played on; the
            // mics under it captured the same take.
            let side = parts.iter().position(|p| *p == "L" || *p == "R");
            let depth = side.map_or_else(|| parts.len().min(3), |s| s.saturating_add(1));
            let shift = if parts.contains(&"Octave") { 12.0 } else { 0.0 };
            let late = parts.contains(&"R");
            Some((format!("perf-{}", key(depth)), Perf::Strum { shift, late }))
        }
        _ => None,
    }
}

/// The recording pass a track was captured in: the kit is one pass for
/// all its mics; tracks sharing a performance are that performance's;
/// anything else is its own.
#[must_use]
pub fn pass(path: &str) -> String {
    let in_process = path == "Drums/Process" || path.starts_with("Drums/Process/");
    if !in_process && (path == "Drums" || path.starts_with("Drums/")) {
        return "Drums".to_owned();
    }
    performance(path).map_or_else(|| path.to_owned(), |(key, _)| key)
}

/// A pitch, in semitones from A4, as a frequency.
fn hz(semitones_from_a4: f64) -> f64 {
    440.0 * (semitones_from_a4 / 12.0).exp2()
}

/// The song's beats as seconds.
fn seconds(beats: u32, bpm: f64) -> f64 {
    f64::from(beats) * 60.0 / bpm.max(1.0)
}

/// How many samples a song of `bars` lasts.
fn length(bars: u32, bpm: f64) -> usize {
    (seconds(bars.saturating_mul(4), bpm) * f64::from(RATE))
        .ceil()
        .to_usize()
        .unwrap_or(0)
}

/// `perf` over the song `shape` is: mono samples at [`RATE`].
#[must_use]
pub fn render(perf: Perf, shape: &Shape) -> Vec<i16> {
    let mut buf = vec![0.0_f64; length(shape.bars, shape.bpm)];
    match perf {
        Perf::Sung { shift, double } => sing(&mut buf, shape, shift, double),
        Perf::Kit(mic) => kit(&mut buf, shape, mic),
        Perf::Strum { shift, late } => strum(
            &mut buf,
            &shape.sections,
            shape.bars,
            shape.bpm,
            shift,
            late,
        ),
    }
    let peak = buf.iter().fold(0.0_f64, |m, s| m.max(s.abs())).max(1e-9);
    buf.iter()
        .map(|s| (s / peak * 26_000.0).round().to_i16().unwrap_or(0))
        .collect()
}

/// The melody, sung: a voice of four harmonics with a vibrato that comes
/// in after the onset, shaped by a short attack and release; a double
/// 80 ms late, wandering ±25 ms, 8 cents sharp — a loose double, so
/// lining it up is plain to see and well over an analysis frame (these
/// files run at 11 kHz, where a frame is 46 ms).
fn sing(buf: &mut [f64], shape: &Shape, shift: f64, double: bool) {
    let rate = f64::from(RATE);
    let sharp = if double { 0.08 } else { 0.0 };
    for (start, len, pitch) in melody_notes(&shape.sections, shape.bars, 60) {
        let on = seconds(start, shape.bpm);
        let lag = if double { double_lag(on) } else { 0.0 };
        let t0 = on + lag;
        let t1 = seconds(start.saturating_add(len), shape.bpm) + lag - 0.04;
        let note = f64::from(pitch) - 69.0 + shift + sharp;
        let from = (t0 * rate).max(0.0).to_usize().unwrap_or(0);
        let to = (t1 * rate).max(0.0).to_usize().unwrap_or(0).min(buf.len());
        let Some(span) = buf.get_mut(from..to) else {
            continue;
        };
        let mut phase = 0.0_f64;
        for (i, out) in span.iter_mut().enumerate() {
            let t = i.to_f64().unwrap_or(0.0) / rate;
            let held = (t - 0.15).max(0.0);
            let vibrato = 0.2 * (held * 5.5 * std::f64::consts::TAU).sin() * (held * 4.0).min(1.0);
            phase += hz(note + vibrato) / rate * std::f64::consts::TAU;
            let remaining = (t1 - t0) - t;
            let envelope = (t / 0.04).min(1.0) * (remaining / 0.06).clamp(0.0, 1.0);
            let voice = harmonics(phase, &[1.0, 0.5, 0.3, 0.15]);
            *out += 0.6 * envelope * voice;
        }
    }
}

/// How late the double sings a note its lead begins at `on` seconds: 80
/// ms, wandering ±25 ms over the song. The one truth both the singing and
/// a test of lining the double back up read.
#[must_use]
pub fn double_lag(on: f64) -> f64 {
    let late = 0.080;
    0.025f64.mul_add(((on + late) * 0.7).sin(), late)
}

/// The melody's note onsets for `shape`, in seconds: what a lead sings,
/// and (each plus [`double_lag`]) its double.
#[must_use]
pub fn melody_onsets(shape: &Shape) -> Vec<f64> {
    melody_notes(&shape.sections, shape.bars, 60)
        .into_iter()
        .map(|(start, ..)| seconds(start, shape.bpm))
        .collect()
}

/// The chords, strummed: on every beat the bar's chord plucked low to
/// high 12 ms apart (high to low on the off-beats), each string a decaying
/// tone; `late` plays it 18 ms behind.
fn strum(buf: &mut [f64], sections: &[Section], bars: u32, bpm: f64, shift: f64, late: bool) {
    let rate = f64::from(RATE);
    let behind = if late { 0.018 } else { 0.0 };
    let beat = seconds(1, bpm);
    for b in 0..bars.saturating_mul(4) {
        let mut strings: Vec<f64> = {
            let chord = chord_at(sections, b.checked_div(4).unwrap_or(0));
            let mut tones = chord_tones(chord);
            tones.push(tones.first().copied().unwrap_or(52.0) + 12.0);
            tones.iter().map(|p| p - 69.0 + shift).collect()
        };
        let up = b.is_multiple_of(2);
        if !up {
            strings.reverse();
        }
        let gain = if up { 1.0 } else { 0.6 };
        for (k, note) in strings.into_iter().enumerate() {
            let onset = 0.012_f64.mul_add(k.to_f64().unwrap_or(0.0), seconds(b, bpm) + behind);
            let from = (onset * rate).to_usize().unwrap_or(0);
            let to = ((onset + beat * 1.5) * rate)
                .to_usize()
                .unwrap_or(0)
                .min(buf.len());
            let Some(span) = buf.get_mut(from..to) else {
                continue;
            };
            let f = hz(note);
            for (i, out) in span.iter_mut().enumerate() {
                let t = i.to_f64().unwrap_or(0.0) / rate;
                let phase = t * f * std::f64::consts::TAU;
                let pluck = harmonics(phase, &[1.0, 0.45, 0.2]);
                *out += 0.25 * gain * (-t * 5.0).exp() * pluck;
            }
        }
    }
}

/// A piece of the kit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    Kick,
    Snare,
    /// Tom 1–4, high to low.
    Tom(u8),
    Hat,
    Crash,
}

/// How far the drummer is off the grid on the groove's `n`th sixteenth.
///
/// ±18 ms, one push for every limb on it and every mic hearing it (one
/// drummer, one moment), different moment to moment — so quantizing the
/// kit has something to tighten.
#[must_use]
pub fn kit_push(n: usize) -> f64 {
    // A fixed hash to ±1: deterministic, so the fixture is the same file
    // every time it is written.
    let x = (n.to_f64().unwrap_or(0.0) * 12.9898).sin() * 43_758.545_3;
    0.018 * 2.0f64.mul_add(x - x.floor(), -1.0)
}

/// The kit's groove over `shape`: (seconds on the grid, piece, level) per
/// hit, before [`kit_push`].
///
/// Kick on one, three and the and of three;
/// snare on two and four; eighth hats; a crash at each section's top and
/// a tom fill down the four toms across its last bar's back half.
#[must_use]
pub fn groove(shape: &Shape) -> Vec<(f64, Piece, f64)> {
    let sixteenth =
        |bar: u32, s: u32| seconds(bar.saturating_mul(16).saturating_add(s), shape.bpm) / 4.0;
    let mut hits = Vec::new();
    for bar in 0..shape.bars {
        let top = shape.sections.iter().any(|sec| sec.start == bar);
        let last = shape
            .sections
            .iter()
            .any(|sec| sec.end == bar.saturating_add(1));
        if top {
            hits.push((sixteenth(bar, 0), Piece::Crash, 1.0));
        }
        for s in [0, 8, 10] {
            hits.push((sixteenth(bar, s), Piece::Kick, 1.0));
        }
        let fill_from = if last { 8 } else { 16 };
        for s in [4, 12].into_iter().filter(|s| *s < fill_from) {
            hits.push((sixteenth(bar, s), Piece::Snare, 1.0));
        }
        for s in (0..fill_from).step_by(2) {
            hits.push((
                sixteenth(bar, s),
                Piece::Hat,
                if s % 4 == 0 { 0.8 } else { 0.5 },
            ));
        }
        for (k, s) in (fill_from..16).enumerate() {
            let tom = u8::try_from(k.checked_div(2).unwrap_or(0).saturating_add(1))
                .unwrap_or(4)
                .min(4);
            hits.push((sixteenth(bar, s), Piece::Tom(tom), 0.9));
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    hits
}

/// Where the drummer played a hit the groove puts at `on` seconds: the
/// moment's [`kit_push`], and a limb's own spread of a millisecond or so
/// — a kick and a hat on one beat are one gesture, not two.
#[must_use]
pub fn kit_played(on: f64, piece: Piece, bpm: f64) -> f64 {
    let sixteenth = 15.0 / bpm.max(1.0);
    let n = (on / sixteenth).round().to_usize().unwrap_or(0);
    let limb = match piece {
        Piece::Kick => 0.0,
        Piece::Snare | Piece::Tom(_) => 0.0012,
        Piece::Hat | Piece::Crash => -0.0015,
    };
    on + kit_push(n) + limb
}

/// The groove as `mic` hears it: each hit where [`kit_played`] says, at
/// the mic's level for its piece.
fn kit(buf: &mut [f64], shape: &Shape, mic: Mic) {
    let rate = f64::from(RATE);
    let mut noise = Noise(0x2545_f491);
    for (on, piece, level) in groove(shape) {
        let gain = level
            * match piece {
                Piece::Kick => mic.kick,
                Piece::Snare => mic.snare,
                Piece::Tom(t) if mic.tom == Some(t) => 1.0,
                Piece::Tom(_) => mic.toms,
                Piece::Hat => mic.hat,
                Piece::Crash => mic.cymbal,
            };
        if gain <= 0.0 {
            continue;
        }
        let (body, decay) = match piece {
            Piece::Kick => (0.30_f64, 9.0_f64),
            Piece::Snare => (0.25, 14.0),
            Piece::Tom(_) => (0.45, 7.0),
            Piece::Hat => (0.06, 60.0),
            Piece::Crash => (1.5, 2.5),
        };
        let (length, decay) = if mic.verb {
            (0.9, decay.min(4.0))
        } else {
            (body, decay)
        };
        let at = kit_played(on, piece, shape.bpm);
        let from = (at * rate).max(0.0).to_usize().unwrap_or(0);
        let to = ((at + length) * rate)
            .to_usize()
            .unwrap_or(0)
            .min(buf.len());
        let Some(span) = buf.get_mut(from..to) else {
            continue;
        };
        let mut phase = 0.0_f64;
        for (i, out) in span.iter_mut().enumerate() {
            let t = i.to_f64().unwrap_or(0.0) / rate;
            let env = (-t * decay).exp() * (t / 0.002).min(1.0);
            let sound = match piece {
                Piece::Kick => {
                    phase +=
                        90.0f64.mul_add((-t * 30.0).exp(), 50.0) / rate * std::f64::consts::TAU;
                    phase.sin()
                }
                Piece::Snare => {
                    phase += 190.0 / rate * std::f64::consts::TAU;
                    0.5f64.mul_add(phase.sin(), 0.7 * noise.next())
                }
                Piece::Tom(t) => {
                    let hz = [150.0, 120.0, 95.0, 75.0]
                        .get(usize::from(t.saturating_sub(1)))
                        .copied()
                        .unwrap_or(100.0);
                    phase += hz / rate * std::f64::consts::TAU;
                    phase.sin()
                }
                Piece::Hat | Piece::Crash => noise.next(),
            };
            *out += 0.5 * gain * env * sound;
        }
    }
}

/// A small deterministic noise source: the same file every write.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (f64::from(self.0) / f64::from(u32::MAX)).mul_add(2.0, -1.0)
    }
}

/// A tone at `phase` with its harmonics: `amps[n]` the (n+1)th's level.
fn harmonics(phase: f64, amps: &[f64]) -> f64 {
    amps.iter().zip(1_u32..).fold(0.0, |sum, (amp, n)| {
        amp.mul_add((f64::from(n) * phase).sin(), sum)
    })
}

/// A chord's three tones, low (around E3), as MIDI pitches.
fn chord_tones(chord: super::song::Degree) -> Vec<f64> {
    chord.pitches(52, 3).into_iter().map(f64::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A singer's languages and a guitar's mics share their take's file;
    /// a take and its double do not.
    #[test]
    fn tracks_that_captured_one_performance_share_it() {
        let file = |p: &str| performance(p).map(|(k, _)| k);
        assert_eq!(file("Vocals/Ron/Main/EN"), file("Vocals/Ron/Main/ES"));
        assert_ne!(file("Vocals/Ron/Main/EN"), file("Vocals/Ron/DBL/EN"));
        assert_eq!(
            file("Electric/Rhythm/Main/L/Amp 1 Royer"),
            file("Electric/Rhythm/Main/L/DI")
        );
        assert_ne!(
            file("Electric/Rhythm/Main/L/DI"),
            file("Electric/Rhythm/Main/R/DI")
        );
        // Every kit mic is its own file: one groove, each mic's balance.
        assert_ne!(file("Drums/Kick/Sum/In"), file("Drums/Kick/Sum/Out"));
        assert!(matches!(
            performance("Drums/Toms/Tom 2/T2"),
            Some((_, Perf::Kit(Mic { tom: Some(2), .. })))
        ));
        assert_eq!(file("Process/FX/Room Sim"), None);
        assert!(matches!(
            performance("Vocals/Ron/DBL/EN"),
            Some((_, Perf::Sung { double: true, .. }))
        ));
    }

    /// The groove keeps time on its grid and the drummer does not: kick
    /// and snare where they belong, every hit pushed off it by up to 18
    /// ms, no two neighbours alike.
    #[test]
    fn the_groove_is_on_the_grid_and_the_drummer_is_not() {
        let shape = super::super::shape::maximal();
        let hits = groove(&shape);
        let kicks = hits.iter().filter(|h| h.1 == Piece::Kick).count();
        let snares = hits.iter().filter(|h| h.1 == Piece::Snare).count();
        assert_eq!(kicks, usize::try_from(shape.bars * 3).unwrap_or(0));
        assert!(snares > 0 && hits.iter().any(|h| matches!(h.1, Piece::Tom(4))));
        let pushes: Vec<f64> = (0..64).map(kit_push).collect();
        assert!(pushes.iter().all(|p| p.abs() <= 0.018));
        assert!(pushes.windows(2).all(|w| (w[0] - w[1]).abs() > 1e-6));
        assert!(pushes.iter().any(|p| p.abs() > 0.009), "it is felt");
    }

    /// A take is the song's length, sings where the melody is, and its
    /// double is not the same samples.
    #[test]
    fn a_take_is_the_song_and_its_double_differs() {
        let shape = super::super::shape::maximal();
        let main = render(
            Perf::Sung {
                shift: 0.0,
                double: false,
            },
            &shape,
        );
        let dbl = render(
            Perf::Sung {
                shift: 0.0,
                double: true,
            },
            &shape,
        );
        assert_eq!(main.len(), length(shape.bars, shape.bpm));
        assert!(main.iter().any(|s| s.unsigned_abs() > 10_000), "it sings");
        assert_ne!(main, dbl);
    }
}
