//! Native file dialog without external dependencies.
//!
//! Uses the system file picker: `zenity` on Linux, `osascript` on macOS,
//! `powershell` on Windows. Falls back gracefully if unavailable.

use std::path::PathBuf;
use std::process::Command;

/// Open a native file dialog for selecting audio files.
///
/// `title` is the dialog window title. Returns `None` if the user cancels
/// or no dialog tool is available.
pub fn pick_audio_file(title: &str) -> Option<PathBuf> {
    pick_file(
        title,
        &[
            "wav", "flac", "mp3", "ogg", "aiff", "aif", "opus", "wma", "m4a",
        ],
    )
}

/// Open a native file dialog for selecting files with the given extensions.
pub fn pick_file(title: &str, extensions: &[&str]) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        pick_file_zenity(title, extensions)
    }

    #[cfg(target_os = "macos")]
    {
        pick_file_macos(title, extensions)
    }

    #[cfg(target_os = "windows")]
    {
        pick_file_windows(title, extensions)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (title, extensions);
        None
    }
}

#[cfg(target_os = "linux")]
fn pick_file_zenity(title: &str, extensions: &[&str]) -> Option<PathBuf> {
    // Build filter string for zenity: "*.wav *.flac *.mp3 ..."
    let filter_pattern: String = extensions
        .iter()
        .map(|e| format!("*.{e}"))
        .collect::<Vec<_>>()
        .join(" ");
    let filter = format!("Audio Files | {filter_pattern}");

    let output = Command::new("zenity")
        .args([
            "--file-selection",
            "--title",
            title,
            "--file-filter",
            &filter,
            "--file-filter",
            "All Files | *",
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn pick_file_macos(title: &str, extensions: &[&str]) -> Option<PathBuf> {
    let types: String = extensions
        .iter()
        .map(|e| format!("\"{e}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let script = format!(
        r#"set theFile to choose file with prompt "{title}" of type {{{types}}}
POSIX path of theFile"#,
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn pick_file_windows(title: &str, extensions: &[&str]) -> Option<PathBuf> {
    let filter_parts: String = extensions
        .iter()
        .map(|e| format!("*.{e}"))
        .collect::<Vec<_>>()
        .join(";");

    let script = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.OpenFileDialog
$f.Title = '{title}'
$f.Filter = 'Audio Files|{filter_parts}|All Files|*.*'
if ($f.ShowDialog() -eq 'OK') {{ $f.FileName }}"#,
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    } else {
        None
    }
}
