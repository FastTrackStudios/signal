//! Local Chatterbox TTS backend (bearcove's `chatterbox-rs`, ONNX / ort).
//!
//! Wraps `chatterbox_rs::chatterbox::Chatterbox`: models are fetched into
//! the Hugging Face cache on first use (`hf::download_chatterbox_assets`),
//! then `synthesize()` produces 24 kHz mono PCM. Loading the models takes
//! seconds and synthesis is far from realtime — use only through
//! [`CueBank`](super::CueBank) at setlist-build time.

use std::path::PathBuf;

use chatterbox_rs::chatterbox::Chatterbox;
use chatterbox_rs::hf::{self, ModelVariant};
use chatterbox_rs::voice::{self, VoiceProfile};

use crate::GuideError;

use super::{TtsAudio, TtsRenderer};

/// Chatterbox model sample rate (`chatterbox_rs::audio::TARGET_SAMPLE_RATE`).
const CHATTERBOX_SAMPLE_RATE: u32 = 24_000;

/// dtype string cbx uses to tag voice profiles / model variants (must match
/// the `--dtype` naming so `pick_voice_for_model` resolves cached profiles).
const fn dtype_str(v: ModelVariant) -> &'static str {
    match v {
        ModelVariant::Fp32 => "fp32",
        ModelVariant::Fp16 => "fp16",
        ModelVariant::Quantized => "quantized",
        ModelVariant::Q4 => "q4",
        ModelVariant::Q4f16 => "q4f16",
        ModelVariant::Q8 => "q8",
        ModelVariant::Q8f16 => "q8f16",
    }
}

/// Configuration for [`ChatterboxTts`].
#[derive(Debug, Clone)]
pub struct ChatterboxTtsConfig {
    /// Hugging Face repo of the ONNX export.
    pub repo_id: String,
    /// Repo revision.
    pub revision: String,
    /// Which quantization variant to download/load. Quantized is a good
    /// CPU default (~4x smaller than fp32).
    pub variant: ModelVariant,
    /// Reference voice wav (Chatterbox is a voice-cloning model; a short
    /// clean clip of the desired voice is required).
    pub voice_wav: PathBuf,
    /// Stable name for the voice, used in cue cache keys.
    pub voice_name: String,
    /// Generation cap (legacy cbx default: 512).
    pub max_new_tokens: usize,
    /// Repetition penalty (cbx default: 1.2).
    pub repetition_penalty: f32,
}

impl ChatterboxTtsConfig {
    /// Default model config for a given reference voice. `Fp16` so it matches
    /// the bundled `default`/`default-fp32` voice profiles (cbx only ships
    /// fp16/fp32 profiles; a profile is tied to its model dtype).
    pub fn with_voice(voice_wav: impl Into<PathBuf>, voice_name: impl Into<String>) -> Self {
        Self {
            repo_id: "ResembleAI/chatterbox-turbo-ONNX".to_string(),
            revision: "main".to_string(),
            variant: ModelVariant::Fp16,
            voice_wav: voice_wav.into(),
            voice_name: voice_name.into(),
            max_new_tokens: 512,
            repetition_penalty: 1.2,
        }
    }
}

/// A [`TtsRenderer`] backed by a locally-loaded Chatterbox model.
///
/// Prefers a cached voice profile (the bundled `default` — no user
/// recording needed); falls back to re-encoding `config.voice_wav` when no
/// profile matches.
pub struct ChatterboxTts {
    model: Chatterbox,
    config: ChatterboxTtsConfig,
    voice_id: String,
    /// A precomputed voice profile (cbx `.cbxvoice`), when one is cached for
    /// this model tuple. `None` → synthesize from the reference wav.
    profile: Option<VoiceProfile>,
}

impl ChatterboxTts {
    /// Download (into the HF cache) and load the model, then resolve a voice:
    /// the cached `default` profile if present, else the reference wav.
    /// Blocking and slow; never call from an audio thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the model assets can't be downloaded or the
    /// model fails to load.
    pub fn load(config: ChatterboxTtsConfig) -> Result<Self, GuideError> {
        let paths =
            hf::download_chatterbox_assets(&config.repo_id, &config.revision, config.variant)
                .map_err(|e| GuideError::Tts(format!("chatterbox asset download: {e:#}")))?;
        let model = Chatterbox::load(&paths)
            .map_err(|e| GuideError::Tts(format!("chatterbox model load: {e:#}")))?;

        // Prefer a cached voice profile matching this model tuple (cbx ships a
        // `default` one via `install_default_voice.sh`). This is the
        // out-of-the-box voice — no reference recording required.
        let dtype = dtype_str(config.variant);
        let (profile, voice_name) = voice::voice_cache_dir()
            .ok()
            .and_then(|dir| {
                let name =
                    voice::pick_voice_for_model(&dir, &config.repo_id, &config.revision, dtype)
                        .ok()
                        .flatten()?;
                let profile = voice::load_voice_profile(&dir, &name).ok()?;
                Some((Some(profile), name))
            })
            .unwrap_or_else(|| (None, config.voice_name.clone()));

        let voice_id = format!(
            "chatterbox:{}@{}:{:?}:{}",
            config.repo_id, config.revision, config.variant, voice_name
        );
        Ok(Self {
            model,
            config,
            voice_id,
            profile,
        })
    }
}

impl TtsRenderer for ChatterboxTts {
    fn voice_id(&self) -> &str {
        &self.voice_id
    }

    fn render(&mut self, text: &str) -> Result<TtsAudio, GuideError> {
        let dtype = dtype_str(self.config.variant);
        let samples = if let Some(profile) = self.profile.clone() {
            self.model
                .synthesize_with_voice_profile(
                    text,
                    &self.config.repo_id,
                    &self.config.revision,
                    dtype,
                    &profile,
                    self.config.max_new_tokens,
                    self.config.repetition_penalty,
                )
                .map_err(|e| {
                    GuideError::Tts(format!("chatterbox synthesize (profile) {text:?}: {e:#}"))
                })?
        } else {
            self.model
                .synthesize(
                    text,
                    &self.config.voice_wav,
                    self.config.max_new_tokens,
                    self.config.repetition_penalty,
                )
                .map_err(|e| GuideError::Tts(format!("chatterbox synthesize {text:?}: {e:#}")))?
        };
        Ok(TtsAudio {
            samples,
            sample_rate: CHATTERBOX_SAMPLE_RATE,
        })
    }
}
