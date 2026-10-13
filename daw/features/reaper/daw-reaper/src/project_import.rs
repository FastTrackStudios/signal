//! Project file import support for REAPER.
//!
//! Registers a `projectimport` handler so REAPER's File > Open dialog
//! can open supported external DAW project/session formats. The flow is:
//!
//! 1. REAPER calls `want_project_file()` to check if we handle the extension
//! 2. REAPER calls `enum_file_extensions()` to populate the file dialog filter
//! 3. REAPER calls `load_project()` with a `ProjectStateContext` to feed RPP lines into

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

#[cfg(feature = "ableton")]
use dawfile_ableton::devices::BuiltinParams;
#[cfg(feature = "ableton")]
use dawfile_ableton::{Device, DeviceFormat};
use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::{ItemBuilder, ReaperProjectBuilder, TrackBuilder};
use reaper_medium::{OwnedProjectImportRegister, ReaperFunctionError, ReaperSession};

// ============================================================================
// Extension registry
// ============================================================================

/// File extensions we handle, with human-readable descriptions for the file
/// dialog. Built per-build so only the formats compiled in (via the
/// `protools`/`ableton`/`aaf`/`dawproject` features) are advertised to REAPER —
/// a format whose feature is off is neither offered in the dialog nor accepted.
fn supported_extensions() -> Vec<(&'static str, &'static str)> {
    let mut v: Vec<(&'static str, &'static str)> = Vec::new();
    #[cfg(feature = "ableton")]
    v.push(("als", "Ableton Live Set (*.als)"));
    #[cfg(feature = "protools")]
    {
        v.push(("ptx", "Pro Tools Session (*.ptx)"));
        v.push(("ptf", "Pro Tools Session (*.ptf)"));
        v.push(("pts", "Pro Tools Session (*.pts)"));
    }
    #[cfg(feature = "aaf")]
    v.push(("aaf", "Advanced Authoring Format (*.aaf)"));
    #[cfg(feature = "dawproject")]
    v.push(("dawproject", "DAWproject (*.dawproject)"));
    v
}

/// Pre-computed CString pairs (extension, description), leaked for stable pointers.
static CACHED_EXTENSIONS: OnceLock<Vec<(CString, CString)>> = OnceLock::new();

fn cached_extensions() -> &'static Vec<(CString, CString)> {
    CACHED_EXTENSIONS.get_or_init(|| {
        supported_extensions()
            .iter()
            .map(|(ext, desc)| (CString::new(*ext).unwrap(), CString::new(*desc).unwrap()))
            .collect()
    })
}

/// Register the shared DAW project importer with REAPER.
pub fn register_project_importer(session: &mut ReaperSession) -> Result<(), ReaperFunctionError> {
    let import_register =
        OwnedProjectImportRegister::new(want_project_file, enum_file_extensions, load_project);
    session.plugin_register_add_project_import(import_register)?;
    tracing::info!(
        extensions = supported_extensions()
            .iter()
            .map(|(ext, _)| *ext)
            .collect::<Vec<_>>()
            .join(","),
        "Project import handler registered"
    );
    Ok(())
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
    let want = supported_extensions()
        .iter()
        .any(|(ext, _)| lower.ends_with(&format!(".{ext}")));
    tracing::info!(path = %path, want, "project_import.want_project_file");
    want
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
    tracing::info!(path = %path, "project_import.load_project: begin");

    match import_file(&path, genstate) {
        Ok(()) => {
            tracing::info!(path = %path, "project_import.load_project: success");
            0
        }
        Err(e) => {
            tracing::error!(path = %path, error = %e, "project_import.load_project: failed");
            -1
        }
    }
}

// ============================================================================
// Import dispatcher
// ============================================================================

/// Convert any supported project file to REAPER RPP text, dispatching on the
/// file extension. Supported inputs: Pro Tools (`.ptx`/`.ptf`/`.pts`),
/// Ableton (`.als`), AAF (`.aaf`), and dawproject (`.dawproject`).
///
/// This is the shared entry point behind both the REAPER import hook and the
/// public [`crate::project_import::convert_to_rpp`]-based file API.
pub fn convert_to_rpp(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let lower = path.to_lowercase();
    // Each arm dispatches to its importer when that format's feature is
    // compiled in, and otherwise returns a clear "not compiled in" error
    // (REAPER won't offer the extension at all — see `supported_extensions` —
    // but the public conversion API can still be called with any path).
    if lower.ends_with(".als") {
        #[cfg(feature = "ableton")]
        return import_ableton(path);
        #[cfg(not(feature = "ableton"))]
        return Err(format!(
            "Ableton (.als) support not compiled in (enable the `ableton` feature): {path}"
        )
        .into());
    }
    if lower.ends_with(".ptx") || lower.ends_with(".ptf") || lower.ends_with(".pts") {
        #[cfg(feature = "protools")]
        return import_protools(path, None);
        #[cfg(not(feature = "protools"))]
        return Err(format!(
            "Pro Tools (.ptx/.ptf/.pts) support not compiled in (enable the `protools` feature): {path}"
        )
        .into());
    }
    if lower.ends_with(".aaf") {
        #[cfg(feature = "aaf")]
        return import_aaf(path);
        #[cfg(not(feature = "aaf"))]
        return Err(format!(
            "AAF (.aaf) support not compiled in (enable the `aaf` feature): {path}"
        )
        .into());
    }
    if lower.ends_with(".dawproject") {
        #[cfg(feature = "dawproject")]
        return import_dawproject(path);
        #[cfg(not(feature = "dawproject"))]
        return Err(format!(
            "DAWproject (.dawproject) support not compiled in (enable the `dawproject` feature): {path}"
        )
        .into());
    }
    Err(format!("Unsupported input format: {path}").into())
}

fn import_file(
    path: &str,
    genstate: *mut reaper_low::raw::ProjectStateContext,
) -> Result<(), Box<dyn std::error::Error>> {
    let rpp_text = convert_to_rpp(path)?;

    // Debug dump: write the converted RPP next to the source file so we can
    // inspect what REAPER is being asked to parse. Keyed off env var to override
    // the default /tmp location.
    let dump_dir = std::env::var("FTS_IMPORT_DUMP_DIR")
        .ok()
        .unwrap_or_else(|| "/tmp".to_string());
    let stem = std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "import".to_string());
    let dump_path = std::path::Path::new(&dump_dir).join(format!("fts-import-{stem}.rpp"));
    if let Err(e) = std::fs::write(&dump_path, &rpp_text) {
        tracing::warn!(
            dump_path = %dump_path.display(),
            error = %e,
            "project_import: could not write RPP dump"
        );
    } else {
        tracing::info!(
            dump_path = %dump_path.display(),
            bytes = rpp_text.len(),
            "project_import: wrote RPP dump"
        );
    }

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
        // ProjectStateContext::AddLine is printf-style — its first argument
        // is the format string. Any `%` characters in the raw RPP would
        // be interpreted as format specifiers and at best print garbage,
        // at worst crash on a missing variadic argument. Pre-escape
        // (the converter never emits intentional format specifiers).
        let escaped = if line.contains('%') {
            line.replace('%', "%%")
        } else {
            line.to_string()
        };
        let c_line = CString::new(escaped)?;
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

#[cfg(feature = "ableton")]
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

/// Synthesize a deterministic REAPER-style GUID from a stable input string.
///
/// REAPER's `TRACKID` is canonically `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
/// We derive 16 bytes via SHA-256 of the input and format them as a v4-shaped
/// UUID so re-running the conversion on the same PT session produces the
/// same GUID for the same track (matters for round-trip identity and for
/// tools that diff converter output).
/// Map a PT fade-def curve byte to a REAPER fade curve. PT/the converter only
/// distinguishes linear (`0`) from curved (non-zero); curved fades map to
/// Bezier, matching the official converter's behaviour.
fn pt_fade_curve(curve: u8) -> dawfile_reaper::types::item::FadeCurveType {
    use dawfile_reaper::types::item::FadeCurveType;
    if curve == 0 {
        FadeCurveType::Linear
    } else {
        FadeCurveType::Bezier
    }
}

fn deterministic_guid(seed: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Two 64-bit hashes of the seed give us 128 bits of GUID material.
    // We don't need cryptographic strength here — just stability + low
    // collision risk across at most a few hundred tracks per session.
    let mut h1 = DefaultHasher::new();
    seed.hash(&mut h1);
    let lo = h1.finish();
    let mut h2 = DefaultHasher::new();
    "salt:".hash(&mut h2);
    seed.hash(&mut h2);
    let hi = h2.finish();
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}",
        (hi >> 32) as u32,
        (hi >> 16) as u16,
        // Variant + version bits zeroed to keep this trivially distinguishable
        // from a true v4 UUID — REAPER doesn't care, but it makes the source
        // of the GUID self-documenting.
        hi as u16,
        (lo >> 48) as u16,
        lo & 0x0000_FFFF_FFFF_FFFFu64,
    )
}

