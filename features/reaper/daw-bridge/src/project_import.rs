//! Project file import support for REAPER.
//!
//! Registers a `projectimport` handler so REAPER's File > Open dialog
//! can open `.als` (Ableton Live Set) files. The flow is:
//!
//! 1. REAPER calls `want_project_file()` to check if we handle the extension
//! 2. REAPER calls `enum_file_extensions()` to populate the file dialog filter
//! 3. REAPER calls `load_project()` with a `ProjectStateContext` to feed RPP lines into

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

use dawfile_ableton::devices::BuiltinParams;
use dawfile_ableton::{Device, DeviceFormat};
use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::{ItemBuilder, ReaperProjectBuilder, TrackBuilder};

// ============================================================================
// Extension registry
// ============================================================================

/// File extensions we handle, with human-readable descriptions for the file dialog.
static EXTENSIONS: &[(&str, &str)] = &[
    ("als", "Ableton Live Set (*.als)"),
    ("ptx", "Pro Tools Session (*.ptx)"),
    ("ptf", "Pro Tools Session (*.ptf)"),
    ("pts", "Pro Tools Session (*.pts)"),
    ("aaf", "Advanced Authoring Format (*.aaf)"),
    ("dawproject", "DAWproject (*.dawproject)"),
];

/// Pre-computed CString pairs (extension, description), leaked for stable pointers.
static CACHED_EXTENSIONS: OnceLock<Vec<(CString, CString)>> = OnceLock::new();

fn cached_extensions() -> &'static Vec<(CString, CString)> {
    CACHED_EXTENSIONS.get_or_init(|| {
        EXTENSIONS
            .iter()
            .map(|(ext, desc)| (CString::new(*ext).unwrap(), CString::new(*desc).unwrap()))
            .collect()
    })
}

// ============================================================================
// C callbacks (registered with REAPER via projectimport)
// ============================================================================

/// Called by REAPER to check if we handle this file.
///
/// # Safety
///
/// `fn_` must be a valid null-terminated C string pointer from REAPER.
pub unsafe extern "C" fn want_project_file(fn_: *const c_char) -> bool {
    let path = unsafe { CStr::from_ptr(fn_) }.to_string_lossy();
    let lower = path.to_lowercase();
    EXTENSIONS
        .iter()
        .any(|(ext, _)| lower.ends_with(&format!(".{ext}")))
}

/// Called by REAPER to enumerate supported extensions for the file dialog.
///
/// REAPER calls with increasing `i` until we return null.
/// `descptr` is an output parameter for the description string.
///
/// # Safety
///
/// `descptr` must be a valid writable pointer (or null) from REAPER.
pub unsafe extern "C" fn enum_file_extensions(
    i: c_int,
    descptr: *mut *mut c_char,
) -> *const c_char {
    let cached = cached_extensions();

    let idx = i as usize;
    if idx >= cached.len() {
        return std::ptr::null();
    }

    if !descptr.is_null() {
        unsafe {
            *descptr = cached[idx].1.as_ptr() as *mut c_char;
        }
    }
    cached[idx].0.as_ptr()
}

/// Called by REAPER to load/import the project file.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
///
/// - `fn_` must be a valid null-terminated C string pointer.
/// - `genstate` must be a valid `ProjectStateContext` pointer from REAPER.
pub unsafe extern "C" fn load_project(
    fn_: *const c_char,
    genstate: *mut reaper_low::raw::ProjectStateContext,
) -> c_int {
    let path = unsafe { CStr::from_ptr(fn_) }.to_string_lossy();

    match import_file(&path, genstate) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!("Failed to import {}: {e}", path);
            -1
        }
    }
}

// ============================================================================
// Import dispatcher
// ============================================================================

fn import_file(
    path: &str,
    genstate: *mut reaper_low::raw::ProjectStateContext,
) -> Result<(), Box<dyn std::error::Error>> {
    let lower = path.to_lowercase();

    let rpp_text = if lower.ends_with(".als") {
        import_ableton(path)?
    } else if lower.ends_with(".ptx") || lower.ends_with(".ptf") || lower.ends_with(".pts") {
        import_protools(path)?
    } else if lower.ends_with(".aaf") {
        import_aaf(path)?
    } else if lower.ends_with(".dawproject") {
        import_dawproject(path)?
    } else {
        return Err(format!("Unsupported format: {path}").into());
    };

    // Feed RPP text to REAPER line by line
    emit_rpp_to_context(genstate, &rpp_text)?;
    Ok(())
}

