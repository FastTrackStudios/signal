//! RPP → PTX conversion.
//!
//! Two backends are supported:
//!
//! 1. **Native** (TODO): we generate the PTX bytes from a parsed RPP
//!    using the template-based writer documented in
//!    `docs/pt-reaper-converter-automation.md`. Not yet implemented;
//!    needs us to finish decoding the field encodings for clip
//!    placement, automation, sends, etc.
//!
//! 2. **Shell-out** (current): invoke the PT Reaper Converter binary
//!    via its hidden `--convert` CLI flag (discovered 2026-05-17,
//!    see `docs/pt-reaper-converter-automation.md`). Works today,
//!    requires the converter installed at one of the common paths
//!    OR `PT_REAPER_CONVERTER_BIN` env var pointing at the binary.
//!
//! The shell-out backend serves as a temporary oracle for the writer.
//! When the native writer is complete, the two can be run in parallel
//! and byte-diffed on every fixture to validate correctness.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("converter binary not found (tried: {0})")]
    BinaryNotFound(String),
    #[error("converter spawn failed: {0}")]
    SpawnFailed(#[from] std::io::Error),
    #[error("converter exited with error: {0}")]
    ConverterError(String),
    #[error("output file not produced: {0}")]
    OutputMissing(PathBuf),
    /// Caller passed an argument outside the valid range or shape.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// The requested write operation has a documented surface but its
    /// implementation is blocked on further reverse engineering. The string
    /// payload describes the specific blocker.
    #[error("unimplemented write operation: {0}")]
    Unimplemented(&'static str),
}

/// Locate the PT Reaper Converter binary. Checks (in order):
/// 1. `PT_REAPER_CONVERTER_BIN` env var (must be an executable file)
/// 2. `/Applications/PT Reaper Converter.app/Contents/MacOS/PT Reaper Converter`
///    (the standard macOS install path)
/// 3. `pt-reaper-converter` in `PATH`
pub fn find_converter_binary() -> Result<PathBuf, WriteError> {
    if let Ok(p) = std::env::var("PT_REAPER_CONVERTER_BIN") {
        let path = PathBuf::from(&p);
        if path.is_file() {
            return Ok(path);
        }
        return Err(WriteError::BinaryNotFound(p));
    }

    let app_path =
        Path::new("/Applications/PT Reaper Converter.app/Contents/MacOS/PT Reaper Converter");
    if app_path.is_file() {
        return Ok(app_path.to_path_buf());
    }

    if let Ok(found) = which_in_path("pt-reaper-converter") {
        return Ok(found);
    }

    Err(WriteError::BinaryNotFound(
        "PT_REAPER_CONVERTER_BIN unset, /Applications path missing, not in PATH".to_string(),
    ))
}

fn which_in_path(name: &str) -> Result<PathBuf, std::io::Error> {
    let path_var = std::env::var_os("PATH").ok_or_else(|| std::io::Error::other("PATH unset"))?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(std::io::Error::from(std::io::ErrorKind::NotFound))
}

/// Convert an RPP file to a PTX file by invoking the converter CLI.
///
/// On success, `output` will contain the generated PTX bytes.
///
/// The converter ALWAYS exits with status 0 (even on error); we detect
/// failure by:
/// - Looking for "ERROR:" in stdout
/// - Verifying the output file was actually produced
pub fn rpp_to_ptx_via_converter(rpp: &Path, ptx: &Path) -> Result<(), WriteError> {
    let bin = find_converter_binary()?;

    // Remove any previous output so the file-exists check is meaningful.
    let _ = std::fs::remove_file(ptx);

    let out = std::process::Command::new(&bin)
        .arg("--convert")
        .arg(rpp)
        .arg(ptx)
        .output()?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("ERROR:") {
        // Extract the first ERROR line
        let err_line = stdout
            .lines()
            .find(|l| l.contains("ERROR:"))
            .unwrap_or("ERROR (no message)");
        return Err(WriteError::ConverterError(err_line.to_string()));
    }

    if !ptx.is_file() {
        return Err(WriteError::OutputMissing(ptx.to_path_buf()));
    }

    Ok(())
}

// Smoke test for this function lives in `daw-reaper` (which depends on
// both `dawfile_protools` and `dawfile_reaper`) so we can build a real
// RPP via the builder. See `crates/daw-reaper/tests/`.
