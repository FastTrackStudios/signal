//! `/reference` — a recording playing in step with the session.
//!
//! The browser half of [`session::reference`]. That module says where
//! the player should be and what it should be doing; this is the thing
//! that does it, and the only part that has to know YouTube exists.
//!
//! **Why the browser is the right home for this.** A YouTube video can
//! only be played by YouTube's own player, in an iframe, on a page —
//! their terms require it and their API assumes it. A native window
//! would have to embed a browser to do the same job, so the web build
//! is not a lesser surface here. It is the one that can.

use dioxus::prelude::*;
use session::reference::{Command, Playing, Reference, Source, follow};

/// The IFrame Player API, loaded once.
///
/// From YouTube's own origin because it must be: the API script and the
/// player it creates talk to each other across the frame boundary, and
/// a copy served from anywhere else is a different origin and a silent
/// no-op.
const API: &str = "https://www.youtube.com/iframe_api";

#[component]
pub fn ReferencePage() -> Element {
    let mut pasted = use_signal(String::new);
    let mut reference = use_signal(|| None::<Reference>);
    let mut refused = use_signal(|| false);

    rsx! {
        document::Script { src: API }
        div { class: "reference",
            h1 { "Play against a reference" }
            p { class: "lede",
                "Paste a video and line it up with the session. The bar \
                 lines follow what you hear, not the other way round."
            }

            div { class: "paste",
                input {
                    r#type: "text",
                    placeholder: "YouTube link, or just the id",
                    value: "{pasted}",
                    oninput: move |event| {
                        pasted.set(event.value());
                        refused.set(false);
                    },
                }
                button {
                    onclick: move |_| {
                        match Source::youtube(&pasted()) {
                            // The id is what is kept. Whatever else was
                            // on the end of the link identified a
                            // playlist, a start time or whoever shared
                            // it — none of it identifies the video.
                            Some(source) => {
                                refused.set(false);
                                reference.set(Some(Reference::new(source)));
                            }
                            None => refused.set(true),
                        }
                    },
                    "Use it"
                }
            }

            if refused() {
                p { class: "refused",
                    "That is not a video I can find an id in. A watch \
                     link, a short link, or the eleven characters on \
                     their own."
                }
            }

            if reference().is_some() {
                Player { reference }
            }
        }
    }
}

