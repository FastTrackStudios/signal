//! The drum workspace's write half: the panel's Apply and the slip
//! drag, landed on the daw through the group rule.
//!
//! The UI never writes — `ExpressionEditor` exposes `on_quantize_*` and
//! `on_slip` callbacks and this is what the standalone runner plugs
//! into them. One host object owns the backend, the kit group's items,
//! and the trigger lanes' summed signal, so every gesture edits the
//! whole kit at once (r[drums.group.kit]) and lands as one undo step.

use std::sync::{Arc, Mutex};

use daw::service::{ItemRef, ProjectContext, Projects};
use daw::standalone::Standalone;
use expression_editor_audio::apply_quantize::{Applied, GroupError, apply_split, apply_warp};
use expression_editor_audio::detect::Transient;
use expression_editor_audio::gate::Hit;
use expression_editor_audio::group_detect::refine_onset;
use expression_editor_audio::panel_bridge::{self, PanelDetect, PanelTarget};
use expression_editor_audio::quantize::SplitConfig;
use expression_editor_audio::slip::slip_hit;
use expression_editor_audio::stretch::stretch_hit;
use expression_editor_core::kit::LaneRole;
use expression_editor_ui::quantize_panel::{Bin, HitPreview, QuantizePanel, WriteMode, histogram};

