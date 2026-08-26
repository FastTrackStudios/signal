//! Document-mode phase-1 integration tests (see `docs/plan/document-mode.md`).
//!
//! Covers the three permanent guarantees:
//! 1. **Determinism** — same document + seed ⇒ byte-identical audio, and a
//!    mid-piece start reproduces the full render's tail exactly (RR slots are
//!    pure hashes of note identity, never counters).
//! 2. **Timing inversion** — a legato-followed note's transition fires
//!    `delay_ms` BEFORE its tick, so the arrival lands ON the tick.
//! 3. **Reactive vs document A/B** — the reactive path speaks `delay_ms`
//!    after the tick (the famous CSS latency); the document path arrives on
//!    the tick — earlier by exactly the configured delay.

use std::path::{Path, PathBuf};

use signal_sampler::document::{annotate, DocEvent, DocNote, DocumentRenderOptions, TrackDocument};
use signal_sampler::{ArticClass, SamplerRig};

const SR: u32 = 48_000;
/// Fixture legato pre-delay for velocities 0–64 (see the styx below).
const SLOW_DELAY_MS: u64 = 333;
const SLOW_DELAY_FRAMES: u64 = SLOW_DELAY_MS * SR as u64 / 1000;

// ── Fixture library ───────────────────────────────────────────────────────────

fn write_sine_wav(path: &Path, frames: usize, freq: f64, amp: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SR,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("create wav");
    for i in 0..frames {
        let t = i as f64 / SR as f64;
        let s = (t * freq * std::f64::consts::TAU).sin() as f32 * amp;
        w.write_sample(s).expect("write sample");
    }
    w.finalize().expect("finalize wav");
}

/// Build a minimal CSS-shaped legato library in `dir`: a sustain body, 4-RR
/// directional legato transitions, and 4-RR shorts, with the CSS expressive
/// velocity→delay curve (333/100 ms).
fn build_fixture(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).expect("mkdir");
    write_sine_wav(&dir.join("sus.wav"), SR as usize, 220.0, 0.5);
    let mut zones = String::from(
        "    { file sus.wav, key_min 0, key_max 127, root_key 60, articulation Sus }\n",
    );
    for rr in 0..4u32 {
        for dirn in ["up", "down"] {
            let f = format!("leg_{dirn}_{rr}.wav");
            // Distinct frequency per RR slot so slot choice is audible.
            write_sine_wav(&dir.join(&f), SR as usize, 300.0 + 40.0 * rr as f64, 0.4);
            zones.push_str(&format!(
                "    {{ file {f}, key_min 0, key_max 127, root_key 64, articulation Leg, direction {dirn}, rr_index {rr} }}\n"
            ));
        }
        let f = format!("stac_{rr}.wav");
        write_sine_wav(
            &dir.join(&f),
            SR as usize / 4,
            500.0 + 60.0 * rr as f64,
            0.4,
        );
        zones.push_str(&format!(
            "    {{ file {f}, key_min 0, key_max 127, root_key 60, articulation Stac, rr_index {rr} }}\n"
        ));
    }
    let styx = format!(
        r#"
name DocFixture
articulations (
    {{ id Sus,  label Sustain,  kind @Sustain, rr 1 }}
    {{ id Leg,  label Legato,   kind @Legato,  rr 4, directional true }}
    {{ id Stac, label Staccato, kind @Short,   rr 4 }}
)
legato_engine {{
    expressive {{
        zones (
            {{vel_range (0 64),   label slow, delay_ms 333}}
            {{vel_range (65 127), label fast, delay_ms 100}}
        )
    }}
    low_latency {{
        zones ( {{vel_range (0 127), label fast, delay_ms 100}} )
    }}
}}
short_note_timing {{ pre_delay_ms 60 }}
keyswitch {{
    cc58_map {{
        0-5   "Sustain: Low Latency Legato"
        21-25 Staccato
    }}
}}
zones (
{zones})
"#
    );
    let spec_path = dir.join("lib.styx");
    std::fs::write(&spec_path, styx).expect("write styx");
    spec_path
}

