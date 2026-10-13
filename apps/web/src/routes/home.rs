//! The landing page.
//!
//! One screen: the claim ("this is a real setlist, playable, not a
//! screenshot of one") backed by an animated — but NOT backend-driven —
//! showcase of the Praise section timeline, cycling on its own. It reads
//! real chart data (the same [`chart_to_layout`] the `/demo` route stamps
//! into the live backend), so the section names/order aren't fabricated,
//! just not wired to a live transport here — that's what `/demo` is for.

use std::time::Duration;

use dioxus::prelude::*;
use session::setlist::chart_import::chart_to_layout;

use crate::Route;
use crate::praise::PRAISE_CHART;

const STEP: Duration = Duration::from_millis(1600);

#[component]
pub fn Home() -> Element {
    let section_names = use_signal(|| {
        chart_to_layout(PRAISE_CHART)
            .map(|layout| {
                layout
                    .sections
                    .iter()
                    .map(|s| match &s.label {
                        Some(label) => format!("{:?} \u{2014} \u{201c}{label}\u{201d}", s.kind),
                        None => format!("{:?}", s.kind),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    let mut current = use_signal(|| 0usize);

    use_future(move || async move {
        loop {
            architect::platform::sleep(STEP).await;
            let len = section_names.read().len();
            if len == 0 {
                continue;
            }
            current.with_mut(|i| *i = (*i + 1) % len);
        }
    });

    let now_playing = use_memo(move || {
        section_names
            .read()
            .get(current())
            .cloned()
            .unwrap_or_default()
    });

    rsx! {
        div { class: "min-h-screen bg-zinc-950 text-zinc-100 flex items-center justify-center p-8",
            div { class: "grid grid-cols-1 lg:grid-cols-2 gap-12 max-w-6xl w-full items-center",
                div { class: "flex flex-col gap-6",
                    h1 { class: "text-5xl sm:text-6xl font-bold tracking-tight",
                        "Your Setlist, "
                        span { class: "bg-gradient-to-r from-sky-400 to-violet-400 bg-clip-text text-transparent",
                            "Live"
                        }
                    }
                    p { class: "text-lg text-zinc-400 max-w-md",
                        "Session turns a charted set into a real, playable transport \u{2014} \
                        songs, sections, tempo, count-ins \u{2014} the same performance view \
                        the app ships, right here."
                    }
                    div { class: "flex gap-3",
                        Link {
                            to: Route::Demo {},
                            class: "rounded-md bg-sky-500 px-5 py-2.5 font-medium text-zinc-950 hover:bg-sky-400 transition-colors",
                            "Try the live demo"
                        }
                    }
                    p { class: "text-sm text-zinc-500", "Open Source \u{2014} No Account Required" }
                }

                div { class: "rounded-xl border border-zinc-800 bg-zinc-900 overflow-hidden shadow-2xl",
                    div { class: "flex items-center justify-between px-4 py-2.5 border-b border-zinc-800 bg-zinc-900/80",
                        span { class: "text-sm font-medium text-zinc-300", "Praise \u{2014} Elevation Worship" }
                        span { class: "text-xs text-zinc-500 uppercase tracking-wide", "auto-playing preview" }
                    }
                    div { class: "flex flex-col gap-4 p-6",
                        span { class: "text-xs uppercase tracking-wide text-zinc-500", "Now playing" }
                        span { class: "text-2xl font-semibold text-sky-300 min-h-[2em]", "{now_playing}" }
                        div { class: "flex flex-wrap gap-1.5",
                            for (i , name) in section_names.read().iter().enumerate() {
                                span {
                                    key: "{i}",
                                    class: if i == current() {
                                        "h-2.5 w-6 rounded-full bg-sky-400 transition-colors"
                                    } else {
                                        "h-2.5 w-6 rounded-full bg-zinc-700 transition-colors"
                                    },
                                    title: "{name}",
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
