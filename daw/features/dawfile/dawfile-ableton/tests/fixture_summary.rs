//! Walk every `.als` fixture and print a rich human-readable summary.
//! Run with `cargo test -p dawfile-ableton --test fixture_summary -- --nocapture`.

use dawfile_ableton::read_live_set;
use std::path::PathBuf;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixtures() -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(FIXTURE_DIR)
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()?.to_str()? == "als").then_some(p)
        })
        .collect();
    v.sort();
    v
}

#[test]
fn dump_all_als_fixtures() {
    let files = fixtures();
    assert!(!files.is_empty(), "no .als fixtures found");

    eprintln!("\n=== {} Ableton fixtures ===", files.len());
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        let set = read_live_set(path).unwrap_or_else(|e| panic!("parse {name}: {e}"));

        eprintln!(
            "\n── {name}\n   version: {} (major={} minor={} patch={} beta={})\n   \
             tempo={:.2} time_sig={}/{} key={:?}\n   \
             audio_tracks={} midi_tracks={} return_tracks={} group_tracks={} master={}\n   \
             locators={} scenes={} tempo_auto={} groove_pool={} furthest_bar={:.2}",
            set.version.creator,
            set.version.major,
            set.version.minor,
            set.version.patch,
            set.version.beta,
            set.tempo,
            set.time_signature.numerator,
            set.time_signature.denominator,
            set.key_signature.as_ref().map(|k| k.scale.as_str()),
            set.audio_tracks.len(),
            set.midi_tracks.len(),
            set.return_tracks.len(),
            set.group_tracks.len(),
            set.master_track.is_some(),
            set.locators.len(),
            set.scenes.len(),
            set.tempo_automation.len(),
            set.groove_pool.len(),
            set.furthest_bar,
        );

        for (i, t) in set.audio_tracks.iter().enumerate().take(5) {
            eprintln!("   audio[{i}]: name={:?}", t.common.effective_name);
        }
        for (i, t) in set.midi_tracks.iter().enumerate().take(5) {
            eprintln!("   midi[{i}]:  name={:?}", t.common.effective_name);
        }
    }
}
