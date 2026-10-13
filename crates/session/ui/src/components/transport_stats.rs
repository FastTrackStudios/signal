//! Where the transport is, as four instruments rather than four pills.
//!
//! These were identical rounded badges: `1.4.500`, `0:34.128`,
//! `128 BPM`, `4/4`. Identical is the problem — they answer four
//! different questions and one of them is the question you are actually
//! asking at any moment, and a row of matching pills makes you read all
//! four to find it.
//!
//! So each is shaped like what it is:
//!
//! - **The bar** is the number you call out, so it is the big one, and
//!   the beat under it is dots rather than a digit — you count dots at
//!   a glance and you read a digit one at a time.
//! - **The tempo** is a pulse, not a quantity: the number sits beside a
//!   dot that brightens on every beat, so a glance says both how fast
//!   and whether anything is moving.
//! - **The meter** is stacked over a rule, the way it is written on a
//!   chart — because that is the shape a musician already reads.
//! - **The clock** is the one nobody performs by, so it is quiet:
//!   monospace, dim, and out of the way until you want it.
//!
//! Everything here is legible at arm's length on a stand, which is the
//! distance this is read from.

use daw_proto::MusicalPosition;

use crate::prelude::*;

/// The most beats a bar can show as dots.
///
/// Past this the dots stop being countable and become a texture — and a
/// meter this large is a notation choice rather than a pulse anybody is
/// following, so the number is shown instead.
pub const MOST_DOTS: usize = 12;

/// Which beat dots are lit for a position in the bar.
///
/// Every beat up to and including the current one, so the bar fills as
/// it is played — a row that empties and refills per beat reads as a
/// blink, where a row that fills reads as progress.
#[must_use]
pub fn dots(beats_per_bar: i32, beat: i32) -> Vec<bool> {
    let count = usize::try_from(beats_per_bar)
        .unwrap_or(4)
        .clamp(1, MOST_DOTS);
    let beat = beat.max(1);
    let lit = usize::try_from(beat).unwrap_or(1).min(count);
    (0..count).map(|i| i < lit).collect()
}

/// How brightly the pulse shows, through a beat.
///
/// Brightest on the beat and decaying across it — the shape of a click,
/// which is what it is standing in for. Never all the way out, because
/// a dot that vanishes reads as something broken rather than something
/// between beats.
#[must_use]
pub fn pulse(subdivision: i32) -> f64 {
    let through = f64::from(subdivision.clamp(0, 999)) / 999.0;
    through.mul_add(-0.65, 1.0)
}

/// `m:ss.mmm`, the way a stopwatch says it.
#[must_use]
pub fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let minutes = (seconds / 60.0).floor();
    let rest = minutes.mul_add(-60.0, seconds);
    format!("{minutes:.0}:{rest:06.3}")
}

/// The transport, as four instruments.
#[component]
pub fn TransportStats(
    /// Where the transport is musically, if the backend knows.
    musical: Option<MusicalPosition>,
    /// Where it is in seconds.
    seconds: f64,
    bpm: f64,
    beats_per_bar: i32,
    beat_unit: i32,
    #[props(default)] looping: bool,
) -> Element {
    let position = musical.unwrap_or(MusicalPosition {
        measure: 1,
        beat: 1,
        subdivision: 0,
    });
    let lit = dots(beats_per_bar, position.beat);
    let glow = pulse(position.subdivision);

    rsx! {
        div { class: "flex items-stretch gap-5",

            // ── The bar, and the beat inside it ────────────────────
            div {
                class: "flex flex-col items-end gap-1",
                "data-testid": "stat-bar",
                div { class: "flex items-baseline gap-1.5",
                    span { class: "text-[10px] uppercase tracking-widest text-muted-foreground", "bar" }
                    span {
                        class: "text-3xl font-bold tabular-nums leading-none",
                        "data-testid": "stat-measure",
                        "{position.measure}"
                    }
                }
                div { class: "flex items-center gap-1",
                    for (index, on) in lit.iter().enumerate() {
                        div {
                            class: if *on {
                                "h-1.5 w-1.5 rounded-full bg-primary"
                            } else {
                                "h-1.5 w-1.5 rounded-full bg-muted-foreground/30"
                            },
                            // The beat just struck holds the pulse, so
                            // the row shows WHERE the bar is and that it
                            // is moving in the same glance.
                            style: if *on && index + 1 == lit.iter().filter(|l| **l).count() {
                                format!("opacity: {glow:.2};")
                            } else {
                                String::new()
                            },
                        }
                    }
                }
            }

            Divider {}

            // ── The tempo, as a pulse ─────────────────────────────
            div {
                class: "flex flex-col items-end gap-1",
                "data-testid": "stat-tempo",
                div { class: "flex items-center gap-2",
                    div {
                        class: "h-2.5 w-2.5 rounded-full bg-primary",
                        style: "opacity: {glow:.2};",
                    }
                    span {
                        class: "text-3xl font-bold tabular-nums leading-none",
                        "data-testid": "stat-bpm",
                        "{bpm:.0}"
                    }
                }
                span { class: "text-[10px] uppercase tracking-widest text-muted-foreground", "bpm" }
            }

            Divider {}

            // ── The meter, written the way a chart writes it ──────
            div {
                class: "flex flex-col items-center justify-center leading-none",
                "data-testid": "stat-meter",
                span { class: "text-xl font-bold tabular-nums", "{beats_per_bar}" }
                div { class: "h-px w-5 bg-foreground/70 my-0.5" }
                span { class: "text-xl font-bold tabular-nums", "{beat_unit}" }
            }

            Divider {}

            // ── The clock, quietly ────────────────────────────────
            div {
                class: "flex flex-col items-end justify-center gap-1",
                "data-testid": "stat-clock",
                span {
                    class: "text-sm font-mono tabular-nums text-muted-foreground",
                    "{clock(seconds)}"
                }
                if looping {
                    span {
                        class: "text-[10px] uppercase tracking-widest text-yellow-500",
                        "data-testid": "stat-loop",
                        "loop"
                    }
                }
            }
        }
    }
}

/// A hairline between two instruments.
///
/// A rule rather than a gap: four things in a row with only space
/// between them read as one thing in four parts, which is what the
/// pills already did wrong.
#[component]
fn Divider() -> Element {
    rsx! {
        div { class: "w-px self-stretch bg-border/70" }
    }
}

// The tests for `dots`, `pulse` and `clock` live in
// `tests/transport_stats.rs`: this crate's lib is `test = false`, so a
// `#[cfg(test)]` module here would never run — which is worse than no
// tests, because it looks like there are some.
