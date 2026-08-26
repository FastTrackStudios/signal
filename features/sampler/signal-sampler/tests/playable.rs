//! Verify that real packs load and produce non-zero audio when triggered.
//!
//! Skips when AudioHaven isn't mounted. Walks one representative pack from
//! each library family, calls `SamplerRig::load_pack`, fires note_on, and
//! asserts the rendered audio buffer has non-zero RMS.

use signal_sampler::{read_pack_header, PlayerPatch, SampleEngine, SamplerBank, SamplerRig};
use std::path::Path;

const SAMPLES: &[(&str, &str, u8)] = &[
    (
        "Keyscape Wurlitzer",
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
        60,
    ),
    (
        "Stylus Default",
        "/run/media/AudioHaven/Signal/Libraries/Drum Kits/Stylus RMX/Packs/Stylus RMX/Core Library/Default/library.signalpack",
        36,
    ),
    (
        "Trilian Big Acid Bass",
        "/run/media/AudioHaven/Signal/Libraries/Keys/Trilian/Packs/Synth Bass/Synth 01/Big Acid Bass/library.signalpack",
        48,
    ),
];

#[test]
fn header_parses_for_each_family() {
    for (label, path, _note) in SAMPLES {
        let p = Path::new(path);
        if !p.exists() {
            eprintln!("skip {label} (missing)");
            continue;
        }
        let h = match read_pack_header(p) {
            Ok(h) => h,
            Err(e) => panic!("{label}: header parse failed: {e}"),
        };
        eprintln!(
            "== {label} == name={:?} zones={} grooves={} samples={}",
            h.spec.name,
            h.spec.zones.len(),
            h.spec.grooves.len(),
            h.sample_count,
        );
    }
}

#[test]
fn keyscape_resolves_a_sample() {
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
    );
    if !path.exists() {
        eprintln!("skip (missing)");
        return;
    }
    let patch = PlayerPatch::from_pack(path).expect("load_pack header");
    eprintln!(
        "patch zoned={} convention map size={}",
        patch.is_zoned(),
        patch.map.total()
    );
    eprintln!(
        "first articulation: {:?}",
        patch.spec.articulations.first().map(|a| (&a.id, &a.kind))
    );
    eprintln!(
        "first section: {:?}",
        patch.spec.sections.first().map(|s| &s.id)
    );
    eprintln!("first mic: {:?}", patch.spec.mics.first().map(|m| &m.id));

    // Try resolving C4 (60) at a few articulation/dynamic combos.
    let section = patch
        .spec
        .sections
        .first()
        .map(|s| s.id.clone())
        .unwrap_or_default();
    let mic = patch
        .spec
        .mics
        .first()
        .map(|m| m.id.clone())
        .unwrap_or_default();
    let artic = patch
        .spec
        .articulations
        .first()
        .map(|a| a.id.clone())
        .unwrap_or_default();
    // Mirror the engine's velocity → dynamic mapping.
    let artic_spec = patch.spec.articulation(&artic).unwrap();
    let dynamics = &artic_spec.dynamics;
    eprintln!("dynamics list: {:?}", dynamics);
    // Mirror the engine's velocity → dynamic mapping.
    let artic_spec = patch.spec.articulation(&artic).unwrap();
    let dynamics = &artic_spec.dynamics;
    eprintln!("dynamics list: {:?}", dynamics);
    for note in [48u8, 60u8, 72u8] {
        for vel in [40u8, 80u8, 100u8, 120u8] {
            let n = dynamics.len();
            let idx = (vel as usize * n / 128).min(n - 1);
            let dyn_label = &dynamics[idx];
            let r = patch.resolve(&signal_sampler::SampleQuery {
                section_id: &section,
                articulation_id: &artic,
                mic_id: &mic,
                dynamic: dyn_label,
                target_note: note,
                direction: "",
                rr: 0,
            });
            eprintln!(
                "  note={note} vel={vel} dyn={dyn_label:?} → {}",
                if r.is_some() { "FOUND" } else { "miss" }
            );
        }
    }
}

