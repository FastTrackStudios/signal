//! The drum workspace load, headless.
//!
//! `--drums` opens a project's kit folder as one workspace: a track per
//! mic, each analysed as percussion, folded into role lanes. This is the
//! loading half of `r[drums.open.runner]` — no window, same reason as
//! `tests/load.rs`.

use std::path::{Path, PathBuf};

use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
use dawfile_reaper::types::item::SourceType;
use expression_editor_core::kit::LaneRole;
use expression_editor_core::{Mode, Viewport};
use expression_editor_standalone::{Args, LoadError, Runner, Source, Target};

fn viewport() -> Viewport {
    Viewport::new(1100.0, 500.0)
}

/// A mono 16-bit PCM second of silence with a few sharp decaying clicks
/// in it — the least audio the envelope gate will call hits.
fn write_click_wav(path: &Path, rate: u32) {
    let frames = rate; // one second
    let mut samples = vec![0.0f64; frames as usize];
    for &at_secs in &[0.1, 0.4, 0.7] {
        let start = (at_secs * rate as f64) as usize;
        for i in 0..((rate / 100) as usize) {
            // Sharp attack, ~10 ms decay: struck, not swelled, which is
            // exactly what the gate's crest condition tests for.
            let t = i as f64 / rate as f64;
            if let Some(s) = samples.get_mut(start + i) {
                *s = 0.9 * (-t / 0.003).exp();
            }
        }
    }

    let data_len = frames * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &v in &samples {
        out.extend_from_slice(&((v * i16::MAX as f64) as i16).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

/// Three mics under a `Drums` folder: `Kick In` inside a `Kick`
/// sub-folder, `Snare Top` inside `Snare`, and `OH` directly under the
/// kit — the smallest project that exercises all three ways a track gets
/// its role (folder, folder, own name).
fn fixture(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let wavs: Vec<PathBuf> = ["kick.wav", "snare.wav", "oh.wav"]
        .iter()
        .map(|n| {
            let p = dir.join(n);
            write_click_wav(&p, 44_100);
            p
        })
        .collect();

    let rpp = ReaperProjectBuilder::new()
        .tempo(120.0)
        .track("Drums", |t| t.folder_start())
        .track("Kick", |t| t.folder_start())
        .track("Kick In", |t| {
            t.item(0.0, 1.0, |i| {
                i.take(wavs[0].to_string_lossy().into_owned(), SourceType::Wave)
            })
            .folder_end(1)
        })
        .track("Snare", |t| t.folder_start())
        .track("Snare Top", |t| {
            t.item(0.0, 1.0, |i| {
                i.take(wavs[1].to_string_lossy().into_owned(), SourceType::Wave)
            })
            .folder_end(1)
        })
        .track("OH", |t| {
            t.item(0.0, 1.0, |i| {
                i.take(wavs[2].to_string_lossy().into_owned(), SourceType::Wave)
            })
            .folder_end(1)
        })
        .build()
        .to_rpp_string();

    let path = dir.join("kit.rpp");
    std::fs::write(&path, rpp).unwrap();
    path
}

// r[verify drums.open.runner]
#[test]
fn a_kit_folder_opens_as_a_drum_workspace() {
    let dir = std::env::temp_dir().join("fts-ee-standalone-drums");
    let path = fixture(&dir);

    let runner = Runner::open(
        &Source::Rpp(path),
        &Target {
            drums: Some(None),
            ..Target::default()
        },
        viewport(),
        None,
    )
    .expect("the kit opens");

    assert_eq!(runner.loaded.kind(), "drums");
    assert!(runner.daw.is_some(), "the backend outlives the load");
    assert!(
        runner.label.contains("drums: Drums (3 mics)"),
        "got {:?}",
        runner.label
    );

    let editor = runner.loaded.editor();
    assert!(editor.stacked, "a drum workspace opens on the stack");
    assert_eq!(editor.tracks.len(), 3, "one track per mic, no folders");
    assert_eq!(editor.mode, Mode::UnpitchedAudio);

    // Role lanes, top to bottom Other / Snare / Kick — kick at the
    // bottom, and no Toms lane because the kit has no toms.
    let roles: Vec<Option<LaneRole>> = editor
        .tracks
        .layout()
        .lanes()
        .iter()
        .map(|l| l.role)
        .collect();
    assert_eq!(
        roles,
        vec![
            Some(LaneRole::Other),
            Some(LaneRole::Snare),
            Some(LaneRole::Kick)
        ]
    );

    // Every mic analysed: unpitched, hits found, waveform behind them.
    for (i, track) in editor.tracks.tracks().iter().enumerate() {
        assert_eq!(track.mode, Mode::UnpitchedAudio, "{}", track.name);
        let doc = if i == editor.tracks.active() {
            &editor.doc
        } else {
            editor.tracks.doc_of(i).expect("parked doc")
        };
        assert!(!doc.notes.is_empty(), "no hits detected on {}", track.name);
        assert!(!doc.peaks.is_empty(), "no peaks on {}", track.name);
    }
}

// r[verify drums.open.runner]
#[test]
fn a_named_kit_folder_is_found_case_insensitively() {
    let dir = std::env::temp_dir().join("fts-ee-standalone-drums-named");
    let path = fixture(&dir);
    let runner = Runner::open(
        &Source::Rpp(path),
        &Target {
            drums: Some(Some("drums".into())),
            ..Target::default()
        },
        viewport(),
        None,
    )
    .expect("the named folder opens");
    assert_eq!(runner.loaded.editor().tracks.len(), 3);
}

// r[verify drums.open.runner]
#[test]
fn a_project_with_no_kit_folder_says_so() {
    let rpp = ReaperProjectBuilder::new()
        .track("Vox", |t| {
            t.item(0.0, 1.0, |i| i.take("vox", SourceType::Midi))
        })
        .build()
        .to_rpp_string();
    let dir = std::env::temp_dir().join("fts-ee-standalone-drums-none");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("no-kit.rpp");
    std::fs::write(&path, rpp).unwrap();

    let err = Runner::open(
        &Source::Rpp(path),
        &Target {
            drums: Some(None),
            ..Target::default()
        },
        viewport(),
        None,
    )
    .expect_err("nothing to open as a kit");
    assert!(matches!(err, LoadError::NoKitFolder { .. }), "got {err:?}");
}

// r[verify drums.open.runner]
#[test]
fn the_drums_flag_parses_with_and_without_a_folder() {
    let bare = Args::parse(["song.rpp", "--drums"].map(String::from)).unwrap();
    assert_eq!(bare.target.drums, Some(None));

    let named = Args::parse(["song.rpp", "--drums", "The Kit"].map(String::from)).unwrap();
    assert_eq!(named.target.drums, Some(Some("The Kit".into())));

    // Before the source, a following token is NOT eaten as the folder —
    // it is the source.
    let ordered = Args::parse(["--drums", "song.rpp"].map(String::from)).unwrap();
    assert_eq!(ordered.target.drums, Some(None));
    assert!(matches!(ordered.source, Source::Rpp(_)));
}

/// Every item on the track holding `item`, as (position, length,
/// offset) — the same fingerprint the slip tests use.
fn pieces_on(
    daw: &daw::standalone::Standalone,
    ctx: &daw::service::ProjectContext,
    item: &daw::service::ItemRef,
) -> Vec<(f64, f64, f64)> {
    use daw::service::{Items, TakeRef, Takes};
    let info = daw.get_item(ctx.clone(), item.clone()).expect("item");
    let mut out: Vec<(f64, f64, f64)> = daw
        .get_items(
            ctx.clone(),
            daw::service::TrackRef::Guid(info.track_guid.clone()),
        )
        .into_iter()
        .map(|i| {
            let offset = daw
                .get_take(
                    ctx.clone(),
                    daw::service::ItemRef::Guid(i.guid.clone()),
                    TakeRef::Active,
                )
                .map(|t| t.start_offset.as_seconds())
                .unwrap_or(0.0);
            (i.position.as_seconds(), i.length.as_seconds(), offset)
        })
        .collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    out
}

fn open_kit(tag: &str) -> Runner {
    let dir = std::env::temp_dir().join(format!("fts-ee-standalone-drums-{tag}"));
    let path = fixture(&dir);
    Runner::open(
        &Source::Rpp(path),
        &Target {
            drums: Some(None),
            ..Target::default()
        },
        viewport(),
        None,
    )
    .expect("the kit opens")
}

// r[verify drums.quantize.apply]
// r[verify drums.quantize.undo]
#[test]
fn the_hosts_apply_cuts_every_mic_the_same_and_one_undo_restores() {
    use daw::service::ProjectContext;
    let runner = open_kit("apply");
    let host = runner.host.as_ref().expect("a drum workspace has a host");
    let daw = runner.daw.as_ref().expect("backend");
    let ctx = ProjectContext::Current;

    let items = host.group();
    assert_eq!(items.len(), 3, "the whole kit is the group");
    let before: Vec<_> = items.iter().map(|i| pieces_on(daw, &ctx, i)).collect();

    // The panel at its defaults — at 120 bpm the fixture's clicks at
    // 0.1/0.4/0.7 s are all off the default grid.
    let panel = expression_editor_ui::QuantizePanel::default();
    let (bins, previews) = host.preview(&panel);
    assert!(!bins.is_empty(), "the histogram has content");
    assert!(previews.iter().any(|p| p.moved), "the plan moves hits");

    let done = host.apply(&panel).expect("applied");
    assert_eq!(done.items, 3, "every mic was edited");
    assert!(done.pieces > 3, "each mic was cut into pieces");

    let first = pieces_on(daw, &ctx, &items[0]);
    assert!(first.len() > 1);
    for item in &items[1..] {
        assert_eq!(pieces_on(daw, &ctx, item), first, "mics cut identically");
    }

    assert!(host.undo(), "one undo step");
    for (i, item) in items.iter().enumerate() {
        assert_eq!(pieces_on(daw, &ctx, item), before[i], "mic {i} restored");
    }
}

// r[verify drums.manual.slip]
#[test]
fn the_hosts_slip_slides_every_mic_and_one_undo_restores() {
    use daw::service::ProjectContext;
    use expression_editor_audio::quantize::SplitConfig;
    let runner = open_kit("slip");
    let host = runner.host.as_ref().expect("host");
    let daw = runner.daw.as_ref().expect("backend");
    let ctx = ProjectContext::Current;
    let items = host.group();
    let before: Vec<_> = items.iter().map(|i| pieces_on(daw, &ctx, i)).collect();

    let cfg = SplitConfig {
        leading_pad_secs: 0.005,
        crossfade_secs: 0.005,
    };
    // Drag the middle click (0.4 s) 30 ms later; the next is at 0.7 s.
    let done = host.slip(0.4, 0.7, 0.030, cfg).expect("slipped");
    assert_eq!(done.items, 3);
    assert_eq!(done.pieces, 9, "three pieces per mic");

    let first = pieces_on(daw, &ctx, &items[0]);
    for item in &items[1..] {
        assert_eq!(pieces_on(daw, &ctx, item), first, "mics slid identically");
    }
    let moved = first
        .iter()
        .find(|(pos, ..)| (pos - (0.395 + 0.030)).abs() < 1e-9);
    assert!(moved.is_some(), "the dragged span landed 30 ms later");

    assert!(host.undo(), "one undo step");
    for (i, item) in items.iter().enumerate() {
        assert_eq!(pieces_on(daw, &ctx, item), before[i], "mic {i} restored");
    }
}

// r[verify drums.manual.stretch]
#[test]
fn the_hosts_stretch_writes_one_marker_map_to_every_mic_and_one_undo_restores() {
    use daw::service::{ProjectContext, StretchMarkers, TakeRef};
    let runner = open_kit("stretch");
    let host = runner.host.as_ref().expect("host");
    let daw = runner.daw.as_ref().expect("backend");
    let ctx = ProjectContext::Current;
    let items = host.group();

    // Drag the middle click (0.4 s) 30 ms later between its neighbours.
    let done = host
        .stretch(0.4, 0.1, 0.7, 0.030, false)
        .expect("stretched");
    assert_eq!(done.items, 3, "every mic warped");
    assert!(done.pieces > 0, "markers were written");

    let markers_of = |item: &daw::service::ItemRef| {
        daw.get_stretch_markers(ctx.clone(), item.clone(), TakeRef::Active)
    };
    let first = markers_of(&items[0]);
    assert!(!first.is_empty(), "the map exists");
    for item in &items[1..] {
        assert_eq!(markers_of(item), first, "identical maps on every mic");
    }
    // The dragged hit is heard 30 ms later: some marker plays 0.4 s of
    // source at 0.43 s of take time.
    assert!(
        first
            .iter()
            .any(|m| (m.source_position - 0.4).abs() < 0.01 && (m.position - 0.43).abs() < 0.01),
        "the hit's marker moved: {first:?}"
    );

    assert!(host.undo(), "one undo step");
    for item in &items {
        assert!(markers_of(item).is_empty(), "markers gone after undo");
    }
}

// r[verify drums.manual.add-remove]
#[test]
fn hand_added_and_removed_hits_shape_the_list_but_not_the_daw() {
    use daw::service::ProjectContext;
    let runner = open_kit("manual");
    let host = runner.host.as_ref().expect("host");
    let daw = runner.daw.as_ref().expect("backend");
    let ctx = ProjectContext::Current;
    let items = host.group();
    let before: Vec<_> = items.iter().map(|i| pieces_on(daw, &ctx, i)).collect();

    let panel = expression_editor_ui::QuantizePanel::default();
    let detected = host.hits(&panel).len();
    assert!(detected >= 3, "the fixture's clicks were found");

    // A hand-placed hit near silence at 0.55 s refines to wherever the
    // sum rises most — the fixture is silent there, so it stays put.
    let landed = host.add_hit(0.55, 0.02);
    assert!(
        (landed - 0.55).abs() <= 0.02,
        "refined nearby, got {landed}"
    );
    assert_eq!(host.hits(&panel).len(), detected + 1, "the list grew");

    // Throwing out a detected hit suppresses it across re-detection.
    host.remove_hit(0.4);
    let now = host.hits(&panel);
    assert_eq!(now.len(), detected, "one added, one removed");
    assert!(
        !now.iter().any(|t| (t.at - 0.4).abs() < 0.015),
        "the removed hit stays gone"
    );
    // Un-adding the hand hit brings the count back down.
    host.remove_hit(landed);
    assert_eq!(host.hits(&panel).len(), detected - 1);

    // And none of it touched the daw.
    for (i, item) in items.iter().enumerate() {
        assert_eq!(pieces_on(daw, &ctx, item), before[i], "mic {i} untouched");
    }
}
