//! Where a single NAM block's thirty milliseconds go: parse, or prewarm?
use std::time::Instant;

fn main() {
    let def = signal_guitar::profiles::worship_def();
    let dps = signal_guitar::profiles::drive_presets();
    let profile = signal_guitar::profiles::build_profile(&def, &dps);
    let mut paths: Vec<String> = Vec::new();
    for p in &profile.patches {
        for b in &p.chain {
            if !b.nam.is_empty() && !paths.contains(&b.nam) {
                paths.push(b.nam.clone());
            }
        }
    }
    // Warm.
    for p in &paths {
        if let Ok(mut m) = neural_amp_modeler::NamModel::load(p) {
            m.reset(48_000.0, 512);
        }
    }
    println!("{:<44} {:>10} {:>10}", "model", "load ms", "reset ms");
    let (mut lt, mut rt) = (0.0f64, 0.0f64);
    for p in &paths {
        let t = Instant::now();
        let Ok(mut m) = neural_amp_modeler::NamModel::load(p) else {
            continue;
        };
        let l = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        m.reset(48_000.0, 512);
        let r = t.elapsed().as_secs_f64() * 1000.0;
        lt += l;
        rt += r;
        let stem = std::path::Path::new(p)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().to_string());
        println!("{:<44.44} {l:>10.2} {r:>10.2}", stem);
    }
    println!("{:<44} {lt:>10.2} {rt:>10.2}", "TOTAL (one each)");
    println!(
        "\n48 loads at this rate: load {:.0} ms, reset {:.0} ms",
        lt / paths.len() as f64 * 48.0,
        rt / paths.len() as f64 * 48.0
    );
}