#[test]
fn keyscape_engine_renders_audio() {
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
    );
    if !path.exists() {
        eprintln!("skip (missing)");
        return;
    }
    let patch = PlayerPatch::from_pack(path).expect("load_pack");
    let section = patch
        .spec
        .sections
        .first()
        .map(|s| s.id.clone())
        .unwrap_or_default();
    let mic = patch
        .spec
        .mics
        .first()
        .map(|m| m.id.clone())
        .unwrap_or_default();
    let mut engine = SampleEngine::new(patch, 48_000, &section, &mic);
    // Audio thread uses get_loaded (lock-free try_read) and skips voices
    // whose samples aren't decoded yet. Preload synchronously here so the
    // single render call has data to play.
    engine.preload_samples();
    engine.note_on(60, 100);
    let mut buf = vec![0.0f32; 4096 * 2]; // stereo block
    engine.render(&mut buf);
    let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
    eprintln!("rms after note_on(60, 100): {rms}");
    assert!(rms > 1e-6, "Keyscape engine produced silent buffer");
}

#[test]
fn many_libraries_render_audio() {
    let candidates: &[&str] = &[
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Rhodes - Classic.signalpack",
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/MKS-20 E Piano 2.signalpack",
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Clavichord.signalpack",
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Glock Toy Piano.signalpack",
    ];
    let mut tested = 0;
    let mut silent: Vec<String> = Vec::new();
    for path in candidates {
        let p = Path::new(path);
        if !p.exists() {
            continue;
        }
        tested += 1;
        let mut bank = SamplerBank::new(48_000);
        if let Err(e) = bank.load_pack("tui", p) {
            eprintln!("load fail {}: {e}", p.display());
            silent.push(p.display().to_string());
            continue;
        }
        // Bank.load_pack now spawns a background preloader; force a
        // synchronous preload here so this offline-render test sees a fully
        // populated cache before we fire notes.
        bank.preload_instrument("tui").expect("preload");
        let mut max_rms: f32 = 0.0;
        for note in [36u8, 48, 60, 72, 84] {
            bank.note_on("tui", note, 100);
            let mut buf = vec![0.0f32; 4096 * 2];
            bank.render(&mut buf);
            let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
            if rms > max_rms {
                max_rms = rms;
            }
            bank.note_off("tui", note);
        }
        eprintln!(
            "  {:55} max_rms={max_rms}",
            p.file_stem().unwrap().to_string_lossy()
        );
        if max_rms < 1e-6 {
            silent.push(p.display().to_string());
        }
    }
    eprintln!("tested {tested}, silent {}", silent.len());
    for s in &silent {
        eprintln!("  SILENT: {s}");
    }
    assert!(
        silent.is_empty(),
        "{} of {} packs silent",
        silent.len(),
        tested
    );
}

/// Sweep MIDI 36–96 at vel 30/64/100 and report which notes produce audio.
/// Used to diagnose "most notes silent" — runs ignored so it doesn't
/// gate CI but is one command away.
#[test]
#[ignore = "diagnostic — slow"]
fn rhodes_silence_sweep() {
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Rhodes - LA Custom.signalpack",
    );
    if !path.exists() {
        return;
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();
    let mut bank = SamplerBank::new(44100);
    bank.set_preload_profile(signal_sampler::PreloadProfile::Full);
    bank.load_pack("tui", path).expect("load_pack");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        let (loaded, total) = bank.preload_progress("tui");
        if loaded >= total && total > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let mut buf = vec![0.0f32; 256 * 2];
    let mut silent = Vec::new();

    // Snapshot probe — verify cache actually contains the path that
    // resolve would return.
    let cache = bank.cache_handle_for("tui").expect("cache");
    let probe = std::path::PathBuf::from("RR01 lacrm 36 35.flac");
    eprintln!(
        "PROBE snapshot.len={} get_loaded(\"{}\")={}",
        cache.snapshot_len(),
        probe.display(),
        cache.get_loaded(&probe).is_some(),
    );

    // Single isolated note probe to see baseline behaviour without
    // accumulated state from the sweep loop.
    bank.note_on("tui", 36, 30);
    eprintln!("PROBE note=36 vel=30: voices={}", bank.active_voices("tui"));
    let mut p = 0.0f32;
    for _ in 0..40 {
        for s in buf.iter_mut() {
            *s = 0.0;
        }
        bank.render(&mut buf);
        for &s in &buf {
            p = p.max(s.abs());
        }
    }
    eprintln!("PROBE peak={p}");
    bank.note_off("tui", 36);
    for _ in 0..40 {
        for s in buf.iter_mut() {
            *s = 0.0;
        }
        bank.render(&mut buf);
    }

    for note in 36u8..=96 {
        for vel in [30u8, 64, 100] {
            bank.note_on("tui", note, vel);
            let vc = bank.active_voices("tui");
            if vc == 0 {
                eprintln!("note={note} vel={vel}: voices=0 — note_on spawned nothing");
            }
            let mut peak = 0.0f32;
            for _ in 0..40 {
                for s in buf.iter_mut() {
                    *s = 0.0;
                }
                bank.render(&mut buf);
                for &s in &buf {
                    peak = peak.max(s.abs());
                }
            }
            bank.note_off("tui", note);
            for _ in 0..30 {
                for s in buf.iter_mut() {
                    *s = 0.0;
                }
                bank.render(&mut buf);
            }
            if peak < 0.001 {
                silent.push((note, vel, peak));
            }
        }
    }
    eprintln!(
        "silent notes (peak<0.001): {}/{}",
        silent.len(),
        (96 - 36 + 1) * 3
    );
    // Print as a per-velocity bitmap so the pattern jumps out.
    for vel in [30u8, 64, 100] {
        let bitmap: String = (36u8..=96)
            .map(|n| {
                if silent.iter().any(|(sn, sv, _)| *sn == n && *sv == vel) {
                    '.'
                } else {
                    '#'
                }
            })
            .collect();
        eprintln!("v{vel:3}: {bitmap}");
    }
}

