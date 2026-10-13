//! Multi-project snapshot registry.
//!
//! REAPER lets multiple project tabs play simultaneously (Preferences →
//! Project: "Allow play tabs in parallel" / per-tab transport). The
//! single-cell hook in `lib.rs` only samples the focused project; this
//! module fans out: a fixed-size array of [`SnapshotCell`]s, one per
//! active project, plus an audio hook that iterates open projects each
//! buffer and writes per-slot.
//!
//! # Why fixed-size + ArcSwap instead of HashMap
//!
//! The audio thread can't allocate. Slot count is bounded by
//! [`MAX_PROJECTS`]; project→slot assignments are stored in an
//! `ArcSwap` that the main thread updates and the audio thread reads
//! wait-free. New projects get the next free slot at first
//! observation by the bridge's main-thread updater task.
//!
//! # Project identity
//!
//! Each open `ReaProject` pointer is mapped to a stable 16-byte
//! [`crate::ProjectId`] by the main-thread updater (UUID v4 generated
//! at first sight). The id is published into the slot before the audio
//! thread's first write, so consumers reading the snapshot always see
//! a populated id. The id is process-local; FTS-session pairs them
//! with longer-lived names (project file path / REAPER project GUID)
//! at its own layer.

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use reaper_medium::{ProjectContext, ReaProject, RealTimeAudioThreadScope, Reaper as MediumReaper};

use crate::{AudioSnapshot, ProjectId, SnapshotCell, split_project_id};

/// Hard cap on simultaneously tracked projects. Sized so the slot
/// array (each cell ~80 bytes) fits comfortably in cache. Plenty
/// for live workloads where 2-4 projects is typical and 16+ is
/// exceptional.
pub const MAX_PROJECTS: usize = 16;

/// One slot. Pre-allocated; activated when the bridge's project
/// updater assigns a `ReaProject` to it.
pub struct ProjectSlot {
    pub cell: SnapshotCell,
    /// REAPER project pointer, stored as a `usize` so it can be
    /// atomically swapped without unsafe. `0` = slot vacant.
    project_ptr: AtomicU64,
    /// Stable 16-byte id for the project currently in this slot.
    /// Written by the main-thread updater alongside `project_ptr`;
    /// read by the audio thread on every sample.
    id_hi: AtomicU64,
    id_lo: AtomicU64,
}

impl ProjectSlot {
    pub const fn new() -> Self {
        Self {
            cell: SnapshotCell::new(),
            project_ptr: AtomicU64::new(0),
            id_hi: AtomicU64::new(0),
            id_lo: AtomicU64::new(0),
        }
    }

    /// Activate the slot with a project pointer + id. Called from
    /// the main-thread updater; safe to call repeatedly to refresh.
    pub fn assign(&self, project: ReaProject, id: ProjectId) {
        let (hi, lo) = split_project_id(id);
        self.id_hi.store(hi, Ordering::Relaxed);
        self.id_lo.store(lo, Ordering::Relaxed);
        self.project_ptr
            .store(project.as_ptr() as usize as u64, Ordering::Release);
    }

    /// Mark the slot vacant. Audio thread will skip it on next pass.
    pub fn clear(&self) {
        self.project_ptr.store(0, Ordering::Release);
        self.id_hi.store(0, Ordering::Relaxed);
        self.id_lo.store(0, Ordering::Relaxed);
    }

    /// `(ReaProject, ProjectId)` if the slot is occupied.
    pub fn current(&self) -> Option<(ReaProject, ProjectId)> {
        let raw = self.project_ptr.load(Ordering::Acquire);
        if raw == 0 {
            return None;
        }
        let ptr = raw as usize as *mut reaper_low::raw::ReaProject;
        let project = ReaProject::new(ptr)?;
        let id = crate::combine_project_id(
            self.id_hi.load(Ordering::Relaxed),
            self.id_lo.load(Ordering::Relaxed),
        );
        Some((project, id))
    }
}

impl Default for ProjectSlot {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-shared registry of per-project [`SnapshotCell`]s.
/// Bridge-side helpers manage slot assignments; the audio hook
/// iterates slots and writes a snapshot per active project per
/// buffer.
pub struct ProjectRegistry {
    slots: [ProjectSlot; MAX_PROJECTS],
}

impl ProjectRegistry {
    pub fn new() -> Self {
        Self {
            slots: [(); MAX_PROJECTS].map(|_| ProjectSlot::new()),
        }
    }

    /// Get a slot by index. Used by consumers iterating registry
    /// state and by the bridge's updater to set/clear assignments.
    pub fn slot(&self, index: usize) -> Option<&ProjectSlot> {
        self.slots.get(index)
    }

