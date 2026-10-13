//! Convert CSI `.zon` zone files to daw-csi Styx zone files.
//!
//! ```sh
//! cargo run -p daw-csi --bin zon2styx -- <files-or-dirs…> > out.zones.styx
//! ```
//!
//! Maps the CSI action/widget vocabulary onto ours where an
//! equivalent exists; everything else is preserved as a comment so
//! nothing is silently dropped. The output compiles directly with
//! `ZoneSet::parse` (set `FTS_CSI_ZONES` to use it).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: zon2styx <files-or-dirs…>");
        std::process::exit(2);
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for a in &args {
        let p = Path::new(a);
        if p.is_dir() {
            collect_zon(p, &mut files);
        } else {
            files.push(p.to_path_buf());
        }
    }
    files.sort();

    let mut zones: BTreeMap<String, ZoneOut> = BTreeMap::new();
    for f in &files {
        let text = match std::fs::read_to_string(f) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("skip {}: {e}", f.display());
                continue;
            }
        };
        for zone in parse_zon(&text) {
            zones.insert(zone.key.clone(), zone);
        }
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "// Converted from CSI .zon files by zon2styx — daw-csi zone format.\n\
         // Unsupported CSI lines are preserved as comments.\n"
    );
    let _ = writeln!(out, "home home\n");
    let _ = writeln!(out, "zones {{");
    for zone in zones.values() {
        emit_zone(&mut out, zone);
    }
    let _ = writeln!(out, "}}");
    print!("{out}");
}

fn collect_zon(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_zon(&p, files);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zon")) {
            files.push(p);
        }
    }
}

// ── Parsed output model ─────────────────────────────────────────────

struct ZoneOut {
    key: String,
    original_name: String,
    includes: Vec<String>,
    navigator: Option<&'static str>,
    display_color: Option<String>,
    /// (key, action) — strip-context bindings (CSI `Widget|`).
    strip: Vec<(String, String)>,
    /// (key, action) — master-section bindings.
    buttons: Vec<(String, String)>,
    /// Original lines we couldn't map.
    unsupported: Vec<String>,
}

/// snake_case a CSI zone name: `SelectedTrackSend` →
/// `selected_track_send`, `VCA` → `vca`, `TrackFXMenu` →
/// `track_fxmenu` (no underscores inside acronym runs).
fn zone_key(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

fn parse_zon(text: &str) -> Vec<ZoneOut> {
    let mut zones = Vec::new();
    let mut cur: Option<ZoneOut> = None;
    let mut in_includes = false;

    for raw_line in text.lines() {
        // Strip comments; CSI also uses a leading `/` to disable lines.
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('/') {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();

        if tokens[0] == "Zone" {
            let name = tokens
                .get(1)
                .map(|s| s.trim_matches('"'))
                .unwrap_or("unnamed");
            cur = Some(ZoneOut {
                key: zone_key(name),
                original_name: name.to_string(),
                includes: Vec::new(),
                navigator: navigator_for_zone(name),
                display_color: None,
                strip: Vec::new(),
                buttons: Vec::new(),
                unsupported: Vec::new(),
            });
            continue;
        }
        if tokens[0] == "ZoneEnd" {
            if let Some(z) = cur.take() {
                zones.push(z);
            }
            continue;
        }
        let Some(zone) = cur.as_mut() else {
            continue;
        };
        if tokens[0] == "IncludedZones" {
            in_includes = true;
            continue;
        }
        if tokens[0] == "IncludedZonesEnd" {
            in_includes = false;
            continue;
        }
        if in_includes {
            zone.includes.push(zone_key(tokens[0]));
            continue;
        }
        if tokens[0] == "OnZoneActivation" {
            // SetXTouchDisplayColors <Color> → display_color.
            if tokens.get(1) == Some(&"SetXTouchDisplayColors")
                && let Some(color) = tokens.get(2)
            {
                zone.display_color = Some(color.to_string());
            } else {
                zone.unsupported.push(line.to_string());
            }
            continue;
        }
        if tokens[0] == "OnZoneDeactivation" || tokens[0] == "OnInitialization" {
            // Deactivation restore is implicit in our model.
            if !line.contains("RestoreXTouchDisplayColors") {
                zone.unsupported.push(line.to_string());
            }
            continue;
        }

        convert_binding(zone, &tokens, line);
    }
    zones
}

fn navigator_for_zone(name: &str) -> Option<&'static str> {
    match name {
        "Home" | "Track" => Some("@Track"),
        "Folder" => Some("@Folder"),
        "VCA" => Some("@Vca"),
        _ => None,
    }
}

/// One `Widget Action [params…]` line → a binding (or an
/// unsupported-comment).
fn convert_binding(zone: &mut ZoneOut, tokens: &[&str], original: &str) {
    let widget_spec = tokens[0];
    let per_strip = widget_spec.ends_with('|');
    let widget_spec = widget_spec.trim_end_matches('|');

    // Modifier chain: Shift+Option+Select. Hold maps; the rest of
    // CSI's qualifiers (Toggle, Global, Flip, Touch, FaderTouch,
    // Increase/Decrease, …) have no equivalent yet.
    let parts: Vec<&str> = widget_spec.split('+').collect();
    let (mod_parts, widget_name) = parts.split_at(parts.len() - 1);
    let mut prefix = String::new();
    for m in mod_parts {
        match *m {
            "Shift" => prefix.push_str("shift+"),
            "Option" => prefix.push_str("option+"),
            "Control" => prefix.push_str("control+"),
            "Alt" => prefix.push_str("alt+"),
            "Hold" => prefix.push_str("hold+"),
            _ => {
                zone.unsupported.push(original.to_string());
                return;
            }
        }
    }
    let widget_name = widget_name[0];

    // Skip modifier-declaration lines (`Shift Shift` etc.) — the
    // driver owns modifier keys.
    if matches!(widget_name, "Shift" | "Option" | "Control" | "Alt")
        && tokens.get(1) == Some(&widget_name)
    {
        return;
    }

    let Some(mut action) = map_action(&tokens[1..]) else {
        zone.unsupported.push(original.to_string());
        return;
    };
    // CSI binds the master fader to TrackVolume under a
    // MasterTrackNavigator; our master fader is its own action.
    if widget_name == "MasterFader" && action == "@TrackVolume" {
        action = "@MasterVolume".into();
    }

    if per_strip {
        let Some(w) = map_strip_widget(widget_name) else {
            zone.unsupported.push(original.to_string());
            return;
        };
        push_unique(
            &mut zone.strip,
            format!("{prefix}{w}"),
            action,
            zone.unsupported.len(),
            original,
            &mut zone.unsupported,
        );
    } else {
        let Some(w) = map_global_widget(widget_name) else {
            zone.unsupported.push(original.to_string());
            return;
        };
        push_unique(
            &mut zone.buttons,
            format!("{prefix}{w}"),
            action,
            zone.unsupported.len(),
            original,
            &mut zone.unsupported,
        );
    }
}

/// CSI allows duplicate widget lines (e.g. BankLeft bound for both
/// Track and SelectedTrack navigators); our map keys are unique —
/// keep the first, comment the rest.
fn push_unique(
    list: &mut Vec<(String, String)>,
    key: String,
    action: String,
    _n: usize,
    original: &str,
    unsupported: &mut Vec<String>,
) {
    if list.iter().any(|(k, _)| k == &key) {
        unsupported.push(format!("(duplicate) {original}"));
        return;
    }
    list.push((key, action));
}

fn map_strip_widget(name: &str) -> Option<&'static str> {
    Some(match name {
        "Fader" => "fader",
        "Rotary" => "vpot",
        "RotaryPush" => "vpot_press",
        "RecordArm" => "rec",
        "Solo" => "solo",
        "Mute" => "mute",
        "Select" => "select",
        "DisplayUpper" => "lcd_top",
        "DisplayLower" => "lcd_bottom",
        _ => return None,
    })
}

