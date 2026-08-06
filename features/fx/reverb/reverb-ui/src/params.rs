//! The reverb's parameters, and the two pieces of editor state a session has
//! to remember.
//!
//! Lives here rather than in the plugin shell for the same reason the
//! compressor's do: the editor is what constrains them. The profile is a
//! parameter so a host can automate the space; the profile *id* is persisted
//! state so growing the list cannot repoint an old project at a different
//! reverb.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use atomic_float::AtomicF32;
use crossbeam_channel::Sender;
use parking_lot::Mutex;
use reverb_dsp::ir::PreparedIrPair;
use nice_plug::prelude::*;

/// Live values the editor reads and the audio thread writes, plus the
/// editor's side of IR loading.
///
/// The meters are plain atomics rather than a channel: the editor samples them
/// when it draws, and a frame that misses an update simply draws the next one.
#[derive(Default)]
pub struct ReverbUiState {
    /// Wet output level in dB, for the tail display.
    pub tail_db: AtomicF32,
    /// Input level in dB, so a face can show what is feeding it.
    pub input_db: AtomicF32,
    /// Where finished impulse responses are posted.
    ///
    /// The plugin owns the other end and hands it over at construction. The
    /// audio thread only ever *receives* on it, inside the chain, and swaps a
    /// pointer — the decode, the resample and the FFT partitioning all happen
    /// on a worker this state spawns.
    pub ir_tx: Mutex<Option<Sender<PreparedIrPair>>>,
    /// Sample rate the IR has to be resampled to. Written by the plugin when
    /// the host tells it, read by the worker.
    pub sample_rate: AtomicF32,
    /// What is loaded now — the file's display name, for the panel.
    pub ir_loaded: Mutex<String>,
    /// A decode is in flight. The panel says so rather than looking broken
    /// for the second and a half a long IR takes.
    pub ir_loading: AtomicBool,
    /// The last failure, if the file could not be read.
    pub ir_error: Mutex<Option<String>>,
}

impl ReverbUiState {
    /// Load an impulse response, off the GUI thread and nowhere near the
    /// audio one.
    ///
    /// Reading a file, resampling it and partitioning it for convolution is
    /// unbounded work — seconds, for a long IR — so it runs on a worker and
    /// arrives as finished partitions that the chain swaps in with a pointer
    /// move. Everything the panel shows while that happens (Loading…, the
    /// name, an error) is written here by the worker.
    pub fn load_ir(self: &Arc<Self>, path: PathBuf) {
        let state = self.clone();
        state.ir_loading.store(true, Ordering::Relaxed);
        *state.ir_error.lock() = None;
        std::thread::spawn(move || {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let sample_rate = state.sample_rate.load(Ordering::Relaxed) as f64;
            let sample_rate = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };

            match reverb_dsp::ir::IrAsset::load(&path, sample_rate) {
                Ok(asset) => {
                    // A mono file convolves to both sides; a stereo one keeps
                    // its own image, which is most of why you record a space
                    // in stereo in the first place.
                    let left = asset.channels[0].clone();
                    let right = asset.channels.get(1).cloned().unwrap_or_else(|| left.clone());
                    let pair = PreparedIrPair::build(&left, &right);
                    let sent = state
                        .ir_tx
                        .lock()
                        .as_ref()
                        .map(|tx| tx.send(pair).is_ok())
                        .unwrap_or(false);
                    if sent {
                        *state.ir_loaded.lock() = name;
                    } else {
                        // No channel means the editor is running without a
                        // plugin behind it (the headless harness), which is
                        // worth saying rather than silently doing nothing.
                        *state.ir_error.lock() = Some(format!("{name}: no engine attached"));
                    }
                }
                Err(e) => *state.ir_error.lock() = Some(format!("{name}: {e}")),
            }
            state.ir_loading.store(false, Ordering::Relaxed);
        });
    }
}