#[cfg(feature = "ableton")]
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

#[cfg(feature = "ableton")]
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

#[cfg(feature = "ableton")]
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

#[cfg(feature = "ableton")]
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

#[cfg(feature = "ableton")]
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

#[cfg(feature = "ableton")]
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

#[cfg(feature = "ableton")]
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

/// REAPER's default `FIXEDLANES` settings for a freshly comp-enabled track.
///
/// Grounded in dawfile-reaper's round-trip fixture, which captures a real
/// REAPER 7 track as `FIXEDLANES 9 0 0 0 0`: bitfield `9` (the value REAPER
/// writes when fixed lanes are turned on), and the three booleans + recording
/// behavior left at their defaults (0). See the `test_parse_complex_track`
/// fixture in `dawfile-reaper/src/types/track.rs`.
fn default_fixed_lanes() -> dawfile_reaper::types::track::FixedLanesSettings {
    dawfile_reaper::types::track::FixedLanesSettings {
        bitfield: 9,
        allow_editing: false,
        show_play_only_lane: false,
        mask_playback: false,
        recording_behavior: 0,
    }
}

/// Apply the track-level fixed-lane metadata (`FIXEDLANES`/`LANENAME`/`LANEREC`)
/// for a Pro Tools track whose playlists were emitted across fixed lanes.
///
/// Shared by the audio and MIDI track paths. Lane 0 is the active playlist
/// (named after `playlist_name`, falling back to the track name when empty);
/// lanes 1..N are the alternates in order. Call only when
/// `track.has_alternate_playlists()`.
#[cfg(feature = "protools")]
fn apply_fixed_lane_settings(t: TrackBuilder, track: &dawfile_protools::Track) -> TrackBuilder {
    let lane_count = track.playlist_count() as i32;
    let mut lane_names: Vec<String> = Vec::with_capacity(lane_count as usize);
    let active_name = if track.playlist_name.is_empty() {
        track.name.clone()
    } else {
        track.playlist_name.clone()
    };
    lane_names.push(active_name);
    for pl in &track.alternate_playlists {
        lane_names.push(pl.name.clone());
    }
    t.fixed_lanes(default_fixed_lanes())
        .lane_names(dawfile_reaper::types::track::LaneNameSettings {
            lane_count,
            lane_names,
        })
        .lane_record(dawfile_reaper::types::track::LaneRecordSettings {
            record_enabled_lane: 0,
            comping_enabled_lane: 0,
            last_comping_lane: 0,
        })
}

/// Emit one Pro Tools playlist's region placements as REAPER items on a single
/// track builder.
///
/// Shared by both the active playlist and every alternate (comp) playlist so
/// alternates get the SAME source/offset/fade treatment as lane 0 — see
/// [`import_protools`]. `lane` is the REAPER fixed-lane index (`LANE` token):
/// `None` for ordinary single-playlist tracks (unchanged output), `Some(n)` for
/// fixed-lane tracks where lane 0 is the active playlist and 1..N the alternates.
///
/// `regions` is the playlist's own placement list; per-track fade defs
/// (`track.fades`) are matched by timeline position, so they only attach to the
/// active playlist (alternates carry no matching fade entries — acceptable, PT
/// stores fades against the active playlist only).
#[cfg(feature = "protools")]
#[allow(clippy::too_many_arguments)]
fn emit_audio_playlist(
    mut t: TrackBuilder,
    regions: &[dawfile_protools::TrackRegion],
    lane: Option<i32>,
    track: &dawfile_protools::Track,
    session: &dawfile_protools::ProToolsSession,
    sample_rate: f64,
    resolve_audio_path: &dyn Fn(&str) -> String,
) -> TrackBuilder {
    for tr in regions {
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

        // Get source filename and resolve to absolute on-disk path. The region's
        // `audio_file_index` from the parser is currently unreliable, so we match
        // the region NAME against the audio file stems (PT auto-generates region
        // names like "02 LORD OF THE FIGHT (Vocals)_1-03.L" where the part before
        // the trailing "-NN.L/R" is the source filename stem).
        fn region_to_file_stem(region_name: &str) -> &str {
            let without_chan = region_name
                .strip_suffix(".L")
                .or_else(|| region_name.strip_suffix(".R"))
                .unwrap_or(region_name);
            if let Some(idx) = without_chan.rfind('-') {
                let suffix = &without_chan[idx + 1..];
                if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                    return &without_chan[..idx];
                }
            }
            without_chan
        }
        let stem = region_to_file_stem(&region.name);
        let filename = session
            .audio_files
            .iter()
            .find(|f| {
                f.filename
                    .strip_suffix(".wav")
                    .or_else(|| f.filename.strip_suffix(".WAV"))
                    .map(|s| s == stem)
                    .unwrap_or(false)
            })
            .or_else(|| {
                session.audio_files.iter().find(|f| {
                    f.filename
                        .strip_suffix(".wav")
                        .or_else(|| f.filename.strip_suffix(".WAV"))
                        .map(|s| stem.starts_with(s) || s.starts_with(stem))
                        .unwrap_or(false)
                })
            })
            .map(|f| f.filename.as_str())
            .unwrap_or("");
        let absolute_path = resolve_audio_path(filename);

        // Match fade entries to this item by timeline position. See
        // `import_protools` for the full PT 0x262f fade-def encoding notes.
        let item_start = tr.start_pos;
        let item_end = item_start.saturating_add(region.length);
        let mut fade_in_secs: Option<(f64, u8)> = None;
        let mut fade_out_secs: Option<(f64, u8)> = None;
        let tolerance: u64 = (sample_rate as u64) / 1000; // ±1 ms
        let starts_in_crossfade = regions.iter().any(|o| {
            let os = o.start_pos;
            let ol = session
                .audio_regions
                .get(o.region_index as usize)
                .map(|r| r.length)
                .unwrap_or(0);
            os < item_start && item_start < os.saturating_add(ol)
        });
        for f in &track.fades {
            let crossfade = f.in_length > 0 && f.out_length > 0;
            let fade_total = f.in_length.max(f.out_length);
            if fade_total == 0 {
                continue;
            }
            if f.start_pos.abs_diff(item_start) <= tolerance {
                if f.in_length > 0 {
                    fade_in_secs = Some((f.in_length as f64 / sample_rate, f.curve));
                } else if f.out_length > 0 && starts_in_crossfade {
                    fade_in_secs = Some((f.out_length as f64 / sample_rate, f.curve));
                }
            }
            let f_end = f.start_pos.saturating_add(fade_total);
            if (crossfade && f.start_pos.abs_diff(item_end) <= tolerance)
                || (f.out_length > 0 && f_end.abs_diff(item_end) <= tolerance)
            {
                let out_len = if f.out_length > 0 {
                    f.out_length
                } else {
                    f.in_length
                };
                fade_out_secs = Some((out_len as f64 / sample_rate, f.curve));
            }
        }

        // Strip the trailing `.L`/`.R` channel suffix so a stereo clip reads as
        // one clean stereo item, matching the official converter.
        let clip_name = region
            .name
            .strip_suffix(".L")
            .or_else(|| region.name.strip_suffix(".R"))
            .unwrap_or(&region.name);

        t = t.item(position_secs, length_secs, |item| {
            let mut item = item;
            if !absolute_path.is_empty() {
                item = item.source_wave(&absolute_path).take_name(clip_name);
            } else {
                item = item.name(clip_name);
            }
            if offset_secs > 0.0 {
                item = item.slip_offset(offset_secs);
            }
            if let Some((fi, c)) = fade_in_secs {
                item = item.fade_in(fi, pt_fade_curve(c));
            }
            if let Some((fo, c)) = fade_out_secs {
                item = item.fade_out(fo, pt_fade_curve(c));
            }
            if let Some(l) = lane {
                item = item.fixed_lane(l);
            }
            item
        });
    }
    t
}