/// Headless repro of TUI silence bug. Builds a SamplerBank directly (no
/// cpal), loads Rhodes pack, fires note 60, renders, asserts non-zero peak.
#[test]
fn rhodes_tui_path_produces_audio() {
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Rhodes - LA Custom.signalpack",
    );
    if !path.exists() {
        eprintln!("skip: pack not found");
        return;
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_test_writer()
        .try_init();

    let mut bank = SamplerBank::new(44100);
    bank.set_preload_profile(signal_sampler::PreloadProfile::Full);
    bank.load_pack("tui", path).expect("load_pack");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        let (loaded, total) = bank.preload_progress("tui");
        if loaded >= total && total > 0 {
            eprintln!("preload {loaded}/{total} done");
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    bank.note_on("tui", 60, 100);

    let mut buf = vec![0.0f32; 256 * 2];
    let mut hold_peak = 0.0f32;
    for _ in 0..50 {
        for s in buf.iter_mut() {
            *s = 0.0;
        }
        bank.render(&mut buf);
        for &s in &buf {
            hold_peak = hold_peak.max(s.abs());
        }
    }
    bank.note_off("tui", 60);
    let mut release_peak = 0.0f32;
    for _ in 0..30 {
        for s in buf.iter_mut() {
            *s = 0.0;
        }
        bank.render(&mut buf);
        for &s in &buf {
            release_peak = release_peak.max(s.abs());
        }
    }
    eprintln!("hold_peak={hold_peak} release_peak={release_peak}");
    assert!(hold_peak > 0.001, "silence while held (peak={hold_peak})");
    assert!(
        release_peak > 0.001,
        "silent during release fade (peak={release_peak})"
    );
    // Guards the release_frames envelope bug (peak=11 regression). A clean
    // single-note release should stay well below clipping.
    assert!(
        release_peak < 2.0,
        "release peak unreasonably loud — envelope regression? (peak={release_peak})"
    );
}

/// Mirror what the TUI does for Rhodes - LA Custom: open cpal, load pack,
/// fire some keys spaced over a few seconds, render. Use this to confirm
/// the audio chain is healthy for the SAME patch the user is trying
/// interactively.
#[test]
#[ignore = "interactive — produces audio on the default sink for ~3 seconds"]
fn rhodes_la_custom_audible() {
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Rhodes - LA Custom.signalpack",
    );
    if !path.exists() {
        return;
    }
    let player = match SamplerRig::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip: cpal failed: {e}");
            return;
        }
    };
    player.load_pack("tui", path).expect("load_pack");
    std::thread::sleep(std::time::Duration::from_millis(2000)); // wait for preload
    let (l, t) = player.preload_progress("tui");
    eprintln!("preload {l}/{t}");
    // Play a C major triad over ~2 seconds.
    for (delay_ms, note) in [(0, 60u8), (300, 64), (600, 67), (900, 72)] {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        eprintln!("note_on({note})");
        player.note_on("tui", note, 100);
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    for note in [60u8, 64, 67, 72] {
        player.note_off("tui", note);
    }
    let stats = player.audio_stats();
    eprintln!(
        "stats: callbacks={} max_render_us={} stream_errors={} overruns={}",
        stats.callbacks, stats.max_render_us, stats.stream_errors, stats.callback_overruns
    );
}

