//! Libraries — the home screen's place to bring a studio's sounds onto this
//! device: its drum kits, its keys packs, each with its size, whether it is
//! here, and a download that pauses, resumes and checks what it got.
//!
//! Everything comes from the studio's content host
//! ([`signal_content_proto::content::ContentLibrary`], over the same
//! WebSocket / iroh link the rig remotes use) and lands under
//! [`root`], laid out as on the studio — a kit's folder is the drum rig's
//! library as it stands, a keys pack is in the folder the keys rig scans.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;
use futures_util::StreamExt as _;
use signal_content_client::{TransferEvent, download_bundle};
use signal_content_proto::BundleInfo;
use signal_content_proto::content::ContentLibraryClient;

use crate::remote::EngineTarget;

/// This device's libraries (`signal_rig_host::store::libraries_dir`).
pub fn root() -> PathBuf {
    signal_rig_host::store::libraries_dir()
}

/// What a bundle's root file is, so "is it here?" is one look.
fn root_file(b: &BundleInfo) -> PathBuf {
    let id = b.id.split_once(':').map_or(b.id.as_str(), |(_, id)| id);
    id.split('/').fold(root(), |p, seg| p.join(seg))
}

#[derive(Clone, Debug, PartialEq)]
enum Dl {
    Running { done: u64, total: u64 },
    Paused { done: u64, total: u64 },
    Done,
    Failed(String),
}

/// The kinds the page lists, in order, with their headings.
const KINDS: [(&str, &str); 2] = [("drum-kit", "Drum kits"), ("keys-pack", "Keys")];

/// The phone (and iPad) wants the streaming twins: one row per bundle,
/// its proxy when the host has one.
fn rows(all: Vec<BundleInfo>) -> Vec<BundleInfo> {
    let mut out: Vec<BundleInfo> = Vec::new();
    for b in all {
        match out.iter_mut().find(|o| o.id == b.id) {
            Some(o) if b.variant == "proxy" => *o = b,
            Some(_) => {}
            None => out.push(b),
        }
    }
    out.sort_by(|a, b| (&a.kind, &a.library, &a.name).cmp(&(&b.kind, &b.library, &b.name)));
    out
}

fn mb(bytes: u64) -> String {
    let m = bytes as f64 / 1_048_576.0;
    if m >= 1024.0 {
        format!("{:.1} GB", m / 1024.0)
    } else {
        format!("{m:.0} MB")
    }
}

/// Keep the screen awake while anything downloads (iOS stops a backgrounded
/// app's transfers).
fn keep_awake(downloads: &HashMap<String, Dl>) {
    let active = downloads.values().any(|d| matches!(d, Dl::Running { .. }));
    #[cfg(target_os = "ios")]
    crate::ios_orientation::set_idle_timer_disabled(active);
    #[cfg(not(target_os = "ios"))]
    let _ = active;
}

/// A kit just arrived: the drum rig plays from its library.
fn use_kit(library: PathBuf) {
    #[cfg(all(feature = "signal-drums-rig", target_os = "ios"))]
    if let Some(d) = crate::rig_engine::engine().and_then(|e| e.drums_backend.clone()) {
        d.set_library(library);
        return;
    }
    tracing::info!(library = %library.display(), "libraries: kit downloaded");
}

