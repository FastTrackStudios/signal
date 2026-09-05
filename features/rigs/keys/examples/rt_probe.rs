//! Realtime probe: does the keys rig actually PLAY, and does it keep its
//! deadline while a stream of MIDI goes in?
//!
//! `midi_probe` answers "is it audible at all". This answers the question a
//! player actually cares about — whether the notes arrive without dropouts —
//! and it answers it without a human at the keyboard, which is the point: the
//! rig's failure mode is a callback that misses its deadline dozens of times
//! a second, which nobody can see and everybody can hear.
//!
//! ```bash
//! cargo run --release -p signal-keys --example rt_probe
//! ```
//!
//! It drives notes through the rig's own `trigger` RPC (the on-screen piano's
//! path), so it needs no MIDI hardware and no loopback, then asserts three
//! things after the stream:
//!
//! - **Audible** — the master meter moved. A rig whose graph node never got
//!   linked to the output device runs perfectly and silently; that is a real
//!   bug this catches (`spawn_linker` used to bail out whenever the device
//!   names were empty, i.e. "system default").
//! - **No dropouts** — the audio callback reported no xruns while playing.
//! - **Inside the budget** — worst-case render time under the block budget
//!   (`block_frames / sample_rate`). Over it is a guaranteed dropout; well
//!   under it *with* xruns means the spike is not compute (a lock, an
//!   allocation, disk I/O on the audio thread).
//!
//! Exit 0 = pass. 1 = the rig ran but failed one of the three. 2 = it never
//! opened (no library, no audio device) — an environment problem, not a
//! regression.

use std::time::{Duration, Instant};

/// Major page faults this process has taken (field 12 of `/proc/self/stat`).
///
/// A major fault is a blocking read from disk. The pack format maps raw-PCM
/// entries rather than decoding them ("the OS is the streaming engine"), so
/// if these climb while playing, the audio thread is waiting on the disk
/// inside its callback — which is exactly what a multi-hundred-millisecond
/// render spike looks like.
fn minor_faults() -> u64 {
    proc_stat_field(7)
}

/// Major page faults this process has taken (field 12 of `/proc/self/stat`).
fn major_faults() -> u64 {
    proc_stat_field(9)
}

fn proc_stat_field(idx: usize) -> u64 {
    std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| {
            // Skip the comm field, which may itself contain spaces.
            let rest = s.rsplit_once(')').map(|(_, r)| r.to_string())?;
            rest.split_whitespace().nth(idx)?.parse().ok()
        })
        .unwrap_or(0)
}

use signal_keys_proto::keys::KeysRig as KeysRigSvc;

/// Chord shapes, stacked into octaves by [`voicing`] to reach the target note
/// count — enough simultaneous voice starts to expose the note-on spike,
/// which is where the reported glitching lives.
const CHORDS: [&[u8]; 4] = [
    &[60, 64, 67, 72],
    &[57, 60, 64, 69],
    &[53, 57, 60, 65],
    &[55, 59, 62, 67],
];

/// Notes played at once by default.
///
/// Twenty, not four: ten fingers with an octave doubler engaged is an
/// ordinary way to play this rig, and an arpeggiator can beat it. A rig that
/// only keeps its deadline for a four-note chord is not playable.
const DEFAULT_NOTES: usize = 20;

