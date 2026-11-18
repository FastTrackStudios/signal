//! The rig's tempo — the engine's.
//!
//! One band, one tempo. With the process's engine installed
//! ([`crate::engine::install_shared`]) the tempo IS that engine's transport
//! tempo: Session's song tempo, or a rig's tap, which goes to the project
//! through the daw's transport service so the song shows it too. Without
//! one (tests, a rig on its own) it is a process-wide value as before.
//!
//! [`get`] is lock-free — the sampler reads it on the audio thread. Readers
//! poll [`generation`] to notice a change, whoever made it.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// BPM as f32 bits; 0 = none set (the value when no engine is installed).
static BPM: AtomicU32 = AtomicU32::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Set the tempo (clamped to 20..400 BPM). Control threads only: with an
/// engine installed this is a daw transport call.
pub fn set(bpm: f32) {
    let bpm = bpm.clamp(20.0, 400.0);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(engine) = crate::engine::shared() {
        use daw::service::transport::service::Transport as _;
        let project = daw::service::ProjectContext::Project(engine.project_guid().to_string());
        if let Err(e) = engine.daw().set_tempo(project, f64::from(bpm)) {
            tracing::warn!(error = %e, "rig tempo: setting the engine's tempo failed");
        }
    }
    if BPM.swap(bpm.to_bits(), Ordering::AcqRel) != bpm.to_bits() {
        GENERATION.fetch_add(1, Ordering::AcqRel);
    }
}

/// The tempo: the engine's when one is installed, else whatever was set.
/// Lock-free.
#[must_use]
pub fn get() -> Option<f32> {
    if let Some(transport) = crate::engine::shared_transport() {
        let bpm = transport.tempo_bpm();
        if bpm > 0.0 {
            return Some(bpm as f32);
        }
    }
    let bits = BPM.load(Ordering::Acquire);
    (bits != 0).then(|| f32::from_bits(bits))
}

/// Changes whenever the tempo does — set here, or by the engine's own
/// transport (Session changing the song's tempo).
#[must_use]
pub fn generation() -> u64 {
    let local = GENERATION.load(Ordering::Acquire);
    crate::engine::shared_transport()
        .map_or(local, |t| local ^ t.tempo_bpm().to_bits().rotate_left(17))
}
