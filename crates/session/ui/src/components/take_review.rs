//! The take review panel — rate what you just played, before the next one.
//!
//! One of these in front of every player: a tablet on a mic stand, the
//! pass they just finished drawn across it, and four buttons. The whole
//! interaction has to survive being done with one hand, in the seconds
//! between somebody saying "again" and the count-in — so everything
//! here is big, nothing is a menu, and the only typing is optional.
//!
//! The model is `session_proto::review`; this draws it. What a mark MEANS —
//! that the worst verdict wins, that a stretch and a whole take are the
//! same kind of thing, that re-rating corrects rather than accumulates
//! — is decided there and tested without a browser.
//!
//! # Why the waveform is one shape for many tracks
//!
//! The drummer has eight microphones and does not want eight waveforms.
//! What they want is the shape of what they played, which is the
//! consolidation of their own tracks — the same thing the arrangement's
//! folder preview draws for a collapsed folder. So the peaks handed in
//! here are already summed: this component takes ONE envelope and never
//! knows how many tracks made it.

use session_proto::review::{Mark, Pass, Span, Verdict};

use crate::prelude::*;

/// The shortest the waveform is ever drawn.
///
/// It takes whatever height the panel has — the point of a tablet
/// turned over to one take is that the take is the size of the screen —
/// but never less than this, which is what a thumb needs to drag across
/// accurately at arm's length.
const WAVE_MIN_H: f64 = 132.0;

/// The consolidated envelope of a pass: one magnitude per column,
/// `0.0..=1.0`.
///
/// A plain list rather than min/max pairs because this is a glance, not
/// an edit: you are looking for where the energy was and where the hole
/// is, and a rectified envelope says both.
pub type Envelope = Vec<f32>;

/// Rate the take.
#[component]
pub fn TakeReview(
    /// The pass being reviewed — normally the one that just ended.
    pass: Pass,
    /// The consolidated peaks of THIS role's tracks over the pass.
    /// Empty while they are still being read: the panel draws a line
    /// and stays usable, because a rating does not need a picture.
    #[props(default)]
    peaks: Envelope,
    /// Which ROLE this tablet is for — "Guitar 1", "Drums". `None`
    /// until somebody says.
    #[props(default)]
    role: Option<String>,
    /// The roles on this session.
    #[props(default)]
    roles: Vec<String>,
    /// Somebody picked a role. An empty string means "not me" — the way
    /// back to the picker.
    on_role: EventHandler<String>,
    /// A verdict was given, on the whole take or on a stretch of it.
    on_mark: EventHandler<Mark>,
) -> Element {
    // The stretch being marked, while a drag is in progress or after
    // one has settled. `None` means the buttons act on the whole take,
    // which is the common case and therefore the resting state.
    let mut selection = use_signal(|| None::<Span>);
    let mut dragging = use_signal(|| false);
    // How wide the waveform is, in CSS pixels, as the browser last laid
    // it out. A drag is a fraction of this; without it the panel would
    // have to assume a width and would be wrong on every screen but
    // one.
    let mut wave_width = use_signal(|| 0.0_f64);

    let length = pass.length();
    let Some(who) = role.clone() else {
        return rsx! {
            div { class: "h-full w-full",
                WhichOneAreYou { roles, on_role }
            }
        };
    };

    let mine = pass.verdict_of(&who);
    let band = pass.consensus();
    let marks = pass.marks.clone();
    let at_of = move |x: f64| {
        let width = wave_width();
        if width <= 0.0 {
            return 0.0;
        }
        (x / width).clamp(0.0, 1.0) * length
    };

    rsx! {
        div { class: "flex flex-col gap-3 select-none h-full min-h-0 w-full",

            // ── Who, and what the band thinks so far ───────────────
            div { class: "flex items-center gap-3",
                div {
                    class: "text-2xl font-bold tracking-tight",
                    "data-testid": "take-number",
                    "Take {pass.number}"
                }
                if let Some(band) = band {
                    VerdictChip { verdict: band, label: "band".to_string() }
                }
                // Your own verdict, beside the band's, so you can see
                // whether you have said anything yet without hunting
                // for a lit button.
                if let Some(mine) = mine {
                    VerdictChip { verdict: mine, label: who.clone() }
                }
            }

            // ── The pass ───────────────────────────────────────────
            div {
                class: "relative flex-1 min-h-0 rounded-xl bg-card border border-border overflow-hidden touch-none",
                style: "min-height: {WAVE_MIN_H}px;",
                onmounted: move |event| async move {
                    if let Ok(rect) = event.data().get_client_rect().await {
                        wave_width.set(rect.size.width);
                    }
                },
                onpointerdown: move |event| {
                    dragging.set(true);
                    let at = at_of(event.data().element_coordinates().x);
                    selection.set(Some(Span::new(at, at)));
                },
                onpointermove: move |event| {
                    if dragging() {
                        let at = at_of(event.data().element_coordinates().x);
                        let from = selection().map_or(at, |span| span.from);
                        selection.set(Some(Span::new(from, at)));
                    }
                },
                onpointerup: move |_| {
                    dragging.set(false);
                    // A tap is not a selection. Anything shorter than a
                    // moment is somebody touching the waveform, and the
                    // buttons should still mean the whole take.
                    if selection().is_some_and(|span| span.length() < 0.25) {
                        selection.set(None);
                    }
                },

                Waveform { peaks, marks, length }

                // The stretch being marked, over everything.
                if let Some(span) = selection() {
                    div {
                        class: "absolute inset-y-0 bg-primary/25 border-x-2 border-primary pointer-events-none",
                        style: "left: {percent(span.from, length)}%; width: {percent(span.length(), length)}%;",
                    }
                }
            }

            // ── What the buttons are about ─────────────────────────
            div { class: "flex items-center gap-3 text-sm text-muted-foreground min-h-6",
                if let Some(span) = selection() {
                    span { class: "text-primary font-medium",
                        "marking {clock(span.from)}–{clock(span.to)}"
                    }
                    button {
                        class: "underline",
                        onclick: move |_| selection.set(None),
                        "the whole take instead"
                    }
                } else {
                    span { "drag across the take to mark part of it" }
                }
            }

            // ── The four buttons ───────────────────────────────────
            div { class: "grid grid-cols-4 gap-3",
                for verdict in [Verdict::Good, Verdict::VeryGood, Verdict::Amazing, Verdict::Mistake] {
                    VerdictButton {
                        verdict,
                        // Lit only when it is this performer's verdict on
                        // the whole take: a selection is a new judgement,
                        // not an edit of the old one.
                        lit: selection().is_none() && mine == Some(verdict),
                        on_press: {
                            let who = who.clone();
                            move |verdict| {
                                let mark = match selection() {
                                    Some(span) => Mark::part(who.clone(), span, verdict),
                                    None => Mark::whole(who.clone(), length, verdict),
                                };
                                on_mark.call(mark);
                                selection.set(None);
                            }
                        },
                    }
                }
            }
        }
    }
}

