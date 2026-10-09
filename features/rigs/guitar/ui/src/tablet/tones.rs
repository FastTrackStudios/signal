//! TONE3000 in the sidebar: captures for the slot the FX row's bar opened
//! (an amp, a drive slot), searched, a tap downloading one and loading it
//! there — on the playing patch, heard at once.
//!
//! Signing in is the FastTrackStudio account's (the account holds the
//! TONE3000 link, so this device needs no TONE3000 authorization of its
//! own); its page opens through the shell's `UrlOpener`.

use dioxus::prelude::*;
use signal_account_proto::account::AccountAuthClient;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CaptureImport, ImportOutcome};
use signal_tone3000_proto::tone3000::Tone3000Client;
use signal_tone3000_proto::{PickedTone, ToneQuery, ToneSummary};
use signal_tone3000_ui::{ToneArt, UrlOpener, use_art_cache, use_tone3000_state};

use super::tokens::*;

/// Whether this build carries TONE3000 (the client is provided).
pub fn available() -> bool {
    try_consume_context::<Tone3000Client>().is_some()
}

/// The slot a block (or module) of the FX row's bar loads captures into:
/// an amp, a drive slot.
pub fn slot_of(block: &str, block_type: &str) -> Option<String> {
    let t = block_type.to_lowercase();
    if t == "module:amp" {
        return Some("Amp L".into());
    }
    if t == "amp" || t == "drive" || t == "boost" {
        return Some(block.to_string());
    }
    // The Drive module: the drive block it was opened from, else its first.
    if t == "module:drive" {
        let b = block.to_lowercase();
        let slot = (b.starts_with("drive ") || b.starts_with("boost")).then(|| block.to_string()).unwrap_or_else(|| "Drive 1".into());
        return Some(slot);
    }
    None
}

/// The catalog's gear for a slot: what a search there should find.
pub(super) fn gears_for(slot: &str) -> Vec<String> {
    let s = slot.to_lowercase();
    if s.starts_with("drive") || s.starts_with("boost") {
        vec!["pedal".into()]
    } else {
        vec!["amp".into(), "full-rig".into(), "amp-cab".into()]
    }
}

