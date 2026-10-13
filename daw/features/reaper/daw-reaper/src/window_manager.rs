//! REAPER implementation of the `WindowManager` service.
//!
//! Layouts are stored in our own JSON files (one per layout name) and
//! applied by driving REAPER's public dock/window APIs:
//! - `DockIsChildOfDock` reads the current state
//! - `DockWindowAddEx` / `Dock_UpdateDockID` moves windows between dockers
//! - `SetWindowPos` (via SWELL) positions floating windows
//! - The "Toolbar: Open/close floating toolbar N" action shows/hides toolbars
//!
//! We deliberately do **not** read or write REAPER's
//! `reaper-screensets.ini` — its per-window `poslist*_data` blobs are
//! produced by plugin-registered `screenset_register` callbacks and
//! aren't safe to round-trip from outside REAPER.

// `reaper-low` exposes `extern "C"` bindings via the `Reaper::get()`
// singleton; calling them does not require `unsafe` in current bindings,
// but historical call sites here still wrap them. Suppress the resulting
// `clippy::unnecessary_unsafe` / `not_unsafe_ptr_arg_deref` noise.
#![allow(unused_unsafe, clippy::not_unsafe_ptr_arg_deref, dead_code)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;

use daw_proto::window_manager::{
    WindowLayout, WindowLayoutOptions, WindowLayoutResult, WindowLayoutSummary, WindowManager,
};
use reaper_high::Reaper as HighReaper;
use reaper_low::Swell;
use reaper_low::raw;

// ─── Window enumeration ─────────────────────────────────────────────────────
//
// REAPER's floating toolbars and panels surface in one of two places:
// - As top-level windows (when free-floating)
// - As descendants of REAPER's main HWND (when docked or hosted in a
//   docker pane)
//
// `for_each_reaper_window` walks both so callers can find a toolbar by
// title regardless of its current dock state.

/// Read a window's title via SWELL. Returns empty string on failure.
fn window_text(hwnd: raw::HWND) -> String {
    if hwnd.is_null() {
        return String::new();
    }
    let mut buf = vec![0u8; 256];
    let written =
        unsafe { Swell::get().GetWindowText(hwnd, buf.as_mut_ptr() as *mut _, buf.len() as _) };
    if written <= 0 {
        return String::new();
    }
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..nul]).to_string()
}

fn for_each_top_level_window<F>(mut visit: F)
where
    F: FnMut(raw::HWND, String) -> bool,
{
    unsafe extern "C" fn cb<F>(hwnd: raw::HWND, lp: raw::LPARAM) -> raw::BOOL
    where
        F: FnMut(raw::HWND, String) -> bool,
    {
        let cb_ptr = lp as *mut F;
        let title = window_text(hwnd);
        if unsafe { (*cb_ptr)(hwnd, title) } {
            1
        } else {
            0
        }
    }
    unsafe {
        Swell::get().EnumWindows(Some(cb::<F>), &mut visit as *mut F as raw::LPARAM);
    }
}

fn for_each_child_window<F>(parent: raw::HWND, mut visit: F)
where
    F: FnMut(raw::HWND, String) -> bool,
{
    if parent.is_null() {
        return;
    }
    unsafe extern "C" fn cb<F>(hwnd: raw::HWND, lp: raw::LPARAM) -> raw::BOOL
    where
        F: FnMut(raw::HWND, String) -> bool,
    {
        let cb_ptr = lp as *mut F;
        let title = window_text(hwnd);
        if unsafe { (*cb_ptr)(hwnd, title) } {
            1
        } else {
            0
        }
    }
    unsafe {
        Swell::get().EnumChildWindows(parent, Some(cb::<F>), &mut visit as *mut F as raw::LPARAM);
    }
}

/// Walk top-level windows, direct children of REAPER's main HWND, and
/// children of any `REAPER_dock` container we find. Toolbars docked
/// in REAPER's standard docks surface as direct children of main;
/// toolbars attached to "top of main window" / "above arrange view"
/// instead live inside a `REAPER_dock` container two levels deep, so
/// we need a manual second hop. We don't fully recurse to keep the
/// log volume sane.
fn for_each_reaper_window<F>(mut visit: F)
where
    F: FnMut(raw::HWND, String) -> bool,
{
    let mut keep_going = true;
    for_each_top_level_window(|hwnd, title| {
        if !keep_going {
            return false;
        }
        keep_going = visit(hwnd, title);
        keep_going
    });
    if !keep_going {
        return;
    }
    let main = unsafe { reaper_low::Reaper::get().GetMainHwnd() };
    // Collect candidate dock containers first; we'll walk into them
    // after the top-level visit (need to avoid mutating `keep_going`
    // from inside one enumeration while issuing another).
    let mut docker_containers: Vec<raw::HWND> = Vec::new();
    for_each_child_window(main, |hwnd, title| {
        if !keep_going {
            return false;
        }
        if title == "REAPER_dock" {
            docker_containers.push(hwnd);
        }
        keep_going = visit(hwnd, title);
        keep_going
    });
    if !keep_going {
        return;
    }
    for container in docker_containers {
        for_each_child_window(container, |hwnd, title| {
            if !keep_going {
                return false;
            }
            keep_going = visit(hwnd, title);
            keep_going
        });
        if !keep_going {
            return;
        }
    }
}

