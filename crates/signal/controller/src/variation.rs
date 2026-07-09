//! Variation switch handlers for context-free action dispatch.
//!
//! These handlers implement the "Switch to Variation N" logic: they read the
//! [`ActiveContext`] to determine what's currently active (Profile, Rig, or Song),
//! resolve the Nth variant, and call the appropriate activate method.
//!
//! Handlers live here (in the controller crate) rather than in the extension or
//! UI because they use the DAW abstraction layer — the same handler works whether
//! invoked from a REAPER action, MIDI CC, keyboard shortcut, or RPC client.

use crate::active_context::ActiveContext;
use crate::{SignalApi, SignalController};
use signal_proto::profile::PatchId;
use signal_proto::rig::RigSceneId;
use signal_proto::song::SectionId;
use signal_proto::traits::{Collection, Variant};
use tracing::{info, warn};

/// Result of a variation switch attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum SwitchResult {
    /// Successfully switched to the Nth variation.
    Switched {
        /// Human-readable name of the variation that was activated.
        name: String,
    },
    /// No active context — nothing to switch.
    NoContext,
    /// The requested index is out of bounds for the active collection.
    OutOfBounds { requested: usize, available: usize },
    /// The active collection could not be loaded.
    LoadError(String),
    /// The activation call failed.
    ActivateError(String),
}

