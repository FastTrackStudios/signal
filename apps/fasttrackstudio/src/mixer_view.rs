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

/// Build a themed `TrackView` (with its live signals) from a daw `Track`
/// plus its routing facts (any sends / any receives).
fn track_view(id: usize, t: &Track, has_sends: bool, has_receives: bool) -> TrackView {
    let hex = t.color.map(|c| format!("#{:06x}", c & 0x00FF_FFFF));
    let mut tv = TrackView::new(id, t.name.clone(), hex.as_deref())
        .fader(t.volume as f32)
        .depth(t.folder_depth.max(0) as u32)
        .routing(has_sends, has_receives);
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

/// Invisible per-track sync: watches a `TrackView`'s `mute`/`solo`/`fader`/`pan`
/// signals and pushes each change to the daw engine via the track's real GUID.
///
/// Rendered as a sibling of the `MixerControlPanel` (which owns the strips and
/// their shared signals) so the MCP stays a pure view — this component is the
/// one place UI intent becomes engine state. First run re-writes the current
/// engine state (a harmless no-op). Solo is ADDITIVE (`solo`/`unsolo`), volume
/// is 1:1 (fader 0–1 → `set_volume`), pan maps the 0–1 signal (0.5 = centre)
/// back to the facade's −1..1.
#[component]
fn TrackSync(guid: String, tv: TrackView) -> Element {
    // Signals are Copy — pull them out so each effect captures only its own.
    let mute = tv.mute;
    let solo = tv.solo;
    let fader = tv.fader;
    let pan = tv.pan;

    // Run `op(handle)` against the track resolved by GUID on the current project.
    async fn with_track<F, Fut>(guid: String, op: F)
    where
        F: FnOnce(daw::rpc::TrackHandle) -> Fut,
        Fut: std::future::Future<Output = Result<(), daw::rpc::Error>>,
    {
        if let Some(daw) = daw::get()
            && let Ok(project) = daw.current_project().await
            && let Ok(Some(handle)) = project.tracks().by_guid(&guid).await
        {
            let _ = op(handle).await;
        }
    }

    {
        let guid = guid.clone();
        use_effect(move || {
            let muted = *mute.read();
            let guid = guid.clone();
            spawn(with_track(guid, move |h| async move {
                if muted { h.mute().await } else { h.unmute().await }
            }));
        });
    }
    {
        let guid = guid.clone();
        use_effect(move || {
            let soloed = *solo.read();
            let guid = guid.clone();
            spawn(with_track(guid, move |h| async move {
                if soloed { h.solo().await } else { h.unsolo().await }
            }));
        });
    }
    {
        let guid = guid.clone();
        use_effect(move || {
            let vol = *fader.read() as f64;
            let guid = guid.clone();
            spawn(with_track(guid, move |h| async move { h.set_volume(vol).await }));
        });
    }
    {
        let guid = guid.clone();
        use_effect(move || {
            let pan_val = (*pan.read() as f64) * 2.0 - 1.0; // 0..1 (0.5=centre) → −1..1
            let guid = guid.clone();
            spawn(with_track(guid, move |h| async move { h.set_pan(pan_val).await }));
        });
    }

    rsx! {}
}

#[component]
pub fn MixerWorkspace() -> Element {
    // Raw daw tracks for the ACTIVE song. The mixer follows the current song:
    // the engine switches the current project on every seek/song-change, so
    // `current_project()` is always the active song's project. We refetch when
    // the active SONG changes (not on in-song section seeks — a memo gates the
    // effect on `song_index` so mid-song navigation doesn't rebuild the strips
    // and drop fader/meter state).
    let mut raw = use_signal(Vec::<(Track, bool, bool)>::new);
    let active_song = use_memo(move || session_ui::ACTIVE_INDICES.read().song_index);
    use_effect(move || {
        // Reactive dependency: re-run only when the active song index changes.
        let _ = active_song.read();
        spawn(async move {
            if let Some(daw) = daw::get()
                && let Ok(project) = daw.current_project().await
                && let Ok(list) = project.tracks().all().await
            {
                // Routing facts per track (any sends / any receives) so the
                // strip's routing indicator is truthful. In-process RPCs —
                // cheap even for a large mixer.
                let mut with_routing = Vec::with_capacity(list.len());
                for t in list {
                    let (mut s, mut r) = (false, false);
                    if let Ok(Some(h)) = project.tracks().by_guid(&t.guid).await {
                        s = h.sends().all().await.map(|v| !v.is_empty()).unwrap_or(false);
                        r = h
                            .receives()
                            .all()
                            .await
                            .map(|v| !v.is_empty())
                            .unwrap_or(false);
                    }
                    with_routing.push((t, s, r));
                }
                raw.set(with_routing);
            }
        });
    });

    // Build the themed TrackViews ONCE per track-set change (in an effect, so
    // the per-track signals are stable across renders — a fader drag isn't
    // reset on the next re-render). Each carries its real engine GUID (the
    // TrackView.id is only the enum index) so `TrackSync` can push to the daw.
    let mut entries = use_signal(Vec::<(String, TrackView)>::new);
    use_effect(move || {
        let built = raw
            .read()
            .iter()
            .enumerate()
            .map(|(i, (t, s, r))| (t.guid.clone(), track_view(i, t, *s, *r)))
            .collect::<Vec<_>>();
        entries.set(built);
    });

    // Per-track metering pump (~30 fps, only while this view is mounted — the
    // future is dropped on unmount). Reads the standalone engine's post-fader
    // peak bank DIRECTLY in-process (already linear 0–1), so ~130 tracks cost
    // zero RPC / marshalling — far lighter than per-track `Peaks` calls. Cell
    // index i matches project track index i (the same order `tracks().all()`
    // returns), so it aligns with `entries`.
    use_future(move || async move {
        loop {
            if let Some(eng) = crate::session_engine::engine() {
                let meters = eng.standalone.meters();
                if !meters.is_empty() {
                    // Snapshot the meter signals (Copy) without a reactive read.
                    let sigs: Vec<(usize, Signal<f32>, Signal<f32>, Signal<f32>)> = entries
                        .peek()
                        .iter()
                        .enumerate()
                        .map(|(i, (_, tv))| (i, tv.level, tv.level_right, tv.peak))
                        .collect();
                    for (i, mut level, mut level_right, mut peak) in sigs {
                        if let Some(cell) = meters.cell(i) {
                            level.set(cell.peak(0).clamp(0.0, 1.0));
                            level_right.set(cell.peak(1).clamp(0.0, 1.0));
                            peak.set(cell.hold(0).max(cell.hold(1)).clamp(0.0, 1.0));
                        }
                    }
                }
            }
            architect::platform::sleep(std::time::Duration::from_millis(33)).await;
        }
    });

    let entries_now = entries.read().clone();
    let list: Vec<TrackView> = entries_now.iter().map(|(_, tv)| tv.clone()).collect();
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
                            "No tracks in the current song."
                        }
                    } else {
                        MixerControlPanel { tracks: list }
                    }
                }
                // Invisible engine-sync siblings — one per track, pushing
                // mute/solo/fader/pan changes to the daw. NOT inside the MCP.
                div {
                    style: "display:none;",
                    for (guid, tv) in entries_now.iter() {
                        TrackSync { key: "{guid}", guid: guid.clone(), tv: tv.clone() }
                    }
                }
            }
        }
    }
}
