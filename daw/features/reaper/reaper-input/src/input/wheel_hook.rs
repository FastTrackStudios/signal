//! Mouse Event Hook
//!
//! Uses window procedure (WndProc) hooking to intercept mouse wheel and click events.
//! This is necessary because TranslateAccel only handles keyboard accelerators.
//!
//! Handles:
//! - WM_MOUSEWHEEL / WM_MOUSEHWHEEL - Mouse wheel events
//! - WM_LBUTTONDOWN / WM_RBUTTONDOWN / WM_MBUTTONDOWN - Mouse click events
//!
//! Logs mouse context on clicks and when context changes (on wheel events).

use crate::input::executor::{
    execute_action, execute_midi_editor_wheel_action, execute_wheel_action,
};
use crate::input::keybinds::{self, KeybindContext};
use crate::input::state::Context;
use crate::input::workflows;
use reaper_high::Reaper;
use reaper_low::Swell;
use reaper_low::raw::{
    GWL_WNDPROC, HWND, LPARAM, LRESULT, POINT, UINT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WPARAM,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use swell_ui::Window;
use tracing::{debug, info, warn};

/// Global state for whether wheel hook is installed
static WHEEL_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Convert input state Context to KeybindContext
fn context_to_keybind_context(context: Context) -> KeybindContext {
    match context {
        Context::Main => KeybindContext::Main,
        Context::Midi | Context::MidiEventListEditor => KeybindContext::Midi,
        Context::MidiInlineEditor => KeybindContext::MidiInline,
        Context::MediaExplorer => KeybindContext::MediaExplorer,
        Context::ExpressionEditor => KeybindContext::Custom(
            crate::input::window_detection::EXPRESSION_EDITOR_TAG.to_string(),
        ),
        Context::CrossfadeEditor | Context::Global => KeybindContext::Global,
    }
}

/// Build a modifier string from key state flags
/// On macOS: ctrl flag actually means Command (⌘)
fn build_modifier_string(ctrl: bool, shift: bool, alt: bool) -> String {
    let mut modifiers = Vec::new();

    // On macOS, the ctrl flag from key_states is actually Command
    #[cfg(target_os = "macos")]
    {
        if ctrl {
            modifiers.push("M"); // Command/Meta
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if ctrl {
            modifiers.push("C"); // Control
        }
    }

    if shift {
        modifiers.push("S");
    }

    if alt {
        modifiers.push("A");
    }

    if modifiers.is_empty() {
        String::new()
    } else {
        format!("<{}->", modifiers.join("-"))
    }
}

fn read_modifier_state_from_keyboard() -> (bool, bool, bool) {
    let swell = Swell::get();
    let is_down = |vk: i32| (swell.GetAsyncKeyState(vk) & 0x8000) != 0;

    let shift = is_down(16) || is_down(160) || is_down(161);
    let alt = is_down(18) || is_down(164) || is_down(165);
    let ctrl = is_down(17) || is_down(162) || is_down(163);

    (ctrl, shift, alt)
}

// Storage for original window procedures.
thread_local! {
    static ORIGINAL_PROCS: RefCell<HashMap<HWND, unsafe extern "C" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT>> = RefCell::new(HashMap::new());
    // Track which windows we've already hooked.
    static HOOKED_WINDOWS: RefCell<HashSet<HWND>> = RefCell::new(HashSet::new());
    /// Track previous mouse context to detect changes
    static PREVIOUS_CONTEXT: RefCell<Option<(crate::input::state::Context, String)>> = const { RefCell::new(None) };
}

/// Determine context from a specific HWND (used for mouse position-based detection)
#[allow(dead_code)]
fn determine_context_from_hwnd(
    hwnd: HWND,
    medium_reaper: &reaper_medium::Reaper,
) -> (crate::input::state::Context, String, String) {
    use crate::input::state::Context;

    // Try to create a Window from the HWND
    let window = if let Some(w) = Window::new(hwnd) {
        w
    } else {
        // Fallback to keyboard focus context
        return crate::input::handler::InputHandler::determine_context();
    };

    let mut found_window_title = String::new();

    // Check if this is the main window
    let main_hwnd = medium_reaper.get_main_hwnd();
    if window.raw_hwnd().as_ptr() == main_hwnd.as_ptr() {
        if let Ok(title) = window.text() {
            found_window_title = title.clone();
        }
        return (Context::Main, "Main".to_string(), found_window_title);
    }

    // Check if this window or any of its parents is a MIDI editor window
    if let Some(midi_editor_hwnd) = medium_reaper.midi_editor_get_active() {
        let mut current = Some(window);
        while let Some(w) = current {
            if w.raw_hwnd().as_ptr() == midi_editor_hwnd.as_ptr() {
                if let Ok(title) = w.text() {
                    found_window_title = title.clone();
                }
                // Check MIDI editor mode to distinguish between piano roll and event list
                let mode = unsafe {
                    medium_reaper
                        .low()
                        .MIDIEditor_GetMode(midi_editor_hwnd.as_ptr())
                };
                match mode {
                    0 => {
                        return (
                            Context::Midi,
                            "MIDI Editor (Piano Roll)".to_string(),
                            found_window_title,
                        );
                    }
                    1 => {
                        return (
                            Context::MidiEventListEditor,
                            "MIDI Event List Editor".to_string(),
                            found_window_title,
                        );
                    }
                    _ => return (Context::Midi, "MIDI Editor".to_string(), found_window_title),
                }
            }
            current = w.parent();
        }
    }

    // Also check if the mouse window itself matches the MIDI editor (direct match)
    if let Some(midi_editor_hwnd) = medium_reaper.midi_editor_get_active()
        && hwnd == midi_editor_hwnd.as_ptr()
    {
        if let Ok(title) = window.text() {
            found_window_title = title.clone();
        }
        let mode = unsafe {
            medium_reaper
                .low()
                .MIDIEditor_GetMode(midi_editor_hwnd.as_ptr())
        };
        match mode {
            0 => {
                return (
                    Context::Midi,
                    "MIDI Editor (Piano Roll)".to_string(),
                    found_window_title,
                );
            }
            1 => {
                return (
                    Context::MidiEventListEditor,
                    "MIDI Event List Editor".to_string(),
                    found_window_title,
                );
            }
            _ => return (Context::Midi, "MIDI Editor".to_string(), found_window_title),
        }
    }

    // Check window title for Media Explorer
    if let Ok(title) = window.text() {
        found_window_title = title.clone();
        let title_lower = title.to_lowercase();
        if title_lower.contains("media explorer") || title_lower.contains("mediaexplorer") {
            return (
                Context::MediaExplorer,
                "Media Explorer".to_string(),
                found_window_title,
            );
        }
        if title_lower.contains("crossfade") && title_lower.contains("editor") {
            return (
                Context::CrossfadeEditor,
                "Crossfade Editor".to_string(),
                found_window_title,
            );
        }
    }

    // Check parent windows
    let mut current = window.parent();
    while let Some(w) = current {
        if let Ok(title) = w.text() {
            if found_window_title.is_empty() {
                found_window_title = title.clone();
            }
            let title_lower = title.to_lowercase();

            if title_lower.contains("media explorer") || title_lower.contains("mediaexplorer") {
                return (
                    Context::MediaExplorer,
                    "Media Explorer".to_string(),
                    found_window_title,
                );
            }
            if title_lower.contains("crossfade") && title_lower.contains("editor") {
                return (
                    Context::CrossfadeEditor,
                    "Crossfade Editor".to_string(),
                    found_window_title,
                );
            }
            // Check for inline editor
            if (title_lower.contains("inline") || title_lower.contains("midi inline"))
                && (title_lower.contains("midi") || title_lower.contains("editor"))
            {
                return (
                    Context::MidiInlineEditor,
                    "MIDI Inline Editor".to_string(),
                    found_window_title,
                );
            }
        }
        current = w.parent();
    }

    // Default to Main if we can't determine
    (Context::Main, "Main".to_string(), found_window_title)
}

/// Log mouse context (called on clicks and context changes)
fn log_mouse_context(
    context: crate::input::state::Context,
    context_name: &str,
    _window_title: &str,
    event_type: &str,
) {
    let _reaper = Reaper::get();

    // Check if context changed
    let context_changed = PREVIOUS_CONTEXT.with(|prev| {
        let mut prev_borrow = prev.borrow_mut();
        let changed = match prev_borrow.as_ref() {
            Some((prev_ctx, prev_name)) => *prev_ctx != context || prev_name != context_name,
            None => true, // First time, always log
        };

        if changed {
            *prev_borrow = Some((context, context_name.to_string()));
        }
        changed
    });

    // Log if context changed or on every click
    if context_changed || event_type == "click" {
        // Don't log here - let the caller log with event details
    }
}

fn should_passthrough_for_text_input(hwnd: HWND) -> bool {
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    if let Some(target_window) = Window::new(hwnd) {
        let target_is_text =
            unsafe { medium_reaper.is_window_text_field(target_window.raw_hwnd()) };
        if target_is_text {
            return true;
        }
    }

    if let Some(focused) = Window::focused() {
        let focused_hwnd = focused.raw_hwnd();
        return unsafe { medium_reaper.is_window_text_field(focused_hwnd) };
    }

    false
}

/// Our custom window procedure that intercepts mouse wheel and click events
unsafe extern "C" fn wheel_hook_proc(hwnd: HWND, msg: UINT, w: WPARAM, l: LPARAM) -> LRESULT {
    // Check if interception is enabled
    if !crate::input::handler::InputHandler::is_enabled() {
        // Pass through to original procedure
        return unsafe { call_original_proc(hwnd, msg, w, l) };
    }

    // FTS-driven slip drag: once a slip is in progress we own the mouse (we
    // captured it on the intercepted mouse-down), so consume moves/up here and
    // drive the slip ourselves. This must run before the normal handlers below.
    if workflows::slip_drag::is_active() {
        match msg {
            WM_MOUSEMOVE => {
                workflows::slip_drag::on_move();
                return 0;
            }
            WM_LBUTTONUP => {
                workflows::slip_drag::on_up();
                return 0;
            }
            _ => {}
        }
    }

    // Handle mouse click events
    match msg {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            // Get mouse position from lParam (client coordinates)
            let x = l as i32 & 0xFFFF;
            let y = (l as i32 >> 16) & 0xFFFF;
            let pt = POINT { x, y };

            // Convert to screen coordinates for context detection
            let swell = Swell::get();
            let mut pt_screen = pt;
            unsafe {
                swell.ClientToScreen(hwnd, &mut pt_screen);
            }

            // Determine context from mouse position
            let reaper = Reaper::get();
            let medium_reaper = reaper.medium_reaper();
            let (context, context_name, _window_title) =
                crate::input::mouse_context::get_context_from_mouse_position(medium_reaper);

            // Log mouse click with context
            log_mouse_context(context, &context_name, &_window_title, "click");

            // Debug logging for mouse clicks
            // Check either unified debug logging OR dedicated mouse context debug
            // This allows testing mouse context detection independently
            let debug_mouse = crate::input::handler::InputHandler::is_debug_logging()
                || workflows::is_debug_mouse_context_enabled();

            if debug_mouse {
                let button = match msg {
                    WM_LBUTTONDOWN => "left",
                    WM_RBUTTONDOWN => "right",
                    WM_MBUTTONDOWN => "middle",
                    _ => "unknown",
                };
                // Use the new comprehensive context detection
                let mm_result = workflows::detect_context_at_point(pt_screen.x, pt_screen.y);
                crate::trace_console_msg(format!(
                    "[DEBUG] Click: {} | {} | pos: ({}, {}) | {}\n",
                    button, mm_result.context, pt_screen.x, pt_screen.y, mm_result.details
                ));
            }

            // Check if there's an active workflow with an armed click action
            // Only trigger on left-click
            if msg == WM_LBUTTONDOWN {
                // First, try the new armed click action system (preferred)
                if let Some(armed_click) = workflows::get_armed_click_action() {
                    // Check if mouse position matches any of the armed contexts
                    if armed_click.matches_position(pt_screen.x, pt_screen.y) {
                        debug!(
                            mode = %crate::current_mode_label(),
                            action = %armed_click.action,
                            context = %context_name,
                            mouse_x = pt_screen.x,
                            mouse_y = pt_screen.y,
                            "Executing armed click action from workflow"
                        );

                        // Execute the action
                        armed_click.execute();

                        // FTS-driven slip: after the action (split) runs, start
                        // our own slip-edit drag on the right piece and eat the
                        // click so REAPER's edge-detection never sees it.
                        if armed_click.slip_drag {
                            let started =
                                workflows::slip_drag::begin(hwnd, pt_screen.x, pt_screen.y);
                            debug!(started, "Armed click started FTS slip drag");
                            return 0;
                        }

                        // Check if we should pass through or eat the click
                        if armed_click.pass_through {
                            return unsafe { call_original_proc(hwnd, msg, w, l) };
                        }

                        // Eat the click - we handled it
                        return 0;
                    }
                }

                // Fallback: try the old click action system (for backwards compatibility)
                if context == Context::Main
                    && let Some(action_command) = workflows::get_click_action()
                {
                    debug!(
                        mode = %crate::current_mode_label(),
                        action = %action_command,
                        context = %context_name,
                        "Executing click action from workflow (legacy)"
                    );

                    // Execute the action
                    if let Err(e) = execute_action(&action_command) {
                        warn!(error = %e, action = %action_command, "Failed to execute click action");
                    } else if crate::input::handler::InputHandler::is_debug_logging() {
                        crate::trace_console_msg(format!(
                            "[DEBUG] Execute: click workflow action '{}' in {}\n",
                            action_command, context_name
                        ));
                    }

                    // Eat the click - we handled it
                    return 0;
                }
            }

            // Default behavior: pass clicks through so normal click/drag interactions work.
            // Specific workflow click actions above can still choose to consume.
            return unsafe { call_original_proc(hwnd, msg, w, l) };
        }
        _ => {}
    }

    // Handle mouse wheel events
    match msg {
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let delta = ((w as u32) >> 16) as i16;
            let is_horizontal = msg == WM_MOUSEHWHEEL;

            // Extract key states from wParam (low 16 bits)
            let key_states = (w as u32) & 0xFFFF;
            let ctrl_from_msg = (key_states & 0x0008) != 0; // MK_CONTROL
            let shift_from_msg = (key_states & 0x0004) != 0; // MK_SHIFT
            let alt_from_msg = (key_states & 0x0020) != 0; // MK_ALT
            let (ctrl_from_key, shift_from_key, alt_from_key) = read_modifier_state_from_keyboard();
            let ctrl = ctrl_from_msg || ctrl_from_key;
            let shift = shift_from_msg || shift_from_key;
            let alt = alt_from_msg || alt_from_key;

            // Determine context from mouse position (not keyboard focus)
            // Use the mouse context module which ports BR_MouseInfo::GetContext
            let reaper = Reaper::get();
            let medium_reaper = reaper.medium_reaper();
            let (context, context_name, _window_title) =
                crate::input::mouse_context::get_context_from_mouse_position(medium_reaper);

            // Log context change on wheel events
            log_mouse_context(context, &context_name, &_window_title, "wheel");

            // Build modifier string for keybind lookup (needed for both debug and binding resolution)
            let modifier_str = build_modifier_string(ctrl, shift, alt);

            // Debug logging for wheel events - unified with keyboard debug logging
            if crate::input::handler::InputHandler::is_debug_logging() {
                let direction = if delta > 0 { "up" } else { "down" };
                let wheel_type = if is_horizontal {
                    "horizontal"
                } else {
                    "vertical"
                };
                let mods = if modifier_str.is_empty() {
                    "none".to_string()
                } else {
                    modifier_str.clone()
                };
                crate::trace_console_msg(format!(
                    "[DEBUG] Wheel: {} {} in {} | delta: {} | modifiers: {}\n",
                    wheel_type, direction, context_name, delta, mods
                ));
            }

            // Check passthrough mode - if on, skip binding resolution and pass through
            if crate::input::handler::InputHandler::is_passthrough() {
                // Log wheel event in passthrough mode
                let direction = if delta > 0 { "up" } else { "down" };
                let wheel_type = if is_horizontal {
                    "horizontal wheel"
                } else {
                    "wheel"
                };
                crate::trace_console_msg(format!(
                    "Mouse {} {} in {} (passthrough)\n",
                    wheel_type, direction, context_name
                ));
                return unsafe { call_original_proc(hwnd, msg, w, l) };
            }

            // Convert context for keybind resolution
            let kb_context = context_to_keybind_context(context);

            // Try to resolve a wheel binding
            if let Some(action) =
                keybinds::resolve_wheel(kb_context, &modifier_str, is_horizontal, delta)
            {
                // Found a binding - execute the action with the wheel delta
                debug!(
                    action = %action,
                    delta = delta,
                    modifiers = %modifier_str,
                    context = ?context,
                    "Executing wheel binding"
                );
                if crate::input::handler::InputHandler::is_debug_logging() {
                    let mods = if modifier_str.is_empty() {
                        "none".to_string()
                    } else {
                        modifier_str.clone()
                    };
                    crate::trace_console_msg(format!(
                        "[DEBUG] Execute: wheel action '{}' in {} | delta: {} | modifiers: {}\n",
                        action, context_name, delta, mods
                    ));
                }

                // Use MIDI Editor executor for MIDI contexts (supports smooth scrolling)
                let result = match context {
                    Context::Midi | Context::MidiEventListEditor => {
                        execute_midi_editor_wheel_action(&action, delta)
                    }
                    _ => execute_wheel_action(&action, delta),
                };

                if let Err(e) = result {
                    warn!(error = %e, action = %action, context = ?context, "Failed to execute wheel action");
                    if crate::input::handler::InputHandler::is_debug_logging() {
                        crate::trace_console_msg(format!(
                            "[DEBUG] Execute FAILED: wheel action '{}' in {} | error: {}\n",
                            action, context_name, e
                        ));
                    }
                } else if crate::input::handler::InputHandler::is_debug_logging() {
                    crate::trace_console_msg(format!(
                        "[DEBUG] Execute OK: wheel action '{}' in {}\n",
                        action, context_name
                    ));
                }

                // Eat the message - we handled it
                return 0;
            }

            // No wheel binding found:
            // - passthrough ON or text input focused -> pass through
            // - intercept ON -> consume
            if crate::input::handler::InputHandler::is_passthrough()
                || should_passthrough_for_text_input(hwnd)
            {
                unsafe { call_original_proc(hwnd, msg, w, l) }
            } else {
                0
            }
        }
        _ => {
            // Pass all other messages through to original procedure
            unsafe { call_original_proc(hwnd, msg, w, l) }
        }
    }
}

