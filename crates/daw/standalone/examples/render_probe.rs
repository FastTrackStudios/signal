//! Headless render probe: load an RPP, materialize, render blocks at a
//! given time, print per-stage RMS — pinpoints where audio dies.
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::media_bay::ProjectRelativeResolver;
use daw_standalone::project_loader::load_rpp_via_bay;
use daw_standalone::sync::Standalone;
use std::path::PathBuf;

fn main() {
    let rpp = std::env::args().nth(1).expect("rpp");
    let at: f64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30.0);
    let text = std::fs::read_to_string(&rpp).expect("read");
    let dir = PathBuf::from(&rpp).parent().map(PathBuf::from).unwrap();
    let daw = Standalone::new();
    daw.media_bay()
        .set_file_resolver(Box::new(ProjectRelativeResolver::new(dir)));
    let t0 = std::time::Instant::now();
    let (proj, audio) = load_rpp_via_bay(&daw, "probe", &rpp, &text).expect("load");
    println!(
        "loaded {} tracks / {} sources in {:?} ({} failed)",
        proj.track_count,
        audio.loaded,
        t0.elapsed(),
        audio.failed.len()
    );
    let sr = 48000u32;
    let r = ProjectRenderer::new(&daw, &proj.project_guid, sr);
    let start = (at * sr as f64) as u64;
    let blocks: u64 = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let mut sum = 0.0f64;
    let mut n = 0usize;
    let mut peak = 0.0f32;
    // Warm the snapshot cache so the timing below is steady-state.
    let _ = r.render_block(start, 1024);
    let t1 = std::time::Instant::now();
    for b in 0..blocks {
        let buf = r.render_block(start + b * 1024, 1024);
        for s in &buf.samples {
            sum += (*s as f64) * (*s as f64);
            peak = peak.max(s.abs());
            n += 1;
        }
    }
    let elapsed = t1.elapsed();
    println!(
        "master @{at}s: rms={:.6} peak={peak:.6} over {n} samples",
        (sum / n as f64).sqrt()
    );
    let audio_seconds = (blocks as f64 * 1024.0) / sr as f64;
    println!(
        "render: {elapsed:?} for {audio_seconds:.3}s of audio ({:.1}x realtime, {:?}/block)",
        audio_seconds / elapsed.as_secs_f64(),
        elapsed / blocks as u32
    );
}
