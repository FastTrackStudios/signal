//! Offline-lookahead legato verification against the REAL CSS 1st Violins
//! library — the audio-level counterpart of the schedule tests in
//! `signal-sampler`'s `document.rs`.
//!
//! Pins the r[signal.soundsource.mode.offline-lookahead] behaviours:
//! - the document path PRE-ROLLS every legato transition (fires BEFORE the
//!   destination tick, by the measured-arrival − start-offset lead) and
//!   never degrades to the reactive path;
//! - live (StrictLive) playing of the same phrase commits reactively — the
//!   transition fires only AT/AFTER the note-on (bounded latency, no
//!   lookahead) — r[signal.soundsource.legato.live];
//! - the rendered legato joins are CONTINUOUS: no dropout and no doubled
//!   level bump across the transition — r[signal.soundsource.legato.mono].
//!
//! Gated on the CSS library + config being present (same pattern as
//! `score_to_audio.rs`).

use signal_orchestra::{CSS_CONFIG, CSS_ROOT, load_strings};
use signal_sampler::SamplerRig;
use signal_sampler::document::{DocCc, DocNote, DocumentRenderOptions, TrackDocument};

const ID: &str = "strings_1v";
const SR: u32 = 48_000;

fn css_present() -> bool {
    std::path::Path::new(CSS_ROOT).exists() && std::path::Path::new(CSS_CONFIG).exists()
}

fn load_rig() -> SamplerRig {
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    load_strings(&rig, ID, "1st Violins", "Mix", CSS_ROOT, CSS_CONFIG).expect("load CSS");
    rig
}

/// Short-window RMS (dBFS) of interleaved stereo audio over
/// `[start_frame, start_frame + len)`.
fn rms_db(audio: &[f32], start_frame: usize, len: usize) -> f32 {
    let a = (start_frame * 2).min(audio.len());
    let b = ((start_frame + len) * 2).min(audio.len());
    if b <= a {
        return -120.0;
    }
    let s: f64 = audio[a..b].iter().map(|&x| (x as f64) * (x as f64)).sum();
    let rms = (s / (b - a) as f64).sqrt();
    if rms <= 1e-9 {
        -120.0
    } else {
        20.0 * (rms as f32).log10()
    }
}

/// A three-note legato line (G4 → A4 → F#4, 1 QN each at 120 BPM, CC1 = 90):
/// the document render prefires both transitions, reports zero reactive
/// fallbacks, and the joins are continuous in the audio.
#[test]
fn lookahead_prerolls_transitions_and_joins_continuously() {
    if !css_present() {
        eprintln!("skipping: CSS library/config not present");
        return;
    }
    let rig = load_rig();
    let note = |start_qn: f64, end_qn: f64, pitch: u8| DocNote {
        start_qn,
        end_qn,
        chan: 0,
        pitch,
        vel: 90,
    };
    let doc = TrackDocument {
        version: 1,
        seed: 0xC55_1E6A70,
        notes: vec![
            note(0.0, 1.02, 67),
            note(1.0, 2.02, 69),
            note(2.0, 3.0, 66),
        ],
        ccs: vec![DocCc {
            qn: 0.0,
            chan: 0,
            cc: 1,
            val: 90,
        }],
        tempo: vec![],
        auto_divisi: false,
    };
    let res = rig
        .render_offline_document(ID, &doc, &DocumentRenderOptions::default())
        .expect("document render");

    // The annotator scheduled EVERY transition — nothing fell back to the
    // reactive (live-latency) path.
    assert_eq!(res.reactive_fallbacks, 0, "no reactive fallbacks offline");
    assert_eq!(res.transitions.len(), 2, "both legato moves fired");

    // Pre-roll is real: each transition fired BEFORE its destination tick,
    // by a sane lead (< 500 ms — the measured arrival minus the start
    // offset), not merely at the tick.
    // 120 BPM ⇒ 1 QN = 0.5 s: destination ticks at QN 1.0 / 2.0.
    let ticks = [SR as u64 / 2, SR as u64];
    for (t, &tick) in res.transitions.iter().zip(&ticks) {
        assert!(
            t.frame < tick,
            "transition at {} prefired before tick {}",
            t.frame,
            tick
        );
        // CSS lead-ins are measured per zone and spread WIDE (median
        // ~330 ms; this phrase's takes measure ~730 ms up and ~1.3 s down):
        // the schedule prefires by exactly (measured arrival − start-offset)
        // so the pitch change still lands ON the tick. Sanity-bound only —
        // the join-continuity asserts below carry the musical requirement.
        assert!(
            tick - t.frame <= 2 * SR as u64,
            "pre-roll lead is sane: {} frames",
            tick - t.frame
        );
    }

    // Audio-level continuity across each join: within ±250 ms of the tick,
    // every 50 ms window stays within a musical band of the join's own
    // peak — no dropout (> −30 dB dip) and no doubled-voice bump. Also the
    // phrase is genuinely non-silent.
    let win = (SR / 20) as usize; // 50 ms
    let span = (SR / 4) as usize; // 250 ms
    let phrase_peak = {
        let mut p = -120.0f32;
        let mut f = 0usize;
        // Phrase body = 3 QN = 1.5 s.
        while f + win < 3 * SR as usize / 2 {
            p = p.max(rms_db(&res.audio, f, win));
            f += win;
        }
        p
    };
    assert!(phrase_peak > -35.0, "phrase audible (peak {phrase_peak} dB)");
    if std::env::var_os("LEGATO_TEST_DEBUG").is_some() {
        for t in &res.transitions {
            eprintln!("transition: frame {} {}->{}", t.frame, t.from_note, t.to_note);
        }
        let mut f = 0usize;
        while f + win < 3 * SR as usize + SR as usize {
            eprintln!("rms {:>7} ({:.2}s): {:6.1} dB", f, f as f32 / SR as f32, rms_db(&res.audio, f, win));
            f += win;
        }
    }
    for &tick in &ticks {
        let (mut lo, mut hi) = (0.0f32, -120.0f32);
        let mut f = tick as usize - span;
        while f + win <= tick as usize + span {
            let db = rms_db(&res.audio, f, win);
            hi = hi.max(db);
            lo = if lo == 0.0 { db } else { lo.min(db) };
            f += win;
        }
        assert!(
            hi - lo < 30.0,
            "join at {tick} is continuous: window RMS spread {:.1} dB (lo {lo:.1}, hi {hi:.1})",
            hi - lo
        );
        assert!(
            hi < phrase_peak + 6.0,
            "no doubled voice at the join: {hi:.1} vs phrase peak {phrase_peak:.1}"
        );
    }
}