fn fixture_rig(tag: &str) -> (SamplerRig, tempdir::Guard) {
    let dir =
        std::env::temp_dir().join(format!("signal-document-mode-{tag}-{}", std::process::id()));
    let spec_path = build_fixture(&dir);
    let rig = SamplerRig::new_offline(SR);
    rig.load_instrument("fixture", &spec_path, Some(&dir), "", "")
        .expect("load fixture instrument");
    rig.preload_instrument("fixture").expect("preload");
    (rig, tempdir::Guard(dir))
}

/// Tiny RAII cleanup for the fixture dir.
mod tempdir {
    pub struct Guard(pub std::path::PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn note(start_qn: f64, end_qn: f64, pitch: u8, vel: u8) -> DocNote {
    note_ch(0, start_qn, end_qn, pitch, vel)
}

fn note_ch(chan: u8, start_qn: f64, end_qn: f64, pitch: u8, vel: u8) -> DocNote {
    DocNote {
        start_qn,
        end_qn,
        chan,
        pitch,
        vel,
    }
}

fn audio_bits(audio: &[f32]) -> Vec<u32> {
    audio.iter().map(|s| s.to_bits()).collect()
}

// ── 1. Determinism ───────────────────────────────────────────────────────────

#[test]
fn document_render_is_byte_identical_across_runs() {
    let doc = TrackDocument {
        seed: 0xD0C_5EED,
        notes: vec![
            note(0.0, 2.1, 60, 90),  // phrase start
            note(2.0, 4.1, 62, 30),  // legato (slow, 333 ms)
            note(4.0, 6.0, 65, 110), // legato (fast, 100 ms)
        ],
        ccs: vec![signal_sampler::DocCc {
            qn: 0.0,
            chan: 0,
            cc: 1,
            val: 96,
        }],
        ..Default::default()
    };
    let opts = DocumentRenderOptions {
        tail_sec: 1.0,
        ..Default::default()
    };

    let (rig_a, _ga) = fixture_rig("det-a");
    let a = rig_a
        .render_offline_document("fixture", &doc, &opts)
        .expect("render a");
    let (rig_b, _gb) = fixture_rig("det-b");
    let b = rig_b
        .render_offline_document("fixture", &doc, &opts)
        .expect("render b");

    assert!(!a.audio.is_empty());
    assert!(a.audio.iter().any(|s| *s != 0.0), "render produced audio");
    assert_eq!(a.audio.len(), b.audio.len());
    assert_eq!(
        audio_bits(&a.audio),
        audio_bits(&b.audio),
        "same document + seed must render byte-identically"
    );
    assert_eq!(a.transitions, b.transitions);
    assert_eq!(
        a.transitions.len(),
        2,
        "both legato notes fired transitions"
    );
    assert_eq!(a.reactive_fallbacks, 0, "document playback never reacts");
}

// ── 1b. Per-line divisi (one engine, many mono lines) ───────────────────────

#[test]
fn divisi_document_prefires_every_line_with_zero_reactive_fallbacks() {
    // Two divisi channels, each a mono legato line. Phase 1 folded these
    // into one line and the second channel degraded to the reactive path;
    // per-line scheduling must prefire BOTH — reactive count exactly 0.
    let doc = TrackDocument {
        seed: 0xD1_7151,
        notes: vec![
            // line 0 (upper desk)
            note_ch(0, 0.0, 2.1, 64, 90),
            note_ch(0, 2.0, 4.0, 65, 30), // legato, slow zone
            // line 1 (lower desk) — overlapping the same span
            note_ch(1, 0.0, 2.6, 55, 90),
            note_ch(1, 2.5, 4.5, 53, 80), // legato, fast zone
        ],
        ..Default::default()
    };

    let (rig, _g) = fixture_rig("divisi");
    let spec = rig.instrument_spec("fixture").expect("spec");
    let sched = annotate(&doc, &spec, SR);
    assert_eq!(sched.legato_count, 2, "one legato edge per channel");

    let res = rig
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 0.5,
                ..Default::default()
            },
        )
        .expect("render");

