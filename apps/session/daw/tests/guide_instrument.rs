//! Every note the guide generator writes is heard through the guide
//! instrument: the click and count always (synthesized if need be), and
//! each section cue from the FTS-GUIDE sample library when it is there.

use daw::plugin::{PluginEvents, PluginInstance, PluginMidiEvent};
use daw_proto::live_midi::{Channel, KeyNumber, MidiEvent, Velocity};
use session_daw::guide_instrument::{GuideInstrument, samples_dir};

const RATE: f64 = 48_000.0;

/// Play one note into a fresh instrument and return the loudest sample
/// over the next two seconds.
fn peak_of(note: u8) -> f32 {
    render(note).iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

/// The first two seconds of one note, left channel.
fn render(note: u8) -> Vec<f32> {
    let mut guide = GuideInstrument::new();
    guide.prepare(RATE, 512).expect("prepare");
    let on = [PluginMidiEvent {
        offset: 0,
        message: MidiEvent::NoteOn {
            channel: Channel::new(0),
            key: KeyNumber::new(note),
            velocity: Velocity::new(100),
        },
    }];
    let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let silence = vec![0.0f32; 512];
    let mut left = Vec::new();
    for block in 0..(2 * 48_000 / 512) {
        let events = if block == 0 {
            PluginEvents {
                midi: &on,
                ..PluginEvents::EMPTY
            }
        } else {
            PluginEvents::EMPTY
        };
        guide
            .process_block(&silence, &silence, &mut l, &mut r, &events)
            .expect("process");
        left.extend_from_slice(&l);
    }
    left
}

/// Zero crossings per second over the loud part of a note — a pitch
/// proxy good enough to order two clicks.
fn crossings_per_second(note: u8) -> f64 {
    let x = render(note);
    let peak = x.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let loud: Vec<f32> = x
        .into_iter()
        .skip_while(|s| s.abs() < peak * 0.1)
        .take(2_400)
        .collect();
    let crossings = loud
        .windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count();
    crossings as f64 / (loud.len() as f64 / RATE)
}

#[test]
fn on_beats_sound_higher_than_off_beats_and_the_one_matches_the_beat() {
    if !samples_dir().join("Click").is_dir() {
        return;
    }
    let (one, beat, off) = (
        crossings_per_second(60),
        crossings_per_second(61),
        crossings_per_second(62),
    );
    eprintln!("zero crossings/s — one {one:.0}, beat {beat:.0}, off-beat {off:.0}");
    assert!(
        (one - beat).abs() < 1.0,
        "the bar's one is the beat's sound"
    );
    assert!(beat > off, "the on-beat tick is the higher one");
}

#[test]
fn the_click_and_count_sound() {
    for (what, note) in [
        ("accent", 60),
        ("beat", 61),
        ("eighth", 62),
        ("count 1", 72),
        ("count 4", 75),
    ] {
        assert!(peak_of(note) > 0.01, "{what} (note {note}) is silent");
    }
}

#[test]
fn every_section_cue_sounds_when_the_library_is_installed() {
    if !samples_dir().join("Guide").is_dir() {
        eprintln!("(skip) no guide samples at {}", samples_dir().display());
        return;
    }
    let cues = [
        ("Verse", 84),
        ("Chorus", 85),
        ("Bridge", 86),
        ("Intro", 87),
        ("Outro", 88),
        ("Instrumental", 89),
        ("Pre-Chorus", 90),
        ("Post-Chorus", 91),
        ("Breakdown", 92),
        ("Interlude", 93),
        ("Tag", 94),
        ("End", 95),
        ("Solo", 96),
        ("Vamp", 97),
        ("Turnaround", 98),
        ("Refrain", 99),
    ];
    let silent: Vec<&str> = cues
        .iter()
        .filter(|(_, note)| peak_of(*note) <= 0.01)
        .map(|(name, _)| *name)
        .collect();
    assert!(silent.is_empty(), "silent cues: {silent:?}");
}