// ============================================================================
// RPP → ProjectStateContext emitter
// ============================================================================

fn emit_rpp_to_context(
    genstate: *mut reaper_low::raw::ProjectStateContext,
    rpp_text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for line in rpp_text.lines() {
        let c_line = CString::new(line)?;
        unsafe {
            let ctx = &mut *genstate;
            ctx.AddLine(c_line.as_ptr());
        }
    }
    Ok(())
}

// ============================================================================
// Ableton → REAPER conversion
// ============================================================================

fn import_ableton(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let set = dawfile_ableton::read_live_set(path)?;

    let mut builder = ReaperProjectBuilder::new();
    builder = builder.tempo_with_time_sig(
        set.tempo,
        set.time_signature.numerator as i32,
        set.time_signature.denominator as i32,
    );

    // Tempo automation
    if !set.tempo_automation.is_empty() {
        builder = builder.tempo_envelope(|env| {
            let mut env = env;
            for point in &set.tempo_automation {
                env = env.point(point.time, point.value);
            }
            env
        });
    }

    // Markers (locators)
    for (i, loc) in set.locators.iter().enumerate() {
        builder = builder.marker(i as i32 + 1, loc.time, &loc.name);
    }

    // Audio tracks
    for track in &set.audio_tracks {
        builder = builder.track(&track.common.effective_name, |t| {
            build_audio_track(t, track)
        });
    }

    // MIDI tracks
    for track in &set.midi_tracks {
        builder = builder.track(&track.common.effective_name, |t| build_midi_track(t, track));
    }

    // Return tracks → REAPER tracks
    for track in &set.return_tracks {
        let name = format!("[Return] {}", track.common.effective_name);
        builder = builder.track(&name, |t| apply_common_track_props(t, &track.common));
    }

    // Group tracks → REAPER folder tracks
    for track in &set.group_tracks {
        let name = format!("[Group] {}", track.common.effective_name);
        builder = builder.track(&name, |t| {
            apply_common_track_props(t, &track.common).folder_start()
        });
    }

    let project = builder.build();
    Ok(project.to_rpp_string())
}

// ── Track builders ─────────────────────────────────────────────────────────

fn apply_common_track_props(
    t: TrackBuilder,
    common: &dawfile_ableton::TrackCommon,
) -> TrackBuilder {
    let mut t = t.volume(common.mixer.volume).pan(common.mixer.pan);

    if common.color != 0 {
        t = t.color(ableton_color_to_rgb(common.color));
    }

    if !common.mixer.speaker_on {
        t = t.muted();
    }

    t
}

fn build_audio_track(t: TrackBuilder, track: &dawfile_ableton::AudioTrack) -> TrackBuilder {
    let mut t = apply_common_track_props(t, &track.common);

    // Arrangement clips → items
    for clip in &track.arrangement_clips {
        let position = clip.common.time;
        let length = clip.common.current_end - clip.common.current_start;

        if length <= 0.0 {
            continue;
        }

        t = t.item(position, length, |item| build_audio_item(item, clip));
    }

    // FX / Devices
    for device in &track.common.devices {
        t = add_device_to_track(t, device);
    }

    t
}

fn build_audio_item(item: ItemBuilder, clip: &dawfile_ableton::AudioClip) -> ItemBuilder {
    let mut item = item.name(&clip.common.name);

    // Audio source
    if let Some(ref sr) = clip.sample_ref
        && let Some(ref path) = sr.path
    {
        item = item.source_wave(path.to_string_lossy().into_owned());
    }

    // Pitch
    if clip.pitch_coarse != 0.0 {
        item = item.pitch(clip.pitch_coarse);
    }

    // Fades
    if let Some(ref fades) = clip.fades {
        if fades.fade_in_length > 0.0 {
            item = item.fade_in(
                fades.fade_in_length,
                dawfile_reaper::types::item::FadeCurveType::Linear,
            );
        }
        if fades.fade_out_length > 0.0 {
            item = item.fade_out(
                fades.fade_out_length,
                dawfile_reaper::types::item::FadeCurveType::Linear,
            );
        }
    }

    item
}

fn build_midi_track(t: TrackBuilder, track: &dawfile_ableton::MidiTrack) -> TrackBuilder {
    let mut t = apply_common_track_props(t, &track.common);

    // Arrangement MIDI clips
    for clip in &track.arrangement_clips {
        let position = clip.common.time;
        let length = clip.common.current_end - clip.common.current_start;

        if length <= 0.0 {
            continue;
        }

        t = t.item(position, length, |item| build_midi_item(item, clip));
    }

    // FX / Devices
    for device in &track.common.devices {
        t = add_device_to_track(t, device);
    }

    t
}