/// Call the original window procedure
/// Uses try_with to avoid RefCell borrow panics if already borrowed
unsafe fn call_original_proc(hwnd: HWND, msg: UINT, w: WPARAM, l: LPARAM) -> LRESULT {
    // Use try_with to avoid panic if RefCell is already borrowed
    // This can happen if the timer callback is running simultaneously
    match ORIGINAL_PROCS.try_with(|orig_map| orig_map.borrow().get(&hwnd).cloned()) {
        Ok(Some(orig_fn)) => unsafe { orig_fn(hwnd, msg, w, l) },
        Ok(None) | Err(_) => {
            // Fallback to default window procedure if we can't get the original
            // or if RefCell is already borrowed
            unsafe { Swell::get().DefWindowProc(hwnd, msg, w, l) }
        }
    }
}

/// Install wheel event hook on a window
pub fn install_wheel_hook(hwnd: HWND) -> Result<(), Box<dyn std::error::Error>> {
    let swell = Swell::get();

    // Check if already hooked (use try_with to avoid panic if already borrowed)
    let _already_hooked = match HOOKED_WINDOWS.try_with(|hooked| hooked.borrow().contains(&hwnd)) {
        Ok(true) => return Ok(()),
        Ok(false) => false,
        Err(_) => {
            // If we can't check, assume not hooked and proceed
            // This is safe because we check again before inserting
            false
        }
    };

    // Use try_with to avoid RefCell borrow panic
    match ORIGINAL_PROCS.try_with(|m| {
        let mut map = m.borrow_mut();

        // Double-check we haven't already hooked this window
        if map.contains_key(&hwnd) {
            return Ok(());
        }

        unsafe {
            // Get the original window procedure
            let get_window_long = swell
                .pointers()
                .GetWindowLong
                .ok_or("GetWindowLong not available")?;
            let old_ptr = get_window_long(hwnd, GWL_WNDPROC);

            // Convert to function pointer
            let orig_fn: unsafe extern "C" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT =
                mem::transmute(old_ptr);

            // Store original procedure
            map.insert(hwnd, orig_fn);

            // Install our hook
            let set_window_long = swell
                .pointers()
                .SetWindowLong
                .ok_or("SetWindowLong not available")?;
            set_window_long(hwnd, GWL_WNDPROC, wheel_hook_proc as *const () as isize);
        }

        // Mark as hooked (use try_with to avoid panic)
        let _ = HOOKED_WINDOWS.try_with(|hooked| {
            hooked.borrow_mut().insert(hwnd);
        });

        Ok(())
    }) {
        Ok(result) => result,
        Err(_) => {
            // If RefCell is already borrowed, log and return error
            tracing::warn!("Could not install wheel hook: RefCell already borrowed");
            Err("RefCell already borrowed".into())
        }
    }
}

