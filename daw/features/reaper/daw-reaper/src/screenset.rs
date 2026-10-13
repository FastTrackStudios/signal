//! REAPER implementation of the FTS screenset service.

use std::ffi::CString;
use std::time::{SystemTime, UNIX_EPOCH};

use daw_proto::{
    CaptureScreensetRequest, Screenset, ScreensetKind, ScreensetMonitor, ScreensetOptions,
    ScreensetRect, ScreensetResult, ScreensetScope, ScreensetSelection, ScreensetSummary,
    ScreensetTrackVisibility, ScreensetWindow, Screensets,
};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use display_info::DisplayInfo;
use reaper_high::Reaper;
use reaper_low::raw;
use reaper_medium::{CommandId, ProjectContext};

use crate::safe_wrappers::ext_state as sw;

/// Stable id for REAPER's main window inside a captured screenset.
const MAIN_WINDOW_ID: &str = "reaper.main";

const SECTION_GLOBAL: &str = "FTS.Screensets";
const REGISTRY_KEY: &str = "registry";

/// REAPER-backed FTS screenset storage and realtime apply service.
#[derive(Clone, Default)]
pub struct ReaperScreenset;

impl ReaperScreenset {
    pub fn new() -> Self {
        Self
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn screenset_key(id: &str) -> String {
    format!("screenset:{id}")
}

fn summary_for(screenset: &Screenset) -> ScreensetSummary {
    ScreensetSummary {
        id: screenset.id.clone(),
        name: screenset.name.clone(),
        description: screenset.description.clone(),
        kind: screenset.kind,
        updated_at_unix: screenset.updated_at_unix,
        tags: screenset.tags.clone(),
        window_count: screenset.windows.len() as u32,
        monitor_count: screenset.monitors.len() as u32,
        track_visibility_count: screenset.track_visibility.len() as u32,
        selected_track_count: screenset
            .selection
            .as_ref()
            .map(|s| s.selected_track_guids.len() as u32)
            .unwrap_or(0),
        action_count: screenset.actions_on_apply.len() as u32,
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("screenset id cannot be empty".to_string());
    }
    if id.contains('\0') || id.contains(':') {
        return Err("screenset id cannot contain NUL or ':'".to_string());
    }
    Ok(())
}

fn cstring(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "screenset values cannot contain NUL bytes".to_string())
}

fn get_global_value(section: &str, key: &str) -> Option<String> {
    let section = cstring(section).ok()?;
    let key = cstring(key).ok()?;
    sw::get_ext_state(Reaper::get().medium_reaper().low(), &section, &key)
}

fn set_global_value(section: &str, key: &str, value: &str, persist: bool) -> Result<(), String> {
    let section = cstring(section)?;
    let key = cstring(key)?;
    let value = cstring(value)?;
    sw::set_ext_state(
        Reaper::get().medium_reaper().low(),
        &section,
        &key,
        &value,
        persist,
    );
    Ok(())
}

fn delete_global_value(section: &str, key: &str, persist: bool) -> Result<(), String> {
    let section = cstring(section)?;
    let key = cstring(key)?;
    sw::delete_ext_state(Reaper::get().medium_reaper().low(), &section, &key, persist);
    Ok(())
}

fn load_registry() -> Vec<ScreensetSummary> {
    get_global_value(SECTION_GLOBAL, REGISTRY_KEY)
        .and_then(|json| facet_json::from_str::<Vec<ScreensetSummary>>(&json).ok())
        .unwrap_or_default()
}

fn save_registry(registry: &[ScreensetSummary], persist: bool) -> Result<(), String> {
    let json = facet_json::to_string(registry)
        .map_err(|err| format!("encode screenset registry: {err}"))?;
    set_global_value(SECTION_GLOBAL, REGISTRY_KEY, &json, persist)
}

fn load_screenset(id: &str) -> Option<Screenset> {
    get_global_value(SECTION_GLOBAL, &screenset_key(id))
        .and_then(|json| facet_json::from_str::<Screenset>(&json).ok())
}

fn save_screenset_immediate(
    mut screenset: Screenset,
    options: ScreensetOptions,
) -> Result<String, String> {
    if options.scope != ScreensetScope::Global {
        return Err("project-scoped FTS screensets are not implemented yet".to_string());
    }
    screenset.id = screenset.id.trim().to_string();
    validate_id(&screenset.id)?;
    if screenset.name.trim().is_empty() {
        screenset.name = screenset.id.clone();
    }
    screenset.schema_version = 1;
    screenset.updated_at_unix = now_unix();

    let json = facet_json::to_string(&screenset)
        .map_err(|err| format!("encode screenset '{}': {err}", screenset.id))?;
    set_global_value(
        SECTION_GLOBAL,
        &screenset_key(&screenset.id),
        &json,
        options.persist,
    )?;

    let summary = summary_for(&screenset);
    let mut registry = load_registry();
    registry.retain(|row| row.id != summary.id);
    registry.push(summary);
    registry.sort_by(|a, b| a.id.cmp(&b.id));
    save_registry(&registry, options.persist)?;
    Ok(screenset.id)
}

/// Build a stable id for a monitor that survives reboots and dock-undock
/// events. We can't trust the OS-assigned monitor index because adding /
/// removing displays renumbers them. Combining the panel name with its
/// resolution is good enough for the laptop ↔ multi-monitor switch case.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn monitor_id(name: &str, width: u32, height: u32) -> String {
    let trimmed = name.trim();
    let safe = if trimmed.is_empty() {
        "display"
    } else {
        trimmed
    };
    format!("{safe}@{width}x{height}")
}