/// Emit one Pro Tools MIDI playlist's region placements as REAPER MIDI items on
/// a single track builder.
///
/// The MIDI counterpart of [`emit_audio_playlist`]: shared by both the active
/// playlist and every alternate (comp) playlist so alternates get the SAME
/// comp-flatten / note-window / fallback-duration treatment as lane 0. `lane` is
/// the REAPER fixed-lane index (`LANE` token): `None` for ordinary
/// single-playlist tracks (unchanged output), `Some(n)` for fixed-lane tracks
/// where lane 0 is the active playlist and 1..N the alternates.
#[cfg(feature = "protools")]
fn emit_midi_playlist(
    mut t: TrackBuilder,
    regions: &[dawfile_protools::TrackRegion],
    lane: Option<i32>,
    session: &dawfile_protools::ProToolsSession,
    sample_rate: f64,
) -> TrackBuilder {
    for tr in regions {
        if tr.region_index as usize >= session.midi_regions.len() {
            continue;
        }
        let region = &session.midi_regions[tr.region_index as usize];

        // Comp flatten: keep only this placement's notes — those
        // within the clip's source window `[clip_lo, note_trim)`
        // (chunk-tick space). See `TrackRegion`.
        let clip_lo = tr.clip_lo_ticks;
        let note_trim = tr.note_trim_ticks;

        let position_secs = tr.start_pos as f64 / sample_rate;
        let length_secs = region.length as f64 / sample_rate;

        if length_secs <= 0.0 || region.events.is_empty() {
            continue;
        }

        t = t.item(position_secs, length_secs, |item| {
            // `.midi(...)` adds the MIDI take; do NOT also call
            // `.source_midi()` — that would push an empty MIDI take
            // first, leaving the real data on take #1 (and REAPER
            // would render the item as silent take #0).
            let mut item = item.name(&region.name).midi(|midi| {
                // Pro Tools stores MIDI at 960,000 ticks/quarter; tell
                // the REAPER builder to use the same PPQN so positions
                // and durations don't need numeric rescaling.
                let mut midi = midi.ticks_per_qn(960_000);

                // Many PT regions store duration=0 for every event
                // (a paired note-on/note-off encoding the parser
                // doesn't fully reconstruct yet). For those events,
                // fall back to "until the next note of the same
                // pitch" so notes sustain naturally instead of
                // truncating to silence.
                const FALLBACK_DUR: u64 = 480_000; // half a quarter
                let region_end = region.length.max(FALLBACK_DUR);

                for (i, event) in region.events.iter().enumerate() {
                    if event.velocity == 0
                        || event.position < clip_lo
                        || event.position >= note_trim
                    {
                        continue;
                    }
                    let dur = if event.duration > 0 {
                        event.duration as u32
                    } else {
                        // Find the next note of the SAME pitch and
                        // end this one just before it. Failing that,
                        // use the region end, capped at one quarter.
                        let next = region.events[i + 1..]
                            .iter()
                            .find(|e| e.note == event.note)
                            .map(|e| e.position);
                        let end = next.unwrap_or(region_end);
                        let span = end.saturating_sub(event.position);
                        span.clamp(FALLBACK_DUR, 960_000) as u32
                    };
                    // Emit relative to the clip's kept-window
                    // start so notes sit inside the item, which
                    // begins `clip_lo` into the take.
                    midi = midi.at(event.position.saturating_sub(clip_lo)).note(
                        0,
                        0,
                        event.note,
                        event.velocity,
                        dur,
                    );
                }
                midi
            });
            if let Some(l) = lane {
                item = item.fixed_lane(l);
            }
            item
        });
    }
    t
}

/// Convert a Pro Tools session at `path` to a REAPER `.rpp` string.
///
/// Public so that command-line tools and tests can run the conversion without
/// going through the REAPER plugin entry point.
#[cfg(feature = "protools")]
pub fn protools_to_rpp(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    import_protools(path, None)
}

/// Like [`protools_to_rpp`], but emit audio-file paths relative to
/// `audio_base` instead of the `.ptx`'s own directory. Use this when the
/// `.ptx` is read from one location but the resulting `.rpp` will be opened
/// somewhere else (e.g. converting locally for a session that lives on another
/// machine): pass the session folder as it exists on the target machine.
#[cfg(feature = "protools")]
pub fn protools_to_rpp_with_audio_base(
    path: &str,
    audio_base: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    import_protools(path, Some(audio_base))
}