/// Build a map of window title → HWND for every visible REAPER window.
/// Refresh per-call: HWNDs change when toolbars close and reopen.
pub fn toolbar_hwnds() -> HashMap<String, raw::HWND> {
    let mut out = HashMap::new();
    for_each_reaper_window(|hwnd, title| {
        if !title.is_empty() {
            // Top-level entries arrive first; keep them over child
            // duplicates so a floating toolbar wins over a docker-pane
            // child with the same title.
            out.entry(title).or_insert(hwnd);
        }
        true
    });
    out
}

// ─── Toolbar state queries ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockState {
    /// Window is docked in a fixed REAPER docker.
    Docked { docker_id: i32 },
    /// Window is docked into a free-positioned ("floating") docker frame.
    FloatingDocker { docker_id: i32 },
    /// Window is a regular floating window — not in any docker.
    Floating,
}

/// Resolve the dock state of any HWND via REAPER's `DockIsChildOfDock`.
pub fn current_dock_state(hwnd: raw::HWND) -> Option<DockState> {
    if hwnd.is_null() {
        return None;
    }
    let reaper = reaper_low::Reaper::get();
    let mut is_floating: bool = false;
    let dock_id = unsafe { reaper.DockIsChildOfDock(hwnd, &mut is_floating) };
    if dock_id < 0 {
        Some(DockState::Floating)
    } else if is_floating {
        Some(DockState::FloatingDocker { docker_id: dock_id })
    } else {
        Some(DockState::Docked { docker_id: dock_id })
    }
}

/// Whether the window is currently shown on screen.
pub fn is_window_visible(hwnd: raw::HWND) -> bool {
    if hwnd.is_null() {
        return false;
    }
    unsafe { Swell::get().IsWindowVisible(hwnd) }
}

/// Read a window's absolute screen rect via SWELL's `GetWindowRect`.
fn window_rect(hwnd: raw::HWND) -> Option<raw::RECT> {
    if hwnd.is_null() {
        return None;
    }
    let mut rect = raw::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe { Swell::get().GetWindowRect(hwnd, &mut rect) };
    if ok { Some(rect) } else { None }
}

/// Primary screen size in pixels via `GetSystemMetrics`. SM_CXSCREEN=0,
/// SM_CYSCREEN=1 (Windows + SWELL agree on these).
fn primary_screen_size() -> (i32, i32) {
    let swell = Swell::get();
    let w = swell.GetSystemMetrics(0);
    let h = swell.GetSystemMetrics(1);
    (w.max(1), h.max(1))
}

/// Match `<Mode> <slot>` titles like `Organize 1`. Used to filter
/// discovery results down to mode toolbars.
fn is_mode_toolbar_title(title: &str) -> bool {
    let Some((mode_word, slot_word)) = title.rsplit_once(' ') else {
        return false;
    };
    if slot_word.parse::<u32>().is_err() {
        return false;
    }
    matches!(
        mode_word,
        "Organize" | "Write" | "Produce" | "Record" | "Edit" | "Mix" | "Master" | "Live" | "Video"
    )
}

// ─── Toolbar identity + visibility actions ──────────────────────────────────
//
// REAPER assigns the 32 floating toolbar slots to three different
// `Toolbar: Open/close toolbar N` action ranges (decoded by probing the
// action list on REAPER 7.66):
//
//   slot  1..= 8 → cmd 41679..41686
//   slot  9..=16 → cmd 41936..41943
//   slot 17..=32 → cmd 42713..42728
//
// These actions are toggleable — `GetToggleCommandStateEx` returns 0/1
// for hidden/shown — so we can read state before firing to make the
// toggle idempotent.

/// Number of floating toolbars REAPER ships.
const REAPER_FLOATING_TOOLBAR_COUNT: u32 = 32;