/// Capture the current host monitor topology.
///
/// `display-info` is cross-platform (Windows / macOS / Linux X11+Wayland).
/// On a host with no displays — e.g. headless CI — we return an empty
/// vec rather than failing, so the screenset itself still saves.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn capture_monitors() -> Vec<ScreensetMonitor> {
    let displays = match DisplayInfo::all() {
        Ok(displays) => displays,
        Err(err) => {
            tracing::warn!("display-info enumeration failed: {err}");
            return Vec::new();
        }
    };
    displays
        .into_iter()
        .map(|d| ScreensetMonitor {
            id: monitor_id(&d.name, d.width, d.height),
            name: d.name,
            x: d.x,
            y: d.y,
            width: d.width,
            height: d.height,
            scale: d.scale_factor,
            primary: d.is_primary,
        })
        .collect()
}

/// Mobile targets have no display-info backend (its wayland stack does
/// not even build there); an empty topology keeps screenset saves working.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn capture_monitors() -> Vec<ScreensetMonitor> {
    Vec::new()
}

/// Read the bounds of an HWND via REAPER's SWELL-compatible `GetWindowRect`.
/// Returns `None` when SWELL isn't loaded or the call fails.
unsafe fn window_rect(hwnd: raw::HWND) -> Option<ScreensetRect> {
    if !reaper_low::Swell::is_available_globally() {
        tracing::debug!("screenset: SWELL unavailable, skipping window_rect");
        return None;
    }
    let mut r = raw::RECT::default();
    let ok = unsafe { reaper_low::Swell::get().GetWindowRect(hwnd, &mut r) };
    if !ok {
        tracing::debug!("screenset: GetWindowRect returned false for hwnd {hwnd:?}");
        return None;
    }
    let width = (r.right - r.left).max(0) as u32;
    let height = (r.bottom - r.top).max(0) as u32;
    Some(ScreensetRect {
        x: r.left,
        y: r.top,
        width,
        height,
    })
}

/// Pick the monitor that contains a window's center point, falling back to
/// the primary display if the window is wholly off-screen. Returns the id
/// we stamped on the corresponding [`ScreensetMonitor`].
fn monitor_for(rect: &ScreensetRect, monitors: &[ScreensetMonitor]) -> Option<String> {
    if monitors.is_empty() {
        return None;
    }
    let cx = rect.x + (rect.width as i32) / 2;
    let cy = rect.y + (rect.height as i32) / 2;
    for m in monitors {
        let mx2 = m.x + m.width as i32;
        let my2 = m.y + m.height as i32;
        if cx >= m.x && cx < mx2 && cy >= m.y && cy < my2 {
            return Some(m.id.clone());
        }
    }
    monitors
        .iter()
        .find(|m| m.primary)
        .or_else(|| monitors.first())
        .map(|m| m.id.clone())
}

/// Capture REAPER's main window into a [`ScreensetWindow`].
fn capture_main_window(monitors: &[ScreensetMonitor]) -> Option<ScreensetWindow> {
    let hwnd = Reaper::get().medium_reaper().get_main_hwnd().as_ptr();
    if hwnd.is_null() {
        return None;
    }
    let bounds = unsafe { window_rect(hwnd) };
    let monitor_id = bounds.as_ref().and_then(|rect| monitor_for(rect, monitors));
    Some(ScreensetWindow {
        id: MAIN_WINDOW_ID.to_string(),
        title: "REAPER".to_string(),
        monitor_id,
        bounds,
        visible: true,
        // The main window is never docked into another container.
        dock_id: None,
        floating: false,
    })
}

