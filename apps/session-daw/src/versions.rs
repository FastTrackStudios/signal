//! The song's versions, from the top bar.
//!
//! Save it (Ctrl+S does too), or save a named checkpoint of it —
//! "Organized", before the mix — which leaves the song open as it is
//! (`crate::projects`); and the checkpoints it has. Through the Action
//! List, as a key or a model would: `SESSION_SAVE` and
//! `SESSION_CHECKPOINT`.

use dioxus::prelude::*;

use crate::shell::{BAR_BG, BAR_H, DIM, RAISED, RULE, TEXT};

/// Good, or not.
const OK: &str = "#4ac26b";
const NO: &str = "#e5a442";

/// One of the song's versions: the song's own, or a checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub name: String,
    /// When it was saved ("5 min ago"); empty for the song's own.
    pub when: String,
    pub path: std::path::PathBuf,
    /// Whether it is the one open now.
    pub open: bool,
}

/// `work` on a thread of its own, its answer awaited: what the panel asks
/// of the song's file never holds the window still.
async fn behind<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("versions".into())
        .spawn(move || {
            let _ = tx.send(work());
        })
        .ok()?;
    rx.await.ok()
}

/// An action run behind the window: what it said, or why not.
async fn run_behind(id: &'static str, args: String) -> Result<String, String> {
    behind(move || {
        crate::actions::list()
            .run_json(id, args.as_bytes())
            .map(|bytes| {
                serde_json::from_slice::<String>(&bytes)
                    .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).into_owned())
            })
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|| Err("the action did not finish".to_owned()))
}

/// A version's row: the open one lit.
const fn row_bg(open: bool) -> &'static str {
    if open { RAISED } else { "transparent" }
}

/// A message's colour: good, or not.
const fn tone(good: bool) -> &'static str {
    if good { OK } else { NO }
}