impl<S> SignalController<S>
where
    S: SignalApi + Send + Sync + 'static,
{
    /// Switch to the Nth variation (1-based) of the active collection.
    ///
    /// - Profile context → activates the Nth patch
    /// - Rig context → switches to the Nth scene
    /// - Song context → jumps to the Nth section
    ///
    /// Returns [`SwitchResult::NoContext`] if nothing is active, or
    /// [`SwitchResult::OutOfBounds`] if N exceeds the variant count.
    pub async fn switch_to_variation(&self, n: usize) -> SwitchResult {
        if n == 0 {
            return SwitchResult::OutOfBounds {
                requested: 0,
                available: 0,
            };
        }
        let index = n - 1; // Convert 1-based to 0-based

        let ctx = self.active_context.get();
        match ctx {
            ActiveContext::None => SwitchResult::NoContext,

            ActiveContext::Profile { id, .. } => {
                self.switch_profile_variation(id.clone(), index).await
            }

            ActiveContext::Rig { id, .. } => self.switch_rig_variation(id.clone(), index).await,

            ActiveContext::Song { id, .. } => self.switch_song_variation(id.clone(), index).await,
        }
    }

    /// Advance to the next variation within the active context.
    pub async fn next_variation(&self) -> SwitchResult {
        let ctx = self.active_context.get();
        match ctx.active_index() {
            Some(current) => self.switch_to_variation(current + 2).await, // +2: 0-based→1-based + 1
            None => SwitchResult::NoContext,
        }
    }

    /// Go back to the previous variation within the active context.
    pub async fn previous_variation(&self) -> SwitchResult {
        let ctx = self.active_context.get();
        match ctx.active_index() {
            Some(0) => SwitchResult::OutOfBounds {
                requested: 0,
                available: 0,
            },
            Some(current) => self.switch_to_variation(current).await, // current is 0-based, so this is prev+1 in 1-based
            None => SwitchResult::NoContext,
        }
    }

    // ── Profile ──────────────────────────────────────────────────

    async fn switch_profile_variation(
        &self,
        profile_id: signal_proto::profile::ProfileId,
        index: usize,
    ) -> SwitchResult {
        let profile = match self.service.load_profile(profile_id.clone()).await {
            Ok(Some(p)) => p,
            Ok(None) => return SwitchResult::LoadError(format!("profile {profile_id} not found")),
            Err(e) => return SwitchResult::LoadError(format!("loading profile: {e}")),
        };

        let patches = profile.variants();
        if index >= patches.len() {
            return SwitchResult::OutOfBounds {
                requested: index + 1,
                available: patches.len(),
            };
        }

        let patch = &patches[index];
        let patch_id: PatchId = patch.id().clone();
        let name = patch.name().to_string();

        info!("switching to profile patch {}: {}", index + 1, name);

        match self
            .profiles()
            .activate(profile_id.clone(), Some(patch_id))
            .await
        {
            Ok(_) => {
                self.active_context.set(ActiveContext::Profile {
                    id: profile_id,
                    active_index: index,
                });
                SwitchResult::Switched { name }
            }
            Err(e) => SwitchResult::ActivateError(format!("profile activate: {e}")),
        }
    }

    // ── Rig ──────────────────────────────────────────────────────

    async fn switch_rig_variation(
        &self,
        rig_id: signal_proto::rig::RigId,
        index: usize,
    ) -> SwitchResult {
        let rig = match self.service.load_rig(rig_id.clone()).await {
            Ok(Some(r)) => r,
            Ok(None) => return SwitchResult::LoadError(format!("rig {rig_id} not found")),
            Err(e) => return SwitchResult::LoadError(format!("loading rig: {e}")),
        };

        let scenes = rig.variants();
        if index >= scenes.len() {
            return SwitchResult::OutOfBounds {
                requested: index + 1,
                available: scenes.len(),
            };
        }

        let scene = &scenes[index];
        let scene_id: RigSceneId = scene.id().clone();
        let name = scene.name().to_string();

        info!("switching to rig scene {}: {}", index + 1, name);

        // Use the rig scene applier for preloaded switching (<5ms).
        // Clone the Arc out of the lock before the .await to avoid Send issues.
        let rig_applier = self.daw_rig_applier.read().expect("lock poisoned").clone();
        if let Some(rig_applier) = rig_applier {
            match rig_applier
                .switch_scene(&rig_id.to_string(), &scene_id.to_string(), Some(&name))
                .await
            {
                Ok(true) => {
                    self.active_context.set(ActiveContext::Rig {
                        id: rig_id,
                        active_index: index,
                    });
                    SwitchResult::Switched { name }
                }
                Ok(false) => {
                    warn!("rig scene {} not ready (still preloading)", name);
                    SwitchResult::ActivateError("scene not ready".into())
                }
                Err(e) => SwitchResult::ActivateError(format!("rig scene switch: {e}")),
            }
        } else {
            warn!("no rig scene applier attached — cannot switch rig scenes");
            SwitchResult::ActivateError("no rig scene applier".into())
        }
    }

    // ── Song ─────────────────────────────────────────────────────

    async fn switch_song_variation(
        &self,
        song_id: signal_proto::song::SongId,
        index: usize,
    ) -> SwitchResult {
        let song = match self.service.load_song(song_id.clone()).await {
            Ok(Some(s)) => s,
            Ok(None) => return SwitchResult::LoadError(format!("song {song_id} not found")),
            Err(e) => return SwitchResult::LoadError(format!("loading song: {e}")),
        };

        let sections = song.sections();
        if index >= sections.len() {
            return SwitchResult::OutOfBounds {
                requested: index + 1,
                available: sections.len(),
            };
        }

        let section = &sections[index];
        let _section_id: SectionId = section.id.clone();
        let name = section.name.clone();

        info!("switching to song section {}: {}", index + 1, name);

        // Clone appliers out of locks before any .await to avoid Send issues.
        let daw_applier = self.daw_applier.read().expect("lock poisoned").clone();
        let rig_applier = self.daw_rig_applier.read().expect("lock poisoned").clone();

        // Resolve the section's source to a concrete activation target
        match &section.source {
            signal_proto::song::SectionSource::Patch { patch_id } => {
                let target = signal_proto::resolve::ResolveTarget::SongSection {
                    song_id: song_id.clone(),
                    section_id: section.id.clone(),
                };
                match self.resolve_target(target).await {
                    Ok(graph) => {
                        if let Some(applier) = daw_applier {
                            let _ = applier.apply_graph(&graph, Some(&name)).await;
                        }
                        self.active_context.set(ActiveContext::Song {
                            id: song_id,
                            active_index: index,
                        });
                        SwitchResult::Switched { name }
                    }
                    Err(e) => SwitchResult::ActivateError(format!(
                        "resolve section patch {patch_id}: {e}"
                    )),
                }
            }
            signal_proto::song::SectionSource::RigScene { rig_id, scene_id } => {
                if let Some(rig_applier) = rig_applier {
                    match rig_applier
                        .switch_scene(&rig_id.to_string(), &scene_id.to_string(), Some(&name))
                        .await
                    {
                        Ok(_) => {
                            self.active_context.set(ActiveContext::Song {
                                id: song_id,
                                active_index: index,
                            });
                            SwitchResult::Switched { name }
                        }
                        Err(e) => SwitchResult::ActivateError(format!("rig scene: {e}")),
                    }
                } else {
                    SwitchResult::ActivateError("no rig scene applier".into())
                }
            }
        }
    }
}
