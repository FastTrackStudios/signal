//! End-to-end smoke test for the RPP→PTX writer (item 9 in the
//! PT-converter feature roadmap).
//!
//! Builds a tiny RPP via the `dawfile-reaper` builder, runs it through
//! the converter via `dawfile_protools::write::rpp_to_ptx_via_converter`,
//! and verifies the resulting PTX parses back with the expected
//! single track.
//!
//! Skipped when the converter binary isn't installed locally (CI on
//! Linux without the macOS converter available).

use dawfile_protools::write;
use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;

#[test]
fn rpp_to_ptx_writer_smoke() {
    let Ok(_) = write::find_converter_binary() else {
        eprintln!("skip: PT Reaper Converter not installed");
        return;
    };

    let rpp = ReaperProjectBuilder::new()
        .sample_rate(48000)
        .track("WriterSmoke", |t| t.color(0xd86e41))
        .build()
        .to_rpp_string();

    let tmp = std::env::temp_dir();
    let rpp_path = tmp.join("writer_smoke.rpp");
    let ptx_path = tmp.join("writer_smoke.ptx");
    std::fs::write(&rpp_path, rpp).unwrap();

    write::rpp_to_ptx_via_converter(&rpp_path, &ptx_path).expect("convert failed");

    let session = dawfile_protools::read_session(ptx_path.to_str().unwrap(), 48000).unwrap();
    let total_tracks = session.audio_tracks.len() + session.midi_tracks.len();
    assert!(
        total_tracks >= 1,
        "expected at least one track, got {total_tracks}"
    );

    // The track we built had color 0xd86e41, which the converter writes
    // as palette index 24 (0x18) at 0x200b +106..+107.
    let probe = session
        .audio_tracks
        .iter()
        .chain(session.midi_tracks.iter())
        .find(|t| t.name == "WriterSmoke")
        .expect("WriterSmoke track");
    assert_eq!(
        probe.color_byte, 0x18,
        "expected color_byte 0x18 for 0xd86e41"
    );
}