/// Full TUI-style integration: open the actual cpal output, load Keyscape,
/// fire a few notes, sleep so the audio callback drains them, query stats.
/// If voice activity is non-zero, the audio chain is end-to-end alive.
#[test]
fn keyscape_via_player_full_chain() {
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
    );
    if !path.exists() {
        eprintln!("skip (missing)");
        return;
    }
    let player = match SamplerRig::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip — cpal output unavailable: {e}");
            return;
        }
    };
    player.load_pack("tui", path).expect("load_pack via player");
    // Settle: skip the lock-contention fallout from the synchronous preload
    // (callback was running but couldn't grab the bank mutex while decode
    // held it). Reset stats and measure clean post-preload behaviour.
    std::thread::sleep(std::time::Duration::from_millis(200));
    player.reset_audio_stats();

    for note in [60u8, 64, 67, 72] {
        player.note_on("tui", note, 100);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    let stats = player.audio_stats();
    eprintln!(
        "post-preload stats: callbacks={} overruns={} stream_errors={} max_render_us={} lock_misses={}",
        stats.callbacks,
        stats.callback_overruns,
        stats.stream_errors,
        stats.max_render_us,
        stats.lock_misses,
    );
    for note in [60u8, 64, 67, 72] {
        player.note_off("tui", note);
    }
    assert!(stats.callbacks > 50, "cpal callbacks not flowing");
    assert!(
        stats.max_render_us < 5_000,
        "render time {}us exceeds 256-frame budget at 48k",
        stats.max_render_us,
    );
    assert_eq!(stats.stream_errors, 0, "stream errors after preload");
    assert_eq!(stats.callback_overruns, 0, "overruns after preload");
}

/// Verify the Kontakt-style background preloader: load_pack returns
/// quickly while a worker thread fills the cache, and notes become audible
/// shortly afterwards once enough samples are decoded.
#[test]
fn keyscape_background_preload_streams_in() {
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
    );
    if !path.exists() {
        eprintln!("skip (missing)");
        return;
    }
    let player = match SamplerRig::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip — cpal output unavailable: {e}");
            return;
        }
    };
    let load_start = std::time::Instant::now();
    player.load_pack("tui", path).expect("load_pack");
    let load_elapsed = load_start.elapsed();
    eprintln!("load_pack returned in {} ms", load_elapsed.as_millis());
    for ms in [100, 500, 1000, 2000, 3000] {
        std::thread::sleep(std::time::Duration::from_millis(
            ms - if ms == 100 { 0 } else { ms / 2 },
        ));
        let (l, t) = player.preload_progress("tui");
        eprintln!("  +{ms} ms → {l}/{t}");
    }
    assert!(
        load_elapsed.as_millis() < 250,
        "load_pack blocked for {} ms — preload should be backgrounded",
        load_elapsed.as_millis()
    );

    // Wait for the preloader to make meaningful progress, then fire notes.
    std::thread::sleep(std::time::Duration::from_secs(3));
    let (loaded, total) = player.preload_progress("tui");
    eprintln!("after 3s wait: preload {loaded}/{total}");
    assert!(loaded > 0, "preloader should have decoded some samples");
    assert_eq!(total, 512, "performance preload should report its target");

    player.reset_audio_stats();
    for note in [60u8, 64, 67, 72] {
        player.note_on("tui", note, 100);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    let stats = player.audio_stats();
    eprintln!(
        "post-stream stats: callbacks={} max_render_us={} stream_errors={}",
        stats.callbacks, stats.max_render_us, stats.stream_errors,
    );
    assert!(stats.callbacks > 50, "no audio callbacks fired");
    assert!(
        stats.max_render_us < 5_000,
        "render time {}us exceeds budget",
        stats.max_render_us,
    );
    assert_eq!(stats.callback_overruns, 0);
}

