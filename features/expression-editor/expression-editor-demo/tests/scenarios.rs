//! The four scenarios #149 is reached by.
//!
//! These are not unit tests. Every mechanism below already has close
//! coverage in its own crate; what these assert is that the mechanisms
//! meet **on one project made of real material** — a 48 kHz vocal take
//! and a 44.1 kHz drum multitrack in the same document, a guitar roll
//! whose row space is not the drum roll's, a synthesized MPE fixture
//! next to three tracks of recorded audio.
//!
//! Each scenario skips on its own material, loudly, so a machine with
//! half the downloads still reports on the other half instead of going
//! uniformly red.

use expression_editor_demo::{Demo, TrackRole, build, material_or_skip};

/// Build once per test. Cheap: it reads directory listings, not audio.
fn demo() -> Demo {
    let m = expression_editor_demo::Material::discover().expect("checked by material_or_skip");
    build(&m)
}

/// Report and bail when this machine lacks one scenario's material.
macro_rules! role_or_skip {
    ($demo:expr, $role:expr) => {
        if !$demo.covers($role) {
            eprintln!("SKIP: no material for {:?} on this machine", $role);
            return;
        }
    };
}

#[test]
fn the_demo_project_holds_all_four_scenarios() {
    let _m = material_or_skip!();
    let d = demo();

    // The point of the project is that the four coexist. Naming which
    // are missing beats a bare count, because the usual cause is one
    // download rather than a broken build.
    let missing: Vec<_> = [
        TrackRole::Vocal,
        TrackRole::Drum,
        TrackRole::Mpe,
        TrackRole::Guitar,
    ]
    .into_iter()
    .filter(|r| !d.covers(*r))
    .collect();
    assert!(
        missing.is_empty(),
        "demo project is missing material for {missing:?} (song: {})",
        d.song
    );

    // Every track the roles point at must actually be in the document,
    // or the roles are lying about what was assembled.
    for t in &d.tracks {
        assert!(
            d.document.tracks.iter().any(|n| n.id == t.id),
            "role track {} is not in the document",
            t.name
        );
    }
}

/// Scenario 1 — a real vocal take, four concerns, one composite.
#[test]
fn scenario_1_a_vocal_take_composites_four_envelopes() {
    let _m = material_or_skip!();
    let d = demo();
    role_or_skip!(d, TrackRole::Vocal);

    use expression_editor_audio::dynamics::{
        BreathConfig, CompressorConfig, DynamicsConfig, GateConfig, SibilanceConfig, analyse,
    };
    use expression_editor_audio::frames::frame_features;

    let track = d.by_role(TrackRole::Vocal)[0];
    let path = track.source.as_ref().expect("a vocal track has a file");
    let (samples, sample_rate) =
        expression_editor_corpus::wav::read_channel(path, Some(0)).expect("read the vocal take");
    assert!(
        !samples.is_empty(),
        "the vocal take {} decoded to nothing",
        track.name
    );

    // Frame the take the way the editor does.
    let hop = (sample_rate / 100.0) as usize;
    let window = hop * 2;
    let frames = samples.len() / hop.max(1);
    let features = frame_features(&samples, sample_rate, window, hop, frames);
    // Voiced-ness is not what this scenario is testing; the four
    // detectors each take their own view of the signal and the gate is
    // the only one that consults this.
    let voiced: Vec<bool> = features.iter().map(|f| f.rms > 1e-4).collect();

    let cfg = DynamicsConfig {
        gate: Some(GateConfig::default()),
        compressor: Some(CompressorConfig::default()),
        breath: Some(BreathConfig::default()),
        sibilance: Some(SibilanceConfig::default()),
    };
    let dynamics = analyse(&features, &voiced, sample_rate / hop as f64, &cfg);

    // The scenario's actual claim: four separate curves, and one
    // composite that is their sum in dB.
    let combined = dynamics.combined(features.len());
    assert_eq!(
        combined.len(),
        features.len(),
        "the composite must cover the whole take"
    );

    let any =
        |v: &[expression_editor_audio::dynamics::GainPoint]| v.iter().any(|p| p.db.abs() > 1e-9);
    assert!(
        any(&dynamics.gate)
            || any(&dynamics.compressor)
            || any(&dynamics.breath)
            || any(&dynamics.sibilance),
        "no detector found anything in a real vocal take — the four \
         envelopes would all be flat, which makes the composite vacuous"
    );

    // Editing one concern must move the composite. This is the property
    // that makes them four envelopes rather than one: recomputing after
    // a change to a single curve has to be visible.
    let before = combined.clone();
    let mut edited = dynamics.clone();
    for p in &mut edited.sibilance {
        p.db -= 6.0;
    }
    let after = edited.combined(features.len());
    if any(&dynamics.sibilance) {
        assert!(
            before
                .iter()
                .zip(&after)
                .any(|(a, b)| (a.db - b.db).abs() > 1e-6),
            "editing the sibilance curve did not change the composite"
        );
    }
}

