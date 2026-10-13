//! Organize mode's chart editor: the song's `.kf`, edited live.
//!
//! The text is the editor's (`editor-view`, Blitz-native), coloured and
//! checked as keyflow as it is typed (`editor-keyflow-lang`: syntax colours,
//! live diagnostics). Each pause in typing lays the chart over the song —
//! the tempo, the markers and section regions, the key and the chords are
//! rebuilt, and the click, count and cues regenerated
//! ([`crate::prepare::apply_chart`]) — then the chart is saved to its `.kf`
//! and the song to its `.session`, the chart panel shows the new chart
//! ([`crate::chart_panel::publish_live`]) and the arrangement reads the
//! song back ([`crate::studio::request_resync`]).
//!
//! Text that does not parse changes nothing in the song: half-typed is the
//! normal state of a chart being edited, and the diagnostics say what is
//! wrong. The work runs on a thread of its own, a quarter-second or so
//! after the last keystroke, so typing never waits on it.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_util::StreamExt as _;

use crate::shell::{DIM, RULE, TEXT};
use crate::studio::StudioSession;

/// How long typing has to pause before the chart is laid over the song.
const PAUSE: Duration = Duration::from_millis(350);

/// What the last pause did.
#[derive(Clone, Debug, PartialEq)]
enum Outcome {
    /// Laid over the song: its sections and tempo, for the status line.
    Applied { sections: usize, bpm: f64 },
    /// Did not parse, or the song refused it — the song is as it was.
    Refused(String),
    /// Parsed and saved to its `.kf`, and the chart panel shows it — but
    /// the song is not rebuilt: this window drives another system, and the
    /// rebuild runs only on the local engine (see `crate::audio_mode`).
    ChartOnly,
}

/// The editor pane.
#[component]
pub fn ChartEditor() -> Element {
    let session: StudioSession = use_context();
    let file = session.chart_file.clone();
    let initial = use_hook(|| {
        file.as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default()
    });
    let state = use_signal(|| editor_state::EditorState::new(&initial));
    let mut outcome = use_signal(|| None::<Outcome>);
    // The worker's replies, back on this thread: a channel the worker can
    // send on from its own, which wakes the window when it does.
    let replies = use_coroutine(move |mut rx: UnboundedReceiver<Outcome>| async move {
        while let Some(reply) = rx.next().await {
            outcome.set(Some(reply));
        }
    });
    let worker = use_hook(|| {
        let project = crate::open::current_song().unwrap_or_default();
        start(project, file.clone(), replies.tx())
    });
    // Every change of text goes to the worker, which waits for the pause —
    // and, in a shared session, to the others.
    let mut last = use_signal(|| initial.clone());
    use_effect(move || {
        let current = state.read();
        publish_caret(&current);
        let text = current.doc.to_string();
        drop(current);
        if *last.peek() != text {
            last.set(text.clone());
            crate::collab::local_chart(&text);
            let _ = worker.send(text);
        }
    });
    // Someone else's edit: into the editor as a remote change (the caret
    // stays put), and into the chart panel. NOT through the worker: the
    // song's rebuild was done by whoever typed it, and its tracks and
    // items arrive through the session doc — rebuilding here would make a
    // second copy of everything under this machine's own guids.
    let mut carets = use_signal(|| 0_u64);
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(Duration::from_millis(100)).await;
            if let Some(text) = crate::collab::take_remote_chart() {
                last.set(text.clone());
                apply_remote(state, &text);
                if let (Some(project), Ok(chart)) =
                    (crate::open::current_song(), keyflow::parse(&text))
                {
                    crate::chart_panel::publish_live(&project, std::sync::Arc::new(chart), None);
                }
            }
            // Other people's carets move with nothing typed here.
            let rev = caret_revision();
            if *carets.peek() != rev {
                carets.set(rev);
            }
        }
    });
    let _ = carets();

    let status = match outcome() {
        None => ("", DIM.to_owned()),
        Some(Outcome::Applied { .. }) => ("", "#4ac26b".to_owned()),
        Some(Outcome::Refused(_) | Outcome::ChartOnly) => ("", "#e3b341".to_owned()),
    };
    let message = match outcome() {
        None => file.as_ref().and_then(|f| f.file_name()).map_or_else(
            || "no chart file".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        ),
        Some(Outcome::Applied { sections, bpm }) => {
            format!("laid over the song — {sections} sections, {bpm} bpm")
        }
        Some(Outcome::Refused(why)) => format!("not applied: {why}"),
        Some(Outcome::ChartOnly) => {
            "chart saved — the song is rebuilt only in Engine mode".to_owned()
        }
    };
    let css =
        keyflow_editor_lang::highlight_css(&keyflow_editor_lang::HighlightTheme::default_dark());
    rsx! {
        style { dangerous_inner_html: editor_view::EDITOR_CSS }
        style { dangerous_inner_html: "{css}" }
        style { dangerous_inner_html: EDITOR_THEME }
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; display:flex; \
                    flex-direction:column; background:#101114;",
            // Clicking in here gives this pane the keyboard: the space bar
            // types a space rather than playing.
            onmousedown: move |event| {
                crate::keys::set_editing(true);
                event.stop_propagation();
            },
            div {
                style: "flex:none; display:flex; align-items:center; gap:8px; height:26px; \
                        padding:0 10px; border-bottom:1px solid {RULE}; font-size:11px;",
                span { style: "width:7px; height:7px; border-radius:4px; background:{status.1};" }
                span {
                    style: "color:{DIM}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                    "{message}"
                }
            }
            div {
                style: "flex:1; min-height:0; overflow:auto; padding:8px 12px; color:{TEXT}; \
                        font-family: ui-monospace, Menlo, monospace; font-size:13px;",
                editor_view::Editor {
                    state,
                    decorations: Some(editor_view::DecorationSource::new(|st| {
                        let mut out = keyflow_editor_lang::keyflow_decorations(st);
                        out.extend(remote_carets(st));
                        out
                    })),
                }
            }
        }
    }
}

