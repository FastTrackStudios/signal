//! Serialization logic for writing REAPER render preset files.

use super::types::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format a preset name for output: quote it when it contains spaces.
fn format_name(name: &str) -> String {
    if name.contains(' ') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

/// Format a string token (filename / directory) for output.
///
/// - Empty string  → `""`
/// - Contains spaces → `"value"`
/// - Otherwise     → bare value
fn format_string_token(s: &str) -> String {
    if s.is_empty() || s.contains(' ') {
        format!("\"{}\"", s)
    } else {
        s.to_string()
    }
}

/// Format a floating-point number for output.
///
/// REAPER writes integers without a decimal point when the fractional part is
/// zero, and uses the shortest representation otherwise.
fn format_f64(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        // Use Rust's default Display, which gives a reasonable shortest form.
        format!("{v}")
    }
}

// ---------------------------------------------------------------------------
// Entry serializers
// ---------------------------------------------------------------------------

fn serialize_renderpreset(p: &RenderPreset, out: &mut String) {
    out.push_str(&format!(
        "<RENDERPRESET {} {} {} {} {} {} {} {}\r\n",
        format_name(&p.name),
        p.sample_rate,
        p.channels,
        p.offline_mode,
        p.use_project_sample_rate,
        p.resample_mode,
        p.various_checkboxes,
        p.various_checkboxes2,
    ));
    out.push_str(&format!("  {}\r\n", p.render_cfg));
    out.push_str(">\r\n");
}

fn serialize_renderpreset2(p: &RenderPreset2, out: &mut String) {
    out.push_str(&format!("<RENDERPRESET2 {}\r\n", format_name(&p.name)));
    out.push_str(&format!("  {}\r\n", p.render_cfg));
    out.push_str(">\r\n");
}

fn serialize_renderpreset_output(o: &RenderPresetOutput, out: &mut String) {
    out.push_str(&format!(
        "RENDERPRESET_OUTPUT {} {} {} {} {} {} {} {} {} {}\r\n",
        format_name(&o.name),
        o.bounds,
        format_f64(o.start),
        format_f64(o.end),
        o.source,
        o.unknown,
        format_string_token(&o.filename),
        o.tail,
        format_string_token(&o.directory),
        o.tail_ms,
    ));
}