    /// Find the slot currently holding `project`, if any.
    pub fn find_slot(&self, project: ReaProject) -> Option<usize> {
        let needle = project.as_ptr() as usize as u64;
        self.slots
            .iter()
            .position(|s| s.project_ptr.load(Ordering::Acquire) == needle)
    }

    /// Find the first vacant slot.
    pub fn find_vacant(&self) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.project_ptr.load(Ordering::Acquire) == 0)
    }

    /// Iterate (slot_index, snapshot) for every currently-occupied
    /// slot. Cheap — just polls each slot's cell once.
    pub fn snapshots(&self) -> Vec<(usize, AudioSnapshot)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.cell.load().map(|snap| (i, snap)))
            .filter(|(_, snap)| snap.project_id != [0u8; 16])
            .collect()
    }

    /// Snapshot the slot mapping (slot_index → ProjectId) for
    /// consumers that need to know which slot holds which project
    /// without polling cells (e.g., the bridge updater's bookkeeping).
    pub fn assignments(&self) -> Vec<(usize, ProjectId)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.current().map(|(_, id)| (i, id)))
            .collect()
    }
}

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Audio hook variant that iterates all active project slots and
/// writes one snapshot per project per audio buffer.
pub struct MultiProjectHook {
    registry: Arc<ProjectRegistry>,
    reaper: MediumReaper<RealTimeAudioThreadScope>,
    counter: u64,
}

impl MultiProjectHook {
    pub fn new(
        registry: Arc<ProjectRegistry>,
        reaper: MediumReaper<RealTimeAudioThreadScope>,
    ) -> Self {
        Self {
            registry,
            reaper,
            counter: 0,
        }
    }
}

impl reaper_medium::OnAudioBuffer for MultiProjectHook {
    fn call(&mut self, args: reaper_medium::OnAudioBufferArgs) {
        if args.is_post {
            return;
        }
        self.counter = self.counter.wrapping_add(1);

        let host_micros = crate::reaper_clock_micros(&self.reaper);
        let sample_rate = args.srate.get();
        let buffer_len = args.len;

        for slot in &self.registry.slots {
            let Some((project, id)) = slot.current() else {
                continue;
            };
            let pos_value = self
                .reaper
                .get_play_position_2_ex(ProjectContext::Proj(project))
                .get();
            let is_playing = self
                .reaper
                .get_play_state_ex(ProjectContext::Proj(project))
                .is_playing;
            slot.cell.store(&AudioSnapshot {
                sequence: self.counter,
                project_id: id,
                host_micros,
                playhead_seconds: pos_value,
                playrate: crate::PUBLISHED_PLAYRATE,
                sample_rate,
                buffer_len,
                is_playing,
            });
        }
    }
}

/// Build the registry + hook pair. Caller registers the returned
/// hook via `ReaperSession::audio_reg_hardware_hook_add` and uses a
/// main-thread task to populate slot assignments from
/// `enum_projects`.
pub fn build_multi_project_hook(
    reaper: MediumReaper<RealTimeAudioThreadScope>,
) -> (Arc<ProjectRegistry>, MultiProjectHook) {
    let registry = Arc::new(ProjectRegistry::new());
    let hook = MultiProjectHook::new(registry.clone(), reaper);
    (registry, hook)
}

// ── Process-global registry ────────────────────────────────────────

static GLOBAL_REGISTRY: std::sync::OnceLock<Arc<ProjectRegistry>> = std::sync::OnceLock::new();

pub fn set_global_registry(reg: Arc<ProjectRegistry>) {
    let _ = GLOBAL_REGISTRY.set(reg);
}

pub fn global_registry() -> Option<&'static Arc<ProjectRegistry>> {
    GLOBAL_REGISTRY.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_has_no_snapshots() {
        let reg = ProjectRegistry::new();
        assert!(reg.snapshots().is_empty());
        assert!(reg.assignments().is_empty());
        assert_eq!(reg.find_vacant(), Some(0));
    }

    #[test]
    fn slot_lifecycle() {
        let reg = ProjectRegistry::new();
        // Forge a non-null pointer for the test. We never deref it
        // here — the audio hook isn't run.
        let fake = std::ptr::NonNull::<reaper_low::raw::ReaProject>::dangling();
        let project = unsafe { ReaProject::new(fake.as_ptr()).unwrap() };
        let id = [1u8; 16];

        let slot_idx = reg.find_vacant().unwrap();
        reg.slot(slot_idx).unwrap().assign(project, id);
        assert_eq!(reg.find_vacant(), Some(slot_idx + 1));
        assert_eq!(reg.find_slot(project), Some(slot_idx));
        assert_eq!(reg.assignments(), vec![(slot_idx, id)]);

        reg.slot(slot_idx).unwrap().clear();
        assert_eq!(reg.find_vacant(), Some(slot_idx));
        assert_eq!(reg.find_slot(project), None);
        assert!(reg.assignments().is_empty());
    }
}