/// Capture every track's TCP/MCP visibility for the current project.
fn capture_track_visibility() -> Vec<ScreensetTrackVisibility> {
    use reaper_medium::TrackArea;
    let project = Reaper::get().current_project();
    project
        .tracks()
        .map(|t| ScreensetTrackVisibility {
            guid: t.guid().to_string_with_braces(),
            name: t.name().map(|s| s.to_str().to_string()).unwrap_or_default(),
            visible_in_tcp: t.is_shown(TrackArea::Tcp),
            visible_in_mixer: t.is_shown(TrackArea::Mcp),
        })
        .collect()
}

/// Restore TCP/MCP visibility from a captured map. Tracks not present in
/// the snapshot are left as-is (not silently hidden).
fn apply_track_visibility(rows: &[ScreensetTrackVisibility]) {
    use reaper_medium::TrackArea;
    let project = Reaper::get().current_project();
    let by_guid: std::collections::HashMap<&str, &ScreensetTrackVisibility> =
        rows.iter().map(|r| (r.guid.as_str(), r)).collect();
    for track in project.tracks() {
        let guid = track.guid().to_string_with_braces();
        let Some(row) = by_guid.get(guid.as_str()) else {
            continue;
        };
        track.set_shown(TrackArea::Tcp, row.visible_in_tcp);
        track.set_shown(TrackArea::Mcp, row.visible_in_mixer);
    }
}

/// Capture the current track selection and project loop / time selection.
fn capture_selection() -> ScreensetSelection {
    let project = Reaper::get().current_project();
    let selected: Vec<String> = project
        .tracks()
        .filter(|t| t.is_selected())
        .map(|t| t.guid().to_string_with_braces())
        .collect();
    let medium = Reaper::get().medium_reaper();
    let mut start = 0.0_f64;
    let mut end = 0.0_f64;
    unsafe {
        medium.low().GetSet_LoopTimeRange2(
            ProjectContext::CurrentProject.to_raw(),
            false, // is_set
            false, // is_loop
            &mut start,
            &mut end,
            false, // allow_autoseek_beats
        );
    }
    ScreensetSelection {
        selected_track_guids: selected,
        time_start_seconds: start,
        time_end_seconds: end,
    }
}

/// Restore track selection and project time selection from a captured
/// [`ScreensetSelection`]. Tracks not in the selection list are deselected.
fn apply_selection(selection: &ScreensetSelection) {
    let project = Reaper::get().current_project();
    let wanted: std::collections::HashSet<&str> = selection
        .selected_track_guids
        .iter()
        .map(|s| s.as_str())
        .collect();
    for track in project.tracks() {
        let guid = track.guid().to_string_with_braces();
        if wanted.contains(guid.as_str()) {
            track.select();
        } else {
            track.unselect();
        }
    }
    let medium = Reaper::get().medium_reaper();
    let mut start = selection.time_start_seconds;
    let mut end = selection.time_end_seconds;
    unsafe {
        medium.low().GetSet_LoopTimeRange2(
            ProjectContext::CurrentProject.to_raw(),
            true,  // is_set
            false, // is_loop
            &mut start,
            &mut end,
            false,
        );
    }
}

/// Apply a window's stored geometry. Currently we only restore position +
/// size for the REAPER main window; per-panel placement requires the dock
/// host adapter (handled separately via `dock_layout_blob`).
fn apply_main_window(window: &ScreensetWindow) -> Result<(), String> {
    if window.id != MAIN_WINDOW_ID {
        return Ok(());
    }
    let Some(rect) = window.bounds else {
        return Ok(());
    };
    let hwnd = Reaper::get().medium_reaper().get_main_hwnd().as_ptr();
    if hwnd.is_null() {
        return Err("main window unavailable".to_string());
    }
    if !reaper_low::Swell::is_available_globally() {
        return Err("SWELL unavailable in this REAPER build".to_string());
    }
    // SWP_NOZORDER (0x4) | SWP_NOACTIVATE (0x10): keep current Z-order
    // and don't steal focus from the foreground app while we're restoring.
    const FLAGS: i32 = 0x4 | 0x10;
    unsafe {
        reaper_low::Swell::get().SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            rect.x,
            rect.y,
            rect.width as i32,
            rect.height as i32,
            FLAGS,
        );
    }
    Ok(())
}

