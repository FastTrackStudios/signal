//! Drum detection services.
use super::*;

impl<D: DrumDaw> DrumHost<D> {
    pub(super) fn detect_of(panel: &QuantizePanel) -> PanelDetect {
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

    pub(super) fn target_of(&self, panel: &QuantizePanel) -> PanelTarget {
        PanelTarget {
            grid_secs: panel.grid_in(self.beat_secs),
            grid_offset_secs: 0.0,
            swing: panel.swing,
            grid_scan: panel.grid_scan,
            tolerance_secs: panel.tolerance,
            strength: panel.config.strength,
        }
    }

    /// Snapshot of every detection signal — `Arc`s, so a detect keeps
    /// reading the audio it started with even across a refresh.
    pub(super) fn trigger_sums(&self) -> Vec<Arc<Vec<f64>>> {
        self.role_sums().into_iter().map(|(_, s)| s).collect()
    }

    /// The same snapshot, keeping which role each signal came from.
    pub(super) fn role_sums(&self) -> Vec<(LaneRole, Arc<Vec<f64>>)> {
        self.ensure_signals();
        self.sums.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Detection settings for counting activity rather than for
    /// choosing what to quantize.
    ///
    /// The two jobs want opposite things. The quantize panel wants
    /// precision — every hit it reports gets *moved*, so a false one
    /// damages the take, and its default sensitivity of 0.5 is set for
    /// that. Fill detection only counts how busy a bar was, where a
    /// missed hit is the costly error and a spurious one is noise the
    /// median absorbs.
    ///
    /// It matters more than it sounds. On `unbreakable` — 160bpm, the
    /// drummer playing about ten hits a second — the panel's default
    /// finds 1.6 a second, roughly a fifth of what was played. Counting
    /// bars against a fifth of the evidence is what made fill counts
    /// swing between three and twenty-four across the album.
    pub(super) fn fill_detect_panel(sensitivity: f64) -> QuantizePanel {
        let mut panel = QuantizePanel::default();
        panel.detect.sensitivity = sensitivity;
        panel
    }

    /// Every detected hit with the drum it was played on, in seconds.
    ///
    /// Detection runs per role rather than on the merged list the
    /// quantize panel uses: which drum was struck is thrown away by the
    /// merge, and it is the whole signal for anything that reasons
    /// about *what* was played rather than *when*.
    pub fn role_hits(&self, panel: &QuantizePanel) -> Vec<(f64, LaneRole)> {
        let detect = Self::detect_of(panel);
        let mut hits: Vec<(f64, LaneRole)> = Vec::new();
        for (role, signal) in self.role_sums() {
            let lanes = vec![vec![signal.as_slice()]];
            for t in panel_bridge::detect_group(&lanes, self.sample_rate, &detect) {
                hits.push((t.at, role));
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        hits
    }

    /// Every detected hit with its drum, using the hybrid detector —
    /// spectral flux to find hits, the envelope to place them.
    ///
    /// No sensitivity to pass: the flux stage scores each frame against
    /// its own neighbourhood, so it follows the material instead of
    /// being told about it. That is the whole point of it.
    // r[impl drums.detect.hybrid]
    pub fn role_hits_hybrid(&self) -> Vec<(f64, LaneRole)> {
        self.role_hits_hybrid_with(&expression_editor_audio::hybrid::HybridConfig::default())
    }

    /// The same, with the flux stage's own settings — for the sweep
    /// that chooses them.
    pub fn role_hits_hybrid_with(
        &self,
        cfg: &expression_editor_audio::hybrid::HybridConfig,
    ) -> Vec<(f64, LaneRole)> {
        let mut hits: Vec<(f64, LaneRole)> = Vec::new();
        for (role, signal) in self.role_sums() {
            for t in
                expression_editor_audio::hybrid::detect(signal.as_slice(), self.sample_rate, cfg)
            {
                hits.push((t.at, role));
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        hits
    }

    /// How many bars the host's tempo map places across the take.
    /// Zero when it cannot place a grid at all.
    pub fn bar_count(&self) -> usize {
        crate::bar_grid(&self.daw, &self.ctx, self.take_secs)
            .len()
            .saturating_sub(1)
    }

    /// The take's fills, computed once and kept until the audio changes.
    ///
    /// Cached because the panel previews on every change a slider makes
    /// and fill detection runs the whole detector over every lane. The
    /// cache is cleared by [`DrumHost::refresh`], which is the only
    /// thing that alters the audio underneath it.
    pub(super) fn fills_cached(&self) -> Vec<expression_editor_core::fills::Fill> {
        if let Ok(cache) = self.fills.lock()
            && let Some(found) = cache.as_ref()
        {
            return found.clone();
        }
        let found = self.fills(&expression_editor_core::fills::FillConfig::default());
        if let Ok(mut cache) = self.fills.lock() {
            *cache = Some(found.clone());
        }
        found
    }

    /// The hits a quantize is allowed to move.
    ///
    /// Everything the detector found, less anything inside a fill when
    /// the panel asks for fills to be protected. The hits are still
    /// *shown* — the lane draws every one — they are simply not moved,
    /// so the user can see what was left alone rather than wondering
    /// where it went.
    // r[impl drums.fills.protect]
    pub(super) fn quantizable(&self, panel: &QuantizePanel) -> Vec<Transient> {
        let hits = self.hits(panel);
        if !panel.protect_fills {
            return hits;
        }
        let fills = self.fills_cached();
        if fills.is_empty() {
            return hits;
        }
        hits.into_iter()
            .filter(|t| !fills.iter().any(|f| t.at >= f.start && t.at < f.end))
            .collect()
    }

    /// The bar boundaries the host's tempo map places across the take.
    pub fn bar_grid_secs(&self) -> Vec<f64> {
        crate::bar_grid(&self.daw, &self.ctx, self.take_secs)
    }

    /// The take's fills, as spans of bars that stop keeping time.
    ///
    /// Detection is run per role rather than on the merged list, since
    /// a fill is recognised by *which* drum was played — a bar full of
    /// toms — and the merged list has thrown that away.
    ///
    /// Empty when the host cannot place bars. A fill span is meaningless
    /// without a bar grid, and guessing one from a single bpm would put
    /// the spans in the wrong place on any song that changes meter.
    // r[impl drums.fills.detect]
    pub fn fills(
        &self,
        cfg: &expression_editor_core::fills::FillConfig,
    ) -> Vec<expression_editor_core::fills::Fill> {
        let bars = crate::bar_grid(&self.daw, &self.ctx, self.take_secs);
        if bars.len() < 2 {
            return Vec::new();
        }
        let hits = if cfg.hybrid_detect {
            self.role_hits_hybrid()
        } else {
            self.role_hits(&Self::fill_detect_panel(cfg.detect_sensitivity))
        };
        expression_editor_core::fills::detect_fills(&bars, &hits, cfg)
    }

    /// Detect + plan for the panel's current settings: the histogram
    /// bins and the per-hit preview the drawer shows.
    // r[impl drums.quantize.preview]
    pub fn preview(&self, panel: &QuantizePanel) -> (Vec<Bin>, Vec<HitPreview>) {
        let hits = self.quantizable(panel);
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
}
