//! Action buttons for Signal management UI

use dioxus::prelude::*;
use signal::profile::{Patch, PatchId, Profile, ProfileId};
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

/// Capture button - captures current REAPER FX chain
#[component]
pub fn CaptureButton() -> Element {
    let _signal = crate::use_signal_service();

    rsx! {
        button {
            class: "px-3 py-1 text-xs rounded bg-purple-600 hover:bg-purple-700 text-white font-medium",
            onclick: move |_| {
                spawn(async move {
                    tracing::info!("Capture from REAPER - TODO: wire to DAW.current_track().get_chunk()");
                });
            },
            "Capture"
        }
    }
}
