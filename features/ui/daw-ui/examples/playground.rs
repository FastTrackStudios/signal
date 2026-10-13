//! DAW-UI playground — the native main window rendered in the browser
//! with fake sample data (no DAW backend).
//!
//! Run:
//! ```sh
//! dx serve --example playground --platform web --features playground
//! ```
//!
//! It mounts [`MainWindowPreview`] — the traced vector TCP, arrangement
//! and mixer (the components the REAPER theme's art is exported from) —
//! fed hand-built `daw_proto` tracks and items. The WALTER panels this
//! example used to demo were deleted 2026-08-19; see
//! `daw_ui::panels`' tombstone.

use std::collections::HashMap;

use daw_proto::{Item, Track};
use daw_ui::components::arrangement_view::ItemPreview;
use daw_ui::components::main_window::MainWindowPreview;
use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Style { {daw_ui::TAILWIND_CSS} }
        div {
            style: "width: 100vw; height: 100vh; overflow: hidden; background: #1b1b1b;",
            MainWindowPreview {
                width: 1280.0,
                height: 800.0,
                tracks: tracks(),
                items: items(),
                previews: previews(),
            }
        }
    }
}

fn tracks() -> Vec<Track> {
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

fn items() -> Vec<Item> {
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
        item("i-snare", "snare", 2.0, 14.0, "Snare"),
        item("i-bass", "bass", 0.0, 16.0, "Bass"),
        item("i-keys", "keys", 4.0, 12.0, "Keys"),
        item("i-vox", "vox", 2.0, 13.0, "Lead Vox"),
    ]
}

fn previews() -> HashMap<String, ItemPreview> {
    let wave = |amp: f32, seed: f32| {
        ItemPreview::Waveform(
            (0..160)
                .map(|i| {
                    let t = i as f32 / 160.0;
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