fn build_midi_item(item: ItemBuilder, clip: &dawfile_ableton::MidiClip) -> ItemBuilder {
    let item = item.name(&clip.common.name);

    // Build MIDI source from key tracks
    item.source_midi().midi(|midi| {
        let mut midi = midi;
        for kt in &clip.key_tracks {
            for note in &kt.notes {
                if !note.is_enabled {
                    continue;
                }
                // Convert beat-relative time to ticks (960 tpqn default)
                let tick_pos = (note.time * 960.0) as u64;
                let tick_dur = (note.duration * 960.0) as u64;
                midi = midi
                    .at(tick_pos)
                    .note(0, 0, kt.midi_key, note.velocity, tick_dur as u32);
            }
        }
        midi
    })
}

// ── Device / FX mapping ────────────────────────────────────────────────────

fn add_device_to_track(mut t: TrackBuilder, device: &Device) -> TrackBuilder {
    if !device.is_on {
        return t; // Skip disabled devices
    }

    match device.format {
        DeviceFormat::Vst2 => {
            t = t.vst(&device.name, "");
        }
        DeviceFormat::Vst3 => {
            t = t.vst3(&device.name, "");
        }
        DeviceFormat::AudioUnit => {
            // REAPER on Linux doesn't support AU, skip
        }
        DeviceFormat::Builtin => {
            if let Some(ref params) = device.builtin_params {
                t = map_builtin_fx(t, params);
            }
        }
        _ => {} // MaxForLive, NoteAlgorithm, Unknown — skip
    }

    t
}

fn map_builtin_fx(mut t: TrackBuilder, params: &BuiltinParams) -> TrackBuilder {
    match params {
        BuiltinParams::Eq8(_) => {
            t = t.js("ReaEQ");
        }
        BuiltinParams::Compressor(_) | BuiltinParams::GlueCompressor(_) => {
            t = t.js("ReaComp");
        }
        BuiltinParams::Gate(_) => {
            t = t.js("ReaGate");
        }
        BuiltinParams::Limiter(_) => {
            t = t.js("ReaLimit");
        }
        BuiltinParams::Reverb(_) => {
            t = t.js("ReaVerbate");
        }
        BuiltinParams::Delay(_) | BuiltinParams::Echo(_) => {
            t = t.js("ReaDelay");
        }
        BuiltinParams::Utility(_) => {
            // Utility is just gain/phase/width — often not needed as separate FX
        }
        _ => {
            tracing::debug!("Unmapped Ableton built-in device, skipping");
        }
    }
    t
}

// ── Color mapping ──────────────────────────────────────────────────────────

// ============================================================================
// Pro Tools → REAPER conversion
// ============================================================================

fn import_protools(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let session = dawfile_protools::read_session(path, 48000)?;
    let sample_rate = session.session_sample_rate as f64;

    let mut builder = ReaperProjectBuilder::new();
    builder = builder.sample_rate(session.session_sample_rate as i32);

    // Audio tracks
    for track in &session.audio_tracks {
        builder = builder.track(&track.name, |t| {
            let mut t = t;
            for tr in &track.regions {
                if tr.region_index as usize >= session.audio_regions.len() {
                    continue;
                }
                let region = &session.audio_regions[tr.region_index as usize];

                let position_secs = tr.start_pos as f64 / sample_rate;
                let length_secs = region.length as f64 / sample_rate;
                let offset_secs = region.sample_offset as f64 / sample_rate;

                if length_secs <= 0.0 {
                    continue;
                }

                // Get source filename
                let filename = session
                    .audio_files
                    .iter()
                    .find(|f| f.index == region.audio_file_index)
                    .map(|f| f.filename.as_str())
                    .unwrap_or("");

                t = t.item(position_secs, length_secs, |item| {
                    let mut item = item.name(&region.name);
                    if !filename.is_empty() {
                        item = item.source_wave(filename);
                    }
                    if offset_secs > 0.0 {
                        item = item.slip_offset(offset_secs);
                    }
                    item
                });
            }
            t
        });
    }

    // MIDI tracks
    for track in &session.midi_tracks {
        builder = builder.track(&track.name, |t| {
            let mut t = t;
            for tr in &track.regions {
                if tr.region_index as usize >= session.midi_regions.len() {
                    continue;
                }
                let region = &session.midi_regions[tr.region_index as usize];

                let position_secs = tr.start_pos as f64 / sample_rate;
                let length_secs = region.length as f64 / sample_rate;

                if length_secs <= 0.0 || region.events.is_empty() {
                    continue;
                }

                t = t.item(position_secs, length_secs, |item| {
                    item.name(&region.name).source_midi().midi(|midi| {
                        let mut midi = midi;
                        for event in &region.events {
                            midi = midi.at(event.position).note(
                                0,
                                0,
                                event.note,
                                event.velocity,
                                event.duration as u32,
                            );
                        }
                        midi
                    })
                });
            }
            t
        });
    }

    let project = builder.build();
    Ok(project.to_rpp_string())
}

