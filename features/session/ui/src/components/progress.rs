//! Progress Bar Components
//!
//! Progress visualization components for songs and sections.
//! Copied from `FastTrackStudio` for exact styling match.

use crate::prelude::*;
use session_proto::QueuedTarget;

/// Represents the loop region as percentages (0-100) within the progress bar
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LoopIndicatorData {
    /// Whether looping is enabled
    pub enabled: bool,
    /// Loop start position as percentage (0-100)
    pub start_percent: f64,
    /// Loop end position as percentage (0-100)
    pub end_percent: f64,
}

impl LoopIndicatorData {
    /// Create a new loop indicator
    #[must_use]
    pub const fn new(enabled: bool, start_percent: f64, end_percent: f64) -> Self {
        Self {
            enabled,
            start_percent: start_percent.clamp(0.0, 100.0),
            end_percent: end_percent.clamp(0.0, 100.0),
        }
    }
}

/// Time signature card component
/// Displays a time signature as a fraction
#[component]
pub fn TimeSignatureCard(
    position_percent: f64,
    label: String,
    top_offset: String,
    card_key: String,
    #[props(default = true)] vertical: bool,
    #[props(default = "sm".to_string())] size: String,
) -> Element {
    // Parse time signature for fraction display
    let (numerator, denominator) = label.find('/').map_or_else(
        || (label.clone(), String::new()),
        |slash_pos| {
            (
                label.get(..slash_pos).unwrap_or("").trim().to_string(),
                label
                    .get(slash_pos.saturating_add(1)..)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            )
        },
    );

    // Size presets
    let (text_size, padding, spacing) = match size.as_str() {
        "xs" => ("text-xs", "px-0.5 py-0.5", "my-0.5"),
        "md" => ("text-sm", "px-1 py-1", "my-0.5"),
        "lg" => ("text-base", "px-2 py-1", "my-1"),
        _ => ("text-xs", "px-1 py-0.5", "my-0.5"),
    };

    let card_class = format!(
        "{text_size} font-medium text-center whitespace-nowrap {padding} rounded bg-accent text-accent-foreground border border-border"
    );
    let divider_class = format!("border-t border-current w-full {spacing}");

    rsx! {
        div {
            key: "{card_key}",
            class: "absolute pointer-events-none",
            style: format!("left: {}%; transform: translateX(-50%); top: {};", position_percent, top_offset),
            div {
                class: "{card_class}",
                if vertical && !denominator.is_empty() {
                    div {
                        class: "flex flex-col items-center leading-tight",
                        span { "{numerator}" }
                        span { class: "{divider_class}" }
                        span { "{denominator}" }
                    }
                } else if !denominator.is_empty() {
                    "{numerator}/{denominator}"
                } else {
                    "{label}"
                }
            }
        }
    }
}

/// Tempo/BPM card component
#[component]
pub fn TempoCard(
    position_percent: f64,
    label: String,
    bottom_offset: String,
    card_key: String,
    #[props(default = "text-xs".to_string())] text_size: String,
    #[props(default = "px-2 py-0.5 rounded text-xs font-medium text-white bg-black/70 whitespace-nowrap".to_string())]
    card_class: String,
    #[props(default = false)] position_above: bool,
    #[props(default = false)] left_align: bool,
) -> Element {
    let transform = if left_align {
        ""
    } else {
        "transform: translateX(-50%);"
    };
    let style = if position_above {
        format!("left: {position_percent}%; {transform} top: {bottom_offset};")
    } else {
        format!("left: {position_percent}%; {transform} top: calc(100% + {bottom_offset});")
    };

    let adjusted_card_class = if left_align && card_class.contains("text-center") {
        card_class.replace("text-center", "text-left")
    } else {
        card_class
    };

    rsx! {
        div {
            key: "{card_key}",
            class: "absolute pointer-events-none",
            style: style,
            div {
                class: "{adjusted_card_class}",
                style: if adjusted_card_class.contains("bg-black/70") {
                    "text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);"
                } else {
                    ""
                },
                "{label}"
            }
        }
    }
}

/// Progress bar section definition
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressSection {
    pub start_percent: f64,
    pub end_percent: f64,
    pub color: String,
    pub name: String,
    /// Short name for space-constrained display (e.g., "INT A" instead of "Interlude A")
    pub short_name: String,
    /// Optional comment/descriptor (e.g., "Woodwinds" from 'Interlude C "Woodwinds"')
    pub comment: Option<String>,
}