/// The small button over the chart's corner that opens and closes the
/// editor beside it — in any mode; Organize opens it on the way in.
#[component]
pub fn EditorToggle(open: Signal<bool>) -> Element {
    let (bg, fg, border) = if open() {
        (crate::shell::ACCENT, "#0b0c0e", crate::shell::ACCENT)
    } else {
        ("rgba(16,17,20,0.85)", TEXT, RULE)
    };
    let title = if open() {
        "Hide the chart's text"
    } else {
        "Edit the chart as text"
    };
    rsx! {
        button {
            title,
            style: "height:22px; padding:0 9px; border-radius:5px; border:1px solid {border}; \
                    background:{bg}; color:{fg}; font-size:11px; font-weight:600; cursor:pointer;",
            onclick: move |_| open.toggle(),
            "Edit"
        }
    }
}

/// This peer's caret, as char offsets (anchor, head).
static CARET: std::sync::Mutex<Option<(usize, usize)>> = std::sync::Mutex::new(None);

fn publish_caret(state: &editor_state::EditorState) {
    let primary = state.selection.primary();
    let rope = state.doc.rope();
    let char_at = |byte: usize| rope.byte_to_char(byte.min(rope.len_bytes()));
    if let Ok(mut slot) = CARET.lock() {
        *slot = Some((char_at(primary.anchor), char_at(primary.head)));
    }
}

/// This peer's caret as stable positions in the shared chart, for
/// presence — `None` outside a session or before the editor opened.
#[must_use]
pub fn local_caret() -> Option<(Vec<u8>, Vec<u8>)> {
    let (anchor, head) = (*CARET.lock().ok()?)?;
    let (doc, _, _) = crate::collab::chart_context()?;
    Some((doc.chart_cursor(anchor)?, doc.chart_cursor(head)?))
}