/// Where impulse responses are looked for.
///
/// `FTS_IR_DIR` overrides; otherwise the user's own IR folder. A missing
/// directory is not an error — it is an empty library, and the panel says so.
pub fn ir_library_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("FTS_IR_DIR") {
        return PathBuf::from(dir);
    }
    dirs_home()
        .map(|home| home.join(".local/share/fts/irs"))
        .unwrap_or_else(|| PathBuf::from("irs"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[derive(Params)]
pub struct ReverbParams {
    /// Which of the seven families' profiles is active, as an index into
    /// [`reverb_profiles::PROFILES`]. A parameter because the space is worth
    /// automating; see [`Self::profile_id`] for what a session actually saves.
    #[id = "profile"]
    pub profile: IntParam,

    #[id = "decay"]
    pub decay: FloatParam,

    #[id = "size"]
    pub size: FloatParam,

    #[id = "predelay"]
    pub predelay: FloatParam,

    #[id = "damping"]
    pub damping: FloatParam,

    #[id = "tone"]
    pub tone: FloatParam,

    #[id = "width"]
    pub width: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

    /// How dense the reflections are — sparse and grainy through to a
    /// smear with no individual echoes left in it.
    #[id = "diffusion"]
    pub diffusion: FloatParam,

    /// Movement inside the tail. A hall wants a little to stop it ringing
    /// metallically; a chorale wants a lot.
    #[id = "modulation"]
    pub modulation: FloatParam,

    /// How long the low band rings relative to the rest. Above 1 the lows
    /// outlast the top, which is what a warm hall does and a plate does not.
    #[id = "bass"]
    pub bass: FloatParam,

    /// The first of the engine's two algorithm-specific controls.
    ///
    /// Every algorithm in `reverb-dsp` takes an `extra_a` / `extra_b` pair and
    /// means something different by them: shimmer amount and pitch, magneto's
    /// saturation, bloom's feedback stages, non-linear's gate shape, spring's
    /// modulation rate. That is not a shortcoming to paper over — it is the
    /// per-family personality, and the panel is what names it. Each face
    /// legends these two with what they do on *that* machine.
    #[id = "chara"]
    pub character_a: FloatParam,

    #[id = "charb"]
    pub character_b: FloatParam,

    // ── Engine controls that `extra_a` / `extra_b` cannot reach ──────────
    //
    // The chain carries a struct per algorithm with the settings that are
    // genuinely that engine's own — a shimmer's interval in semitones, how
    // many springs are in the tank. The coarse pair gets you an amount; these
    // get you the thing itself.

    /// Shimmer's first voice, in semitones. The engine's own control: the
    /// coarse `extra_b` mapping only offers octave-up, fifth and octave-down.
    #[id = "shimint"]
    pub shimmer_interval: FloatParam,

    /// Springs in the tank (1–3). Two is the usual guitar amp; one is
    /// thinner and drips harder; three is the big outboard tank.
    #[id = "springs"]
    pub springs: IntParam,

    /// Bloom's overtone generator — octave-up partials fed into the trail.
    /// Zero is off and costs nothing.
    #[id = "harmonics"]
    pub harmonics: FloatParam,

    /// Chorale's per-voice randomisation: how much the singers drift apart
    /// from each other. Zero is one voice in unison with itself.
    #[id = "singers"]
    pub singers: FloatParam,

    /// Magneto's feedback into the tape input — the repeats piling up.
    #[id = "regen"]
    pub regen: FloatParam,

    /// Non-linear's chop depth: the LFO that cuts the tail into slices.
    #[id = "chop"]
    pub chop: FloatParam,

    /// The impulse response a session reopens with.
    ///
    /// A path rather than the audio: IRs are large, and a project that
    /// silently embedded megabytes of convolution data would be a surprise.
    /// A file that has moved is reported rather than guessed at.
    #[persist = "ir_path"]
    pub ir_path: parking_lot::RwLock<String>,

    /// What a session restores from.
    ///
    /// The index is not stable: adding a family, or another plate, renumbers
    /// everything after it, and a project saved last year would open on
    /// whatever now sits at that number. The id is stable, so it is what gets
    /// written down. Same contract as the compressor's.
    #[persist = "profile_id"]
    pub profile_id: parking_lot::RwLock<String>,

    /// The editor's form factor, persisted by id.
    #[persist = "editor_form"]
    pub editor_form: parking_lot::RwLock<String>,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            profile: IntParam::new(
                "Space",
                reverb_profiles::profile_index("hall_concert").unwrap_or(0) as i32,
                IntRange::Linear {
                    min: 0,
                    max: (reverb_profiles::PROFILES.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                reverb_profiles::PROFILES
                    .get(v.max(0) as usize)
                    .map(|p| p.name.to_string())
                    .unwrap_or_else(|| "—".to_string())
            })),

            decay: FloatParam::new("Decay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            size: FloatParam::new("Size", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            predelay: FloatParam::new(
                "Pre-Delay",
                20.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 250.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            damping: FloatParam::new("Damping", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            tone: FloatParam::new("Tone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            width: FloatParam::new("Width", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            diffusion: FloatParam::new("Diffusion", 0.7, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            modulation: FloatParam::new("Modulation", 0.2, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            bass: FloatParam::new("Bass", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            character_a: FloatParam::new("Character A", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            character_b: FloatParam::new("Character B", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            shimmer_interval: FloatParam::new(
                "Interval",
                12.0,
                FloatRange::Linear { min: -12.0, max: 12.0 },
            )
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            springs: IntParam::new("Springs", 2, IntRange::Linear { min: 1, max: 3 }),

            harmonics: FloatParam::new("Harmonics", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            singers: FloatParam::new("Singers", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            regen: FloatParam::new("Regen", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            chop: FloatParam::new("Chop", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            ir_path: parking_lot::RwLock::new(String::new()),
            profile_id: parking_lot::RwLock::new(String::new()),
            editor_form: parking_lot::RwLock::new(String::new()),
        }
    }
}

impl ReverbParams {
    /// The profile index the editor should be showing.
    ///
    /// The persisted id wins when this build still has it; otherwise the
    /// index, which is what a pre-id session has.
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        reverb_profiles::profile_index(&id)
            .unwrap_or_else(|| (self.profile.value().max(0) as usize).min(reverb_profiles::PROFILES.len() - 1))
    }

    /// The active profile.
    pub fn resolved_profile(&self) -> &'static reverb_profiles::Profile {
        &reverb_profiles::PROFILES[self.resolved_profile_index()]
    }

    /// Record the id for `index` — call wherever the profile changes.
    pub fn store_profile_id(&self, index: usize) {
        let id = reverb_profiles::PROFILES
            .get(index)
            .map(|p| p.id)
            .unwrap_or("hall_concert");
        *self.profile_id.write() = id.to_string();
    }

    pub fn resolved_editor_form(&self) -> fts_ui_audio::EditorForm {
        fts_ui_audio::EditorForm::from_id(&self.editor_form.read()).unwrap_or_default()
    }

    pub fn store_editor_form(&self, form: fts_ui_audio::EditorForm) {
        *self.editor_form.write() = form.id().to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_restores_from_the_id_and_not_the_number() {
        let params = ReverbParams::default();
        params.store_profile_id(reverb_profiles::profile_index("spring_vintage").unwrap());
        assert_eq!(params.resolved_profile().id, "spring_vintage");

        // A profile this build does not have — a project from a newer
        // version — falls back to the index rather than guessing.
        *params.profile_id.write() = "gravity_well".to_string();
        assert_eq!(
            params.resolved_profile_index(),
            params.profile.value() as usize,
        );
    }
}