/// The first thing a tablet shows: whose is it?
#[component]
fn WhichOneAreYou(roles: Vec<String>, on_role: EventHandler<String>) -> Element {
    rsx! {
        div { class: "flex flex-col gap-4",
            div {
                class: "text-xl font-semibold",
                "data-testid": "who-are-you",
                "Which one is this?"
            }
            if roles.is_empty() {
                p { class: "text-sm text-muted-foreground",
                    "No roles on this session yet — they come from the tracks."
                }
            }
            div { class: "grid grid-cols-3 gap-3 w-full max-w-3xl",
                for role in roles {
                    button {
                        class: "h-16 rounded-xl bg-secondary text-secondary-foreground text-lg font-semibold active:brightness-125",
                        "data-testid": "role-{role}",
                        onclick: {
                            let role = role.clone();
                            move |_| on_role.call(role.clone())
                        },
                        "{role}"
                    }
                }
            }
        }
    }
}

/// One of the four.
#[component]
fn VerdictButton(verdict: Verdict, lit: bool, on_press: EventHandler<Verdict>) -> Element {
    let (face, tint) = match verdict {
        Verdict::Good => ("★", "bg-secondary text-secondary-foreground"),
        Verdict::VeryGood => ("★★", "bg-secondary text-secondary-foreground"),
        Verdict::Amazing => ("★★★", "bg-secondary text-secondary-foreground"),
        Verdict::Mistake => ("✕", "bg-red-500/15 text-red-400"),
    };
    // Lit is a ring rather than a fill: the buttons must read the same
    // at a glance from three feet away whether or not one is chosen.
    let ring = if lit { "ring-2 ring-primary" } else { "" };
    rsx! {
        button {
            // h-16 is the smallest target that survives being hit with
            // a stick still in your hand.
            class: "h-16 rounded-xl text-2xl font-semibold active:brightness-125 {tint} {ring}",
            // A stable hook, so a test presses the button it means
            // rather than matching on a glyph that is a design choice.
            "data-testid": "verdict-{verdict.token()}",
            onclick: move |_| on_press.call(verdict),
            "{face}"
        }
    }
}