#[component]
pub fn TonesPanel(slot: String) -> Element {
    let client = use_hook(try_consume_context::<Tone3000Client>);
    let account = use_hook(try_consume_context::<AccountAuthClient>);
    let opener = try_use_context::<UrlOpener>();
    let rig = use_hook(try_consume_context::<RigClient>);
    use_art_cache();
    let state = use_tone3000_state();
    let mut status = state.status;
    let downloads = state.downloads;
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<ToneSummary>::new);
    let mut error = use_signal(String::new);
    let mut searching = use_signal(|| false);
    let mut open = use_signal(|| None::<PickedTone>);
    // A capture asked for: (tone, model id, model name), loaded once its
    // download lands.
    let mut wanted = use_signal(|| None::<(PickedTone, String, String)>);
    let mut outcome = use_signal(|| None::<ImportOutcome>);
    let mut waiting_sign_in = use_signal(|| false);

    // Search: on a change of the text (or the slot), and once at first.
    let slot_now = use_signal(|| slot.clone());
    if *slot_now.peek() != slot {
        let mut s = slot_now;
        s.set(slot.clone());
    }
    let run_search = {
        let client = client.clone();
        use_callback(move |()| {
            let client = client.clone();
            let gears = gears_for(&slot_now.peek());
            let text = query.peek().trim().to_string();
            searching.set(true);
            spawn(async move {
                if let Some(c) = client {
                    let q = ToneQuery { text, gears, format: "nam".into(), sort: if query.peek().trim().is_empty() { "trending".into() } else { String::new() }, page: 1, page_size: 30, architecture: String::new() };
                    match c.search(q).await {
                        Ok(page) => {
                            error.set(page.error.clone());
                            results.set(page.tones);
                        }
                        Err(e) => error.set(format!("{e:?}")),
                    }
                }
                searching.set(false);
            });
        })
    };
    {
        use_effect(use_reactive!(|slot| {
            let _ = slot;
            if status.read().signed_in {
                run_search(());
            }
        }));
    }
    // Signed in at last (the sheet's page done): search.
    {
        let client = client.clone();
        use_future(move || {
            let client = client.clone();
            async move {
                loop {
                    architect::platform::sleep(std::time::Duration::from_secs(2)).await;
                    if !*waiting_sign_in.peek() {
                        continue;
                    }
                    if let Some(c) = client.clone()
                        && let Ok(s) = c.status().await
                        && s.signed_in
                    {
                        waiting_sign_in.set(false);
                        status.set(s);
                        run_search(());
                    }
                }
            }
        });
    }
    // The download asked for landed: load it into the slot.
    use_effect(move || {
        let map = downloads.read();
        let Some((tone, model_id, model_name)) = wanted.peek().clone() else { return };
        let Some(p) = map.get(&model_id) else { return };
        if !p.done {
            return;
        }
        wanted.set(None);
        if !p.error.is_empty() {
            outcome.set(Some(ImportOutcome { ok: false, message: p.error.clone(), ..ImportOutcome::default() }));
            return;
        }
        let import = CaptureImport { name: model_name, path: p.path.clone(), gear: tone.gear.clone(), group: tone.name.clone(), slot: slot.clone() };
        let rig = rig.clone();
        spawn(async move {
            if let Some(r) = rig {
                match r.load_capture(import).await {
                    Ok(o) => outcome.set(Some(o)),
                    Err(e) => outcome.set(Some(ImportOutcome { ok: false, message: format!("{e:?}"), ..ImportOutcome::default() })),
                }
            }
        });
    });

    let signed_in = status.read().signed_in;
    let row = format!("width: 100%; display: flex; align-items: center; gap: 12px; min-height: 64px; padding: 8px 14px; border: none; border-bottom: 1px solid {RULE}; background: transparent; color: {INK}; text-align: left; font-family: inherit; cursor: pointer;");

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: {FONT}; color: {INK};",
            // What it did: loaded where, or why not.
            if let Some(o) = outcome() {
                super::captures::OutcomeBanner { outcome: o, on_dismiss: move |()| outcome.set(None) }
            }
            if !signed_in {
                // Sign in through the account.
                div { style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 24px;",
                    span { style: "font-size: 18px; font-weight: 750;", "TONE3000" }
                    button {
                        style: "height: 48px; padding: 0 22px; border: none; border-radius: {R}; background: #3f3f46; color: #ffffff; font-size: 16px; font-weight: 700; font-family: {FONT}; cursor: pointer;",
                        onclick: move |_| {
                            let (account, opener) = (account.clone(), opener.clone());
                            spawn(async move {
                                if let Some(a) = account
                                    && let Ok(req) = a.begin_sign_in().await
                                    && !req.authorize_url.is_empty()
                                {
                                    waiting_sign_in.set(true);
                                    if let Some(o) = opener {
                                        o.open(req.authorize_url);
                                    }
                                }
                            });
                        },
                        if waiting_sign_in() { "Waiting for sign-in…" } else { "Sign in with FastTrackStudio" }
                    }
                }
            } else if let Some(tone) = open() {
                // A tone's captures: a tap loads one.
                div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-bottom: 1px solid {RULE};",
                    button { style: "width: {HIT}px; height: {HIT}px; border: none; background: transparent; color: {INK_2}; font-size: 20px; cursor: pointer;", onclick: move |_| open.set(None), "‹" }
                    div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                        span { style: "font-size: 16px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{tone.name}" }
                        span { style: "font-size: 12px; color: {INK_3};", "{tone.creator} · {tone.license}" }
                    }
                }
                div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                    for m in tone.models.iter().cloned() {
                        {
                            let busy = wanted.read().as_ref().is_some_and(|w| w.1 == m.id);
                            let pct = downloads.read().get(&m.id).filter(|p| !p.done).map(|p| p.percent);
                            let client = client.clone();
                            let tone2 = tone.clone();
                            rsx! {
                                button {
                                    key: "{m.id}",
                                    style: "{row}",
                                    onclick: move |_| {
                                        wanted.set(Some((tone2.clone(), m.id.clone(), m.name.clone())));
                                        outcome.set(None);
                                        let (client, tid, mid) = (client.clone(), tone2.id.clone(), m.id.clone());
                                        spawn(async move {
                                            if let Some(c) = client {
                                                let _ = c.download_model(tid, mid).await;
                                            }
                                        });
                                    },
                                    span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: 600;", "{m.name}" }
                                    if let Some(p) = pct {
                                        span { style: "font-size: 13px; color: {INK_2};", "{p}%" }
                                    } else if busy {
                                        span { style: "font-size: 13px; color: {INK_2};", "…" }
                                    } else {
                                        span { style: "font-size: 13px; color: {INK_3};", "Load" }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // Search, and the tones.
                div { style: "flex-shrink: 0; padding: 10px 12px; border-bottom: 1px solid {RULE};",
                    label { style: "display: flex; align-items: center; gap: 8px; height: 42px; padding: 0 12px; border-radius: {R}; background: {FILL};",
                        input {
                            value: "{query}",
                            placeholder: "Search TONE3000",
                            style: "flex: 1; min-width: 0; height: 100%; border: none; background: transparent; color: {INK}; font-size: 15px; font-family: {FONT};",
                            oninput: move |e| query.set(e.value()),
                            onkeydown: move |e: KeyboardEvent| if e.key() == Key::Enter { run_search(()) },
                        }
                        button { style: "height: {HIT}px; padding: 0 10px; border: none; border-radius: 6px; background: rgba(255,255,255,0.1); color: {INK}; font-size: 13px; font-weight: 650; font-family: {FONT}; cursor: pointer;", onclick: move |_| run_search(()), "Search" }
                    }
                }
                div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                    if searching() {
                        div { style: "padding: 20px; color: {INK_3}; font-size: 14px;", "Searching…" }
                    }
                    if !error().is_empty() {
                        div { style: "padding: 16px; color: #fca5a5; font-size: 14px;", "{error}" }
                    }
                    for t in results().into_iter() {
                        {
                            let client = client.clone();
                            let id = t.id.clone();
                            rsx! {
                                button {
                                    key: "{t.id}",
                                    style: "{row}",
                                    onclick: move |_| {
                                        let (client, id) = (client.clone(), id.clone());
                                        spawn(async move {
                                            if let Some(c) = client
                                                && let Ok(tone) = c.tone(id).await
                                            {
                                                if tone.error.is_empty() {
                                                    open.set(Some(tone));
                                                } else {
                                                    error.set(tone.error);
                                                }
                                            }
                                        });
                                    },
                                    div { style: "width: 52px; height: 52px; flex-shrink: 0; border-radius: 8px; overflow: hidden;",
                                        ToneArt { url: t.image.clone(), height: "52px".to_string(), label: t.title.clone() }
                                    }
                                    div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                                        span { style: "font-size: 15px; font-weight: 650; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{t.title}" }
                                        span { style: "font-size: 12px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{t.creator} · {t.models_count} · {t.gear}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
