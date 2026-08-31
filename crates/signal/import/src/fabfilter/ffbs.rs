//! Parser for FabFilter's `FFBS` state container.
//!
//! This is the state FabFilter's **CLAP** plugins write into a host project —
//! the `<STATE>` block of a REAPER `<CLAP …>` FX entry — as opposed to the
//! `.ffp` preset files handled by [`super::parser`]. Binary `.ffp` files use
//! the same container, so this parser serves both.
//!
//! Unlike the VST3 three-segment framing (see `daw-reaper`'s `plugin_bridge`),
//! every base64 line of a CLAP `<STATE>` block concatenates into **one** byte
//! stream before decoding.
//!
//! ```text
//! magic   "FFBS"                    4 B
//! version u32 LE                    4 B   (1 in every observed build)
//! count   u32 LE = N                4 B
//! params  f32 LE × N                4N B
//! trailer 4CC + preset metadata     rest  (see FfbsMetadata)
//! ```
//!
//! See `spec/project-state-formats.md` for the field-level decode.

/// Preset metadata from the trailer that follows the float vector.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FfbsMetadata {
    /// 4-character plugin signature, e.g. `FQ4p` (Pro-Q 4), `FPGr` (Pro-G).
    pub signature: String,
    /// Preset name, e.g. `"Tom High Sustain"`.
    pub preset_name: Option<String>,
    /// Preset folder, e.g. `"Toms (2)"`.
    pub folder: Option<String>,
    /// Key/value pairs from the `CuSV` section (`AUTHOR`, `DESCRIPTION`, …).
    pub fields: Vec<(String, String)>,
}

impl FfbsMetadata {
    /// Look up a `CuSV` field by key (case-insensitive).
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// `AUTHOR`, if present.
    pub fn author(&self) -> Option<&str> {
        self.field("AUTHOR")
    }

    /// `DESCRIPTION`, if present.
    pub fn description(&self) -> Option<&str> {
        self.field("DESCRIPTION")
    }
}

/// A decoded FabFilter `FFBS` state blob.
#[derive(Debug, Clone, PartialEq)]
pub struct FfbsState {
    /// Container format version (`1` in every observed build).
    pub version: u32,
    /// The flat parameter vector. Layout is plugin-specific — see
    /// [`super::proq4`] for Pro-Q 4.
    pub params: Vec<f32>,
    /// Trailer metadata. Absent (default) if the blob has no trailer.
    pub metadata: FfbsMetadata,
}

/// Errors from parsing an `FFBS` blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FfbsError {
    /// Fewer than the 12 header bytes.
    TooShort,
    /// The first four bytes were not `FFBS`.
    BadMagic([u8; 4]),
    /// The declared float count does not fit in the remaining bytes.
    TruncatedParams { declared: usize, available: usize },
}

impl std::fmt::Display for FfbsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfbsError::TooShort => write!(f, "FFBS blob shorter than its 12-byte header"),
            FfbsError::BadMagic(m) => {
                write!(f, "not an FFBS blob: magic {:?}", String::from_utf8_lossy(m))
            }
            FfbsError::TruncatedParams {
                declared,
                available,
            } => write!(
                f,
                "FFBS declares {declared} floats but only {available} are present"
            ),
        }
    }
}

impl std::error::Error for FfbsError {}