/// Verify middle-out priority preloading: shortly after `load_pack`, the
/// central note range should be cached while the extremes are not.
#[test]
fn keyscape_middle_out_priority() {
    use signal_sampler::PlayerPatch;
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
    );
    if !path.exists() {
        eprintln!("skip (missing)");
        return;
    }

    let player = match SamplerRig::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip — cpal output unavailable: {e}");
            return;
        }
    };
    player.load_pack("tui", path).expect("load_pack");
    // Give the preloader long enough to do ~10–20% of the work.
    std::thread::sleep(std::time::Duration::from_millis(800));
    let (l, t) = player.preload_progress("tui");
    eprintln!("preloaded {l}/{t} after 800ms");
    assert!(l > 0 && l < t, "expected partial preload");

    // Inspect the patch's expected ordering: paths near middle C should
    // come BEFORE paths far from middle C.
    let patch = PlayerPatch::from_pack(path).expect("read pack");
    let ordered = patch.sample_paths_centered(60);
    let half = ordered.len() / 4;
    let mut near = 0;
    let mut far = 0;
    for p in ordered.iter().take(half) {
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        // Keyscape stems contain the note number as the second whitespace-
        // separated field (e.g. `NMWurl 60 a 80-89-o`).
        if let Some(note_str) = stem.split_whitespace().nth(1) {
            if let Ok(n) = note_str.parse::<i32>() {
                if (n - 60).abs() <= 24 {
                    near += 1
                } else {
                    far += 1
                }
            }
        }
    }
    eprintln!("first quarter of priority list: {near} within ±24 of C4, {far} beyond",);
    assert!(
        near > far * 4,
        "priority list isn't middle-out: near={near} far={far}",
    );
}

/// Verify a `.signalblock` file parses, resolves its referenced
/// `.signalpack`, and produces audio when triggered.
#[test]
fn signalblock_loads_and_plays() {
    use signal_sampler::BlockSpec;
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Blocks/Tama Bubinga Kick.signalblock",
    );
    if !path.exists() {
        eprintln!("skip (missing)");
        return;
    }
    let spec = BlockSpec::from_file(path).expect("parse .signalblock");
    eprintln!(
        "block: name={:?} type={:?} pack={:?} gain_db={}",
        spec.name, spec.block_type, spec.pack, spec.params.gain_db,
    );
    assert_eq!(spec.block_type, "sampler");
    assert!(spec.pack.contains("Kick"));

    let mut bank = SamplerBank::new(48_000);
    bank.load_block("kick", path).expect("load_block via bank");
    bank.preload_instrument("kick").expect("preload");
    bank.note_on("kick", 36, 100);
    let mut buf = vec![0.0f32; 4096 * 2];
    bank.render(&mut buf);
    let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
    eprintln!("rms after note 36: {rms}");
    assert!(rms > 1e-6, "block produced silent buffer");
}

/// Per-piece Engine: parses + plays the bubinga kick via `.signalengine`.
#[test]
fn mm2_kick_engine_loads_and_plays() {
    use signal_sampler::EngineSpec;
    let engine_path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Engines/MM2 Tama Bubinga Kick.signalengine",
    );
    if !engine_path.exists() {
        eprintln!("skip (missing — author the file first)");
        return;
    }
    let spec = EngineSpec::from_file(engine_path).expect("parse .signalengine");
    eprintln!(
        "engine: name={:?} type={:?} layers={} ports={}",
        spec.name,
        spec.engine_type,
        spec.layers.len(),
        spec.ports.len()
    );
    assert_eq!(spec.engine_type, "kick");

    let mut bank = SamplerBank::new(48_000);
    let dir = engine_path.parent().unwrap();
    bank.load_engine_spec("kick", &spec, dir)
        .expect("load_engine_spec");
    bank.preload_instrument("kick").expect("preload");
    bank.note_on("kick", 36, 100);
    let mut buf = vec![0.0f32; 4096 * 2];
    bank.render(&mut buf);
    let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
    eprintln!("kick rms: {rms}");
    assert!(rms > 1e-6, "kick engine produced silent buffer");
}

