//! Track Control Panel (TCP) — vertical track list mirroring a DAW's left sidebar.
//!
//! Shows all tracks with name, color, mute/solo/arm, volume, and pan.
//! Folder tracks are indented to reflect hierarchy.

use crate::prelude::*;
use daw_proto::Track;

/// Vertical track list panel that polls the DAW for track state.
///
/// Mirrors the TCP (Track Control Panel) in a traditional DAW — a compact
/// vertical list where each row represents one track with its key controls.
#[component]
pub fn TrackControlPanel() -> Element {
    let mut tracks = use_signal(Vec::<Track>::new);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut connected = use_signal(|| false);
    let mut selected_guid = use_signal(|| Option::<String>::None);

    use_future(move || async move {
        tracing::info!("TCP: waiting for DAW connection...");

        loop {
            if daw_control::Daw::try_get().is_some() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        let daw = daw_control::Daw::get();
        connected.set(true);
        tracing::info!("TCP: DAW connected, fetching tracks...");

        loop {
            match daw.current_project().await {
                Ok(project) => match project.tracks().all().await {
                    Ok(track_list) => {
                        tracks.set(track_list);
                        error_msg.set(None);
                    }
                    Err(e) => {
                        error_msg.set(Some(format!("Failed to fetch tracks: {:?}", e)));
                    }
                },
                Err(e) => {
                    error_msg.set(Some(format!("Failed to get project: {:?}", e)));
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    });

    let is_connected = *connected.read();
    let track_list = tracks.read();
    let err = error_msg.read();

    if !is_connected {
        return rsx! {
            div { class: "h-full w-full flex items-center justify-center bg-card text-muted-foreground text-sm",
                "Waiting for DAW connection..."
            }
        };
    }

    if let Some(msg) = err.as_ref() {
        return rsx! {
            div { class: "h-full w-full flex items-center justify-center bg-card text-red-400 text-sm p-4",
                "{msg}"
            }
        };
    }

    // Compute folder depth for indentation.
    // Track.folder_depth encodes: positive = start N folder levels, negative = close N levels.
    // We accumulate depth as we iterate to get the actual nesting level for each track.
    let depths = compute_depths(&track_list);

    rsx! {
        div { class: "h-full w-full flex flex-col bg-card overflow-hidden",
            // Header
            div { class: "px-2 py-1.5 border-b border-border flex items-center justify-between flex-shrink-0",
                span { class: "text-[10px] font-semibold text-muted-foreground uppercase tracking-wider", "TCP" }
                span { class: "text-[10px] text-muted-foreground", "{track_list.len()}" }
            }

            // Track rows — vertical scroll
            div { class: "flex-1 overflow-y-auto overflow-x-hidden",
                for (i, track) in track_list.iter().enumerate() {
                    {
                        let depth = depths.get(i).copied().unwrap_or(0);
                        let is_selected = selected_guid.read().as_deref() == Some(&track.guid);
                        let guid = track.guid.clone();
                        rsx! {
                            TcpRow {
                                key: "{track.guid}",
                                track: track.clone(),
                                depth: depth,
                                is_selected: is_selected,
                                on_click: move |_| {
                                    selected_guid.set(Some(guid.clone()));
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Compute nesting depth for each track based on folder_depth values.
///
/// REAPER encodes folder structure as:
/// - folder_depth > 0 -> this track starts a folder (depth increases after it)
/// - folder_depth < 0 -> this track closes |N| folder levels
/// - folder_depth == 0 -> normal track at current depth
fn compute_depths(tracks: &[Track]) -> Vec<u32> {
    let mut depths = Vec::with_capacity(tracks.len());
    let mut current_depth: i32 = 0;

    for track in tracks {
        // This track renders at current_depth
        depths.push(current_depth.max(0) as u32);

        // Then adjust depth for the next track
        if track.is_folder {
            current_depth += 1;
        }
        if track.folder_depth < 0 {
            current_depth += track.folder_depth; // negative, so subtracts
        }
    }

    depths
}

// ── TCP Row ─────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct TcpRowProps {
    track: Track,
    depth: u32,
    is_selected: bool,
    on_click: EventHandler<MouseEvent>,
}

#[component]
fn TcpRow(props: TcpRowProps) -> Element {
    let track = &props.track;
    let indent_px = props.depth * 12;

    let color_css = track
        .color
        .map(|c| format!("#{:06x}", c & 0xFFFFFF))
        .unwrap_or_else(|| "#6b7280".to_string());

    let vol_db = if track.volume > 0.0 {
        20.0 * track.volume.log10()
    } else {
        -100.0
    };

    let pan_label = if track.pan.abs() < 0.01 {
        "C".to_string()
    } else if track.pan < 0.0 {
        format!("{:.0}L", track.pan.abs() * 100.0)
    } else {
        format!("{:.0}R", track.pan * 100.0)
    };

    let row_bg = if props.is_selected {
        "bg-accent/20"
    } else {
        "hover:bg-muted/50"
    };

    let mute_class = if track.muted {
        "bg-red-500 text-white"
    } else {
        "bg-zinc-700/50 text-zinc-500"
    };
    let solo_class = if track.soloed {
        "bg-yellow-500 text-black"
    } else {
        "bg-zinc-700/50 text-zinc-500"
    };
    let arm_class = if track.armed {
        "bg-red-600 text-white"
    } else {
        "bg-zinc-700/50 text-zinc-500"
    };

    let name_weight = if track.is_folder {
        "font-semibold"
    } else {
        "font-normal"
    };

    rsx! {
        div {
            class: "flex items-center gap-1 px-1 py-0.5 border-b border-border/50 cursor-pointer {row_bg}",
            style: "padding-left: {indent_px + 4}px;",
            onclick: move |e| props.on_click.call(e),

            // Color swatch
            div {
                class: "w-2 h-5 rounded-sm flex-shrink-0",
                style: "background-color: {color_css};",
            }

            // Track name
            div { class: "flex-1 min-w-0 text-[11px] text-foreground truncate {name_weight}",
                "{track.name}"
            }

            // M / S / R buttons
            div { class: "flex gap-0.5 flex-shrink-0",
                span { class: "w-4 h-4 flex items-center justify-center rounded text-[8px] font-bold {mute_class}", "M" }
                span { class: "w-4 h-4 flex items-center justify-center rounded text-[8px] font-bold {solo_class}", "S" }
                span { class: "w-4 h-4 flex items-center justify-center rounded text-[8px] font-bold {arm_class}", "R" }
            }

            // Volume (dB)
            div { class: "w-10 text-right text-[9px] font-mono text-muted-foreground flex-shrink-0",
                if vol_db > -100.0 {
                    "{vol_db:.1}"
                } else {
                    "-inf"
                }
            }

            // Pan
            div { class: "w-6 text-center text-[9px] font-mono text-muted-foreground flex-shrink-0",
                "{pan_label}"
            }
        }
    }
}