/// The Versions button and its panel.
#[component]
pub fn Versions() -> Element {
    let mut open = use_signal(|| false);
    let mut name = use_signal(String::new);
    let mut said = use_signal(|| None::<(bool, String)>);
    let mut listed = use_signal(Vec::<Version>::new);
    let mut unsaved = use_signal(|| None::<bool>);
    // The versions and whether the song is saved: read behind the window
    // (a fingerprint of every item, the file opened), shown when read.
    let refresh = move || {
        spawn(async move {
            if let Some((versions, dirty)) = behind(|| {
                (
                    crate::actions::open_song_versions(),
                    crate::actions::open_song_unsaved(),
                )
            })
            .await
            {
                listed.set(versions);
                unsaved.set(dirty);
            }
        });
    };
    // A version picked to open, while asking whether to save this one.
    let mut switching = use_signal(|| None::<Version>);
    let back = try_use_context::<crate::shell::Back>();
    // Out of the song and in again as `to`, saved first if asked.
    let mut leave = move |to: Version| {
        let Some(crate::shell::Back(back)) = back else {
            said.set(Some((
                false,
                "this window cannot open another version".to_owned(),
            )));
            return;
        };
        crate::projects::open_next(to.path);
        open.set(false);
        switching.set(None);
        back.call(());
    };
    let switch = move |to: Version, save: bool| {
        spawn(async move {
            if save && let Err(e) = run_behind("SESSION_SAVE", String::new()).await {
                said.set(Some((false, format!("not saved, so not switched: {e}"))));
                switching.set(None);
                return;
            }
            leave(to);
        });
    };
    let mut run = move |id: &'static str, args: String| {
        said.set(Some((true, "…".to_owned())));
        spawn(async move {
            let outcome = run_behind(id, args).await;
            said.set(Some(match outcome {
                Ok(text) => (true, text),
                Err(e) => (false, e),
            }));
            refresh();
        });
    };
    let mut checkpoint = move || {
        let wanted = name.peek().trim().to_owned();
        if wanted.is_empty() {
            said.set(Some((false, "a checkpoint needs a name".to_owned())));
            return;
        }
        run(
            "SESSION_CHECKPOINT",
            serde_json::json!({ "name": wanted }).to_string(),
        );
        name.set(String::new());
    };
    let (lit, ink) = if open() {
        (RAISED, TEXT)
    } else {
        ("transparent", DIM)
    };
    rsx! {
        div {
            style: "position:relative; flex:none; display:flex; align-items:stretch;",
            onmousedown: move |event| event.stop_propagation(),
            button {
                title: "Save the song, or a checkpoint of it (Ctrl+S saves)",
                style: "flex:none; width:44px; display:flex; align-items:center; justify-content:center; \
                        padding:0; border:none; background:{lit}; color:{ink}; cursor:pointer;",
                onclick: move |_| {
                    let now = !open();
                    open.set(now);
                    if now {
                        said.set(None);
                        refresh();
                    }
                },
                lucide_dioxus::History { size: 18, color: ink }
            }
            if open() {
                div {
                    style: "position:absolute; top:{BAR_H}px; right:0; width:320px; z-index:40; \
                            display:flex; flex-direction:column; gap:10px; padding:12px; \
                            background:{BAR_BG}; border:1px solid {RULE}; border-radius:0 0 10px 10px; \
                            color:{TEXT}; font-size:13px;",
                    button {
                        style: "height:34px; border-radius:8px; border:1px solid {RULE}; background:{RAISED}; \
                                color:{TEXT}; font-family:inherit; font-size:13px; font-weight:600; cursor:pointer;",
                        onclick: move |_| run("SESSION_SAVE", String::new()),
                        "Save the song  ·  Ctrl+S"
                    }
                    match unsaved() {
                        Some(true) => rsx! { span { style: "color:{NO}; font-size:12px;", "● Unsaved changes" } },
                        Some(false) => rsx! { span { style: "color:{OK}; font-size:12px;", "Saved" } },
                        None => rsx! {},
                    }
                    span { style: "color:{DIM}; font-size:12px; line-height:1.4;",
                        "A checkpoint saves the song as it is now, beside it, under a name — and keeps you in the song."
                    }
                    div {
                        style: "display:flex; gap:8px;",
                        input {
                            style: "flex:1; min-width:0; height:32px; padding:0 10px; border-radius:8px; \
                                    border:1px solid {RULE}; background:#0f1012; color:{TEXT}; \
                                    font-family:inherit; font-size:13px;",
                            placeholder: "Name — Organized, Mix A…",
                            value: "{name}",
                            oninput: move |event| name.set(event.value()),
                            onkeydown: move |event| {
                                if event.key() == Key::Enter {
                                    checkpoint();
                                }
                            },
                        }
                        button {
                            style: "flex:none; height:32px; padding:0 12px; border-radius:8px; border:none; \
                                    background:#3aa0ff; color:#0b0c0e; font-family:inherit; font-size:13px; \
                                    font-weight:650; cursor:pointer;",
                            onclick: move |_| checkpoint(),
                            "Checkpoint"
                        }
                    }
                    if let Some((good, text)) = said() {
                        span {
                            style: "font-size:12px; line-height:1.4; color:{tone(good)}; overflow-wrap:anywhere;",
                            "{text}"
                        }
                    }
                    if !listed().is_empty() {
                        div {
                            style: "display:flex; flex-direction:column; border-top:1px solid {RULE}; padding-top:8px; gap:2px;",
                            span { style: "color:{DIM}; font-size:11px; text-transform:uppercase; letter-spacing:0.06em; padding-bottom:4px;", "Versions" }
                            for version in listed() {
                                button {
                                    key: "{version.path.display()}",
                                    style: "display:flex; justify-content:space-between; gap:8px; height:30px; padding:0 8px; \
                                            border-radius:6px; border:none; text-align:left; font-family:inherit; font-size:13px; \
                                            background:{row_bg(version.open)}; color:{TEXT}; cursor:pointer;",
                                    disabled: version.open,
                                    onclick: move |_| {
                                        // Nothing to lose: straight there.
                                        if unsaved() == Some(false) {
                                            switch(version.clone(), false);
                                        } else {
                                            switching.set(Some(version.clone()));
                                        }
                                    },
                                    span { "{version.name}" }
                                    span { style: "color:{DIM};", if version.open { "open" } else { "{version.when}" } }
                                }
                            }
                        }
                    }
                    if let Some(to) = switching() {
                        div {
                            style: "display:flex; flex-direction:column; gap:8px; border-top:1px solid {RULE}; padding-top:8px;",
                            span { style: "line-height:1.4;", "Open {to.name}? Save this version first, or leave it as it was last saved." }
                            div {
                                style: "display:flex; gap:8px;",
                                button {
                                    style: "flex:1; height:32px; border-radius:8px; border:none; background:#3aa0ff; color:#0b0c0e; \
                                            font-family:inherit; font-size:13px; font-weight:650; cursor:pointer;",
                                    onclick: {
                                        let to = to.clone();
                                        move |_| switch(to.clone(), true)
                                    },
                                    "Save and open"
                                }
                                button {
                                    style: "flex:1; height:32px; border-radius:8px; border:1px solid {RULE}; background:{RAISED}; \
                                            color:{TEXT}; font-family:inherit; font-size:13px; cursor:pointer;",
                                    onclick: move |_| switch(to.clone(), false),
                                    "Open"
                                }
                                button {
                                    style: "flex:none; height:32px; padding:0 10px; border-radius:8px; border:none; background:transparent; \
                                            color:{DIM}; font-family:inherit; font-size:13px; cursor:pointer;",
                                    onclick: move |_| switching.set(None),
                                    "Cancel"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