    assert_eq!(
        res.reactive_fallbacks, 0,
        "every divisi line's transition must arrive via prefire, never the reactive path"
    );
    assert_eq!(res.transitions.len(), 2, "both lines fired transitions");
    let mut lines: Vec<u8> = res.transitions.iter().map(|t| t.line).collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![0, 1], "one transition per divisi line");

    // Each line's arrival lands on its own tick: line 0 at QN 2 (vel 30 ⇒
    // 333 ms), line 1 at QN 2.5 (vel 80 ⇒ 100 ms). 120 BPM ⇒ 0.5 s/QN.
    let t0 = res.transitions.iter().find(|t| t.line == 0).unwrap();
    let t1 = res.transitions.iter().find(|t| t.line == 1).unwrap();
    assert_eq!(t0.to_note, 65);
    assert_eq!(t0.frame + SLOW_DELAY_FRAMES, 48_000);
    assert_eq!(t1.to_note, 53);
    assert_eq!(t1.frame + 100 * SR as u64 / 1000, 60_000);

    // Determinism holds for multi-line documents too.
    let (rig2, _g2) = fixture_rig("divisi-b");
    let res2 = rig2
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 0.5,
                ..Default::default()
            },
        )
        .expect("render 2");
    assert_eq!(audio_bits(&res.audio), audio_bits(&res2.audio));
    assert_eq!(res.transitions, res2.transitions);
}

#[test]
fn mid_piece_start_matches_full_render_and_rr_slots() {
    // Two phrases separated by 4 s of silence (longer than any voice tail),
    // so the engine is quiescent at the phrase-2 boundary.
    let doc = TrackDocument {
        seed: 0xBEEF,
        notes: vec![
            // phrase 1
            note(0.0, 2.1, 60, 90),
            note(2.0, 3.0, 62, 30), // legato
            // phrase 2 — starts at QN 11 (120 BPM ⇒ 5.5 s ⇒ frame 264000)
            note(11.0, 13.1, 64, 90),
            note(13.0, 14.0, 66, 30), // legato
        ],
        ..Default::default()
    };
    let phrase2_frame: u64 = 264_000; // qn 11 at 120 BPM, 48 kHz

    let (rig_full, _gf) = fixture_rig("mid-full");
    let full = rig_full
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 1.0,
                ..Default::default()
            },
        )
        .expect("full render");

    let (rig_mid, _gm) = fixture_rig("mid-mid");
    let mid = rig_mid
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 1.0,
                start_frame: phrase2_frame,
                ..Default::default()
            },
        )
        .expect("mid render");

    // Position independence: the notes played from bar N use the same RR
    // slots as the full render — the schedule pins slots from note identity,
    // so the annotated slots are identical by construction; assert the
    // *audio* proves the engine obeyed them.
    let offset = (phrase2_frame * 2) as usize;
    let full_tail = &full.audio[offset..];
    assert_eq!(full_tail.len(), mid.audio.len());
    assert!(mid.audio.iter().any(|s| *s != 0.0), "mid render audible");
    assert_eq!(
        audio_bits(full_tail),
        audio_bits(&mid.audio),
        "starting mid-piece must reproduce the full render's tail exactly"
    );

    // The phrase-2 transition fired at the same absolute frame in both runs.
    let full_p2: Vec<_> = full
        .transitions
        .iter()
        .filter(|t| t.frame >= phrase2_frame)
        .collect();
    assert_eq!(full_p2.len(), 1);
    assert_eq!(mid.transitions.len(), 1);
    assert_eq!(full_p2[0], &mid.transitions[0]);

    // And the annotated schedule's RR pins are identical whether or not the
    // playback starts mid-piece (pure hash — no counters to advance).
    let spec = rig_full.instrument_spec("fixture").expect("spec");
    let s1 = annotate(&doc, &spec, SR);
    let s2 = annotate(&doc, &spec, SR);
    let slots = |s: &signal_sampler::Schedule| -> Vec<u32> {
        s.events
            .iter()
            .filter_map(|e| match e.kind {
                DocEvent::NoteOn { rr, .. }
                | DocEvent::NoteOff { rr, .. }
                | DocEvent::LegatoPrefire { rr, .. } => Some(rr),
                DocEvent::Cc { .. } => None,
            })
            .collect()
    };
    assert_eq!(slots(&s1), slots(&s2));
}

// ── 1c. Articulation-class output buses (stems) ──────────────────────────────