#[cfg(feature = "protools")]
fn import_protools(
    path: &str,
    audio_base: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let session = dawfile_protools::read_session(path, 48000)?;
    let sample_rate = session.session_sample_rate as f64;

    // Pro Tools stores audio files in `Audio Files/` next to the .ptx.
    // Resolve a bare filename ("kick.wav") to the on-disk absolute path so
    // REAPER items point at real audio instead of dangling-source stubs.
    // `audio_base` overrides the .ptx directory for path emission (used when
    // the .rpp will be opened on a different machine than the .ptx was read).
    let session_dir = match audio_base {
        Some(b) => std::path::PathBuf::from(b),
        None => std::path::Path::new(path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
    };
    // When a base override is given the files aren't local, so skip the
    // `.exists()` probes and always emit `<base>/Audio Files/<file>`.
    let force_emit = audio_base.is_some();
    let audio_files_dir = session_dir.join("Audio Files");
    let resolve_audio_path = |filename: &str| -> String {
        if filename.is_empty() {
            return String::new();
        }
        // Already absolute → use as-is.
        let p = std::path::Path::new(filename);
        if p.is_absolute() {
            return filename.to_string();
        }
        // Try Audio Files/ subdir first (the standard layout).
        let in_audio = audio_files_dir.join(filename);
        if force_emit || in_audio.exists() {
            return in_audio.to_string_lossy().into_owned();
        }
        // Fall back to session dir.
        let in_session = session_dir.join(filename);
        if in_session.exists() {
            return in_session.to_string_lossy().into_owned();
        }
        // Last resort: emit the Audio Files/ path even if the file is
        // missing — REAPER will show a "missing media" item rather than
        // an empty one, and the user can relink.
        in_audio.to_string_lossy().into_owned()
    };

    let mut builder = ReaperProjectBuilder::new();
    builder = builder.sample_rate(session.session_sample_rate as i32);

    // Initial tempo + time signature
    //
    // Pro Tools stores BPM relative to the click note value (encoded in
    // `ticks_per_beat`: 960000 = quarter, 480000 = eighth, 1440000 = dotted-quarter, etc.).
    // REAPER always interprets tempo as quarter-note BPM regardless of time signature.
    // Convert: reaper_bpm = pt_bpm × (pt_tpb / 960000).
    fn pt_bpm_to_reaper(pt_bpm: f64, pt_tpb: u64) -> f64 {
        if pt_tpb == 0 {
            pt_bpm
        } else {
            pt_bpm * (pt_tpb as f64 / 960_000.0)
        }
    }
    let init_tpb = session
        .tempo_events
        .first()
        .map(|t| t.ticks_per_beat)
        .unwrap_or(960_000);
    let init_bpm = pt_bpm_to_reaper(session.bpm, init_tpb);
    let (init_num, init_den) = session
        .meter_events
        .first()
        .map(|m| (m.numerator as i32, m.denominator as i32))
        .unwrap_or((4, 4));
    builder = builder.tempo_with_time_sig(init_bpm, init_num, init_den);

    // Tempo / meter envelope — emit only if there's more than one change.
    let has_tempo_changes = session.tempo_events.len() > 1;
    let has_meter_changes = session.meter_events.len() > 1;
    if has_tempo_changes || has_meter_changes {
        // Merge tempo + meter events on a single timeline keyed by sample position.
        // At each point we know the active (bpm, num, den); emit one envelope point.
        let mut positions: Vec<u64> = session
            .tempo_events
            .iter()
            .map(|t| t.sample_start)
            .chain(session.meter_events.iter().map(|m| m.sample_start))
            .collect();
        positions.sort_unstable();
        positions.dedup();

        builder = builder.tempo_envelope(|mut env| {
            let mut cur_bpm = init_bpm;
            let mut cur_num = init_num;
            let mut cur_den = init_den;
            for sample_pos in positions {
                if let Some(t) = session
                    .tempo_events
                    .iter()
                    .find(|t| t.sample_start == sample_pos)
                {
                    cur_bpm = pt_bpm_to_reaper(t.bpm, t.ticks_per_beat);
                }
                let meter_changed = session
                    .meter_events
                    .iter()
                    .find(|m| m.sample_start == sample_pos)
                    .map(|m| {
                        cur_num = m.numerator as i32;
                        cur_den = m.denominator as i32;
                    })
                    .is_some();
                let pos_secs = sample_pos as f64 / sample_rate;
                env = if meter_changed {
                    env.point_with_time_sig(pos_secs, cur_bpm, cur_num, cur_den)
                } else {
                    env.point(pos_secs, cur_bpm)
                };
            }
            env
        });
    }

    // Markers (Pro Tools Memory Locations). PT stores each marker's RGB
    // directly; REAPER's MARKER colour int is `0x01000000 | (r<<16)|(g<<8)|b`
    // (the `0x01000000` flag = "use custom colour"). Verified against the
    // official converter (INTRO → 0x01FF7CD6) and the session's actual colours.
    for marker in &session.markers {
        let pos_secs = marker.sample_pos as f64 / sample_rate;
        let color = marker
            .color_rgb
            .map(|(r, g, b)| 0x0100_0000i32 | ((r as i32) << 16) | ((g as i32) << 8) | (b as i32))
            .unwrap_or(0);
        builder = builder.marker_with_color(marker.number as i32, pos_secs, &marker.name, color);
    }

    // Build the unified track-emission order from PT's 0x251a display order
    // (`Track.display_order`). PT interleaves audio, MIDI, master and
    // folder/divider tracks in one on-screen list; emitting audio-then-MIDI
    // (or sorting by channel index) scrambles that. We assemble one merged
    // list — applying stereo-sibling dedup to the audio side first — then
    // stable-sort it by display order so the REAPER layout matches the source.
    #[derive(Clone, Copy)]
    enum Src {
        Audio,
        Midi,
    }
    struct Emit {
        src: Src,
        idx: usize,
        is_stereo: bool,
    }
    let mut emit: Vec<Emit> = Vec::new();
    // Pro Tools stores stereo tracks as TWO consecutive entries (one per
    // channel) sharing the same `name`, referencing the same stereo source via
    // `.L` / `.R` region siblings. Skip the R-channel sibling so we emit ONE
    // REAPER track per stereo pair.
    let mut i = 0;
    while i < session.audio_tracks.len() {
        let is_stereo = session
            .audio_tracks
            .get(i + 1)
            .map(|next| next.name == session.audio_tracks[i].name)
            .unwrap_or(false);
        emit.push(Emit {
            src: Src::Audio,
            idx: i,
            is_stereo,
        });
        i += if is_stereo { 2 } else { 1 };
    }
    for j in 0..session.midi_tracks.len() {
        emit.push(Emit {
            src: Src::Midi,
            idx: j,
            is_stereo: false,
        });
    }
    // Stable sort by PT display order. Tracks that didn't match a 0x251a entry
    // carry `u32::MAX` and sink to the bottom, keeping their relative order.
    emit.sort_by_key(|e| match e.src {
        Src::Audio => session.audio_tracks[e.idx].display_order,
        Src::Midi => session.midi_tracks[e.idx].display_order,
    });

    // Compute REAPER folder open/close from PT's reconstructed `folder_depth`
    // (0 = top level) over the merged emission order. PT's per-track depth and
    // folder-parent flag come from the `0x210b`/`0x210c` group decode in
    // dawfile-protools (see `Track::folder_depth` / `Track::is_folder`).
    //
    // REAPER models folders implicitly: a folder PARENT carries `ISBUS 1 1`
    // (handled below via `folder_start()`), and the LAST track in a folder
    // carries `ISBUS 2 -N` to pop N levels (`folder_end(N)`). We walk the
    // emission order tracking the current depth; whenever the NEXT track is
    // shallower (or we hit the end) the current track closes the difference.
    //
    // Folder parents are depth `d` and their children depth `d+1`; a leaf at
    // depth `d` that is the last before a return to depth `d-k` closes `k`
    // levels. Sessions with no `0x210c` groups have all-zero depth → no
    // folders, matching the official converter's flattening of PT "divider"
    // tracks.
    let depths: Vec<u32> = emit
        .iter()
        .map(|e| match e.src {
            Src::Audio => session.audio_tracks[e.idx].folder_depth,
            Src::Midi => session.midi_tracks[e.idx].folder_depth,
        })
        .collect();
    let folder_end_levels: Vec<i32> = {
        let n = depths.len();
        let mut ends = vec![0i32; n];
        for i in 0..n {
            let next = depths.get(i + 1).copied().unwrap_or(0);
            if depths[i] > next {
                ends[i] = (depths[i] - next) as i32;
            }
        }
        ends
    };
    // A track OPENS a REAPER folder only when the following track is deeper.
    // We drive folder-open off the reconstructed depth — NOT the raw
    // `is_folder` role byte — so PT "divider" folder-parents that have no
    // children (empty `0x210c`, e.g. the ALL THAT I AM session) stay flat,
    // matching the official converter instead of opening a folder that never
    // closes and swallows every later track.
    let folder_starts: Vec<bool> = (0..depths.len())
        .map(|i| depths.get(i + 1).copied().unwrap_or(0) > depths[i])
        .collect();

    // PT track comments are emitted as SWS/S&M Track Notes (a project-level
    // <EXTENSIONS> block keyed by track GUID), matching the official converter.
    // Collected here and appended after the project is serialized.
    let mut track_notes: Vec<(String, String)> = Vec::new();

    // PT's Master track (kind 0x05) is emitted as a normal REAPER track (REAPER's
    // own master is implicit). We tag its GUID in an <EXTENSIONS> block so a
    // future native RPP→PTX writer can round-trip it back to a PT Master rather
    // than a regular track/bus. Collected here, injected after serialization.
    let mut master_guids: Vec<String> = Vec::new();

    // Converted plugin FX, keyed by owning track name. Each entry is the raw
    // `<VST …>` block + preset name produced by the plugin bridge (e.g.
    // Omnisphere state transplanted into a Reaper VST3 chunk).
    // Normalize a track name by stripping a trailing ".NN" playlist suffix,
    // for fuzzy matching when the emitted RPP track name carries a playlist
    // suffix the PT plugin container's name does not.
    fn norm_track_name(name: &str) -> &str {
        match name.rsplit_once('.') {
            Some((base, suf)) if !suf.is_empty() && suf.chars().all(|c| c.is_ascii_digit()) => base,
            _ => name,
        }
    }
    // Pending converted plugins (track_name, fx). Claimed at most once per
    // emitted track: prefer an exact track-name match, then a normalized one.
    let mut pending_fx: Vec<(String, Option<crate::plugin_bridge::ConvertedFx>)> = session
        .plugin_states
        .iter()
        .filter_map(|ps| {
            crate::plugin_bridge::convert(ps).map(|fx| (ps.track_name.clone(), Some(fx)))
        })
        .collect();
    // Claim the FX for a track: exact name match first, else normalized.
    let mut claim_fx = |track_name: &str| -> Vec<crate::plugin_bridge::ConvertedFx> {
        if let Some(slot) = pending_fx
            .iter_mut()
            .find(|(n, fx)| fx.is_some() && n == track_name)
        {
            return slot.1.take().into_iter().collect();
        }
        if let Some(slot) = pending_fx
            .iter_mut()
            .find(|(n, fx)| fx.is_some() && norm_track_name(n) == norm_track_name(track_name))
        {
            return slot.1.take().into_iter().collect();
        }
        Vec::new()
    };

    for (plan_idx, e) in emit.iter().enumerate() {
        let end_levels = folder_end_levels[plan_idx];
        let is_folder_start = folder_starts[plan_idx];
        match e.src {
            Src::Audio => {
                let track = &session.audio_tracks[e.idx];
                let is_stereo = e.is_stereo;
                // REAPER track names must be clean; the PT output assignment
                // (`track.output`, e.g. "Analog 1-2"/"Bus 1") is routing, not part of
                // the name. Routing-to-bus emission is a separate follow-on.
                let display_name = track.name.clone();
                let track_guid = deterministic_guid(&format!("pt:{}:{}", plan_idx, track.name));
                if !track.comment.is_empty() {
                    track_notes.push((track_guid.clone(), track.comment.clone()));
                }
                if track.is_master {
                    master_guids.push(track_guid.clone());
                }
                builder = builder.track(&display_name, |t| {
                    let mut t = t
                        .guid(&track_guid)
                        .automation_mode(dawfile_reaper::types::track::AutomationMode::Read);
                    if is_folder_start {
                        t = t.folder_start();
                    }
                    // Apply per-track mix state from PT's 0x1029 block.
                    // Volume is in 0.1 dB units (centibel); ≤ -1440 means -∞.
                    // Pan is -100..+100. For STEREO PT tracks the stored "pan" is the
                    // left-channel pan (defaults to -100 = full-left, the natural
                    // state), NOT a balance adjustment. REAPER's default stereo-pan
                    // mode would interpret pan = -1 as "mute the right channel", so
                    // for stereo we emit pan = 0 (centered balance) regardless.
                    let linear_vol = if track.volume_centibel <= -1440 {
                        0.0
                    } else {
                        10f64.powf(track.volume_centibel as f64 / 200.0)
                    };
                    let reaper_pan = if is_stereo {
                        0.0
                    } else {
                        (track.pan as f64 / 100.0).clamp(-1.0, 1.0)
                    };
                    let mut t = t.volume(linear_vol).pan(reaper_pan);
                    // Mute via `0x1029 +5`. Accurate for the LORD family stems +
                    // ClickPrint + Inst stack; over-reports on SYZ / AC GTR / El
                    // Gtr / Bass Demo where the byte is set but PT shows audible.
                    // See `lord_of_the_fight_mute_pattern` test for the diff.
                    if track.mute {
                        t = t.muted();
                    }
                    if let Some(rgb) = pt_color_to_rgb(track.color_byte) {
                        t = t.color(rgb);
                    }
                    // Preserve PT track view height (px) so the track keeps its
                    // shape on round-trip. 0 = not parsed → leave Reaper default.
                    if track.height_px > 0 {
                        t = t.height(track.height_px as i32);
                    }
                    // Volume / mute automation envelopes (parsed from 0x260a[0]
                    // / 0x260a[1]). Volume points are centibel → linear gain
                    // (1.0 = 0 dB). Mute points are step/square (Reaper MUTEENV:
                    // 0 = muted, 1 = audible).
                    if !track.volume_automation.is_empty() {
                        t = t.envelope("VOLENV2", |mut e| {
                            e = e.active().visible();
                            for bp in &track.volume_automation {
                                let secs = bp.time_samples as f64 / sample_rate;
                                let lin = 10f64.powf(bp.value_centibel as f64 / 200.0);
                                e = e.linear(secs, lin);
                            }
                            e
                        });
                    }
                    if !track.mute_automation.is_empty() {
                        t = t.envelope("MUTEENV", |mut e| {
                            e = e.active().visible();
                            for bp in &track.mute_automation {
                                let secs = bp.time_samples as f64 / sample_rate;
                                let v = if bp.muted { 1.0 } else { 0.0 };
                                e = e.square(secs, v);
                            }
                            e
                        });
                    }
                    // Pan automation (0x260d > 0x260c[0] > 0x260a[0]) → PANENV2.
                    // Stored value is pan × 100; Reaper pan is -1..1.
                    if !track.pan_automation.is_empty() {
                        t = t.envelope("PANENV2", |mut e| {
                            e = e.active().visible();
                            for bp in &track.pan_automation {
                                let secs = bp.time_samples as f64 / sample_rate;
                                e = e.linear(secs, (bp.value as f64 / 100.0).clamp(-1.0, 1.0));
                            }
                            e
                        });
                    }
                    // Alternate (comp) playlists → REAPER fixed item lanes.
                    // A PT track with >1 playlist becomes ONE fixed-lane REAPER
                    // track: lane 0 = active playlist (`track.regions`), lanes
                    // 1..N = each `track.alternate_playlists[i]`. Single-playlist
                    // tracks keep their old output (lane = None, no FIXEDLANES).
                    let has_lanes = track.has_alternate_playlists();
                    let active_lane = if has_lanes { Some(0) } else { None };
                    t = emit_audio_playlist(
                        t,
                        &track.regions,
                        active_lane,
                        track,
                        &session,
                        sample_rate,
                        &resolve_audio_path,
                    );
                    if has_lanes {
                        for (i, pl) in track.alternate_playlists.iter().enumerate() {
                            t = emit_audio_playlist(
                                t,
                                &pl.regions,
                                Some(i as i32 + 1),
                                track,
                                &session,
                                sample_rate,
                                &resolve_audio_path,
                            );
                        }
                        t = apply_fixed_lane_settings(t, track);
                    }
                    if end_levels > 0 {
                        t = t.folder_end(end_levels);
                    }
                    t
                });
            }
            Src::Midi => {
                let track = &session.midi_tracks[e.idx];
                // REAPER track names must be clean; the PT output assignment
                // (`track.output`, e.g. "Analog 1-2"/"Bus 1") is routing, not part of
                // the name. Routing-to-bus emission is a separate follow-on.
                let display_name = track.name.clone();
                let track_guid = deterministic_guid(&format!("pt:{}:{}", plan_idx, track.name));
                if !track.comment.is_empty() {
                    track_notes.push((track_guid.clone(), track.comment.clone()));
                }
                if track.is_master {
                    master_guids.push(track_guid.clone());
                }
                let fxs = claim_fx(&track.name);
                let fx_guid_base = track_guid.clone();
                builder = builder.track(&display_name, |t| {
                    let mut t = t
                        .guid(&track_guid)
                        .automation_mode(dawfile_reaper::types::track::AutomationMode::Read);
                    if is_folder_start {
                        t = t.folder_start();
                    }
                    // Converted instrument plugins (e.g. Omnisphere) — transplant
                    // the full plugin state onto the track's FX chain.
                    for (i, fx) in fxs.iter().enumerate() {
                        let block = fx.raw_block.clone();
                        let preset = fx.preset_name.clone();
                        let fxid = deterministic_guid(&format!("{fx_guid_base}:fx{i}"));
                        t = t.fx(move |b| b.raw_block(block).preset(preset).fxid(fxid).wak(0, 0));
                    }
                    let linear_vol = if track.volume_centibel <= -1440 {
                        0.0
                    } else {
                        10f64.powf(track.volume_centibel as f64 / 200.0)
                    };
                    let reaper_pan = (track.pan as f64 / 100.0).clamp(-1.0, 1.0);
                    let mut t = t.volume(linear_vol).pan(reaper_pan);
                    // Mute via `0x1029 +5`. Accurate for the LORD family stems +
                    // ClickPrint + Inst stack; over-reports on SYZ / AC GTR / El
                    // Gtr / Bass Demo where the byte is set but PT shows audible.
                    // See `lord_of_the_fight_mute_pattern` test for the diff.
                    if track.mute {
                        t = t.muted();
                    }
                    if let Some(rgb) = pt_color_to_rgb(track.color_byte) {
                        t = t.color(rgb);
                    }
                    // Preserve PT track view height (px) so the track keeps its
                    // shape on round-trip. 0 = not parsed → leave Reaper default.
                    if track.height_px > 0 {
                        t = t.height(track.height_px as i32);
                    }
                    // Volume / mute automation envelopes (parsed from 0x260a[0]
                    // / 0x260a[1]). Volume points are centibel → linear gain
                    // (1.0 = 0 dB). Mute points are step/square (Reaper MUTEENV:
                    // 0 = muted, 1 = audible).
                    if !track.volume_automation.is_empty() {
                        t = t.envelope("VOLENV2", |mut e| {
                            e = e.active().visible();
                            for bp in &track.volume_automation {
                                let secs = bp.time_samples as f64 / sample_rate;
                                let lin = 10f64.powf(bp.value_centibel as f64 / 200.0);
                                e = e.linear(secs, lin);
                            }
                            e
                        });
                    }
                    if !track.mute_automation.is_empty() {
                        t = t.envelope("MUTEENV", |mut e| {
                            e = e.active().visible();
                            for bp in &track.mute_automation {
                                let secs = bp.time_samples as f64 / sample_rate;
                                let v = if bp.muted { 1.0 } else { 0.0 };
                                e = e.square(secs, v);
                            }
                            e
                        });
                    }
                    // Pan automation (0x260d > 0x260c[0] > 0x260a[0]) → PANENV2.
                    // Stored value is pan × 100; Reaper pan is -1..1.
                    if !track.pan_automation.is_empty() {
                        t = t.envelope("PANENV2", |mut e| {
                            e = e.active().visible();
                            for bp in &track.pan_automation {
                                let secs = bp.time_samples as f64 / sample_rate;
                                e = e.linear(secs, (bp.value as f64 / 100.0).clamp(-1.0, 1.0));
                            }
                            e
                        });
                    }
                    // Alternate (comp) playlists → REAPER fixed item lanes.
                    // Mirrors the audio path: a MIDI track with >1 playlist
                    // becomes ONE fixed-lane REAPER track (lane 0 = active
                    // playlist, lanes 1..N = each alternate). Single-playlist
                    // tracks keep their old output (lane = None, no FIXEDLANES).
                    let has_lanes = track.has_alternate_playlists();
                    let active_lane = if has_lanes { Some(0) } else { None };
                    t = emit_midi_playlist(t, &track.regions, active_lane, &session, sample_rate);
                    if has_lanes {
                        for (i, pl) in track.alternate_playlists.iter().enumerate() {
                            t = emit_midi_playlist(
                                t,
                                &pl.regions,
                                Some(i as i32 + 1),
                                &session,
                                sample_rate,
                            );
                        }
                        t = apply_fixed_lane_settings(t, track);
                    }
                    if end_levels > 0 {
                        t = t.folder_end(end_levels);
                    }
                    t
                });
            }
        }
    }

    let project = builder.build();
    let mut rpp = project.to_rpp_string();

    // Append the track comments as an <EXTENSIONS> block of <S&M_TRACKNOTES>
    // entries (the SWS Track Notes format the official converter uses),
    // inserted as the last child before the project's closing `>`. Each note
    // is keyed by the track's GUID; comment lines are emitted verbatim with a
    // leading `|`.
    if !track_notes.is_empty() || !master_guids.is_empty() {
        let mut ext = String::from("  <EXTENSIONS\n");
        for (guid, comment) in &track_notes {
            ext.push_str(&format!("    <S&M_TRACKNOTES {guid}\n"));
            for line in comment.split('\n') {
                ext.push_str(&format!("      |{line}\n"));
            }
            ext.push_str("    >\n");
        }
        // Mark each PT Master track (auto-generated on import) so a native
        // RPP→PTX writer can map it back to a PT Master rather than a regular
        // track. `1` = auto-generated by the Pro Tools import.
        for guid in &master_guids {
            ext.push_str(&format!(
                "    <FTS_PTIMPORT_MASTER {guid}\n      1\n    >\n"
            ));
        }
        ext.push_str("  >\n");
        if let Some(pos) = rpp.rfind("\n>") {
            rpp.insert_str(pos + 1, &ext);
        } else {
            rpp.push_str(&ext);
        }
    }

    Ok(rpp)
}

// ============================================================================
// AAF → REAPER conversion
// ============================================================================

#[cfg(feature = "aaf")]
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

#[cfg(feature = "dawproject")]
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

#[cfg(feature = "dawproject")]
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

// PT track-color palette: index -> (r,g,b). Derived empirically by sweeping
// every index 0..=255 through the official PT→Reaper converter (47-track
// unanimous agreement per index). idx 0 is Reaper's default "no color".
pub const PT_TRACK_PALETTE: [(u8, u8, u8); 256] = [
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (65, 110, 216),
    (65, 110, 216),
    (65, 75, 216),
    (105, 65, 216),
    (163, 65, 216),
    (216, 65, 209),
    (216, 65, 153),
    (216, 65, 120),
    (216, 65, 77),
    (216, 65, 80),
    (216, 102, 65),
    (216, 148, 65),
    (216, 211, 65),
    (151, 216, 65),
    (90, 216, 65),
    (65, 216, 85),
    (65, 216, 105),
    (65, 216, 135),
    (65, 216, 171),
    (65, 216, 214),
    (65, 181, 216),
    (65, 148, 216),
    (65, 120, 216),
    (65, 110, 216),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
    (56, 102, 68),
    (56, 102, 77),
    (56, 102, 88),
    (56, 102, 101),
    (56, 91, 102),
    (56, 81, 102),
    (56, 72, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 69, 102),
    (56, 59, 102),
    (68, 56, 102),
    (85, 56, 102),
    (102, 56, 99),
    (102, 56, 82),
    (102, 56, 72),
    (102, 56, 59),
    (102, 56, 60),
    (102, 67, 56),
    (102, 81, 56),
    (102, 100, 56),
    (82, 102, 56),
    (63, 102, 56),
    (56, 102, 62),
];

/// Convert a Pro Tools track color palette index to a REAPER `COLOR`/`PEAKCOL`
/// integer (`0x01000000 | (B << 16) | (G << 8) | R`).
///
/// `index` is the palette byte parsed from the track's `0x200b` block (framed
/// by `01 <idx> 00 01 00 03`). The index→RGB mapping is PT's genuine palette,
/// recovered by sweeping every index 0..=255 through the official PT→Reaper
/// converter and reading back the emitted `PEAKCOL` (47-track unanimous
/// agreement per index). It is therefore exact and complete: change a track's
/// color in Pro Tools and the parsed index — and hence this color — follows.
///
/// Returns `None` only when the lookup would yield Reaper's plain default
/// (index 0/1), so no explicit color is emitted and the track keeps Reaper's
/// own default.
pub fn pt_color_to_rgb(index: u8) -> Option<u32> {
    // idx 0/1 map to Reaper's default track color (56, 69, 102) — treat as
    // "no custom color" so we don't override Reaper's default needlessly.
    if index <= 1 {
        return None;
    }
    let (r, g, b) = PT_TRACK_PALETTE[index as usize];
    Some(0x01_00_00_00 | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

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

/// Build a self-contained, synthetic Pro Tools session with one audio track
/// that has an active playlist plus two alternate playlists (3 playlists /
/// takes total), drive it through the same per-track lane wiring the importer
/// uses (`emit_audio_playlist` + `default_fixed_lanes` + lane name/record
/// settings), and return the built REAPER project in fixed-lane (comp) mode.
///
/// This is the conversion seam shared by the `dump_lanes_rpp` example and the
/// `alternate_playlists_*` tests: a `ProToolsSession` in, a
/// `dawfile_reaper::ReaperProject` out. Serialize it with
/// `ReaperProject::to_rpp_string()` to get loadable .rpp text.
#[cfg(feature = "protools")]
pub fn build_demo_playlist_project() -> dawfile_reaper::ReaperProject {
    use dawfile_protools::{
        AudioFile, AudioRegion, Playlist, ProToolsSession, Track, TrackKind, TrackRegion,
    };

    let mk_region = |index: u16, name: &str, length: u64| AudioRegion {
        name: name.to_string(),
        index,
        start_pos: 0,
        sample_offset: 0,
        length,
        audio_file_index: 0,
        source_file_uid: None,
    };
    let mk_placement = |region_index: u16, start_pos: u64| TrackRegion {
        region_index,
        start_pos,
        clip_flag_53: false,
        clip_muted: false,
        clip_color: None,
        clip_lo_ticks: 0,
        note_trim_ticks: u64::MAX,
    };

    let mut session = ProToolsSession {
        version: 12,
        session_sample_rate: 48000,
        bpm: 120.0,
        tempo_events: Vec::new(),
        meter_events: Vec::new(),
        markers: Vec::new(),
        audio_files: Vec::new(),
        audio_regions: Vec::new(),
        audio_tracks: Vec::new(),
        midi_regions: Vec::new(),
        midi_tracks: Vec::new(),
        plugins: Vec::new(),
        io_channels: Vec::new(),
        routing_entries: Vec::new(),
        edit_groups: Vec::new(),
        stem_mappings: Vec::new(),
        internal_tracks: Vec::new(),
        plugin_states: Vec::new(),
    };
    session.audio_files.push(AudioFile {
        filename: "Take.wav".to_string(),
        index: 0,
        length: 96_000,
        source_uid: None,
    });
    // region 0 = active take, region 1/2 = alternate takes.
    session.audio_regions.push(mk_region(0, "Take-01", 48_000));
    session.audio_regions.push(mk_region(1, "Take-02", 48_000));
    session.audio_regions.push(mk_region(2, "Take-03", 48_000));

    let track = Track {
        name: "Vocal".to_string(),
        kind: TrackKind::Audio,
        index: 0,
        playlist_name: "Vocal".to_string(),
        regions: vec![mk_placement(0, 0)],
        fades: Vec::new(),
        volume_centibel: 0,
        mute: false,
        solo: false,
        solo_defeat: false,
        inactive: false,
        pan: 0,
        alternate_playlists: vec![
            Playlist {
                name: "Vocal.01".to_string(),
                regions: vec![mk_placement(1, 0)],
            },
            Playlist {
                name: "Vocal.02".to_string(),
                regions: vec![mk_placement(2, 0)],
            },
        ],
        output: String::new(),
        color_byte: 0,
        height_px: 0,
        volume_automation: Vec::new(),
        mute_automation: Vec::new(),
        pan_automation: Vec::new(),
        is_folder: false,
        folder_depth: 0,
        display_order: 0,
        comment: String::new(),
        is_master: false,
    };

    let sample_rate = 48_000.0;
    let resolve = |f: &str| f.to_string();

    // Reproduce the importer's per-track lane wiring (see `import_protools`):
    // active playlist on lane 0, each alternate on lanes 1..N.
    let mut builder = ReaperProjectBuilder::new();
    builder = builder.track("Vocal", |mut t| {
        t = emit_audio_playlist(
            t,
            &track.regions,
            Some(0),
            &track,
            &session,
            sample_rate,
            &resolve,
        );
        for (i, pl) in track.alternate_playlists.iter().enumerate() {
            t = emit_audio_playlist(
                t,
                &pl.regions,
                Some(i as i32 + 1),
                &track,
                &session,
                sample_rate,
                &resolve,
            );
        }
        let lane_count = track.playlist_count() as i32;
        let mut lane_names = vec![track.playlist_name.clone()];
        for pl in &track.alternate_playlists {
            lane_names.push(pl.name.clone());
        }
        t.fixed_lanes(default_fixed_lanes())
            .lane_names(dawfile_reaper::types::track::LaneNameSettings {
                lane_count,
                lane_names,
            })
            .lane_record(dawfile_reaper::types::track::LaneRecordSettings {
                record_enabled_lane: 0,
                comping_enabled_lane: 0,
                last_comping_lane: 0,
            })
    });

    builder.build()
}

#[cfg(all(test, feature = "protools"))]
mod folder_tests {
    use super::{default_fixed_lanes, emit_audio_playlist, protools_to_rpp};
    use dawfile_reaper::RppSerialize;
    use dawfile_reaper::builder::ReaperProjectBuilder;

    fn fixture(name: &str) -> String {
        format!(
            "{}/tests/reaper-assets/protools/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    /// Extract the (NAME, ISBUS) sequence from RPP track blocks in order.
    fn track_isbus(rpp: &str) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        let mut cur_name: Option<String> = None;
        let mut cur_isbus: Option<String> = None;
        for line in rpp.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("NAME ") {
                if let Some(n) = cur_name.take() {
                    out.push((n, cur_isbus.take()));
                }
                cur_name = Some(rest.trim_matches('"').to_string());
                cur_isbus = None;
            } else if let Some(rest) = t.strip_prefix("ISBUS ") {
                cur_isbus = Some(rest.to_string());
            }
        }
        if let Some(n) = cur_name.take() {
            out.push((n, cur_isbus.take()));
        }
        out
    }

    /// Authored: FolderA{ChildA1, ChildA2{GrandA2a, GrandA2b}}, SiblingB.
    /// Expect FolderA + ChildA2 open folders, GrandA2b closes 2 levels.
    #[test]
    fn nested_folder_round_trip_emits_isbus() {
        let rpp = protools_to_rpp(&fixture("folder-nesting.ptx")).expect("convert");
        let seq = track_isbus(&rpp);
        let get = |n: &str| {
            seq.iter()
                .find(|(name, _)| name == n)
                .map(|(_, i)| i.clone())
        };
        assert_eq!(get("FolderA"), Some(Some("1 1".to_string())));
        assert_eq!(get("ChildA1"), Some(None));
        assert_eq!(get("ChildA2"), Some(Some("1 1".to_string())));
        assert_eq!(get("GrandA2a"), Some(None));
        assert_eq!(get("GrandA2b"), Some(Some("2 -2".to_string())));
        assert_eq!(get("SiblingB"), Some(None));
    }

    /// Authored: two independent sibling folders F1{leafA,leafB} F2{leafC,leafD}
    /// plus a top-level leaf. Each folder closes one level on its last child.
    // ── Pro Tools alternate playlists → REAPER fixed lanes ──────────────────
    use dawfile_protools::{
        AudioFile, AudioRegion, Playlist, ProToolsSession, Track, TrackKind, TrackRegion,
    };

    /// Build a minimal PT audio region referencing audio-file index 0.
    fn mk_region(index: u16, name: &str, length: u64) -> AudioRegion {
        AudioRegion {
            name: name.to_string(),
            index,
            start_pos: 0,
            sample_offset: 0,
            length,
            audio_file_index: 0,
            source_file_uid: None,
        }
    }

    fn mk_placement(region_index: u16, start_pos: u64) -> TrackRegion {
        TrackRegion {
            region_index,
            start_pos,
            clip_flag_53: false,
            clip_muted: false,
            clip_color: None,
            clip_lo_ticks: 0,
            note_trim_ticks: u64::MAX,
        }
    }

    /// A bare PT audio track with no alternate playlists.
    fn mk_track(name: &str, regions: Vec<TrackRegion>) -> Track {
        Track {
            name: name.to_string(),
            kind: TrackKind::Audio,
            index: 0,
            playlist_name: name.to_string(),
            regions,
            fades: Vec::new(),
            volume_centibel: 0,
            mute: false,
            solo: false,
            solo_defeat: false,
            inactive: false,
            pan: 0,
            alternate_playlists: Vec::new(),
            output: String::new(),
            color_byte: 0,
            height_px: 0,
            volume_automation: Vec::new(),
            mute_automation: Vec::new(),
            pan_automation: Vec::new(),
            is_folder: false,
            folder_depth: 0,
            display_order: 0,
            comment: String::new(),
            is_master: false,
        }
    }

    fn empty_session() -> ProToolsSession {
        ProToolsSession {
            version: 12,
            session_sample_rate: 48000,
            bpm: 120.0,
            tempo_events: Vec::new(),
            meter_events: Vec::new(),
            markers: Vec::new(),
            audio_files: Vec::new(),
            audio_regions: Vec::new(),
            audio_tracks: Vec::new(),
            midi_regions: Vec::new(),
            midi_tracks: Vec::new(),
            plugins: Vec::new(),
            io_channels: Vec::new(),
            routing_entries: Vec::new(),
            edit_groups: Vec::new(),
            stem_mappings: Vec::new(),
            internal_tracks: Vec::new(),
            plugin_states: Vec::new(),
        }
    }

    // ── PT MIDI comp playlists → REAPER fixed lanes (synthetic, no fixture) ──
    //
    // No bundled PT fixture carries MIDI alternate playlists (verified by
    // scanning every `.ptx`/`.ptf` in `dawfile-protools/tests/fixtures`: zero
    // MIDI tracks have a non-empty `alternate_playlists`). So the MIDI lane path
    // is exercised the same in-process way the audio synthetic test uses: build
    // a fixed-lane REAPER project from an in-memory MIDI `ProToolsSession` by
    // driving the SAME importer seam (`emit_midi_playlist` +
    // `apply_fixed_lane_settings`, mirroring the `Src::Midi` arm in
    // `import_protools`), then assert on the emitted RPP.

    use dawfile_protools::{MidiEvent, MidiRegion};

    /// A single-note MIDI region. `clip_lo_ticks = 0` / `note_trim_ticks =
    /// u64::MAX` on the referencing placement keep the note (see `mk_placement`).
    fn mk_midi_region(index: u16, name: &str, note: u8) -> MidiRegion {
        MidiRegion {
            name: name.to_string(),
            index,
            start_pos: 0,
            sample_offset: 0,
            length: 96_000,
            clip_start_ticks: 0,
            clip_src_ticks: 0,
            clip_len_ticks: 0,
            take_offset_ticks: 0,
            events: vec![MidiEvent {
                position: 0,
                duration: 480,
                note,
                velocity: 96,
            }],
        }
    }

    /// Build a MIDI `ProToolsSession` with one MIDI track that has an active
    /// playlist plus `alternates` alternate playlists (one single-note region
    /// each). `alternates == 0` → single-playlist track (no lanes expected).
    fn build_midi_session(alternates: usize) -> ProToolsSession {
        let midi_regions: Vec<MidiRegion> = (0..=alternates)
            .map(|i| mk_midi_region(i as u16, &format!("Keys.{i:02}"), 60 + i as u8))
            .collect();

        let alternate_playlists: Vec<Playlist> = (0..alternates)
            .map(|i| Playlist {
                name: format!("Keys.{:02}", i + 1),
                // Alternate k references MIDI region k+1.
                regions: vec![mk_placement(i as u16 + 1, 48_000 * (i as u64 + 1))],
            })
            .collect();

        let track = Track {
            name: "Keys".to_string(),
            kind: TrackKind::Midi,
            index: 0,
            playlist_name: "Keys".to_string(),
            // Active playlist references MIDI region 0.
            regions: vec![mk_placement(0, 0)],
            fades: Vec::new(),
            volume_centibel: 0,
            mute: false,
            solo: false,
            solo_defeat: false,
            inactive: false,
            pan: 0,
            alternate_playlists,
            output: String::new(),
            color_byte: 0,
            height_px: 0,
            volume_automation: Vec::new(),
            mute_automation: Vec::new(),
            pan_automation: Vec::new(),
            is_folder: false,
            folder_depth: 0,
            display_order: 0,
            comment: String::new(),
            is_master: false,
        };

        let mut session = empty_session();
        session.midi_regions = midi_regions;
        session.midi_tracks = vec![track];
        session
    }

    /// Drive a synthetic MIDI session through the importer's per-track lane
    /// wiring (the same `emit_midi_playlist` + `apply_fixed_lane_settings`
    /// sequence the `Src::Midi` arm of `import_protools` uses) and serialize.
    fn build_midi_lane_rpp(session: &ProToolsSession) -> String {
        let track = &session.midi_tracks[0];
        let sample_rate = 48_000.0;
        let has_lanes = track.has_alternate_playlists();
        let active_lane = if has_lanes { Some(0) } else { None };

        let mut builder = ReaperProjectBuilder::new();
        builder = builder.track(&track.name, |mut t| {
            t = super::emit_midi_playlist(t, &track.regions, active_lane, session, sample_rate);
            if has_lanes {
                for (i, pl) in track.alternate_playlists.iter().enumerate() {
                    t = super::emit_midi_playlist(
                        t,
                        &pl.regions,
                        Some(i as i32 + 1),
                        session,
                        sample_rate,
                    );
                }
                t = super::apply_fixed_lane_settings(t, track);
            }
            t
        });
        builder.build().to_rpp_string()
    }

    fn count_line(rpp: &str, token: &str) -> usize {
        rpp.lines()
            .filter(|l| l.trim_start().starts_with(token))
            .count()
    }

    /// The fixed lane each item sits on, read out of the emitted RPP.
    ///
    /// There is no per-item `LANE` token in REAPER's format — a fixed-lanes
    /// track divides its height evenly and each item states its slice as
    /// `YPOS <y> <height> <mode>`, so the lane is `y / height`. This is the
    /// same derivation `Item::from_block` performs on read.
    fn item_lanes(rpp: &str) -> Vec<i32> {
        rpp.lines()
            .filter_map(|l| l.trim_start().strip_prefix("YPOS "))
            .filter_map(|rest| {
                let mut f = rest.split_whitespace();
                let y: f64 = f.next()?.parse().ok()?;
                let height: f64 = f.next()?.parse().ok()?;
                (height > 1e-9).then(|| (y / height).round() as i32)
            })
            .collect()
    }

    /// A MIDI track with an active playlist + 2 alternates becomes a fixed-lane
    /// REAPER track: FIXEDLANES/LANENAME present, items on lanes 0/1/2, and the
    /// item sources are MIDI (`<SOURCE MIDI`), not audio.
    #[test]
    fn midi_alternate_playlists_become_fixed_lanes() {
        let session = build_midi_session(2);
        let rpp = build_midi_lane_rpp(&session);

        assert!(count_line(&rpp, "FIXEDLANES") > 0, "expected FIXEDLANES");
        assert!(count_line(&rpp, "LANENAME ") > 0, "expected LANENAME");
        // Per-item lane assignments: active on lane 0, alternates on 1 and 2.
        let mut lanes = item_lanes(&rpp);
        lanes.sort_unstable();
        assert_eq!(
            lanes,
            vec![0, 1, 2],
            "expected the active playlist on lane 0 and the alternates on 1 \
             and 2:\n{rpp}"
        );
        // Items must be MIDI, not audio.
        assert!(
            rpp.contains("<SOURCE MIDI"),
            "expected MIDI item sources:\n{rpp}"
        );
        assert!(
            !rpp.contains("<SOURCE WAVE"),
            "MIDI comp track must not emit audio sources:\n{rpp}"
        );
    }

    /// Negative control: a single-playlist MIDI track (no alternates) is NOT
    /// converted into fixed lanes.
    #[test]
    fn midi_single_playlist_has_no_fixed_lanes() {
        let session = build_midi_session(0);
        let rpp = build_midi_lane_rpp(&session);

        assert_eq!(
            count_line(&rpp, "FIXEDLANES"),
            0,
            "single-playlist MIDI track must not emit FIXEDLANES:\n{rpp}"
        );
        assert_eq!(
            count_line(&rpp, "LANENAME "),
            0,
            "single-playlist MIDI track must not emit LANENAME:\n{rpp}"
        );
        // It must still emit its one MIDI item.
        assert!(
            rpp.contains("<SOURCE MIDI"),
            "expected one MIDI item:\n{rpp}"
        );
    }

    #[test]
    fn alternate_playlists_become_fixed_lanes() {
        // Same 3-playlist synthetic session the example/round-trip test use.
        let project = super::build_demo_playlist_project();

        // Domain-level assertions on the built track.
        let rtrack = &project.tracks[0];
        assert!(
            rtrack.fixed_lanes.is_some(),
            "multi-playlist track must enter fixed-lane mode"
        );
        let names = rtrack
            .lane_names
            .as_ref()
            .expect("lane names should be populated");
        assert_eq!(names.lane_count, 3);
        assert_eq!(
            names.lane_names,
            vec![
                "Vocal".to_string(),
                "Vocal.01".to_string(),
                "Vocal.02".to_string()
            ]
        );
        assert_eq!(
            rtrack.lane_record.as_ref().map(|r| r.comping_enabled_lane),
            Some(0)
        );

        // One item per playlist, distributed across lanes 0, 1, 2.
        let mut lanes: Vec<i32> = rtrack
            .items
            .iter()
            .map(|it| it.lane.expect("each item should carry a lane index"))
            .collect();
        lanes.sort_unstable();
        assert_eq!(lanes, vec![0, 1, 2]);

        // And it survives serialization: FIXEDLANES on the track, and each
        // item's lane as the `YPOS` slice REAPER reads it back from.
        let rpp = project.to_rpp_string();
        assert!(
            rpp.contains("FIXEDLANES"),
            "RPP must carry FIXEDLANES token"
        );
        let mut emitted = item_lanes(&rpp);
        emitted.sort_unstable();
        assert_eq!(
            emitted,
            vec![0, 1, 2],
            "every item's lane must reach the RPP as YPOS geometry:\n{rpp}"
        );
    }

    /// Strongest verification short of a real PT fixture: serialize the
    /// synthetic 3-playlist project to raw .rpp TEXT, assert on the exact
    /// REAPER tokens, then re-parse that text back into REAPER domain types
    /// and confirm the fixed-lane structure survives a full round-trip
    /// (i.e. REAPER would load it).
    #[test]
    fn alternate_playlists_serialize_to_valid_rpp() {
        use dawfile_reaper::io::parse_project_text;

        let project = super::build_demo_playlist_project();
        let rpp = project.to_rpp_string();

        // ── Raw RPP text assertions ────────────────────────────────────────
        // Exactly one FIXEDLANES line on the (single) comp track.
        let fixedlanes: Vec<&str> = rpp
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("FIXEDLANES "))
            .collect();
        assert_eq!(
            fixedlanes.len(),
            1,
            "expected exactly one FIXEDLANES line, got {fixedlanes:?}"
        );

        // Exactly one LANENAME line, listing all three playlist names. REAPER
        // format: `LANENAME "name0" "name1" "name2"` (names only, no count).
        let lanename: Vec<&str> = rpp
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("LANENAME "))
            .collect();
        assert_eq!(
            lanename.len(),
            1,
            "expected exactly one LANENAME line, got {lanename:?}"
        );
        let ln = lanename[0];
        for name in ["Vocal", "Vocal.01", "Vocal.02"] {
            assert!(
                ln.contains(&format!("\"{name}\"")),
                "LANENAME line must list playlist {name:?}: {ln}"
            );
        }

        // Exactly one LANEREC line.
        let lanerec: Vec<&str> = rpp
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("LANEREC "))
            .collect();
        assert_eq!(
            lanerec.len(),
            1,
            "expected exactly one LANEREC line, got {lanerec:?}"
        );

        // One item per playlist per lane. The lane is not a token of its own:
        // three lanes means each item states `YPOS <lane/3> 0.333… 2`, and
        // REAPER divides that back out on read.
        let mut lanes = item_lanes(&rpp);
        lanes.sort_unstable();
        assert_eq!(
            lanes,
            vec![0, 1, 2],
            "expected exactly one item on each of lanes 0, 1 and 2:\n{rpp}"
        );

        // ── Re-parse the emitted RPP back into REAPER domain types ──────────
        // This proves the .rpp we emit is structurally valid and round-trips.
        let reparsed = parse_project_text(&rpp)
            .expect("emitted RPP must re-parse cleanly into REAPER domain types");

        let rtrack = reparsed
            .tracks
            .iter()
            .find(|t| t.fixed_lanes.is_some())
            .expect("re-parsed project must contain a fixed-lane track");

        assert!(
            rtrack.fixed_lanes.is_some(),
            "re-parsed track must still be in fixed-lane mode"
        );
        let names = rtrack
            .lane_names
            .as_ref()
            .expect("re-parsed track must carry lane names");
        assert_eq!(
            names.lane_names,
            vec![
                "Vocal".to_string(),
                "Vocal.01".to_string(),
                "Vocal.02".to_string(),
            ],
            "re-parsed lane names must match the three playlists"
        );
        assert!(
            rtrack.lane_record.is_some(),
            "re-parsed track must carry lane-record settings"
        );

        // Each re-parsed item carries its expected lane index (0, 1, 2).
        let mut lanes: Vec<i32> = rtrack
            .items
            .iter()
            .map(|it| it.lane.expect("re-parsed item must carry a lane index"))
            .collect();
        lanes.sort_unstable();
        assert_eq!(
            lanes,
            vec![0, 1, 2],
            "re-parsed items must land on lanes 0, 1, 2"
        );
    }

    #[test]
    fn single_playlist_track_has_no_lanes() {
        // A track with no alternate playlists must produce lane=None items and
        // no FIXEDLANES (no regression for the common case).
        let mut session = empty_session();
        session.audio_files.push(AudioFile {
            filename: "Take.wav".to_string(),
            index: 0,
            length: 48_000,
            source_uid: None,
        });
        session.audio_regions.push(mk_region(0, "Take-01", 48_000));

        let track = mk_track("Gtr", vec![mk_placement(0, 0)]);
        let resolve = |f: &str| f.to_string();

        let mut builder = ReaperProjectBuilder::new();
        builder = builder.track("Gtr", |t| {
            // has_lanes == false → active_lane None, no FIXEDLANES setters.
            emit_audio_playlist(
                t,
                &track.regions,
                None,
                &track,
                &session,
                48_000.0,
                &resolve,
            )
        });
        let project = builder.build();
        let rtrack = &project.tracks[0];
        assert!(rtrack.fixed_lanes.is_none());
        assert!(rtrack.lane_names.is_none());
        assert_eq!(rtrack.items.len(), 1);
        assert_eq!(rtrack.items[0].lane, None);
    }

    #[test]
    fn sibling_folders_round_trip_emits_isbus() {
        let rpp = protools_to_rpp(&fixture("folder-siblings.ptx")).expect("convert");
        let seq = track_isbus(&rpp);
        let get = |n: &str| {
            seq.iter()
                .find(|(name, _)| name == n)
                .map(|(_, i)| i.clone())
        };
        assert_eq!(get("F1"), Some(Some("1 1".to_string())));
        assert_eq!(get("leafA"), Some(None));
        assert_eq!(get("leafB"), Some(Some("2 -1".to_string())));
        assert_eq!(get("F2"), Some(Some("1 1".to_string())));
        assert_eq!(get("leafC"), Some(None));
        assert_eq!(get("leafD"), Some(Some("2 -1".to_string())));
        assert_eq!(get("leafTop"), Some(None));
    }
}