/// Map a 1-based floating-toolbar slot to its `Toolbar: Open/close
/// toolbar N` REAPER command ID. Returns `None` for out-of-range slots.
fn toolbar_toggle_command_id(slot: u32) -> Option<u32> {
    match slot {
        1..=8 => Some(41678 + slot),
        9..=16 => Some(41927 + slot),
        17..=32 => Some(42696 + slot),
        _ => None,
    }
}

/// Decode a `<Mode> <N>` title into the 1-based floating-toolbar slot
/// that the renamer assigned at startup. Mode order matches
/// `Mode::ALL` in session::mode_actions; slot = mode_idx*3 + n.
fn slot_for_mode_toolbar_title(title: &str) -> Option<u32> {
    let (mode_word, slot_word) = title.rsplit_once(' ')?;
    let n: u32 = slot_word.parse().ok()?;
    if !(1..=3).contains(&n) {
        return None;
    }
    let mode_idx = match mode_word {
        "Organize" => 0,
        "Write" => 1,
        "Produce" => 2,
        "Record" => 3,
        "Edit" => 4,
        "Mix" => 5,
        "Master" => 6,
        "Live" => 7,
        "Video" => 8,
        // No "Minimal" — that mode has no toolbars.
        _ => return None,
    };
    Some((mode_idx as u32) * 3 + n)
}

/// Resolve a layout name to its 0-based mode index (`Organize` → 0,
/// ..., `Video` → 8). Case-sensitive — matches the names we register
/// via `mode_defs` exactly.
fn mode_index_for_layout_name(name: &str) -> Option<u32> {
    match name {
        "Organize" => Some(0),
        "Write" => Some(1),
        "Produce" => Some(2),
        "Record" => Some(3),
        "Edit" => Some(4),
        "Mix" => Some(5),
        "Master" => Some(6),
        "Live" => Some(7),
        "Video" => Some(8),
        "Minimal" => Some(9),
        _ => None,
    }
}

/// REAPER's native screenset window-set action IDs, verified from the
/// action list on REAPER 7.66:
/// - `40454 + N` = "Screenset: Load window set #(N+1)" for N in 0..=9
/// - `40474 + N` = "Screenset: Save window set #(N+1)" for N in 0..=9
const LOAD_WINDOW_SET_BASE_CMD: u32 = 40454;
const SAVE_WINDOW_SET_BASE_CMD: u32 = 40474;
const MAX_NATIVE_WINDOW_SETS: u32 = 10;

fn fire_main_command(cmd_id: u32) {
    // Use the high-level wrapper that resolves to `Main_OnCommandEx`
    // with an explicit `ProjectContext::CurrentProject`. The bare
    // `Main_OnCommand` we used before silently dropped screenset
    // load actions (saves worked, loads didn't) — REAPER's load path
    // appears to require a valid project context, which the
    // bare-call version doesn't pass.
    let reaper = HighReaper::get();
    reaper.medium_reaper().main_on_command_ex(
        reaper_medium::CommandId::new(cmd_id),
        0,
        reaper_medium::ProjectContext::CurrentProject,
    );
}

/// Build the full list of mode-toolbar titles in slot order
/// (`Organize 1` at slot 1, ..., `Video 3` at slot 27). Used to know
/// which toolbars are "mode managed" for hide-others-on-apply logic.
/// `Minimal` is intentionally excluded — it has no toolbars.
fn all_mode_toolbar_titles() -> Vec<(u32, String)> {
    let modes = [
        "Organize", "Write", "Produce", "Record", "Edit", "Mix", "Master", "Live", "Video",
    ];
    let mut out = Vec::with_capacity(modes.len() * 3);
    for (mode_idx, mode) in modes.iter().enumerate() {
        for n in 1..=3u32 {
            let slot = (mode_idx as u32) * 3 + n;
            out.push((slot, format!("{mode} {n}")));
        }
    }
    out
}

/// Is the toolbar at this slot currently visible? Reads REAPER's
/// toggle-command state for the matching open/close action.
fn is_toolbar_slot_visible(slot: u32) -> Option<bool> {
    let cmd = toolbar_toggle_command_id(slot)?;
    let state = unsafe { reaper_low::Reaper::get().GetToggleCommandStateEx(0, cmd as i32) };
    if state < 0 { None } else { Some(state != 0) }
}

/// Toggle a floating toolbar's visibility via REAPER's `Toolbar:
/// Open/close toolbar N` action. No-op when the toolbar is already in
/// the requested state.
fn set_toolbar_slot_visible(slot: u32, show: bool) -> Result<(), String> {
    let Some(cmd_id) = toolbar_toggle_command_id(slot) else {
        return Err(format!("toolbar slot {slot} out of range 1..=32"));
    };
    if let Some(current) = is_toolbar_slot_visible(slot)
        && current == show
    {
        return Ok(());
    }
    unsafe {
        reaper_low::Reaper::get().Main_OnCommand(cmd_id as i32, 0);
    }
    Ok(())
}