#[test]
fn class_bus_split_sums_to_main_and_isolates_shorts() {
    // Legato phrase (Longs), then — after a gap longer than any voice tail —
    // a staccato-only region (CC58 → Staccato ⇒ Shorts). The temporal
    // separation makes the bit-identity assertions exact: float summation is
    // order-sensitive, so "split buses sum to main" holds bit-for-bit when
    // (as here) the classes never sound in the same sample. The routing
    // itself is per-voice and sample-accurate regardless.
    let doc = TrackDocument {
        seed: 0x57E4,
        notes: vec![
            // Longs: sustain → legato
            note(0.0, 2.1, 60, 90),
            note(2.0, 4.0, 62, 30),
            // Shorts: staccato pair, 10 s later (120 BPM ⇒ QN 20 = 10 s)
            note(20.0, 20.4, 64, 100),
            note(21.0, 21.4, 66, 100),
        ],
        // CC58 = 23 (Staccato band) between the regions.
        ccs: vec![signal_sampler::DocCc {
            qn: 16.0,
            chan: 0,
            cc: 58,
            val: 23,
        }],
        ..Default::default()
    };
    let opts = DocumentRenderOptions {
        tail_sec: 1.0,
        ..Default::default()
    };

    // Reference: the plain (phase-1) main-out document render.
    let (rig_main, _gm) = fixture_rig("bus-main");
    let main = rig_main
        .render_offline_document("fixture", &doc, &opts)
        .expect("main render");
    assert_eq!(main.reactive_fallbacks, 0);
    assert!(main.audio.iter().any(|s| *s != 0.0));

    // Default routing (all → "main"): ONE bus, bit-identical to the plain
    // render — routing must not perturb voice order, RR, or timing.
    let (rig_def, _gd) = fixture_rig("bus-default");
    let def = rig_def
        .render_offline_document_buses("fixture", &doc, &opts)
        .expect("default-routing render");
    assert_eq!(def.buses.len(), 1);
    let def_main = def.buses.get("main").expect("main bus");
    assert_eq!(
        audio_bits(def_main),
        audio_bits(&main.audio),
        "default all→main routing must be bit-identical to the plain document render"
    );
    assert_eq!(def.transitions, main.transitions);

    // Split routing: Longs and Shorts land in their own stereo buses.
    let (rig_split, _gs) = fixture_rig("bus-split");
    rig_split.set_class_bus(ArticClass::Longs, "longs");
    rig_split.set_class_bus(ArticClass::Shorts, "shorts");
    let split = rig_split
        .render_offline_document_buses("fixture", &doc, &opts)
        .expect("split render");
    assert_eq!(split.reactive_fallbacks, 0);
    let longs = split.buses.get("longs").expect("longs bus");
    let shorts = split.buses.get("shorts").expect("shorts bus");
    assert_eq!(longs.len(), main.audio.len());
    assert_eq!(shorts.len(), main.audio.len());

    // Sum of the stems == the main render, bit for bit.
    let sum: Vec<f32> = longs
        .iter()
        .zip(shorts.iter())
        .map(|(l, s)| l + s)
        .collect();
    assert_eq!(
        audio_bits(&sum),
        audio_bits(&main.audio),
        "Longs + Shorts must recombine to the main render bit-exactly"
    );

    // Shorts land ONLY in the Shorts bus: the staccato region (from the
    // first pre-rolled note-on at 10 s − 60 ms) carries energy in `shorts`
    // and none in `longs`; the legato region (first 4 s) is the reverse.
    let stac_start = 2 * (10 * SR as usize - 60 * SR as usize / 1000); // interleaved index
    let legato_end = 2 * 4 * SR as usize;
    let energy = |buf: &[f32]| -> f64 { buf.iter().map(|s| (*s as f64) * (*s as f64)).sum() };
    assert!(
        energy(&shorts[stac_start..]) > 0.0,
        "staccato energy in the Shorts bus"
    );
    assert_eq!(
        energy(&longs[stac_start..]),
        0.0,
        "no Longs energy in the staccato-only region"
    );
    assert!(
        energy(&longs[..legato_end]) > 0.0,
        "legato energy in the Longs bus"
    );
    assert_eq!(
        energy(&shorts[..legato_end]),
        0.0,
        "no Shorts energy in the legato region"
    );
}

