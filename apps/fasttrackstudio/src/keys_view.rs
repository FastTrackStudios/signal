//! The phone keys rig — a portrait shell over the in-process sampler.
//!
//! Two pages behind a top tab bar:
//!
//! - **Play**: engine status + the downloaded preset list (tap to load)
//!   + an on-screen test key strip. Real playing comes from a hardware
//!   MIDI keyboard on the phone (CoreMIDI: USB-C / BLE / network).
//! - **Library**: the pack host connection (the studio engine or a
//!   hosted mirror, ws URL or iroh endpoint id) and the host's proxy
//!   pack list with download + resume + progress. Finished downloads
//!   land in Documents/FastTrackStudio/Packs/Keys, the keys rig
//!   rescans, and the preset appears on Play.
//!
//! Speaks `signal-keys-proto` directly (no signal-keys-ui — its
//! desktop render deps don't build for iOS), polling status at meter
//! rate the way the watch bridge does.

use std::collections::HashMap;

use dioxus::prelude::*;
use futures_util::StreamExt as _;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::{KeysPreset, KeysStatus};
use signal_packs_proto::PackInfo;

use crate::pack_client::{self, DownloadEvent};
use crate::remote::EngineTarget;

/// Which keys page is showing.
#[derive(Clone, Copy, PartialEq)]
enum KeysPage {
    Play,
    Library,
}

/// One pack's download lifecycle, keyed by pack name.
#[derive(Clone, PartialEq)]
enum DlState {
    Running { done: u64, total: u64 },
    Done,
    Failed(String),
}

/// The keys shell: tab bar + page, portrait.
#[component]
pub fn KeysShell(on_home: EventHandler<()>) -> Element {
    #[cfg(target_os = "ios")]
    use_hook(crate::ios_orientation::portrait);

    let keys = use_hook(try_consume_context::<KeysRigClient>);
    let mut page = use_signal(|| KeysPage::Play);
    let mut status = use_signal(KeysStatus::default);
    let mut presets = use_signal(Vec::<KeysPreset>::new);

    // Status + preset poll — local in-process calls, cheap at meter rate.
    {
        let keys = keys.clone();
        use_future(move || {
            let keys = keys.clone();
            async move {
                let Some(keys) = keys else { return };
                loop {
                    if let Ok(s) = keys.status().await {
                        status.set(s);
                    }
                    if let Ok(p) = keys.presets().await {
                        presets.set(p);
                    }
                    architect::platform::sleep(std::time::Duration::from_millis(600)).await;
                }
            }
        });
    }

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden;",
            // Top bar: back + title + page tabs.
            div { style: "display: flex; align-items: center; gap: 10px; padding: 10px 14px 6px;",
                button {
                    style: "appearance: none; background: transparent; border: none; \
                            color: #71717a; font-size: 15px; font-weight: 600; padding: 4px 6px 4px 0;",
                    onclick: move |_| on_home.call(()),
                    "‹ Home"
                }
                span { style: "font-size: 16px; font-weight: 700; color: #e4e4e7;", "Keys" }
                div { style: "flex: 1;" }
                for (p, label) in [(KeysPage::Play, "Play"), (KeysPage::Library, "Library")] {
                    button {
                        style: format!(
                            "appearance: none; border: none; border-radius: 8px; padding: 6px 14px; \
                             font-size: 13px; font-weight: 600; background: {}; color: {};",
                            if page() == p { "#101821" } else { "transparent" },
                            if page() == p { "#38bdf8" } else { "#52525b" },
                        ),
                        onclick: move |_| page.set(p),
                        "{label}"
                    }
                }
            }
            match page() {
                KeysPage::Play => rsx! {
                    PlayPage { status: status(), presets: presets() }
                },
                KeysPage::Library => rsx! {
                    LibraryPage {
                        downloaded: presets().iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                    }
                },
            }
        }
    }
}

// ── Play ────────────────────────────────────────────────────────────────────

