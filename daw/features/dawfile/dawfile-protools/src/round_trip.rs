//! Parse → re-encrypt → re-parse integrity check.
//!
//! The property a reader for an undocumented format has to keep: reading a
//! session and writing it back must return the original bytes, including
//! every block this crate does not understand, and re-parsing those bytes
//! must yield the same session. A regression here is silent — the file
//! still opens, it just quietly lost something.
//!
//! This lives in the library rather than in the `round_trip` example
//! because the test that runs it over every fixture used to shell out to
//! `cargo build --example round_trip` and then spawn the binary once per
//! fixture. The build was 38 seconds; the actual round-tripping is 0.3.
//! The example is now a command-line wrapper over [`check_round_trip`] and
//! the test calls it in-process.

use std::path::Path;

/// Round-trip `path` and report the first way it failed to survive.
///
/// `Ok(())` means: the file parsed, re-encrypted to byte-identical output,
/// re-parsed, and every public field of the two sessions compared equal.
///
/// # Errors
///
/// Returns a human-readable description of the first discrepancy — a parse
/// failure, a byte that changed (with its offset), or the name of the first
/// session field that differs.
pub fn check_round_trip(path: &Path) -> Result<(), String> {
    let session_a =
        crate::read_session(path, 0).map_err(|e| format!("initial parse failed: {e}"))?;

    let original = std::fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    let raw = crate::parse_raw(original.clone()).map_err(|e| format!("parse_raw failed: {e}"))?;
    let re_encrypted = raw.encrypt();

    // Byte-level identity (must hold for unknown-block passthrough).
    if re_encrypted.len() != original.len() {
        return Err(format!(
            "byte length changed: orig={} encrypted={}",
            original.len(),
            re_encrypted.len()
        ));
    }
    for (i, (a, b)) in original.iter().zip(re_encrypted.iter()).enumerate() {
        if a != b {
            return Err(format!(
                "byte mismatch at 0x{i:04x}: orig=0x{a:02x} re-encrypted=0x{b:02x}"
            ));
        }
    }

    let mut buf = re_encrypted;
    let session_b =
        crate::parse::parse_session(&mut buf, 0).map_err(|e| format!("re-parse failed: {e}"))?;

    compare_sessions(&session_a, &session_b)
}

/// Field-by-field equality of two parses of the same session.
fn compare_sessions(a: &crate::ProToolsSession, b: &crate::ProToolsSession) -> Result<(), String> {
    macro_rules! check {
        ($field:ident) => {
            if a.$field != b.$field {
                return Err(format!(
                    "field `{}` mismatch:\n  a = {:?}\n  b = {:?}",
                    stringify!($field),
                    a.$field,
                    b.$field
                ));
            }
        };
    }
    check!(version);
    check!(session_sample_rate);
    check!(bpm);
    check!(tempo_events);
    check!(meter_events);
    check!(markers);
    check!(audio_files);
    check!(audio_regions);
    check!(audio_tracks);
    check!(midi_regions);
    check!(midi_tracks);
    check!(plugins);
    check!(io_channels);
    Ok(())
}
