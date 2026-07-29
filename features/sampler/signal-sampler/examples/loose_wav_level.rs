//! Level check for the LOOSE-WAV (non-pack) instrument path: load a single
//! wav as a one-zone percussion instrument, strike it, and compare the
//! rendered peak against the file's own peak. They should be within a few
//! dB; a large constant deficit means the loose-wav path is mis-scaling.
//!   cargo run --release -p signal-sampler --example loose_wav_level -- <file.wav>

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from(
        std::env::args().nth(1).ok_or("usage: loose_wav_level <file.wav>")?,
    );
    let dir = path.parent().unwrap().to_path_buf();
    let file = path.file_name().unwrap().to_string_lossy().into_owned();

    // What the decoder itself sees.
    let data = signal_sampler::engine::cache::load_sample(&path)?;
    let decoded_peak =
        (0..data.pcm.len()).fold(0.0f32, |a, i| a.max(data.pcm.sample(i).abs()));
    println!(
        "decoded peak: {decoded_peak:.4}  ({} frames, {} ch)",
        data.num_frames, data.channels
    );

    let styx = format!(
        r#"
name "level test"
category drum
instrument perc
sections ( {{ id main, label Main, lowest_note "C-1", highest_note "G9" }} )
mics ( {{ id default, label Default, kind close }} )
articulations ( {{ id hit, label Hit, kind @Short, rr 1 }} )
zones (
    {{ file "{file}", key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127, articulation hit, mic default }}
)
"#
    );
    let spec_path = std::env::temp_dir().join("fts-loose-level.styx");
    std::fs::write(&spec_path, styx)?;

    let rig = signal_sampler::SamplerRig::new_offline(48_000);
    rig.load_instrument("t".to_string(), &spec_path, Some(&dir), "main", "default")?;
    let chan: u8 = std::env::var("CH").ok().and_then(|v| v.parse().ok()).unwrap_or(9);
    let note: u8 = std::env::var("NOTE").ok().and_then(|v| v.parse().ok()).unwrap_or(36);
    if std::env::var("NO_SET_CH").is_err() {
        rig.set_midi_channel("t".to_string(), chan);
    }
    rig.set_default_instrument("t".to_string());
    let _ = rig.preload_instrument("t");
    println!("chan {chan} note {note}");

    let mut buf = vec![0.0f32; 512 * 2];
    for _ in 0..40 {
        buf.iter_mut().for_each(|s| *s = 0.0);
        let _ = rig.render_offline(&mut buf);
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    rig.midi_message(chan, 0x90, note, 127);
    let mut peak = 0.0f32;
    for _ in 0..60 {
        buf.iter_mut().for_each(|s| *s = 0.0);
        rig.render_offline(&mut buf)?;
        for &s in buf.iter() {
            peak = peak.max(s.abs());
        }
    }
    println!("rendered peak: {peak:.5}");
    let ratio = decoded_peak / peak.max(1e-9);
    println!("decoded / rendered = {ratio:.1}x  ({:.1} dB)", 20.0 * ratio.log10());
    Ok(())
}