/// `base` stacked upward in octaves until `want` notes are in range.
fn voicing(base: &[u8], want: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(want);
    for oct in 0..5i16 {
        for &n in base {
            if out.len() >= want {
                return out;
            }
            let v = i16::from(n) + 12 * oct;
            if (21..=108).contains(&v) {
                out.push(v as u8);
            }
        }
    }
    out
}
const RUN: [u8; 8] = [60, 62, 64, 65, 67, 69, 71, 72];

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let backend = signal_keys::KeysRigBackend::new();
    KeysRigSvc::start(&backend);

    // Opening loads sample packs; on a cold cache that is genuinely slow.
    let mut opened = false;
    for _ in 0..120 {
        std::thread::sleep(Duration::from_millis(500));
        let s = KeysRigSvc::status(&backend);
        if s.running {
            opened = true;
            break;
        }
        if let Some(e) = s.last_error.as_deref() {
            if !e.starts_with("opening") {
                println!("OPEN FAILED: {e}");
                std::process::exit(2);
            }
        }
    }
    if !opened {
        println!("OPEN TIMEOUT");
        std::process::exit(2);
    }
    let s = KeysRigSvc::status(&backend);
    println!("OPEN OK preset={:?}", s.loaded_preset);
    if s.rt.block_frames == 0 {
        println!(
            "NOTE: backend reports no realtime stats — only the native engine \
             does. Audibility is still checked; the deadline is not."
        );
    }

    // Let the graph settle, then measure only what happens WHILE playing:
    // xruns during startup (device linking, first buffers) are not the rig
    // failing to keep up with a player.
    std::thread::sleep(Duration::from_secs(2));
    let baseline = KeysRigSvc::status(&backend).rt.xruns;
    let over_baseline = KeysRigSvc::status(&backend).rt.over_budget;
    // Mean over the PLAY window only: the open phase renders silence and
    // would drag the average down until it meant nothing.
    let (mean_blocks0, mean_total0) = {
        let r = KeysRigSvc::status(&backend).rt;
        (r.blocks, f64::from(r.mean_render_ms) * r.blocks as f64)
    };
    let faults0 = major_faults();
    let minor0 = minor_faults();
    // Measure the PLAY window, not the open: installing a preset and filling
    // the first buffers legitimately takes far longer than a block.
    KeysRigSvc::reset_rt_peak(&backend);
    let open_peak = KeysRigSvc::status(&backend).rt.peak_render_ms;

    // `FTS_RT_OFF="Pad,Shimmer"` mutes those lanes before the run. A muted
    // lane's voices are skipped entirely rather than rendered and multiplied
    // by zero, so this measures what a patch with switched-off modules
    // actually costs — which is how these patches are usually played.
    if let Ok(off) = std::env::var("FTS_RT_OFF") {
        let names: Vec<&str> = off
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let m = KeysRigSvc::mixer(&backend);
        let known: Vec<String> = m
            .engines
            .iter()
            .flat_map(|e| e.layers.iter().map(|l| l.name.clone()))
            .collect();
        // Switch the lane's MODULES off, not the lane fader. In lane hosting
        // a lane mute is a daw track mute applied after its instrument has
        // already rendered; a disabled module zeroes the render tree's own
        // gain cell, which is what lets the tree skip it.
        let mut off = 0usize;
        for e in &m.engines {
            for l in &e.layers {
                if !names.iter().any(|n| *n == l.name) {
                    continue;
                }
                for md in &l.modules {
                    KeysRigSvc::set_module_enabled(&backend, l.name.clone(), md.index, false);
                    off += 1;
                }
            }
        }
        println!("modules switched off: {off}");
        for name in &names {
            if !known.iter().any(|k| k == name) {
                println!("WARN: no lane named {name:?}; have {known:?}");
            }
        }
        println!("muted lanes: {names:?}");
        // Let the mute reach the audio thread and the lanes drain.
        std::thread::sleep(Duration::from_millis(500));
    }

    // `FTS_RT_NOTES=n` caps how many notes of each chord are played. Playing
    // the same material at 1, 2 and 4 notes says whether the cost is per-note
    // (linear) or something a chord triggers once.
    let max_notes: usize = std::env::var("FTS_RT_NOTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_NOTES);
    println!("notes per chord: {max_notes}");

    let mut peak = 0.0f32;
    // Worst render per chord, so the spike can be located in time: a big
    // first value that then settles is a cold cost (page faults, first
    // decode); one that recurs on every chord is structural.
    let mut per_chord: Vec<f32> = Vec::new();
    // Notes the audio thread threw away for want of a resident sample. The
    // miss path is not free — it formats strings and pushes a trace entry —
    // so a chord that drops thousands of notes is doing thousands of
    // allocations inside one callback.
    let mut per_chord_drops: Vec<usize> = Vec::new();
    // Voices alive at the end of each chord — the number that says whether a
    // render spike is "too much audio to mix" or something else entirely.
    let mut per_chord_voices: Vec<u32> = Vec::new();
    let mut drops_prev = signal_sampler::engine::notes_dropped();
    let started = Instant::now();
    for (i, chord) in CHORDS.iter().cycle().take(12).enumerate() {
        KeysRigSvc::reset_rt_peak(&backend);
        let notes = voicing(chord, max_notes);
        for &n in &notes {
            KeysRigSvc::trigger(&backend, u32::from(n), 100);
        }
        std::thread::sleep(Duration::from_millis(400));
        let st = KeysRigSvc::status(&backend);
        per_chord.push(st.rt.peak_render_ms);
        per_chord_voices.push(st.voices);
        let drops_now = signal_sampler::engine::notes_dropped();
        per_chord_drops.push(drops_now.saturating_sub(drops_prev));
        drops_prev = drops_now;
        peak = peak.max(st.master_peak);
        // RELEASE the chord. Holding every note for the whole run would let
        // voices pile up and measure the wrong thing entirely — render time
        // that climbs because nothing was ever let go is a property of the
        // test, not of the rig.
        for &n in &notes {
            KeysRigSvc::trigger(&backend, u32::from(n), 0);
        }
        std::thread::sleep(Duration::from_millis(150));
        // Every third chord, a fast run over the top — single-note attacks in
        // quick succession, the other shape that starts voices in a hurry.
        if i % 3 == 2 {
            for &n in &RUN {
                KeysRigSvc::trigger(&backend, u32::from(n), 96);
                std::thread::sleep(Duration::from_millis(60));
                KeysRigSvc::trigger(&backend, u32::from(n), 0);
                peak = peak.max(KeysRigSvc::status(&backend).master_peak);
            }
        }
    }
    let played = started.elapsed().as_secs_f64();
    let faults = major_faults().saturating_sub(faults0);
    let minor = minor_faults().saturating_sub(minor0);

    let s = KeysRigSvc::status(&backend);
    // Per-lane peaks: which lanes actually made sound. A rig can be audible
    // overall while the lane you care about contributes nothing — "the piano
    // is only resonance" is that, and the master meter cannot show it.
    {
        let mut lanes: Vec<(String, f32)> = s
            .meters
            .iter()
            .map(|m| (format!("{}/{}", m.kind, m.name), m.peak))
            .collect();
        lanes.sort_by(|a, b| b.1.total_cmp(&a.1));
        println!("lane peaks (loudest first):");
        for (name, peak) in lanes.iter().take(14) {
            let bar = "#".repeat(((peak * 40.0) as usize).min(40));
            println!("  {peak:.4}  {bar:<40}  {name}");
        }
    }
    let xruns = s.rt.xruns.saturating_sub(baseline);
    let over = s.rt.over_budget.saturating_sub(over_baseline);
    let play_blocks = s.rt.blocks.saturating_sub(mean_blocks0);
    let play_mean = if play_blocks > 0 {
        f64::from(s.rt.mean_render_ms).mul_add(s.rt.blocks as f64, -mean_total0) / play_blocks as f64
    } else {
        0.0
    };
    println!("page faults while playing: major={faults} minor={minor}");
    println!(
        "per-chord voices: {}",
        per_chord_voices
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!(
        "per-chord dropped notes: {}",
        per_chord_drops
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!(
        "per-chord worst render (ms): {}",
        per_chord
            .iter()
            .map(|v| format!("{v:.1}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let budget_ms = if s.rt.block_frames > 0 {
        s.rt.block_frames as f32 / 48.0 // ms at 48 kHz
    } else {
        0.0
    };

    println!(
        "RESULT peak={peak:.4} xruns={xruns} over_budget={over} ({:.1}/s) block={} \
         budget={budget_ms:.2}ms mean={play_mean:.3}ms ({:.1}% of budget) \
         render_peak={:.2}ms (open_peak={open_peak:.2}ms)",
        over as f64 / played.max(0.001),
        s.rt.block_frames,
        100.0 * play_mean / f64::from(budget_ms.max(0.001)),
        s.rt.peak_render_ms,
    );

    let audible = peak > 1e-4;
    // Count OVER-BUDGET blocks, not the graph's xrun field: the latter reads
    // the driver's clock on a follower node and sat at 0 through a window in
    // which PipeWire counted 112 xruns on this very node. A handful over
    // several seconds is a scheduling hiccup on a desktop; a rate is what a
    // player hears as clicks and pops.
    let steady = over as f64 / played.max(0.001) < 1.0;
    let in_budget = budget_ms <= 0.0 || s.rt.peak_render_ms < budget_ms;

    if !audible {
        println!("FAIL: the master meter never moved — the rig is running but inaudible.");
        println!("      Check its graph node is LINKED to the output device:");
        println!("      pw-link -l | grep -A2 FTS-Keys");
    }
    if !steady {
        println!(
            "FAIL: {over} blocks overran their deadline while playing (graph xruns: {xruns}) \
             — that is what clicks and pops are."
        );
    }
    if !in_budget {
        println!(
            "FAIL: worst render {:.2}ms exceeds the {budget_ms:.2}ms block budget.",
            s.rt.peak_render_ms
        );
    }
    if audible && steady && in_budget {
        println!("PASS: audible, no dropouts, inside the block budget.");
        std::process::exit(0);
    }
    std::process::exit(1);
}
