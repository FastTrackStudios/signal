//! Performance View Layout
//!
//! Complete performance view layout with sidebar navigation and main progress display.
//! This is a reusable layout component that apps can import and use directly.
//!
//! Based on the `FastTrackStudio` desktop app performance view.
//! Uses Tailwind CSS for styling.
//!
//! Components call `SetlistService` methods directly via `Session::get().setlist()`.
//! The consuming app must call `Session::init()` during startup.

use crate::components::{
    ChannelStripPropsBuilder_Optional, CommentMarker, FadedSongTitle,
    FadedSongTitlePropsBuilder_Optional, LoopIndicatorData, LyricSyncViewPropsBuilder_Optional,
    MeasureIndicator, MixGroupPropsBuilder_Optional, MixerViewPropsBuilder_Optional,
    NudgeBtnPropsBuilder_Optional, ProgressSection, SectionItem, SectionProgressBar,
    SectionProgressBarPropsBuilder_Optional, SegmentedProgressBarPropsBuilder_Optional, SongItem,
    SongItemData, SongItemPropsBuilder_Optional, SongProgressBar,
    SongProgressBarPropsBuilder_Optional, SongTitle, SongTitlePropsBuilder_Optional,
    TempoCardPropsBuilder_Optional, TempoMarkerData, TimeSignatureCardPropsBuilder_Optional,
    TransportControlBar, TransportControlBarPropsBuilder_Optional, TransportStats,
    TransportStatsPropsBuilder_Optional,
};
use crate::prelude::*;
use crate::signals::{
    Session, ACTIVE_INDICES, ARMED_TRACK_COUNT, LATENCY_TRACKER, PLAYBACK_STATE, SETLIST_STRUCTURE,
    SONG_CHARTS, SONG_TRANSPORT, SONG_VIEWS,
};

/// The `#<KEY>` metadata token from a keyflow chart (`"A"` from `#A 127bpm`).
/// A finite `f64`, clamped to `i32`'s range, as an `i32`.
fn clamped_i32(x: f64) -> i32 {
    let clamped = x.clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    // Range-checked above; std has no non-`as` float-to-int conversion.
    #[allow(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    {
        clamped as i32
    }
}

fn chart_meta_key(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|t| {
            t.len() > 1
                && t.starts_with('#')
                && t.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic)
        })
        .and_then(|t| t.get(1..).map(str::to_string))
}

/// Key label for a setlist song in the navigator: the setlist override's
/// rendered key (the fingered shape key when a capo is set), else the song's
/// own key from its chart metadata. `None` hides the badge.
fn nav_key_label(song_idx: usize) -> Option<String> {
    let setlist = SETLIST_STRUCTURE.read();
    let song = setlist.songs.get(song_idx)?;
    if let Some(k) = SONG_VIEWS
        .read()
        .get(&song.project_guid)
        .and_then(super::super::signals::SongView::display_key_label)
    {
        return Some(k);
    }
    let charts = SONG_CHARTS.read();
    let text = charts
        .get(&song.project_guid)
        .map(|c| c.chart_text.clone())
        .or_else(|| song.chart_text.clone())?;
    chart_meta_key(&text)
}

/// Performance view layout
///
/// Complete performance view with:
/// - Large progress visualization with transport controls
///
/// This component subscribes to global signals and provides a complete
/// performance view interface that apps can use directly.
///
/// # Example
///
/// ```rust,no_run
/// use session_ui::PerformanceLayout;
/// use dioxus::prelude::*;
///
/// fn App() -> Element {
///     rsx! {
///         PerformanceLayout {}
///     }
/// }
/// ```
#[component]
pub fn PerformanceLayout() -> Element {
    // PERFORMANCE: This top-level component should NOT subscribe to any high-frequency signals.
    // Child components handle their own subscriptions to minimize re-renders.
    //
    // Architecture:
    // - PerformanceMainContent: Subscribes to transport signals for progress display
    // - TransportControlBar: Subscribes to PLAYBACK_STATE for play/pause button

    rsx! {
        div {
            class: "flex h-full w-full bg-background text-foreground",
            PerformanceMainContent {}
        }
    }
}

/// Static song structure for sidebar - memoized to avoid recalculation
/// This only contains data that changes when the setlist structure changes,
/// NOT transport/progress data which updates at 60Hz
#[derive(Clone, PartialEq)]
struct SidebarSongStructure {
    name: String,
    bright_color: String,
    muted_color: String,
    sections: Vec<SidebarSectionStructure>,
}