// ─── Mode docker layout config ──────────────────────────────────────────────
//
// REAPER's 16 dockers are user-configured to physical screen positions,
// so we can't infer top/left/right from the docker ID alone. The user
// names which docker ID corresponds to each position via a small JSON
// config at `<resource>/fasttrackstudio/mode_docker_layout.json`. The
// file is loaded fresh on each `apply_layout` call so edits take effect
// without a restart. Falls back to a `Default` instance when missing.

const MODE_DOCKER_LAYOUT_PATH: &str = "fasttrackstudio/mode_docker_layout.json";

fn load_mode_docker_layout() -> daw_proto::window_manager::ModeDockerLayout {
    use daw_proto::window_manager::ModeDockerLayout;

    // 1) Try the user's explicit override JSON.
    let resource = HighReaper::get().resource_path();
    let path = PathBuf::from(resource.as_str()).join(MODE_DOCKER_LAYOUT_PATH);
    if let Ok(contents) = std::fs::read_to_string(&path) {
        match facet_json::from_str::<ModeDockerLayout>(&contents) {
            Ok(layout) => return layout,
            Err(err) => tracing::warn!(
                path = %path.display(),
                error = %err,
                "mode_docker_layout.json failed to parse — falling back to auto-detect"
            ),
        }
    }

    // 2) Auto-detect: ask REAPER which docker is at each main-window
    // edge and use those IDs. Falls back to compile-time defaults when
    // no docker is attached to a position.
    auto_detect_mode_docker_layout()
}

/// REAPER's [`reaper_low::Reaper::DockGetPosition`] returns these
/// constants for a docker's main-window edge attachment.
///
/// Validated by probing on REAPER 7.66 — the values match REAPER's
/// public documentation: bottom=0, left=1, top=2, right=3.
mod docker_position {
    pub const BOTTOM: i32 = 0;
    pub const LEFT: i32 = 1;
    pub const TOP: i32 = 2;
    pub const RIGHT: i32 = 3;
}

/// Read each docker (0..=15) and resolve which one is currently
/// attached to the top, left, and right of the main window. Returns
/// defaults when a position is empty (no docker attached there).
fn auto_detect_mode_docker_layout() -> daw_proto::window_manager::ModeDockerLayout {
    use daw_proto::window_manager::ModeDockerLayout;

    let defaults = ModeDockerLayout::default();
    let reaper = reaper_low::Reaper::get();

    let mut top: Option<i32> = None;
    let mut left: Option<i32> = None;
    let mut right: Option<i32> = None;
    for docker_id in 0..16 {
        let pos = unsafe { reaper.DockGetPosition(docker_id) };
        match pos {
            docker_position::TOP if top.is_none() => top = Some(docker_id),
            docker_position::LEFT if left.is_none() => left = Some(docker_id),
            docker_position::RIGHT if right.is_none() => right = Some(docker_id),
            _ => {}
        }
    }

    let layout = ModeDockerLayout {
        top: top.unwrap_or(defaults.top),
        left: left.unwrap_or(defaults.left),
        right: right.unwrap_or(defaults.right),
    };
    tracing::info!(
        top_docker = layout.top,
        left_docker = layout.left,
        right_docker = layout.right,
        top_auto_detected = top.is_some(),
        left_auto_detected = left.is_some(),
        right_auto_detected = right.is_some(),
        "WindowManager: mode docker layout resolved"
    );
    layout
}

/// Render a `DockGetPosition` code as a short label for logs.
fn docker_position_label(pos: i32) -> &'static str {
    match pos {
        docker_position::BOTTOM => "bottom",
        docker_position::LEFT => "left",
        docker_position::TOP => "top",
        docker_position::RIGHT => "right",
        _ => "floating/unattached",
    }
}

/// Walk every REAPER window and group them by which docker they're
/// currently in (`DockIsChildOfDock`). Windows that aren't docked
/// anywhere are dropped. Used by [`log_docker_landscape`] to emit a
/// snapshot per mode switch.
fn windows_by_docker() -> HashMap<i32, Vec<(String, bool)>> {
    let mut map: HashMap<i32, Vec<(String, bool)>> = HashMap::new();
    for_each_reaper_window(|hwnd, title| {
        if title.is_empty() {
            return true;
        }
        let reaper = reaper_low::Reaper::get();
        let mut is_floating: bool = false;
        let dock_id = unsafe { reaper.DockIsChildOfDock(hwnd, &mut is_floating) };
        if dock_id >= 0 {
            map.entry(dock_id).or_default().push((title, is_floating));
        }
        true
    });
    map
}