#[component]
pub fn LibrariesPage(on_back: EventHandler<()>) -> Element {
    let mut client = use_signal(|| None::<ContentLibraryClient>);
    let mut bundles = use_signal(Vec::<BundleInfo>::new);
    let mut note = use_signal(String::new);
    let mut downloads = use_signal(HashMap::<String, Dl>::new);
    let mut cancels = use_signal(HashMap::<String, Arc<AtomicBool>>::new);
    let mut host = use_signal(|| {
        crate::remote::engine_iroh_id()
            .map(|id| id.to_string())
            .or_else(|| crate::prefs::get("signal-engine-ws-url"))
            .unwrap_or_default()
    });

    let refresh = use_callback(move |()| {
        let target = EngineTarget::current();
        note.set(format!("Connecting to {}…", target.label()));
        spawn(async move {
            let attempt = async {
                let c: ContentLibraryClient = crate::remote::establish_verbose(&target).await?;
                let list = c
                    .bundles(String::new())
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                Ok::<_, String>((c, list))
            };
            match attempt.await {
                Ok((c, list)) => {
                    let list = rows(list);
                    note.set(if list.is_empty() {
                        "The studio has nothing to give yet".into()
                    } else {
                        format!("Studio · {}", target.label())
                    });
                    bundles.set(list);
                    client.set(Some(c));
                }
                Err(e) => {
                    note.set(format!("Studio not reachable — {e}"));
                    client.set(None);
                }
            }
        });
    });
    use_hook(|| refresh.call(()));

    let begin = use_callback(move |b: BundleInfo| {
        let Some(c) = client() else { return };
        let id = b.id.clone();
        downloads.write().insert(
            id.clone(),
            Dl::Running {
                done: 0,
                total: b.size_bytes,
            },
        );
        keep_awake(&downloads());
        let (mut rx, cancel) = download_bundle(c, b.clone(), root());
        cancels.write().insert(id.clone(), cancel);
        spawn(async move {
            while let Some(ev) = rx.next().await {
                match ev {
                    TransferEvent::Progress { done, total } => {
                        downloads
                            .write()
                            .insert(id.clone(), Dl::Running { done, total });
                    }
                    TransferEvent::Paused { done, total } => {
                        downloads
                            .write()
                            .insert(id.clone(), Dl::Paused { done, total });
                    }
                    TransferEvent::Done(path) => {
                        downloads.write().insert(id.clone(), Dl::Done);
                        if b.kind == "drum-kit" {
                            use_kit(path);
                        }
                    }
                    TransferEvent::Failed(e) => {
                        downloads.write().insert(id.clone(), Dl::Failed(e));
                    }
                    TransferEvent::File { .. } => {}
                }
                keep_awake(&downloads());
            }
        });
    });

    let on_connect = move |_| {
        let raw = host().trim().to_string();
        if raw.starts_with("ws://") || raw.starts_with("wss://") {
            crate::prefs::set("signal-engine-ws-url", &raw);
            crate::remote::store_engine_iroh_id(None);
        } else if !raw.is_empty() {
            crate::remote::store_engine_iroh_id(Some(&raw));
        }
        refresh.call(());
    };

    let list = bundles();
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; background: #0b0b0d; color: #e4e4e7; font-family: -apple-system, 'SF Pro Text', system-ui, sans-serif;",
            // The way back, and where it all comes from.
            div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 12px; padding: 14px 18px; border-bottom: 1px solid #1f1f23;",
                button {
                    style: "height: 44px; display: flex; align-items: center; gap: 8px; padding: 0 12px 0 4px; border: none; background: transparent; color: #a1a1aa; font-size: 15px; font-weight: 650; cursor: pointer;",
                    onclick: move |_| on_back.call(()),
                    svg { width: "9", height: "15", view_box: "0 0 9 15",
                        path { d: "M7.5 1.5 1.5 7.5l6 6", fill: "none", stroke: "#a1a1aa", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                    }
                    "Instruments"
                }
                span { style: "font-size: 20px; font-weight: 800;", "Libraries" }
                span { style: "flex: 1;" }
                input {
                    style: "width: 340px; max-width: 40%; height: 40px; background: #131316; border: 1px solid #1f1f23; border-radius: 10px; padding: 0 12px; color: #e4e4e7; font-size: 13px;",
                    placeholder: "Studio: ws://host:4040/vox or iroh id",
                    value: "{host}",
                    oninput: move |e| host.set(e.value()),
                }
                button {
                    style: "height: 40px; padding: 0 16px; border: none; border-radius: 10px; background: #101821; color: #38bdf8; font-size: 14px; font-weight: 700; cursor: pointer;",
                    onclick: on_connect,
                    "Connect"
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 8px 18px 24px;",
                if !note().is_empty() {
                    div { style: "padding: 10px 0; font-size: 13px; color: #a1a1aa;", "{note}" }
                }
                for (kind, heading) in KINDS {
                    if list.iter().any(|b| b.kind == kind) {
                        div { key: "{kind}",
                            div { style: "padding: 18px 0 8px; font-size: 12px; font-weight: 750; letter-spacing: 0.1em; text-transform: uppercase; color: #71717a;", "{heading}" }
                            for b in list.iter().filter(|b| b.kind == kind).cloned() {
                                BundleRow {
                                    key: "{b.id}",
                                    here: root_file(&b).exists(),
                                    state: downloads().get(&b.id).cloned(),
                                    on_get: move |b: BundleInfo| begin.call(b),
                                    on_pause: move |id: String| {
                                        if let Some(c) = cancels().get(&id) {
                                            c.store(true, Ordering::Relaxed);
                                        }
                                    },
                                    bundle: b,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One kit or pack: what it is, how big, and here / downloading / get it.
#[component]
fn BundleRow(
    bundle: BundleInfo,
    here: bool,
    state: Option<Dl>,
    on_get: EventHandler<BundleInfo>,
    on_pause: EventHandler<String>,
) -> Element {
    let b = bundle.clone();
    let id = bundle.id.clone();
    let library = bundle
        .library
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let pct = |done: u64, total: u64| {
        if total == 0 {
            0
        } else {
            (done * 100 / total).min(100)
        }
    };
    let button = "height: 40px; min-width: 120px; padding: 0 16px; border: none; border-radius: 10px; font-size: 14px; font-weight: 700; cursor: pointer;";
    rsx! {
        div { style: "display: flex; align-items: center; gap: 14px; padding: 12px 0; border-bottom: 1px solid #1a1a1e;",
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                span { style: "font-size: 16px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{bundle.name}" }
                span { style: "font-size: 13px; color: #71717a;", "{library} · {mb(bundle.size_bytes)} · {bundle.files} files" }
                match &state {
                    Some(Dl::Running { done, total }) | Some(Dl::Paused { done, total }) => rsx! {
                        span { style: "height: 4px; border-radius: 2px; background: #1f1f23; overflow: hidden; margin-top: 4px;",
                            span { style: "display: block; height: 100%; width: {pct(*done, *total)}%; background: #38bdf8;" }
                        }
                    },
                    Some(Dl::Failed(e)) => rsx! { span { style: "font-size: 12px; color: #f87171;", "{e}" } },
                    _ => rsx! {},
                }
            }
            match state {
                Some(Dl::Running { done, total }) => rsx! {
                    button { style: "{button} background: #18181b; color: #e4e4e7;", onclick: move |_| on_pause.call(id.clone()), "Pause · {pct(done, total)}%" }
                },
                Some(Dl::Paused { .. }) | Some(Dl::Failed(_)) => rsx! {
                    button { style: "{button} background: #101821; color: #38bdf8;", onclick: move |_| on_get.call(b.clone()), "Resume" }
                },
                Some(Dl::Done) => rsx! { span { style: "min-width: 120px; text-align: center; font-size: 14px; font-weight: 700; color: #22c55e;", "On this device" } },
                None if here => rsx! { span { style: "min-width: 120px; text-align: center; font-size: 14px; font-weight: 700; color: #22c55e;", "On this device" } },
                None => rsx! {
                    button { style: "{button} background: #101821; color: #38bdf8;", onclick: move |_| on_get.call(b.clone()), "Download" }
                },
            }
        }
    }
}