#[derive(Clone, PartialEq)]
struct SidebarSectionStructure {
    label: String,
    bright_color: String,
    muted_color: String,
    comment: Option<String>,
    start_seconds: f64,
    end_seconds: f64,
}

/// Performance sidebar component
///
/// Displays song list with expandable sections and progress bars.
/// Shows progress for ALL songs that have transport state (independent playback).
///
/// PERFORMANCE: This component only subscribes to `ACTIVE_INDICES` for selection state.
/// Transport updates (60Hz) are handled by individual `SidebarSongItem` components
/// to avoid re-rendering the entire sidebar.
#[component]
pub fn PerformanceSidebar(
    /// When set, a song click calls this instead of the `Session` singleton —
    /// lets a client-driven host (e.g. the web setlist player) reuse this
    /// navigator without an engine. Falls back to `Session` when `None`.
    #[props(default)]
    on_song_select: Option<Callback<usize>>,
    /// When set, a section click calls this `(song_idx, section_idx)` instead
    /// of the `Session` singleton.
    #[props(default)]
    on_section_select: Option<Callback<(usize, usize)>>,
    /// When true, the navigator's active song uses a themed selection
    /// (subtle muted-color tint + primary ring) instead of the dark "stage"
    /// fill — for light-themed hosts like the Task setlist player. Defaults
    /// false, so the desktop performance view is unchanged.
    #[props(default)]
    plain_selection: bool,
) -> Element {
    // Read active indices - updates when song/section selection changes
    let indices = ACTIVE_INDICES.read();
    let active_song_index = indices.song_index;
    let active_section_index = indices.section_index;
    drop(indices);

    // Memoize the static song structure - only recalculates when setlist changes
    let song_structures = use_memo(move || {
        let setlist = SETLIST_STRUCTURE.read();
        setlist
            .songs
            .iter()
            .map(|song| SidebarSongStructure {
                name: song.name.clone(),
                bright_color: song.bright_color(),
                muted_color: song.muted_color(),
                sections: song
                    .sections
                    .iter()
                    .map(|section| SidebarSectionStructure {
                        label: section.display_name(),
                        bright_color: section.bright_color(),
                        muted_color: section.muted_color(),
                        comment: section.comment.clone(),
                        start_seconds: section.start_seconds,
                        end_seconds: section.end_seconds,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>()
    });

    rsx! {
        div {
            class: "h-full w-full bg-sidebar overflow-y-auto",

            // Header
            div {
                class: "p-4",
                h2 {
                    class: "text-xl font-bold text-sidebar-foreground mb-4",
                    "Navigator"
                }
            }

            // Song list - each song item handles its own transport subscription
            div {
                class: "space-y-1 pr-4 pb-4",

                for (song_idx, song_structure) in song_structures.read().iter().enumerate() {
                    SidebarSongItemReactive {
                        key: "{song_idx}",
                        song_idx: song_idx,
                        song_structure: song_structure.clone(),
                        is_expanded: active_song_index == Some(song_idx),
                        current_section_index: if active_song_index == Some(song_idx) {
                            active_section_index
                        } else {
                            None
                        },
                        on_song_select,
                        on_section_select,
                        plain_selection,
                    }
                }
            }
        }
    }
}

/// Reactive song item that subscribes to its own transport updates
///
/// PERFORMANCE: Only subscribes to transport if song is expanded or playing.
/// Collapsed/inactive songs use `peek()` to avoid 60Hz re-renders.
/// Section progress is only calculated for expanded songs.
#[component]
fn SidebarSongItemReactive(
    song_idx: usize,
    song_structure: SidebarSongStructure,
    is_expanded: bool,
    current_section_index: Option<usize>,
    #[props(default)] on_song_select: Option<Callback<usize>>,
    #[props(default)] on_section_select: Option<Callback<(usize, usize)>>,
    #[props(default)] plain_selection: bool,
) -> Element {
    // First, peek to check if this song is playing (without subscribing)
    let is_song_playing = SONG_TRANSPORT
        .peek()
        .get(&song_idx)
        .is_some_and(|t| t.is_playing);

    // PERFORMANCE: Only subscribe to transport updates if we need to show progress
    // (song is expanded or actively playing). Otherwise, use static 0% progress.
    let needs_transport = is_expanded || is_song_playing;

    let (song_progress, position_seconds) = if needs_transport {
        // Subscribe to transport - this component will re-render on updates
        let transport = SONG_TRANSPORT.read().get(&song_idx).cloned();

        let pos = transport
            .as_ref()
            .and_then(|t| t.position.time.map(|time| time.as_seconds()))
            .unwrap_or(0.0);

        // Calculate song progress
        let song_start = song_structure
            .sections
            .first()
            .map_or(0.0, |s| s.start_seconds);
        let song_end = song_structure
            .sections
            .last()
            .map_or(0.0, |s| s.end_seconds);
        let song_duration = song_end - song_start;
        let progress = if song_duration > 0.0 && transport.is_some() {
            ((pos - song_start) / song_duration * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };

        (progress, pos)
    } else {
        // Inactive song - no transport subscription, static 0% progress
        (0.0, 0.0)
    };

    // PERFORMANCE: Only build section data if expanded (sections are hidden otherwise)
    let sections: Vec<SectionItem> = if is_expanded {
        song_structure
            .sections
            .iter()
            .map(|section| {
                let section_duration = section.end_seconds - section.start_seconds;
                let section_progress = if section_duration > 0.0
                    && position_seconds >= section.start_seconds
                    && position_seconds <= section.end_seconds
                {
                    ((position_seconds - section.start_seconds) / section_duration * 100.0)
                        .clamp(0.0, 100.0)
                } else if position_seconds > section.end_seconds {
                    100.0
                } else {
                    0.0
                };

                SectionItem {
                    label: section.label.clone(),
                    progress: section_progress,
                    bright_color: section.bright_color.clone(),
                    muted_color: section.muted_color.clone(),
                    comment: section.comment.clone(),
                }
            })
            .collect()
    } else {
        // Collapsed song - no sections needed (won't be rendered)
        Vec::new()
    };

    let song_data = SongItemData {
        label: song_structure.name.clone(),
        progress: song_progress,
        bright_color: song_structure.bright_color.clone(),
        muted_color: song_structure.muted_color,
        sections,
        key_label: nav_key_label(song_idx),
    };

    rsx! {
        SongItem {
            song_data: song_data,
            index: song_idx,
            is_expanded: is_expanded,
            is_playing: is_song_playing,
            current_section_index: current_section_index,
            plain_selection: plain_selection,
            on_song_click: Callback::new(move |()| {
                if let Some(cb) = on_song_select {
                    cb.call(song_idx);
                } else {
                    spawn(async move {
                        match Session::get().setlist().seek_to_song(song_idx).await {
                            Ok(()) => tracing::info!("seek_to_song({}) ok", song_idx),
                            Err(e) => tracing::error!("seek_to_song({}) failed: {:?}", song_idx, e),
                        }
                    });
                }
            }),
            on_section_click: Callback::new(move |section_idx| {
                if let Some(cb) = on_section_select {
                    cb.call((song_idx, section_idx));
                } else {
                    spawn(async move {
                        let _ = Session::get().setlist().seek_to_section(song_idx, section_idx).await;
                    });
                }
            }),
        }
    }
}

/// Standalone transport control panel for the dock system.
///
/// Reads playback state and loop state from global signals, wires up
/// callbacks to `Session::get().setlist()` service methods.
#[component]
pub fn TransportPanel(
    /// Show the Arm + Record controls. Recording environments (a REAPER-
    /// connected desktop) want them; a playback-only setlist surface
    /// (the in-process daw-standalone player, the browser remote) does
    /// not — there's nothing here to arm or record into. Defaults true.
    #[props(default = true)]
    show_recording: bool,
) -> Element {
    let indices = ACTIVE_INDICES.read();
    let is_looping = indices.looping;
    drop(indices);

    let playback_state = *PLAYBACK_STATE.read();
    let is_playing = matches!(
        playback_state,
        daw_proto::PlayState::Playing | daw_proto::PlayState::Recording
    );
    let is_recording = matches!(playback_state, daw_proto::PlayState::Recording);

    // Record-arm has no push signal yet, so track the operator's intent
    // locally (optimistic). Toggling sends the arm/disarm to the active
    // song's selected tracks.
    let mut is_armed = use_signal(|| false);

    rsx! {
        div {
            class: "h-full w-full flex flex-col bg-background",
            TransportControlBar {
                is_playing: is_playing,
                is_looping: is_looping,
                is_recording: is_recording,
                is_armed: is_armed(),
                armed_track_count: *ARMED_TRACK_COUNT.read(),
                show_recording: show_recording,
                on_play_pause: Callback::new(move |()| {
                    LATENCY_TRACKER.write().start_play_toggle();
                    spawn(async move {
                        let _ = Session::get().setlist().toggle_playback().await;
                    });
                }),
                on_loop_toggle: Callback::new(move |()| {
                    spawn(async move {
                        let _ = Session::get().setlist().toggle_song_loop().await;
                    });
                }),
                on_record_toggle: Callback::new(move |()| {
                    spawn(async move {
                        let _ = Session::get().setlist().toggle_recording().await;
                    });
                }),
                on_arm_toggle: Callback::new(move |()| {
                    let next = !is_armed();
                    is_armed.set(next);
                    spawn(async move {
                        let _ = Session::get().setlist().set_song_record_arm(next).await;
                    });
                }),
                on_back: Callback::new(move |()| {
                    spawn(async move {
                        let _ = Session::get().setlist().previous_section().await;
                    });
                }),
                on_forward: Callback::new(move |()| {
                    spawn(async move {
                        let _ = Session::get().setlist().next_section().await;
                    });
                }),
            }
        }
    }
}

/// Main performance content area
///
/// Displays song title, both progress bars (song and section), transport info badges,
/// and transport controls at the bottom.
///
/// PERFORMANCE: This component subscribes to `ACTIVE_INDICES` and `SONG_TRANSPORT`.
/// Static song data (progress sections, markers) is memoized and only recalculates
/// when song selection changes.
#[component]
fn PerformanceMainContent() -> Element {
    // Read indices - we need song_index and section_index
    let indices = ACTIVE_INDICES.read();
    let song_index = indices.song_index;
    let _section_index = indices.section_index;
    let queued_target = indices.queued_target.clone();
    drop(indices);

    // Get transport state for current song
    let transport_state = song_index.and_then(|idx| SONG_TRANSPORT.read().get(&idx).cloned());

    // Memoize current song - reads ACTIVE_INDICES inside so it re-fires on song change
    let current_song = use_memo(move || {
        let indices = ACTIVE_INDICES.read();
        let idx = indices.song_index;
        let setlist = SETLIST_STRUCTURE.read();
        idx.and_then(|i| setlist.songs.get(i).cloned())
    });

    // Memoize current section - reads ACTIVE_INDICES inside so it re-fires on section change
    let current_section = use_memo(move || {
        let indices = ACTIVE_INDICES.read();
        let sec_idx = indices.section_index;
        current_song
            .read()
            .as_ref()
            .and_then(|s| sec_idx.and_then(|idx| s.sections.get(idx).cloned()))
    });

    // Memoize progress sections - reads ACTIVE_INDICES to ensure reactivity on song change
    let progress_sections = use_memo(move || {
        let _indices = ACTIVE_INDICES.read();
        current_song
            .read()
            .as_ref()
            .map(|song| {
                let sections_start = song
                    .sections
                    .first()
                    .map_or(song.start_seconds, |s| s.start_seconds);
                let sections_end = song
                    .sections
                    .last()
                    .map_or(song.end_seconds, |s| s.end_seconds);
                let sections_duration = sections_end - sections_start;

                if sections_duration <= 0.0 {
                    return Vec::new();
                }

                song.sections
                    .iter()
                    .map(|section| {
                        let start_percent =
                            ((section.start_seconds - sections_start) / sections_duration) * 100.0;
                        let end_percent =
                            ((section.end_seconds - sections_start) / sections_duration) * 100.0;

                        ProgressSection {
                            name: section.display_name(),
                            short_name: section.short_display(),
                            start_percent: start_percent.max(0.0),
                            end_percent: end_percent.min(100.0),
                            color: section.bright_color(),
                            comment: section.comment.clone(),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });

    // Calculate progress values - these depend on transport_state which updates at 60Hz
    let song_progress = {
        let song = current_song.read();
        song.as_ref()
            .and_then(|song| {
                transport_state.as_ref().map(|t| {
                    let position_seconds = t.position.time.map_or(0.0, |time| time.as_seconds());
                    let sections_start = song
                        .sections
                        .first()
                        .map_or(song.start_seconds, |s| s.start_seconds);
                    let sections_end = song
                        .sections
                        .last()
                        .map_or(song.end_seconds, |s| s.end_seconds);
                    let sections_duration = sections_end - sections_start;

                    if sections_duration <= 0.0 {
                        return 0.0;
                    }

                    let relative_pos = position_seconds - sections_start;
                    (relative_pos / sections_duration) * 100.0
                })
            })
            .unwrap_or(0.0)
            .clamp(0.0, 100.0)
    };

    let section_progress = {
        let section = current_section.read();
        section
            .as_ref()
            .and_then(|section| {
                transport_state.as_ref().map(|t| {
                    let position_seconds = t.position.time.map_or(0.0, |time| time.as_seconds());
                    let section_duration = section.end_seconds - section.start_seconds;
                    if section_duration > 0.0
                        && position_seconds >= section.start_seconds
                        && position_seconds <= section.end_seconds
                    {
                        ((position_seconds - section.start_seconds) / section_duration) * 100.0
                    } else if position_seconds > section.end_seconds {
                        100.0
                    } else {
                        0.0
                    }
                })
            })
            .unwrap_or(0.0)
    };

    // Get next song info - use peek() since we don't need reactivity for this
    let next_song_info = {
        let setlist = SETLIST_STRUCTURE.peek();
        song_index.and_then(|idx| {
            idx.checked_add(1).and_then(|next_idx| {
                setlist
                    .songs
                    .get(next_idx)
                    .map(|song| (song.name.clone(), song.bright_color()))
            })
        })
    };

    // Song key for animation detection (changes when song changes)
    let song_key = song_index.map(|i| i.to_string());

    // Memoize tempo markers - reads ACTIVE_INDICES to ensure reactivity on song change
    let tempo_markers = use_memo(move || {
        let _indices = ACTIVE_INDICES.read();
        current_song
            .read()
            .as_ref()
            .map(|song| {
                let mut markers = Vec::new();

                if let Some(tempo) = song.tempo {
                    markers.push(TempoMarkerData {
                        position_percent: 0.0,
                        label: format!("{tempo:.0} bpm"),
                        is_tempo: true,
                        is_time_sig: false,
                        show_line_only: false,
                    });
                }

                if let Some(ref ts) = song.time_signature {
                    markers.push(TempoMarkerData {
                        position_percent: 0.0,
                        label: format!("{}/{}", ts.numerator, ts.denominator),
                        is_tempo: false,
                        is_time_sig: true,
                        show_line_only: false,
                    });
                }

                // Mid-song time-signature changes from the parsed chart. Each
                // change records the section it begins in; map that to the
                // section's start time → percent along the song so the bar
                // marks meter changes, not just the opening signature.
                if let Some(chart) = song.parsed_chart.as_ref() {
                    let song_start = song.start_seconds;
                    let song_duration = song.end_seconds - song_start;
                    if song_duration > 0.0 {
                        for change in &chart.time_signature_changes {
                            let Some(section) = song.sections.get(change.section_index) else {
                                continue;
                            };
                            let percent = ((section.start_seconds - song_start) / song_duration
                                * 100.0)
                                .clamp(0.0, 100.0);
                            // The opening signature already marks the start;
                            // skip a change landing on it to avoid a dup marker.
                            if percent <= 0.5 {
                                continue;
                            }
                            markers.push(TempoMarkerData {
                                position_percent: percent,
                                label: format!(
                                    "{}/{}",
                                    change.time_signature.numerator,
                                    change.time_signature.denominator
                                ),
                                is_tempo: false,
                                is_time_sig: true,
                                show_line_only: false,
                            });
                        }
                    }
                }

                markers
            })
            .unwrap_or_default()
    });

    // Memoize comment markers - reads ACTIVE_INDICES to ensure reactivity on song change
    let comment_markers = use_memo(move || {
        let _indices = ACTIVE_INDICES.read();
        current_song
            .read()
            .as_ref()
            .map(|song| {
                let sections_start = song
                    .sections
                    .first()
                    .map_or(song.start_seconds, |s| s.start_seconds);
                let sections_end = song
                    .sections
                    .last()
                    .map_or(song.end_seconds, |s| s.end_seconds);
                let sections_duration = sections_end - sections_start;

                if sections_duration <= 0.0 {
                    return Vec::new();
                }

                song.comments
                    .iter()
                    .filter_map(|comment| {
                        if comment.position_seconds >= sections_start
                            && comment.position_seconds <= sections_end
                        {
                            let position_percent = ((comment.position_seconds - sections_start)
                                / sections_duration)
                                * 100.0;
                            Some(CommentMarker {
                                position_percent: position_percent.clamp(0.0, 100.0),
                                text: comment.text.clone(),
                                color: comment.color_hex(),
                                is_count_in: comment.is_count_in,
                                section_only: comment.section_only,
                                position_seconds: comment.position_seconds,
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    });

    // PERFORMANCE: Extract tempo/time sig values and memoize measure indicators
    // These only change when tempo/time signature actually changes, not every frame
    let current_bpm = transport_state.as_ref().map_or(120.0, |t| t.bpm);
    let current_time_sig_num = transport_state.as_ref().map_or(4, |t| t.time_sig_num);
    let current_time_sig_denom = transport_state.as_ref().map_or(4, |t| t.time_sig_denom);

    // Memoize measure indicators - reads ACTIVE_INDICES to ensure reactivity on song/section change
    let measure_indicators = use_memo(move || {
        let _indices = ACTIVE_INDICES.read();
        let section = current_section.read();
        let song = current_song.read();
        section
            .as_ref()
            .map(|section| {
                let section_duration = section.end_seconds - section.start_seconds;
                let section_start = section.start_seconds;
                if section_duration <= 0.0 {
                    return Vec::new();
                }

                let bpm = current_bpm;
                let time_sig_num = current_time_sig_num;
                let time_sig_denom = current_time_sig_denom;

                let seconds_per_beat = 60.0 / bpm;
                let seconds_per_measure = seconds_per_beat * f64::from(time_sig_num);
                let num_measures =
                    clamped_i32((section_duration / seconds_per_measure).ceil().max(0.0));

                // Determine the content start (SONGSTART marker position)
                let content_start = song.as_ref().map_or(0.0, |song| {
                    song.start_seconds + song.count_in_seconds.unwrap_or(0.0)
                });

                // Calculate measure number relative to content start (SONGSTART = measure 1)
                // Measures before SONGSTART are 0, -1, -2, etc.
                let section_start_measure =
                    clamped_i32(((section_start - content_start) / seconds_per_measure).floor());

                (0..num_measures)
                    .map(|i| {
                        let position_percent =
                            (f64::from(i) * seconds_per_measure / section_duration) * 100.0;
                        let measure_from_content_start = section_start_measure.saturating_add(i);

                        let display_number = if measure_from_content_start >= 0 {
                            measure_from_content_start.saturating_add(1)
                        } else {
                            measure_from_content_start
                        };

                        MeasureIndicator {
                            position_percent: position_percent.min(100.0),
                            measure_number: display_number,
                            time_signature: Some((
                                u8::try_from(time_sig_num).unwrap_or(4),
                                u8::try_from(time_sig_denom).unwrap_or(4),
                            )),
                            musical_position: daw_proto::MusicalPosition::new(
                                measure_from_content_start,
                                0,
                                0,
                            ),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    });

    // Memoize section-specific comment markers - reads ACTIVE_INDICES to ensure reactivity
    let section_comment_markers = use_memo(move || {
        let _indices = ACTIVE_INDICES.read();
        let section = current_section.read();
        let song = current_song.read();
        section
            .as_ref()
            .and_then(|section| {
                song.as_ref().map(|song| {
                    let section_duration = section.end_seconds - section.start_seconds;
                    if section_duration <= 0.0 {
                        return Vec::new();
                    }

                    song.comments
                        .iter()
                        .filter_map(|comment| {
                            if comment.position_seconds >= section.start_seconds
                                && comment.position_seconds < section.end_seconds
                            {
                                let position_percent = ((comment.position_seconds
                                    - section.start_seconds)
                                    / section_duration)
                                    * 100.0;
                                Some(CommentMarker {
                                    position_percent: position_percent.clamp(0.0, 100.0),
                                    text: comment.text.clone(),
                                    color: comment.color_hex(),
                                    is_count_in: comment.is_count_in,
                                    section_only: comment.section_only,
                                    position_seconds: comment.position_seconds,
                                })
                            } else {
                                None
                            }
                        })
                        .collect()
                })
            })
            .unwrap_or_default()
    });

    // Build loop indicator data from transport state
    // The loop_region in TransportState is already in percentages (0.0-1.0)
    let loop_indicator = transport_state.as_ref().and_then(|t| {
        if t.is_looping {
            t.loop_region.map(|(start, end)| LoopIndicatorData {
                enabled: true,
                start_percent: start * 100.0, // Convert to percentage (0-100)
                end_percent: end * 100.0,
            })
        } else {
            None
        }
    });

    // Capture song_index for closures
    let song_index_for_section_click = song_index;

    rsx! {
        div {
            class: "flex-1 flex flex-col overflow-hidden bg-background",

            // Scrollable content area
            div {
                class: "flex-1 overflow-y-auto",
                // Header, then the map, then the work.
                //
                // This used to be a centred stack: the song bar sat in the
                // middle and everything else hung off it by `calc(50% ±
                // …rem)`. That put the thing you read most in the middle of
                // whatever space there happened to be, and it left no room
                // that belonged to anything else — every addition had to
                // push off the centre too. Top-anchored, the measurements
                // are gaps between siblings, and everything under the bars
                // is free.
                div {
                    class: "flex flex-col h-full gap-3",

                    // ── The header: what song, and where in it ──────
                    //
                    // Everything you ask ABOUT the song, in one band
                    // across the top: what it is, what is next, where
                    // the transport is. The bars go under it because
                    // they are the song itself rather than facts about
                    // it — you read the header, then you read the map.
                    //
                    // The title used to be the largest thing on the
                    // screen, centred above everything. It is a label,
                    // and it was spending a third of the view saying
                    // what you already knew you were playing.
                    div {
                        class: "flex-none flex items-center justify-between gap-6 px-4 py-2 border-b border-border bg-card/40",
                        div { class: "flex flex-col gap-0.5 min-w-0",
                            if let Some(ref song) = *current_song.read() {
                                SongTitle {
                                    song_name: song.name.clone(),
                                }
                            }
                            // Next song (faded)
                            if let Some((next_name, next_color)) = next_song_info {
                                FadedSongTitle {
                                    song_name: next_name,
                                    color: next_color,
                                }
                            }
                        }
                        div { class: "flex items-center gap-4 flex-none",
                            // Which tablet this is. In the header rather
                            // than in the panel, because it is true of
                            // the DEVICE and not of the take: it stays
                            // put when the take changes, and tapping it
                            // is how you hand the tablet to somebody
                            // else.
                            if let Some(role) = crate::signals::ROLE() {
                                button {
                                    class: "flex items-center gap-2 px-3 py-1.5 rounded-full bg-secondary text-secondary-foreground",
                                    "data-testid": "role-indicator",
                                    onclick: move |_| *crate::signals::ROLE.write() = None,
                                    span { class: "h-2 w-2 rounded-full bg-primary" }
                                    span { class: "text-sm font-semibold", "{role}" }
                                }
                            }
                            if let Some(ref transport) = transport_state {
                                TransportStats {
                                    musical: transport.position.musical,
                                    seconds: transport.position.time.map_or(0.0, |t| t.as_seconds()),
                                    bpm: transport.bpm,
                                    beats_per_bar: transport.time_sig_num,
                                    beat_unit: transport.time_sig_denom,
                                    looping: transport.is_looping,
                                }
                            }
                        }
                    }

                    // Main Song Progress Bar
                    //
                    // Edge to edge: it is a map of the song and every
                    // pixel of it is a place you can be, so a margin
                    // either side is two places you cannot.
                    div {
                        key: "{song_key.clone().unwrap_or_else(|| \"none\".to_string())}",
                        class: "flex-none w-full",
                        if !progress_sections.read().is_empty() {
                            SongProgressBar {
                                progress: song_progress,
                                sections: progress_sections.read().clone(),
                                on_section_click: Some(Callback::new(move |section_idx: usize| {
                                    if let Some(song_idx) = song_index_for_section_click {
                                        spawn(async move {
                                            let _ = Session::get().setlist().seek_to_section(song_idx, section_idx).await;
                                        });
                                    }
                                })),
                                on_comment_click: Some(Callback::new({
                                    move |position_seconds: f64| {
                                        if let Some(song_idx) = song_index {
                                            spawn(async move {
                                                tracing::info!(
                                                    "Seeking to comment at {:.2}s in song {}",
                                                    position_seconds,
                                                    song_idx
                                                );
                                                let _ = Session::get()
                                                    .setlist()
                                                    .seek_to_time(song_idx, position_seconds)
                                                    .await;
                                            });
                                        }
                                    }
                                })),
                                tempo_markers: tempo_markers.read().clone(),
                                comment_markers: comment_markers.read().clone(),
                                song_key: song_key.clone(),
                                loop_indicator: loop_indicator,
                                queued_target: queued_target.clone(),
                            }
                        }
                    }

                    // Section Progress Bar
                    div {
                        class: "flex-none",
                        div {
                            class: "w-full",
                            SectionProgressBar {
                                progress: section_progress,
                                sections: progress_sections.read().clone(),
                                measure_indicators: measure_indicators.read().clone(),
                                comment_markers: section_comment_markers.read().clone(),
                                song_key: song_key,
                                on_measure_click: Some(Callback::new({
                                    move |musical_position: daw_proto::MusicalPosition| {
                                        if let Some(song_idx) = song_index {
                                            // The measure field contains the absolute measure number
                                            let measure = musical_position.measure;
                                            spawn(async move {
                                                tracing::info!(
                                                    "Going to measure {} in song {}",
                                                    measure,
                                                    song_idx
                                                );
                                                let _ = Session::get()
                                                    .setlist()
                                                    .goto_measure(song_idx, measure)
                                                    .await;
                                            });
                                        }
                                    }
                                })),
                                on_comment_click: Some(Callback::new({
                                    move |position_seconds: f64| {
                                        if let Some(song_idx) = song_index {
                                            spawn(async move {
                                                tracing::info!(
                                                    "Seeking to comment at {:.2}s in song {}",
                                                    position_seconds,
                                                    song_idx
                                                );
                                                let _ = Session::get()
                                                    .setlist()
                                                    .seek_to_time(song_idx, position_seconds)
                                                    .await;
                                            });
                                        }
                                    }
                                })),
                                queued_target: queued_target,
                            }
                        }
                    }

                    // ── The work ───────────────────────────────────
                    //
                    // Everything the head is not: the take being
                    // reviewed now, the chart beside it when that
                    // lands, whatever else the room needs. It takes
                    // all the height that is left, which is why the
                    // head above is as short as it can be.
                    div {
                        class: "flex-1 min-h-0 flex flex-row gap-4 px-4 pb-4",
                    // ── The take just played ───────────────────
                        //
                        // The space the head stopped taking. Present only
                        // when there IS a pass to review — a panel showing
                        // an invented take would be worse than no panel.
                        if let Some(pass) = crate::signals::TAKE_UNDER_REVIEW() {
                            // Takes its share of the row and all of the
                            // height: a tablet turned over to one take
                            // should show the take at the size of the
                            // screen.
                            div { class: "flex-1 min-w-0 min-h-0",
                                crate::components::TakeReview {
                                    pass,
                                    peaks: crate::signals::TAKE_PEAKS(),
                                    role: crate::signals::ROLE(),
                                    roles: crate::signals::ROLES(),
                                    on_role: move |role: String| {
                                        *crate::signals::ROLE.write() =
                                            (!role.is_empty()).then_some(role);
                                    },
                                    on_mark: move |mark: session_proto::review::Mark| {
                                        // To the service, which keeps it with
                                        // the song and tells the other
                                        // tablets. It comes back through
                                        // `TakeMarked` — including to this
                                        // one, so what is on screen is what
                                        // the room has rather than what this
                                        // device hopes it has.
                                        let pass = crate::signals::TAKE_UNDER_REVIEW()
                                            .map(|pass| pass.number);
                                        let song = ACTIVE_INDICES.read().song_index;
                                        if let (Some(pass), Some(song)) = (pass, song) {
                                            spawn(async move {
                                                if let Err(error) = Session::get()
                                                    .setlist()
                                                    .mark_take(song, pass, mark)
                                                    .await
                                                {
                                                    tracing::warn!(
                                                        error = ?error,
                                                        "the mark did not reach the session"
                                                    );
                                                }
                                            });
                                        }
                                    },
                                }
                            }
                        }

                    }
                }
            }

        }
    }
}
