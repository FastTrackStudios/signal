//! Lyric Sync View
//!
//! A presentational, editable lyric-timing editor. Renders each aligned word
//! as a chip coloured by its confidence rating (Verified / Review / Failed),
//! lets the user select a word and nudge its start time. Domain-agnostic: it
//! takes a flat `Vec<SyncWord>` (produced by keyflow-sync's forced alignment)
//! and reports edits via `on_nudge(index, delta_seconds)`. The app owns the
//! `TimingMap` sidecar and applies the nudges.

use crate::prelude::*;

/// Confidence tier for an aligned word (mirrors `keyflow_sync::timing::Rating`,
/// kept local so session-ui stays independent of the sync crate).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyncRating {
    Verified,
    Review,
    Failed,
}

/// One aligned lyric word on the timeline.
#[derive(Clone, PartialEq)]
pub struct SyncWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub confidence: f32,
    pub rating: SyncRating,
}

impl SyncRating {
    /// (border, background, label) styling per tier.
    const fn palette(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Verified => ("#16a34a", "rgba(22,163,74,0.16)", "Verified"),
            Self::Review => ("#d97706", "rgba(217,119,6,0.16)", "Review"),
            Self::Failed => ("#dc2626", "rgba(220,38,38,0.14)", "Failed"),
        }
    }
}

fn fmt_time(t: f64) -> String {
    let minutes_f = (t / 60.0).floor();
    let m = if minutes_f.is_finite() && minutes_f >= 0.0 {
        minutes_f
    } else {
        0.0
    };
    let s = t - m * 60.0;
    format!("{m:.0}:{s:05.2}")
}

/// An editable lyric-timing view bound to a word list.
#[component]
pub fn LyricSyncView(
    words: Vec<SyncWord>,
    /// `(word_index, delta_seconds)` — shift a word's start by delta.
    on_nudge: Callback<(usize, f64)>,
) -> Element {
    let mut selected = use_signal(|| None::<usize>);

    let (v, r, f) = words
        .iter()
        .fold((0usize, 0usize, 0usize), |(v, r, f), w| match w.rating {
            SyncRating::Verified => (v.saturating_add(1), r, f),
            SyncRating::Review => (v, r.saturating_add(1), f),
            SyncRating::Failed => (v, r, f.saturating_add(1)),
        });
    let total = words.len().max(1);
    let sel = selected().and_then(|i| words.get(i).cloned().map(|w| (i, w)));

    rsx! {
        div {
            style: "flex:1; min-height:0; display:flex; flex-direction:column; background:var(--background,#0b0b0d); color:#e4e4e7;",

            // ── Header: rating summary + nudge controls for the selected word ──
            div {
                style: "display:flex; align-items:center; gap:16px; padding:10px 16px; border-bottom:1px solid #27272a; flex-wrap:wrap;",
                span { style: "font-weight:600; font-size:13px;", "Lyric Sync" }
                span { style: "font-size:12px; color:#16a34a;", "● {v} verified" }
                span { style: "font-size:12px; color:#d97706;", "● {r} review" }
                span { style: "font-size:12px; color:#dc2626;", "● {f} failed" }
                span { style: "font-size:11px; color:#71717a;",
                    "{100 * v / total}% verified of {words.len()} words"
                }

                div { style: "flex:1;" }

                if let Some((i, w)) = sel {
                    span { style: "font-size:12px; color:#a1a1aa;",
                        "“{w.text}” @ {fmt_time(w.start)}"
                    }
                    NudgeBtn { label: "−0.10", on_click: move |_| on_nudge.call((i, -0.10)) }
                    NudgeBtn { label: "−0.02", on_click: move |_| on_nudge.call((i, -0.02)) }
                    NudgeBtn { label: "+0.02", on_click: move |_| on_nudge.call((i, 0.02)) }
                    NudgeBtn { label: "+0.10", on_click: move |_| on_nudge.call((i, 0.10)) }
                } else {
                    span { style: "font-size:11px; color:#71717a;", "select a word to nudge its timing" }
                }
            }

            // ── Word flow ──
            div {
                style: "flex:1; min-height:0; overflow-y:auto; padding:16px; display:flex; flex-wrap:wrap; gap:6px; align-content:flex-start;",
                for (i, w) in words.iter().cloned().enumerate() {
                    {
                        let (border, bg, _) = w.rating.palette();
                        let is_sel = selected() == Some(i);
                        let ring = if is_sel { "box-shadow:0 0 0 2px #6366f1;" } else { "" };
                        rsx! {
                            button {
                                key: "{i}",
                                onclick: move |_| selected.set(Some(i)),
                                title: "{fmt_time(w.start)}–{fmt_time(w.end)}  conf {w.confidence:.2}",
                                style: "display:inline-flex; flex-direction:column; align-items:center; gap:1px; \
                                        padding:3px 8px; border-radius:6px; border:1px solid {border}; background:{bg}; \
                                        color:#f4f4f5; font-size:13px; cursor:pointer; {ring}",
                                span { "{w.text}" }
                                span { style: "font-size:9px; color:#a1a1aa; font-variant-numeric:tabular-nums;",
                                    "{w.start:.1}s"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NudgeBtn(label: String, on_click: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            onclick: move |e| on_click.call(e),
            style: "padding:3px 8px; border-radius:5px; border:1px solid #3f3f46; background:#18181b; \
                    color:#e4e4e7; font-size:12px; font-variant-numeric:tabular-nums; cursor:pointer;",
            "{label}"
        }
    }
}
