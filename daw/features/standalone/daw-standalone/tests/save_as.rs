//! Saving as a new `.rpp` — the write-back experiment.
//!
//! Three claims, in rising order of ambition (`drums.save.*`):
//! a save never touches the original and lands beside it; an unedited
//! save is byte-identical to the original; an edited save reopens with
//! the edit in it.

#![cfg(feature = "rpp-save")]

use daw_proto::{
    Duration, ItemRef, Items, PositionInSeconds, ProjectContext, Projects, StretchMarker,
    StretchMarkers, StretchTakeRef, TakeRef, Takes, Tracks,
};
use daw_standalone::save::save_project_as;
use daw_standalone::sync::Standalone;

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "daw-save-as-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A small but real project: two mic tracks, one item each, audio on
/// disk (relative path), GUIDs on every entity — the shape REAPER
/// itself writes, so id-matched patching is exercised.
fn fixture(dir: &std::path::Path) -> std::path::PathBuf {
    // A short mono wav with two clicks.
    let sr = 48_000u32;
    let n = sr as usize; // 1 s
    let mut wav = Vec::with_capacity(44 + n * 2);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + n as u32 * 2).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sr.to_le_bytes());
    wav.extend_from_slice(&(sr * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(n as u32 * 2).to_le_bytes());
    for i in 0..n {
        let click = matches!(i / 100, 120 | 240);
        let v = if click { 0.8f64 } else { 0.0 };
        wav.extend_from_slice(&((v * i16::MAX as f64) as i16).to_le_bytes());
    }
    std::fs::write(dir.join("mic.wav"), &wav).expect("wav");

    let mut rpp =
        String::from("<REAPER_PROJECT 0.1 \"7.0/linux-x86_64\" 1700000000\n  TEMPO 120 4 4 0\n");
    for (i, name) in ["Kick In", "Kick Out"].iter().enumerate() {
        rpp.push_str(&format!(
            concat!(
                "  <TRACK {{AAAAAAAA-0000-0000-0000-00000000000{i}}}\n",
                "    NAME \"{name}\"\n",
                "    TRACKID {{AAAAAAAA-0000-0000-0000-00000000000{i}}}\n",
                "    <ITEM\n",
                "      POSITION 0\n",
                "      LENGTH 1\n",
                "      IGUID {{BBBBBBBB-0000-0000-0000-00000000000{i}}}\n",
                "      NAME mic.wav\n",
                "      SOFFS 0\n",
                "      PLAYRATE 1 1 0 -1 0 0.0025\n",
                "      GUID {{CCCCCCCC-0000-0000-0000-00000000000{i}}}\n",
                "      <SOURCE WAVE\n",
                "        FILE \"mic.wav\"\n",
                "      >\n",
                "    >\n",
                "  >\n",
            ),
            i = i,
            name = name,
        ));
    }
    rpp.push_str(">\n");
    let path = dir.join("session.rpp");
    std::fs::write(&path, rpp).expect("rpp");
    path
}

fn open(path: &std::path::Path) -> (Standalone, String) {
    let daw = Standalone::new();
    let info = Projects::open(&daw, &path.to_string_lossy()).expect("open");
    (daw, info.guid)
}

// r[verify drums.save.new-file]
// r[verify drums.save.patched]
#[test]
fn an_unedited_save_is_a_byte_identical_sibling() {
    let dir = TempDir::new();
    let original = fixture(&dir.0);
    let before = std::fs::read(&original).unwrap();

    let (daw, guid) = open(&original);
    let written = save_project_as(&daw, &guid).expect("saved");

    assert_ne!(written, original, "never the original path");
    assert_eq!(
        written.file_name().unwrap().to_string_lossy(),
        "session.fts-edit.rpp"
    );
    assert_eq!(
        std::fs::read(&original).unwrap(),
        before,
        "original untouched"
    );
    assert_eq!(
        std::fs::read(&written).unwrap(),
        before,
        "an unedited save is byte-identical"
    );

    // A second save numbers up rather than clobbering the first.
    let second = save_project_as(&daw, &guid).expect("saved again");
    assert_eq!(
        second.file_name().unwrap().to_string_lossy(),
        "session.fts-edit-2.rpp"
    );
}

