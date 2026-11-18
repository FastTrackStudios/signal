//! Parser for `FabFilter` `.ffp` text-format presets (Pro-Q 4, Pro-C 3).
//!
//! Text-format `.ffp` files use an INI-like structure:
//! ```text
//! [Preset]
//! Signature=FFPQ
//! Author=FabFilter
//! Description=A warm EQ curve
//! Tags=Drums,Bright,Bus
//! [Parameters]
//! Band 1 Frequency=9.96
//! ...
//! ```
//! Binary `.ffp` files start with a 4-byte signature (no `[Preset]` header).

/// Parsed metadata and parameters from a text-format `.ffp` file.
#[derive(Debug, Clone, PartialEq)]
pub struct FfpPreset {
    pub signature: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    /// Raw parameter key-value pairs from the `[Parameters]` section,
    /// in the order they appear in the file.
    pub parameters: Vec<(String, f64)>,
}

/// Check whether raw bytes represent a text-format `.ffp` file.
///
/// Text presets start with `[Preset]` (possibly with a UTF-8 BOM).
#[must_use]
pub fn is_text_format(bytes: &[u8]) -> bool {
    let s = std::str::from_utf8(bytes).unwrap_or("");
    let trimmed = s.trim_start_matches('\u{feff}').trim_start();
    trimmed.starts_with("[Preset]")
}

/// Parse metadata and parameters from a text-format `.ffp` file.
///
/// Extracts Signature, Author, Description, and Tags from the `[Preset]` section,
/// and all key=value pairs from the `[Parameters]` section.
///
/// # Errors
///
/// Returns an error if the `Signature` field is missing from the `[Preset]` section.
pub fn parse_ffp_text(content: &str) -> eyre::Result<FfpPreset> {
    let content = content.trim_start_matches('\u{feff}');
    let mut signature = String::new();
    let mut author = None;
    let mut description = None;
    let mut tags = Vec::new();
    let mut parameters = Vec::new();

    #[derive(PartialEq)]
    enum Section {
        None,
        Preset,
        Parameters,
    }
    let mut section = Section::None;

    for line in content.lines() {
        let line = line.trim();

        if line.eq_ignore_ascii_case("[Preset]") {
            section = Section::Preset;
            continue;
        }
        if line.eq_ignore_ascii_case("[Parameters]") {
            section = Section::Parameters;
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = Section::None;
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match section {
                Section::Preset => match key {
                    "Signature" => signature = value.to_string(),
                    "Author" => {
                        let v = value.trim_matches('"');
                        if !v.is_empty() {
                            author = Some(v.to_string());
                        }
                    }
                    "Description" => {
                        let v = value.trim_matches('"');
                        if !v.is_empty() {
                            description = Some(v.to_string());
                        }
                    }
                    "Tags" => {
                        let v = value.trim_matches('"');
                        tags = v
                            .split(',')
                            .map(|t| t.trim().to_string())
                            .filter(|t| !t.is_empty())
                            .collect();
                    }
                    _ => {}
                },
                Section::Parameters => {
                    if let Ok(v) = value.parse::<f64>() {
                        parameters.push((key.to_string(), v));
                    }
                }
                Section::None => {}
            }
        }
    }

    if signature.is_empty() {
        eyre::bail!("Missing Signature in [Preset] section");
    }

    Ok(FfpPreset {
        signature,
        author,
        description,
        tags,
        parameters,
    })
}

/// Look up a parameter value by key from the ordered parameter list.
fn param_get(params: &[(String, f64)], key: &str) -> Option<f64> {
    params.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
}

/// Extract meaningful block parameters from parsed `FabFilter` parameters.
///
/// Rather than dumping all 400+ band parameters, extracts only the parameters
/// that are useful for browsing: active EQ bands, compressor controls, etc.
/// Values are normalized to `[0.0, 1.0]` for display as percentages.
///
/// Returns `ImportedParameter` values that include the original DAW parameter
/// name when it differs from the shortened display name. This allows the apply
/// path to send the correct name to `set_parameter_by_name`.
#[must_use]
pub fn extract_block_parameters(
    signature: &str,
    params: &[(String, f64)],
) -> Vec<crate::types::ImportedParameter> {
    match signature {
        "FQ4p" | "FFPQ" => extract_proq_params(params),
        "FC3p" | "FFPC" => extract_proc_params(params),
        _ => extract_generic_params(params),
    }
}

