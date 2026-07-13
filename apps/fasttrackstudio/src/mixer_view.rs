//! Mixer workspace — the REAL reaper-style mixer: daw-ui's WALTER-themed
//! `MixerControlPanel` (MCP) under a `ThemeProvider`, fed `TrackView`s built
//! from the in-process daw facade (the seeded Praise stems). Plus an "Open in
//! REAPER" button — the project round-trips through `dawfile`.
//!
//! The MCP is theme-driven with INLINE styles (no Tailwind), so it renders
//! correctly in the WebView regardless of the app's Tailwind sheet, and can
//! later be skinned from a real `.ReaperTheme` (theming::reaper_import).

use dioxus::prelude::*;

use daw::service::Track;
use daw_ui::panels::{MixerControlPanel, TrackView};
use daw_ui::theming::{ThemeContext, ThemeProvider};

/// Build a themed `TrackView` (with its live signals) from a daw `Track`.
fn track_view(id: usize, t: &Track) -> TrackView {
    let hex = t.color.map(|c| format!("#{:06x}", c & 0x00FF_FFFF));
    let mut tv = TrackView::new(id, t.name.clone(), hex.as_deref())
        .fader(t.volume as f32)
        .depth(t.folder_depth.max(0) as u32)
        .routing(false, false);
    if t.is_folder {
        tv = tv.folder();
    }
    // Initialize the live signals from the track's current state.
    *tv.mute.write() = t.muted;
    *tv.solo.write() = t.soloed;
    *tv.pan.write() = (0.5 + t.pan as f32 / 2.0).clamp(0.0, 1.0);
    tv
}

/// Fetch the current project's tracks through the daw facade, export them to a
/// `.rpp` via dawfile, and open REAPER on it. Native-only. Exports track
/// STRUCTURE (names/colours/folders/vol/pan/mute); audio items/takes are not
/// yet exported.
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
    // Raw daw tracks (fetched once — the seeded stems are static). The daw
    // facade is initialized before the UI launches, so a single fetch suffices.
    let mut raw = use_signal(Vec::<Track>::new);
    use_future(move || async move {
        if let Some(daw) = daw::get() {
            if let Ok(project) = daw.current_project().await {
                if let Ok(list) = project.tracks().all().await {
                    raw.set(list);
                }
            }
        }
    });

    // Build the themed TrackViews ONCE per track-set change (in an effect, so
    // the per-track signals are stable across renders — a fader drag isn't
    // reset on the next re-render).
    let mut views = use_signal(Vec::<TrackView>::new);
    use_effect(move || {
        let built = raw
            .read()
            .iter()
            .enumerate()
            .map(|(i, t)| track_view(i, t))
            .collect::<Vec<_>>();
        views.set(built);
    });

    let list = views.read().clone();
    rsx! {
        ThemeProvider {
            theme: ThemeContext::new(),
            div {
                style: "flex:1; min-height:0; display:flex; flex-direction:column;",
                // Toolbar with the REAPER hand-off.
                div {
                    style: "flex:0 0 auto; display:flex; align-items:center; gap:8px; \
                            padding:6px 12px; border-bottom:1px solid #27272a; background:#0b0b0d;",
                    span {
                        style: "font-size:11px; font-weight:700; letter-spacing:0.06em; \
                                text-transform:uppercase; color:#a1a1aa;",
                        "Mixer"
                    }
                    div { style: "flex:1;" }
                    button {
                        onclick: move |_| open_in_reaper(),
                        style: "padding:5px 12px; border-radius:6px; border:1px solid #3f3f46; \
                                background:#18181b; color:#e4e4e7; font-size:12px; cursor:pointer;",
                        "Open setlist in REAPER"
                    }
                }
                // The real reaper-style MCP.
                div {
                    style: "flex:1; min-height:0;",
                    if list.is_empty() {
                        div {
                            style: "padding:24px; color:#a1a1aa; font-size:13px;",
                            "No tracks yet — the Praise stems aren't seeded."
                        }
                    } else {
                        MixerControlPanel { tracks: list }
                    }
                }
            }
        }
    }
}