/// MM2 Modern Kit Preset: composes 14 per-piece Engines, routes per note,
/// streams in priority order (kick/snare first, china last).
#[test]
fn mm2_preset_loads_with_priority() {
    use signal_sampler::PresetSpec;
    let preset_path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Presets/Angr.signalpreset",
    );
    if !preset_path.exists() {
        eprintln!("skip (missing — author the file first)");
        return;
    }
    let preset = PresetSpec::from_file(preset_path).expect("parse .signalpreset");
    eprintln!(
        "preset: {:?}, {} engines, {} note routes",
        preset.name,
        preset.engines.len(),
        preset.note_routing.len()
    );
    assert!(preset.engines.len() >= 10);

    let mut bank = SamplerBank::new(48_000);
    let dir = preset_path.parent().unwrap();
    let ids = bank
        .load_preset_spec("mm2", &preset, dir)
        .expect("load_preset_spec");
    eprintln!("registered {} engine ids", ids.len());

    std::thread::sleep(std::time::Duration::from_millis(800));
    let (kick_l, kick_t) = bank.preload_progress("mm2:kick");
    let (china_l, china_t) = bank.preload_progress("mm2:china");
    eprintln!("after 800ms: kick {kick_l}/{kick_t}, china {china_l}/{china_t}");
    if kick_t > 0 {
        assert!(kick_l > 0, "kick should have started loading first");
    }
    // Fire the kick — note 36 should route via Preset note_routing.
    bank.note_on("mm2", 36, 100);
    let mut buf = vec![0.0f32; 4096 * 2];
    bank.render(&mut buf);
    let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
    eprintln!("preset kick rms: {rms}");
    assert!(rms > 1e-6, "preset kick produced silent buffer");
}

// ── Multi-channel routing tests ───────────────────────────────────────────

const ROOMS_PRESET: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Presets/Rooms Demo.signalpreset";

/// Multi-mic engine: load the bubinga kick via the multi-mic engine
/// spec, render, and verify both `main` and `room` ports carry audio.
#[test]
fn multi_mic_kick_renders_per_port() {
    use signal_sampler::{EngineSpec, PresetSpec};
    let preset_path = Path::new(ROOMS_PRESET);
    if !preset_path.exists() {
        eprintln!("skip (missing — author Rooms Demo preset)");
        return;
    }
    // Confirm the multi-mic engine parses with two layers + ports.
    let kick_engine = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Engines/MM2 Bubinga Kick MultiMic.signalengine",
    );
    let spec = EngineSpec::from_file(kick_engine).expect("parse multi-mic engine");
    assert_eq!(spec.layers.len(), 2, "expected in + room layers");
    assert_eq!(spec.ports.len(), 2, "expected main + room ports");

    // Load the preset, preload kick samples, fire a kick, and read both
    // engine port buffers from the bank.
    let preset = PresetSpec::from_file(preset_path).expect("parse preset");
    let dir = preset_path.parent().unwrap();
    let mut bank = SamplerBank::new(48_000);
    bank.load_preset_spec("demo", &preset, dir)
        .expect("load preset");
    bank.preload_instrument("demo:kick").expect("preload kick");
    bank.preload_instrument("demo:snare")
        .expect("preload snare");

    bank.note_on("demo", 36, 100);
    let mut master = vec![0.0f32; 4096 * 2];
    bank.render(&mut master);

    let main_port = bank
        .preset_engine_port_buffer("demo", "kick", "main")
        .expect("kick.main port");
    let room_port = bank
        .preset_engine_port_buffer("demo", "kick", "room")
        .expect("kick.room port");
    let main_rms: f32 =
        (main_port.iter().map(|s| s * s).sum::<f32>() / main_port.len() as f32).sqrt();
    let room_rms: f32 =
        (room_port.iter().map(|s| s * s).sum::<f32>() / room_port.len() as f32).sqrt();
    eprintln!("multi_mic_kick: main={main_rms} room={room_rms}");
    assert!(main_rms > 1e-6, "kick.main port silent");
    assert!(room_rms > 1e-6, "kick.room port silent");
}

