//! One-off mic-data probe across representative packs.

use signal_sampler::read_pack_header;
use std::collections::HashMap;
use std::path::Path;

#[test]
fn probe_rhodes_la_custom() {
    let p =
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Rhodes - LA Custom.signalpack";
    let path = Path::new(p);
    if !path.exists() {
        return;
    }
    let h = read_pack_header(path).unwrap();
    eprintln!(
        "mics ({}): {:?}",
        h.spec.mics.len(),
        h.spec.mics.iter().map(|m| &m.id).collect::<Vec<_>>()
    );
    eprintln!("zones: {}", h.spec.zones.len());
    eprintln!("articulations: {}", h.spec.articulations.len());
    eprintln!("samples: {}", h.sample_count);
}

#[test]
fn probe_mic_data_across_packs() {
    let paths = [
        "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Packs/Kick/22x18'' Tama Starclassic Bubinga Kick.signalpack",
        "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Packs/Snare/14x5.5'' DW Collector's Steel Snare.signalpack",
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Wurlitzer 200A.signalpack",
        "/run/media/AudioHaven/Signal/Libraries/Drum Kits/Stylus RMX/Packs/Stylus RMX/Core Library/Default/library.signalpack",
    ];
    for p in &paths {
        let path = Path::new(p);
        if !path.exists() {
            eprintln!("skip {}", path.display());
            continue;
        }
        let h = read_pack_header(path).expect("read header");
        eprintln!("{}", path.file_name().unwrap().to_string_lossy());
        eprintln!(
            "  mics ({}): {:?}",
            h.spec.mics.len(),
            h.spec.mics.iter().map(|m| &m.id).collect::<Vec<_>>()
        );
        let mut mic_count: HashMap<String, usize> = HashMap::new();
        for z in &h.spec.zones {
            *mic_count.entry(z.mic.clone()).or_insert(0) += 1;
        }
        eprintln!("  zone-mic distribution: {:?}", mic_count);
    }
}