// ============================================================================
// AAF → REAPER conversion
// ============================================================================

fn import_aaf(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let session = dawfile_aaf::read_session(path)?;

    let mut builder = ReaperProjectBuilder::new();
    builder = builder.sample_rate(session.session_sample_rate as i32);

    // Markers
    for (i, marker) in session.markers.iter().enumerate() {
        let pos_secs = marker.edit_rate.to_seconds(marker.position);
        builder = builder.marker(i as i32 + 1, pos_secs, &marker.comment);
    }

    // Tracks
    for track in &session.tracks {
        // Skip non-audio/midi tracks
        match track.kind {
            dawfile_aaf::TrackKind::Audio | dawfile_aaf::TrackKind::Midi => {}
            _ => continue,
        }

        builder = builder.track(&track.name, |t| {
            let mut t = t;
            for clip in &track.clips {
                let position_secs = track.edit_rate.to_seconds(clip.start_position);
                let length_secs = track.edit_rate.to_seconds(clip.length);

                if length_secs <= 0.0 {
                    continue;
                }

                match &clip.kind {
                    dawfile_aaf::ClipKind::SourceClip {
                        source_file,
                        source_start,
                        ..
                    } => {
                        t = t.item(position_secs, length_secs, |item| {
                            let mut item = item;

                            // Strip file:/// prefix from source path
                            if let Some(path_url) = source_file {
                                let file_path = path_url
                                    .strip_prefix("file:///")
                                    .or_else(|| path_url.strip_prefix("file://"))
                                    .unwrap_or(path_url);
                                item = item.source_wave(file_path);
                            }

                            // Source offset
                            let offset_secs = track.edit_rate.to_seconds(*source_start);
                            if offset_secs > 0.0 {
                                item = item.slip_offset(offset_secs);
                            }

                            item
                        });
                    }
                    dawfile_aaf::ClipKind::Filler => {
                        // Silence gap — skip
                    }
                    _ => {
                        // Transitions, operations — skip for now
                    }
                }
            }
            t
        });
    }

    let project = builder.build();
    Ok(project.to_rpp_string())
}

// ============================================================================
// DAWproject → REAPER conversion
// ============================================================================

fn import_dawproject(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let project = dawfile_dawproject::read_project(path)?;

    let mut builder = ReaperProjectBuilder::new();
    builder = builder.tempo_with_time_sig(
        project.transport.tempo,
        project.transport.numerator as i32,
        project.transport.denominator as i32,
    );

    // Markers and tempo automation from arrangement
    if let Some(ref arr) = project.arrangement {
        for (i, marker) in arr.markers.iter().enumerate() {
            builder = builder.marker(i as i32 + 1, marker.time, &marker.name);
        }

        // Tempo automation
        if !arr.tempo_automation.is_empty() {
            builder = builder.tempo_envelope(|env| {
                let mut env = env;
                for tp in &arr.tempo_automation {
                    env = env.point(tp.time, tp.bpm);
                }
                env
            });
        }
    }

    // Build tracks
    add_dawproject_tracks(&mut builder, &project.tracks, project.arrangement.as_ref());

    let rpp_project = builder.build();
    Ok(rpp_project.to_rpp_string())
}