// ── 1c². Lookahead auto-divisi (annotate-side allocator) ─────────────────────

#[test]
fn auto_divisi_held_top_note_keeps_its_line_over_moving_lower_voice() {
    // One channel, two voices: a held top note over a moving lower voice.
    // The ranking is by what is SOUNDING at each onset (keyflow
    // assign_channels parity), so the top note owns line 0 for its whole
    // duration and every lower re-articulation lands on line 1.
    let doc = TrackDocument {
        seed: 0xD1,
        auto_divisi: true,
        notes: vec![
            note(0.0, 8.0, 76, 90), // held top
            // moving lower voice, legato-connected (overlapping)
            note(0.0, 2.1, 60, 90),
            note(2.0, 4.1, 62, 30),
            note(4.0, 6.0, 64, 80),
        ],
        ..Default::default()
    };
    let (rig, _g) = fixture_rig("autodiv-held");
    let spec = rig.instrument_spec("fixture").expect("spec");
    let sched = annotate(&doc, &spec, SR);

    // Top note: one plain NoteOn + NoteOff on line 0, never displaced.
    let top_events: Vec<_> = sched
        .events
        .iter()
        .filter(|e| {
            matches!(e.kind,
                DocEvent::NoteOn { note, .. } | DocEvent::NoteOff { note, .. }
                    if note == 76)
        })
        .collect();
    assert_eq!(top_events.len(), 2);
    assert!(
        top_events.iter().all(|e| e.line == 0),
        "held top note keeps line 0 throughout"
    );

    // Lower voice: line 1 for every event, with its own legato edges.
    assert_eq!(sched.legato_count, 2, "62 and 64 arrive via prefire");
    let lower_prefires: Vec<_> = sched
        .events
        .iter()
        .filter(|e| matches!(e.kind, DocEvent::LegatoPrefire { .. }))
        .collect();
    assert_eq!(lower_prefires.len(), 2);
    assert!(lower_prefires.iter().all(|e| e.line == 1));

    // Playback: both lines prefired, zero reactive fallbacks.
    let res = rig
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 0.5,
                ..Default::default()
            },
        )
        .expect("render");
    assert_eq!(res.reactive_fallbacks, 0);
    assert_eq!(res.transitions.len(), 2);
    assert!(res.transitions.iter().all(|t| t.line == 1));
}

#[test]
fn auto_divisi_counterpoint_is_deterministic_from_mid_piece() {
    // Two-voice counterpoint on ONE channel, two phrases separated by
    // silence longer than any tail: auto-divisi must assign 2 stable lines,
    // and a mid-piece start must reproduce the full render's tail exactly
    // (the assignment is a pure function of the document — no counters).
    let phrase = |ofs: f64, up: u8| -> Vec<DocNote> {
        vec![
            note(ofs, ofs + 2.1, up, 90),
            note(ofs + 2.0, ofs + 4.0, up + 2, 30),
            note(ofs, ofs + 2.6, 55, 90),
            note(ofs + 2.5, ofs + 4.5, 53, 80),
        ]
    };
    let mut notes = phrase(0.0, 72);
    notes.extend(phrase(16.0, 74)); // QN 16 = 8 s at 120 BPM
    let doc = TrackDocument {
        seed: 0xC0DA,
        auto_divisi: true,
        notes,
        ..Default::default()
    };
    let phrase2_frame: u64 = 16 * SR as u64 / 2; // QN 16 at 120 BPM

    let (rig_full, _gf) = fixture_rig("autodiv-full");
    let sched = annotate(&doc, &rig_full.instrument_spec("fixture").unwrap(), SR);
    assert_eq!(sched.legato_count, 4, "two legato edges per phrase");
    let mut lines: Vec<u8> = sched.events.iter().map(|e| e.line).collect();
    lines.sort_unstable();
    lines.dedup();
    assert_eq!(
        lines,
        vec![0, 1],
        "counterpoint splits into exactly 2 lines"
    );

    let full = rig_full
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 1.0,
                ..Default::default()
            },
        )
        .expect("full render");
    assert_eq!(full.reactive_fallbacks, 0);
    assert_eq!(full.transitions.len(), 4);

    let (rig_mid, _gm) = fixture_rig("autodiv-mid");
    let mid = rig_mid
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 1.0,
                start_frame: phrase2_frame,
                ..Default::default()
            },
        )
        .expect("mid render");
    assert_eq!(mid.reactive_fallbacks, 0);
    let offset = (phrase2_frame * 2) as usize;
    assert_eq!(
        audio_bits(&full.audio[offset..]),
        audio_bits(&mid.audio),
        "auto-divisi render must be position-independent"
    );
}

