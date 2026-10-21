//! What the front page shows besides the catalogue: each ready rig's live
//! state (what it opens on), tonight's set, and where the sound comes from —
//! read from the rig engine this device plays (in-process on the iPad, over
//! the engine link on the desktop), refreshed every few seconds.

use std::time::Duration;

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_rigs_proto::perform::service::RigPerformClient;

use crate::rigs::{FrontState, Rig, RigTile, SetInfo, SetSong};

/// How often the page re-reads the engines.
const REFRESH: Duration = Duration::from_secs(4);

/// The front page's state, kept fresh while the page is up.
pub fn use_front() -> Signal<FrontState> {
    let mut front = use_signal(FrontState::default);
    use_future(move || async move {
        let mut links = Links::default();
        loop {
            let next = links.gather().await;
            if *front.peek() != next {
                front.set(next);
            }
            architect::platform::sleep(REFRESH).await;
        }
    });
    front
}

/// The engine clients the page reads, made once and kept.
#[derive(Default)]
struct Links {
    guitar: Option<RigClient>,
    drums: Option<RigPerformClient>,
}

impl Links {
    /// This device's clients: the in-process engine's on the iPad, else ones
    /// dialled to the engine.
    async fn connect(&mut self) {
        #[cfg(target_os = "ios")]
        if let Some(e) = crate::rig_engine::engine() {
            self.guitar = Some(e.rig.clone());
            #[cfg(feature = "signal-drums-rig")]
            {
                self.drums = e.drums.as_ref().and_then(|d| d.perform.clone());
            }
            return;
        }
        #[cfg(not(target_os = "ios"))]
        {
            let target = crate::remote::EngineTarget::current();
            if self.guitar.is_none() {
                self.guitar = crate::remote::establish::<RigClient>(&target).await;
            }
            if self.drums.is_none() {
                self.drums = crate::remote::establish::<RigPerformClient>(&target)
                    .await
                    .map(|c| architect::scope_client!(c, Rig::Drums.slug()));
            }
        }
    }

    fn studio() -> String {
        #[cfg(target_os = "ios")]
        {
            format!("This {}", crate::ios_scene::device_name())
        }
        #[cfg(not(target_os = "ios"))]
        {
            match crate::remote::EngineTarget::current() {
                crate::remote::EngineTarget::Ws(url) if url.contains("127.0.0.1") || url.contains("localhost") => "This Mac".to_string(),
                t => t.label(),
            }
        }
    }

    async fn gather(&mut self) -> FrontState {
        if self.guitar.is_none() || self.drums.is_none() {
            self.connect().await;
        }
        let mut state = FrontState { studio: Self::studio(), ..FrontState::default() };
        if let Some(rig) = self.guitar.clone() {
            match rig.perf().await {
                Ok(perf) => {
                    state.connected = true;
                    let status = rig.status().await.ok();
                    let patch = status.as_ref().and_then(|s| s.active_patch.clone()).unwrap_or_default();
                    let opens_on = [perf.profile_name.clone(), patch].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
                    let live = status.is_some_and(|s| s.running);
                    // The guitar engine plays one instrument at a time: what it
                    // holds now is that instrument's; the other opens on its own.
                    let bass = perf.instrument.eq_ignore_ascii_case("bass");
                    let stacks = perf.stacks.iter().map(|st| (st.name.clone(), st.current_patch.clone(), st.is_active)).collect();
                    let tile = RigTile { opens_on, live, stacks };
                    state.tiles.push((if bass { Rig::Bass } else { Rig::Guitar }, tile));
                    state.set = tonight(&rig, &perf).await;
                }
                Err(_) => self.guitar = None,
            }
        }
        if let Some(drums) = self.drums.clone() {
            match drums.state().await {
                Ok(s) => {
                    // The kit loaded; with none, nothing (the row describes the
                    // rig — a profile's name says nothing about what plays).
                    let opens_on = s.active_patch.replace(" · Default", "");
                    // A kit's stack is its kit; what it lands on is the mix.
                    let stacks = s
                        .stacks
                        .iter()
                        .map(|st| (st.name.clone(), st.current_patch.rsplit(" · ").next().unwrap_or_default().to_string(), st.is_active))
                        .collect();
                    state.tiles.push((Rig::Drums, RigTile { opens_on, live: !s.active_patch.is_empty(), stacks }));
                }
                Err(_) => self.drums = None,
            }
        }
        state
    }
}

/// Tonight's set: the one up in the guitar rig, with its day.
async fn tonight(rig: &RigClient, perf: &signal_guitar_proto::PerformanceModel) -> Option<SetInfo> {
    if perf.songs.is_empty() {
        return None;
    }
    let idx = perf.setlist_index as usize;
    let entry = rig.library().await.ok().and_then(|l| l.setlists.get(idx).cloned());
    let name = entry
        .as_ref()
        .map(|e| if !e.title.is_empty() { e.title.clone() } else if !e.event.is_empty() { e.event.clone() } else { e.name.clone() })
        .or_else(|| perf.setlists.get(idx).cloned())
        .unwrap_or_default();
    let day = entry.map(|e| day_of(&e.date)).unwrap_or_default();
    Some(SetInfo {
        name,
        day,
        songs: perf.songs.iter().map(|s| SetSong { name: s.name.clone(), key: s.key.clone(), bpm: s.bpm }).collect(),
        up: perf.song_index as usize,
    })
}

/// `2026-10-06` → `6 Oct`.
fn day_of(iso: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mut parts = iso.trim().split('-');
    let (_, m, d) = (parts.next(), parts.next().and_then(|m| m.parse::<usize>().ok()), parts.next().and_then(|d| d.parse::<u32>().ok()));
    match (m, d) {
        (Some(m), Some(d)) if (1..=12).contains(&m) => format!("{d} {}", MONTHS[m - 1]),
        _ => String::new(),
    }
}

/// Make song `index` of tonight's set the one up, in the guitar rig.
pub fn play_song(index: usize) {
    // Forever, not the page's: the page closes as the guitar opens, and the
    // call must not go with it.
    let _ = dioxus_core::spawn_forever(async move {
        #[cfg(target_os = "ios")]
        let rig = crate::rig_engine::engine().map(|e| e.rig.clone());
        #[cfg(not(target_os = "ios"))]
        let rig = crate::remote::establish::<RigClient>(&crate::remote::EngineTarget::current()).await;
        if let Some(rig) = rig {
            match rig.select_song(index as u32).await {
                Ok(a) if !a.ok => tracing::warn!(index, reason = a.message, "front page: song refused"),
                Err(e) => tracing::warn!(index, "front page: song not sent: {e:?}"),
                Ok(_) => {}
            }
        }
    });
}

/// Put the guitar rig in Setlist mode (where sets are made).
pub fn open_setlists() {
    let _ = dioxus_core::spawn_forever(async move {
        #[cfg(target_os = "ios")]
        let rig = crate::rig_engine::engine().map(|e| e.rig.clone());
        #[cfg(not(target_os = "ios"))]
        let rig = crate::remote::establish::<RigClient>(&crate::remote::EngineTarget::current()).await;
        if let Some(rig) = rig
            && let Err(e) = rig.set_perform_mode(2).await
        {
            tracing::warn!("front page: setlist mode not sent: {e:?}");
        }
    });
}
