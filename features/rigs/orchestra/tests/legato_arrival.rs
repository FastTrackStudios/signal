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
    OnsetKind, TimingCase, pitch_arrival, spectral_flux, timing_corpus,
};
use signal_orchestra::{CSS_CONFIG, CSS_ROOT, load_strings};
use signal_sampler::PlayerPatch;
use signal_sampler::SamplerRig;
use signal_sampler::document::{
    DocEvent, DocumentRenderOptions, MarkerKind, annotate, qn_to_frame,
};
use signal_sampler::spec::LibrarySpec;

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
    let pre_delay_frames = i64::from(
        spec.short_note_timing
            .as_ref()
            .map(|s| s.pre_delay_ms)
            .unwrap_or(0),
    ) * i64::from(SR)
        / 1000;

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
            DocEvent::NoteOn { note, .. } => {
                assert_eq!(note, exp.pitch, "{} note {i}: note-on pitch", case.name);
                match exp.kind {
                    OnsetKind::Short => (
                        ev.frame as i64 + pre_delay_frames,
                        "short pre-roll + pre_delay",
                    ),
                    OnsetKind::PhraseStart => (ev.frame as i64, "fresh attack frame"),
                    other => panic!(
                        "{} note {i}: plain note-on for {:?} — legato edge lost",
                        case.name, other
                    ),
                }
            }
            _ => unreachable!(),
        };
        assert_eq!(
            arrival, grid,
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
    for case in timing_corpus() {
        assert_schedule_identities(&case, &spec);
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
        if z.interval == 0 || z.lead_in_ms <= 0.0 || z.loop_end <= z.loop_start {
            continue;
        }
        let lead_frames = (f64::from(z.lead_in_ms) / 1000.0 * f64::from(SR)) as u32;
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

        // ── Acoustic cross-check (validates the markers) ─────────────────
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
                OnsetKind::PhraseStart => (flux.leading_edge(exp.sec, 0.10, 0.25), "flux-edge"),
            };
            let Some(t) = measured else {
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
                OnsetKind::Legato => legato_errs_ms.push(err),
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
    assert!(lm <= 70.0, "legato mean pitch-arrival error {lm:.1} ms");
    assert!(lp95 <= 120.0, "legato p95 pitch-arrival error {lp95:.1} ms");
    assert!(
        lworst <= 180.0,
        "legato worst pitch-arrival error {lworst:.1} ms — a marker is lying"
    );
    for e in &short_errs_ms {
        assert!(
            (-25.0..=150.0).contains(e),
            "short flux peak {e:.1} ms outside the RR attack band"
        );
    }
    for e in &edge_errs_ms {
        assert!(
            (-35.0..=160.0).contains(e),
            "fresh-attack leading edge {e:.1} ms outside the bloom band"
        );
    }
    for e in &rebow_errs_ms {
        // Presence check only: inside continuous same-pitch sound the
        // strongest spectral change in the window can be the previous
        // note's own vibrato/bow fluctuation, so the band is wide.
        assert!(
            (-150.0..=250.0).contains(e),
            "re-bow flux peak {e:.1} ms — no bow change near the tick"
        );
    }
}
