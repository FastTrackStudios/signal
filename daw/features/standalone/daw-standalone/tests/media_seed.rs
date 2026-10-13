//! Seed a project with a real on-disk WAV via `media_seed::seed_media_tracks`
//! and confirm the render graph plays it — the end-to-end "stems are audible"
//! proof for the setlist demo.
#![cfg(feature = "decode")]

use std::io::Write;
use std::path::Path;

use daw_proto::ProjectInfo;
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::media_seed::{StemSpec, seed_media_tracks};
use daw_standalone::sync::Standalone;

/// Write a mono 16-bit PCM sine WAV — the minimal real RIFF/PCM file
/// `PcmFile::open` will memory-map.
fn write_sine_wav(path: &Path, seconds: f32, sample_rate: u32, freq: f32) {
    let frames = (seconds * sample_rate as f32) as usize;
    let mut pcm = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let t = n as f32 / sample_rate as f32;
        let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        pcm.extend_from_slice(&((s * i16::MAX as f32) as i16).to_le_bytes());
    }
    let data_len = pcm.len() as u32;
    let byte_rate = sample_rate * 2; // mono, 16-bit
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap(); // fmt chunk size
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
    f.write_all(&sample_rate.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap(); // block align
    f.write_all(&16u16.to_le_bytes()).unwrap(); // bits
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    f.write_all(&pcm).unwrap();
}

fn rms(buf: &daw_standalone::audio_engine::render::StereoBuffer) -> f32 {
    let mut s = 0.0f64;
    for i in 0..buf.frames {
        let l = buf.samples[i * 2] as f64;
        let r = buf.samples[i * 2 + 1] as f64;
        s += l * l + r * r;
    }
    ((s / (buf.frames.max(1) * 2) as f64).sqrt()) as f32
}

#[test]
fn seeded_disk_stem_renders_audible() {
    let sr = 48_000u32;
    let wav = std::env::temp_dir().join("fts_media_seed_tone.wav");
    write_sine_wav(&wav, 1.0, sr, 220.0);

    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "seed".into(),
        path: String::new(),
    });

    let stems = vec![StemSpec::new(
        "Tone",
        wav.to_string_lossy().to_string(),
        Some("Group"),
    )];
    let report = seed_media_tracks(&daw, &guid, &stems, 0.0, 1.0);

    assert_eq!(
        report.materialize.loaded, 1,
        "stem should mmap from disk (failed: {:?})",
        report.materialize.failed
    );
    assert_eq!(report.tracks_created, 1, "one stem track");
    assert_eq!(report.folders_created, 1, "one group folder");

    // Render the first half-second: the seeded sine must be audible.
    let r = ProjectRenderer::new(&daw, &guid, sr);
    let block = r.render_block(0, (sr / 2) as usize);
    let level = rms(&block);
    assert!(
        level > 0.001,
        "seeded on-disk stem should render audible, rms={level}"
    );

    let _ = std::fs::remove_file(&wav);
}