/// Pro-Q 4: extract active bands with Frequency, Gain, Q, Shape.
///
/// Display names are shortened (`"B1 Freq"`) while `daw_name` preserves
/// the original FFP parameter key (`"Band 1 Frequency"`) that REAPER uses.
fn extract_proq_params(params: &[(String, f64)]) -> Vec<crate::types::ImportedParameter> {
    use crate::types::ImportedParameter;
    let mut result = Vec::new();

    // Find which bands are active
    for band in 1..=24 {
        let used = param_get(params, &format!("Band {band} Used")).unwrap_or(0.0);
        let enabled = param_get(params, &format!("Band {band} Enabled")).unwrap_or(0.0);

        if used < 0.5 || enabled < 0.5 {
            continue;
        }

        let freq = param_get(params, &format!("Band {band} Frequency")).unwrap_or(10.0);
        let gain = param_get(params, &format!("Band {band} Gain")).unwrap_or(0.0);
        let q = param_get(params, &format!("Band {band} Q")).unwrap_or(0.5);
        let shape = param_get(params, &format!("Band {band} Shape")).unwrap_or(0.0);

        let shape_name = match shape as u32 {
            1 => "Low Shelf",
            2 => "Low Cut",
            3 => "High Shelf",
            4 => "High Cut",
            5 => "Notch",
            6 => "Band Pass",
            7 => "Tilt Shelf",
            8 => "Flat Tilt",
            _ => "Bell",
        };

        // Frequency is log2(Hz), range ~3.32 (10Hz) to ~14.29 (20kHz)
        let freq_norm = ((freq - 3.32) / (14.29 - 3.32)).clamp(0.0, 1.0) as f32;
        // Gain range: roughly -30dB to +30dB
        let gain_norm = ((gain + 30.0) / 60.0).clamp(0.0, 1.0) as f32;
        // Q is already roughly 0-10, normalize
        let q_norm = (q / 10.0).clamp(0.0, 1.0) as f32;
        // Shape is discrete 0-8 (9 values), normalize for REAPER
        let shape_norm = (shape / 8.0).clamp(0.0, 1.0) as f32;

        // Enable the band — without these, the band stays invisible in REAPER
        result.push(ImportedParameter {
            name: format!("B{band} Used"),
            value: 1.0,
            daw_name: Some(format!("Band {band} Used")),
        });
        result.push(ImportedParameter {
            name: format!("B{band} Enabled"),
            value: 1.0,
            daw_name: Some(format!("Band {band} Enabled")),
        });
        result.push(ImportedParameter {
            name: format!("B{band} Freq"),
            value: freq_norm,
            daw_name: Some(format!("Band {band} Frequency")),
        });
        result.push(ImportedParameter {
            name: format!("B{band} Gain"),
            value: gain_norm,
            daw_name: Some(format!("Band {band} Gain")),
        });
        result.push(ImportedParameter {
            name: format!("B{band} Q"),
            value: q_norm,
            daw_name: Some(format!("Band {band} Q")),
        });
        result.push(ImportedParameter {
            name: format!("B{band} {shape_name}"),
            value: shape_norm,
            daw_name: Some(format!("Band {band} Shape")),
        });
    }

    // Global params — names already match REAPER, so daw_name is None
    if let Some(output) = param_get(params, "Output Level") {
        result.push(ImportedParameter {
            name: "Output".into(),
            value: ((output + 36.0) / 72.0).clamp(0.0, 1.0) as f32,
            daw_name: Some("Output Level".into()),
        });
    }
    if let Some(gain_scale) = param_get(params, "Gain Scale") {
        result.push(ImportedParameter {
            name: "Gain Scale".into(),
            value: gain_scale.clamp(0.0, 1.0) as f32,
            daw_name: None,
        });
    }
    if let Some(auto_gain) = param_get(params, "Auto Gain") {
        result.push(ImportedParameter {
            name: "Auto Gain".into(),
            value: auto_gain.clamp(0.0, 1.0) as f32,
            daw_name: None,
        });
    }

    result
}

/// Pro-C 3: extract main compressor controls.
///
/// Pro-C parameter names already match REAPER's native names, so `daw_name` is `None`.
fn extract_proc_params(params: &[(String, f64)]) -> Vec<crate::types::ImportedParameter> {
    use crate::types::ImportedParameter;
    let mut result = Vec::new();

    // Style (enum 0-7) — display name is decorated, daw_name preserves "Style"
    if let Some(style) = param_get(params, "Style") {
        let style_name = match style as u32 {
            1 => "Classic",
            2 => "Opto",
            3 => "Vocal",
            4 => "Mastering",
            5 => "Bus",
            6 => "Punch",
            7 => "Pumping",
            _ => "Clean",
        };
        result.push(ImportedParameter {
            name: format!("Style: {style_name}"),
            value: (style / 7.0).clamp(0.0, 1.0) as f32,
            daw_name: Some("Style".into()),
        });
    }

    // Core controls — in the order they appear on the plugin UI
    let controls = [
        ("Threshold", -60.0, 0.0),
        ("Ratio", 1.0, 20.0),
        ("Knee", 0.0, 72.0),
        ("Range", 0.0, 60.0),
        ("Attack", 0.0, 1.0),
        ("Release", 0.0, 2.5),
        ("Hold", 0.0, 500.0),
        ("Lookahead", 0.0, 20.0),
        ("Mix", 0.0, 1.0),
        ("Wet Gain", -1.0, 1.0),
        ("Dry Gain", -1.0, 1.0),
        ("Input Level", -36.0, 36.0),
        ("Output Level", -36.0, 36.0),
    ];

    for (name, min, max) in controls {
        if let Some(val) = param_get(params, name) {
            let norm = ((val - min) / (max - min)).clamp(0.0, 1.0) as f32;
            result.push(ImportedParameter {
                name: name.to_string(),
                value: norm,
                daw_name: None,
            });
        }
    }

    if let Some(auto_gain) = param_get(params, "Auto Gain") {
        result.push(ImportedParameter {
            name: "Auto Gain".into(),
            value: auto_gain.clamp(0.0, 1.0) as f32,
            daw_name: None,
        });
    }
    if let Some(auto_release) = param_get(params, "Auto Release") {
        result.push(ImportedParameter {
            name: "Auto Release".into(),
            value: auto_release.clamp(0.0, 1.0) as f32,
            daw_name: None,
        });
    }

    result
}

/// Generic: extract all parameters with simple 0-1 clamping.
fn extract_generic_params(params: &[(String, f64)]) -> Vec<crate::types::ImportedParameter> {
    params
        .iter()
        .map(|(k, v)| crate::types::ImportedParameter {
            name: k.clone(),
            value: v.clamp(0.0, 1.0) as f32,
            daw_name: None,
        })
        .collect()
}