/// Emit a per-docker info line summarising the current docker
/// landscape: each docker's position attachment, occupant count, and
/// occupant titles. Called at the start of every `apply_layout` so the
/// log carries a snapshot of REAPER's state immediately before the
/// mode switch reshuffles it.
fn log_docker_landscape(context: &str) {
    let reaper = reaper_low::Reaper::get();
    let windows = windows_by_docker();
    for docker_id in 0..16 {
        let pos = unsafe { reaper.DockGetPosition(docker_id) };
        let occupants = windows.get(&docker_id).cloned().unwrap_or_default();
        let titles: Vec<String> = occupants
            .iter()
            .map(|(t, floating)| {
                if *floating {
                    format!("{t} (floating)")
                } else {
                    t.clone()
                }
            })
            .collect();
        if pos < 0 && occupants.is_empty() {
            continue;
        }
        // Log raw codes (both decimal and hex) plus low-bit decoding so
        // the user can spot non-standard codes like "top of main view"
        // that may bit-pack additional state.
        tracing::info!(
            context = %context,
            docker_id,
            position_code = pos,
            position_hex = format!("{:#06x}", pos as u32),
            low_bits = pos & 0x3,
            high_bits = pos >> 2,
            position = %docker_position_label(pos),
            occupants = occupants.len(),
            titles = %titles.join(" | "),
            "Docker"
        );
    }
}

/// Diagnostic: log the position + occupants of every docker (0..=15)
/// for ad-hoc inspection. Same data as the per-mode-switch snapshot
/// but triggered on demand from the action list.
pub fn debug_dump_docker_positions() {
    log_docker_landscape("debug-action");
    // Also emit every window the recursive walker finds with its raw
    // `DockIsChildOfDock` result — including windows where the API
    // returns -1 (not a known dock child). Catches the case where a
    // toolbar lives somewhere our docker-filtered view drops it.
    let reaper = reaper_low::Reaper::get();
    for_each_reaper_window(|hwnd, title| {
        if title.is_empty() {
            return true;
        }
        let mut is_floating: bool = false;
        let dock_id = unsafe { reaper.DockIsChildOfDock(hwnd, &mut is_floating) };
        tracing::info!(
            title = %title,
            hwnd = ?(hwnd as *const c_void),
            dock_id,
            is_floating,
            "EnumWindow"
        );
        true
    });
}

/// Resolve the target docker ID for a mode-toolbar slot (1..=3) using
/// the user's configured top/left/right mapping.
fn docker_id_for_slot_position(
    slot_in_mode: u32,
    layout: &daw_proto::window_manager::ModeDockerLayout,
) -> Option<i32> {
    match slot_in_mode {
        1 => Some(layout.top),
        2 => Some(layout.left),
        3 => Some(layout.right),
        _ => None,
    }
}

/// SWP_NOZORDER flag — keep current Z-order, only move/resize.
const SWP_NOZORDER: i32 = 0x0004;
/// SWP_NOACTIVATE — don't steal focus while repositioning.
const SWP_NOACTIVATE: i32 = 0x0010;

/// Position a window to a monitor-relative rectangle. `rect` is
/// interpreted as fractions of the primary monitor. Used to place
/// floating windows (and floating docker frames) at predictable spots
/// regardless of the user's monitor size.
fn position_window_to_monitor_rect(hwnd: raw::HWND, rect: &daw_proto::window_manager::MonitorRect) {
    if hwnd.is_null() {
        return;
    }
    let (w, h) = primary_screen_size();
    let wf = w as f32;
    let hf = h as f32;
    let x = (rect.x_frac * wf).round() as i32;
    let y = (rect.y_frac * hf).round() as i32;
    let cx = (rect.w_frac * wf).round().max(1.0) as i32;
    let cy = (rect.h_frac * hf).round().max(1.0) as i32;
    unsafe {
        Swell::get().SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            cx,
            cy,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Detach a window from any docker it's in. The window remains valid
/// (REAPER turns it into a floating window) but stops occupying a tab
/// slot in whatever docker it was living in. Used on hidden mode
/// toolbars so they don't leave a stale tab header next to the active
/// mode's toolbar.
fn undock_window(hwnd: raw::HWND) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        reaper_low::Reaper::get().DockWindowRemove(hwnd);
    }
}