/// Tempo/time signature marker definition
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoMarker {
    pub position_percent: f64,
    pub is_tempo: bool,
    pub is_time_sig: bool,
    pub show_line_only: bool,
}

/// Full tempo marker with label (for passing to components)
#[derive(Clone, Debug, PartialEq)]
pub struct TempoMarkerData {
    pub position_percent: f64,
    pub label: String,
    pub is_tempo: bool,
    pub is_time_sig: bool,
    pub show_line_only: bool,
}

/// Measure indicator definition for section progress bar
#[derive(Clone, Debug, PartialEq)]
pub struct MeasureIndicator {
    pub position_percent: f64,
    pub measure_number: i32,
    pub time_signature: Option<(u8, u8)>,
    pub musical_position: daw_proto::MusicalPosition,
}

/// Comment marker for displaying notes/reminders above the progress bar
#[derive(Clone, Debug, PartialEq)]
pub struct CommentMarker {
    /// Position as percentage (0-100) within the progress bar
    pub position_percent: f64,
    /// Comment text to display
    pub text: String,
    /// Optional color (CSS hex string like "#ff0000")
    pub color: Option<String>,
    /// Whether this is a count-in marker (special styling)
    pub is_count_in: bool,
    /// Whether this comment should only appear in the section progress bar
    pub section_only: bool,
    /// Position in seconds (for seeking when clicked)
    pub position_seconds: f64,
}

