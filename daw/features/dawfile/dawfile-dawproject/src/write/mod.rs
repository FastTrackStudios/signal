//! Serialize DawProject types to DawProject XML format.
//!
//! Produces `project.xml` and `metadata.xml` content suitable for packing
//! into a `.dawproject` ZIP archive.

pub mod xml;

use crate::types::*;
use xml::XmlWriter;

/// Serialize a `DawProject` to `project.xml` content (UTF-8 string).
pub fn write_project_xml(project: &DawProject) -> String {
    let mut w = XmlWriter::new();
    w.open("Project", &[("version", &project.version)]);

    if let Some(app) = &project.application {
        w.empty(
            "Application",
            &[("name", &app.name), ("version", &app.version)],
        );
    }

    write_transport(&mut w, &project.transport);

    if !project.tracks.is_empty() {
        w.wrap("Structure", &[], |w| {
            for track in &project.tracks {
                write_track(w, track);
            }
        });
    }

    if let Some(arr) = &project.arrangement {
        write_arrangement(&mut w, arr);
    }

    if !project.scenes.is_empty() {
        w.wrap("Scenes", &[], |w| {
            for scene in &project.scenes {
                write_scene(w, scene);
            }
        });
    }

    w.close("Project");
    w.finish()
}

/// Serialize `ProjectMetadata` to `metadata.xml` content (UTF-8 string).
pub fn write_metadata_xml(meta: &ProjectMetadata) -> String {
    let mut w = XmlWriter::new();
    w.open("MetaData", &[]);
    write_opt_text(&mut w, "Title", meta.title.as_deref());
    write_opt_text(&mut w, "Artist", meta.artist.as_deref());
    write_opt_text(&mut w, "Album", meta.album.as_deref());
    write_opt_text(&mut w, "Composer", meta.composer.as_deref());
    write_opt_text(&mut w, "Songwriter", meta.songwriter.as_deref());
    write_opt_text(&mut w, "Producer", meta.producer.as_deref());
    write_opt_text(&mut w, "OriginalArtist", meta.original_artist.as_deref());
    write_opt_text(&mut w, "Arranger", meta.arranger.as_deref());
    write_opt_text(&mut w, "Year", meta.year.as_deref());
    write_opt_text(&mut w, "Genre", meta.genre.as_deref());
    write_opt_text(&mut w, "Copyright", meta.copyright.as_deref());
    write_opt_text(&mut w, "Website", meta.website.as_deref());
    write_opt_text(&mut w, "Comment", meta.comment.as_deref());
    w.close("MetaData");
    w.finish()
}

// ─── Transport ───────────────────────────────────────────────────────────────

fn write_transport(w: &mut XmlWriter, t: &Transport) {
    w.wrap("Transport", &[], |w| {
        let tempo_str = format!("{:.6}", t.tempo);
        w.empty(
            "Tempo",
            &[("value", &tempo_str), ("min", "60"), ("max", "200")],
        );
        let num = t.numerator.to_string();
        let den = t.denominator.to_string();
        w.empty(
            "TimeSignature",
            &[("numerator", &num), ("denominator", &den)],
        );
    });
}

// ─── Tracks ──────────────────────────────────────────────────────────────────

