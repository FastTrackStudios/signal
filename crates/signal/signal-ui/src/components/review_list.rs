//! Review list and review card components.
//!
//! Provides:
//! - [`ReviewCard`] -- a single review with star rating, text excerpt, and author
//! - [`ReviewList`] -- a scrollable list of reviews

use dioxus::prelude::*;

use super::star_rating::StarRating;

// region: --- ReviewCard

/// Data for a single review to display.
#[derive(Clone, PartialEq)]
pub struct ReviewData {
    pub author_name: String,
    pub score: u8,
    pub review_text: Option<String>,
    pub created_at: String,
}

/// Displays a single review with star rating, author, and text excerpt.
#[component]
pub fn ReviewCard(
    review: ReviewData,
    /// Max characters for the review excerpt before truncation.
    #[props(default = 200)]
    max_excerpt_len: usize,
) -> Element {
    let excerpt = review.review_text.as_ref().map(|text| {
        if text.len() > max_excerpt_len {
            format!("{}...", &text[..max_excerpt_len])
        } else {
            text.clone()
        }
    });

    rsx! {
        div { class: "p-3 rounded-lg bg-zinc-800/50 border border-zinc-700/50",
            // Header: stars + author + date
            div { class: "flex items-center justify-between mb-1",
                div { class: "flex items-center gap-2",
                    StarRating { score: review.score }
                    span { class: "text-sm font-medium text-zinc-300",
                        "{review.author_name}"
                    }
                }
                span { class: "text-xs text-zinc-500", "{review.created_at}" }
            }

            // Review text excerpt
            if let Some(excerpt) = &excerpt {
                p { class: "text-sm text-zinc-400 mt-1 leading-relaxed",
                    "{excerpt}"
                }
            }
        }
    }
}

// endregion: --- ReviewCard

// region: --- ReviewList

/// Scrollable list of reviews.
///
/// Shows up to `max_visible` reviews with a count header.
#[component]
pub fn ReviewList(
    /// Reviews to display.
    reviews: Vec<ReviewData>,
    /// Maximum number of reviews to show (0 = show all).
    #[props(default = 5)]
    max_visible: usize,
) -> Element {
    let total = reviews.len();

    if total == 0 {
        return rsx! {
            div { class: "text-sm text-zinc-500 italic py-4 text-center",
                "No reviews yet. Be the first to review!"
            }
        };
    }

    let visible = if max_visible > 0 {
        &reviews[..reviews.len().min(max_visible)]
    } else {
        &reviews[..]
    };

    rsx! {
        div { class: "space-y-2",
            // Header
            div { class: "flex items-center justify-between mb-2",
                span { class: "text-sm font-medium text-zinc-300",
                    "Reviews ({total})"
                }
            }

            // Review cards
            for review in visible.iter() {
                ReviewCard { review: review.clone() }
            }

            // "Show more" hint
            if max_visible > 0 && total > max_visible {
                div { class: "text-xs text-zinc-500 text-center pt-1",
                    "+ {total - max_visible} more reviews"
                }
            }
        }
    }
}

// endregion: --- ReviewList