/// The player, the loop that keeps it in step, and the two marks that
/// line it up.
#[component]
fn Player(reference: Signal<Option<Reference>>) -> Element {
    let Some(Source::Youtube(id)) = reference().map(|r| r.source) else {
        return rsx! {
            p { "Only YouTube is wired up so far." }
        };
    };

    // The player's own position, as of the last tick. Kept because the
    // marking gesture needs it: "that, there, is this, here" is asked
    // about where the video IS, and asking the player again at the
    // moment of the click would answer a tenth of a second later than
    // the thing the person was pointing at.
    let mut player_at = use_signal(|| 0.0_f64);
    // The first of two marks, waiting for its pair.
    let mut pending = use_signal(|| None::<(f64, f64)>);

    // Hand the player to the API once the frame exists. The API calls
    // back a global when it has loaded, which may be before or after
    // this component mounts — so both orders have to work, which is
    // what the `if` in the script is for.
    use_effect(move || {
        let _ = dioxus::document::eval(SETUP);
    });

    // The loop. Ten times a second, not every frame: the player is
    // asked across a frame boundary and the tolerance is eighty
    // milliseconds, so a faster loop would ask more often than the
    // answer can change.
    use_future(move || async move {
        loop {
            // The wasm-cfg-split seam: browser timers here, tokio
            // on native, so this loop is the same code either way.
            architect::platform::sleep(std::time::Duration::from_millis(100)).await;
            let Ok(state) = dioxus::document::eval(READ).await else {
                continue;
            };
            let (at, playing) = match state.as_str() {
                Some(text) => match text.split_once('|') {
                    Some((at, playing)) => (at.parse::<f64>().unwrap_or(0.0), playing == "1"),
                    None => continue,
                },
                None => continue,
            };
            player_at.set(at);
            let (Some(current), Some((session_at, rolling))) = (reference(), session_now()) else {
                // Nothing to follow is not the same as stopped: with no
                // session on this page the player is the person's to
                // drive, which is how they find the moment they are
                // about to mark.
                continue;
            };
            let command = next_command(&current, session_at, rolling, at, playing);
            let js = match command {
                Command::Seek(to) => format!("window.ftsReference?.seekTo({to}, true);"),
                Command::Play => "window.ftsReference?.playVideo();".to_owned(),
                Command::Pause => "window.ftsReference?.pauseVideo();".to_owned(),
                // The common case, and it must cost nothing: a
                // player nudged every tick stutters.
                Command::Nothing => continue,
            };
            let _ = dioxus::document::eval(&js);
        }
    });

    rsx! {
        div { class: "player",
            iframe {
                id: "fts-reference",
                width: "640",
                height: "360",
                // `enablejsapi` is what makes the player answerable at
                // all; without it the frame plays and ignores everyone.
                src: "https://www.youtube-nocookie.com/embed/{id}?enablejsapi=1&rel=0",
                // nocookie above, and no autoplay: a reference that
                // started playing when the page loaded would be a
                // reference talking over whatever you were listening to.
                allow: "accelerometer; encrypted-media; picture-in-picture",
                title: "Reference recording",
            }

            // Two marks, one button. The first says where the recording
            // starts against the session; the second says how fast it
            // runs. Two buttons would imply an order you have to
            // remember, and the order is the only thing about this
            // gesture that matters.
            div { class: "marks",
                button {
                    onclick: move |_| {
                        let Some(current) = reference() else { return };
                        let at = session_now().map_or(0.0, |(at, _)| at);
                        let (lined, next) = mark(&current, pending(), (at, player_at()));
                        reference.set(Some(lined));
                        pending.set(next);
                    },
                    if pending().is_some() { "Mark the second moment" } else { "Mark this moment" }
                }
                if let Some(reference) = reference() {
                    p { class: "lined-up",
                        "Session {reference.anchor:.3}s is {reference.from:.3}s in, "
                        "at {reference.rate:.4}×."
                    }
                }
                if session_now().is_none() {
                    p { class: "unfollowed",
                        "No song is playing, so the video is yours to \
                         drive — find the moment, then mark it."
                    }
                }
            }
        }
    }
}

/// Where the session is, and whether it is rolling.
///
/// `None` when there is no song to follow — this page opened on its
/// own, before any setlist. The distinction matters: a missing session
/// read as "stopped at zero" would pause the video every tenth of a
/// second and there would be no way to scrub it to the moment you were
/// about to mark.
fn session_now() -> Option<(f64, bool)> {
    let song = session_ui::ACTIVE_INDICES().song_index?;
    let transport = session_ui::SONG_TRANSPORT().get(&song).cloned()?;
    let at = transport.position.time.map_or(0.0, |t| t.as_seconds());
    Some((at, transport.is_playing))
}

/// Fold a marked moment into the reference.
///
/// The first mark lines the recording up on that moment. The second
/// gives it a rate and finishes the pair, so the next mark starts a new
/// one — which is what makes a wrong mark cost one more click rather
/// than a reset button nobody would find.
///
/// Each moment is `(session, recording)`.
#[must_use]
pub fn mark(
    reference: &Reference,
    pending: Option<(f64, f64)>,
    moment: (f64, f64),
) -> (Reference, Option<(f64, f64)>) {
    match pending {
        Some(first) => (reference.lined_up(first, moment), None),
        None => (reference.aligned(moment.0, moment.1), Some(moment)),
    }
}

