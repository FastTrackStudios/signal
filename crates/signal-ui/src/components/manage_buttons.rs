//! Action buttons for Signal management UI

use dioxus::prelude::*;
use signal::profile::{Patch, PatchId, Profile, ProfileId};
use signal::rig::{Rig, RigId, RigScene, RigSceneId, RigType};
use signal::setlist::{Setlist, SetlistEntry, SetlistEntryId, SetlistId};
use signal::song::{Section, SectionId, Song, SongId};

/// Create Profile button - handles profile creation
#[component]
pub fn CreateProfileButton(on_created: Option<EventHandler<()>>) -> Element {
    let signal = crate::use_signal_service();

    rsx! {
        button {
            class: "px-2 py-0.5 text-[10px] rounded bg-blue-600 hover:bg-blue-700 text-white font-medium",
            onclick: move |_| {
                let signal = signal.clone();
                let on_created = on_created.clone();
                spawn(async move {
                    let rigs = signal.rigs().list().await.unwrap_or_default();
                    if let Some(r) = rigs.first() {
                        if let Some(s) = r.variants.first() {
                            let patch = Patch::from_rig_scene(PatchId::new(), "Clean", r.id.clone(), s.id.clone());
                            let profile = Profile::new(ProfileId::new(), "New Profile", patch);
                            let _ = signal.profiles().save(profile).await;
                            tracing::info!("Created new profile");
                            if let Some(handler) = on_created {
                                handler.call(());
                            }
                        }
                    }
                });
            },
            "+ Profile"
        }
    }
}

/// Create Song button - handles song creation
#[component]
pub fn CreateSongButton(on_created: Option<EventHandler<()>>) -> Element {
    let signal = crate::use_signal_service();

    rsx! {
        button {
            class: "px-2 py-0.5 text-[10px] rounded bg-blue-600 hover:bg-blue-700 text-white font-medium",
            onclick: move |_| {
                let signal = signal.clone();
                let on_created = on_created.clone();
                spawn(async move {
                    let profiles = signal.profiles().list().await.unwrap_or_default();
                    if let Some(prof) = profiles.first() {
                        if let Some(patch) = prof.patches.first() {
                            let section = Section::from_patch(SectionId::new(), "Intro", patch.id.clone());
                            let song = Song::new(SongId::new(), "New Song", section);
                            let _ = signal.songs().save(song).await;
                            tracing::info!("Created new song");
                            if let Some(handler) = on_created {
                                handler.call(());
                            }
                        }
                    }
                });
            },
            "+ Song"
        }
    }
}

/// Create Rig button - creates an empty rig with a default scene
#[component]
pub fn CreateRigButton(
    rig_type: RigType,
    on_created: Option<EventHandler<()>>,
    on_create_rig: Option<EventHandler<RigType>>,
) -> Element {
    let signal = crate::use_signal_service();

    rsx! {
        button {
            class: "px-2 py-0.5 text-[10px] rounded bg-blue-600 hover:bg-blue-700 text-white font-medium",
            onclick: move |_| {
                if let Some(ref handler) = on_create_rig {
                    handler.call(rig_type);
                    return;
                }
                let signal = signal.clone();
                let on_created = on_created.clone();
                let rt = rig_type;
                spawn(async move {
                    let rig = Rig::new(
                        RigId::new(),
                        "New Rig",
                        vec![],
                        RigScene::new(RigSceneId::new(), "Default"),
                    )
                    .with_rig_type(rt);
                    let _ = signal.rigs().save(rig).await;
                    tracing::info!("Created new rig");
                    if let Some(handler) = on_created {
                        handler.call(());
                    }
                });
            },
            "+ Rig"
        }
    }
}

/// Create Setlist button - creates a setlist with the first available song
#[component]
pub fn CreateSetlistButton(on_created: Option<EventHandler<()>>) -> Element {
    let signal = crate::use_signal_service();
    let mut editing = use_signal(|| false);
    let mut name_text = use_signal(String::new);

    let do_create = move || {
        let name = name_text().trim().to_string();
        if name.is_empty() {
            editing.set(false);
            return;
        }
        editing.set(false);
        name_text.set(String::new());
        let signal = signal.clone();
        let on_created = on_created.clone();
        spawn(async move {
            let songs = signal.songs().list().await.unwrap_or_default();
            if let Some(song) = songs.first() {
                let entry = SetlistEntry::new(SetlistEntryId::new(), &song.name, song.id.clone());
                let setlist = Setlist::new(SetlistId::new(), &name, entry);
                let _ = signal.setlists().save(setlist).await;
                tracing::info!("Created new setlist: {name}");
                if let Some(handler) = on_created {
                    handler.call(());
                }
            } else {
                tracing::warn!("No songs available to create setlist");
            }
        });
    };

    if editing() {
        let mut commit = do_create.clone();
        rsx! {
            input {
                class: "px-1.5 py-0.5 text-[10px] rounded bg-zinc-800 border border-zinc-600 text-zinc-200 outline-none w-24",
                placeholder: "Setlist name...",
                value: "{name_text}",
                autofocus: true,
                oninput: move |e| name_text.set(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    e.stop_propagation();
                    if e.key() == Key::Enter { commit(); }
                    if e.key() == Key::Escape { editing.set(false); name_text.set(String::new()); }
                },
                onfocusout: move |_| { editing.set(false); name_text.set(String::new()); },
            }
        }
    } else {
        rsx! {
            button {
                class: "px-2 py-0.5 text-[10px] rounded bg-blue-600 hover:bg-blue-700 text-white font-medium",
                onclick: move |_| {
                    editing.set(true);
                },
                "+ Setlist"
            }
        }
    }
}

/// Capture button - captures current REAPER FX chain.
/// The caller provides `on_capture` to handle the actual DAW interaction,
/// keeping this component free of DAW dependencies.
#[component]
pub fn CaptureButton(on_capture: Option<EventHandler<()>>) -> Element {
    rsx! {
        button {
            class: "px-3 py-1 text-xs rounded bg-purple-600 hover:bg-purple-700 text-white font-medium",
            onclick: move |_| {
                if let Some(ref handler) = on_capture {
                    handler.call(());
                } else {
                    tracing::info!("Capture: no handler wired");
                }
            },
            "Capture"
        }
    }
}
