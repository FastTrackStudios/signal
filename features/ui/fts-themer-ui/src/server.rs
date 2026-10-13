//! Server functions — the only code here that touches disk.
//!
//! The browser never asks the server "what color is `col_cursor`?": it holds
//! the whole `.ReaperTheme` as a string and edits it locally, so a color
//! change costs no round trip and the preview updates on the same frame. The
//! server is only involved at the edges — reading the theme once, writing it
//! back, and the image work that can't happen in wasm.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// A theme as the browser needs it: the two texts it edits and previews from.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ThemeSources {
    /// Theme name (the `ui_img` folder).
    pub name: String,
    /// Full `.ReaperTheme` ini text.
    pub ini: String,
    /// Full `rtconfig.txt` text — the WALTER program the preview evaluates.
    pub rtconfig: String,
    /// Accent variants this theme currently offers.
    pub accents: Vec<String>,
}

/// Read a theme directory.
#[server]
pub async fn load_theme(path: String) -> Result<ThemeSources, ServerFnError> {
    use fts_themer::ThemeDir;

    let theme = ThemeDir::open(&path).map_err(err)?;
    let ini = std::fs::read_to_string(theme.ini_path()).map_err(err)?;
    let rtconfig = std::fs::read_to_string(theme.rtconfig_path()).map_err(err)?;
    let accents = theme.accents().unwrap_or_default();

    Ok(ThemeSources {
        name: theme.name.clone(),
        ini,
        rtconfig,
        accents,
    })
}

/// Write edited ini text back to the theme's `.ReaperTheme`.
///
/// The text is re-parsed and re-serialized rather than written through: it
/// arrived from a browser and shouldn't be trusted to still be a valid ini,
/// and the round trip is what guarantees the untouched lines are byte-identical.
#[server]
pub async fn save_theme(path: String, ini: String) -> Result<String, ServerFnError> {
    use fts_themer::{ThemeDir, ThemeIni};

    let theme = ThemeDir::open(&path).map_err(err)?;
    let parsed = ThemeIni::parse(&ini);
    if parsed.is_empty() {
        return Err(ServerFnError::new(
            "refusing to save: no [color theme] entries parsed",
        ));
    }
    let target = theme.ini_path();
    parsed.save(&target).map_err(err)?;
    Ok(target.display().to_string())
}

/// Generate an accent variant — artwork at every DPI scale plus its layouts.
#[server]
pub async fn add_accent(
    path: String,
    name: String,
    color: String,
    from: String,
    keep_tone: bool,
) -> Result<Vec<String>, ServerFnError> {
    use fts_themer::{ThemeDir, color::Rgb};

    let theme = ThemeDir::open(&path).map_err(err)?;
    let color = Rgb::parse_hex(&color).map_err(err)?;
    let report = fts_themer::add_accent(&theme, &name, color, &from, keep_tone).map_err(err)?;
    Ok(report
        .images
        .iter()
        .map(|p| p.display().to_string())
        .collect())
}

/// Flatten any error into a `ServerFnError`, keeping the whole `anyhow` chain
/// so the browser shows "read X: permission denied", not just "error".
#[cfg(feature = "server")]
fn err(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(format!("{e:#}"))
}
