//! Focused legato diagnostic: drive one G4→A4 legato transition offline and
//! report what actually happens — voices spawned, peak/RMS through the hold, and
//! any sample/cache misses. Ground truth for "legato is weird/silent".
//!
//! ```text
//! cargo run --release -p signal-sampler --example legato_test
//! ```

use signal_sampler::SamplerRig;
use std::path::PathBuf;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str = "features/rigs/orchestra/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;

fn rms_db(b: &[f32]) -> f32 {
    let r = (b.iter().map(|x| x * x).sum::<f32>() / b.len() as f32).sqrt();
    if r > 0.0 {
        20.0 * r.log10()
    } else {
        -99.0
    }
}

fn main() -> eyre::Result<()> {
    let css_root = PathBuf::from(CSS_ROOT);
    let spec = css_root
        .join("_patches")
        .join("1st Violins")
        .join("library.styx");
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    rig.load_instrument_with_config(
        ID,
        &PathBuf::from(CSS_CONFIG),
        &spec,
        &css_root,
        "1st Violins",
        "Mix",
    )?;
    rig.set_solo_mic(ID, Some("Mix".into()));
    // CSS-parity harness: reproduce Kontakt's expressive reactive latency.
    // The strict live policy (PlayMode::StrictLive) would otherwise force the
    // low_latency tables regardless of the MIDI's CC58 "expressive" request.
    rig.set_legato_mode(ID, true, true);
    rig.set_attack_ms(ID, 20);
    rig.set_release_ms(ID, 400);
    rig.cc(ID, 58, 8); // Expressive Legato
    rig.cc(ID, 1, 90);
    rig.cc(ID, 2, 90);

    let warm = |n: u8| {
        let w = rig.warm_note(ID, n);
        println!(
            "warm {n}: loaded={} failed={} bytes={}",
            w.loaded, w.failed, w.bytes
        );
    };
    // C4(60) → D4(62): expect to HEAR C then a transition arriving at D (not E).
    let (from, to) = (60u8, 62u8);
    warm(from);
    warm(to);
    let mut acc: Vec<f32> = Vec::new();
    let render = |rig: &SamplerRig, label: &str, frames: usize, acc: &mut Vec<f32>| {
        let mut b = vec![0.0f32; frames * 2];
        rig.render_offline(&mut b).ok();
        let peak = b.iter().fold(0f32, |m, &s| m.max(s.abs()));
        println!(
            "  {label}: voices={} rms={:.1}dB peak={peak:.4}",
            rig.active_voices(ID),
            rms_db(&b)
        );
        acc.extend_from_slice(&b);
    };

    println!("\n-- C4 note-on (first note) --");
    rig.note_on(ID, from, 90);
    render(&rig, "C4 +0.3s", SR as usize * 3 / 10, &mut acc);
    println!("-- D4 note-on (legato, vel90 → expect arrive at D) --");
    rig.note_on(ID, to, 90);
    for i in 0..10 {
        render(
            &rig,
            &format!("D4 +{}ms", (i + 1) * 200),
            SR as usize / 5,
            &mut acc,
        );
    }
    rig.note_off(ID, to);
    render(&rig, "rel", SR as usize / 2, &mut acc);

    // Write the held note to wav for pitch verification (legato_pitch).
    let mut f = std::fs::File::create("legato_cd.wav")?;
    use std::io::Write;
    let n = acc.len() as u32;
    let db = n * 2;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + db).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&SR.to_le_bytes())?;
    f.write_all(&(SR * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&db.to_le_bytes())?;
    for &s in &acc {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    println!(
        "\nwrote legato_cd.wav ({:.1}s)",
        acc.len() as f32 / 2.0 / SR as f32
    );
    Ok(())
}
