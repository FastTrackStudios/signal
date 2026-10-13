//! The audio path, against signals whose truth is known.
//!
//! Synthesised rather than recorded, deliberately: a test that asserts
//! "this note came out as C4" is only meaningful if C4 is what went in,
//! and no recording lets you say that to the cent. The waveform is a
//! sawtooth-ish stack of harmonics with an amplitude envelope, which is
//! close enough to a voice for YIN — it is a period detector, and a
//! periodic signal is a periodic signal.

use expression_editor_audio::{TakeConfig, analyze_take, to_mono};
use expression_editor_core::doc::NoteId;

const SR: f64 = 44100.0;

fn midi_to_hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

/// A sung tone: harmonics, an attack and release, and optional vibrato.
fn tone(midi: f64, secs: f64, vibrato_cents: f64) -> Vec<f64> {
    let n = (SR * secs) as usize;
    let mut phase = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            let f = t / secs;
            // 5 Hz vibrato, the rate a singer actually uses.
            let cents = vibrato_cents * (t * core::f64::consts::TAU * 5.0).sin();
            let hz = midi_to_hz(midi + cents / 100.0);
            phase += core::f64::consts::TAU * hz / SR;
            // A few harmonics: YIN keys off periodicity, and a pure
            // sine is an easier signal than any real voice.
            let s = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.3 * (phase * 3.0).sin();
            // Attack and release, so note segmentation has edges.
            let env = (t / 0.02).min(1.0) * ((secs - t) / 0.05).clamp(0.0, 1.0);
            s * env * 0.3 * (0.9 + 0.1 * (f * 3.0).sin())
        })
        .collect()
}

fn silence(secs: f64) -> Vec<f64> {
    vec![0.0; (SR * secs) as usize]
}

/// Broadband noise at a plausible consonant level — an "s".
fn sibilant(secs: f64) -> Vec<f64> {
    let n = (SR * secs) as usize;
    let mut state = 0x2545F491_4F6CDD1Du64;
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 0.25
        })
        .collect()
}

fn cat(parts: &[Vec<f64>]) -> Vec<f64> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}

/// A note's sounding pitch: the row plus its contour's centre.
fn sounding(doc: &expression_editor_core::ExpressionDoc, id: NoteId) -> f64 {
    let n = doc.note(id).unwrap();
    n.row as f64
        + expression_editor_core::blob::decompose(
            &n.pitch,
            n.start,
            n.end,
            128,
            doc.time_base.units_per_second(120.0),
            0.0,
        )
        .center
}

#[test]
fn a_sung_note_comes_back_at_the_pitch_it_was_sung() {
    // A4 = 69.
    let audio = tone(69.0, 1.0, 0.0);
    let a = analyze_take(&audio, SR, TakeConfig::default());

    assert!(!a.doc.notes.is_empty(), "the note was found at all");
    let pitch = sounding(&a.doc, NoteId(1));
    assert!((pitch - 69.0).abs() < 0.25, "wanted A4 (69), got {pitch}");
    assert_eq!(a.doc.note(NoteId(1)).unwrap().row, 69);
}

#[test]
fn pitch_is_recovered_across_the_vocal_range() {
    // Low male through high female, which is where a tracker's octave
    // errors show up if its search range is wrong.
    for midi in [45.0, 52.0, 60.0, 69.0, 76.0, 81.0] {
        let audio = tone(midi, 0.8, 0.0);
        let a = analyze_take(&audio, SR, TakeConfig::default());
        assert!(!a.doc.notes.is_empty(), "nothing found at midi {midi}");
        let got = sounding(&a.doc, NoteId(1));
        assert!(
            (got - midi).abs() < 0.35,
            "midi {midi} came back as {got} — an error near 12 is an octave slip"
        );
    }
}

#[test]
fn a_phrase_becomes_one_note_per_pitch_in_order() {
    let audio = cat(&[
        tone(60.0, 0.5, 0.0),
        silence(0.12),
        tone(64.0, 0.5, 0.0),
        silence(0.12),
        tone(67.0, 0.5, 0.0),
    ]);
    let a = analyze_take(&audio, SR, TakeConfig::default());

    assert_eq!(a.doc.notes.len(), 3, "three sung notes, three notes");
    let rows: Vec<i32> = a.doc.notes.iter().map(|n| n.row).collect();
    assert_eq!(rows, vec![60, 64, 67]);
    // And in time order, which the blob↔note pairing depends on.
    for pair in a.doc.notes.windows(2) {
        assert!(pair[0].start < pair[1].start);
    }
}