// ── 1d. Play-mode policy: strict live low-latency by default ─────────────────

#[test]
fn strict_live_uses_low_latency_tables_and_lookahead_uses_expressive() {
    // vel 30: expressive table ⇒ 333 ms, low_latency table ⇒ 100 ms.
    // Live default (PlayMode::StrictLive) must take the low_latency value —
    // no exceptions — while the document path keeps the expressive lead.
    let block: usize = 64;
    let fast_frames: u64 = 100 * SR as u64 / 1000;
    let tick: u64 = 48_000;

    let (live, _g) = fixture_rig("strict-live");
    live.set_legato_fire_log_enabled("fixture", true);
    let mut cur: u64 = 0;
    let render_until = |target: u64, cur: &mut u64| {
        while *cur < target {
            let frames = ((target - *cur) as usize).min(block);
            let mut buf = vec![0.0f32; frames * 2];
            live.render_offline(&mut buf).expect("render");
            *cur += frames as u64;
        }
    };
    live.note_on("fixture", 60, 90);
    render_until(tick, &mut cur);
    live.note_on("fixture", 62, 30); // slow-zone velocity
    render_until(tick + 2 * SLOW_DELAY_FRAMES, &mut cur);

    let log = live.legato_fire_log("fixture");
    assert_eq!(log.len(), 1);
    let fire = log[0].frame;
    assert!(
        fire >= tick + fast_frames - block as u64 && fire <= tick + fast_frames,
        "StrictLive must fire on the low_latency table (~100 ms), got +{} frames",
        fire - tick
    );

    // The document path (Lookahead) keeps the expressive 333 ms lead.
    let doc = TrackDocument {
        seed: 1,
        notes: vec![note(0.0, 2.1, 60, 90), note(2.0, 4.0, 62, 30)],
        ..Default::default()
    };
    let spec = live.instrument_spec("fixture").expect("spec");
    let sched = annotate(&doc, &spec, SR);
    let prefire = sched
        .events
        .iter()
        .find(|e| matches!(e.kind, DocEvent::LegatoPrefire { .. }))
        .expect("prefire");
    assert_eq!(prefire.frame, tick - SLOW_DELAY_FRAMES);

    // And a document render leaves the engine back in StrictLive.
    let (rig, _g2) = fixture_rig("strict-restore");
    rig.render_offline_document(
        "fixture",
        &doc,
        &DocumentRenderOptions {
            tail_sec: 0.1,
            ..Default::default()
        },
    )
    .expect("render");
    assert_eq!(
        rig.play_mode("fixture"),
        Some(signal_sampler::PlayMode::StrictLive),
        "document render must restore the strict live policy when done"
    );
}

#[test]
fn strict_live_shorts_have_zero_preroll() {
    // Live shorts fire AT note-on — the 60 ms pre-delay is a schedule-only
    // concept (it means "start EARLY", which live playing cannot do).
    let (rig, _g) = fixture_rig("strict-shorts");
    rig.cc("fixture", 58, 23); // Staccato band
    rig.note_on("fixture", 64, 100);
    let mut buf = vec![0.0f32; 64 * 2];
    rig.render_offline(&mut buf).expect("render");
    assert!(
        buf.iter().any(|s| *s != 0.0),
        "a StrictLive short must sound within the first block after note-on"
    );
}

// ── 2. Timing inversion ──────────────────────────────────────────────────────