/// Install wheel hook on the main REAPER window
pub fn install_main_window_hook() -> Result<(), Box<dyn std::error::Error>> {
    if WHEEL_HOOK_INSTALLED.load(Ordering::Relaxed) {
        return Ok(());
    }

    let reaper = Reaper::get();
    let main_hwnd = reaper.medium_reaper().get_main_hwnd();

    // Convert Hwnd to raw HWND pointer
    install_wheel_hook(main_hwnd.as_ptr())?;

    WHEEL_HOOK_INSTALLED.store(true, Ordering::Relaxed);
    info!("Mouse wheel hook installed on main window");

    Ok(())
}

/// Check for and hook MIDI editor windows
/// Call this periodically to ensure all MIDI editor windows are hooked
pub fn check_and_hook_midi_editors() {
    if !crate::input::handler::InputHandler::is_enabled() {
        return;
    }

    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    // Check the active MIDI editor
    let Some(midi_editor_hwnd) = medium_reaper.midi_editor_get_active() else {
        return;
    };
    let frame = midi_editor_hwnd.as_ptr();

    // `WM_MOUSEWHEEL` in the piano roll is delivered to the notes-view /
    // piano-view CHILD windows, not the top-level MIDI editor frame — the same
    // reason the arrange view hooks its trackview child rather than the main
    // window. Hooking only the frame means scrolls in the piano roll never
    // reach us and fall through to REAPER's native handling. Hook the children
    // (plus the frame, for ruler/toolbar-area scrolls).
    let mut targets = vec![frame];
    if let Some(notes) = crate::input::reaper_windows::get_notes_view(frame, medium_reaper) {
        targets.push(notes);
    }
    if let Some(piano) = crate::input::reaper_windows::get_piano_view(frame, medium_reaper) {
        targets.push(piano);
    }

    for hwnd in targets {
        let already_hooked = HOOKED_WINDOWS.with(|hooked| hooked.borrow().contains(&hwnd));
        if already_hooked {
            continue;
        }
        if let Err(e) = install_wheel_hook(hwnd) {
            tracing::warn!("Failed to hook MIDI editor window: {}", e);
        } else {
            info!("Hooked MIDI editor view for wheel events");
            crate::trace_console_msg("🎹 Hooked MIDI editor view for wheel events\n");
        }
    }
}