fn map_global_widget(name: &str) -> Option<&'static str> {
    Some(match name {
        "MasterFader" => "master_fader",
        "JogWheel" => "jog",
        "Play" => "play",
        "Stop" => "stop",
        "Record" => "record",
        "Rewind" => "rewind",
        "FastForward" => "fast_forward",
        "Cycle" => "cycle",
        "BankLeft" => "bank_left",
        "BankRight" => "bank_right",
        "ChannelLeft" => "channel_left",
        "ChannelRight" => "channel_right",
        "Flip" => "flip",
        "GlobalView" => "global_view",
        "Marker" => "marker",
        "Nudge" => "nudge",
        "Drop" => "drop",
        "Replace" => "replace",
        "Click" => "click",
        "Solo" => "solo_global",
        "Up" => "up",
        "Down" => "down",
        "Left" => "left",
        "Right" => "right",
        "Zoom" => "zoom",
        "Scrub" => "scrub",
        "Save" => "save",
        "Undo" => "undo",
        "Cancel" => "cancel",
        "Enter" => "enter",
        "nameValue" => "name_value",
        "smpteBeats" => "smpte_beats",
        "Track" => "assign_track",
        "Send" => "assign_send",
        "Pan" => "assign_pan",
        "Plugin" => "assign_plugin",
        "EQ" => "assign_eq",
        "Instrument" | "Inst" => "assign_inst",
        "F1" => "f1",
        "F2" => "f2",
        "F3" => "f3",
        "F4" => "f4",
        "F5" => "f5",
        "F6" => "f6",
        "F7" => "f7",
        "F8" => "f8",
        _ => return None,
    })
}