/// Routing graph: kick.main and kick.room are routed differently.
/// Their port buffers must contain DIFFERENT audio (different mic
/// captures of the same hit).
#[test]
fn preset_routing_graph_separates_buses() {
    use signal_sampler::PresetSpec;
    let preset_path = Path::new(ROOMS_PRESET);
    if !preset_path.exists() {
        eprintln!("skip (missing)");
        return;
    }
    let preset = PresetSpec::from_file(preset_path).expect("parse preset");
    let dir = preset_path.parent().unwrap();
    let mut bank = SamplerBank::new(48_000);
    bank.load_preset_spec("demo", &preset, dir)
        .expect("load preset");
    bank.preload_instrument("demo:kick").expect("preload kick");

    bank.note_on("demo", 36, 100);
    let mut master = vec![0.0f32; 4096 * 2];
    bank.render(&mut master);

    let main_port = bank
        .preset_engine_port_buffer("demo", "kick", "main")
        .unwrap();
    let room_port = bank
        .preset_engine_port_buffer("demo", "kick", "room")
        .unwrap();
    // Different mic captures of the same source — buffers must differ.
    let mut diff_count = 0usize;
    for (a, b) in main_port.iter().zip(room_port.iter()) {
        if (a - b).abs() > 1e-6 {
            diff_count += 1;
        }
    }
    eprintln!("differing samples: {diff_count}/{}", main_port.len());
    assert!(
        diff_count > main_port.len() / 100,
        "main and room port carry the same audio — multi-mic split failed"
    );
}

/// Module mute litmus: with the Rooms module live, master receives
/// dry close-mic + parallel rooms. Muting the module zeros the rooms
/// contribution while close mics keep playing — master energy drops
/// but is still non-zero.
#[test]
fn module_input_mute_works() {
    use signal_sampler::PresetSpec;
    let preset_path = Path::new(ROOMS_PRESET);
    if !preset_path.exists() {
        eprintln!("skip (missing)");
        return;
    }
    let preset = PresetSpec::from_file(preset_path).expect("parse preset");
    let dir = preset_path.parent().unwrap();
    let mut bank = SamplerBank::new(48_000);
    bank.load_preset_spec("demo", &preset, dir)
        .expect("load preset");
    bank.preload_instrument("demo:kick").expect("preload kick");

    // Pass 1: rooms ON.
    bank.note_on("demo", 36, 100);
    let mut master_a = vec![0.0f32; 4096 * 2];
    bank.render(&mut master_a);
    let rms_a: f32 = (master_a.iter().map(|s| s * s).sum::<f32>() / master_a.len() as f32).sqrt();

    // Pass 2: mute Rooms module, fresh hit.
    bank.note_off("demo", 36);
    let muted = bank.set_module_muted("demo", "rooms", true);
    assert!(muted, "rooms module not found");
    bank.note_on("demo", 36, 100);
    let mut master_b = vec![0.0f32; 4096 * 2];
    bank.render(&mut master_b);
    let rms_b: f32 = (master_b.iter().map(|s| s * s).sum::<f32>() / master_b.len() as f32).sqrt();

    eprintln!("master rms — rooms on: {rms_a}, rooms muted: {rms_b}");
    assert!(rms_a > 1e-6, "master silent with rooms on");
    assert!(
        rms_b > 1e-6,
        "master silent with rooms muted (close mic should still play)"
    );
    assert!(
        rms_b < rms_a,
        "muting rooms didn't reduce master energy: a={rms_a} b={rms_b}"
    );
}

/// Reproduce what the TUI does: load_pack via SamplerBank, fire note_on, mix.
/// Catches integration issues that don't show up loading via PlayerPatch alone.
#[test]
fn keyscape_via_bank_renders_audio() {
    let path = Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
    );
    if !path.exists() {
        eprintln!("skip (missing)");
        return;
    }
    let mut bank = SamplerBank::new(48_000);
    bank.load_pack("tui", path).expect("load_pack via bank");
    bank.preload_instrument("tui").expect("preload"); // wait for streaming preload
    bank.note_on("tui", 60, 100);
    let mut buf = vec![0.0f32; 4096 * 2];
    bank.render(&mut buf);
    let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
    eprintln!("bank rms: {rms}");
    assert!(rms > 1e-6, "bank produced silent buffer for Keyscape");
}
