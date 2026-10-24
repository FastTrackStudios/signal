use crate::NamError;
use std::path::Path;

/// Metadata extracted from a WAV file header.
#[derive(Debug, Clone)]
pub struct IrMetadata {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub duration_ms: f64,
}

/// Parse a WAV header to extract IR metadata — header only, no sample
/// decode, so the library scanner stays cheap over big IR banks.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed as a valid WAV file.
pub fn parse_wav_header(path: &Path) -> Result<IrMetadata, NamError> {
    let info = fts_sample::probe(path).map_err(|e| NamError::ParseError(e.to_string()))?;
    Ok(IrMetadata {
        channels: info.channels,
        sample_rate: info.sample_rate,
        bits_per_sample: info.bits_per_sample,
        duration_ms: info.duration_secs() * 1000.0,
    })
}