/// Scenario 2 — multitracked drums, quantized to the grid.
#[test]
fn scenario_2_a_drum_multitrack_quantizes_to_the_grid() {
    let _m = material_or_skip!();
    let d = demo();
    role_or_skip!(d, TrackRole::Drum);

    use expression_editor_audio::onsets;
    use expression_editor_tools::quantize::{self, QuantizeConfig};

    let track = d.by_role(TrackRole::Drum)[0];
    let path = track.source.as_ref().expect("a drum track has a file");
    let (samples, sample_rate) =
        expression_editor_corpus::wav::read_channel(path, Some(0)).expect("read the close mic");

    let cfg = onsets::OnsetConfig::default();
    let hop = cfg.hop.max(1);
    let hits = onsets::detect(&samples, sample_rate, cfg);
    assert!(
        hits.len() > 4,
        "found only {} onsets in {} — a close mic of a real kit has \
         more than that, so either the read or the detector is wrong",
        hits.len(),
        track.name
    );

    // Quantize in seconds against a grid derived from the material
    // itself rather than an assumed tempo: this session is not
    // click-locked and a wrong grid would make the test assert on
    // nothing.
    // Onsets are reported in frames; the grid is in seconds. A bare
    // f64 is not `Timed` on purpose — the tool carries everything that
    // belongs to an event, so an event is what it must be given.
    let mut times: Vec<Hit> = hits
        .iter()
        .map(|h| Hit {
            at: h.frame as f64 * hop as f64 / sample_rate,
            strength: h.strength,
        })
        .collect();
    let grid = median_gap(&times.iter().map(|h| h.at).collect::<Vec<_>>());
    assert!(grid > 0.0, "could not derive a grid from the onsets");

    let qcfg = QuantizeConfig {
        grid,
        strength: 1.0,
        ..Default::default()
    };
    let plan = quantize::plan(&times, qcfg);
    let moved: Vec<usize> = plan.moves.iter().map(|m| m.index).collect();
    quantize::apply(&mut times, &plan);

    // Not "every hit lands on the grid" — that is not what the tool
    // promises. A division takes at most one event, so when two hits
    // want the same one the weaker is deliberately left alone and
    // reported in `unmatched`. Asserting otherwise would be asserting
    // against the ghost-note rule.
    assert!(
        !moved.is_empty(),
        "the plan moved nothing out of {} onsets",
        times.len()
    );
    for i in &moved {
        let t = times[*i].at;
        let off = (t / grid).round() * grid - t;
        assert!(
            off.abs() < grid * 1e-6,
            "hit {i} was planned onto the grid but sits {off} off a \
             {grid}s grid after apply"
        );
    }

    // And the plan must be about this material rather than rejecting
    // most of it: a grid derived from the hits themselves should claim
    // a clear majority of them.
    let ratio = moved.len() as f64 / times.len() as f64;
    assert!(
        ratio > 0.5,
        "only {}/{} hits matched a {grid}s grid ({:.0}%) — the derived \
         grid does not describe this performance",
        moved.len(),
        times.len(),
        ratio * 100.0
    );
}

/// One detected drum hit, as the quantize tool wants to see it.
struct Hit {
    at: f64,
    /// The detector's own peak, used only for ranking two hits that
    /// want the same division.
    strength: f64,
}

impl expression_editor_tools::event::Timed for Hit {
    fn onset(&self) -> f64 {
        self.at
    }

    fn move_to(&mut self, to: f64) {
        self.at = to;
    }

    fn strength(&self) -> f64 {
        self.strength
    }
}