/// One role lane's contribution to the host: its members' edit items,
/// and (for a trigger lane) the summed signal detection runs on.
pub struct HostLane {
    pub role: LaneRole,
    pub items: Vec<ItemRef>,
    /// The members' mean, kept only for lanes that detect
    /// (`LaneRole::is_detection_source`). `None` elsewhere — the sum of
    /// the room mics would cost memory nothing reads.
    pub summed: Option<Vec<f64>>,
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
pub struct DrumHost {
    daw: Standalone,
    ctx: ProjectContext,
    lanes: Vec<HostLane>,
    /// The trigger lanes' summed signal, parallel to `lanes`. Behind a
    /// lock because [`DrumHost::refresh`] recomputes it after an edit
    /// lands — detection must run on the audio as it *is*, not as it
    /// loaded. `Arc` so a detect in flight keeps its snapshot.
    sums: Mutex<Vec<Option<Arc<Vec<f64>>>>>,
    manual: Mutex<ManualHits>,
    pub sample_rate: f64,
    /// The group's shared take length, seconds — the longest edit item.
    pub take_secs: f64,
    /// One beat, seconds, from the project tempo. What turns the
    /// panel's division/feel into `grid_secs`. r[impl drums.group.tempo]
    pub beat_secs: f64,
}

impl DrumHost {
    pub fn new(
        daw: Standalone,
        ctx: ProjectContext,
        mut lanes: Vec<HostLane>,
        sample_rate: f64,
        take_secs: f64,
        beat_secs: f64,
    ) -> Self {
        let sums = lanes
            .iter_mut()
            .map(|l| l.summed.take().map(Arc::new))
            .collect();
        Self {
            daw,
            ctx,
            lanes,
            sums: Mutex::new(sums),
            manual: Mutex::new(ManualHits::default()),
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

    fn detect_of(panel: &QuantizePanel) -> PanelDetect {
        let d = &panel.detect;
        PanelDetect {
            threshold_db: d.threshold_db,
            sensitivity: d.sensitivity,
            crest_db: d.crest_db,
            high_pass_hz: d.high_pass_hz,
            low_pass_hz: d.low_pass_hz,
            gain: d.gain,
            retrigger_secs: d.retrigger_secs,
            time_offset_secs: d.time_offset_secs,
        }
    }

    fn target_of(&self, panel: &QuantizePanel) -> PanelTarget {
        PanelTarget {
            grid_secs: panel.grid_in(self.beat_secs),
            grid_offset_secs: 0.0,
            swing: panel.swing,
            grid_scan: panel.grid_scan,
            tolerance_secs: panel.tolerance,
            strength: panel.config.strength,
        }
    }

    /// Snapshot of the trigger lanes' sums — `Arc`s, so a detect keeps
    /// reading the audio it started with even across a refresh.
    fn trigger_sums(&self) -> Vec<Arc<Vec<f64>>> {
        self.sums
            .lock()
            .map(|s| s.iter().flatten().cloned().collect())
            .unwrap_or_default()
    }

    /// Detect + plan for the panel's current settings: the histogram
    /// bins and the per-hit preview the drawer shows.
    // r[impl drums.quantize.preview]
    pub fn preview(&self, panel: &QuantizePanel) -> (Vec<Bin>, Vec<HitPreview>) {
        let hits = self.hits(panel);
        let (_plan, previews) = panel_bridge::preview_hits(&hits, &self.target_of(panel));
        (histogram(&hits, 24), previews)
    }

    /// The merged hit list at the panel's current detect settings, with
    /// the hand overlay applied: removed hits suppressed, added hits in.
    // r[impl drums.manual.add-remove]
    pub fn hits(&self, panel: &QuantizePanel) -> Vec<Transient> {
        let sums = self.trigger_sums();
        let lanes: Vec<Vec<&[f64]>> = sums.iter().map(|s| vec![s.as_slice()]).collect();
        let detected =
            panel_bridge::detect_group(&lanes, self.sample_rate, &Self::detect_of(panel));
        let Ok(m) = self.manual.lock() else {
            return detected;
        };
        let mut out: Vec<Transient> = detected
            .into_iter()
            .filter(|t| !m.removed.iter().any(|r| (t.at - r).abs() <= MANUAL_TOL))
            .collect();
        for &at in &m.added {
            if out.iter().any(|t| (t.at - at).abs() <= MANUAL_TOL) {
                continue;
            }
            // A hand-placed hit is intent, not evidence — full loudness,
            // so a grid-scan contest never drops it for a ghost.
            out.push(Transient {
                at,
                loudness: 1.0,
                crest_db: 0.0,
                hit: Hit {
                    sample: (at * self.sample_rate).max(0.0) as usize,
                    peak: 1.0,
                    rms: 1.0,
                    crest_db: 0.0,
                },
            });
        }
        out.sort_by(|a, b| a.at.total_cmp(&b.at));
        out
    }

    /// Add a hit by hand, refined to the nearest attack in the trigger
    /// lanes' sum. Returns where it landed. Edits the hit list only —
    /// nothing reaches the daw until a drag or Apply.
    // r[impl drums.manual.add-remove]
    pub fn add_hit(&self, at: f64, window_secs: f64) -> f64 {
        let sums = self.trigger_sums();
        let refined = sums
            .iter()
            .map(|s| refine_onset(s, self.sample_rate, at, window_secs))
            .fold(None::<f64>, |best, t| match best {
                // The refinement nearest the click wins across lanes.
                Some(b) if (b - at).abs() <= (t - at).abs() => Some(b),
                _ => Some(t),
            })
            .unwrap_or(at);
        if let Ok(mut m) = self.manual.lock() {
            m.removed.retain(|r| (r - refined).abs() > MANUAL_TOL);
            m.added.push(refined);
        }
        refined
    }

    /// Throw a hit out by hand. A hand-added hit is simply un-added; a
    /// detected one is suppressed.
    // r[impl drums.manual.add-remove]
    pub fn remove_hit(&self, at: f64) {
        if let Ok(mut m) = self.manual.lock() {
            let had = m.added.len();
            m.added.retain(|a| (a - at).abs() > MANUAL_TOL);
            if m.added.len() == had {
                m.removed.push(at);
            }
        }
    }

    /// Write the panel's plan to the whole kit, one undo step.
    // r[impl drums.quantize.apply]
    pub fn apply(&self, panel: &QuantizePanel) -> Result<Applied, GroupError> {
        let hits = self.hits(panel);
        let (plan, _) = panel_bridge::preview_hits(&hits, &self.target_of(panel));
        let items = self.group();
        self.daw.begin_undo_block(self.ctx.clone(), "Quantize kit");
        let out = match panel.mode {
            WriteMode::Split => {
                let cfg = SplitConfig {
                    leading_pad_secs: panel.pad,
                    crossfade_secs: panel.crossfade,
                };
                let pieces = plan.splits(self.take_secs, cfg);
                apply_split(&self.daw, self.ctx.clone(), &items, &pieces, cfg)
            }
            WriteMode::Warp => {
                let frames = (self.take_secs * self.sample_rate).ceil() as usize;
                match plan.alignment(frames, self.sample_rate) {
                    Some(a) => apply_warp(&self.daw, self.ctx.clone(), &items, &a),
                    None => Ok(Applied::default()),
                }
            }
        };
        self.daw
            .end_undo_block(self.ctx.clone(), "Quantize kit", None);
        out
    }

    /// Slip one hit across the whole kit, one undo step.
    // r[impl drums.manual.slip]
    pub fn slip(
        &self,
        hit: f64,
        next: f64,
        delta: f64,
        cfg: SplitConfig,
    ) -> Result<Applied, GroupError> {
        let items = self.group();
        self.daw.begin_undo_block(self.ctx.clone(), "Slip hit");
        let out = slip_hit(
            &self.daw,
            self.ctx.clone(),
            &items,
            hit,
            next,
            self.take_secs,
            delta,
            cfg,
        );
        self.daw.end_undo_block(self.ctx.clone(), "Slip hit", None);
        out
    }

    /// Stretch one hit across the whole kit, one undo step — the WARP
    /// twin of [`DrumHost::slip`]. `both` is the BothStretch law: the
    /// take's ends pin instead of the neighbours.
    // r[impl drums.manual.stretch]
    pub fn stretch(
        &self,
        hit: f64,
        prev: f64,
        next: f64,
        delta: f64,
        both: bool,
    ) -> Result<Applied, GroupError> {
        let items = self.group();
        self.daw.begin_undo_block(self.ctx.clone(), "Stretch hit");
        let out = stretch_hit(
            &self.daw,
            self.ctx.clone(),
            &items,
            hit,
            prev,
            next,
            self.take_secs,
            delta,
            both,
            self.sample_rate,
        );
        self.daw
            .end_undo_block(self.ctx.clone(), "Stretch hit", None);
        out
    }

    /// One undo step back — the whole last gesture.
    pub fn undo(&self) -> bool {
        self.daw.undo(self.ctx.clone())
    }

    /// Re-read the kit from the daw after an edit landed, and hand back
    /// a fresh document per member track so the lanes draw the audio
    /// where it now *is* — split pieces, slid spans and all. Also
    /// recomputes the trigger lanes' sums, so the next detect and the
    /// next Alt+click see the edited audio too.
    ///
    /// Returns `(track_guid, doc)` pairs; the caller pushes them into
    /// the editor with `Editor::reload_track_doc`.
    pub fn refresh(&self) -> Vec<(String, expression_editor_core::ExpressionDoc)> {
        use daw::service::{Items, Tracks};
        let mut docs = Vec::new();
        let mut new_sums: Vec<Option<Arc<Vec<f64>>>> = Vec::with_capacity(self.lanes.len());
        for lane in &self.lanes {
            // Member tracks, from the lanes' anchor items (piece 0 of a
            // split keeps the original item guid, so this stays valid
            // across edits).
            let mut track_guids: Vec<String> = Vec::new();
            for item in &lane.items {
                if let Some(info) = self.daw.get_item(self.ctx.clone(), item.clone())
                    && !track_guids.contains(&info.track_guid)
                {
                    track_guids.push(info.track_guid);
                }
            }
            let mut lane_sum: Option<Vec<f64>> = None;
            let mut members = 0usize;
            for guid in &track_guids {
                let Some(track) = Tracks::all(&self.daw, self.ctx.clone())
                    .into_iter()
                    .find(|t| &t.guid == guid)
                else {
                    continue;
                };
                let samples = crate::track_timeline(
                    &self.daw,
                    &self.ctx,
                    &track,
                    self.take_secs,
                    self.sample_rate,
                );
                if lane.role.is_detection_source() {
                    let sum = lane_sum.get_or_insert_with(|| vec![0.0; samples.len()]);
                    for (o, v) in sum.iter_mut().zip(samples.iter()) {
                        *o += v;
                    }
                    members += 1;
                }
                let mut doc = crate::percussion_doc(&samples, self.sample_rate);
                crate::attach_regions(&self.daw, &self.ctx, &mut doc);
                docs.push((guid.clone(), doc));
            }
            new_sums.push(lane_sum.map(|mut sum| {
                let scale = 1.0 / members.max(1) as f64;
                for v in &mut sum {
                    *v *= scale;
                }
                Arc::new(sum)
            }));
        }
        if let Ok(mut sums) = self.sums.lock() {
            *sums = new_sums;
        }
        docs
    }

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

/// The host as the window shares it: the callbacks each hold a clone.
pub type SharedDrumHost = Arc<DrumHost>;
