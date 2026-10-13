//! Preview — the native main window, as a fixed reference render.
//!
//! This used to execute the edited theme's WALTER program live
//! (`DawWorkspace` over the deleted `daw_ui::panels` family — see that
//! module's tombstone). The native components read `daw-theme`'s
//! defaults and their own traced geometry, **not** the ini palette
//! being edited here, so for now the preview shows the product's real
//! UI at its stock look while the editor's colour panels stay the
//! authoritative view of the palette edit. Wiring the ini palette
//! through `daw_theme_art::dress` so colour edits re-skin this render
//! is the follow-up; faking it by tinting a screenshot is not.
//!
//! Element artwork was already vector-fallback in the old preview (the
//! PNG atlases need a filesystem) — push to REAPER to judge artwork,
//! same as before.

use std::collections::HashMap;

use daw_proto::{Item, Track};
use daw_ui::components::arrangement_view::ItemPreview;
use daw_ui::components::main_window::MainWindowPreview;
use dioxus::prelude::*;

#[component]
pub fn Preview() -> Element {
    rsx! {
        div { class: "preview-frame",
            MainWindowPreview {
                width: 900.0,
                height: 620.0,
                tracks: sample_tracks(),
                items: sample_items(),
                previews: sample_previews(),
            }
        }
        p { class: "preview-note",
            "The native UI at its stock look — colour edits apply in REAPER, "
            "not here, until the palette is wired through daw-theme-art. "
            "Push to REAPER to judge the exported theme."
        }
    }
}

/// A small, representative arrangement: a drum folder with children and
/// a couple of coloured tracks with clips.
fn sample_tracks() -> Vec<Track> {
    let t = |guid: &str, name: &str, colour: u32, index: u32, depth: i32| Track {
        guid: guid.into(),
        name: name.into(),
        color: Some(colour),
        index,
        folder_depth: depth,
        volume: 0.8,
        ..Default::default()
    };
    let mut drums = t("drums", "DRUMS", 0xe0567a, 0, 0);
    drums.is_folder = true;
    vec![
        drums,
        t("kick", "Kick", 0xe0567a, 1, 1),
        t("snare", "Snare", 0xe0567a, 2, 1),
        t("bass", "Bass", 0x5b8def, 3, 0),
        t("keys", "Keys", 0xf2b134, 4, 0),
        t("vox", "Lead Vox", 0x3ddc97, 5, 0),
    ]
}

fn sample_items() -> Vec<Item> {
    let item = |guid: &str, track: &str, from: f64, secs: f64, label: &str| Item {
        guid: guid.into(),
        track_guid: track.into(),
        position: daw_proto::primitives::PositionInSeconds::from_seconds(from),
        length: daw_proto::primitives::Duration::from_seconds(secs),
        label: Some(label.to_string()),
        ..Default::default()
    };
    vec![
        item("i-kick", "kick", 0.0, 16.0, "Kick"),
        item("i-snare", "snare", 0.0, 16.0, "Snare"),
        item("i-bass", "bass", 0.0, 16.0, "Bass"),
        item("i-keys", "keys", 4.0, 12.0, "Keys"),
        item("i-vox", "vox", 2.0, 13.0, "Lead Vox"),
    ]
}

fn sample_previews() -> HashMap<String, ItemPreview> {
    let wave = |amp: f32, seed: f32| {
        ItemPreview::Waveform(
            (0..120)
                .map(|i| {
                    let t = i as f32 / 120.0;
                    let env = (1.0 - t).powf(0.35) * amp;
                    let s =
                        ((t * 37.0 + seed).sin() * 0.6 + (t * 91.0 + seed * 2.0).sin() * 0.4) * env;
                    s.abs().min(1.0)
                })
                .collect(),
        )
    };
    HashMap::from([
        ("i-kick".to_string(), wave(0.9, 0.0)),
        ("i-snare".to_string(), wave(0.7, 1.3)),
        ("i-bass".to_string(), wave(0.8, 2.1)),
        ("i-keys".to_string(), wave(0.5, 3.7)),
        ("i-vox".to_string(), wave(0.85, 4.2)),
    ])
}
