//! Section progress bar — shows progress within the current section.
//!
//! Displays tempo markers, time signature changes, measure indicators,
//! and comment markers as overlays on the section timeline.

use crate::components::{
    CommentMarker, MeasureIndicator, ProgressSection, TempoCard, TempoMarkerData, TimeSignatureCard,
};
use crate::prelude::*;
use crate::signals::{ACTIVE_INDICES, SETLIST_STRUCTURE};
use session_proto::QueuedTarget;

/// Section progress bar component
///
/// Shows progress within the current section as a small bar below the main progress bar.
/// Can display tempo markers, time signature markers, measure indicators, and comment markers.
#[component]
pub fn SectionProgressBar(
    /// Current progress percentage (0-100)
    progress: f64,
    sections: Vec<ProgressSection>,
    #[props(default)] tempo_markers: Vec<TempoMarkerData>,
    #[props(default)] measure_indicators: Vec<MeasureIndicator>,
    #[props(default)] comment_markers: Vec<CommentMarker>,
    #[props(default)] song_key: Option<String>,
    #[props(default)] on_measure_click: Option<Callback<daw_proto::MusicalPosition>>,
    #[props(default)] on_comment_click: Option<Callback<f64>>,
    #[props(default)] queued_target: Option<QueuedTarget>,
    /// The bar's height as CSS (`1rem` unless given) — a touchscreen's is
    /// taller, so each measure is a target a finger can hit.
    #[props(default)]
    height: Option<String>,
) -> Element {
    let bar_height = height.unwrap_or_else(|| "1rem".to_owned());
    let current_progress = progress;

    // Track progress changes to detect jumps and disable animations
    let mut prev_progress = use_signal(|| None::<f64>);
    let mut prev_song_key = use_signal(|| None::<String>);

    let song_key_for_memo = song_key.clone();
    let should_animate = use_memo(move || {
        let prev = prev_progress();
        let prev_key = prev_song_key();
        let song_changed = prev_key != song_key_for_memo;
        let large_jump = prev.is_some_and(|prev_val| (current_progress - prev_val).abs() > 5.0);
        !song_changed && !large_jump && prev.is_some()
    });

    let current_progress_for_effect = current_progress;
    let song_key_for_effect = song_key;
    use_effect(move || {
        prev_progress.set(Some(current_progress_for_effect));
        if prev_song_key() != song_key_for_effect {
            prev_song_key.set(song_key_for_effect.clone());
        }
    });

    // Get the active section color from the SETLIST_STRUCTURE signal
    // This ensures we use the actual current section, not calculated from progress
    let base_color = {
        let setlist = SETLIST_STRUCTURE.read();
        let active = ACTIVE_INDICES.read();

        // Get the active song and section
        if let (Some(song_idx), Some(section_idx)) = (active.song_index, active.section_index) {
            setlist
                .songs
                .get(song_idx)
                .and_then(|song| song.sections.get(section_idx))
                .map_or_else(
                    || {
                        // Fallback to first section color if no active section
                        sections
                            .first()
                            .map_or_else(|| "rgb(100, 100, 100)".to_string(), |s| s.color.clone())
                    },
                    session_proto::Section::bright_color,
                )
        } else {
            // Fallback to first section color if no active section
            sections
                .first()
                .map_or_else(|| "rgb(100, 100, 100)".to_string(), |s| s.color.clone())
        }
    };

    // Convert measure indicators into measure sections for clickable/highlightable measures
    let measure_sections: Vec<_> = measure_indicators
        .iter()
        .enumerate()
        .map(|(idx, measure)| {
            let measure_start = measure.position_percent;
            // End position is the next measure's start, or 100% if this is the last measure
            let measure_end = measure_indicators
                .get(idx.saturating_add(1))
                .map_or(100.0, |m| m.position_percent);
            let measure_width = measure_end - measure_start;

            // Calculate how much of this measure is filled
            let filled_percent = if current_progress <= measure_start {
                0.0
            } else if current_progress >= measure_end {
                100.0
            } else {
                let progress_in_measure = current_progress - measure_start;
                (progress_in_measure / measure_width) * 100.0
            };

            (
                idx,
                measure_start,
                measure_end,
                measure_width,
                measure.clone(),
                filled_percent,
            )
        })
        .collect();

    // Find the active measure
    let _active_measure = measure_sections
        .iter()
        .find(|(_, start, end, _, _, _)| current_progress >= *start && current_progress < *end)
        .or_else(|| {
            if let Some((_, start, end, _, _, _)) = measure_sections.last() {
                if current_progress >= *start && current_progress <= *end {
                    measure_sections.last()
                } else {
                    None
                }
            } else {
                None
            }
        });

    // Helper function to check if two markers overlap (same as SongProgressBar)
    let check_overlap = |pos1: f64, pos2: f64| -> bool { (pos1 - pos2).abs() < 2.0 };

    // Pre-calculate positions for time signature markers (all in same row, no staggering)
    // For section progress bar, use -2rem offset (cards closer to bar for compact display)
    let time_sig_with_positions: Vec<_> = {
        tempo_markers
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_time_sig)
            .map(|(orig_idx, marker)| (orig_idx, marker, "-2rem"))
            .collect()
    };

    // Calculate line positions dynamically based on card size
    // Lines should go FROM progress bar UP TO bottom of cards
    // Card top is at -2rem (card_top_offset)
    // For "sm" size cards with horizontal text "4/4":
    //   - text-xs line-height: ~0.75rem
    //   - py-0.5 padding: 0.5rem top + 0.5rem bottom = 1rem total
    //   - border: ~0.125rem (1px top + 1px bottom)
    //   - Total card height: ~1.75rem
    // Card bottom = -2rem + 1.75rem = -0.25rem
    // Line should extend from progress bar (0) to card bottom (-0.25rem)
    // So line height = 0 - (-0.25rem) = 0.25rem
    let card_size = "sm"; // Section progress bar uses small cards
    let card_height_rem = match card_size {
        "xs" => 1.25,  // text-xs + py-0.5 + border
        "md" => 2.125, // text-sm + py-1 + border
        "lg" => 2.625, // text-base + py-1 + border
        _ => 1.75,     // text-xs + py-0.5 + border (horizontal text, more accurate)
    };
    let card_top_offset_rem = 2.0; // Card position in rem (negative, so -2rem)
                                   // Card bottom position: card_top + card_height = -2rem + 1.75rem = -0.25rem
                                   // Line height is distance from progress bar (0) to card bottom (-0.25rem) = 0.25rem
    let card_bottom_rem = -card_top_offset_rem + card_height_rem; // -2 + 1.75 = -0.25
    let line_height_rem = -card_bottom_rem; // 0.25 (distance from 0 to -0.25rem)
    let _line_height = format!("{line_height_rem}rem");

    // Pre-calculate positions for tempo markers with staggering
    let tempo_with_positions: Vec<_> = {
        let tempo_markers_list: Vec<_> = tempo_markers
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_tempo)
            .collect();

        let mut result = Vec::new();
        for (idx, (orig_idx, marker)) in tempo_markers_list.iter().enumerate() {
            // Check if this marker overlaps with previous ones
            let needs_stagger = if idx > 0 {
                tempo_markers_list.get(..idx).is_some_and(|slice| {
                    slice.iter().any(|(_, prev_marker)| {
                        check_overlap(marker.position_percent, prev_marker.position_percent)
                    })
                })
            } else {
                false
            };

            let bottom_offset = if needs_stagger && idx % 2 == 1 {
                "0.75rem" // Lower lane for odd indices when overlapping
            } else {
                "0.5rem" // Default higher lane (closer to progress bar)
            };
            result.push((*orig_idx, *marker, bottom_offset));
        }
        result
    };

    // Check if we have measure indicators to render
    let has_measures = !measure_sections.is_empty();

    // Pre-calculate phrase boundaries (every 4 measures within this section)
    // These are thicker lines that extend above and below the progress bar
    // The grouping resets at the start of each section, so we use the measure index
    // within the section (0-indexed), not the absolute measure number from REAPER
    let phrase_boundaries: Vec<(usize, f64)> = measure_indicators
        .iter()
        .enumerate()
        .filter_map(|(index, _measure)| {
            // index is 0-indexed within this section
            // We want lines after measures 4, 8, 12... (index 3, 7, 11...)
            // So we check if (index + 1) % 4 == 0
            let measure_in_section = index.saturating_add(1); // 1-indexed measure within section
            if measure_in_section % 4 == 0 {
                // Get the end position of this measure (start of next measure, or 100%)
                let next_index = index.saturating_add(1);
                let phrase_line_percent = measure_indicators
                    .get(next_index)
                    .map_or(100.0, |m| m.position_percent);
                // Only include if not at the very end (100%)
                if phrase_line_percent < 99.5 {
                    Some((index, phrase_line_percent))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    rsx! {
        div {
            class: "relative flex flex-col items-center justify-center w-full",
            // Time signature and tempo markers (positioned above and below progress bar)
            // Cards are positioned relative to the outer container
            if !tempo_markers.is_empty() {
                // Time signature markers (above progress bar) - horizontal display for section progress bar
                for (orig_idx, marker, top_offset) in time_sig_with_positions.iter() {
                    TimeSignatureCard {
                        position_percent: marker.position_percent,
                        label: marker.label.clone(),
                        top_offset: top_offset.to_string(),
                        card_key: format!("section-timesig-label-{orig_idx}"),
                        vertical: false,
                        size: "sm".to_string(),
                    }
                }
                // Tempo/BPM markers (below progress bar, with staggering)
                // First marker (at 0%) is left-aligned, others are centered
                for (orig_idx, marker, bottom_offset) in tempo_with_positions.iter() {
                    TempoCard {
                        position_percent: marker.position_percent,
                        label: marker.label.clone(),
                        bottom_offset: bottom_offset.to_string(),
                        card_key: format!("section-tempo-label-{orig_idx}"),
                        card_class: "text-xs font-medium text-center whitespace-nowrap px-1 py-1 rounded bg-accent text-accent-foreground border border-border".to_string(),
                        left_align: marker.position_percent < 0.5, // Left-align first marker at 0%
                    }
                }
            }
            // Comment markers (displayed above section progress bar)
            if !comment_markers.is_empty() {
                for (index, comment) in comment_markers.iter().enumerate() {
                    {
                        // Check if this comment is the queued target
                        let is_queued = matches!(
                            &queued_target,
                            Some(QueuedTarget::Comment { position_seconds, .. })
                                if (*position_seconds - comment.position_seconds).abs() < 0.1
                        );

                        rsx! {
                            div {
                                key: "section-comment-{index}",
                                class: if is_queued {
                                    "absolute z-50 cursor-pointer animate-pulse"
                                } else if on_comment_click.is_some() {
                                    "absolute z-50 cursor-pointer hover:scale-105 transition-transform"
                                } else {
                                    "absolute pointer-events-none z-50"
                                },
                                style: format!(
                                    "left: {}%; transform: translateX(-50%); top: -1.5rem;",
                                    comment.position_percent
                                ),
                                onclick: {
                                    let callback_opt = on_comment_click;
                                    let position_seconds = comment.position_seconds;
                                    move |_| {
                                        if let Some(callback) = &callback_opt {
                                            callback.call(position_seconds);
                                        }
                                    }
                                },
                                div {
                                    class: if is_queued {
                                        "text-[10px] font-bold text-center whitespace-nowrap px-1.5 py-0.5 rounded border ring-2 ring-white ring-opacity-80"
                                    } else if comment.is_count_in {
                                        "text-[10px] font-bold text-center whitespace-nowrap px-1.5 py-0.5 rounded border"
                                    } else {
                                        "text-[10px] font-medium text-center whitespace-nowrap px-1.5 py-0.5 rounded border"
                                    },
                                    style: comment.color.as_ref().map_or_else(
                                        || {
                                            if comment.is_count_in {
                                                "background-color: rgba(251, 191, 36, 0.2); border-color: #fbbf24; color: #fbbf24;".to_string()
                                            } else {
                                                "background-color: var(--color-accent); border-color: var(--color-border); color: var(--color-accent-foreground);".to_string()
                                            }
                                        },
                                        |color| {
                                            format!(
                                                "background-color: {color}20; border-color: {color}; color: {color};"
                                            )
                                        }
                                    ),
                                    "{comment.text}"
                                }
                            }
                        }
                    }
                }
            }
            // Progress bar content (with padding to align with cards)
            div {
                class: "relative w-full rounded-lg overflow-hidden bg-secondary",
                style: "height:{bar_height};",
                // Tempo/Time signature marker lines (inside progress bar for proper positioning)
                // Lines extend from top of progress bar upwards
                if !tempo_markers.is_empty() {
                    for (index, marker) in tempo_markers.iter().enumerate() {
                        div {
                            key: "section-tempo-marker-{index}",
                            class: "absolute pointer-events-none z-40",
                            style: format!(
                                "left: {}%; width: 1px; top: -2rem; bottom: 0; background-color: rgba(255, 255, 255, 0.5); transform: translateX(-50%);",
                                marker.position_percent
                            ),
                        }
                    }
                }
                // Render measure sections if we have them
                if has_measures {
                    // Measure sections (clickable and highlightable)
                    for (measure_idx, measure_start, _measure_end, measure_width, measure, filled_percent) in measure_sections.iter() {
                        div {
                            key: "measure-section-{measure_idx}",
                            class: if on_measure_click.is_some() {
                                "absolute h-full z-10 cursor-pointer transition-all duration-200 hover:brightness-110 hover:ring-2 hover:ring-white hover:ring-opacity-50"
                            } else {
                                "absolute h-full z-0"
                            },
                            style: format!(
                                "left: {}%; width: {}%;",
                                measure_start,
                                measure_width
                            ),
                            onclick: {
                                let callback_opt = on_measure_click;
                                let musical_pos = measure.musical_position;
                                move |_| {
                                    if let Some(callback) = &callback_opt {
                                        callback.call(musical_pos);
                                    }
                                }
                            },
                            // Measure background (full color, will be brightened by progress overlay)
                            div {
                                class: "absolute inset-0 h-full transition-all duration-200",
                                style: format!(
                                    "background-color: {}; opacity: 0.3;",
                                    base_color
                                ),
                            }
                            // Filled portion within this measure
                            div {
                                class: if should_animate() {
                                    "absolute left-0 top-0 h-full transition-all duration-100 ease-linear"
                                } else {
                                    "absolute left-0 top-0 h-full"
                                },
                                style: format!(
                                    "width: {}%; background-color: {}; opacity: 0.8;",
                                    filled_percent,
                                    base_color
                                ),
                            }
                        }
                    }
                    // Measure boundary lines (subtle vertical lines at measure boundaries)
                    for (index, measure) in measure_indicators.iter().enumerate() {
                        if measure.position_percent > 0.0 {
                            div {
                                key: "measure-boundary-{index}",
                                class: "absolute pointer-events-none z-30",
                                style: format!(
                                    "left: {}%; width: 1px; top: 0; bottom: 0; background-color: rgba(255, 255, 255, 0.3); transform: translateX(-50%);",
                                    measure.position_percent
                                ),
                            }
                        }
                    }
                    // Thicker phrase lines every 4 measures (extends above and below the bar)
                    for (index, phrase_percent) in phrase_boundaries.iter() {
                        div {
                            key: "phrase-boundary-{index}",
                            class: "absolute pointer-events-none z-40",
                            style: format!(
                                "left: {}%; width: 2px; top: -0.25rem; bottom: -0.25rem; background-color: rgba(255, 255, 255, 0.6); transform: translateX(-50%);",
                                phrase_percent
                            ),
                        }
                    }
                } else {
                    // Simple progress bar when no measure indicators
                    // Background (muted color)
                    div {
                        class: "absolute inset-0 h-full",
                        style: format!(
                            "background-color: {}; opacity: 0.3;",
                            base_color
                        ),
                    }
                    // Filled progress portion
                    div {
                        class: if should_animate() {
                            "absolute left-0 top-0 h-full transition-all duration-100 ease-linear"
                        } else {
                            "absolute left-0 top-0 h-full"
                        },
                        style: format!(
                            "width: {}%; background-color: {}; opacity: 0.8;",
                            current_progress,
                            base_color
                        ),
                    }
                }
            }
        }
    }
}