#[test]
fn transition_fires_before_tick_and_arrives_on_tick() {
    // B (vel 30 ⇒ slow zone, 333 ms) starts at QN 2 = 1.0 s = frame 48000.
    let tick: u64 = 48_000;
    let doc = TrackDocument {
        seed: 1,
        notes: vec![note(0.0, 2.1, 60, 90), note(2.0, 4.0, 62, 30)],
        ..Default::default()
    };

    let (rig, _g) = fixture_rig("inversion");
    // The annotated schedule places the prefire exactly delay_ms early.
    let spec = rig.instrument_spec("fixture").expect("spec");
    let sched = annotate(&doc, &spec, SR);
    let prefire = sched
        .events
        .iter()
        .find(|e| matches!(e.kind, DocEvent::LegatoPrefire { .. }))
        .expect("prefire scheduled");
    assert_eq!(prefire.frame, tick - SLOW_DELAY_FRAMES);

    // And the engine actually fired the transition at that frame.
    let res = rig
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 0.5,
                ..Default::default()
            },
        )
        .expect("render");
    assert_eq!(res.transitions.len(), 1);
    let fire = &res.transitions[0];
    assert_eq!(fire.to_note, 62);
    assert_eq!(
        fire.frame,
        tick - SLOW_DELAY_FRAMES,
        "transition sample starts delay_ms BEFORE the destination tick"
    );
    assert_eq!(
        fire.frame + SLOW_DELAY_FRAMES,
        tick,
        "destination arrival lands exactly on the tick"
    );
}

// ── 3. Reactive vs document A/B ──────────────────────────────────────────────

#[test]
fn reactive_vs_document_arrival_differs_by_configured_delay() {
    let tick: u64 = 48_000;
    let block: usize = 64;

    // Document path: prefire ⇒ arrival on the tick (asserted above); the
    // transition voice spawns at tick − delay.
    let doc = TrackDocument {
        seed: 1,
        notes: vec![note(0.0, 2.1, 60, 90), note(2.0, 4.0, 62, 30)],
        ..Default::default()
    };
    let (doc_rig, _gd) = fixture_rig("ab-doc");
    let doc_res = doc_rig
        .render_offline_document(
            "fixture",
            &doc,
            &DocumentRenderOptions {
                tail_sec: 0.5,
                ..Default::default()
            },
        )
        .expect("document render");
    assert_eq!(doc_res.transitions.len(), 1);
    let doc_fire = doc_res.transitions[0].frame;
    let doc_arrival = doc_fire + SLOW_DELAY_FRAMES;
    assert_eq!(doc_arrival, tick);

    // Reactive path: the same two notes played live at their ticks. The
    // engine arms the countdown at note-on and the transition speaks
    // delay_ms AFTER the tick (the famous latency every DAW compensates
    // with negative track delay).
    let (live_rig, _gl) = fixture_rig("ab-live");
    live_rig.set_legato_mode("fixture", true, true); // same expressive curve
    live_rig.set_legato_fire_log_enabled("fixture", true);
    let mut cur: u64 = 0;
    let render_until = |target: u64, cur: &mut u64| {
        while *cur < target {
            let frames = ((target - *cur) as usize).min(block);
            let mut buf = vec![0.0f32; frames * 2];
            live_rig.render_offline(&mut buf).expect("render");
            *cur += frames as u64;
        }
    };
    live_rig.note_on("fixture", 60, 90);
    render_until(tick, &mut cur);
    live_rig.note_on("fixture", 62, 30); // vel 30 ⇒ 333 ms countdown
    render_until(tick + 2 * SLOW_DELAY_FRAMES, &mut cur);
    let live_log = live_rig.legato_fire_log("fixture");
    assert_eq!(live_log.len(), 1);
    let live_fire = live_log[0].frame;

    // Reactive speaks late: fire ≈ tick + delay (block-quantised — the
    // countdown fires at the head of the block in which it elapses).
    assert!(
        live_fire >= tick + SLOW_DELAY_FRAMES - block as u64
            && live_fire <= tick + SLOW_DELAY_FRAMES,
        "reactive transition speaks delay_ms after the tick (got {live_fire}, tick {tick})"
    );

    // THE INVERSION, measured: the document arrival is earlier than the
    // reactive speak point by exactly the configured delay (± one live block).
    let delta = live_fire - doc_arrival;
    assert!(
        (SLOW_DELAY_FRAMES - block as u64..=SLOW_DELAY_FRAMES).contains(&delta),
        "document arrival must lead the reactive one by the configured delay \
         (delay {SLOW_DELAY_FRAMES}, measured {delta})"
    );
}
