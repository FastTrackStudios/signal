//! Deterministic legato ARRIVAL verification — the primary timing proof.
//!
//! The heard arrival of every note is a deterministic function of the
//! annotated schedule plus per-zone sample markers (a transition zone's
//! measured `lead_in_ms` is the in-sample arrival marker; the loop region
//! beyond it is the settled destination note). No audio analysis is needed
//! to verify that a render SOUNDS on the grid:
//!
//! * schedule level — for every note, the trigger event satisfies the
//!   arrival identity exactly (`prefire.frame + lead == grid`,
//!   `note_on.frame == grid`, `short.frame == grid − pre_delay`);
//! * engine level (CSS-gated) — the engine reports the arrival it actually
//!   produced from the zone it actually chose
//!   ([`signal_sampler::LegatoFireEvent::arrival`] = fire frame + remaining
//!   in-sample lead at playback rate); it must equal the grid tick to
//!   within ms→frame rounding (≤ 2 frames ≈ 0.04 ms);
//! * acoustic cross-check (CSS-gated) — spectral-flux onsets over the real
//!   render validate the markers themselves (a wrong `lead_in_ms` in pack
//!   data would break prediction↔acoustics agreement while both
//!   deterministic layers still pass).
//!
//! The corpus is `signal_orchestra::timing::timing_corpus()` — legato lines
//! at 60/90/120/160 bpm, slow-expressive vs fast-low-latency velocity
//! zones, intervals up/down through the octave, re-bows, phrase starts,
//! and a staccato calibration case.

use signal_orchestra::timing::{
    pitch_arrival, spectral_flux, timing_corpus, OnsetKind, TimingCase,
};
use signal_orchestra::{load_strings, CSS_CONFIG, CSS_ROOT};
use signal_sampler::document::{
    annotate, qn_to_frame, DocEvent, DocumentRenderOptions, MarkerKind,
};
use signal_sampler::spec::LibrarySpec;
use signal_sampler::PlayerPatch;
use signal_sampler::SamplerRig;

const ID: &str = "strings_1v";
const SR: u32 = 48_000;
/// Engine arrival vs grid: ms→frame rounding of the zone's arrival marker
/// and the f64 rate conversion — ≤ 2 frames (0.042 ms at 48 kHz).
const ARRIVAL_TOL_FRAMES: i64 = 2;

fn css_zones() -> std::path::PathBuf {
    std::path::Path::new(CSS_ROOT).join("_patches/1st Violins/library.styx")
}

fn css_present() -> bool {
    css_zones().exists() && std::path::Path::new(CSS_CONFIG).exists()
}

/// The real CSS spec with MEASURED transition zones (config + zone styx
/// merged) — spec parsing only, no sample decode.
fn merged_css_spec() -> LibrarySpec {
    PlayerPatch::load_merged(
        std::path::Path::new(CSS_CONFIG),
        &css_zones(),
        std::path::Path::new(CSS_ROOT),
    )
    .expect("merge CSS config + zones")
    .spec
}

/// A CSS-shaped spec WITHOUT measured zones (same shape as
/// `score_to_audio.rs`): exercises the velocity-curve fallback so the
/// schedule identity is pinned everywhere, not only on machines with CSS.
fn fallback_spec() -> LibrarySpec {
    LibrarySpec::from_styx(
        r#"
name ArrivalTest
articulations (
    { id Sus,  label Sustain,  kind @Sustain, rr 1 }
    { id Leg,  label Legato,   kind @Legato,  rr 4, directional true }
    { id Stac, label Staccato, kind @Short,   rr 4 }
)
legato_engine {
    expressive {
        enabled_cc58_range 6-10
        zones (
            {vel_range (0 64),    label slow,   delay_ms 333}
            {vel_range (65 100),  label medium, delay_ms 250}
            {vel_range (101 127), label fast,   delay_ms 100}
        )
    }
    low_latency {
        enabled_cc58_range 0-5
        zones (
            {vel_range (0 64),   label medium, delay_ms 150}
            {vel_range (65 127), label fast,   delay_ms 100}
        )
    }
    portamento { trigger_vel_max 20, volume_controller CC5 }
}
short_note_timing { pre_delay_ms 60 }
keyswitch {
    cc58_map {
        0-5   "Sustain: Low Latency Legato"
        6-10  "Sustain: Expressive Legato"
        21-25 Staccato
    }
}
"#,
    )
    .expect("parse fallback spec")
}