fn add_dawproject_tracks(
    builder: &mut ReaperProjectBuilder,
    tracks: &[dawfile_dawproject::Track],
    arrangement: Option<&dawfile_dawproject::Arrangement>,
) {
    for track in tracks {
        let is_folder = !track.children.is_empty();
        let track_id = track.id.clone();

        // Take ownership via std::mem::replace to work around borrow issues
        let old = std::mem::replace(builder, ReaperProjectBuilder::new());
        *builder = old.track(&track.name, |t| {
            let mut t = t;

            // Mixer state from channel
            if let Some(ref ch) = track.channel {
                t = t.volume(ch.volume).pan(ch.pan);
                if ch.muted {
                    t = t.muted();
                }
                if ch.solo {
                    t = t.soloed();
                }

                // Devices/plugins
                for dev in &ch.devices {
                    if !dev.enabled {
                        continue;
                    }
                    match dev.format {
                        dawfile_dawproject::DeviceFormat::Vst2 => {
                            t = t.vst(&dev.name, "");
                        }
                        dawfile_dawproject::DeviceFormat::Vst3 => {
                            t = t.vst3(&dev.name, "");
                        }
                        dawfile_dawproject::DeviceFormat::Clap => {
                            t = t.clap(&dev.name, "");
                        }
                        _ => {}
                    }
                }
            }

            // Color
            if let Some(ref color_hex) = track.color
                && let Some(color) = parse_hex_color(color_hex)
            {
                t = t.color(color);
            }

            if is_folder {
                t = t.folder_start();
            }

            // Find lanes for this track in the arrangement
            if let Some(arr) = arrangement {
                for lane in &arr.lanes {
                    if lane.track != track_id {
                        continue;
                    }
                    if let dawfile_dawproject::LaneContent::Clips(clips) = &lane.content {
                        for clip in clips {
                            if !clip.enabled {
                                continue;
                            }
                            let pos = clip.time;
                            let len = clip.duration;
                            if len <= 0.0 {
                                continue;
                            }

                            t = t.item(pos, len, |item| {
                                let mut item = item;
                                if let Some(ref name) = clip.name {
                                    item = item.name(name);
                                }

                                match &clip.content {
                                    dawfile_dawproject::ClipContent::Audio(audio) => {
                                        if let Some(ref file_ref) = audio.file {
                                            item = item.source_wave(&file_ref.path);
                                        }
                                    }
                                    dawfile_dawproject::ClipContent::Notes(notes) => {
                                        item = item.source_midi().midi(|midi| {
                                            let mut midi = midi;
                                            for note in notes {
                                                let tick = (note.time * 960.0) as u64;
                                                let dur = (note.duration * 960.0) as u32;
                                                let vel = (note.velocity * 127.0) as u8;
                                                midi = midi.at(tick).note(
                                                    0,
                                                    note.channel,
                                                    note.key,
                                                    vel,
                                                    dur,
                                                );
                                            }
                                            midi
                                        });
                                    }
                                    _ => {}
                                }

                                // Fades
                                if let Some(fade_in) = clip.fade_in_time
                                    && fade_in > 0.0
                                {
                                    item = item.fade_in(
                                        fade_in,
                                        dawfile_reaper::types::item::FadeCurveType::Linear,
                                    );
                                }
                                if let Some(fade_out) = clip.fade_out_time
                                    && fade_out > 0.0
                                {
                                    item = item.fade_out(
                                        fade_out,
                                        dawfile_reaper::types::item::FadeCurveType::Linear,
                                    );
                                }

                                item
                            });
                        }
                    }
                }
            }

            t
        });

        // Recurse for child tracks
        if !track.children.is_empty() {
            add_dawproject_tracks(builder, &track.children, arrangement);

            // Add a folder-end marker on the last child
            // We use an empty track with folder_end to close the folder
            let old = std::mem::replace(builder, ReaperProjectBuilder::new());
            *builder = old.track("", |t| t.folder_end(1));
        }
    }
}

/// Parse a hex color string like "#FF8800" into a REAPER-compatible RGB u32.
fn parse_hex_color(hex: &str) -> Option<u32> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u32::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u32::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u32::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r << 16) | (g << 8) | b)
}

// ── Color mapping ──────────────────────────────────────────────────────────

/// Map Ableton color index (0-69) to a REAPER-compatible RGB integer.
///
/// Ableton uses a fixed 70-color palette. For now, we use a simplified
/// 7-hue mapping. A full palette can be added later.
fn ableton_color_to_rgb(color_index: i32) -> u32 {
    let (r, g, b): (u32, u32, u32) = match color_index % 7 {
        0 => (0xFF, 0x00, 0x00), // Red
        1 => (0xFF, 0xA5, 0x00), // Orange
        2 => (0xFF, 0xFF, 0x00), // Yellow
        3 => (0x00, 0xFF, 0x00), // Green
        4 => (0x00, 0xBF, 0xFF), // Blue
        5 => (0x80, 0x00, 0xFF), // Purple
        6 => (0xFF, 0x69, 0xB4), // Pink
        _ => (0x80, 0x80, 0x80), // Gray (unreachable)
    };
    (r << 16) | (g << 8) | b
}
