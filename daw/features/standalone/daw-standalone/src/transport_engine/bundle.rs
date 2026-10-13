//! Per-project transport bundle: engine + soft clock + subscriber pumps.
//!
//! Each project owns one [`TransportBundle`]:
//!
//! - `Arc<TransportShared>` — atomic state. Cloned into every place
//!   that needs read/write (transport service, future mixer, etc.).
//! - Soft clock task — drives [`TransportShared::advance`] from
//!   wall-clock when no audio callback is owning the engine. Lets
//!   playback work in tests / headless / pre-mixer-integration WASM.
//! - Subscriber pump tasks — one per `subscribe()` call, each emits
//!   `PositionTick`s at ~30Hz and `TransportEvent`s on transitions.
//!
//! When the bundle drops, all tasks abort.

use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::sync::{Arc, Mutex};

use crate::platform;
use architect::platform::{JoinHandle, spawn};

use super::clock::SampleClock;
use super::engine::{PlayStateRepr, TransportShared, TransportSnapshot};
use super::tempo_map::{DynamicTempoMap, StaticTempoMap};

/// Soft-clock tick period. 10ms = 100 Hz advance — plenty for UI even
/// without sample-accurate driving from the mixer.
const SOFT_TICK: Duration = Duration::from_millis(10);

/// UI position-tick period. ~30 Hz matches `daw-proto`'s documented
/// rate for `PositionTick`.
const UI_TICK: Duration = Duration::from_millis(33);

/// Per-project engine + tasks.
pub struct TransportBundle {
    pub shared: Arc<TransportShared>,
    /// Toggle the soft clock on/off. Audio callbacks can flip this off
    /// once they take over sample-accurate driving.
    pub soft_clock_enabled: Arc<AtomicBool>,
    /// Optional keyframed tempo map. When present, `snapshot()` uses
    /// it for the musical-position field; when absent, falls back to
    /// the engine's `tempo_bpm()` via [`StaticTempoMap`].
    dynamic_tempo: Mutex<Option<Arc<DynamicTempoMap>>>,
    _soft_clock: JoinHandle<()>,
}