/// Map a CSI action + params onto our `@Action` syntax. `None` =
/// unsupported (caller comments the line out).
fn map_action(tokens: &[&str]) -> Option<String> {
    let name = *tokens.first()?;
    // `TrackPan [ 0.5 ]` — a literal param means "set to value":
    // only the pan/width reset idioms are supported.
    let has_value_param = tokens.contains(&"[");

    Some(match name {
        "TrackVolume" if !has_value_param => "@TrackVolume".into(),
        "TrackPan" if has_value_param => "@TrackPanReset".into(),
        "TrackPan" | "TrackPanAutoLeft" => "@TrackPan".into(),
        "TrackMute" => "@TrackMute".into(),
        "TrackSolo" => "@TrackSolo".into(),
        "TrackRecordArm" => "@TrackRecordArm".into(),
        "TrackUniqueSelect" => "@TrackSelect".into(),
        "TrackSelect" => "@TrackSelectAdditive".into(),
        "TrackToggleFolderSpill" => "@FolderSpill".into(),
        "TrackInvertPolarity" => "@TrackTogglePolarity".into(),
        "TrackRangeSelect" => "@TrackRangeSelect".into(),
        "TrackToggleVCASpill" => "@VcaSpill".into(),
        "ClearAllSolo" => "@ClearAllSolo".into(),
        "NoAction" => "@NoAction".into(),
        "TrackNameDisplay" => "@TrackName".into(),
        "TrackPanDisplay" | "TrackPanAutoLeftDisplay" => "@PanDisplay".into(),
        "TrackVolumeDisplay" => "@VolumeDisplay".into(),
        "TrackSendVolume" => "@SendVolume".into(),
        "TrackSendPan" => "@SendPan".into(),
        "TrackSendMute" => "@SendMute".into(),
        "TrackSendNameDisplay" => "@SendNameDisplay".into(),
        "TrackSendVolumeDisplay" => "@SendVolumeDisplay".into(),
        "TrackSendPanDisplay" => "@SendPanDisplay".into(),
        "TrackReceiveVolume" => "@ReceiveVolume".into(),
        "TrackReceivePan" => "@ReceivePan".into(),
        "TrackReceiveMute" => "@ReceiveMute".into(),
        "TrackReceiveNameDisplay" => "@ReceiveNameDisplay".into(),
        "TrackReceiveVolumeDisplay" => "@ReceiveVolumeDisplay".into(),
        "TrackReceivePanDisplay" => "@ReceivePanDisplay".into(),
        "Play" => "@Play".into(),
        "Stop" => "@Stop".into(),
        "Record" => "@Record".into(),
        "CycleTimeline" => "@ToggleLoop".into(),
        "Rewind" => "@NudgePosition{seconds -5}".into(),
        "FastForward" => "@NudgePosition{seconds 5}".into(),
        "MoveEditCursor" => "@JogPosition{seconds_per_tick 1}".into(),
        "GoHome" => "@GoZone{zone home}".into(),
        "SaveProject" => "@SaveProject".into(),
        "Undo" => "@Undo".into(),
        "Redo" => "@Redo".into(),
        "Marker" => "@AddMarker".into(),
        "CycleTimeDisplayModes" => "@CycleTimeDisplay".into(),
        "ToggleScrollLink" => "@ToggleScrollLink".into(),
        "Flip" => "@Flip".into(),
        // REAPER action passthrough → our command registry. Ids may
        // arrive quoted in the .zon ("_S&M_FXOFF|") — strip those.
        "Reaper" => {
            let id = tokens.get(1)?.trim_matches('"');
            format!("@Command{{id \"{id}\"}}")
        }
        "GoZone" => {
            let target = tokens.get(1)?;
            format!("@GoZone{{zone {}}}", zone_key(target))
        }
        "Bank" => {
            let what = tokens.get(1)?;
            let amount: i32 = tokens.get(2)?.parse().ok()?;
            match *what {
                "Track" | "Folder" | "VCA" => format!("@Bank{{amount {amount}}}"),
                "SelectedTrackSend" => format!("@BankSends{{amount {amount}}}"),
                "SelectedTrackReceive" => format!("@BankReceives{{amount {amount}}}"),
                _ => return None,
            }
        }
        _ => return None,
    })
}

fn emit_zone(out: &mut String, zone: &ZoneOut) {
    let _ = writeln!(
        out,
        "    /// Converted from CSI zone \"{}\".",
        zone.original_name
    );
    let _ = writeln!(out, "    {} {{", zone.key);
    if !zone.includes.is_empty() {
        let _ = writeln!(out, "        include ({})", zone.includes.join(" "));
    }
    if let Some(nav) = zone.navigator {
        let _ = writeln!(out, "        navigator {nav}");
    }
    if let Some(color) = &zone.display_color {
        let _ = writeln!(out, "        display_color @{color}");
    }
    if !zone.strip.is_empty() {
        let _ = writeln!(out, "        strip {{");
        for (k, v) in &zone.strip {
            let _ = writeln!(out, "            {k} {v}");
        }
        let _ = writeln!(out, "        }}");
    }
    if !zone.buttons.is_empty() {
        let _ = writeln!(out, "        buttons {{");
        for (k, v) in &zone.buttons {
            let _ = writeln!(out, "            {k} {v}");
        }
        let _ = writeln!(out, "        }}");
    }
    if !zone.unsupported.is_empty() {
        let _ = writeln!(out, "        // Not yet portable from CSI:");
        for u in &zone.unsupported {
            let _ = writeln!(out, "        // {u}");
        }
    }
    let _ = writeln!(out, "    }}");
}