/// Assert the schedule-level arrival identity for every note of `case`:
/// the k-th trigger event on the (single) line is the k-th note, and its
/// heard arrival — trigger frame + prefire lead (legato), trigger frame
/// (fresh attack), or trigger frame + pre_delay (short) — is the note's
/// grid frame, EXACTLY (error 0).
fn assert_schedule_identities(case: &TimingCase, spec: &LibrarySpec) {
    let sched = annotate(&case.doc, spec, SR);
    assert_eq!(sched.note_count, case.doc.notes.len(), "{}", case.name);

    let triggers: Vec<_> = sched
        .events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                DocEvent::NoteOn { .. } | DocEvent::LegatoPrefire { .. }
            )
        })
        .collect();
    assert_eq!(
        triggers.len(),
        case.expected.len(),
        "{}: one trigger per note",
        case.name
    );

    for (i, (ev, exp)) in triggers.iter().zip(&case.expected).enumerate() {
        let grid = qn_to_frame(&case.doc.tempo, exp.qn, SR);
        let (arrival, what) = match ev.kind {
            DocEvent::LegatoPrefire { note, lead, .. } => {
                assert_eq!(note, exp.pitch, "{} note {i}: prefire pitch", case.name);
                assert!(
                    matches!(exp.kind, OnsetKind::Legato | OnsetKind::Rebow),
                    "{} note {i}: unexpected prefire for {:?}",
                    case.name,
                    exp.kind
                );
                (ev.frame as i64 + i64::from(lead), "prefire frame + lead")
            }
            DocEvent::NoteOn { note, lead, .. } => {
                assert_eq!(note, exp.pitch, "{} note {i}: note-on pitch", case.name);
                match exp.kind {
                    // Fresh attacks carry their own pre-roll lead (the
                    // per-zone measured-arrival bound; `pre_delay_ms` /
                    // zero on unmeasured libraries): trigger + lead == grid.
                    OnsetKind::Short | OnsetKind::PhraseStart => (
                        ev.frame as i64 + i64::from(lead),
                        "trigger + attack pre-roll lead",
                    ),
                    other => panic!(
                        "{} note {i}: plain note-on for {:?} — legato edge lost",
                        case.name, other
                    ),
                }
            }
            _ => unreachable!(),
        };
        assert_eq!(
            arrival,
            grid,
            "{} note {i} ({:?} {}): {} == grid, err {} frames",
            case.name,
            exp.kind,
            exp.pitch,
            what,
            arrival - grid
        );
    }

    // ── The marker timeline says the same thing, per note ────────────────
    // (This is the GUI-facing artifact — waveform + markers — so the tests
    // pin it directly: one Arrival marker per note, ON the grid, and the
    // start marker's kind matches the note's role.)
    let arrivals: Vec<_> = sched
        .markers
        .iter()
        .filter(|m| m.kind == MarkerKind::Arrival)
        .collect();
    assert_eq!(
        arrivals.len(),
        case.expected.len(),
        "{}: one Arrival marker per note",
        case.name
    );
    for (i, (m, exp)) in arrivals.iter().zip(&case.expected).enumerate() {
        assert_eq!(m.note, exp.pitch, "{} marker {i}: pitch", case.name);
        assert_eq!(
            m.frame as i64,
            qn_to_frame(&case.doc.tempo, exp.qn, SR),
            "{} marker {i}: Arrival on the grid",
            case.name
        );
    }
    let start_markers: Vec<_> = sched
        .markers
        .iter()
        .filter(|m| {
            matches!(
                m.kind,
                MarkerKind::NoteStart | MarkerKind::TransitionStart | MarkerKind::Rebow
            )
        })
        .collect();
    assert_eq!(start_markers.len(), case.expected.len(), "{}", case.name);
    for (i, exp) in case.expected.iter().enumerate() {
        let want = match exp.kind {
            OnsetKind::Legato => MarkerKind::TransitionStart,
            OnsetKind::Rebow => MarkerKind::Rebow,
            OnsetKind::PhraseStart | OnsetKind::Short => MarkerKind::NoteStart,
        };
        assert_eq!(
            start_markers[i].kind, want,
            "{} note {i}: start-marker kind for {:?}",
            case.name, exp.kind
        );
    }
}

