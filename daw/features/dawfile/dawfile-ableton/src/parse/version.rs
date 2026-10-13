//! Ableton version detection.
//!
//! The version lives in the `<Ableton>` root element's `MinorVersion` attribute.
//! Format: `"MAJOR.MINOR_PATCH"` (e.g., `"12.0_12049"`, `"11.2_11215"`)
//! or `"MAJOR.MINOR.PATCH"` (e.g., `"12.0.12120"`).
//!
//! The `MajorVersion` attribute is always "5" and is NOT the actual version.

use crate::error::{AbletonError, AbletonResult};
use crate::types::AbletonVersion;
use roxmltree::Node;

/// Parse the Ableton version from the root `<Ableton>` element.
pub fn parse_version(root: Node<'_, '_>) -> AbletonResult<AbletonVersion> {
    let minor_version = root
        .attribute("MinorVersion")
        .ok_or(AbletonError::InvalidVersion("missing MinorVersion".into()))?;

    let creator = root.attribute("Creator").unwrap_or("").to_string();

    // Check for beta
    let beta = root
        .attribute("SchemaChangeCount")
        .is_some_and(|v| v == "beta");

    // Parse the version string.
    // Formats seen in the wild:
    //   "12.0_12049"    (underscore separator)
    //   "12.0.12120"    (dot separator)
    //   "11.0_11202"
    //   "10.0.2_11.0.0" (complex, take first part)
    let version_part = minor_version.split('_').next().unwrap_or(minor_version);
    let parts: Vec<&str> = version_part.split('.').collect();

    let major: u32 = parts
        .first()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AbletonError::InvalidVersion(minor_version.to_string()))?;

    let minor: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    // Patch can come from after underscore or third dot component
    let patch: u32 = if minor_version.contains('_') {
        minor_version
            .split('_')
            .nth(1)
            .and_then(|s| s.split('.').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    } else {
        parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0)
    };

    if major < 8 {
        return Err(AbletonError::UnsupportedVersion { major, minor });
    }

    Ok(AbletonVersion {
        major,
        minor,
        patch,
        beta,
        creator,
    })
}