/// A verdict as a small badge.
#[component]
fn VerdictChip(verdict: Verdict, label: String) -> Element {
    let (face, tint) = match verdict {
        Verdict::Mistake => ("✕", "bg-red-500/15 text-red-400"),
        Verdict::Amazing => ("★★★", "bg-secondary text-secondary-foreground"),
        Verdict::VeryGood => ("★★", "bg-secondary text-secondary-foreground"),
        Verdict::Good => ("★", "bg-secondary text-secondary-foreground"),
    };
    rsx! {
        div { class: "px-3 py-1 rounded-full text-sm font-medium flex items-center gap-2 {tint}",
            span { "{face}" }
            span { class: "text-muted-foreground", "{label}" }
        }
    }
}

/// The pass, drawn.
///
/// SVG rather than a canvas: it scales with the panel, it needs no
/// measurement pass to draw at the right size, and a mark over it is an
/// ordinary element rather than something redrawn by hand.
#[component]
fn Waveform(peaks: Envelope, marks: Vec<Mark>, length: f64) -> Element {
    let shape = envelope_path(&peaks);
    rsx! {
        // The marks under the shape, so the shape stays readable.
        for mark in marks.iter().filter(|mark| !mark.span.is_whole(length)) {
            div {
                class: if mark.verdict.is_good() {
                    "absolute inset-y-0 bg-emerald-400/20 border-x border-emerald-400/60"
                } else {
                    "absolute inset-y-0 bg-red-400/20 border-x border-red-400/60"
                },
                style: "left: {percent(mark.span.from, length)}%; width: {percent(mark.span.length(), length)}%;",
            }
        }
        svg {
            class: "absolute inset-0 w-full h-full text-primary/70",
            view_box: "0 0 1000 100",
            preserve_aspect_ratio: "none",
            if shape.is_empty() {
                // Nothing read yet: a line, not a drawn waveform. A
                // shape that is not the audio is worse than no shape,
                // because it looks like the audio.
                line {
                    x1: "0",
                    y1: "50",
                    x2: "1000",
                    y2: "50",
                    stroke: "currentColor",
                    stroke_width: "1",
                    opacity: "0.4",
                }
            } else {
                path { d: "{shape}", fill: "currentColor" }
            }
        }
    }
}

/// The envelope as a filled path, mirrored about the middle.
fn envelope_path(peaks: &[f32]) -> String {
    if peaks.is_empty() {
        return String::new();
    }
    let columns = count(peaks.len());
    let step = 1000.0 / columns;
    let mut top = String::from("M 0 50");
    let mut bottom = String::new();
    for (i, peak) in peaks.iter().enumerate() {
        let x = f64::from(u32::try_from(i).unwrap_or(u32::MAX)) * step;
        let h = f64::from(peak.clamp(0.0, 1.0)) * 48.0;
        top.push_str(&format!(" L {x:.1} {:.1}", 50.0 - h));
        bottom.insert_str(0, &format!(" L {x:.1} {:.1}", 50.0 + h));
    }
    format!("{top}{bottom} Z")
}

/// A length as a float, without an `as`.
fn count(len: usize) -> f64 {
    f64::from(u32::try_from(len).unwrap_or(u32::MAX))
}

/// A position as a percentage of the pass, for CSS.
fn percent(seconds: f64, length: f64) -> f64 {
    if length <= 0.0 {
        return 0.0;
    }
    (seconds / length * 100.0).clamp(0.0, 100.0)
}

/// `m:ss.s`, for saying which stretch is being marked.
fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let minutes = (seconds / 60.0).floor();
    let rest = minutes.mul_add(-60.0, seconds);
    format!("{minutes:.0}:{rest:04.1}")
}

#[cfg(test)]
mod tests {
    use super::{clock, envelope_path, percent};

    #[test]
    fn an_empty_envelope_draws_nothing_rather_than_a_flat_lie() {
        assert!(envelope_path(&[]).is_empty());
    }

    /// The path is closed and mirrored, so it fills.
    #[test]
    fn the_envelope_is_a_closed_shape() {
        let path = envelope_path(&[0.0, 1.0, 0.5]);
        assert!(path.starts_with("M 0 50"), "{path}");
        assert!(path.ends_with(" Z"), "{path}");
        // The loudest column reaches the top and its mirror.
        assert!(path.contains("2.0"), "{path}");
        assert!(path.contains("98.0"), "{path}");
    }

    #[test]
    fn a_position_is_a_percentage_of_the_pass() {
        assert!((percent(30.0, 120.0) - 25.0).abs() < 1e-9);
        // A pass with no length cannot divide, and does not panic.
        assert!(percent(30.0, 0.0).abs() < 1e-9);
        // Nothing runs off either end.
        assert!((percent(300.0, 120.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_time_reads_as_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00.0");
        assert_eq!(clock(64.25), "1:04.2");
    }
}