/// The gap the material itself suggests, so the grid is not an
/// assumption about a session nobody played to a click.
fn median_gap(times: &[f64]) -> f64 {
    let mut gaps: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.is_empty() {
        return 0.0;
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    gaps[gaps.len() / 2]
}

/// Scenario 3 — per-note bend, pressure and timbre.
#[test]
fn scenario_3_mpe_carries_bend_pressure_and_timbre() {
    let _m = material_or_skip!();
    let d = demo();
    role_or_skip!(d, TrackRole::Mpe);

    use expression_editor_core::Dimension;

    // Synthesized, not read: #159 found nothing usable on the machine
    // and #177 resolved it by generating the fixture.
    let doc = expression_editor_daw::fixture::doc();
    assert!(!doc.notes.is_empty(), "the MPE fixture has no notes");

    // The claim is per-note, so every dimension must vary *within* a
    // note rather than being a per-channel constant.
    for dim in [Dimension::Pitch, Dimension::Pressure, Dimension::Timbre] {
        let moving = doc
            .notes
            .iter()
            .filter(|n| {
                let c = n.curve(dim);
                c.points().len() > 1
                    && c.points()
                        .iter()
                        .any(|p| (p.value - c.points()[0].value).abs() > 1e-6)
            })
            .count();
        assert!(
            moving > 0,
            "no note has a moving {dim:?} curve — that is MPE with the \
             expression left out"
        );
    }

    // And the notes must overlap on distinct channels, or it is a
    // monophonic line wearing MPE's clothes.
    let overlapping = doc.notes.iter().any(|a| {
        doc.notes
            .iter()
            .any(|b| b.id != a.id && b.start < a.end && b.end > a.start)
    });
    assert!(overlapping, "no two fixture notes overlap");
}

/// Scenario 4 — a Guitar Pro file as a six-string roll with bend flow.
#[test]
fn scenario_4_a_guitar_pro_file_becomes_a_string_roll() {
    let _m = material_or_skip!();
    let d = demo();
    role_or_skip!(d, TrackRole::Guitar);

    let track = d.by_role(TrackRole::Guitar)[0];
    let path = track.source.as_ref().expect("the guitar track has a file");
    let imported = expression_editor_guitarpro::import_file(&path.to_string_lossy())
        .expect("import the transcription");

    assert!(
        imported.doc.notes.len() > 100,
        "only {} notes imported from {} — the inventory counted 568",
        imported.doc.notes.len(),
        track.name
    );

    // A guitar roll is a full MIDI roll: the row is the sounding pitch,
    // and the string rides on the note so the part can be coloured by
    // where it was played and the fret can be computed.
    let strings = imported.tuning.strings();
    assert_eq!(strings, 6, "expected a six-string tuning");
    let (lo, hi) = imported.doc.row_space.bounds();
    for n in &imported.doc.notes {
        assert!(
            n.row >= lo && n.row <= hi,
            "a note at pitch {} is off the neck ({lo}..={hi})",
            n.row
        );
        let s = n.string.expect("every imported note knows its string");
        let fret = expression_editor_core::rows::fret_of(n, &imported.tuning)
            .expect("a note with a string has a fret");
        assert!(
            (0..=imported.tuning.frets as i32).contains(&fret),
            "a note on string {s} computes to fret {fret}"
        );
    }

    // The part must actually move across the neck and across pitch —
    // either collapsing alone would pass the bounds above and be a
    // transcription of nothing.
    let used_strings: std::collections::BTreeSet<u8> =
        imported.doc.notes.iter().filter_map(|n| n.string).collect();
    let used_pitches: std::collections::BTreeSet<i32> =
        imported.doc.notes.iter().map(|n| n.row).collect();
    assert!(
        used_strings.len() >= 4,
        "only {} of {strings} strings are played: {used_strings:?}",
        used_strings.len()
    );
    assert!(
        used_pitches.len() > 12,
        "only {} distinct pitches — a pitch roll that flat is a lane, \
         not a melody",
        used_pitches.len()
    );

    // Bend flow is the part scenario 4 names specifically: the file has
    // 40 bends, and they have to arrive as curves rather than as a flag.
    let bent = imported
        .doc
        .notes
        .iter()
        .filter(|n| {
            let c = n.curve(expression_editor_core::Dimension::Pitch);
            c.points().len() > 1
        })
        .count();
    assert!(
        bent > 0,
        "not one bend survived the import — the roll would draw flat"
    );
}