/// A number that changes when anyone's caret does.
fn caret_revision() -> u64 {
    use std::hash::{Hash, Hasher};
    let Some((_, presence, me)) = crate::collab::chart_context() else {
        return 0;
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut entries: Vec<(String, Vec<u8>)> = presence
        .states()
        .into_iter()
        .filter(|(k, _)| !k.starts_with(&me) && k.ends_with(session::sync::presence::STATE))
        .filter_map(|(k, v)| {
            let state = session::sync::presence::PeerState::decode(&v)?;
            let (a, h) = state.chart_caret?;
            Some((k, [a, h].concat()))
        })
        .collect();
    entries.sort();
    entries.hash(&mut hasher);
    hasher.finish()
}

/// Everyone else's caret and selection in the chart, in their colour.
fn remote_carets(state: &editor_state::EditorState) -> Vec<editor_state::DecoratedRange> {
    let Some((doc, presence, me)) = crate::collab::chart_context() else {
        return Vec::new();
    };
    let rope = state.doc.rope();
    let max = rope.len_chars();
    let mut out = Vec::new();
    for (key, value) in presence.states() {
        if key.starts_with(&me) || !key.ends_with(session::sync::presence::STATE) {
            continue;
        }
        let Some(peer) = session::sync::presence::PeerState::decode(&value) else {
            continue;
        };
        let Some((anchor, head)) = peer.chart_caret.as_ref() else {
            continue;
        };
        let (Some(a), Some(h)) = (
            doc.resolve_chart_cursor(anchor),
            doc.resolve_chart_cursor(head),
        ) else {
            continue;
        };
        let (a, h) = (rope.char_to_byte(a.min(max)), rope.char_to_byte(h.min(max)));
        let (r, g, b) = (
            (peer.color >> 16) & 0xff,
            (peer.color >> 8) & 0xff,
            peer.color & 0xff,
        );
        let color = format!("rgb({r},{g},{b})");
        if a != h {
            out.push(editor_state::DecoratedRange::mark_with_attrs(
                a.min(h)..a.max(h),
                "collab-selection",
                vec![(
                    "style".to_owned(),
                    format!("background-color: rgba({r},{g},{b},0.28);"),
                )],
            ));
        }
        let name = peer
            .name
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        // A bar the height of the line, and the name above it — clear of
        // the text it points into.
        out.push(editor_state::DecoratedRange::widget(
            h,
            format!(
                "<span class=\"collab-caret\" style=\"display:inline-block; position:relative; \
                 width:0; height:1.2em; vertical-align:text-bottom; \
                 border-left:2px solid {color}; margin-left:-1px;\">\
                 <span style=\"position:absolute; bottom:1.15em; left:-2px; background:{color}; \
                 color:#101114; font-size:9px; line-height:12px; padding:0 3px; \
                 border-radius:3px 3px 3px 0; white-space:nowrap;\">{name}</span></span>"
            ),
        ));
    }
    out
}

/// Bring the editor to `text` as a change tagged `"remote"`, so this
/// peer's caret stays where it was.
fn apply_remote(state: Signal<editor_state::EditorState>, text: &str) {
    let current = state.peek().clone();
    let changes = editor_crdt::remote_text_to_changes(current.doc.rope(), text);
    if changes.is_empty() {
        return;
    }
    let mut state = state;
    state.set(
        current.update(
            editor_state::TransactionSpec::new()
                .changes(changes)
                .user_event("remote"),
        ),
    );
}

/// The editor's own palette tokens, dark, for the stylesheet's variables.
const EDITOR_THEME: &str = "
:root {
    --background: #101114;
    --muted: #1a1c20;
    --foreground: #e5e7eb;
    --muted-foreground: #8b9099;
    --primary: #3aa0ff;
}
";

/// Start the worker that lays chart text over `project`: it takes the
/// latest text once typing has paused, applies it, saves, and replies.
fn start(
    project: String,
    file: Option<PathBuf>,
    reply: futures_channel::mpsc::UnboundedSender<Outcome>,
) -> mpsc::Sender<String> {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::Builder::new()
        .name("session-chart-editor".into())
        .spawn(move || {
            while let Ok(mut text) = rx.recv() {
                // Coalesce: whatever arrives before the pause replaces it.
                while let Ok(newer) = rx.recv_timeout(PAUSE) {
                    text = newer;
                }
                let outcome = apply(&project, file.as_deref(), &text);
                let _ = reply.unbounded_send(outcome);
            }
        })
        .ok();
    tx
}

/// Lay `text` over the song, save it, and say how it went.
fn apply(project: &str, file: Option<&std::path::Path>, text: &str) -> Outcome {
    let chart = match keyflow::parse(text) {
        Ok(chart) => chart,
        Err(e) => return Outcome::Refused(e.to_string()),
    };
    // Another song picked while this was waiting: its engine project is not
    // this one's, and the guide works on the current song.
    if crate::open::current_song().as_deref() != Some(project) {
        return Outcome::Refused("another song is up".to_owned());
    }
    let applied = crate::open::with_local_engine(crate::open::LocalOnly::ChartRebuild, |daw| {
        let rebuilt = crate::prepare::apply_chart(daw, project, text, true)?;
        // Saved where the song was opened from, when that is a `.session`.
        let saved = daw
            .read_project(project, |p| std::path::PathBuf::from(&p.info.path))
            .filter(|path| crate::open::is_session(path));
        if let Some(dir) = saved {
            crate::session_file::save_session_behind(daw, project, &dir);
        }
        Ok::<_, eyre::Report>(rebuilt)
    });
    // Remote: nothing to lay it over here. The chart is still the song's
    // chart — saved, and shown — and its tempo and sections stay the
    // remote's until it is opened in Engine mode.
    let Some(applied) = applied else {
        if let Some(path) = file
            && let Err(e) = std::fs::write(path, text)
        {
            tracing::warn!(error = %e, "chart editor: the chart could not be saved");
        }
        crate::chart_panel::publish_live(project, std::sync::Arc::new(chart), None);
        return Outcome::ChartOnly;
    };
    match applied {
        Ok(rebuilt) => {
            if let Some(path) = file
                && let Err(e) = std::fs::write(path, text)
            {
                tracing::warn!(error = %e, "chart editor: the chart could not be saved");
            }
            crate::chart_panel::publish_live(
                project,
                std::sync::Arc::new(chart),
                Some(rebuilt.built.song_start_seconds),
            );
            crate::studio::request_resync();
            Outcome::Applied {
                sections: rebuilt.built.sections,
                bpm: rebuilt.built.tempo_bpm,
            }
        }
        Err(e) => Outcome::Refused(e.to_string()),
    }
}