impl core::fmt::Debug for TransportBundle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TransportBundle")
            .field("playhead_samples", &self.shared.playhead_samples().0)
            .field("play_state", &self.shared.play_state())
            .field(
                "soft_clock_enabled",
                &self.soft_clock_enabled.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl TransportBundle {
    /// Spawn the bundle. `sample_rate` is the engine's output sample
    /// rate (defaults to 48k for headless until a mixer attaches).
    pub fn spawn(sample_rate: u32, initial_bpm: f64) -> Self {
        let shared = Arc::new(TransportShared::new(sample_rate, initial_bpm));
        let soft_clock_enabled = Arc::new(AtomicBool::new(true));

        let task_shared = shared.clone();
        let task_enabled = soft_clock_enabled.clone();
        let soft_clock = spawn(async move {
            // The sync clock (native: daw-transport-sync's; web: the
            // browser's) — each tick is one "buffer" spanning
            // [last, now), stamped with its start so sync snapshots and
            // scheduled locates work with no device too.
            let mut last = crate::transport_sync::now_micros();
            loop {
                platform::sleep(SOFT_TICK).await;
                let now = crate::transport_sync::now_micros();
                let dt_micros = now - last;
                let stamp = last;
                last = now;

                if !task_enabled.load(Ordering::Relaxed) {
                    continue;
                }
                let sr = task_shared.sample_rate() as f64;
                let frames = (dt_micros * 1e-6 * sr).round() as i64;
                if frames > 0 && frames <= u32::MAX as i64 {
                    task_shared.begin_block(frames as u32, stamp, dt_micros);
                }
            }
        });

        Self {
            shared,
            soft_clock_enabled,
            dynamic_tempo: Mutex::new(None),
            _soft_clock: soft_clock,
        }
    }

    /// Install (or replace) the keyframed tempo map for this project.
    /// Pass `None` to revert to the engine's static BPM.
    pub fn set_dynamic_tempo(&self, map: Option<Arc<DynamicTempoMap>>) {
        *self.dynamic_tempo.lock().expect("dynamic tempo poisoned") = map;
    }

    /// Inspect the currently-installed dynamic tempo map.
    pub fn dynamic_tempo(&self) -> Option<Arc<DynamicTempoMap>> {
        self.dynamic_tempo
            .lock()
            .expect("dynamic tempo poisoned")
            .clone()
    }

    /// Disable the soft clock — call this when a sample-accurate
    /// driver (cpal callback / AudioWorklet) takes over.
    pub fn disable_soft_clock(&self) {
        self.soft_clock_enabled.store(false, Ordering::Relaxed);
    }

    /// Re-enable the soft clock — call when the audio driver detaches.
    pub fn enable_soft_clock(&self) {
        self.soft_clock_enabled.store(true, Ordering::Relaxed);
    }

    /// Snapshot the transport for the proto mirror / UI tick. Uses
    /// the dynamic tempo map if installed, otherwise the engine's
    /// static BPM. Honors `playrate` in the musical conversion.
    pub fn snapshot(&self) -> TransportSnapshot {
        let s = &*self.shared;
        let clock = SampleClock::new(s.sample_rate());
        let samples = s.playhead_samples();
        let seconds = clock.samples_to_seconds(samples);
        let speed = s.playrate();

        let musical = if let Some(dynamic) = self.dynamic_tempo() {
            dynamic.samples_to_musical(samples, speed, &clock)
        } else {
            let static_map = StaticTempoMap::new(s.tempo_bpm().max(1e-3));
            static_map.samples_to_musical(samples, speed, &clock)
        };

        TransportSnapshot {
            samples,
            seconds,
            musical,
            play_state: s.play_state(),
        }
    }
}

// ── Subscriber pump ────────────────────────────────────────────────────

/// Spawn a per-subscriber pump task. The task lives until either the
/// `Tx` is dropped/closed by the consumer or the returned `JoinHandle`
/// is aborted.
///
/// Pump duties:
/// - Emit an initial `TransportEvent::Snapshot` so the subscriber syncs
///   without re-querying.
/// - Every UI tick, emit `PositionTick` (if `filter.position`).
/// - On any field change (play_state / tempo / playrate / looping /
///   loop region), emit the corresponding `TransportEvent` (if
///   `filter.state`).
pub fn spawn_subscriber_pump(
    project_guid: String,
    shared: Arc<TransportShared>,
    initial_proto: daw_proto::Transport,
    filter: daw_proto::transport::TransportSubscription,
    tx: vox::Tx<daw_proto::transport::TransportStreamEvent>,
) -> JoinHandle<()> {
    use daw_proto::transport::{TransportEvent, TransportStreamEvent};

    spawn(async move {
        // Initial snapshot — clients expect it before any tick.
        if filter.state {
            let ev = TransportStreamEvent::State(TransportEvent::Snapshot {
                project_guid: project_guid.clone(),
                state: initial_proto.clone(),
            });
            if tx.send(ev).await.is_err() {
                return;
            }
        }

        let mut last_play_state = shared.play_state();
        let mut last_looping = shared.is_looping();
        let mut last_metronome = shared.metronome();
        let mut last_tempo_bpm = shared.tempo_bpm();
        let mut last_playrate = shared.playrate();

        loop {
            platform::sleep(UI_TICK).await;

            // ── State-change events ────────────────────────────────
            if filter.state {
                let cur_play = shared.play_state();
                if cur_play != last_play_state {
                    last_play_state = cur_play;
                    let ev = TransportStreamEvent::State(TransportEvent::PlayStateChanged {
                        project_guid: project_guid.clone(),
                        play_state: play_state_to_proto(cur_play),
                    });
                    if tx.send(ev).await.is_err() {
                        return;
                    }
                }
                let cur_loop = shared.is_looping();
                let cur_metro = shared.metronome();
                if cur_loop != last_looping {
                    last_looping = cur_loop;
                    let ev = TransportStreamEvent::State(TransportEvent::LoopingChanged {
                        project_guid: project_guid.clone(),
                        looping: cur_loop,
                    });
                    if tx.send(ev).await.is_err() {
                        return;
                    }
                }
                if cur_metro != last_metronome {
                    last_metronome = cur_metro;
                    let ev = TransportStreamEvent::State(TransportEvent::MetronomeChanged {
                        project_guid: project_guid.clone(),
                        enabled: cur_metro,
                    });
                    if tx.send(ev).await.is_err() {
                        return;
                    }
                }
                let cur_tempo = shared.tempo_bpm();
                if (cur_tempo - last_tempo_bpm).abs() > f64::EPSILON {
                    last_tempo_bpm = cur_tempo;
                    let ev = TransportStreamEvent::State(TransportEvent::TempoChanged {
                        project_guid: project_guid.clone(),
                        tempo: daw_proto::primitives::Tempo::from_bpm(cur_tempo),
                        time_signature: initial_proto.time_signature,
                    });
                    if tx.send(ev).await.is_err() {
                        return;
                    }
                }
                let cur_rate = shared.playrate();
                if (cur_rate - last_playrate).abs() > f64::EPSILON {
                    last_playrate = cur_rate;
                    let ev = TransportStreamEvent::State(TransportEvent::PlayrateChanged {
                        project_guid: project_guid.clone(),
                        playrate: cur_rate,
                    });
                    if tx.send(ev).await.is_err() {
                        return;
                    }
                }
            }

            // ── Position ticks ─────────────────────────────────────
            if filter.position {
                let snap = super::engine::TransportEngine {
                    shared: shared.clone(),
                }
                .snapshot();
                let pos = daw_proto::primitives::Position::from_time(
                    daw_proto::primitives::PositionInSeconds::from_seconds(snap.seconds.0),
                );
                // Carry the real project guid: consumers (e.g. session's
                // setlist events pump) map ticks → songs by guid, and an
                // empty guid silently drops every per-song TransportUpdate
                // (frozen musical position on all standalone remotes).
                let tick = daw_proto::transport::PositionTick {
                    project_guid: project_guid.clone(),
                    playhead: pos.clone(),
                    edit_cursor: pos,
                    is_playing: snap.play_state.is_advancing(),
                };
                if tx.send(TransportStreamEvent::Position(tick)).await.is_err() {
                    return;
                }
            }
        }
    })
}

fn play_state_to_proto(s: PlayStateRepr) -> daw_proto::PlayState {
    match s {
        PlayStateRepr::Stopped => daw_proto::PlayState::Stopped,
        PlayStateRepr::Playing => daw_proto::PlayState::Playing,
        PlayStateRepr::Paused => daw_proto::PlayState::Paused,
        PlayStateRepr::Recording => daw_proto::PlayState::Recording,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The playhead must NEVER freeze because the output device died. When the
    /// audio callback owns the transport it disables the soft clock (the
    /// callback drives `advance()`); if that device then dies the callback stops
    /// firing. The stream-error handler recovers by re-enabling the soft clock
    /// (a plain `enable_soft_clock()` / `soft_clock_enabled = true`) — this test
    /// exercises exactly that recovery and asserts the playhead resumes.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn soft_clock_recovery_unfreezes_playhead() {
        let bundle = TransportBundle::spawn(48_000, 120.0);
        bundle.shared.set_play_state(PlayStateRepr::Playing);

        // 1. Soft clock enabled → playhead advances from wall-clock.
        platform::sleep(Duration::from_millis(120)).await;
        let advanced = bundle.shared.playhead_samples().0;
        assert!(
            advanced > 0,
            "soft clock should advance the playhead while enabled"
        );

        // 2. Audio callback takes over → soft clock disabled. Simulating a dead
        //    device (no callback firing), the playhead must FREEZE.
        bundle.disable_soft_clock();
        platform::sleep(Duration::from_millis(40)).await; // let the in-flight tick settle
        let frozen_at = bundle.shared.playhead_samples().0;
        platform::sleep(Duration::from_millis(120)).await;
        assert_eq!(
            bundle.shared.playhead_samples().0,
            frozen_at,
            "with the soft clock disabled and no audio callback, the playhead must not advance"
        );

        // 3. Recovery: the stream-error callback re-enables the soft clock. The
        //    playhead must resume — never left frozen by a dead device.
        bundle.enable_soft_clock();
        platform::sleep(Duration::from_millis(120)).await;
        assert!(
            bundle.shared.playhead_samples().0 > frozen_at,
            "re-enabling the soft clock (the stream-error recovery) must resume the playhead"
        );
    }
}