/// Segmented progress bar component with different colored sections
#[component]
pub fn SegmentedProgressBar(
    /// Current progress percentage (0-100)
    progress: f64,
    sections: Vec<ProgressSection>,
    #[props(default)] tempo_markers: Vec<TempoMarkerData>,
    #[props(default)] comment_markers: Vec<CommentMarker>,
    #[props(default)] song_key: Option<String>,
    #[props(default)] on_section_click: Option<Callback<usize>>,
    #[props(default)] on_comment_click: Option<Callback<f64>>,
    #[props(default)] loop_indicator: Option<LoopIndicatorData>,
    #[props(default)] queued_target: Option<QueuedTarget>,
    /// The bar's height as CSS (`5rem` unless given) — a phone's is slim.
    #[props(default)]
    height: Option<String>,
    /// Whether each section is named on the bar — off where it is too
    /// narrow to read (a phone held upright): the colours say enough.
    #[props(default = true)]
    labels: bool,
    /// Square-cornered, to run edge to edge in a bar (a phone's).
    #[props(default)]
    flat: bool,
) -> Element {
    let bar_height = height.unwrap_or_else(|| "5rem".to_owned());
    let corners = if flat { "" } else { "rounded-lg" };
    // Text width estimation constants (in pixels):
    // We use generous estimates since text will truncate with ellipsis if needed.
    // Better to show text that gets truncated than hide it entirely.
    //
    // - Assume progress bar is ~1200px wide (common desktop width)
    // - Use smaller char width estimates to be more lenient
    // - Minimal padding assumption
    const ESTIMATED_BAR_WIDTH_PX: f64 = 1200.0;
    const CHAR_WIDTH_NAME_PX: f64 = 5.5; // text-xs - lenient estimate
    const CHAR_WIDTH_COMMENT_PX: f64 = 4.5; // text-[10px] - lenient estimate
    const SECTION_PADDING_PX: f64 = 8.0; // minimal padding

    // Helper function to check if two markers overlap
    fn check_overlap(pos1: f64, pos2: f64) -> bool {
        (pos1 - pos2).abs() < 2.0
    }

    let current_progress = progress;

    // Track progress changes to detect jumps and disable animations
    let mut prev_progress = use_signal(|| None::<f64>);
    let mut prev_song_key = use_signal(|| None::<String>);

    let song_key_for_memo = song_key.clone();
    let should_animate = use_memo(move || {
        let prev = prev_progress();
        let prev_key = prev_song_key();
        let song_changed = prev_key != song_key_for_memo;
        let large_jump = prev.map_or(false, |prev_val| (current_progress - prev_val).abs() > 5.0);
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

    // Pre-calculate STATIC section data with intelligent text fitting
    // This is memoized because it only depends on sections prop, not progress

    // Compute static section data from props (text fitting, display names).
    // Not memoized — recomputes each render so prop changes are always reflected.
    let static_section_data: Vec<_> = sections
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let section_start = section.start_percent;
            let section_end = section.end_percent;
            let section_width_percent = section.end_percent - section.start_percent;

            // Estimate available pixel width for this section
            let section_width_px = (section_width_percent / 100.0)
                .mul_add(ESTIMATED_BAR_WIDTH_PX, -SECTION_PADDING_PX);

            // Calculate text widths
            let name_width_px =
                f64::from(u32::try_from(section.name.len()).unwrap_or(0)) * CHAR_WIDTH_NAME_PX;
            let short_name_width_px =
                f64::from(u32::try_from(section.short_name.len()).unwrap_or(0))
                    * CHAR_WIDTH_NAME_PX;

            // Decide which name to display based on available width
            let display_name = if name_width_px <= section_width_px {
                section.name.clone()
            } else if short_name_width_px <= section_width_px {
                section.short_name.clone()
            } else {
                // Too narrow for even the short name — an illegible
                // micro-label is noise; the current-section readout above
                // the bar (and the segment's hover title) carry the name.
                String::new()
            };

            // Show comment if it fits within the section width
            let comment = section.comment.as_ref().and_then(|comment_text| {
                let comment_width_px = f64::from(u32::try_from(comment_text.len()).unwrap_or(0))
                    * CHAR_WIDTH_COMMENT_PX;
                if comment_width_px <= section_width_px {
                    Some(comment_text.clone())
                } else {
                    None
                }
            });

            (
                index,
                section_start,
                section_end,
                section_width_percent,
                section.color.clone(),
                display_name,
                comment,
            )
        })
        .collect();

    // Calculate dynamic filled percentages per-frame
    let section_data: Vec<_> = static_section_data
        .iter()
        .map(
            |(
                index,
                section_start,
                section_end,
                section_width_percent,
                color,
                display_name,
                comment,
            )| {
                let filled_percent = if current_progress <= *section_start {
                    0.0
                } else if current_progress >= *section_end {
                    100.0
                } else {
                    let progress_in_section = current_progress - section_start;
                    (progress_in_section / section_width_percent) * 100.0
                };

                (
                    *index,
                    *section_start,
                    *section_end,
                    *section_width_percent,
                    color.clone(),
                    filled_percent,
                    display_name.clone(),
                    comment.clone(),
                )
            },
        )
        .collect();

    // Calculate card positions - these are constants
    let card_size = "sm";
    let card_height_rem = match card_size {
        "xs" => 2.5,
        "md" => 4.0,
        "lg" => 5.0,
        _ => 3.5,
    };

    let line_end_rem = -0.75;
    let line_height = format!("calc(5rem + {}rem)", -line_end_rem);
    let card_top_offset_rem = line_end_rem - card_height_rem;
    let card_top_offset_str = format!("{card_top_offset_rem}rem");

    // Memoize marker positions - only recalculate when tempo_markers changes
    let tempo_markers_for_memo = tempo_markers.clone();
    let card_top_offset_for_memo = card_top_offset_str;
    let time_sig_with_positions = use_memo(move || {
        tempo_markers_for_memo
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_time_sig && !m.show_line_only)
            .map(|(orig_idx, marker)| (orig_idx, marker.clone(), card_top_offset_for_memo.clone()))
            .collect::<Vec<_>>()
    });

    // Memoize filtered time signature markers
    let tempo_markers_for_filtered = tempo_markers.clone();
    let filtered_time_sig_markers = use_memo(move || {
        tempo_markers_for_filtered
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_time_sig && m.show_line_only)
            .map(|(idx, m)| (idx, m.clone()))
            .collect::<Vec<_>>()
    });

    // Memoize tempo marker positions with staggering
    let tempo_markers_for_tempo = tempo_markers.clone();
    let tempo_with_positions = use_memo(move || {
        let tempo_markers_list: Vec<_> = tempo_markers_for_tempo
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_tempo)
            .collect();

        let mut result = Vec::new();
        for (idx, (orig_idx, marker)) in tempo_markers_list.iter().enumerate() {
            let needs_stagger = if idx > 0 {
                tempo_markers_list.iter().take(idx).any(|(_, prev_marker)| {
                    check_overlap(marker.position_percent, prev_marker.position_percent)
                })
            } else {
                false
            };

            let bottom_offset = if needs_stagger && idx % 2 == 1 {
                "0.75rem"
            } else {
                "0.5rem"
            };
            result.push((*orig_idx, (*marker).clone(), bottom_offset));
        }
        result
    });

    rsx! {
        div {
            class: "relative flex flex-col items-center justify-center h-20 w-full",
            style: "height: {bar_height};",
            // Tempo/Time Signature labels
            if !tempo_markers.is_empty() {
                // Time signature markers
                for (orig_idx, marker, top_offset) in time_sig_with_positions.read().iter() {
                    TimeSignatureCard {
                        position_percent: marker.position_percent,
                        label: marker.label.clone(),
                        top_offset: top_offset.clone(),
                        card_key: format!("timesig-label-{orig_idx}"),
                        size: "sm".to_string(),
                    }
                }
                // Tempo/BPM markers
                for (orig_idx, marker, bottom_offset) in tempo_with_positions.read().iter() {
                    TempoCard {
                        position_percent: marker.position_percent,
                        label: marker.label.clone(),
                        bottom_offset: bottom_offset.to_string(),
                        card_key: format!("tempo-label-{orig_idx}"),
                        card_class: "text-xs font-medium text-center whitespace-nowrap px-1 py-1 rounded bg-accent text-accent-foreground border border-border".to_string(),
                        left_align: marker.position_percent < 0.5,
                    }
                }
                // Small lines for filtered time signature markers
                for (index, marker) in filtered_time_sig_markers.read().iter() {
                    if marker.position_percent > 0.5 {
                        div {
                            key: "filtered-timesig-line-{index}",
                            class: "absolute pointer-events-none z-50",
                            style: format!(
                                "left: {}%; width: 1px; top: 0; height: 0.5rem; background-color: rgba(255, 255, 255, 0.5); transform: translateX(-50%);",
                                marker.position_percent
                            ),
                        }
                    }
                }
                // Tempo/Time Signature marker lines
                for (index, marker) in tempo_markers.iter().enumerate() {
                    if marker.position_percent >= 0.5 {
                        if marker.is_time_sig && !marker.show_line_only {
                            div {
                                key: "tempo-marker-{index}",
                                class: "absolute pointer-events-none z-50",
                                style: format!(
                                    "left: {}%; width: 1px; bottom: 0; height: {}; background-color: rgba(255, 255, 255, 0.7); transform: translateX(-50%);",
                                    marker.position_percent,
                                    line_height
                                ),
                            }
                        } else if marker.is_tempo {
                            div {
                                key: "tempo-marker-{index}",
                                class: "absolute pointer-events-none z-50",
                                style: format!(
                                    "left: {}%; width: 1px; top: 5rem; height: 2rem; background-color: rgba(255, 255, 255, 0.7); transform: translateX(-50%);",
                                    marker.position_percent
                                ),
                            }
                        }
                    }
                }
            }
            // Comment markers (displayed above progress bar, similar to time signature cards)
            // Filter out section_only comments - those only appear in SectionProgressBar
            if !comment_markers.is_empty() {
                for (index, comment) in comment_markers.iter().enumerate().filter(|(_, c)| !c.section_only) {
                    {
                        // Check if this comment is the queued target
                        let is_queued = matches!(
                            &queued_target,
                            Some(QueuedTarget::Comment { position_seconds, .. })
                                if (*position_seconds - comment.position_seconds).abs() < 0.1
                        );

                        rsx! {
                            div {
                                key: "comment-{index}",
                                class: if is_queued {
                                    "absolute z-50 cursor-pointer animate-pulse"
                                } else if on_comment_click.is_some() {
                                    "absolute z-50 cursor-pointer hover:scale-105 transition-transform"
                                } else {
                                    "absolute pointer-events-none z-50"
                                },
                                style: format!(
                                    "left: {}%; transform: translateX(-50%); top: -2.5rem;",
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
                                        "text-xs font-bold text-center whitespace-nowrap px-2 py-1 rounded border-2 ring-2 ring-white ring-opacity-80"
                                    } else if comment.is_count_in {
                                        "text-xs font-bold text-center whitespace-nowrap px-2 py-1 rounded border-2"
                                    } else {
                                        "text-xs font-medium text-center whitespace-nowrap px-2 py-1 rounded border"
                                    },
                                    style: comment.color.as_ref().map_or_else(
                                        || {
                                            if comment.is_count_in {
                                                "background-color: rgba(251, 191, 36, 0.2); border-color: #fbbf24; color: #fbbf24;".to_string()
                                            } else {
                                                "background-color: var(--color-accent); border-color: var(--color-border); color: var(--color-accent-foreground);".to_string()
                                            }
                                        },
                                        |color| format!("background-color: {color}20; border-color: {color}; color: {color};"),
                                    ),
                                    "{comment.text}"
                                }
                            }
                        }
                    }
                    // Vertical line from comment to progress bar
                    div {
                        class: "absolute pointer-events-none z-40",
                        style: comment.color.as_ref().map_or_else(
                            || {
                                if comment.is_count_in {
                                    format!(
                                        "left: {}%; width: 1px; top: -0.75rem; height: 0.75rem; background-color: rgba(251, 191, 36, 0.5); transform: translateX(-50%);",
                                        comment.position_percent
                                    )
                                } else {
                                    format!(
                                        "left: {}%; width: 1px; top: -0.75rem; height: 0.75rem; background-color: rgba(255, 255, 255, 0.5); transform: translateX(-50%);",
                                        comment.position_percent
                                    )
                                }
                            },
                            |color| {
                                format!(
                                    "left: {}%; width: 1px; top: -0.75rem; height: 0.75rem; background-color: {}80; transform: translateX(-50%);",
                                    comment.position_percent, color
                                )
                            },
                        ),
                    }
                }
            }
            // Main segmented progress bar
            div {
                                class: "relative w-full h-20 {corners} overflow-hidden bg-secondary",
                style: "height: {bar_height};",
                // Render sections as background layers
                for (index, section_start, _section_end, section_width, section_color, _filled_percent, _section_name, _section_comment) in section_data.iter() {
                    {
                        // Check if this section is the queued target
                        let is_queued = matches!(
                            &queued_target,
                            Some(QueuedTarget::Section { section_index, .. }) if *section_index == *index
                        );

                        rsx! {
                            div {
                                key: "{index}",
                                class: if is_queued {
                                    "absolute h-full z-0 cursor-pointer animate-pulse"
                                } else if on_section_click.is_some() {
                                    "absolute h-full z-0 cursor-pointer transition-all duration-200 hover:brightness-110 hover:ring-2 hover:ring-white hover:ring-opacity-50"
                                } else {
                                    "absolute h-full z-0"
                                },
                                style: format!("left: {}%; width: {}%;", section_start, section_width),
                                // Hover reveals the full name even when the
                                // segment is too narrow for a legible label.
                                title: "{sections.get(*index).map(|s| s.name.as_str()).unwrap_or_default()}",
                                onclick: {
                                    let callback_opt = on_section_click;
                                    let idx = *index;
                                    move |_| {
                                        if let Some(callback) = &callback_opt {
                                            callback.call(idx);
                                        }
                                    }
                                },
                                div {
                                    class: if is_queued {
                                        "absolute inset-0 h-full transition-all duration-200 ring-2 ring-white ring-opacity-80"
                                    } else {
                                        "absolute inset-0 h-full transition-all duration-200"
                                    },
                                    style: format!("background-color: {};", section_color),
                                }
                            }
                        }
                    }
                }
                // Section boundary lines
                for (index, section_start, _section_end, _section_width, _section_color, _filled_percent, _section_name, _section_comment) in section_data.iter() {
                    if *section_start > 0.0 {
                        div {
                            key: "section-boundary-{index}",
                            class: "absolute pointer-events-none z-20",
                            style: format!(
                                "left: {}%; width: 1px; top: 0; bottom: 0; background-color: rgba(255, 255, 255, 0.15);",
                                section_start
                            ),
                        }
                    }
                }
                // Dark overlay covering unfilled portion
                div {
                    class: if should_animate() {
                        "absolute left-0 top-0 h-full transition-all duration-100 ease-linear z-10 pointer-events-none"
                    } else {
                        "absolute left-0 top-0 h-full z-10 pointer-events-none"
                    },
                    style: {
                        let progress = current_progress.clamp(0.0, 100.0);
                        format!(
                            "left: {}%; width: {}%; background-color: rgba(0, 0, 0, 0.6); border-top-right-radius: 0.5rem; border-bottom-right-radius: 0.5rem;",
                            progress,
                            100.0 - progress
                        )
                    },
                }
                // Section name text (with optional comment below)
                for (index, section_start, _section_end, section_width, _section_color, _filled_percent, section_name, section_comment) in section_data.iter() {
                    if labels && !section_name.is_empty() {
                        div {
                            key: "text-{index}",
                            class: "absolute h-full pointer-events-none z-30 overflow-hidden",
                            style: format!("left: {}%; width: {}%;", section_start, section_width),
                            // Stack section name and optional comment vertically
                            div {
                                class: "absolute inset-0 flex flex-col items-center justify-center gap-0.5 overflow-hidden",
                                style: "text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);",
                                // Section name
                                span {
                                    class: "text-xs font-medium text-white truncate max-w-full",
                                    "{section_name}"
                                }
                                // Comment (smaller, lighter text below)
                                if let Some(comment) = section_comment {
                                    span {
                                        class: "text-[10px] text-white/70 italic truncate max-w-full",
                                        "{comment}"
                                    }
                                }
                            }
                        }
                    }
                }
                // Loop indicator overlay and markers
                if let Some(ref loop_ind) = loop_indicator {
                    if loop_ind.enabled {
                        // Yellow tinted overlay for loop region
                        div {
                            class: "absolute top-0 bottom-0 pointer-events-none z-40",
                            style: format!(
                                "left: {}%; width: {}%; background-color: rgba(250, 204, 21, 0.15); border-top: 2px solid #facc15; border-bottom: 2px solid #facc15;",
                                loop_ind.start_percent,
                                loop_ind.end_percent - loop_ind.start_percent
                            ),
                        }
                        // Start marker - yellow triangle pointing down
                        div {
                            class: "absolute pointer-events-none z-50",
                            style: format!("left: {}%; top: 0; transform: translateX(-50%);", loop_ind.start_percent),
                            div {
                                style: "width: 0; height: 0; border-left: 6px solid transparent; border-right: 6px solid transparent; border-top: 8px solid #facc15;",
                            }
                        }
                        // End marker - yellow triangle pointing down
                        div {
                            class: "absolute pointer-events-none z-50",
                            style: format!("left: {}%; top: 0; transform: translateX(-50%);", loop_ind.end_percent),
                            div {
                                style: "width: 0; height: 0; border-left: 6px solid transparent; border-right: 6px solid transparent; border-top: 8px solid #facc15;",
                            }
                        }
                        // Bottom triangles (pointing up)
                        div {
                            class: "absolute pointer-events-none z-50",
                            style: format!("left: {}%; bottom: 0; transform: translateX(-50%);", loop_ind.start_percent),
                            div {
                                style: "width: 0; height: 0; border-left: 6px solid transparent; border-right: 6px solid transparent; border-bottom: 8px solid #facc15;",
                            }
                        }
                        div {
                            class: "absolute pointer-events-none z-50",
                            style: format!("left: {}%; bottom: 0; transform: translateX(-50%);", loop_ind.end_percent),
                            div {
                                style: "width: 0; height: 0; border-left: 6px solid transparent; border-right: 6px solid transparent; border-bottom: 8px solid #facc15;",
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Song progress bar component using segmented progress bar
///
/// This is a wrapper around `SegmentedProgressBar` that handles song-specific logic.
#[component]
pub fn SongProgressBar(
    /// Current progress percentage (0-100)
    progress: f64,
    sections: Vec<ProgressSection>,
    #[props(default)] on_section_click: Option<Callback<usize>>,
    #[props(default)] on_comment_click: Option<Callback<f64>>,
    #[props(default)] tempo_markers: Vec<TempoMarkerData>,
    #[props(default)] comment_markers: Vec<CommentMarker>,
    #[props(default)] song_key: Option<String>,
    #[props(default)] loop_indicator: Option<LoopIndicatorData>,
    #[props(default)] queued_target: Option<QueuedTarget>,
    /// The bar's height as CSS (see [`SegmentedProgressBar`]).
    #[props(default)]
    height: Option<String>,
    /// Whether the sections are named (see [`SegmentedProgressBar`]).
    #[props(default = true)]
    labels: bool,
    /// Square-cornered (see [`SegmentedProgressBar`]).
    #[props(default)]
    flat: bool,
) -> Element {
    rsx! {
        div {
            class: "w-full relative",
            SegmentedProgressBar {
                height: height,
                labels: labels,
                flat: flat,
                progress: progress,
                sections: sections,
                tempo_markers: tempo_markers,
                comment_markers: comment_markers,
                song_key: song_key,
                on_section_click: on_section_click,
                on_comment_click: on_comment_click,
                loop_indicator: loop_indicator,
                queued_target: queued_target,
            }
        }
    }
}
