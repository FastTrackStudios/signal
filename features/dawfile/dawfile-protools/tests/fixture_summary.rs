//! Walk every Pro Tools fixture and print a structured summary.
//! Run with `cargo test -p dawfile-protools --test fixture_summary -- --nocapture`.

use dawfile_protools::read_session;
use std::path::PathBuf;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixtures() -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(FIXTURE_DIR)
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            let ext = p.extension()?.to_str()?;
            matches!(ext, "ptx" | "ptf" | "pts" | "pt5").then_some(p)
        })
        .collect();
    v.sort();
    v
}

#[test]
fn dump_all_protools_fixtures() {
    let files = fixtures();
    assert!(!files.is_empty());

    eprintln!("\n=== {} Pro Tools fixtures ===", files.len());
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        let s = read_session(path, 48000).unwrap_or_else(|e| panic!("parse {name}: {e}"));

        let total_audio_regions_active: usize =
            s.audio_tracks.iter().map(|t| t.regions.len()).sum();
        let total_midi_events: usize = s.midi_regions.iter().map(|r| r.events.len()).sum();

        let alt_playlists = s.total_alternate_playlists();
        let total_active = s.total_active_region_placements();

        eprintln!(
            "── {name}\n   pt_version={} sr={} bpm={:.2} tempo_events={} meter_events={}\n   \
             audio_files={} audio_regions={} (active={}) audio_tracks={}\n   \
             midi_regions={} midi_events={} midi_tracks={} markers={} plugins={} io_channels={}\n   \
             ↳ all_tracks={} active_placements={} alt_playlists={}",
            s.version,
            s.session_sample_rate,
            s.bpm,
            s.tempo_events.len(),
            s.meter_events.len(),
            s.audio_files.len(),
            s.audio_regions.len(),
            total_audio_regions_active,
            s.audio_tracks.len(),
            s.midi_regions.len(),
            total_midi_events,
            s.midi_tracks.len(),
            s.markers.len(),
            s.plugins.len(),
            s.io_channels.len(),
            s.all_tracks().count(),
            total_active,
            alt_playlists,
        );

        // Highlight tracks with alternate playlists (Pro Tools "Playlists" comp feature).
        for t in s
            .all_tracks()
            .filter(|t| t.has_alternate_playlists())
            .take(5)
        {
            eprintln!(
                "   comp: {:?} ({:?}) playlists={} alts=[{}]",
                t.name,
                t.kind,
                t.playlist_count(),
                t.alternate_playlists
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        for (i, f) in s.audio_files.iter().enumerate().take(5) {
            eprintln!("   wav[{i}]: {:?} len={}", f.filename, f.length);
        }
    }
}