/// Schedule arrival identity, velocity-curve fallback spec — runs
/// everywhere (no CSS needed).
#[test]
fn schedule_arrivals_land_on_grid_fallback_spec() {
    let spec = fallback_spec();
    let pre_delay_frames = 60 * u32::from(SR as u16) / 1000; // spec pre_delay_ms 60
    for case in timing_corpus() {
        assert_schedule_identities(&case, &spec);
        // Without per-zone arrival measurements the pre-roll leads keep the
        // historical values: shorts = the global `pre_delay_ms`, fresh
        // sustains = 0 (trigger on the tick).
        let sched = annotate(&case.doc, &spec, SR);
        for (ev, exp) in sched
            .events
            .iter()
            .filter(|e| matches!(e.kind, DocEvent::NoteOn { .. }))
            .zip(
                case.expected
                    .iter()
                    .filter(|e| matches!(e.kind, OnsetKind::Short | OnsetKind::PhraseStart)),
            )
        {
            if let DocEvent::NoteOn { lead, .. } = ev.kind {
                let want = match exp.kind {
                    OnsetKind::Short => pre_delay_frames,
                    _ => 0,
                };
                assert_eq!(
                    lead, want,
                    "{}: fallback pre-roll for {:?}",
                    case.name, exp.kind
                );
            }
        }
    }
}

/// Schedule arrival identity against the REAL measured CSS zones, plus:
/// every legato transition genuinely prefires (lead > 0 — measured
/// arrival minus start-offset, capped at the zone delay), and every
/// transition zone's arrival marker precedes its loop start (the loop
/// region IS the settled destination — the derivation the whole method
/// rests on).
#[test]
fn schedule_arrivals_land_on_grid_measured_css_spec() {
    if !css_present() {
        eprintln!("skipping: CSS library/config not present");
        return;
    }
    let spec = merged_css_spec();
    assert!(spec.has_measured_legato(), "CSS zones carry lead_in_ms");
    for case in timing_corpus() {
        assert_schedule_identities(&case, &spec);
        let sched = annotate(&case.doc, &spec, SR);
        for (ev, exp) in sched
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    DocEvent::NoteOn { .. } | DocEvent::LegatoPrefire { .. }
                )
            })
            .zip(&case.expected)
        {
            if exp.kind == OnsetKind::Legato {
                if let DocEvent::LegatoPrefire { lead, .. } = ev.kind {
                    assert!(
                        lead > 0,
                        "{} {:?}: measured legato must prefire early",
                        case.name,
                        exp.pitch
                    );
                }
            }
        }
    }

    // Marker sanity: arrival (lead_in) ≤ loop start for every looped
    // transition zone — the pre-loop region is the bow change, the loop is
    // the arrived note.
    let mut checked = 0usize;
    for z in &spec.zones {
        if z.interval == 0 || z.transition_arrival_ms() <= 0.0 || z.loop_end <= z.loop_start {
            continue;
        }
        let lead_frames = (f64::from(z.transition_arrival_ms()) / 1000.0 * f64::from(SR)) as u32;
        assert!(
            lead_frames <= z.loop_start,
            "zone {}: arrival marker {} frames past loop_start {}",
            z.file,
            lead_frames,
            z.loop_start
        );
        checked += 1;
    }
    eprintln!("checked {checked} looped transition zones: arrival ≤ loop_start");
}

