//! Star rating display and input components.
//!
//! Provides:
//! - [`StarRating`] -- read-only star display (filled/empty stars)
//! - [`StarRatingInput`] -- interactive star input with hover preview
//! - [`PresetRatingBadge`] -- compact average rating badge for lists

use dioxus::prelude::*;

// region: --- StarRating (read-only)

/// Displays a star rating as filled/empty star characters.
///
/// Shows 5 stars total, with `score` filled and the rest empty.
#[component]
pub fn StarRating(
    /// Rating value (1-5). 0 shows all empty stars.
    score: u8,
    /// Optional CSS class for the container.
    #[props(default)]
    class: Option<String>,
) -> Element {
    let score = score.min(5);
    let class = class.unwrap_or_default();

    rsx! {
        span { class: "inline-flex items-center gap-0.5 {class}",
            for i in 1..=5u8 {
                span {
                    class: if i <= score { "text-yellow-400" } else { "text-zinc-600" },
                    if i <= score { "\u{2605}" } else { "\u{2606}" }
                }
            }
        }
    }
}

// endregion: --- StarRating

// region: --- StarRatingInput

/// Interactive star rating input with hover preview.
///
/// Users click a star to set their rating. Hover shows preview state.
#[component]
pub fn StarRatingInput(
    /// Current score (1-5, or 0 for unrated).
    score: u8,
    /// Callback when user clicks a star.
    on_rate: EventHandler<u8>,
    /// Whether the input is disabled.
    #[props(default)]
    disabled: bool,
) -> Element {
    let score = score.min(5);
    let mut hover_score = use_signal(|| 0u8);
    let display_score = if hover_score() > 0 {
        hover_score()
    } else {
        score
    };

    rsx! {
        span {
            class: "inline-flex items-center gap-0.5",
            class: if disabled { "opacity-50 cursor-not-allowed" } else { "cursor-pointer" },
            onmouseleave: move |_| hover_score.set(0),
            for i in 1..=5u8 {
                span {
                    class: if i <= display_score { "text-yellow-400 text-lg" } else { "text-zinc-600 text-lg" },
                    class: if !disabled { "hover:scale-110 transition-transform" } else { "" },
                    onmouseenter: {
                        let disabled = disabled;
                        move |_| {
                            if !disabled {
                                hover_score.set(i);
                            }
                        }
                    },
                    onclick: {
                        let disabled = disabled;
                        move |_| {
                            if !disabled {
                                on_rate.call(i);
                            }
                        }
                    },
                    if i <= display_score { "\u{2605}" } else { "\u{2606}" }
                }
            }
        }
    }
}

// endregion: --- StarRatingInput

// region: --- PresetRatingBadge

/// Compact badge showing average rating and count.
///
/// Displays: "★ 4.2 (12)" or "No ratings" when count is 0.
#[component]
pub fn PresetRatingBadge(
    /// Average rating (0.0-5.0).
    average: f64,
    /// Number of ratings.
    count: u64,
) -> Element {
    if count == 0 {
        return rsx! {
            span { class: "text-xs text-zinc-500", "No ratings" }
        };
    }

    rsx! {
        span { class: "inline-flex items-center gap-1 text-xs",
            span { class: "text-yellow-400", "\u{2605}" }
            span { class: "text-zinc-300", "{average:.1}" }
            span { class: "text-zinc-500", "({count})" }
        }
    }
}

// endregion: --- PresetRatingBadge