fn resolve_command_id(command: &str) -> Result<CommandId, String> {
    if let Ok(command_id) = command.parse::<u32>() {
        return Ok(CommandId::new(command_id));
    }
    Reaper::get()
        .action_by_command_name(command)
        .command_id()
        .map_err(|err| format!("action not found '{command}': {err}"))
}

fn apply_screenset_immediate(id: &str) -> Result<String, String> {
    validate_id(id)?;
    let screenset = load_screenset(id).ok_or_else(|| format!("screenset not found: {id}"))?;
    // Restore the per-kind state first so any post-apply actions
    // (e.g. "show mixer") observe the new layout/visibility.
    match screenset.kind {
        ScreensetKind::Window => {
            for window in &screenset.windows {
                if let Err(err) = apply_main_window(window) {
                    tracing::warn!("apply window '{}': {err}", window.id);
                }
            }
        }
        ScreensetKind::TrackSet => {
            apply_track_visibility(&screenset.track_visibility);
        }
        ScreensetKind::SelectionSet => {
            if let Some(selection) = &screenset.selection {
                apply_selection(selection);
            }
        }
    }
    let reaper = Reaper::get().medium_reaper();
    for action in &screenset.actions_on_apply {
        let command_id = resolve_command_id(action)?;
        reaper.main_on_command_ex(command_id, 0, ProjectContext::CurrentProject);
    }
    set_global_value(SECTION_GLOBAL, "active", &screenset.id, true)?;
    Ok(screenset.id)
}

impl Screensets for crate::Reaper {
    fn capture(&self, request: CaptureScreensetRequest) -> ScreensetResult {
        let mut screenset = Screenset {
            id: request.id,
            name: request.name,
            description: request.description,
            kind: request.kind,
            schema_version: 1,
            updated_at_unix: now_unix(),
            tags: request.tags,
            actions_on_apply: request.actions_on_apply,
            ..Default::default()
        };
        match request.kind {
            ScreensetKind::Window => {
                let monitors = capture_monitors();
                let mut windows = Vec::new();
                if let Some(main) = capture_main_window(&monitors) {
                    windows.push(main);
                }
                screenset.monitors = monitors;
                screenset.windows = windows;
            }
            ScreensetKind::TrackSet => {
                screenset.track_visibility = capture_track_visibility();
            }
            ScreensetKind::SelectionSet => {
                screenset.selection = Some(capture_selection());
            }
        }
        match save_screenset_immediate(screenset, request.options) {
            Ok(id) => ScreensetResult::ok(id),
            Err(err) => ScreensetResult::error(err),
        }
    }

    fn save(&self, screenset: Screenset, options: ScreensetOptions) -> ScreensetResult {
        match save_screenset_immediate(screenset, options) {
            Ok(id) => ScreensetResult::ok(id),
            Err(err) => ScreensetResult::error(err),
        }
    }

    fn list(&self, options: ScreensetOptions) -> Vec<ScreensetSummary> {
        if options.scope != ScreensetScope::Global {
            return Vec::new();
        }
        load_registry()
    }

    fn get(&self, id: &str, options: ScreensetOptions) -> Option<Screenset> {
        if options.scope != ScreensetScope::Global || validate_id(id).is_err() {
            return None;
        }
        load_screenset(id)
    }

    fn apply(&self, id: &str, options: ScreensetOptions) -> ScreensetResult {
        if options.scope != ScreensetScope::Global {
            return ScreensetResult::error("project-scoped FTS screensets are not implemented yet");
        }
        match apply_screenset_immediate(id) {
            Ok(id) => ScreensetResult::ok(id),
            Err(err) => ScreensetResult::error(err),
        }
    }

    fn delete(&self, id: &str, options: ScreensetOptions) -> ScreensetResult {
        if options.scope != ScreensetScope::Global {
            return ScreensetResult::error("project-scoped FTS screensets are not implemented yet");
        }
        if let Err(err) = validate_id(id) {
            return ScreensetResult::error(err);
        }
        if let Err(err) = delete_global_value(SECTION_GLOBAL, &screenset_key(id), options.persist) {
            return ScreensetResult::error(err);
        }
        let mut registry = load_registry();
        registry.retain(|row| row.id != id);
        if let Err(err) = save_registry(&registry, options.persist) {
            return ScreensetResult::error(err);
        }
        ScreensetResult::ok(id.to_string())
    }
}
