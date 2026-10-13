//! Essence descriptor and locator parsing.
//!
//! A `SourceMob` has an `EssenceDescriptor` that describes the media it
//! contains.  Under audio essence descriptors (`SoundDescriptor`,
//! `PCMDescriptor`, `WAVEDescriptor`, `AIFCDescriptor`) we find:
//! - Sample rate, channel count, bit depth
//! - `Locator` objects pointing to the actual file(s) on disk

use crate::error::AafResult;
use crate::parse::auid::{
    CLASS_AIFF_DESCRIPTOR, CLASS_NETWORK_LOCATOR, CLASS_PCM_DESCRIPTOR, CLASS_SOUND_DESCRIPTOR,
    CLASS_TEXT_LOCATOR, CLASS_WAVE_DESCRIPTOR,
};
use crate::parse::cfb_store::CfbStore;
use crate::parse::pids::*;
use crate::parse::properties::Properties;
use crate::types::AudioEssenceInfo;
use std::path::Path;

/// Information extracted from an EssenceDescriptor (audio types only).
#[derive(Debug, Clone)]
pub struct EssenceDescriptorData {
    /// Audio format info, if this is a sound descriptor.
    pub audio_info: Option<AudioEssenceInfo>,
    /// Resolved file URLs from `NetworkLocator` objects.
    pub urls: Vec<String>,
    /// Human-readable paths from `TextLocator` objects.
    pub text_paths: Vec<String>,
}

/// Parse the `EssenceDescriptor` object at `desc_dir`.
///
/// Handles `SoundDescriptor`, `PCMDescriptor`, `WAVEDescriptor`,
/// `AIFCDescriptor`, and any unknown descriptor type (returning empty
/// `audio_info` but still parsing locators).
pub fn parse_essence_descriptor(
    store: &CfbStore,
    desc_dir: &Path,
) -> AafResult<EssenceDescriptorData> {
    let props_raw = store.properties(desc_dir).unwrap_or(&[]);
    let props = if props_raw.is_empty() {
        None
    } else {
        Some(Properties::parse(props_raw, desc_dir)?)
    };

    // Attempt to parse audio info if we have properties.
    let audio_info = props.as_ref().and_then(parse_audio_info);

    // Parse locators (EssenceDescriptor.Locator is a vector).
    let (urls, text_paths) = if let Some(ref p) = props {
        parse_locators(store, desc_dir, p)?
    } else {
        (Vec::new(), Vec::new())
    };

    Ok(EssenceDescriptorData {
        audio_info,
        urls,
        text_paths,
    })
}

/// Extract `AudioEssenceInfo` from a SoundDescriptor / PCMDescriptor.
fn parse_audio_info(props: &Properties) -> Option<AudioEssenceInfo> {
    let class = props.effective_class()?;

    // Accept PCMDescriptor, SoundDescriptor, WAVEDescriptor, AIFCDescriptor.
    let is_audio = class == CLASS_PCM_DESCRIPTOR
        || class == CLASS_SOUND_DESCRIPTOR
        || class == CLASS_WAVE_DESCRIPTOR
        || class == CLASS_AIFF_DESCRIPTOR;
    if !is_audio {
        return None;
    }

    // AudioSamplingRate: prefer SoundDescriptor PID, fall back to FileDescriptor.
    let sample_rate = props
        .edit_rate(PID_SOUND_DESCRIPTOR_AUDIO_SAMPLING_RATE)
        .map(|r| r.numerator as u32)
        .or_else(|| {
            props
                .edit_rate(PID_FILE_DESCRIPTOR_SAMPLE_RATE)
                .map(|r| r.numerator as u32)
        })
        .unwrap_or(0);

    let channels = props.u32_le(PID_SOUND_DESCRIPTOR_CHANNELS).unwrap_or(1);
    let quantization_bits = props
        .u32_le(PID_SOUND_DESCRIPTOR_QUANTIZATION_BITS)
        .unwrap_or(16);
    let length_samples = props.i64_le(PID_FILE_DESCRIPTOR_LENGTH).unwrap_or(0);

    Some(AudioEssenceInfo {
        sample_rate,
        channels,
        quantization_bits,
        length_samples,
    })
}

/// Parse all `Locator` objects referenced from this EssenceDescriptor.
/// Returns `(network_urls, text_paths)`.
fn parse_locators(
    store: &CfbStore,
    desc_dir: &Path,
    props: &Properties,
) -> AafResult<(Vec<String>, Vec<String>)> {
    let mut urls = Vec::new();
    let mut texts = Vec::new();

    // The Locator property is a strong ref vector.
    let coll_name = match props.strong_ref_collection(PID_ESSENCE_DESCRIPTOR_LOCATOR) {
        Some((_, name)) => name,
        None => return Ok((urls, texts)),
    };

    let coll_dir = desc_dir.join(&coll_name);
    let locator_dirs = store.vector_elements(&coll_dir);

    for loc_dir in locator_dirs {
        let loc_props_raw = match store.properties(&loc_dir) {
            Some(r) => r,
            None => continue,
        };
        let loc_props = Properties::parse(loc_props_raw, &loc_dir)?;
        let class = match loc_props.effective_class() {
            Some(c) => c,
            None => continue,
        };

        if class == CLASS_NETWORK_LOCATOR {
            if let Some(url) = loc_props.string(PID_NETWORK_LOCATOR_URL) {
                urls.push(url);
            }
        } else if class == CLASS_TEXT_LOCATOR
            && let Some(name) = loc_props.string(PID_TEXT_LOCATOR_NAME)
        {
            texts.push(name);
        }
        // Other locator types (EmbeddedLocator, etc.) are ignored.
    }

    Ok((urls, texts))
}

/// Return the best available file reference: prefer `file://` URLs over
/// text locator paths.
pub fn best_locator_url(data: &EssenceDescriptorData) -> Option<String> {
    // Prefer a `file://` or `file:` URL.
    for url in &data.urls {
        if url.to_lowercase().starts_with("file:") {
            return Some(url.clone());
        }
    }
    // Fall back to any URL.
    if let Some(url) = data.urls.first() {
        return Some(url.clone());
    }
    // Fall back to text locator path.
    data.text_paths.first().cloned()
}