/// Hand the iframe to the IFrame API.
///
/// Written to work in either order: the API script may load before this
/// runs or after, and a page that only handled one of those would work
/// on a warm cache and not on a cold one — which is the worst kind of
/// intermittent.
const SETUP: &str = r"
    window.ftsMakeReference = function () {
        if (window.ftsReference) { return; }
        const frame = document.getElementById('fts-reference');
        if (!frame || !window.YT || !window.YT.Player) { return; }
        window.ftsReference = new window.YT.Player('fts-reference', {});
    };
    window.onYouTubeIframeAPIReady = window.ftsMakeReference;
    window.ftsMakeReference();
";

/// What the player says it is doing: `seconds|1` or `seconds|0`.
///
/// One string rather than two calls, because the two would be read a
/// frame apart and a position from before a state change is how a loop
/// decides to correct something that already corrected itself.
const READ: &str = r"
    (() => {
        const p = window.ftsReference;
        if (!p || !p.getCurrentTime) { return '0|0'; }
        const playing = p.getPlayerState && p.getPlayerState() === 1;
        return p.getCurrentTime() + '|' + (playing ? '1' : '0');
    })()
";

/// What to do about the player this frame, as a plain question.
///
/// Separated from the component so the decision can be tested without a
/// browser: everything above is an iframe and an input box, and
/// everything that could be wrong is here or in `session::reference`.
#[must_use]
pub fn next_command(
    reference: &Reference,
    session_at: f64,
    rolling: bool,
    player_at: f64,
    player_playing: bool,
) -> Command {
    follow(
        reference,
        session_at,
        rolling,
        Playing {
            at: player_at,
            playing: player_playing,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{mark, next_command};
    use session::reference::{Command, Reference, Source};

    fn video() -> Reference {
        Reference::new(Source::Youtube("dQw4w9WgXcQ".into())).aligned(4.0, 30.0)
    }

    /// The page asks the same question the domain answers.
    ///
    /// Thin on purpose: the moment this function starts deciding
    /// anything itself, the decision is in a component and can only be
    /// tested by driving a browser.
    #[test]
    fn the_page_defers_to_the_domain() {
        let r = video();
        // Session at the anchor, player at the start: go to 30.
        assert_eq!(next_command(&r, 4.0, true, 0.0, false), Command::Seek(30.0));
        // In the right place and stopped: start it.
        assert_eq!(next_command(&r, 4.0, true, 30.0, false), Command::Play);
        // In step: leave it be.
        assert_eq!(next_command(&r, 4.0, true, 30.0, true), Command::Nothing);
        // Session stopped: stop first, whatever else is true.
        assert_eq!(next_command(&r, 4.0, false, 999.0, true), Command::Pause);
    }

    /// One mark lines the recording up on that moment and waits for its
    /// pair; the pair gives the rate and finishes.
    #[test]
    fn two_marks_line_the_recording_up_and_set_its_rate() {
        let first = Reference::new(Source::Youtube("dQw4w9WgXcQ".into()));
        let (lined, pending) = mark(&first, None, (0.0, 10.0));
        assert_eq!(pending, Some((0.0, 10.0)));
        assert_eq!(
            next_command(&lined, 0.0, true, 0.0, false),
            Command::Seek(10.0)
        );

        let (lined, pending) = mark(&lined, pending, (100.0, 210.0));
        assert_eq!(pending, None, "the pair is finished");
        assert!((lined.rate - 2.0).abs() < 1e-9, "rate was {}", lined.rate);
        // Halfway through the session is halfway through the span.
        assert_eq!(
            next_command(&lined, 50.0, true, 0.0, false),
            Command::Seek(110.0)
        );
    }

    /// A third mark starts a new pair rather than adding to the old
    /// one, which is how a mark in the wrong place is undone.
    #[test]
    fn a_third_mark_starts_over() {
        let r = video();
        let (_, pending) = mark(&r, None, (0.0, 10.0));
        let (r, pending) = mark(&r, pending, (100.0, 210.0));
        let (r, pending) = mark(&r, pending, (8.0, 60.0));
        assert_eq!(pending, Some((8.0, 60.0)));
        assert!((r.anchor - 8.0).abs() < 1e-9);
        assert!((r.from - 60.0).abs() < 1e-9);
    }
}