// r[verify drums.save.reopens]
#[test]
fn an_edited_group_survives_save_and_reopen() {
    let dir = TempDir::new();
    let original = fixture(&dir.0);
    let (daw, guid) = open(&original);
    let ctx = ProjectContext::Project(guid.clone());

    // The kind of edit drum editing makes: split both mics at 0.5 s,
    // slide the right piece 30 ms later, and warp-mark one take.
    let all: Vec<ItemRef> = Tracks::all(&daw, ctx.clone())
        .iter()
        .flat_map(|t| {
            Items::get_items(&daw, ctx.clone(), daw_proto::TrackRef::Guid(t.guid.clone()))
        })
        .map(|i| ItemRef::Guid(i.guid))
        .collect();
    assert_eq!(all.len(), 2);
    for item in &all {
        let piece = Items::duplicate_item(&daw, ctx.clone(), item.clone()).expect("dup");
        let piece = ItemRef::Guid(piece);
        Items::set_length(&daw, ctx.clone(), item.clone(), Duration::from_seconds(0.5)).unwrap();
        Items::set_position(
            &daw,
            ctx.clone(),
            piece.clone(),
            PositionInSeconds::from_seconds(0.53),
        )
        .unwrap();
        Items::set_length(
            &daw,
            ctx.clone(),
            piece.clone(),
            Duration::from_seconds(0.47),
        )
        .unwrap();
        Takes::set_start_offset(
            &daw,
            ctx.clone(),
            piece.clone(),
            TakeRef::Active,
            Duration::from_seconds(0.5),
        )
        .unwrap();
    }
    StretchMarkers::set_stretch_markers(
        &daw,
        StretchTakeRef {
            project: ctx.clone(),
            item: all[0].clone(),
            take: TakeRef::Active,
        },
        vec![StretchMarker::new(0.0, 0.0), StretchMarker::new(0.2, 0.25)],
    )
    .unwrap();

    let written = save_project_as(&daw, &guid).expect("saved");

    // Reopen the written file in a fresh backend.
    let (daw2, guid2) = open(&written);
    let ctx2 = ProjectContext::Project(guid2);
    let mut layouts: Vec<Vec<(f64, f64, f64)>> = Vec::new();
    for t in Tracks::all(&daw2, ctx2.clone()) {
        if t.is_folder {
            continue;
        }
        let mut items: Vec<(f64, f64, f64)> = Items::get_items(
            &daw2,
            ctx2.clone(),
            daw_proto::TrackRef::Guid(t.guid.clone()),
        )
        .iter()
        .map(|i| {
            let off = Takes::get_take(
                &daw2,
                ctx2.clone(),
                ItemRef::Guid(i.guid.clone()),
                TakeRef::Active,
            )
            .map(|tk| tk.start_offset.as_seconds())
            .unwrap_or(0.0);
            (i.position.as_seconds(), i.length.as_seconds(), off)
        })
        .collect();
        items.sort_by(|a, b| a.0.total_cmp(&b.0));
        layouts.push(items);
    }
    assert_eq!(layouts.len(), 2, "both mics reopened");
    for items in &layouts {
        assert_eq!(items.len(), 2, "the split survived");
        let (left, right) = (items[0], items[1]);
        assert!((left.0 - 0.0).abs() < 1e-9 && (left.1 - 0.5).abs() < 1e-9);
        assert!(
            (right.0 - 0.53).abs() < 1e-9,
            "slid piece at 0.53, got {}",
            right.0
        );
        assert!((right.2 - 0.5).abs() < 1e-9, "offset survived");
    }
    // The warp markers came back on the first mic's first item.
    let first_track = Tracks::all(&daw2, ctx2.clone())
        .into_iter()
        .find(|t| !t.is_folder)
        .unwrap();
    let first_item = Items::get_items(
        &daw2,
        ctx2.clone(),
        daw_proto::TrackRef::Guid(first_track.guid),
    )
    .into_iter()
    .min_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()))
    .unwrap();
    let markers = StretchMarkers::get_stretch_markers(
        &daw2,
        ctx2,
        ItemRef::Guid(first_item.guid),
        TakeRef::Active,
    );
    assert_eq!(markers.len(), 2, "SM lines round-tripped");
    assert!((markers[1].source_position - 0.25).abs() < 1e-9);
}
