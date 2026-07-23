//! Articulation cycle — plays one note in EACH CSS articulation (via CC58
//! keyswitches) so every articulation can be heard / verified functional.
//!
//! LIVE path (exact-CSS reactive engine, NOT document mode): dispatch CC58 +
//! note-on/off and render the block directly. Each articulation gets a 3 s
//! slot. Prints a manifest and reports which produced sound vs came out silent.
//!
//! ```text
//! cargo run --release -p signal-sampler --example artic_cycle
//! ```

use std::path::PathBuf;

use signal_sampler::SamplerRig;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str =
    "features/rigs/orchestra/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;

/// (CC58 value, label, expected zone-articulation tag the engine should spawn).
/// MeasuredTremolo has no dedicated samples (KSP-scripted) → falls back to
/// Tremolo, which is the closest correct behavior.
const ARTICS: &[(u8, &str, &str)] = &[
    (3, "Legato-LowLatency", "Nonvib"),
    (8, "Legato-Expressive", "Nonvib"),
    (13, "Spiccato", "Spiccato"),
    (18, "Staccatissimo", "Staccatissimo"),
    (23, "Staccato", "Staccato"),
    (28, "Sfz", "Sfz"),
    (33, "Pizzicato", "Pizzicato"),
    (38, "Bartokpizz", "Bartokpizz"),
    (43, "ColLegno", "Clegno"),
    (48, "Trills", "Trills"), // HTrills / WTrills by velocity
    (53, "Harmonics", "Harm"),
    (58, "Tremolo", "Tremolo"),
    (63, "MeasuredTremolo", "Tremolo"),
    (68, "Marcato-noOverlay", "Marcato"),
    (73, "Marcato-overlay", "Marcato"),
];

const NOTE: u8 = 67; // G4 (CSS reference note)
const SLOT_S: f64 = 3.0;
const HOLD_S: f64 = 2.0;

fn write_wav(path: &std::path::Path, samples: &[f32]) -> eyre::Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn main() -> eyre::Result<()> {
    let css_root = PathBuf::from(CSS_ROOT);
    let zones = css_root.join("_patches/1st Violins/library.styx");
    if !zones.exists() {
        eyre::bail!("CSS Violin 1 patch not found at {}", zones.display());
    }
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    rig.load_instrument_with_config(
        ID,
        &PathBuf::from(CSS_CONFIG),
        &zones,
        &css_root,
        "1st Violins",
        "Mix",
    )?;
    rig.set_solo_mic(ID, Some("Mix".into()));
    rig.set_midi_channel(ID, 0); // bind channel 0 → this instrument (live dispatch)
    rig.set_attack_ms(ID, 20);
    rig.set_release_ms(ID, 400);

    rig.set_trace_enabled(ID, true);
    // Baseline expression up front.
    rig.cc(ID, 1, 84); // dynamic (mid)
    rig.cc(ID, 2, 0); // non-vib

    // Render helper: advance the engine `secs` seconds into `out`.
    let render = |rig: &SamplerRig, out: &mut Vec<f32>, secs: f64| -> eyre::Result<()> {
        let frames = (secs * SR as f64) as usize;
        let mut buf = vec![0.0f32; frames * 2];
        rig.render_offline(&mut buf)?;
        out.extend_from_slice(&buf);
        Ok(())
    };

    let mut audio: Vec<f32> = Vec::new();
    for (ks, _label, _expect) in ARTICS {
        rig.cc(ID, 58, *ks);
        render(&rig, &mut audio, 0.1)?; // let the keyswitch settle
        rig.warm_note(ID, NOTE); // preload the articulation's sample (offline cache)
        rig.note_on(ID, NOTE, 90);
        render(&rig, &mut audio, HOLD_S)?;
        rig.note_off(ID, NOTE);
        render(&rig, &mut audio, SLOT_S - HOLD_S - 0.1)?;
    }

    let trace = rig.render_trace(ID);
    let out = PathBuf::from("target/artic_cycle.wav");
    write_wav(&out, &audio)?;

    use signal_sampler::TraceKind;
    let mono: Vec<f32> = audio.chunks(2).map(|c| 0.5 * (c[0] + c[1])).collect();
    let rms = |a: usize, b: usize| -> f32 {
        let (a, b) = (a.min(mono.len()), b.min(mono.len()));
        if b <= a {
            return 0.0;
        }
        (mono[a..b].iter().map(|&v| v * v).sum::<f32>() / (b - a) as f32).sqrt()
    };
    // Verdict is trace-based: did the EXPECTED articulation spawn at real gain?
    // (RMS alone can't tell a right-but-quiet artic — col legno, harmonics —
    // from a wrong one.) `Trills` matches HTrills/WTrills by substring.
    println!(
        "wrote {} ({:.1}s)\n\n{:>7} {:<3} {:<18} {:<14} {:>8}  status",
        out.display(),
        audio.len() as f64 / 2.0 / SR as f64,
        "time",
        "cc",
        "articulation",
        "spawned",
        "rms"
    );
    let mut broken = Vec::new();
    for (i, (ks, label, expect)) in ARTICS.iter().enumerate() {
        let base = i as f64 * SLOT_S;
        let (a, b) = (
            (base * SR as f64) as u64,
            ((base + SLOT_S) * SR as f64) as u64,
        );
        // Best (loudest) spawn whose articulation matches the expected tag.
        let hit = trace
            .events
            .iter()
            .filter(|e| e.frame >= a && e.frame < b)
            .filter_map(|e| match &e.kind {
                TraceKind::VoiceSpawn(v) if v.gain > 0.01 && v.articulation.contains(expect) => {
                    Some((v.articulation.clone(), v.gain))
                }
                _ => None,
            })
            .max_by(|x, y| x.1.total_cmp(&y.1));
        let e = rms(
            ((base + 0.15) * SR as f64) as usize,
            ((base + 2.1) * SR as f64) as usize,
        );
        let (spawned, status) = match hit {
            Some((art, g)) => (format!("{art} g={g:.2}"), "✓ ok"),
            None => {
                broken.push(*label);
                ("(none)".to_string(), "✗ BROKEN")
            }
        };
        println!("{base:>6.1}s {ks:<3} {label:<18} {spawned:<14} {e:>8.4}  {status}");
    }
    println!();
    if broken.is_empty() {
        println!(
            "All {} articulations spawn the correct articulation. ✓",
            ARTICS.len()
        );
    } else {
        println!("BROKEN ({}): {}", broken.len(), broken.join(", "));
    }
    Ok(())
}