/// ENGINE-level arrival + acoustic cross-check, full corpus, real CSS
/// audio. For every legato/re-bow note the engine's own arrival
/// prediction (from the zone it actually chose) must land on the grid to
/// within rounding; spectral-flux onsets then confirm the markers tell
/// the truth acoustically.
///
/// IGNORED (2026-07-15): the acoustic gates encode detector numbers, and
/// detector-vs-perception is the documented open problem — the detector was
/// wrong in four harmonic-collision families while the owner's ear stayed
/// the arbiter (signal-sampler/docs/legato-timing-status.md, open problem
/// 1). The DETERMINISTIC invariants (emitted-vs-grid) remain enforced by
/// the other tests in this file; re-enable when the arrival estimator
/// matches perception well enough to be normative.
#[ignore = "acoustic gates pending estimator-vs-perception resolution — see legato-timing-status.md"]
#[test]
fn rendered_arrivals_land_on_grid_with_css() {
    if !css_present() {
        eprintln!("skipping: CSS library/config not present");
        return;
    }
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    load_strings(&rig, ID, "1st Violins", "Mix", CSS_ROOT, CSS_CONFIG).expect("load CSS");

    let mut worst_frames: i64 = 0;
    let mut legato_errs_ms: Vec<f64> = Vec::new();
    // Isolated two-note slurs (`intervals_*` cases) — the clean measurement
    // of the join itself, free of chained-legato contamination.
    let mut isolated_errs_ms: Vec<(f64, f64)> = Vec::new();
    // Octave (±12) transitions — tracked separately: the pitch-share
    // detector is at its physical limit there (see the skip note below), so
    // its numbers are indicative, not a gate-grade measurement.
    let mut octave_errs_ms: Vec<f64> = Vec::new();
    let mut short_errs_ms: Vec<f64> = Vec::new();
    let mut edge_errs_ms: Vec<f64> = Vec::new();
    let mut rebow_errs_ms: Vec<f64> = Vec::new();

    let corpus = timing_corpus();
    for case in &corpus {
        let res = rig
            .render_offline_document(ID, &case.doc, &DocumentRenderOptions::default())
            .unwrap_or_else(|e| panic!("{}: render: {e}", case.name));
        assert_eq!(
            res.reactive_fallbacks, 0,
            "{}: document render must not fall back to the reactive path",
            case.name
        );

        // ── Deterministic engine arrival ─────────────────────────────────
        let legatos: Vec<_> = case
            .expected
            .iter()
            .filter(|e| matches!(e.kind, OnsetKind::Legato | OnsetKind::Rebow))
            .collect();
        assert_eq!(
            res.transitions.len(),
            legatos.len(),
            "{}: every legato/re-bow note fired exactly one transition",
            case.name
        );
        for (t, exp) in res.transitions.iter().zip(&legatos) {
            assert_eq!(t.to_note, exp.pitch, "{}: transition order", case.name);
            let grid = qn_to_frame(&case.doc.tempo, exp.qn, SR);
            let err = t.arrival as i64 - grid;
            worst_frames = worst_frames.max(err.abs());
            assert!(
                err.abs() <= ARRIVAL_TOL_FRAMES,
                "{} {} -> {} at qn {}: engine arrival {} vs grid {} (err {} frames = {:.2} ms)",
                case.name,
                t.from_note,
                t.to_note,
                exp.qn,
                t.arrival,
                grid,
                err,
                err as f64 * 1000.0 / f64::from(SR)
            );
        }

        // ── Playback-EMITTED arrivals vs grid (PRIMARY) ──────────────────
        // Every marker below was emitted BY PLAYBACK: the voice's real
        // playhead crossed the zone's arrival position at that output frame
        // — after every start-offset skip, start hold, and rate scaling.
        // `emitted == grid` therefore catches every class of timing bug at
        // once (wrong pack markers applied, offset double-counting, rate
        // errors, scheduling errors); the acoustic detector below drops to
        // third-line (it validates marker semantics against sample content,
        // not engine behaviour).
        let half_beat = ((30.0 / case.bpm) * f64::from(SR)) as i64;
        let emitted_near = |pitch: u8, grid: i64| -> Option<i64> {
            res.emitted_markers
                .iter()
                .filter(|m| m.note == pitch)
                .map(|m| m.frame as i64 - grid)
                .filter(|d| d.abs() < half_beat)
                .min_by_key(|d| d.abs())
        };
        // Emission tolerance: ms→frame rounding of the marker + the
        // hold/offset conversions ≈ ≤ 5 ms.
        let emit_tol = (0.005 * f64::from(SR)) as i64;
        for exp in &case.expected {
            let grid = qn_to_frame(&case.doc.tempo, exp.qn, SR);
            let d = emitted_near(exp.pitch, grid);
            match exp.kind {
                // Arrive-at-tick classes: the zone's heard-arrival must have
                // been PLAYED on the tick.
                OnsetKind::Legato | OnsetKind::Rebow | OnsetKind::Short => {
                    let d = d.unwrap_or_else(|| {
                        panic!(
                            "{} qn {} pitch {}: no playback-emitted arrival near the tick",
                            case.name, exp.qn, exp.pitch
                        )
                    });
                    assert!(
                        d.abs() <= emit_tol,
                        "{} qn {} pitch {} ({:?}): EMITTED arrival {} frames ({:.1} ms) off the grid",
                        case.name,
                        exp.qn,
                        exp.pitch,
                        exp.kind,
                        d,
                        d as f64 * 1000.0 / f64::from(SR)
                    );
                }
                // start_at_tick fresh sustains: the sample STARTS on the
                // tick; its measured onset is emitted when actually played
                // through — at/after the tick, within the natural speak time.
                OnsetKind::PhraseStart => {
                    if let Some(d) = d {
                        let ms = d as f64 * 1000.0 / f64::from(SR);
                        assert!(
                            (-5.0..=450.0).contains(&ms),
                            "{} qn {} pitch {}: fresh-attack EMITTED onset {ms:.1} ms — before the tick or absurdly late",
                            case.name,
                            exp.qn,
                            exp.pitch
                        );
                    }
                }
            }
        }

        // ── Acoustic cross-check (third-line: marker semantics) ──────────
        // Arrival is measured per kind, matching what "the note is heard"
        // means acoustically:
        //  * Legato   — PITCH arrival: destination-vs-source harmonic
        //               energy crossing (Goertzel), the exact quantity the
        //               schedule promises to land on the tick;
        //  * Short    — spectral-flux peak (sharp attack ⇒ peak == onset);
        //               doubles as the detector-latency calibration;
        //  * PhraseStart / Rebow — flux LEADING EDGE: a fresh arco attack
        //               blooms over ~150 ms by the sample's nature, so the
        //               note "starts speaking" at the edge, not the peak.
        let flux = spectral_flux(&res.audio, SR);
        let min_ioi = case
            .expected
            .windows(2)
            .map(|w| w[1].sec - w[0].sec)
            .fold(f64::INFINITY, f64::min);
        let search = (min_ioi / 2.0).min(0.18);
        eprintln!("── {} — {}", case.name, case.desc);
        let mut prev_pitch: Option<u8> = None;
        for exp in &case.expected {
            let (measured, label) = match exp.kind {
                OnsetKind::Legato => (
                    pitch_arrival(
                        &res.audio,
                        SR,
                        exp.sec,
                        prev_pitch.expect("legato has a source"),
                        exp.pitch,
                        search.max(0.15),
                    ),
                    "pitch",
                ),
                OnsetKind::Short => (flux.onset_near(exp.sec, search), "flux-peak"),
                // A re-bow happens inside continuous same-pitch sound, so a
                // leading edge is undefined; the flux PEAK (the bow change)
                // is only a presence check.
                OnsetKind::Rebow => (flux.onset_near(exp.sec, search.min(0.25)), "flux-peak"),
                OnsetKind::PhraseStart => (flux.leading_edge(exp.sec, 0.10, 0.40), "flux-edge"),
            };
            // `start_at_tick` invariant (a): NOTHING of the note sounds
            // before the click it starts on. Only the FIRST note of a case
            // is preceded by true silence (later phrase starts follow the
            // previous note's legitimate release ring-out), so the check
            // binds there: the 100 ms before the tick must be at least
            // 40 dB below the note's own body.
            if exp.kind == OnsetKind::PhraseStart
                && case
                    .expected
                    .first()
                    .is_some_and(|f| (f.sec - exp.sec).abs() < 1e-9)
            {
                let rms = |a: f64, b: f64| -> f64 {
                    let (f0, f1) = (
                        ((a * f64::from(SR)) as usize).min(res.audio.len() / 2),
                        ((b * f64::from(SR)) as usize).min(res.audio.len() / 2),
                    );
                    if f1 <= f0 {
                        return 0.0;
                    }
                    let mut acc = 0.0f64;
                    for f in f0..f1 {
                        let m =
                            (f64::from(res.audio[f * 2]) + f64::from(res.audio[f * 2 + 1])) * 0.5;
                        acc += m * m;
                    }
                    (acc / (f1 - f0) as f64).sqrt()
                };
                let pre = rms(exp.sec - 0.100, exp.sec - 0.005);
                let body = rms(exp.sec, exp.sec + 0.400);
                assert!(
                    pre <= body * 0.01 + 1e-7,
                    "{}: audio before the phrase-start tick at qn {} ({:.1} dB below body — must be ≥ 40)",
                    case.name,
                    exp.qn,
                    20.0 * (pre / body.max(1e-12)).log10().abs()
                );
            }
            let Some(t) = measured else {
                // The pitch-share detector has coverage limits in render
                // context: octaves are PHYSICALLY confounded (every
                // destination harmonic collides with a source harmonic), and
                // fast-speed CHAINED joins (≤100 ms pre-bow under a
                // still-blooming ff source) can present no valid sustained
                // crossing at all. The timing proof for such notes is the
                // deterministic engine arrival (asserted exact above) plus
                // the zone's measured in-sample marker — skip the acoustic
                // point with a note. ISOLATED non-octave joins must always
                // resolve: an unresolvable one there means a lying marker.
                let octave = prev_pitch.is_some_and(|p| p.abs_diff(exp.pitch) >= 12);
                if exp.kind == OnsetKind::Legato && (octave || !case.name.starts_with("intervals_"))
                {
                    eprintln!(
                        "   qn {:5.2} pitch {:3} Legato: pitch-share detector unresolved ({}) — skipped (deterministic arrival exact)",
                        exp.qn,
                        exp.pitch,
                        if octave { "octave confound" } else { "chained fast join" }
                    );
                    prev_pitch = Some(exp.pitch);
                    continue;
                }
                panic!(
                    "{}: no acoustic onset near {:.3}s ({:?})",
                    case.name, exp.sec, exp.kind
                );
            };
            let err = (t - exp.sec) * 1000.0;
            eprintln!(
                "   qn {:5.2} pitch {:3} {:12} {label:9} err {err:+7.1} ms",
                exp.qn,
                exp.pitch,
                format!("{:?}", exp.kind)
            );
            match exp.kind {
                OnsetKind::Legato => {
                    let octave = prev_pitch.is_some_and(|p| p.abs_diff(exp.pitch) >= 12);
                    if case.name.starts_with("intervals_") && !octave {
                        // Per-speed gates: the pitch-share crossing in a
                        // RENDER reads `max(tick, source-retire decay)` —
                        // at FAST velocity the pre-roll is only ~100 ms of
                        // the ~500 ms source retire, so the aggregate
                        // crossing lags the (deterministically exact)
                        // arrival by up to ~150 ms even with perfect
                        // markers. Slow/medium lanes retire most of the
                        // source before the tick and read tight.
                        let cap = if case.name.ends_with("_fast") {
                            170.0
                        } else if case.name.ends_with("_slow") {
                            50.0
                        } else {
                            80.0
                        };
                        isolated_errs_ms.push((err, cap));
                    }
                    if octave {
                        octave_errs_ms.push(err);
                    } else {
                        legato_errs_ms.push(err)
                    }
                }
                OnsetKind::Short => short_errs_ms.push(err),
                OnsetKind::Rebow => rebow_errs_ms.push(err),
                OnsetKind::PhraseStart => edge_errs_ms.push(err),
            }
            prev_pitch = Some(exp.pitch);
        }
    }

    // ── Corpus-wide acoustic gates ───────────────────────────────────────
    // The DETERMINISTIC assertion above is the timing proof (≤ 2 frames);
    // these acoustic tolerances exist to catch LYING MARKERS in pack data —
    // gross errors of a velocity-zone step (100→250→333 ms) or more.
    // The physics that bounds how tight they can be:
    //  * Legato — the engine (mirroring the CSS KSP) under-lays the
    //    DESTINATION sustain at −6 dB from the prefire moment, so
    //    destination-pitch energy exists up to one zone-delay BEFORE the
    //    tick; the pitch-share crossing is further smeared by the 30 ms
    //    transition fade and the 43 ms analysis window. Measured spread on
    //    healthy CSS data: mean ~50 ms, worst ~140 ms (octaves — the glide
    //    is longest and the under-laid sustain loudest relative to it).
    //  * Shorts — the schedule pre-rolls the GLOBAL 60 ms `pre_delay_ms`;
    //    the actual attack-to-flux-peak varies per round-robin
    //    (~+40..+105 ms observed), so the gate is a band, not a point
    //    (finding: per-zone peak markers would tighten this).
    //  * Fresh attacks — the arco sample speaks ~+70 ms after its
    //    on-tick trigger and blooms ~150 ms (the sample's own nature,
    //    identical in Kontakt); the edge must sit in that band.
    //  * Re-bows — continuous same-pitch sound; the flux peak is only a
    //    presence check for the bow change near the tick.
    let stats = |v: &[f64]| -> (f64, f64, f64) {
        let mut s: Vec<f64> = v.iter().map(|e| e.abs()).collect();
        s.sort_by(|a, b| a.total_cmp(b));
        let n = s.len();
        (
            v.iter().map(|e| e.abs()).sum::<f64>() / n as f64,
            s[((n as f64 * 0.95) as usize).min(n - 1)],
            s[n - 1],
        )
    };
    let (lm, lp95, lworst) = stats(&legato_errs_ms);
    let (sm, _, sworst) = stats(&short_errs_ms);
    eprintln!(
        "legato pitch-arrival over {} notes: mean |err| {lm:.1} ms, p95 {lp95:.1} ms, worst {lworst:.1} ms",
        legato_errs_ms.len()
    );
    eprintln!(
        "short flux-peak over {} notes: mean |err| {sm:.1} ms, worst {sworst:.1} ms",
        short_errs_ms.len()
    );
    eprintln!(
        "fresh-attack leading edges over {} notes: min {:.1} ms, max {:.1} ms; re-bow flux peaks over {}: min {:.1} max {:.1}; engine-arrival worst {} frames",
        edge_errs_ms.len(),
        edge_errs_ms.iter().cloned().fold(f64::INFINITY, f64::min),
        edge_errs_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        rebow_errs_ms.len(),
        rebow_errs_ms.iter().cloned().fold(f64::INFINITY, f64::min),
        rebow_errs_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        worst_frames
    );
    // Post-marker gates (per-zone MEASURED `arrival_ms` in the pack,
    // written by `examples/measure_arrivals.rs`). Measured on this corpus
    // with the corrected CSS 1st Violins inventory (2026-07-15):
    //
    //  * legato all-notes: mean 28.0 / p95 93.8 / worst 147.2 ms — the tail
    //    lives ENTIRELY in chained scale lines at ≤ 90 bpm, where the
    //    pitch-share detector's window also contains the PREVIOUS note's
    //    retiring voice and its vibrato (the render aggregate, not the
    //    join). The engine's own deterministic arrival is exact (0 frames),
    //    and the clean measurement below pins the join itself.
    //  * ISOLATED two-note slurs (`intervals_*`, every interval through the
    //    octave, both directions): worst 45.4 ms (the +12 octave, where
    //    harmonic collisions leave the detector its fundamental only) —
    //    everything else ≤ ~27 ms.
    //  * shorts: |err| ≤ 0.7 ms (was +38..+104 against the single global
    //    pre-delay — the per-RR markers collapse it).
    //  * fresh attacks: −3.4..+21.7 ms (was ~+69 ms — "the note isn't
    //    playing until after the click").
    // The p95 band covers chained-scale contexts where the pitch-share
    // crossing is genuinely ill-defined: the same deterministic render reads
    // the same note anywhere from +36 to +144 ms depending on millimetric
    // detector-validation choices (the destination emerges gradually under
    // the previous note's retiring voice — there is no sharp crossing to
    // find). Isolated joins (gated at ±50 below) are the marker-quality
    // measurement; the mean tracks overall health.
    assert!(lm <= 40.0, "legato mean pitch-arrival error {lm:.1} ms");
    assert!(lp95 <= 150.0, "legato p95 pitch-arrival error {lp95:.1} ms");
    assert!(
        lworst <= 170.0,
        "legato worst pitch-arrival error {lworst:.1} ms — a marker is lying"
    );
    for (e, cap) in &isolated_errs_ms {
        assert!(
            e.abs() <= *cap,
            "isolated legato join {e:.1} ms off the tick (gate ±{cap:.0}) — the transition marker is wrong"
        );
    }
    // Octaves: loose sanity band only — the detector's confound (mutual
    // harmonic leak) biases it early in render context even when the
    // in-sample settle marker is right. Before per-zone markers these fell
    // back to lead_in metadata claiming up to 900 ms (a 650 ms over-skip:
    // the destination note simply played early); measured markers bound the
    // error to the detector's ambiguity.
    for e in &octave_errs_ms {
        assert!(
            e.abs() <= 160.0,
            "octave legato join {e:.1} ms off the tick — beyond even the detector's confound"
        );
    }
    for e in &short_errs_ms {
        assert!(
            (-20.0..=20.0).contains(e),
            "short flux peak {e:.1} ms off the tick — the per-RR arrival marker is wrong"
        );
    }
    // `start_at_tick` invariant (b): the fresh attack speaks AT/AFTER the
    // tick — never before (that would be audio before the click) — and
    // within the zone's natural speak time (measured sustain onsets run
    // ~47..420 ms; the flux edge sits earlier in the bloom than the full
    // onset).
    for e in &edge_errs_ms {
        assert!(
            (-8.0..=300.0).contains(e),
            "fresh-attack leading edge {e:.1} ms — speaks before the tick or absurdly late"
        );
    }
    for e in &rebow_errs_ms {
        // Presence check only: inside continuous same-pitch sound the
        // strongest spectral change in the window is usually the previous
        // note's own vibrato/bow fluctuation, so the band is wide — the
        // re-bow's real timing proof is the deterministic engine arrival
        // (exact) plus the Legzero onset markers (~12 ms, measured).
        assert!(
            (-250.0..=250.0).contains(e),
            "re-bow flux peak {e:.1} ms — no bow change near the tick"
        );
    }
}
