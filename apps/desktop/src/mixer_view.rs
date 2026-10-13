//! Mixer workspace — daw-ui's **native** mixer (`daw_ui::MixerPanel`,
//! the vector components the REAPER theme's art is exported from) over
//! the in-process daw facade. Plus an "Open in REAPER" button — the
//! project round-trips through `dawfile`.
//!
//! This used to build `TrackView`s for the WALTER-themed
//! `MixerControlPanel`; that family was deleted 2026-08-19 (see
//! `daw_ui::panels`' tombstone). The native panel needs none of that
//! plumbing: it self-connects to the installed daw singleton, follows
//! the current project on its own poll + track-event subscription (so
//! song switches land within its refresh), and writes fader/pan/mute/
//! solo back itself through `ControlSync`.

use dioxus::prelude::*;

/// Fetch the current project's tracks through the daw facade, export
/// them to a `.rpp` via dawfile, and open REAPER on it. Native-only.
/// Exports track STRUCTURE (names/colours/folders/vol/pan/mute); audio
/// items/takes are not yet exported.
#[cfg(not(target_arch = "wasm32"))]
fn open_in_reaper() {
    spawn(async move {
        let Some(daw) = daw::get() else { return };
        let Ok(project) = daw.current_project().await else {
            return;
        };
        let Ok(tracks) = project.tracks().all().await else {
            return;
        };
        let rpp = dawfile_reaper::daw_tracks_to_rpp_project_text(&tracks);

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::PathBuf::from(home).join("Downloads/fts-setlist.rpp");
        if let Err(e) = std::fs::write(&path, rpp) {
            tracing::warn!("open-in-reaper: write {path:?}: {e}");
            return;
        }
        tracing::info!("open-in-reaper: wrote {path:?}");

        let candidates = [
            "reaper".to_string(),
            format!(
                "{}/Downloads/reaper766_linux_x86_64/REAPER/reaper",
                std::env::var("HOME").unwrap_or_default()
            ),
        ];
        for bin in candidates {
            if std::process::Command::new(&bin).arg(&path).spawn().is_ok() {
                tracing::info!("open-in-reaper: launched {bin}");
                return;
            }
        }
        tracing::warn!("open-in-reaper: no reaper binary found; .rpp is at {path:?}");
    });
}

#[cfg(target_arch = "wasm32")]
fn open_in_reaper() {}

#[component]
pub fn MixerWorkspace() -> Element {
    rsx! {
        div {
            style: "flex:1; min-height:0; display:flex; flex-direction:column;",
            // The REAPER hand-off. The workspace is named by the app
            // bar's crumb, so this is an action row, not a title bar.
            div {
                style: "flex:0 0 auto; display:flex; align-items:center; gap:8px; \
                        padding:10px 14px 4px;",
                div { style: "flex:1;" }
                button {
                    onclick: move |_| open_in_reaper(),
                    style: "padding:5px 12px; border-radius:6px; border:1px solid #3f3f46; \
                            background:#18181b; color:#e4e4e7; font-size:12px; cursor:pointer;",
                    "Open setlist in REAPER"
                }
            }
            // The native mixer. Self-connecting: it waits for the daw
            // singleton, polls the current project, and subscribes to
            // track events — no view-model to build here.
            div {
                style: "flex:1; min-height:0;",
                daw_ui::MixerPanel {}
            }
        }
    }
}
