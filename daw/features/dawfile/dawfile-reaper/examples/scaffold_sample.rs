//! Emit a sample keyflow-scaffold .rpp for REAPER round-trip testing.
//! Usage: cargo run -p dawfile-reaper --example scaffold_sample -- <out.rpp>
use dawfile_reaper::scaffold::*;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "scaffold-sample.rpp".into());
    let spec = ScaffoldSpec {
        timestamp: 1_700_000_000,
        bpm: 120.0,
        time_sig: (4, 4),
        tracks: vec![
            TrackSpec {
                name: "Keyflow".into(),
                color: None,
                folder: FolderRole::Start,
            },
            TrackSpec {
                name: "KEY".into(),
                color: None,
                folder: FolderRole::Child,
            },
            TrackSpec {
                name: "CHORD".into(),
                color: None,
                folder: FolderRole::Child,
            },
            TrackSpec {
                name: "MELODY".into(),
                color: None,
                folder: FolderRole::Child,
            },
            TrackSpec {
                name: "SCALE".into(),
                color: None,
                folder: FolderRole::End,
            },
        ],
        sections: vec![
            SectionSpec {
                name: "IN".into(),
                start_seconds: 0.0,
                end_seconds: 8.0,
            },
            SectionSpec {
                name: "VS".into(),
                start_seconds: 8.0,
                end_seconds: 24.0,
            },
            SectionSpec {
                name: "CH".into(),
                start_seconds: 24.0,
                end_seconds: 40.0,
            },
        ],
        key_sigs: vec![
            KeySigSpec {
                measure: 0,
                root: 0,
                accidental: 1,
                scale_mask: SCALE_MASK_MAJOR,
            },
            KeySigSpec {
                measure: 8,
                root: 7,
                accidental: 1,
                scale_mask: SCALE_MASK_MAJOR,
            },
        ],
    };
    std::fs::write(&out, build_scaffold_rpp(&spec)).unwrap();
    println!("wrote {out}");
}