/// Force-dock a toolbar HWND into the given docker via REAPER's
/// `DockWindowAdd` (which accepts a numeric docker ID directly, unlike
/// `DockWindowAddEx` which keys off an ident string), then call
/// `DockWindowActivate` to bring the docker visible if it was hidden
/// and switch its active tab to this toolbar (so users see the
/// requested toolbar, not whichever sibling was previously active when
/// the docker hosts multiple windows). No-op on null HWND.
fn dock_window_to(hwnd: raw::HWND, title: &str, docker_id: i32) {
    if hwnd.is_null() {
        return;
    }
    let reaper = reaper_low::Reaper::get();
    let mut name_buf: Vec<u8> = title.as_bytes().to_vec();
    name_buf.push(0);
    unsafe {
        reaper.DockWindowAdd(hwnd, name_buf.as_ptr() as *const _, docker_id, true);
        // Ensures the docker is shown and this window's tab is active.
        // Without this a `DockWindowAdd` into a hidden docker leaves
        // the toolbar invisible until the user manually opens the
        // docker; calling Activate after Add is REAPER's documented
        // pattern for "I want this window in front, now".
        reaper.DockWindowActivate(hwnd);
    }
}

// ─── Storage (JSON files, one per layout) ───────────────────────────────────
//
// Layouts live under `<reaper_resource_path>/fasttrackstudio/layouts/`
// with one `<name>.json` per layout. Storing per-file keeps each layout
// independently editable, diffable, and avoids any contention with
// REAPER's own ini files. `facet_json` round-trips the same proto types
// the RPC surface uses.

const LAYOUTS_SUBDIR: &str = "fasttrackstudio/layouts";

fn layouts_dir() -> Option<PathBuf> {
    let resource = HighReaper::get().resource_path();
    Some(PathBuf::from(resource.as_str()).join(LAYOUTS_SUBDIR))
}

fn layout_path(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    // Strict whitelist to keep names safe as filesystem paths and as
    // future config keys: alphanumerics, space, dash, underscore.
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return None;
    }
    Some(layouts_dir()?.join(format!("{name}.json")))
}