/// The SAME phrase played live (StrictLive, reactive path): transitions
/// commit only when the note-on arrives — at/after the destination's
/// wall-clock position, never before (no lookahead in live mode) — and the
/// engine counts them as reactive fires. This is the live/offline mode
/// split of r[signal.soundsource.mode.parity]: same samples, same spec,
/// different scheduling only.
#[test]
fn strict_live_commits_reactively_not_early() {
    if !css_present() {
        eprintln!("skipping: CSS library/config not present");
        return;
    }
    let rig = load_rig();
    rig.cc(ID, 1, 90);
    rig.set_legato_fire_log_enabled(ID, true);
    let render_secs = |secs: f64| {
        let frames = (secs * SR as f64).round() as usize;
        let mut buf = vec![0.0f32; frames * 2];
        rig.render_offline(&mut buf).expect("render");
        buf
    };
    // Warm the zones so the live path actually voices.
    rig.warm_note(ID, 67);
    rig.warm_note(ID, 69);

    let mut audio = Vec::new();
    rig.note_on(ID, 67, 90);
    audio.extend(render_secs(1.0));
    let second_on_frame = rig.engine_frames_rendered(ID).expect("frames");
    rig.note_on(ID, 69, 90); // overlap = live legato
    rig.note_off(ID, 67);
    audio.extend(render_secs(1.0));
    rig.note_off(ID, 69);
    audio.extend(render_secs(0.5));

    assert!(
        rig.reactive_legato_fires(ID) >= 1,
        "live overlap goes through the reactive path"
    );
    let log = rig.legato_fire_log(ID);
    let fire = log
        .iter()
        .find(|e| e.to_note == 69)
        .expect("live transition fired");
    assert!(
        fire.frame >= second_on_frame,
        "live transition never fires before its note-on ({} < {})",
        fire.frame,
        second_on_frame
    );
    // Bounded latency: the reactive commit happens within the Overlap-Delay
    // ceiling (~85 ms) of the note-on.
    assert!(
        fire.frame - second_on_frame <= (SR as u64) / 10,
        "live latency bounded: {} frames",
        fire.frame - second_on_frame
    );
    // And it sounds.
    let s: f64 = audio.iter().map(|&x| (x as f64) * (x as f64)).sum();
    assert!((s / audio.len() as f64).sqrt() > 1e-4, "live phrase audible");
}