fn serialize_renderpreset_ext(e: &RenderPresetExt, out: &mut String) {
    out.push_str(&format!(
        "RENDERPRESET_EXT {} {} {} {} {} {} {} {} {} {} {} {}\r\n",
        format_name(&e.name),
        e.normalize_mode,
        format_f64(e.normalize_target),
        format_f64(e.brickwall_target),
        format_f64(e.fadein_length),
        format_f64(e.fadeout_length),
        e.fadein_shape,
        e.fadeout_shape,
        format_f64(e.trim_lead),
        format_f64(e.trim_trail),
        format_f64(e.pad_start),
        format_f64(e.pad_end),
    ));
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl RenderPresetFile {
    /// Serialize this file back to the `reaper-render.ini` text format.
    ///
    /// Each line is terminated with `\r\n` (CR-LF) to match REAPER's format.
    pub fn serialize(&self) -> String {
        let mut output = String::new();
        for entry in &self.entries {
            match entry {
                RenderPresetEntry::Preset(p) => serialize_renderpreset(p, &mut output),
                RenderPresetEntry::Preset2(p) => serialize_renderpreset2(p, &mut output),
                RenderPresetEntry::Output(o) => serialize_renderpreset_output(o, &mut output),
                RenderPresetEntry::Ext(e) => serialize_renderpreset_ext(e, &mut output),
            }
        }
        output
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_renderpreset_unquoted() {
        let f = RenderPresetFile {
            entries: vec![RenderPresetEntry::Preset(RenderPreset {
                name: "MyPreset".to_string(),
                sample_rate: 44100,
                channels: 2,
                offline_mode: 0,
                use_project_sample_rate: 0,
                resample_mode: 3,
                various_checkboxes: 0,
                various_checkboxes2: 0,
                render_cfg: "SGFsbG8=".to_string(),
            })],
        };
        assert_eq!(
            f.serialize(),
            "<RENDERPRESET MyPreset 44100 2 0 0 3 0 0\r\n  SGFsbG8=\r\n>\r\n"
        );
    }

    #[test]
    fn serialize_renderpreset_quoted_name() {
        let f = RenderPresetFile {
            entries: vec![RenderPresetEntry::Preset(RenderPreset {
                name: "My Preset".to_string(),
                sample_rate: 48000,
                channels: 1,
                offline_mode: 2,
                use_project_sample_rate: 1,
                resample_mode: 0,
                various_checkboxes: 5,
                various_checkboxes2: 4,
                render_cfg: "YWJj".to_string(),
            })],
        };
        assert_eq!(
            f.serialize(),
            "<RENDERPRESET \"My Preset\" 48000 1 2 1 0 5 4\r\n  YWJj\r\n>\r\n"
        );
    }

    #[test]
    fn serialize_renderpreset2() {
        let f = RenderPresetFile {
            entries: vec![RenderPresetEntry::Preset2(RenderPreset2 {
                name: "Secondary".to_string(),
                render_cfg: "bXAz".to_string(),
            })],
        };
        assert_eq!(f.serialize(), "<RENDERPRESET2 Secondary\r\n  bXAz\r\n>\r\n");
    }

    #[test]
    fn serialize_renderpreset2_quoted_name() {
        let f = RenderPresetFile {
            entries: vec![RenderPresetEntry::Preset2(RenderPreset2 {
                name: "My Secondary".to_string(),
                render_cfg: "bXAz".to_string(),
            })],
        };
        assert_eq!(
            f.serialize(),
            "<RENDERPRESET2 \"My Secondary\"\r\n  bXAz\r\n>\r\n"
        );
    }

    #[test]
    fn serialize_output_bare() {
        let f = RenderPresetFile {
            entries: vec![RenderPresetEntry::Output(RenderPresetOutput {
                name: "MyPreset".to_string(),
                bounds: 1,
                start: 0.0,
                end: 0.0,
                source: 0,
                unknown: 0,
                filename: "$project".to_string(),
                tail: 0,
                directory: "".to_string(),
                tail_ms: 1000,
            })],
        };
        assert_eq!(
            f.serialize(),
            "RENDERPRESET_OUTPUT MyPreset 1 0 0 0 0 $project 0 \"\" 1000\r\n"
        );
    }

    #[test]
    fn serialize_output_quoted_fields() {
        let f = RenderPresetFile {
            entries: vec![RenderPresetEntry::Output(RenderPresetOutput {
                name: "My Preset".to_string(),
                bounds: 0,
                start: 1.5,
                end: 10.25,
                source: 1,
                unknown: 0,
                filename: "my output".to_string(),
                tail: 1,
                directory: "/home/user/audio".to_string(),
                tail_ms: 500,
            })],
        };
        assert_eq!(
            f.serialize(),
            "RENDERPRESET_OUTPUT \"My Preset\" 0 1.5 10.25 1 0 \"my output\" 1 /home/user/audio 500\r\n"
        );
    }

    #[test]
    fn serialize_ext() {
        let f = RenderPresetFile {
            entries: vec![RenderPresetEntry::Ext(RenderPresetExt {
                name: "MyPreset".to_string(),
                normalize_mode: 0,
                normalize_target: 1.0,
                brickwall_target: 1.0,
                fadein_length: 0.0,
                fadeout_length: 0.0,
                fadein_shape: 0,
                fadeout_shape: 0,
                trim_lead: 0.031623,
                trim_trail: 0.031623,
                pad_start: 0.0,
                pad_end: 0.0,
            })],
        };
        let s = f.serialize();
        assert!(s.starts_with("RENDERPRESET_EXT MyPreset 0 1 1 0 0 0 0 "));
        assert!(s.ends_with("0 0\r\n"));
    }

    #[test]
    fn format_f64_integer_value() {
        assert_eq!(super::format_f64(0.0), "0");
        assert_eq!(super::format_f64(44100.0), "44100");
        assert_eq!(super::format_f64(-1.0), "-1");
    }

    #[test]
    fn format_f64_fractional_value() {
        assert_eq!(super::format_f64(1.5), "1.5");
        assert_eq!(super::format_f64(10.25), "10.25");
    }

    #[test]
    fn format_name_no_spaces() {
        assert_eq!(super::format_name("MyPreset"), "MyPreset");
    }

    #[test]
    fn format_name_with_spaces() {
        assert_eq!(super::format_name("My Preset"), "\"My Preset\"");
    }

    #[test]
    fn format_string_token_empty() {
        assert_eq!(super::format_string_token(""), "\"\"");
    }

    #[test]
    fn format_string_token_with_spaces() {
        assert_eq!(super::format_string_token("my output"), "\"my output\"");
    }

    #[test]
    fn format_string_token_bare() {
        assert_eq!(super::format_string_token("$project"), "$project");
    }
}
