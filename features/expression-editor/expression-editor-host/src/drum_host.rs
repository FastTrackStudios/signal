//! The drum workspace's write half: the panel's Apply and the slip
//! drag, landed on the daw through the group rule.
//!
//! The UI never writes — `ExpressionEditor` exposes `on_quantize_*` and
//! `on_slip` callbacks and this is what the standalone runner plugs
//! into them. One host object owns the backend, the kit group's items,
//! and the trigger lanes' summed signal, so every gesture edits the
//! whole kit at once (r[drums.group.kit]) and lands as one undo step.

use std::sync::{Arc, Mutex};

use daw::service::{ItemRef, ProjectContext};
#[cfg(feature = "standalone")]
use daw::standalone::Standalone;
use expression_editor_audio::apply_quantize::apply_warp;
use expression_editor_audio::daw_bound::DrumDaw;
use expression_editor_audio::detect::Transient;
use expression_editor_audio::gate::Hit;
use expression_editor_audio::group_detect::refine_onset;
use expression_editor_audio::panel_bridge::{self, PanelDetect, PanelTarget};
mod detection;
mod refresh;
mod write;
use expression_editor_audio::stretch::stretch_hit;
use expression_editor_core::kit::LaneRole;
use expression_editor_tools::quantize_panel::{
    Bin, HitPreview, QuantizePanel, WriteMode, histogram,
};

/// One role lane's contribution to the host: its members' edit items,
/// and (for a trigger lane) the summed signal detection runs on.
pub struct HostLane {
    pub role: LaneRole,
    pub items: Vec<ItemRef>,
    /// The signals detection runs on: one per detection *unit*, not one
    /// per lane. Toms are a unit each — so a hit can be attributed to
    /// the tom that made it rather than to "some tom" — and within a
    /// unit a trigger is weighted over the mics it shares a drum with.
    /// Empty for lanes that do not detect (`LaneRole::is_detection_source`);
    /// summing the room mics would cost memory nothing reads.
    pub signals: Vec<Vec<f64>>,
}

/// Hand edits to the hit list, layered over detection.
///
/// The detector's output is recomputed on every panel change; a hit the
/// user added or threw out must survive that, so the overlay is kept
/// here and applied after every detect (r[drums.manual.add-remove]).
/// Nothing lands on the daw until a drag or Apply.
#[derive(Default)]
struct ManualHits {
    added: Vec<f64>,
    removed: Vec<f64>,
}

/// How close a removed time must be to a detected hit to suppress it —
/// the pick radius of the gesture, not a detection window.
const MANUAL_TOL: f64 = 0.015;

/// Everything a drum-workspace gesture needs to reach the daw.
pub struct DrumHost<D>
where
    D: DrumDaw,
{
    daw: D,
    ctx: ProjectContext,
    lanes: Vec<HostLane>,
    /// Every detection signal across all lanes, flattened — one per
    /// detection unit, so four triggered toms contribute four. Behind a
    /// lock because [`DrumHost::refresh`] recomputes them after an edit
    /// lands — detection must run on the audio as it *is*, not as it
    /// loaded. `Arc` so a detect in flight keeps its snapshot.
    sums: Mutex<Vec<(LaneRole, Arc<Vec<f64>>)>>,
    /// Set when the host was built without its detection signals — a
    /// cached open, which skips the decode that produces them — and
    /// cleared by the first detection, which reads the audio then.
    ///
    /// Without this a reopened session detected over nothing: no fill
    /// bands, a quantize panel that found no hits, "protect fills"
    /// protecting nothing. The decode is paid at the first detection
    /// instead of at load, so the window still opens from the cache;
    /// what the cache can never stand in for is the audio itself.
    signals_pending: std::sync::atomic::AtomicBool,
    manual: Mutex<ManualHits>,
    /// The take's fills, computed on demand and dropped on refresh.
    fills: Mutex<Option<Vec<expression_editor_core::fills::Fill>>>,
    pub sample_rate: f64,
    /// End of the shared project timeline, in seconds.
    pub take_secs: f64,
    /// One beat, seconds, from the project tempo. What turns the
    /// panel's division/feel into `grid_secs`. r[impl drums.group.tempo]
    pub beat_secs: f64,
}

impl<D: DrumDaw> DrumHost<D> {
    pub fn new(
        daw: D,
        ctx: ProjectContext,
        mut lanes: Vec<HostLane>,
        sample_rate: f64,
        take_secs: f64,
        beat_secs: f64,
    ) -> Self {
        // Tagged with the role they came from: detection merges every
        // signal into one hit list, but a fill is recognised by *which*
        // drum was played, so that has to survive the flattening.
        let sums: Vec<(LaneRole, Arc<Vec<f64>>)> = lanes
            .iter_mut()
            .flat_map(|l| {
                let role = l.role;
                std::mem::take(&mut l.signals)
                    .into_iter()
                    .map(move |s| (role, Arc::new(s)))
            })
            .collect();
        // Signals are missing rather than absent only when a lane that
        // detects has none: a kit with no detection source has nothing
        // to hydrate, and must not decode on every detect looking for it.
        let pending = sums.is_empty() && lanes.iter().any(|l| l.role.is_detection_source());
        Self {
            daw,
            ctx,
            lanes,
            sums: Mutex::new(sums),
            signals_pending: std::sync::atomic::AtomicBool::new(pending),
            manual: Mutex::new(ManualHits::default()),
            fills: Mutex::new(None),
            sample_rate,
            take_secs,
            beat_secs,
        }
    }

    /// Every member item of every role lane — the kit group. Edits are
    /// applied to all of it, never to one lane. r[impl drums.group.kit]
    pub fn group(&self) -> Vec<ItemRef> {
        self.lanes.iter().flat_map(|l| l.items.clone()).collect()
    }
}

/// The host as the window shares it: the callbacks each hold a clone.
///
/// The backend is explicit; application shells can provide their own aliases.
pub type SharedDrumHost<D> = Arc<DrumHost<D>>;

/// Saving a copy is the standalone window's feature, not the
/// workspace's: it writes a new `.rpp` beside the original, which is
/// what a window with no host application has to do. In REAPER the user
/// saves through REAPER, so this is the one thing the generic host
/// deliberately does not offer.
#[cfg(feature = "standalone")]
impl DrumHost<Standalone> {
    /// Save the project as a **new** `.rpp` beside its original —
    /// `<stem>.fts-edit.rpp` — never over it. Returns the path written.
    // r[impl drums.save.new-file]
    pub fn save(&self) -> Result<std::path::PathBuf, String> {
        let guid = match &self.ctx {
            ProjectContext::Project(g) => g.clone(),
            ProjectContext::Current => return Err("host has no project guid".into()),
        };
        daw::standalone::save::save_project_as(&self.daw, &guid)
    }
}