/// Whether `bytes` starts with the `FFBS` magic.
/// Write a parameter vector back out as an `FFBS` blob.
///
/// The inverse of [`parse`], for pushing a decoded preset *into* a hosted
/// plugin through `load_state`. That is the only way to reach several of
/// Pro-Q's parameters: it refuses host writes to `Used`, `Dynamic Range`,
/// `Threshold` and the dynamics timing — they come back at their defaults —
/// so a preset cannot be set up one parameter at a time. Loading it as state
/// is how the plugin's own behavior becomes measurable, which is what the mode
/// tables and the threshold span were recovered with.
///
/// The trailer is omitted: it carries the preset's name and author, which the
/// plugin does not need in order to sound like the preset.
pub fn encode(params: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + params.len() * 4);
    out.extend_from_slice(b"FFBS");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(params.len() as u32).to_le_bytes());
    for v in params {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

pub fn is_ffbs(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[..4] == b"FFBS"
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Parse an `FFBS` blob.
///
/// A malformed *trailer* is not an error — the parameter vector is the useful
/// payload, so metadata degrades to [`FfbsMetadata::default`].
pub fn parse(bytes: &[u8]) -> Result<FfbsState, FfbsError> {
    if bytes.len() < 12 {
        return Err(FfbsError::TooShort);
    }
    if !is_ffbs(bytes) {
        let mut m = [0u8; 4];
        m.copy_from_slice(&bytes[..4]);
        return Err(FfbsError::BadMagic(m));
    }

    let version = u32_at(bytes, 4).ok_or(FfbsError::TooShort)?;
    let count = u32_at(bytes, 8).ok_or(FfbsError::TooShort)? as usize;

    let available = (bytes.len() - 12) / 4;
    if count > available {
        return Err(FfbsError::TruncatedParams {
            declared: count,
            available,
        });
    }

    let mut params = Vec::with_capacity(count);
    for i in 0..count {
        let o = 12 + i * 4;
        params.push(f32::from_le_bytes([
            bytes[o],
            bytes[o + 1],
            bytes[o + 2],
            bytes[o + 3],
        ]));
    }

    let metadata = parse_metadata(&bytes[12 + count * 4..]).unwrap_or_default();

    Ok(FfbsState {
        version,
        params,
        metadata,
    })
}

/// Read a `[u32 LE length][bytes]` string. Returns the string and the offset
/// just past it.
fn lp_string(b: &[u8], off: usize) -> Option<(String, usize)> {
    let len = u32_at(b, off)? as usize;
    // Guard against the `0xffffffff` sentinel and any corrupt length.
    if len > b.len().saturating_sub(off + 4) {
        return None;
    }
    let s = String::from_utf8_lossy(b.get(off + 4..off + 4 + len)?).into_owned();
    Some((s, off + 4 + len))
}

/// Parse the trailer that follows the float vector.
///
/// ```text
/// 4CC signature | u32 version | <lp preset name> | u32 sentinel | u32
/// <lp folder> | "CuSV" | u32 count | count × (<lp key> <lp value>)
/// ```
///
/// The bytes between the folder and `CuSV` vary between plugins, so the
/// key/value section is located by scanning for the `CuSV` tag rather than by
/// a fixed offset.
fn parse_metadata(b: &[u8]) -> Option<FfbsMetadata> {
    if b.len() < 8 {
        return None;
    }
    let signature = String::from_utf8_lossy(&b[..4]).into_owned();

    let mut meta = FfbsMetadata {
        signature,
        ..Default::default()
    };

    // Preset name follows the 4CC + trailer version.
    let mut off = 8;
    if let Some((name, next)) = lp_string(b, off) {
        if !name.is_empty() {
            meta.preset_name = Some(name);
        }
        off = next;
    }

    // Skip the sentinel (0xffffffff) and the following count word, then read
    // the folder. Both are fixed-width, so a short trailer just ends here.
    off += 8;
    if let Some((folder, _)) = lp_string(b, off) {
        if !folder.is_empty() {
            meta.folder = Some(folder);
        }
    }

    // Key/value section: find the `CuSV` tag.
    if let Some(pos) = b.windows(4).position(|w| w == b"CuSV") {
        let mut o = pos + 4;
        // A version word precedes the pair count.
        o += 4;
        if let Some(count) = u32_at(b, o) {
            o += 4;
            for _ in 0..count.min(64) {
                let Some((key, next)) = lp_string(b, o) else {
                    break;
                };
                let Some((val, next2)) = lp_string(b, next) else {
                    break;
                };
                meta.fields.push((key, val));
                o = next2;
            }
        }
    }

    Some(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal FFBS blob with the trailer shape the plugins emit.
    fn blob(sig: &[u8; 4], params: &[f32], preset: &str, folder: &str) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"FFBS");
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&(params.len() as u32).to_le_bytes());
        for p in params {
            v.extend_from_slice(&p.to_le_bytes());
        }
        v.extend_from_slice(sig);
        v.extend_from_slice(&3u32.to_le_bytes());
        v.extend_from_slice(&(preset.len() as u32).to_le_bytes());
        v.extend_from_slice(preset.as_bytes());
        v.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&(folder.len() as u32).to_le_bytes());
        v.extend_from_slice(folder.as_bytes());
        v.extend_from_slice(b"CuSV");
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes()); // one pair
        for s in ["AUTHOR", "bManic"] {
            v.extend_from_slice(&(s.len() as u32).to_le_bytes());
            v.extend_from_slice(s.as_bytes());
        }
        v
    }

    #[test]
    fn parses_header_params_and_trailer() {
        let raw = blob(b"FC3p", &[1.0, -12.0, 0.5], "Tom High Sustain", "T2 (2)");
        let st = parse(&raw).unwrap();

        assert_eq!(st.version, 1);
        assert_eq!(st.params, vec![1.0, -12.0, 0.5]);
        assert_eq!(st.metadata.signature, "FC3p");
        assert_eq!(st.metadata.preset_name.as_deref(), Some("Tom High Sustain"));
        assert_eq!(st.metadata.folder.as_deref(), Some("T2 (2)"));
        assert_eq!(st.metadata.author(), Some("bManic"));
    }

    #[test]
    fn rejects_non_ffbs_and_truncated() {
        assert!(matches!(parse(b"nope---------"), Err(FfbsError::BadMagic(_))));
        assert_eq!(parse(b"FFBS"), Err(FfbsError::TooShort));

        // Declares 10 floats, supplies 2.
        let mut raw = Vec::from(*b"FFBS");
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.extend_from_slice(&10u32.to_le_bytes());
        raw.extend_from_slice(&[0u8; 8]);
        assert_eq!(
            parse(&raw),
            Err(FfbsError::TruncatedParams {
                declared: 10,
                available: 2
            })
        );
    }

    #[test]
    fn missing_trailer_is_not_an_error() {
        let mut raw = Vec::from(*b"FFBS");
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.extend_from_slice(&2.5f32.to_le_bytes());
        let st = parse(&raw).unwrap();
        assert_eq!(st.params, vec![2.5]);
        assert_eq!(st.metadata, FfbsMetadata::default());
    }

    /// What `encode` writes, `parse` reads back unchanged.
    #[test]
    fn a_round_trip_preserves_the_parameter_vector() {
        let params: Vec<f32> = (0..600).map(|i| i as f32 * 0.017).collect();
        let blob = encode(&params);
        assert!(is_ffbs(&blob), "the encoder must produce something the parser accepts");

        let back = parse(&blob).expect("parses");
        assert_eq!(back.version, 1);
        assert_eq!(back.params, params);
        // No trailer was written, so none is claimed.
        assert_eq!(back.metadata, FfbsMetadata::default());
    }
}