fn ensure_layouts_dir() -> std::io::Result<PathBuf> {
    let dir = layouts_dir().ok_or_else(|| std::io::Error::other("no REAPER resource path"))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn load_layout_from_disk(name: &str) -> Option<WindowLayout> {
    let path = layout_path(name)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    facet_json::from_str::<WindowLayout>(&contents).ok()
}

fn write_layout_to_disk(layout: &WindowLayout) -> std::io::Result<()> {
    let path = layout_path(&layout.name).ok_or_else(|| {
        std::io::Error::other(format!(
            "layout name '{}' invalid (must be alnum/space/-/_)",
            layout.name
        ))
    })?;
    ensure_layouts_dir()?;
    let json = facet_json::to_string(layout)
        .map_err(|e| std::io::Error::other(format!("serialize layout: {e}")))?;
    std::fs::write(&path, json)
}

fn list_layouts_from_disk() -> Vec<WindowLayoutSummary> {
    let Some(dir) = layouts_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(layout) = facet_json::from_str::<WindowLayout>(&contents) else {
            tracing::warn!(
                path = %path.display(),
                "skipping unreadable FTS layout file"
            );
            continue;
        };
        out.push(summary_for(&layout));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn summary_for(layout: &WindowLayout) -> WindowLayoutSummary {
    WindowLayoutSummary {
        name: layout.name.clone(),
        description: layout.description.clone(),
        toolbar_count: layout.toolbars.len() as u32,
        action_count: layout.actions_on_apply.len() as u32,
    }
}

// ─── Diagnostics ────────────────────────────────────────────────────────────

pub fn debug_dump_top_level_windows() {
    let mut top_count = 0usize;
    for_each_top_level_window(|hwnd, title| {
        if !title.is_empty() {
            tracing::info!(
                hwnd = ?(hwnd as *const c_void),
                title = %title,
                scope = "top",
                "REAPER window"
            );
            top_count += 1;
        }
        true
    });
    let main = unsafe { reaper_low::Reaper::get().GetMainHwnd() };
    let mut child_count = 0usize;
    for_each_child_window(main, |hwnd, title| {
        if !title.is_empty() {
            tracing::info!(
                hwnd = ?(hwnd as *const c_void),
                title = %title,
                scope = "child-of-main",
                "REAPER window"
            );
            child_count += 1;
        }
        true
    });
    tracing::info!(top_count, child_count, "Window enumeration complete");
}

/// Diagnostic: scan REAPER command IDs in the toolbar action range and
/// log the human-readable name for each one. Finds the actual
/// "Toolbar: Open/close floating toolbar N" command IDs on the user's
/// REAPER build without us having to hardcode/guess them.
pub fn debug_log_toolbar_command_names() {
    use reaper_medium::SectionContext;

    let medium = HighReaper::get().medium_reaper();
    // First-8 toolbar open/close actions cluster around 41679..41686;
    // toolbars 9-32 are elsewhere in REAPER 7's action space. Scan
    // wider and log anything whose name contains "toolbar".
    let mut hits = 0usize;
    for cmd_id in 41600u32..43500u32 {
        let name: Option<String> = unsafe {
            medium.kbd_get_text_from_cmd(
                reaper_medium::CommandId::new(cmd_id),
                SectionContext::MainSection,
                |cstr| {
                    cstr.as_c_str()
                        .to_str()
                        .unwrap_or("(invalid utf-8)")
                        .to_string()
                },
            )
        };
        if let Some(name) = name
            && name.to_lowercase().contains("toolbar")
        {
            tracing::info!(command_id = cmd_id, name = %name, "Toolbar action");
            hits += 1;
        }
    }
    tracing::info!(hits, "Toolbar command-id probe complete");
}

/// Force every mode toolbar slot (1..=24) to visible. Bypasses the
/// usual mode-based show/hide and just toggles each toolbar on if
/// it's currently off. Useful for one-shot "give me HWNDs for every
/// toolbar at once" workflows like a full attachment-state capture.
pub fn open_all_mode_toolbars() {
    let mut opened = 0usize;
    let mut already_open = 0usize;
    let mut errored = 0usize;
    for (slot, _title) in all_mode_toolbar_titles() {
        match is_toolbar_slot_visible(slot) {
            Some(true) => already_open += 1,
            _ => match set_toolbar_slot_visible(slot, true) {
                Ok(()) => opened += 1,
                Err(err) => {
                    tracing::warn!(slot, error = %err, "open-all toolbar toggle failed");
                    errored += 1;
                }
            },
        }
    }
    tracing::info!(
        opened,
        already_open,
        errored,
        "Open-all mode toolbars complete"
    );
}

/// Diagnostic: emit a deep state dump for every mode toolbar — visible
/// flag, dock_id, is_floating, and absolute screen rect. Lets us see
/// the *live* per-toolbar attachment REAPER tracks in memory (which
/// the saved `reaper.ini` only mirrors on shutdown). Use to capture
/// what a manually-positioned toolbar looks like so we can replicate
/// its state for siblings.
pub fn debug_dump_mode_toolbar_attachments() {
    let hwnds = toolbar_hwnds();
    let reaper = reaper_low::Reaper::get();
    for (slot, title) in all_mode_toolbar_titles() {
        let Some(&hwnd) = hwnds.get(&title) else {
            tracing::info!(
                slot,
                title = %title,
                "Mode toolbar attachment: HWND not found (likely hidden/never opened)"
            );
            continue;
        };
        let mut is_floating: bool = false;
        let dock_id = unsafe { reaper.DockIsChildOfDock(hwnd, &mut is_floating) };
        let visible = is_window_visible(hwnd);
        let rect = window_rect(hwnd).unwrap_or(raw::RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        });
        tracing::info!(
            slot,
            title = %title,
            visible,
            dock_id,
            is_floating,
            left = rect.left,
            top = rect.top,
            width = rect.right - rect.left,
            height = rect.bottom - rect.top,
            "Mode toolbar attachment"
        );
    }
}

pub fn debug_dump_toolbar_states() {
    let hwnds = toolbar_hwnds();
    let mut shown = 0usize;
    for (title, hwnd) in &hwnds {
        if !is_mode_toolbar_title(title) {
            continue;
        }
        let dock = current_dock_state(*hwnd);
        let visible = is_window_visible(*hwnd);
        tracing::info!(
            title = %title,
            hwnd = ?(*hwnd as *const c_void),
            visible,
            dock = ?dock,
            "Toolbar state"
        );
        shown += 1;
    }
    tracing::info!(toolbars = shown, "Toolbar state probe complete");
}

// ─── WindowManager service impl (stubbed) ───────────────────────────────────
//
// The persistent storage + apply/save algorithms land in follow-up tasks
// (#10, #11, #12). Returning explicit "not yet implemented" errors keeps
// the trait satisfied without pretending broken behaviour is success.

/// Last layout applied this process lifetime. REAPER doesn't expose
/// "current layout" through its API, so this is our memory of it.
static CURRENT_LAYOUT_NAME: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn record_current_layout(name: &str) {
    if let Ok(mut guard) = CURRENT_LAYOUT_NAME.lock() {
        *guard = Some(name.to_string());
    }
}

impl WindowManager for crate::Reaper {
    fn apply_layout(&self, name: String, _options: WindowLayoutOptions) -> WindowLayoutResult {
        // Apply = fire REAPER's native "Screenset: Load window set
        // #(mode_idx + 1)" action. REAPER itself restores the full
        // window state (toolbar positions including "top of main",
        // docker layouts, mixer flags, etc.) that the user set up
        // by hand and captured via `save_layout`.
        let Some(mode_idx) = mode_index_for_layout_name(&name) else {
            return WindowLayoutResult::error(format!(
                "no mode named '{name}' (expected Organize/Write/Produce/Record/Edit/Mix/Master/Live/Video/Minimal)"
            ));
        };
        if mode_idx >= MAX_NATIVE_WINDOW_SETS {
            return WindowLayoutResult::error(format!(
                "mode index {mode_idx} exceeds REAPER's {MAX_NATIVE_WINDOW_SETS} native window-set slots"
            ));
        }
        let cmd = LOAD_WINDOW_SET_BASE_CMD + mode_idx;
        log_docker_landscape(&format!("before-apply:{name}"));
        fire_main_command(cmd);
        record_current_layout(&name);
        tracing::info!(
            layout = %name,
            mode_idx,
            command_id = cmd,
            "WindowManager: native window-set load fired"
        );
        log_docker_landscape(&format!("after-apply:{name}"));
        WindowLayoutResult::ok(name)
    }

    fn list_layouts(&self) -> Vec<WindowLayoutSummary> {
        // We don't read REAPER's persisted screenset names here — the
        // 9 mode slots are derived from the renaming convention, not
        // an arbitrary list. Return a synthetic summary for each mode.
        let mut out = Vec::new();
        for (name, toolbar_count) in [
            ("Organize", 3),
            ("Write", 3),
            ("Produce", 3),
            ("Record", 3),
            ("Edit", 3),
            ("Mix", 3),
            ("Master", 3),
            ("Live", 3),
            ("Video", 3),
            ("Minimal", 0),
        ] {
            out.push(WindowLayoutSummary {
                name: name.to_string(),
                description: format!("Mode {name} — native REAPER window set"),
                toolbar_count,
                action_count: 0,
            });
        }
        out
    }

    fn current_layout(&self) -> Option<WindowLayoutSummary> {
        let name = CURRENT_LAYOUT_NAME.lock().ok()?.clone()?;
        Some(WindowLayoutSummary {
            name: name.clone(),
            description: format!("Mode {name} — native REAPER window set"),
            toolbar_count: 3,
            action_count: 0,
        })
    }

    fn get_layout(&self, name: String) -> Option<WindowLayout> {
        mode_index_for_layout_name(&name)?;
        Some(WindowLayout {
            name: name.clone(),
            description: format!("Mode {name} — native REAPER window set"),
            toolbars: Vec::new(),
            actions_on_apply: Vec::new(),
        })
    }

    fn save_layout(&self, layout: WindowLayout) -> WindowLayoutResult {
        // Save = fire REAPER's native "Screenset: Save window set
        // #(mode_idx + 1)" action. REAPER captures the current window
        // state — toolbar positions, dockers, mixer flags — directly
        // into `reaper-screensets.ini` slot N. Our `apply_layout` then
        // restores from that slot on next mode switch.
        let Some(mode_idx) = mode_index_for_layout_name(&layout.name) else {
            return WindowLayoutResult::error(format!(
                "no mode named '{}' (expected Organize/Write/Produce/Record/Edit/Mix/Master/Live/Video/Minimal)",
                layout.name
            ));
        };
        if mode_idx >= MAX_NATIVE_WINDOW_SETS {
            return WindowLayoutResult::error(format!(
                "mode index {mode_idx} exceeds REAPER's {MAX_NATIVE_WINDOW_SETS} native window-set slots"
            ));
        }
        let cmd = SAVE_WINDOW_SET_BASE_CMD + mode_idx;
        fire_main_command(cmd);
        tracing::info!(
            name = %layout.name,
            mode_idx,
            command_id = cmd,
            "WindowManager: native window-set save fired"
        );
        WindowLayoutResult::ok(layout.name)
    }

    fn delete_layout(&self, name: String) -> WindowLayoutResult {
        // REAPER's native window sets don't have a "clear slot"
        // action — slot contents are overwritten by the next save.
        // We expose this as an explicit error rather than pretending.
        WindowLayoutResult::error(format!(
            "delete not supported for native window sets — re-save layout '{name}' to overwrite"
        ))
    }
}