fn write_track(w: &mut XmlWriter, track: &Track) {
    let content_type_str: String = track
        .content_types
        .iter()
        .map(|ct| ct.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let mut attrs: Vec<(&str, &str)> = vec![
        ("contentType", &content_type_str),
        ("id", &track.id),
        ("name", &track.name),
    ];
    let color_s;
    if let Some(c) = &track.color {
        color_s = c.clone();
        attrs.push(("color", &color_s));
    }
    let comment_s;
    if let Some(c) = &track.comment {
        comment_s = c.clone();
        attrs.push(("comment", &comment_s));
    }
    if !track.loaded {
        attrs.push(("loaded", "false"));
    }

    w.open("Track", &attrs);
    if let Some(ch) = &track.channel {
        write_channel(w, ch);
    }
    for child in &track.children {
        write_track(w, child);
    }
    w.close("Track");
}

fn write_channel(w: &mut XmlWriter, ch: &Channel) {
    let audio_ch = ch.audio_channels.to_string();
    let mut attrs: Vec<(&str, &str)> = vec![
        ("role", ch.role.as_str()),
        ("audioChannels", &audio_ch),
        ("id", &ch.id),
    ];
    let dest_s;
    if let Some(d) = &ch.destination {
        dest_s = d.clone();
        attrs.push(("destination", &dest_s));
    }
    let blend_s;
    if let Some(b) = &ch.blend_mode {
        blend_s = b.clone();
        attrs.push(("blendMode", &blend_s));
    }
    if ch.solo {
        attrs.push(("solo", "true"));
    }

    w.open("Channel", &attrs);

    let vol = format!("{:.6}", ch.volume);
    w.empty("Volume", &[("value", &vol)]);
    let pan = format!("{:.6}", ch.pan);
    w.empty("Pan", &[("value", &pan)]);
    let mute = if ch.muted { "true" } else { "false" };
    w.empty("Mute", &[("value", mute)]);

    if !ch.sends.is_empty() {
        w.wrap("Sends", &[], |w| {
            for send in &ch.sends {
                write_send(w, send);
            }
        });
    }

    if !ch.devices.is_empty() {
        w.wrap("Devices", &[], |w| {
            for device in &ch.devices {
                write_device(w, device);
            }
        });
    }

    w.close("Channel");
}

fn write_send(w: &mut XmlWriter, send: &Send) {
    let vol_str = format!("{:.6}", send.volume);
    let send_type = if send.pre_fader { "pre" } else { "post" };
    w.open(
        "Send",
        &[("destination", &send.destination), ("type", send_type)],
    );
    w.empty("Volume", &[("value", &vol_str)]);
    let pan_str = format!("{:.6}", send.pan);
    w.empty("Pan", &[("value", &pan_str)]);
    if !send.enabled {
        w.empty("Enable", &[("value", "false")]);
    }
    w.close("Send");
}

// ─── Devices ─────────────────────────────────────────────────────────────────

fn write_device(w: &mut XmlWriter, device: &Device) {
    let tag = match device.format {
        DeviceFormat::Vst2 => "Vst2Plugin",
        DeviceFormat::Vst3 => "Vst3Plugin",
        DeviceFormat::Clap => "ClapPlugin",
        DeviceFormat::Au => "AuPlugin",
        DeviceFormat::Equalizer => "Equalizer",
        DeviceFormat::Compressor => "Compressor",
        DeviceFormat::Limiter => "Limiter",
        DeviceFormat::NoiseGate => "NoiseGate",
        DeviceFormat::Builtin | DeviceFormat::Unknown => "BuiltinDevice",
    };

    let bypass = if device.enabled { "false" } else { "true" };
    let mut attrs: Vec<(&str, &str)> = vec![("deviceName", &device.name), ("bypass", bypass)];
    let role_s;
    if let Some(role) = &device.device_role {
        role_s = role.as_str();
        attrs.push(("deviceRole", role_s));
    }
    let plugin_id_s;
    if let Some(pid) = &device.plugin_id {
        plugin_id_s = pid.clone();
        attrs.push(("pluginId", &plugin_id_s));
    }
    let vendor_s;
    if let Some(v) = &device.vendor {
        vendor_s = v.clone();
        attrs.push(("deviceVendor", &vendor_s));
    }
    let version_s;
    if let Some(ver) = &device.plugin_version {
        version_s = ver.clone();
        attrs.push(("pluginVersion", &version_s));
    }
    let device_id_s;
    if let Some(did) = &device.device_id {
        device_id_s = did.clone();
        attrs.push(("deviceID", &device_id_s));
    }
    let path_s;
    if let Some(path) = &device.plugin_path {
        path_s = path.to_string_lossy().into_owned();
        attrs.push(("deviceFile", &path_s));
    }
    if !device.loaded {
        attrs.push(("loaded", "false"));
    }

    let has_builtin = !matches!(device.builtin_content, BuiltinDeviceContent::None);
    let has_body = !device.parameters.is_empty() || device.state.is_some() || has_builtin;
    if has_body {
        w.open(tag, &attrs);
        write_builtin_content(w, &device.builtin_content);
        if !device.parameters.is_empty() {
            w.wrap("Parameters", &[], |w| {
                for param in &device.parameters {
                    write_device_parameter(w, param);
                }
            });
        }
        write_device_state(w, &device.state);
        w.close(tag);
    } else {
        w.empty(tag, &attrs);
    }
}

fn write_builtin_content(w: &mut XmlWriter, content: &BuiltinDeviceContent) {
    match content {
        BuiltinDeviceContent::None => {}
        BuiltinDeviceContent::Equalizer(bands) => {
            for band in bands {
                let mut attrs: Vec<(&str, &str)> =
                    vec![("id", &band.id), ("type", band.band_type.as_str())];
                let order_s;
                if let Some(o) = band.order {
                    order_s = o.to_string();
                    attrs.push(("order", &order_s));
                }
                let enabled_str = if band.enabled { "true" } else { "false" };
                w.open("Band", &attrs);
                write_param_real(w, "Freq", band.freq);
                write_param_real(w, "Gain", band.gain);
                write_param_real(w, "Q", band.q);
                w.empty("Enabled", &[("value", enabled_str)]);
                w.close("Band");
            }
        }
        BuiltinDeviceContent::Compressor(p) => {
            write_param_real(w, "Threshold", p.threshold);
            write_param_real(w, "Ratio", p.ratio);
            write_param_real(w, "Attack", p.attack);
            write_param_real(w, "Release", p.release);
            write_param_real(w, "InputGain", p.input_gain);
            write_param_real(w, "OutputGain", p.output_gain);
            if let Some(auto) = p.auto_makeup {
                w.empty(
                    "AutoMakeup",
                    &[("value", if auto { "true" } else { "false" })],
                );
            }
        }
        BuiltinDeviceContent::Limiter(p) => {
            write_param_real(w, "Threshold", p.threshold);
            write_param_real(w, "Attack", p.attack);
            write_param_real(w, "Release", p.release);
            write_param_real(w, "InputGain", p.input_gain);
            write_param_real(w, "OutputGain", p.output_gain);
        }
        BuiltinDeviceContent::NoiseGate(p) => {
            write_param_real(w, "Threshold", p.threshold);
            write_param_real(w, "Range", p.range);
            write_param_real(w, "Ratio", p.ratio);
            write_param_real(w, "Attack", p.attack);
            write_param_real(w, "Release", p.release);
        }
    }
}

fn write_param_real(w: &mut XmlWriter, name: &str, value: Option<f64>) {
    if let Some(v) = value {
        let s = format!("{v:.6}");
        w.empty(name, &[("value", &s)]);
    }
}

fn write_device_parameter(w: &mut XmlWriter, param: &DeviceParameter) {
    let name_s;
    let mut base: Vec<(&str, &str)> = vec![("parameterID", &param.id)];
    if let Some(name) = &param.name {
        name_s = name.clone();
        base.push(("name", &name_s));
    }

    match &param.value {
        DeviceParameterValue::Real {
            value,
            min,
            max,
            unit,
        } => {
            let v = format!("{value:.6}");
            let mut attrs = base;
            attrs.push(("value", &v));
            let min_s;
            if let Some(m) = min {
                min_s = format!("{m:.6}");
                attrs.push(("min", &min_s));
            }
            let max_s;
            if let Some(m) = max {
                max_s = format!("{m:.6}");
                attrs.push(("max", &max_s));
            }
            if let Some(u) = unit {
                attrs.push(("unit", u.as_str()));
            }
            w.empty("RealParameter", &attrs);
        }
        DeviceParameterValue::Bool(v) => {
            let val = if *v { "true" } else { "false" };
            let mut attrs = base;
            attrs.push(("value", val));
            w.empty("BoolParameter", &attrs);
        }
        DeviceParameterValue::Integer { value, min, max } => {
            let v = value.to_string();
            let mut attrs = base;
            attrs.push(("value", &v));
            let min_s;
            if let Some(m) = min {
                min_s = m.to_string();
                attrs.push(("min", &min_s));
            }
            let max_s;
            if let Some(m) = max {
                max_s = m.to_string();
                attrs.push(("max", &max_s));
            }
            w.empty("IntegerParameter", &attrs);
        }
        DeviceParameterValue::Enum {
            value,
            count,
            labels,
        } => {
            let v = value.to_string();
            let c = count.to_string();
            let mut attrs = base;
            attrs.push(("value", &v));
            attrs.push(("count", &c));
            if labels.is_empty() {
                w.empty("EnumParameter", &attrs);
            } else {
                w.open("EnumParameter", &attrs);
                for label in labels {
                    w.text_elem("label", &[], label);
                }
                w.close("EnumParameter");
            }
        }
        DeviceParameterValue::TimeSignature {
            numerator,
            denominator,
        } => {
            let num = numerator.to_string();
            let den = denominator.to_string();
            let mut attrs = base;
            attrs.push(("numerator", &num));
            attrs.push(("denominator", &den));
            w.empty("TimeSignatureParameter", &attrs);
        }
    }
}

fn write_device_state(w: &mut XmlWriter, state: &Option<DeviceState>) {
    match state {
        Some(DeviceState::File(file)) => w.empty("State", &[("file", file)]),
        Some(DeviceState::Base64(data)) => w.text_elem("State", &[], data),
        None => {}
    }
}

// ─── Arrangement ─────────────────────────────────────────────────────────────

fn write_arrangement(w: &mut XmlWriter, arr: &Arrangement) {
    let mut attrs: Vec<(&str, &str)> = vec![("id", &arr.id)];
    let name_s;
    if let Some(n) = &arr.name {
        name_s = n.clone();
        attrs.push(("name", &name_s));
    }
    let color_s;
    if let Some(c) = &arr.color {
        color_s = c.clone();
        attrs.push(("color", &color_s));
    }
    let comment_s;
    if let Some(c) = &arr.comment {
        comment_s = c.clone();
        attrs.push(("comment", &comment_s));
    }
    w.open("Arrangement", &attrs);

    if !arr.lanes.is_empty() {
        w.open("Lanes", &[("timeUnit", arr.time_unit.as_str())]);
        for lane in &arr.lanes {
            write_lane(w, lane);
        }
        w.close("Lanes");
    }

    if !arr.markers.is_empty() {
        w.open("Markers", &[]);
        for marker in &arr.markers {
            write_marker(w, marker);
        }
        w.close("Markers");
    }

    if !arr.tempo_automation.is_empty() {
        write_tempo_automation(w, &arr.tempo_automation);
    }
    if !arr.time_sig_automation.is_empty() {
        write_time_sig_automation(w, &arr.time_sig_automation);
    }

    w.close("Arrangement");
}

fn write_tempo_automation(w: &mut XmlWriter, points: &[TempoPoint]) {
    w.open("TempoAutomation", &[]);
    w.open("Points", &[("unit", "bpm")]);
    w.empty("Target", &[]);
    for pt in points {
        let time = format!("{:.6}", pt.time);
        let bpm = format!("{:.6}", pt.bpm);
        w.empty(
            "RealPoint",
            &[
                ("time", &time),
                ("value", &bpm),
                ("interpolation", pt.interpolation.as_str()),
            ],
        );
    }
    w.close("Points");
    w.close("TempoAutomation");
}

fn write_time_sig_automation(w: &mut XmlWriter, points: &[TimeSignaturePoint]) {
    w.open("TimeSignatureAutomation", &[]);
    w.open("Points", &[]);
    w.empty("Target", &[]);
    for pt in points {
        let time = format!("{:.6}", pt.time);
        let num = pt.numerator.to_string();
        let den = pt.denominator.to_string();
        w.empty(
            "TimeSignaturePoint",
            &[("time", &time), ("numerator", &num), ("denominator", &den)],
        );
    }
    w.close("Points");
    w.close("TimeSignatureAutomation");
}

fn write_lane(w: &mut XmlWriter, lane: &Lane) {
    match &lane.content {
        LaneContent::Clips(clips) => {
            let mut attrs: Vec<(&str, &str)> = vec![("track", &lane.track), ("id", &lane.id)];
            let unit_s;
            if let Some(unit) = lane.time_unit {
                unit_s = unit.as_str();
                attrs.push(("timeUnit", unit_s));
            }
            w.open("Clips", &attrs);
            for clip in clips {
                write_clip(w, clip);
            }
            w.close("Clips");
        }
        LaneContent::Notes(notes) => {
            w.open("Notes", &[("track", &lane.track), ("id", &lane.id)]);
            for note in notes {
                write_note(w, note);
            }
            w.close("Notes");
        }
        LaneContent::Automation(auto_pts) => {
            write_automation_points(w, auto_pts);
        }
        LaneContent::Markers(markers) => {
            w.open("Markers", &[("track", &lane.track), ("id", &lane.id)]);
            for marker in markers {
                write_marker(w, marker);
            }
            w.close("Markers");
        }
    }
}

fn write_automation_points(w: &mut XmlWriter, auto_pts: &AutomationPoints) {
    let mut attrs: Vec<(&str, &str)> = vec![("id", &auto_pts.id)];
    if let Some(unit) = auto_pts.unit {
        attrs.push(("unit", unit.as_str()));
    }
    w.open("Points", &attrs);

    // Target element
    write_automation_target(w, &auto_pts.target);

    for pt in &auto_pts.points {
        let time = format!("{:.6}", pt.time);
        let value = format!("{:.6}", pt.value);
        w.empty(
            "RealPoint",
            &[
                ("time", &time),
                ("value", &value),
                ("interpolation", pt.interpolation.as_str()),
            ],
        );
    }
    w.close("Points");
}

fn write_automation_target(w: &mut XmlWriter, target: &AutomationTarget) {
    let mut attrs: Vec<(&str, &str)> = Vec::new();
    let param_s;
    if let Some(p) = &target.parameter {
        param_s = p.clone();
        attrs.push(("parameter", &param_s));
    }
    if let Some(expr) = target.expression {
        attrs.push(("expression", expr.as_str()));
    }
    let ch_s;
    if let Some(ch) = target.channel {
        ch_s = ch.to_string();
        attrs.push(("channel", &ch_s));
    }
    let key_s;
    if let Some(key) = target.key {
        key_s = key.to_string();
        attrs.push(("key", &key_s));
    }
    let ctrl_s;
    if let Some(ctrl) = target.controller {
        ctrl_s = ctrl.to_string();
        attrs.push(("controller", &ctrl_s));
    }
    w.empty("Target", &attrs);
}

// ─── Clips ───────────────────────────────────────────────────────────────────

fn write_clip(w: &mut XmlWriter, clip: &Clip) {
    let time = format!("{:.6}", clip.time);
    let dur = format!("{:.6}", clip.duration);
    let mut attrs: Vec<(&str, &str)> = vec![("time", &time), ("duration", &dur), ("id", &clip.id)];

    let unit_s;
    if let Some(unit) = clip.time_unit {
        unit_s = unit.as_str();
        attrs.push(("timeUnit", unit_s));
    }
    let cunit_s;
    if let Some(unit) = clip.content_time_unit {
        cunit_s = unit.as_str();
        attrs.push(("contentTimeUnit", cunit_s));
    }
    let name_s;
    if let Some(n) = &clip.name {
        name_s = n.clone();
        attrs.push(("name", &name_s));
    }
    let color_s;
    if let Some(c) = &clip.color {
        color_s = c.clone();
        attrs.push(("color", &color_s));
    }
    let comment_s;
    if let Some(c) = &clip.comment {
        comment_s = c.clone();
        attrs.push(("comment", &comment_s));
    }
    if !clip.enabled {
        attrs.push(("enable", "false"));
    }
    let ps_s;
    if let Some(ps) = clip.play_start {
        ps_s = format!("{ps:.6}");
        attrs.push(("playStart", &ps_s));
    }
    let pe_s;
    if let Some(pe) = clip.play_stop {
        pe_s = format!("{pe:.6}");
        attrs.push(("playStop", &pe_s));
    }
    let ref_s;
    if let Some(r) = &clip.reference {
        ref_s = r.clone();
        attrs.push(("reference", &ref_s));
    }
    let fi_s;
    if let Some(fi) = clip.fade_in_time {
        fi_s = format!("{fi:.6}");
        attrs.push(("fadeInTime", &fi_s));
    }
    let fo_s;
    if let Some(fo) = clip.fade_out_time {
        fo_s = format!("{fo:.6}");
        attrs.push(("fadeOutTime", &fo_s));
    }
    let ftu_s;
    if let Some(ftu) = clip.fade_time_unit {
        ftu_s = ftu.as_str();
        attrs.push(("fadeTimeUnit", ftu_s));
    }
    w.open("Clip", &attrs);

    if let Some(ls) = &clip.loop_settings {
        let ls_s = format!("{:.6}", ls.loop_start);
        let le_s = format!("{:.6}", ls.loop_end);
        let ps_s2 = format!("{:.6}", ls.play_start);
        w.empty(
            "Loops",
            &[
                ("loopStart", &ls_s),
                ("loopEnd", &le_s),
                ("playStart", &ps_s2),
            ],
        );
    }

    match &clip.content {
        ClipContent::Audio(audio) => write_audio(w, audio),
        ClipContent::Video(video) => write_video(w, video),
        ClipContent::Notes(notes) => {
            w.open("Notes", &[]);
            for note in notes {
                write_note(w, note);
            }
            w.close("Notes");
        }
        ClipContent::Empty => {}
    }

    w.close("Clip");
}

fn write_audio(w: &mut XmlWriter, audio: &AudioContent) {
    let mut attrs: Vec<(&str, &str)> = Vec::new();
    let sr_s;
    if let Some(sr) = audio.sample_rate {
        sr_s = sr.to_string();
        attrs.push(("sampleRate", &sr_s));
    }
    let ch_s;
    if let Some(ch) = audio.channels {
        ch_s = ch.to_string();
        attrs.push(("channels", &ch_s));
    }
    let dur_s;
    if let Some(d) = audio.duration {
        dur_s = format!("{d:.6}");
        attrs.push(("duration", &dur_s));
    }
    let algo_s;
    if let Some(a) = &audio.algorithm {
        algo_s = a.clone();
        attrs.push(("algorithm", &algo_s));
    }
    write_media_element(w, "Audio", &attrs, audio.file.as_ref(), &audio.warps);
}

fn write_video(w: &mut XmlWriter, video: &VideoContent) {
    let mut attrs: Vec<(&str, &str)> = Vec::new();
    let sr_s;
    if let Some(sr) = video.sample_rate {
        sr_s = sr.to_string();
        attrs.push(("sampleRate", &sr_s));
    }
    let ch_s;
    if let Some(ch) = video.channels {
        ch_s = ch.to_string();
        attrs.push(("channels", &ch_s));
    }
    let dur_s;
    if let Some(d) = video.duration {
        dur_s = format!("{d:.6}");
        attrs.push(("duration", &dur_s));
    }
    let algo_s;
    if let Some(a) = &video.algorithm {
        algo_s = a.clone();
        attrs.push(("algorithm", &algo_s));
    }
    write_media_element(w, "Video", &attrs, video.file.as_ref(), &video.warps);
}

fn write_media_element(
    w: &mut XmlWriter,
    tag: &str,
    attrs: &[(&str, &str)],
    file: Option<&FileReference>,
    warps: &Warps,
) {
    let needs_body = file.is_some() || !warps.warps.is_empty() || warps.content_time_unit.is_some();
    if !needs_body {
        w.empty(tag, attrs);
    } else {
        w.open(tag, attrs);
        if let Some(file_ref) = file {
            let external = if file_ref.external { "true" } else { "false" };
            w.empty("File", &[("path", &file_ref.path), ("external", external)]);
        }
        let needs_warps_elem = !warps.warps.is_empty() || warps.content_time_unit.is_some();
        if needs_warps_elem {
            let mut warps_attrs: Vec<(&str, &str)> = Vec::new();
            let unit_s;
            if let Some(unit) = warps.content_time_unit {
                unit_s = unit.as_str();
                warps_attrs.push(("contentTimeUnit", unit_s));
            }
            if warps.warps.is_empty() {
                w.empty("Warps", &warps_attrs);
            } else {
                w.open("Warps", &warps_attrs);
                for warp in &warps.warps {
                    let t = format!("{:.6}", warp.time);
                    let ct = format!("{:.6}", warp.content_time);
                    w.empty("Warp", &[("time", &t), ("contentTime", &ct)]);
                }
                w.close("Warps");
            }
        }
        w.close(tag);
    }
}

fn write_note(w: &mut XmlWriter, note: &Note) {
    let time = format!("{:.6}", note.time);
    let dur = format!("{:.6}", note.duration);
    let ch = note.channel.to_string();
    let key = note.key.to_string();
    let vel = format!("{:.6}", note.velocity);
    let mut attrs: Vec<(&str, &str)> = vec![
        ("time", &time),
        ("duration", &dur),
        ("channel", &ch),
        ("key", &key),
        ("vel", &vel),
    ];
    let rel_s;
    if let Some(rel) = note.release_velocity {
        rel_s = format!("{rel:.6}");
        attrs.push(("rel", &rel_s));
    }
    w.empty("Note", &attrs);
}

fn write_marker(w: &mut XmlWriter, marker: &Marker) {
    let time = format!("{:.6}", marker.time);
    let mut attrs: Vec<(&str, &str)> = vec![("time", &time), ("name", &marker.name)];
    let color_s;
    if let Some(c) = &marker.color {
        color_s = c.clone();
        attrs.push(("color", &color_s));
    }
    let comment_s;
    if let Some(c) = &marker.comment {
        comment_s = c.clone();
        attrs.push(("comment", &comment_s));
    }
    w.empty("Marker", &attrs);
}

// ─── Scenes ──────────────────────────────────────────────────────────────────

fn write_scene(w: &mut XmlWriter, scene: &Scene) {
    let mut attrs: Vec<(&str, &str)> = vec![("id", &scene.id)];
    let name_s;
    if let Some(n) = &scene.name {
        name_s = n.clone();
        attrs.push(("name", &name_s));
    }
    let color_s;
    if let Some(c) = &scene.color {
        color_s = c.clone();
        attrs.push(("color", &color_s));
    }
    let comment_s;
    if let Some(c) = &scene.comment {
        comment_s = c.clone();
        attrs.push(("comment", &comment_s));
    }
    w.open("Scene", &attrs);
    if let Some(content) = &scene.content {
        write_scene_content(w, content);
    }
    w.close("Scene");
}

fn write_scene_content(w: &mut XmlWriter, content: &SceneContent) {
    match content {
        SceneContent::Lanes(lanes) => {
            w.open("Lanes", &[]);
            for lane in lanes {
                write_lane(w, lane);
            }
            w.close("Lanes");
        }
        SceneContent::Slot(slot) => write_clip_slot(w, slot),
        SceneContent::Clips(clips) => {
            w.open("Clips", &[]);
            for clip in clips {
                write_clip(w, clip);
            }
            w.close("Clips");
        }
        SceneContent::Notes(notes) => {
            w.open("Notes", &[]);
            for note in notes {
                write_note(w, note);
            }
            w.close("Notes");
        }
        SceneContent::Markers(markers) => {
            w.open("Markers", &[]);
            for marker in markers {
                write_marker(w, marker);
            }
            w.close("Markers");
        }
    }
}

fn write_clip_slot(w: &mut XmlWriter, slot: &ClipSlot) {
    let has_stop = if slot.has_stop { "true" } else { "false" };
    let mut slot_attrs: Vec<(&str, &str)> = vec![("id", &slot.id), ("hasStop", has_stop)];
    let time_s;
    if let Some(t) = slot.time {
        time_s = format!("{t:.6}");
        slot_attrs.push(("time", &time_s));
    }
    let dur_s;
    if let Some(d) = slot.duration {
        dur_s = format!("{d:.6}");
        slot_attrs.push(("duration", &dur_s));
    }
    w.open("ClipSlot", &slot_attrs);
    if let Some(clip) = &slot.clip {
        write_clip(w, clip);
    }
    w.close("ClipSlot");
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_opt_text(w: &mut XmlWriter, tag: &str, value: Option<&str>) {
    if let Some(text) = value {
        w.text_elem(tag, &[], text);
    }
}
