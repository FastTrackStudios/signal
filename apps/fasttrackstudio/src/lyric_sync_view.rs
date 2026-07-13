//! Lyric Sync workspace — loads a keyflow-sync `TimingMap` sidecar (per-word
//! forced-alignment timings + confidence) and shows it in session-ui's
//! editable `LyricSyncView`. Nudges shift a word's start/end and write the
//! sidecar back to disk, so a manual pass over the aligner's output persists.
//!
//! Sidecar path: `$FTS_LYRIC_SYNC`, else the bundled Praise demo sidecar under
//! the Worship-MultiTracks folder. Missing file ⇒ a friendly empty state.

use std::path::PathBuf;

use dioxus::prelude::*;

use keyflow_sync::timing::{Rating, TimingMap};
use session_ui::components::{LyricSyncView, SyncRating, SyncWord};

fn sidecar_path() -> PathBuf {
    if let Ok(p) = std::env::var("FTS_LYRIC_SYNC") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Downloads/Worship MultiTracks/sync/praise.kf.json")
}

fn to_sync_word(w: &keyflow_sync::timing::WordEntry) -> SyncWord {
    SyncWord {
        text: w.text.clone(),
        start: w.start as f64,
        end: w.end as f64,
        confidence: w.confidence,
        rating: match w.rating {
            Rating::Verified => SyncRating::Verified,
            Rating::Review => SyncRating::Review,
            Rating::Failed => SyncRating::Failed,
        },
    }
}

#[component]
pub fn LyricSyncWorkspace() -> Element {
    // The loaded sidecar (None until the effect runs / if the file is absent).
    let mut map = use_signal(|| None::<TimingMap>);
    let path = use_signal(sidecar_path);

    use_effect(move || {
        let p = path.read().clone();
        map.set(TimingMap::read_file(&p).ok());
    });

    // Nudge: shift word `i`'s start (and end, to preserve duration) by `delta`,
    // clamp at 0, persist the sidecar.
    let on_nudge = Callback::new(move |(i, delta): (usize, f64)| {
        let mut guard = map.write();
        if let Some(tm) = guard.as_mut() {
            if let Some(w) = tm.words.get_mut(i) {
                let dur = (w.end - w.start).max(0.0);
                w.start = (w.start + delta as f32).max(0.0);
                w.end = w.start + dur;
            }
            let _ = tm.write_file(&*path.read());
        }
    });

    let snapshot = map.read().clone();
    rsx! {
        div {
            style: "flex:1; min-height:0; display:flex; flex-direction:column;",
            match snapshot {
                Some(tm) if !tm.words.is_empty() => {
                    let words: Vec<SyncWord> = tm.words.iter().map(to_sync_word).collect();
                    rsx! { LyricSyncView { words, on_nudge } }
                }
                _ => rsx! {
                    div {
                        style: "padding:24px; color:#a1a1aa; font-size:13px; line-height:1.6;",
                        "No lyric-sync sidecar found at "
                        code { style: "color:#e4e4e7;", "{path.read().display()}" }
                        "."
                        br {}
                        "Generate one with "
                        code { style: "color:#e4e4e7;",
                            "kf sync <song.wav> <chart.kf> --separate -o <sidecar>.kf.json"
                        }
                        " (or set FTS_LYRIC_SYNC)."
                    }
                }
            }
        }
    }
}