/// Install wheel hook on the arrange view window
/// This is critical for click interception on items
pub fn install_arrange_view_hook() -> Result<(), Box<dyn std::error::Error>> {
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    // Get arrange view window using the reaper_windows helper
    if let Some(arrange_hwnd) = crate::input::reaper_windows::get_arrange_wnd(medium_reaper) {
        // Check if we've already hooked this window
        let already_hooked = HOOKED_WINDOWS
            .try_with(|hooked| hooked.borrow().contains(&arrange_hwnd))
            .unwrap_or(false);

        if !already_hooked {
            install_wheel_hook(arrange_hwnd)?;
            info!("Mouse hook installed on arrange view window");
        }
    } else {
        return Err("Arrange view window not found".into());
    }

    Ok(())
}

/// Check for and hook arrange view window (call periodically)
pub fn check_and_hook_arrange_view() {
    // Hook if either input handler is enabled OR debug mouse context is enabled
    let should_hook = crate::input::handler::InputHandler::is_enabled()
        || workflows::is_debug_mouse_context_enabled();

    if !should_hook {
        return;
    }

    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    // Get arrange view window
    if let Some(arrange_hwnd) = crate::input::reaper_windows::get_arrange_wnd(medium_reaper) {
        hook_window_if_needed(arrange_hwnd, "arrange view");
    }

    // Also hook the ruler
    if let Some(ruler_hwnd) = crate::input::reaper_windows::get_ruler_wnd(medium_reaper) {
        hook_window_if_needed(ruler_hwnd, "ruler");
    }

    // Also hook TCP if available
    let (tcp_hwnd, _) = crate::input::reaper_windows::get_tcp_wnd(medium_reaper);
    if let Some(tcp) = tcp_hwnd {
        hook_window_if_needed(tcp, "TCP");
    }

    // Hook the main REAPER window to catch clicks in other areas
    let main_hwnd = medium_reaper.get_main_hwnd();
    hook_window_if_needed(main_hwnd.as_ptr(), "main window");
}

/// Hook a window if not already hooked
fn hook_window_if_needed(hwnd: reaper_low::raw::HWND, name: &str) {
    let already_hooked = HOOKED_WINDOWS
        .try_with(|hooked| hooked.borrow().contains(&hwnd))
        .unwrap_or(false);

    if !already_hooked {
        if let Err(e) = install_wheel_hook(hwnd) {
            tracing::warn!("Failed to hook {} window: {}", name, e);
        } else {
            info!("Hooked {} window for mouse events", name);
        }
    }
}

/// Restore all window procedure hooks
pub fn restore_all_hooks() {
    let swell = Swell::get();

    ORIGINAL_PROCS.with(|m| {
        let mut map = m.borrow_mut();

        unsafe {
            if let Some(set_window_long) = swell.pointers().SetWindowLong {
                for (hwnd, orig_fn) in map.drain() {
                    set_window_long(hwnd, GWL_WNDPROC, orig_fn as usize as isize);
                }
            }
        }
    });

    HOOKED_WINDOWS.with(|hooked| {
        hooked.borrow_mut().clear();
    });

    WHEEL_HOOK_INSTALLED.store(false, Ordering::Relaxed);
    info!("All wheel hooks restored");
}
