//! Sanity-check that retag-packs leaves real packs parseable with new tags.
//! Skips when the AudioHaven mount isn't present.

use signal_sampler::read_pack_header;
use std::path::Path;

fn show(label: &str, path: &Path) {
    if !path.exists() {
        eprintln!("skip {label} (missing)");
        return;
    }
    let h = match read_pack_header(path) {
        Ok(h) => h,
        Err(e) => panic!("{}: {e}", path.display()),
    };
    eprintln!("== {label} ==");
    eprintln!("  name:       {}", h.spec.name);
    eprintln!("  instrument: {:?}", h.spec.instrument);
    eprintln!("  category:   {:?}", h.spec.category);
    eprintln!("  style:      {:?}", h.spec.style);
    eprintln!("  tags:       {} entries", h.spec.tags.len());
    for t in h.spec.tags.iter().take(5) {
        eprintln!("    [{:?}] {:?}", t.category, t.value);
    }
}

#[test]
fn retag_round_trip_real_packs() {
    show(
        "Stylus suite",
        Path::new(
            "/run/media/AudioHaven/Signal/Libraries/Drum Kits/Stylus RMX/Packs/Stylus RMX/Core Library/RMX Grooves/51-Scuba Duba/library.signalpack",
        ),
    );
    show(
        "Keyscape Wurlitzer",
        Path::new(
            "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
        ),
    );
    show(
        "Trilian",
        Path::new(
            "/run/media/AudioHaven/Signal/Libraries/Keys/Trilian/Packs/Synth Bass/Synth 01/Big Acid Bass/library.signalpack",
        ),
    );
}