#[test]
fn vibrato_survives_as_contour_rather_than_splitting_the_note() {
    // ±50 cents at 5 Hz: unmistakable vibrato, and well outside the
    // tracker's note-matching tolerance if it were read as pitch change.
    let audio = tone(62.0, 1.2, 50.0);
    let a = analyze_take(&audio, SR, TakeConfig::default());

    assert_eq!(
        a.doc.notes.len(),
        1,
        "vibrato is one note with a wobble, not a run of notes"
    );
    let n = a.doc.note(NoteId(1)).unwrap();
    // The contour has to actually contain the wobble.
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for k in 0..200 {
        let t = n.start + (n.end - n.start) * (k as f64 / 199.0);
        let v = n.pitch.sample(t, 0.0);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    assert!(
        hi - lo > 0.5,
        "the vibrato is in the curve: swing was {} semitones",
        hi - lo
    );
    // ...and the note still sits where it was sung.
    assert!((sounding(&a.doc, NoteId(1)) - 62.0).abs() < 0.3);
}

#[test]
fn the_take_waveform_and_the_note_envelopes_are_filled() {
    let audio = cat(&[tone(60.0, 0.4, 0.0), silence(0.2), tone(64.0, 0.4, 0.0)]);
    let a = analyze_take(&audio, SR, TakeConfig::default());

    assert!(
        !a.doc.peaks.is_empty(),
        "the backdrop has something to draw"
    );
    assert!(
        a.doc.peaks.iter().any(|&v| v > 0.5),
        "and it is normalized against the loudest frame"
    );
    for n in &a.doc.notes {
        assert!(!n.envelope.is_empty(), "every note carries its waveform");
        assert!(n.envelope.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}

#[test]
fn a_consonant_reads_as_unvoiced_and_silence_does_not() {
    let audio = cat(&[
        tone(60.0, 0.4, 0.0),
        sibilant(0.25),
        tone(60.0, 0.4, 0.0),
        silence(0.4),
    ]);
    let a = analyze_take(&audio, SR, TakeConfig::default());

    assert!(!a.doc.unvoiced.is_empty(), "the consonant was found");
    let fr = a.frame_rate;
    // The sibilant sits between 0.4 s and 0.65 s.
    let hit = a
        .doc
        .unvoiced
        .iter()
        .any(|(s, e)| *s / fr < 0.66 && *e / fr > 0.39);
    assert!(hit, "got spans {:?} at {fr} fps", a.doc.unvoiced);

    // The trailing silence must not be shaded as a consonant: it is
    // nothing at all, and calling it a sibilant makes the bands
    // meaningless.
    let in_silence = a.doc.unvoiced.iter().any(|(s, _)| *s / fr > 1.1);
    assert!(
        !in_silence,
        "silence is not a sibilant: {:?}",
        a.doc.unvoiced
    );
}

#[test]
fn stereo_is_summed_before_analysis() {
    let mono = tone(60.0, 0.5, 0.0);
    let stereo: Vec<f64> = mono.iter().flat_map(|&s| [s, s]).collect();
    let summed = to_mono(&stereo, 2);
    assert_eq!(summed.len(), mono.len());
    assert!(summed.iter().zip(&mono).all(|(a, b)| (a - b).abs() < 1e-12));
    // A one-channel buffer passes through rather than being halved.
    assert_eq!(to_mono(&mono, 1).len(), mono.len());
}

#[test]
fn an_empty_or_silent_take_analyses_to_nothing_rather_than_panicking() {
    for audio in [Vec::new(), silence(0.5)] {
        let a = analyze_take(&audio, SR, TakeConfig::default());
        assert!(a.doc.notes.is_empty());
    }
}

#[test]
fn editing_and_committing_moves_the_blob_the_editor_moved() {
    let audio = tone(60.0, 0.8, 0.0);
    let mut a = analyze_take(&audio, SR, TakeConfig::default());
    assert!(!a.doc.notes.is_empty());

    // Transpose up a tone, the way a body drag would.
    a.doc.note_mut(NoteId(1)).unwrap().row += 2;
    a.commit();

    assert!(
        (a.pitch.blobs[0].center_midi - 62.0).abs() < 0.35,
        "got {}",
        a.pitch.blobs[0].center_midi
    );
    // No timing edit, so no warp beyond the identity.
    assert!(a.pitch.markers.iter().all(|m| m.d_time.abs() < 1e-6));
}

#[test]
fn a_timing_edit_produces_a_non_identity_warp() {
    let audio = cat(&[tone(60.0, 0.4, 0.0), silence(0.1), tone(64.0, 0.4, 0.0)]);
    let mut a = analyze_take(&audio, SR, TakeConfig::default());
    assert!(a.doc.notes.len() >= 2);

    let id = a.doc.notes[1].id;
    let n = a.doc.note_mut(id).unwrap();
    n.start += 5.0;
    n.end += 5.0;
    a.commit();

    assert!(
        a.pitch.markers.iter().any(|m| m.d_time.abs() > 1.0),
        "the move reached the renderer as a warp"
    );
}

#[cfg(feature = "render")]
#[test]
fn a_transposed_note_renders_at_its_new_pitch() {
    // The whole loop: analyse, edit, commit, resynthesise, and analyse
    // the result. If this holds, the surface is connected to audio.
    let audio = tone(60.0, 1.0, 0.0);
    let mut a = analyze_take(&audio, SR, TakeConfig::default());
    assert!(!a.doc.notes.is_empty());

    a.doc.note_mut(NoteId(1)).unwrap().row += 4;
    a.commit();
    let out = a.render(&audio);

    assert_eq!(out.len(), audio.len(), "the render keeps the take's length");
    let again = analyze_take(&out, SR, TakeConfig::default());
    assert!(
        !again.doc.notes.is_empty(),
        "the rendered audio still sings"
    );
    let got = sounding(&again.doc, NoteId(1));
    assert!(
        (got - 64.0).abs() < 0.5,
        "asked for 64, the rendered audio measures {got}"
    );
}
