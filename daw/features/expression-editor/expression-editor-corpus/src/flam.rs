//! The flam sweep — the part of the corpus that cannot be downloaded.
//!
//! A flam is two strikes on the same drum a few tens of milliseconds
//! apart, one of them a ghost. It is the case every onset detector
//! gets wrong, and there is essentially no public data on it: eleven
//! annotated flams exist across the entire published corpus, all in
//! MDB-Drums, whose annotation QC "checks for duplicate labels within a
//! 50 ms window" — so a flam is recorded as *one* label and even those
//! eleven cannot say where the second strike landed.
//!
//! Authoring the sweep solves that outright. We know where both
//! strikes are because we put them there, to the sample.
//!
//! ## Both orderings, because they fail differently
//!
//! [`Side::Before`] is the flam as a drummer plays it: quiet grace,
//! then the accent 5–60 ms later. Nothing masks the accent, so the
//! detector finds it easily — and then *discards the grace*, because
//! peaks are taken in strength order and the minimum-spacing rule
//! evicts the weaker of any close pair. The failure is a suppression
//! rule, and it is total below the spacing floor.
//!
//! [`Side::After`] is the case [`onsets`] describes in its own module
//! docs: the second strike "rises out of the first one's decay rather
//! than out of silence, so its spectral change is small and it falls
//! below the threshold". Here masking, not suppression, is what hides
//! it, and it fades in gradually as the spacing opens up.
//!
//! [`onsets`]: expression_editor_audio::onsets
//!
//! Sweeping only one of the two would produce a curve that looked like
//! an answer and described half the problem.
//!
//! ## The grid
//!
//! Spacing runs 5 to 60 ms because that brackets the phenomenon: below
//! 5 ms two strikes are one thickened attack that no detector should
//! separate, above 60 ms it is two notes and any detector should.
//! Grace velocity runs 0.15 to 0.6 of the accent, which is the ghost
//! range — quieter than that is a brush of the head, louder is a
//! double stroke.
//!
//! The same grid drives both renderers. [`FlamSweep::render`] draws it
//! with [`crate::synth`] for a test that needs no download;
//! [`crate::smf`] writes it as MIDI so `fetch-corpus.sh` can render the
//! identical grid through a real kit, where the answer counts.

use crate::synth::Snare;

/// Which strike is the ghost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Grace first, accent second — the flam a drummer plays.
    Before,
    /// Accent first, ghost second — the decay-masking case.
    After,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

/// One rendered flam: two strikes at known times and known levels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlamCase {
    /// Position in the sweep, so a result can be traced back to a
    /// place in the rendered file.
    pub index: usize,
    pub side: Side,
    /// Gap between the two strikes, in milliseconds.
    pub spacing_ms: f64,
    /// The ghost's level as a fraction of the accent's.
    pub grace_velocity: f64,
    /// When the ghost strike happens, in seconds from the start.
    pub grace_secs: f64,
    /// When the accent happens, in seconds from the start.
    pub accent_secs: f64,
}

impl FlamCase {
    /// The two strike times in the order they sound.
    pub fn strikes(&self) -> [f64; 2] {
        match self.side {
            Side::Before => [self.grace_secs, self.accent_secs],
            Side::After => [self.accent_secs, self.grace_secs],
        }
    }

    /// The strike that is hard to find — the ghost, in both orderings.
    pub fn ghost_secs(&self) -> f64 {
        self.grace_secs
    }
}

/// The sweep's shape.
#[derive(Clone, Debug)]
pub struct FlamSweep {
    pub spacings_ms: Vec<f64>,
    pub grace_velocities: Vec<f64>,
    pub sides: Vec<Side>,
    /// Accent level, 0..=1.
    pub accent_velocity: f64,
    /// Silence between one case and the next, in seconds.
    ///
    /// Generous on purpose. The detector subtracts a *moving median*,
    /// so cases packed close together raise each other's local floor
    /// and the curve would measure the packing as much as the flam.
    pub gap_secs: f64,
    pub sample_rate: f64,
}

impl Default for FlamSweep {
    fn default() -> Self {
        Self {
            // 5 ms steps through the range where the answer changes,
            // then coarser once it is settled.
            spacings_ms: vec![5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 60.0],
            grace_velocities: vec![0.15, 0.25, 0.40, 0.60],
            sides: vec![Side::Before, Side::After],
            accent_velocity: 1.0,
            gap_secs: 0.75,
            // The rate CrocellKit is at, so the synthetic sweep and the
            // rendered one are measured on the same hop grid.
            sample_rate: 48_000.0,
        }
    }
}

impl FlamSweep {
    /// Every case in the grid, laid out on one timeline.
    ///
    /// Ordered side-major then spacing then velocity, so a rendered
    /// file can be scrubbed and the structure heard: all the
    /// grace-first cases opening up, then all the ghost-after ones.
    pub fn cases(&self) -> Vec<FlamCase> {
        let mut out = Vec::new();
        // Half a gap of lead-in: a hit in the first frames has nothing
        // before it to be an increase over. `onsets` pads a window to
        // handle exactly this, and giving it real silence too means the
        // first case is not a special case.
        let mut cursor = self.gap_secs * 0.5;
        for &side in &self.sides {
            for &spacing_ms in &self.spacings_ms {
                for &grace_velocity in &self.grace_velocities {
                    let spacing = spacing_ms / 1000.0;
                    let (grace_secs, accent_secs) = match side {
                        Side::Before => (cursor, cursor + spacing),
                        Side::After => (cursor + spacing, cursor),
                    };
                    out.push(FlamCase {
                        index: out.len(),
                        side,
                        spacing_ms,
                        grace_velocity,
                        grace_secs,
                        accent_secs,
                    });
                    cursor += self.gap_secs;
                }
            }
        }
        out
    }

