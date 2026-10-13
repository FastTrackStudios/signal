//! Drum refresh services.
use super::*;

/// What one read of the kit yields: the detection units' summed signals,
/// role-tagged, and — when asked for — a fresh document per member track.
type KitRead = (
    Vec<(LaneRole, Arc<Vec<f64>>)>,
    Vec<(String, expression_editor_core::ExpressionDoc)>,
);

impl<D: DrumDaw> DrumHost<D> {
    /// Re-read the kit from the daw after an edit landed, and hand back
    /// a fresh document per member track so the lanes draw the audio
    /// where it now *is* — split pieces, slid spans and all. Also
    /// recomputes the trigger lanes' sums, so the next detect and the
    /// next Alt+click see the edited audio too.
    ///
    /// Returns `(track_guid, doc)` pairs; the caller pushes them into
    /// the editor with `Editor::reload_track_doc`.
    pub fn refresh(&self) -> Vec<(String, expression_editor_core::ExpressionDoc)> {
        let (new_sums, docs) = self.read_kit(true);
        if let Ok(mut sums) = self.sums.lock() {
            *sums = new_sums;
        }
        // A refresh reads the audio, so nothing is pending after it.
        self.signals_pending
            .store(false, std::sync::atomic::Ordering::Release);
        // The audio moved, so the fills have to be found again.
        if let Ok(mut fills) = self.fills.lock() {
            *fills = None;
        }
        docs
    }

    /// Read the detection signals a cached open left out, once.
    ///
    /// Every detector reads its signals through here. On a cold open
    /// this is a flag check; on a cached one the first call decodes the
    /// kit and the rest are flag checks. The same read as `refresh`,
    /// minus the documents: the cache already placed every hit where a
    /// fresh analysis would (see `percussion_doc_cached`).
    pub(super) fn ensure_signals(&self) {
        use std::sync::atomic::Ordering;
        if !self.signals_pending.load(Ordering::Acquire) {
            return;
        }
        let (new_sums, _) = self.read_kit(false);
        if let Ok(mut sums) = self.sums.lock() {
            *sums = new_sums;
        }
        self.signals_pending.store(false, Ordering::Release);
    }

    /// Re-read every member's audio from the daw: the detection units'
    /// summed signals, and — when asked — a fresh document per member
    /// track.
    fn read_kit(&self, with_docs: bool) -> KitRead {
        use daw::service::Tracks;
        let tracks = Tracks::all(&self.daw, self.ctx.clone());
        let mut docs = Vec::new();
        let mut new_sums: Vec<(LaneRole, Arc<Vec<f64>>)> = Vec::with_capacity(self.lanes.len());
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
            // Re-read every member, then rebuild this lane's detection
            // units from the *current* audio. The unit split and the
            // trigger weighting must match the load path exactly, or a
            // detect would silently change meaning after the first edit.
            let mut names: Vec<String> = Vec::new();
            let mut takes: Vec<Vec<f64>> = Vec::new();
            for guid in &track_guids {
                let Some(track) = tracks.iter().find(|t| &t.guid == guid) else {
                    continue;
                };
                let samples = crate::track_timeline(
                    &self.daw,
                    &self.ctx,
                    track,
                    self.take_secs,
                    self.sample_rate,
                );
                if with_docs {
                    let mut doc = crate::percussion_doc(&samples, self.sample_rate);
                    crate::attach_timeline(&self.daw, &self.ctx, &mut doc);
                    docs.push((guid.clone(), doc));
                }
                names.push(track.name.clone());
                takes.push(samples);
            }
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            // r[impl drums.group.detection-source]
            for unit in expression_editor_core::kit::detection_units(lane.role, &refs) {
                let sig = crate::blend(unit.iter().map(|&(u, w)| (takes[u].as_slice(), w)));
                if !sig.is_empty() {
                    new_sums.push((lane.role, Arc::new(sig)));
                }
            }
        }
        (new_sums, docs)
    }
}