#[component]
fn PlayPage(status: KeysStatus, presets: Vec<KeysPreset>) -> Element {
    let keys = use_hook(try_consume_context::<KeysRigClient>);
    let loaded = status.loaded_preset.clone().unwrap_or_default();
    let peak_pct = (status.master_peak.clamp(0.0, 1.0) * 100.0) as u32;

    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 8px 14px 20px; \
                      display: flex; flex-direction: column; gap: 12px;",
            // Status strip: engine LED + loaded patch + peak.
            div { style: "display: flex; align-items: center; gap: 10px; padding: 10px 12px; \
                          background: #131316; border: 1px solid #1f1f23; border-radius: 12px;",
                span {
                    style: format!(
                        "width: 9px; height: 9px; border-radius: 999px; background: {}; box-shadow: 0 0 8px {};",
                        if status.running { "#22c55e" } else { "#3f3f46" },
                        if status.running { "#22c55e88" } else { "transparent" },
                    )
                }
                span { style: "font-size: 13px; font-weight: 600; color: #e4e4e7;",
                    if loaded.is_empty() { "No patch loaded" } else { "{loaded}" }
                }
                div { style: "flex: 1;" }
                match &status.midi_port {
                    Some(port) => rsx! {
                        span { style: "font-size: 10px; color: #38bdf8;", "MIDI: {port}" }
                    },
                    None => rsx! {
                        span { style: "font-size: 10px; color: #52525b;", "MIDI: all inputs" }
                    },
                }
            }
            // Peak meter.
            div { style: "height: 4px; border-radius: 2px; background: #1b1b1f; overflow: hidden;",
                div { style: "height: 100%; width: {peak_pct}%; background: #22c55e;" }
            }
            if !status.running {
                button {
                    style: "appearance: none; border: none; border-radius: 12px; padding: 12px; \
                            background: linear-gradient(135deg, #10283f, #0b3a52); color: #7dd3fc; \
                            font-size: 14px; font-weight: 700;",
                    onclick: {
                        let keys = keys.clone();
                        move |_| {
                            if let Some(keys) = keys.clone() {
                                spawn(async move { let _ = keys.start().await; });
                            }
                        }
                    },
                    "Start audio"
                }
            }
            // Presets.
            span { style: "font-size: 11px; font-weight: 600; letter-spacing: 0.12em; \
                           text-transform: uppercase; color: #52525b; padding-top: 4px;",
                "Patches"
            }
            if presets.is_empty() {
                div { style: "padding: 18px; text-align: center; color: #52525b; font-size: 13px; \
                              border: 1px dashed #1f1f23; border-radius: 12px;",
                    "No packs downloaded yet — grab the pianos from the Library tab."
                }
            }
            for (i, preset) in presets.iter().enumerate() {
                {
                    let active = preset.loaded;
                    let keys = keys.clone();
                    let name = preset.name.clone();
                    let kind = preset.kind.clone();
                    rsx! {
                        button {
                            style: format!(
                                "appearance: none; text-align: left; border-radius: 12px; padding: 12px 14px; \
                                 display: flex; align-items: center; gap: 10px; border: 1px solid {}; \
                                 background: {}; color: #e4e4e7;",
                                if active { "#38bdf8" } else { "#1f1f23" },
                                if active { "#101821" } else { "#131316" },
                            ),
                            onclick: move |_| {
                                if let Some(keys) = keys.clone() {
                                    let i = i as u32;
                                    spawn(async move { let _ = keys.load_preset(i).await; });
                                }
                            },
                            div { style: "display: flex; flex-direction: column; gap: 2px;",
                                span { style: "font-size: 14px; font-weight: 600;", "{name}" }
                                span { style: "font-size: 11px; color: #71717a;", "{kind}" }
                            }
                            div { style: "flex: 1;" }
                            if active {
                                span { style: "font-size: 10px; font-weight: 700; color: #38bdf8; \
                                               letter-spacing: 0.1em;", "LOADED" }
                            }
                        }
                    }
                }
            }
            // Test keys: two octaves of white keys — enough to hear a patch
            // without a keyboard attached.
            span { style: "font-size: 11px; font-weight: 600; letter-spacing: 0.12em; \
                           text-transform: uppercase; color: #52525b; padding-top: 4px;",
                "Test keys"
            }
            div { style: "display: flex; gap: 3px;",
                for note in [48u32, 50, 52, 53, 55, 57, 59, 60, 62, 64, 65, 67, 69, 71, 72] {
                    {
                        let keys = keys.clone();
                        let keys_up = keys.clone();
                        rsx! {
                            button {
                                style: "flex: 1; height: 88px; border-radius: 0 0 6px 6px; border: 1px solid #27272a; \
                                        background: #fafafa; touch-action: none;",
                                onpointerdown: move |_| {
                                    if let Some(keys) = keys.clone() {
                                        spawn(async move { let _ = keys.trigger(note, 100).await; });
                                    }
                                },
                                onpointerup: move |_| {
                                    if let Some(keys) = keys_up.clone() {
                                        spawn(async move { let _ = keys.trigger(note, 0).await; });
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Library ─────────────────────────────────────────────────────────────────

#[component]
fn LibraryPage(downloaded: Vec<String>) -> Element {
    let keys = use_hook(try_consume_context::<KeysRigClient>);
    // The pack host address: a ws URL ("ws://192.168.1.20:4040/vox") or an
    // iroh endpoint id for p2p across networks. Saved via prefs, read by
    // EngineTarget::current().
    let mut host = use_signal(|| {
        crate::remote::engine_iroh_id()
            .map(|id| id.to_string())
            .or_else(|| crate::prefs::get("signal-engine-ws-url"))
            .unwrap_or_default()
    });
    let mut packs = use_signal(Vec::<PackInfo>::new);
    let mut note = use_signal(String::new);
    let mut downloads = use_signal(HashMap::<String, DlState>::new);
    // Where the listed packs came from — downloads use the same source.
    let mut source = use_signal(|| None::<pack_client::PackSource>);

    let refresh = use_callback(move |_: ()| {
        note.set("Connecting…".into());
        spawn(async move {
            // The phone wants streaming variants of keys packs.
            let keys_proxy = |list: Vec<PackInfo>| -> Vec<PackInfo> {
                list.into_iter()
                    .filter(|p| p.variant == "proxy" && p.category.starts_with("Keys"))
                    .collect()
            };
            // Preferred: the vox pack host (studio engine, p2p or LAN).
            let target = EngineTarget::current();
            let host_err = match pack_client::fetch_packs(target.clone()).await {
                Ok(Ok(list)) => {
                    source.set(Some(pack_client::PackSource::Vox(target.clone())));
                    let list = keys_proxy(list);
                    note.set(if list.is_empty() {
                        "Host reachable — no proxy keys packs published.".into()
                    } else {
                        format!("Connected: {}", target.label())
                    });
                    packs.set(list);
                    return;
                }
                Ok(Err(e)) => e,
                Err(_) => "connection cancelled".into(),
            };
            // Backup: the HTTPS mirror on fasttrackstudio.app.
            match pack_client::fetch_mirror_packs().await {
                Ok(Ok(list)) => {
                    source.set(Some(pack_client::PackSource::Mirror));
                    packs.set(keys_proxy(list));
                    note.set("Connected: fasttrackstudio.app mirror (host offline)".into());
                }
                Ok(Err(mirror_err)) => {
                    source.set(None);
                    note.set(format!("{host_err} · {mirror_err}"));
                }
                Err(_) => {
                    source.set(None);
                    note.set(format!("{host_err} · mirror cancelled"));
                }
            }
        });
    });

    // First entry: list immediately when a host is already saved.
    use_hook(|| {
        if !host().is_empty() {
            refresh.call(());
        }
    });

    let on_save = move |_| {
        let raw = host().trim().to_string();
        if raw.starts_with("ws://") || raw.starts_with("wss://") {
            crate::prefs::set("signal-engine-ws-url", &raw);
            crate::remote::store_engine_iroh_id(None);
        } else if !raw.is_empty() {
            // Not a URL — treat as an iroh endpoint id.
            crate::remote::store_engine_iroh_id(Some(&raw));
        }
        refresh.call(());
    };

    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 8px 14px 20px; \
                      display: flex; flex-direction: column; gap: 12px;",
            span { style: "font-size: 11px; font-weight: 600; letter-spacing: 0.12em; \
                           text-transform: uppercase; color: #52525b;",
                "Pack host"
            }
            div { style: "display: flex; gap: 8px;",
                input {
                    style: "flex: 1; min-width: 0; background: #131316; border: 1px solid #1f1f23; \
                            border-radius: 10px; padding: 10px 12px; color: #e4e4e7; font-size: 13px;",
                    placeholder: "ws://studio:4040/vox or iroh endpoint id",
                    value: "{host}",
                    oninput: move |e| host.set(e.value()),
                }
                button {
                    style: "appearance: none; border: none; border-radius: 10px; padding: 10px 16px; \
                            background: #101821; color: #38bdf8; font-size: 13px; font-weight: 700;",
                    onclick: on_save,
                    "Connect"
                }
            }
            if !note().is_empty() {
                span { style: "font-size: 12px; color: #a1a1aa;", "{note}" }
            }
            for info in packs() {
                {
                    let name = info.name.clone();
                    let size_gb = info.size_bytes as f64 / 1e9;
                    let dl = downloads().get(&name).cloned();
                    let already = downloaded.contains(&name) || matches!(dl, Some(DlState::Done));
                    let keys = keys.clone();
                    rsx! {
                        div { style: "display: flex; flex-direction: column; gap: 8px; padding: 12px 14px; \
                                      background: #131316; border: 1px solid #1f1f23; border-radius: 12px;",
                            div { style: "display: flex; align-items: center; gap: 10px;",
                                div { style: "display: flex; flex-direction: column; gap: 2px;",
                                    span { style: "font-size: 14px; font-weight: 600; color: #e4e4e7;", "{name}" }
                                    span { style: "font-size: 11px; color: #71717a;",
                                        {format!("{} · {size_gb:.2} GB · streaming proxy", info.category)}
                                    }
                                }
                                div { style: "flex: 1;" }
                                if already {
                                    span { style: "font-size: 10px; font-weight: 700; color: #22c55e; \
                                                   letter-spacing: 0.1em;", "ON DEVICE" }
                                } else if !matches!(dl, Some(DlState::Running { .. })) {
                                    button {
                                        style: "appearance: none; border: none; border-radius: 8px; \
                                                padding: 8px 14px; background: #101821; color: #38bdf8; \
                                                font-size: 12px; font-weight: 700;",
                                        onclick: {
                                            let info = info.clone();
                                            let keys = keys.clone();
                                            move |_| {
                                                let info = info.clone();
                                                let keys = keys.clone();
                                                let name = info.name.clone();
                                                downloads.write().insert(
                                                    name.clone(),
                                                    DlState::Running { done: 0, total: info.size_bytes },
                                                );
                                                let dl_source = source()
                                                    .unwrap_or_else(|| {
                                                        pack_client::PackSource::Vox(
                                                            EngineTarget::current(),
                                                        )
                                                    });
                                                let mut rx = pack_client::start_download(
                                                    dl_source,
                                                    info,
                                                    pack_client::keys_packs_dir(),
                                                );
                                                spawn(async move {
                                                    while let Some(ev) = rx.next().await {
                                                        match ev {
                                                            DownloadEvent::Progress { done, total } => {
                                                                downloads.write().insert(
                                                                    name.clone(),
                                                                    DlState::Running { done, total },
                                                                );
                                                            }
                                                            DownloadEvent::Done(_) => {
                                                                downloads.write().insert(name.clone(), DlState::Done);
                                                                // New pack on disk — teach the rig.
                                                                if let Some(keys) = keys.clone() {
                                                                    let _ = keys.rescan().await;
                                                                }
                                                            }
                                                            DownloadEvent::Failed(e) => {
                                                                downloads.write().insert(
                                                                    name.clone(),
                                                                    DlState::Failed(e),
                                                                );
                                                            }
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        "Download"
                                    }
                                }
                            }
                            match dl {
                                Some(DlState::Running { done, total }) => {
                                    let pct = if total > 0 { done * 100 / total } else { 0 };
                                    rsx! {
                                        div { style: "display: flex; align-items: center; gap: 8px;",
                                            div { style: "flex: 1; height: 5px; border-radius: 3px; \
                                                          background: #1b1b1f; overflow: hidden;",
                                                div { style: "height: 100%; width: {pct}%; background: #38bdf8;" }
                                            }
                                            span { style: "font-size: 10px; color: #71717a; min-width: 76px; text-align: right;",
                                                {format!("{:.1} / {:.1} GB", done as f64 / 1e9, total as f64 / 1e9)}
                                            }
                                        }
                                    }
                                }
                                Some(DlState::Failed(e)) => rsx! {
                                    span { style: "font-size: 11px; color: #ef4444;", "{e}" }
                                },
                                _ => rsx! {},
                            }
                        }
                    }
                }
            }
        }
    }
}