    /// Total length of the sweep, in seconds.
    pub fn duration_secs(&self) -> f64 {
        let cases = self.cases();
        cases
            .last()
            .map(|c| c.strikes()[1] + self.gap_secs)
            .unwrap_or(0.0)
    }

    /// Draw the sweep with the synthetic snare.
    pub fn render(&self) -> Rendered {
        self.render_with(&Snare::new(self.sample_rate))
    }

    /// Draw the sweep with a particular snare — the seam a future
    /// convolution against a real kit sample slots into.
    pub fn render_with(&self, snare: &Snare) -> Rendered {
        let cases = self.cases();
        let len = (self.duration_secs() * self.sample_rate).ceil() as usize;
        let mut samples = vec![0.0f64; len];
        for case in &cases {
            let accent = (case.accent_secs * self.sample_rate).round() as usize;
            let grace = (case.grace_secs * self.sample_rate).round() as usize;
            snare.strike(&mut samples, accent, self.accent_velocity);
            snare.strike(&mut samples, grace, case.grace_velocity);
        }
        Rendered {
            samples,
            sample_rate: self.sample_rate,
            cases,
        }
    }
}

/// A rendered sweep, with the ground truth that produced it.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub samples: Vec<f64>,
    pub sample_rate: f64,
    pub cases: Vec<FlamCase>,
}

impl Rendered {
    /// The ground truth as CSV — the file a rendered-through-a-real-kit
    /// run is measured against, since that path loses the in-memory
    /// cases.
    pub fn truth_csv(&self) -> String {
        let mut s = String::from("index,side,spacing_ms,grace_velocity,grace_secs,accent_secs\n");
        for c in &self.cases {
            s.push_str(&format!(
                "{},{},{},{},{:.6},{:.6}\n",
                c.index,
                c.side.as_str(),
                c.spacing_ms,
                c.grace_velocity,
                c.grace_secs,
                c.accent_secs
            ));
        }
        s
    }
}

/// Read back what [`Rendered::truth_csv`] wrote.
pub fn parse_truth_csv(text: &str) -> Result<Vec<FlamCase>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("index,") {
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
        if f.len() != 6 {
            return Err(format!("line {}: want 6 fields, got {}", n + 1, f.len()));
        }
        let num = |i: usize| -> Result<f64, String> {
            f[i].parse::<f64>()
                .map_err(|e| format!("line {}: field {i}: {e}", n + 1))
        };
        out.push(FlamCase {
            index: num(0)? as usize,
            side: match f[1] {
                "before" => Side::Before,
                "after" => Side::After,
                other => return Err(format!("line {}: unknown side {other:?}", n + 1)),
            },
            spacing_ms: num(2)?,
            grace_velocity: num(3)?,
            grace_secs: num(4)?,
            accent_secs: num(5)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_is_the_product_of_its_axes() {
        let sweep = FlamSweep::default();
        assert_eq!(
            sweep.cases().len(),
            sweep.spacings_ms.len() * sweep.grace_velocities.len() * sweep.sides.len()
        );
    }

    #[test]
    fn spacing_is_exact_in_both_orderings() {
        for case in FlamSweep::default().cases() {
            let [first, second] = case.strikes();
            let measured = (second - first) * 1000.0;
            assert!(
                (measured - case.spacing_ms).abs() < 1e-9,
                "{case:?} measured {measured} ms"
            );
            assert!(second > first, "{case:?} is out of order");
        }
    }

    #[test]
    fn cases_never_overlap() {
        let sweep = FlamSweep::default();
        let cases = sweep.cases();
        for pair in cases.windows(2) {
            let end = pair[0].strikes()[1];
            let next = pair[1].strikes()[0];
            assert!(
                next - end > 0.5,
                "cases {} and {} are {:.3}s apart",
                pair[0].index,
                pair[1].index,
                next - end
            );
        }
    }

    #[test]
    fn truth_csv_round_trips() {
        let rendered = FlamSweep {
            // Rendering the full grid here would be 48 s of audio for a
            // test about text.
            spacings_ms: vec![5.0, 30.0],
            grace_velocities: vec![0.25],
            ..Default::default()
        }
        .render();
        let back = parse_truth_csv(&rendered.truth_csv()).expect("parses");
        assert_eq!(back.len(), rendered.cases.len());
        for (a, b) in back.iter().zip(&rendered.cases) {
            assert_eq!(a.index, b.index);
            assert_eq!(a.side, b.side);
            assert!((a.grace_secs - b.grace_secs).abs() < 1e-6);
            assert!((a.accent_secs - b.accent_secs).abs() < 1e-6);
        }
    }
}
