//! Input Handler
//!
//! Main handler that processes keypresses and manages the input system.
//! Uses TranslateAccel to intercept keypresses before REAPER processes them.
//!
//! Key events flow through the `input::InputProcessor` state machine via the
//! `processor` module. The processor handles both single-key bindings and
//! multi-key sequences (including which-key prefix trees) natively.

use crate::input::keybinds::KeybindContext;
use crate::input::keybinds::which_key::WhichKeyEntry;
use crate::input::state::Context;
use crate::input::which_key_component::OverlayEntry;
use crate::input::window_detection;
use input::command::InputCommand;
use reaper_high::Reaper;
use reaper_low::raw;
use reaper_medium::{
    AccelMsgKind, AcceleratorBehavior, AcceleratorKeyCode, AcceleratorPosition, TranslateAccel,
    TranslateAccelArgs, TranslateAccelResult,
};
use std::ffi::CStr;
use swell_ui::Window;
use tracing::{debug, info};

/// Global state for whether FTS-Input interception is enabled
static INTERCEPTION_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Global state for whether FTS-Input should eat keys or just log them (passthrough mode)
static PASSTHROUGH_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Global state for debug logging mode (logs all key events to REAPER console)
static DEBUG_LOGGING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Global state for whether the handler is currently registered
/// When false, the handler is not registered at all (completely transparent)
static HANDLER_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Input handler that processes keyboard input via TranslateAccel
///
/// This intercepts keypresses BEFORE REAPER processes them, allowing us
/// to build key sequences similar to reaper-keys.
pub struct InputHandler {
    // Handler state
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InputHandler {
    pub fn new() -> Self {
        Self {}
    }

    #[cfg(not(target_os = "macos"))]
    fn ascii_punctuation_key(key_code: u32) -> Option<&'static str> {
        match key_code {
            33 => Some("!"),
            34 => Some("\""),
            35 => Some("#"),
            36 => Some("$"),
            37 => Some("%"),
            38 => Some("&"),
            39 => Some("'"),
            40 => Some("("),
            41 => Some(")"),
            42 => Some("*"),
            43 => Some("+"),
            44 => Some(","),
            45 => Some("-"),
            46 => Some("."),
            47 => Some("/"),
            58 => Some(":"),
            59 => Some(";"),
            60 => Some("<"),
            61 => Some("="),
            62 => Some(">"),
            63 => Some("?"),
            64 => Some("@"),
            91 => Some("["),
            92 => Some("\\"),
            93 => Some("]"),
            94 => Some("^"),
            95 => Some("_"),
            96 => Some("`"),
            123 => Some("{"),
            124 => Some("|"),
            125 => Some("}"),
            126 => Some("~"),
            _ => None,
        }
    }

    fn modifiers_to_string(behavior: &enumflags2::BitFlags<AcceleratorBehavior>) -> String {
        let mut modifiers = Vec::new();
        if behavior.contains(AcceleratorBehavior::Control) {
            modifiers.push("ctrl");
        }
        if behavior.contains(AcceleratorBehavior::Shift) {
            modifiers.push("shift");
        }
        if behavior.contains(AcceleratorBehavior::Alt) {
            modifiers.push("alt");
        }

        if modifiers.is_empty() {
            "none".to_string()
        } else {
            modifiers.join("+")
        }
    }

    fn runtime_state_string() -> String {
        format!(
            "profile={} preset={}",
            crate::current_profile()
                .map(|profile| profile.as_str())
                .unwrap_or("none"),
            crate::input::keybinds::active_preset_name(),
        )
    }

    /// Convert a key code and modifiers to a key string representation (for debug logging)
    fn key_to_string(
        key: AcceleratorKeyCode,
        behavior: &enumflags2::BitFlags<AcceleratorBehavior>,
    ) -> String {
        Self::key_to_string_with_flags(key, behavior, 0)
    }

    /// Like [`key_to_string`] but takes the raw SWELL lParam byte so the
    /// macOS physical Control key (FLWIN bit) renders as `<C-…>` instead
    /// of getting mistaken for `[` (key code 91 = both VK_LWIN and `[`).
    fn key_to_string_with_flags(
        key: AcceleratorKeyCode,
        behavior: &enumflags2::BitFlags<AcceleratorBehavior>,
        raw_flags: u8,
    ) -> String {
        const SWELL_FLWIN: u8 = 0x20;
        let key_code = key.get();
        #[cfg(target_os = "macos")]
        let rawctrl = (raw_flags & SWELL_FLWIN) != 0;

        // Check modifiers
        let ctrl = behavior.contains(AcceleratorBehavior::Control);
        let alt = behavior.contains(AcceleratorBehavior::Alt);
        let shift = behavior.contains(AcceleratorBehavior::Shift);

        // On macOS: Command (⌘) is reported as ctrl, so we map it to M (Meta).
        // Physical Ctrl (⌃) arrives via FLWIN (raw_flags), surfaced as a real
        // ctrl modifier here so `<CC-…>` bindings can render correctly.
        #[cfg(target_os = "macos")]
        let (cmd, ctrl_key) = (ctrl, rawctrl);
        #[cfg(not(target_os = "macos"))]
        let (cmd, ctrl_key) = (false, ctrl);

        #[cfg(target_os = "macos")]
        let key_str = {
            // Punctuation: macOS delivers the literal character (e.g. Shift+/
            // arrives as ASCII '?'). Emit the character verbatim so bindings
            // written as `keys "?"` / `keys "!"` / etc. match directly. The
            // OS-supplied shift flag is honored (it's normally false for
            // already-shifted punctuation, since the shift is "consumed" by
            // producing the character).
            if let Some(literal) = match key_code {
                44 => Some(","),
                46 => Some("."),
                47 => Some("/"),
                59 => Some(";"),
                39 => Some("'"),
                45 => Some("-"),
                61 => Some("="),
                // Codes 91/92 collide with VK_LWIN/VK_RWIN; treat them as
                // `[` / `\` only when no FLWIN bit was set (real bracket
                // press). With FLWIN, fall through to the modifier branch.
                91 if !rawctrl => Some("["),
                92 if !rawctrl => Some("\\"),
                93 => Some("]"),
                96 => Some("`"),
                33 => Some("!"),
                64 => Some("@"),
                35 => Some("#"),
                36 => Some("$"),
                37 => Some("%"),
                94 => Some("^"),
                38 => Some("&"),
                42 => Some("*"),
                40 => Some("("),
                41 => Some(")"),
                60 => Some("<"),
                62 => Some(">"),
                63 => Some("?"),
                58 => Some(":"),
                34 => Some("\""),
                95 => Some("_"),
                43 => Some("+"),
                123 => Some("{"),
                125 => Some("}"),
                124 => Some("|"),
                126 => Some("~"),
                _ => None,
            } {
                literal.to_string()
            } else {
                match key_code {
                    16 | 160 | 161 => return "shift".to_string(),
                    17 | 162 | 163 => return "cmd".to_string(),
                    18 | 164 | 165 => return "alt".to_string(),
                    91 => return "ctrl".to_string(),
                    92 => return "ctrl".to_string(),
                    65..=90 => char::from_u32((key_code + 32) as u32)
                        .unwrap_or('?')
                        .to_string(),
                    48..=57 => char::from_u32(key_code as u32).unwrap_or('?').to_string(),
                    8 => "backspace".to_string(),
                    9 => "tab".to_string(),
                    13 => "enter".to_string(),
                    27 => "esc".to_string(),
                    32 => "space".to_string(),
                    0x25 => "left".to_string(),
                    0x26 => "up".to_string(),
                    0x27 => "right".to_string(),
                    0x28 => "down".to_string(),
                    0x70..=0x7B => format!("f{}", key_code - 0x70 + 1),
                    0x21 => "pageup".to_string(),
                    0x22 => "pagedown".to_string(),
                    0x23 => "end".to_string(),
                    0x24 => "home".to_string(),
                    0x2D => "insert".to_string(),
                    0x2E => "delete".to_string(),
                    0xBA => ";".to_string(),
                    0xBB => "=".to_string(),
                    0xBC => ",".to_string(),
                    0xBD => "-".to_string(),
                    0xBE => ".".to_string(),
                    0xBF => "/".to_string(),
                    0xC0 => "`".to_string(),
                    0xDB => "[".to_string(),
                    0xDC => "\\".to_string(),
                    0xDD => "]".to_string(),
                    0xDE => "'".to_string(),
                    _ => format!("key{}", key_code),
                }
            }
        };

        #[cfg(not(target_os = "macos"))]
        let key_str = if let Some(actual) = Self::ascii_punctuation_key(key_code as u32) {
            actual.to_string()
        } else {
            match key_code {
                16 | 160 | 161 => return "shift".to_string(),
                17 | 162 | 163 => return "ctrl".to_string(),
                18 | 164 | 165 => return "alt".to_string(),
                91 => return "lmeta".to_string(),
                92 => return "rmeta".to_string(),
                65..=90 => char::from_u32((key_code + 32) as u32)
                    .unwrap_or('?')
                    .to_string(),
                48..=57 => char::from_u32(key_code as u32).unwrap_or('?').to_string(),
                8 => "backspace".to_string(),
                9 => "tab".to_string(),
                13 => "enter".to_string(),
                27 => "esc".to_string(),
                32 => "space".to_string(),
                0x25 => "left".to_string(),
                0x26 => "up".to_string(),
                0x27 => "right".to_string(),
                0x28 => "down".to_string(),
                0x70..=0x7B => format!("f{}", key_code - 0x70 + 1),
                0x21 => "pageup".to_string(),
                0x22 => "pagedown".to_string(),
                0x23 => "end".to_string(),
                0x24 => "home".to_string(),
                0x2D => "insert".to_string(),
                0x2E => "delete".to_string(),
                0xBA => ";".to_string(),
                0xBB => "=".to_string(),
                0xBC => ",".to_string(),
                0xBD => "-".to_string(),
                0xBE => ".".to_string(),
                0xBF => "/".to_string(),
                0xC0 => "`".to_string(),
                0xDB => "[".to_string(),
                0xDC => "\\".to_string(),
                0xDD => "]".to_string(),
                0xDE => "'".to_string(),
                _ => format!("key{}", key_code),
            }
        };

        let mut modifiers = Vec::new();
        if ctrl_key {
            modifiers.push("C");
        }
        if cmd {
            modifiers.push("M");
        }
        if shift {
            modifiers.push("S");
        }
        if alt {
            modifiers.push("A");
        }

        if modifiers.is_empty() {
            key_str
        } else {
            format!("<{}-{}>", modifiers.join("-"), key_str)
        }
    }

    /// Check if text input is currently focused
    fn is_text_focused() -> bool {
        if let Some(window) = Window::focused() {
            let hwnd = window.raw_hwnd();
            let reaper = Reaper::get();
            let medium_reaper = reaper.medium_reaper();
            // SAFETY: We got the HWND from Window::focused(), so it should be valid
            unsafe { medium_reaper.is_window_text_field(hwnd) }
        } else {
            false
        }
    }

    /// Determine context from current focused window
    /// Returns (Context, context_name, window_title)
    /// Made public so wheel_hook can use it
    pub fn determine_context() -> (Context, String, String) {
        let reaper = Reaper::get();
        let medium_reaper = reaper.medium_reaper();
        window_detection::detect_context_from_focus_compat(medium_reaper)
    }

    /// Context for a specific accelerator message.
    ///
    /// Prefers the message's own target HWND — that's the window REAPER is
    /// actually delivering this keystroke to, so it tracks click-driven focus
    /// changes (MIDI editor → arrange) that `GetFocus`-based polling can lag
    /// behind on SWELL/Linux. Falls back to focus detection when the message
    /// carries no HWND.
    ///
    /// Docked-editor disambiguation: a MIDI editor docked inside the main
    /// window receives its keys via the *main* HWND, so an hwnd result of
    /// `Main` is ambiguous whenever the active editor is docked. In that case
    /// the keyboard focus decides — but only then; for floating editors the
    /// hwnd is authoritative (trusting stale `GetFocus` there is what used to
    /// keep routing everything to the editor after clicking back into main).
    pub fn determine_context_for_msg(args: &TranslateAccelArgs) -> (Context, String, String) {
        let reaper = Reaper::get();
        let medium_reaper = reaper.medium_reaper();
        let hwnd = args.msg.raw().hwnd;
        if hwnd.is_null() {
            return window_detection::detect_context_from_focus_compat(medium_reaper);
        }

        let result = window_detection::detect_context_from_hwnd(hwnd, medium_reaper);
        if result.context != Context::Main {
            return (result.context, result.context_name, result.window_title);
        }

        let active_editor_docked = medium_reaper.midi_editor_get_active().is_some_and(|ed| {
            window_detection::is_window_child_of(
                ed.as_ptr(),
                medium_reaper.get_main_hwnd().as_ptr(),
            )
        });
        if active_editor_docked {
            let focus = window_detection::detect_context_from_focus(medium_reaper);
            if matches!(
                focus.context,
                Context::Midi | Context::MidiEventListEditor | Context::MidiInlineEditor
            ) {
                return (focus.context, focus.context_name, focus.window_title);
            }
        }

        (result.context, result.context_name, result.window_title)
    }
}

/// TranslateAccel implementation for intercepting keypresses
impl TranslateAccel for InputHandler {
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        // CRITICAL: If interception is disabled, return NotOurWindow IMMEDIATELY
        if !INTERCEPTION_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            return TranslateAccelResult::NotOurWindow;
        }

        // Some embedded hosts do not reliably drive the reaper-input timer.
        // The reload checker is internally throttled, so polling from input
        // events gives hot reload a dependable fallback without doing work on
        // every keypress.
        crate::check_config_reload();

        // Check the raw message type to detect mouse wheel and other events
        let raw_msg = args.msg.raw();
        let raw_message_type = raw_msg.message;
        let msg_type = args.msg.message();

        match msg_type {
            AccelMsgKind::KeyDown
            | AccelMsgKind::KeyUp
            | AccelMsgKind::SysKeyDown
            | AccelMsgKind::SysKeyUp
            | AccelMsgKind::Char => {
                // Normal keyboard events
            }
            _ => {
                crate::trace_console_msg(format!(
                    "FTS-Input: Non-keyboard message type: {:?} (raw: 0x{:X} = {})\n",
                    msg_type, raw_message_type, raw_message_type
                ));
            }
        }

        // Detect mouse wheel events
        if raw_message_type == raw::WM_MOUSEWHEEL || raw_message_type == raw::WM_MOUSEHWHEEL {
            return Self::handle_mouse_wheel(args, raw_message_type);
        }

        let msg_type = args.msg.message();

        // Handle key release for continuous actions
        if msg_type == AccelMsgKind::KeyUp || msg_type == AccelMsgKind::SysKeyUp {
            // Cache the raw lParam low byte once so the keyup/keydown paths
            // see the FLWIN bit (macOS physical Ctrl) — reaper-rs's
            // `AcceleratorBehavior` truncates it away otherwise.
            let raw_flags = (args.msg.raw().lParam as u32) as u8;
            if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                let key = args.msg.key();
                let behavior = args.msg.behavior();
                let key_str = Self::key_to_string_with_flags(key, &behavior, raw_flags);
                let (_context, context_name, _) = Self::determine_context();
                info!(
                    mode = %crate::current_mode_label(),
                    key = %key_str, raw = key.get(), context = %context_name,
                    ctrl = behavior.contains(AcceleratorBehavior::Control),
                    shift = behavior.contains(AcceleratorBehavior::Shift),
                    alt = behavior.contains(AcceleratorBehavior::Alt),
                    "[keyup]"
                );
                crate::trace_console_msg(format!(
                    "[DEBUG] KeyUp: '{}' (raw: {}) in {} | modifiers: {} | {}\n",
                    key_str,
                    key.get(),
                    context_name,
                    Self::modifiers_to_string(&behavior),
                    Self::runtime_state_string(),
                ));
            }
            // Notify the processor so it can drop the chord from `held_keys`
            // and tear down sticky which-key state when a prefix anchor is
            // released. Done before passthrough/text-focus checks so the
            // held-key bookkeeping stays consistent even if we forward the
            // event to REAPER.
            let should_hide_overlay = crate::input::processor::notify_key_release(
                args.msg.key(),
                &args.msg.behavior(),
                raw_flags,
            );
            if should_hide_overlay {
                crate::input::which_key_overlay::hide();
            }

            // Note: continuous actions (tempo grid) are now managed by fts-extensions.
            if PASSTHROUGH_MODE.load(std::sync::atomic::Ordering::Relaxed)
                || Self::is_text_focused()
            {
                return TranslateAccelResult::NotOurWindow;
            }
            // The expression editor tracks its own key state (momentary
            // keys like R release on keyup); its releases belong to the
            // panel, not to this hook.
            if Self::determine_context_for_msg(&args).0 == Context::ExpressionEditor {
                return TranslateAccelResult::PassOnToWindow;
            }
            return TranslateAccelResult::Eat;
        }

        // Only process KeyDown and SysKeyDown events
        if msg_type != AccelMsgKind::KeyDown && msg_type != AccelMsgKind::SysKeyDown {
            return TranslateAccelResult::NotOurWindow;
        }

        // Record activity so timeout-driven sequence expiry doesn't trip
        // while the user is auto-repeating a held prefix.
        crate::mark_input_activity();

        // If text is focused, always pass through
        if Self::is_text_focused() {
            return TranslateAccelResult::NotOurWindow;
        }

        let key = args.msg.key();
        let behavior = args.msg.behavior();
        // Raw bottom byte of lParam captures every SWELL flag, not just
        // the four reaper-rs exposes (FVIRTKEY=0x01, FSHIFT=0x04,
        // FCONTROL=0x08, FALT=0x10, FLWIN=0x20). FLWIN = macOS physical
        // Control; we surface it through the bridge as a real ctrl
        // modifier so `<CC-…>` bindings can match.
        let raw_flags = (args.msg.raw().lParam as u32) as u8;
        let key_str = Self::key_to_string_with_flags(key, &behavior, raw_flags);

        // Determine context and update the processor's context. Resolved from
        // this message's target HWND (not polled focus) so clicking back into
        // the main window reroutes immediately.
        let (context, context_name, window_title) = Self::determine_context_for_msg(&args);
        let keybind_context = Self::context_to_keybind_context(&context);

        // Debug logging
        if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
            info!(
                mode = %crate::current_mode_label(),
                key = %key_str, raw = key.get(), context = %context_name,
                window = %window_title,
                ctrl = behavior.contains(AcceleratorBehavior::Control),
                shift = behavior.contains(AcceleratorBehavior::Shift),
                alt = behavior.contains(AcceleratorBehavior::Alt),
                raw_flags = format!("0x{:02X}", raw_flags),
                "[keydown]"
            );
            crate::trace_console_msg(format!(
                "[DEBUG] KeyDown: '{}' (raw: {}) in {} | modifiers: {} | {}\n",
                key_str,
                key.get(),
                context_name,
                Self::modifiers_to_string(&behavior),
                Self::runtime_state_string(),
            ));
        }

        // === Cmd+W: Toggle which-key cheat sheet ===
        // Check for Meta+w before sending to processor
        {
            if key_str == "<M-w>" {
                if crate::input::which_key_overlay::is_visible() {
                    crate::input::which_key_overlay::hide();
                } else {
                    crate::input::which_key_overlay::show_all_prefixes();
                }
                return TranslateAccelResult::Eat;
            }

            // Esc dismisses the cheat sheet overlay if visible and no sequence is active
            if key_str == "esc"
                && crate::input::which_key_overlay::is_visible()
                && !crate::input::processor::needs_timeout()
            {
                crate::input::which_key_overlay::hide();
            }
        }

        if key_str == "esc" && crate::input::processor::needs_timeout() {
            if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed)
                && let Some(pending) = crate::input::processor::pending_display()
            {
                info!(mode = %crate::current_mode_label(), pending = %pending, "[pending:cancel]");
                crate::trace_console_msg(format!(
                    "[DEBUG] Pending cancelled: '{}' in {} | {}\n",
                    pending,
                    context_name,
                    Self::runtime_state_string()
                ));
            }

            crate::input::processor::clear_pending();
            if crate::input::which_key_overlay::is_visible() {
                crate::input::which_key_overlay::hide();
            }
            return TranslateAccelResult::Eat;
        }

        // === The expression editor is its own input surface ===
        // Only bindings declared for its context fire; everything else
        // goes to the panel itself (whose component has its own
        // handlers), and *never* on to REAPER's accelerator tables —
        // which is what makes typing at the editor safe from global
        // shortcuts.
        if context == Context::ExpressionEditor {
            let bound = crate::input::processor::resolve_exact_context(&keybind_context, &key_str);
            return match bound {
                Some(action_id) => {
                    let (clean, _) = classify_action(action_id.as_str());
                    if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                        info!(
                            mode = %crate::current_mode_label(),
                            action = %clean, key = %key_str,
                            "[expression-editor action]"
                        );
                    }
                    Self::execute_action(clean, false);
                    TranslateAccelResult::Eat
                }
                None => TranslateAccelResult::PassOnToWindow,
            };
        }

        // === Process key through InputProcessor ===
        // Update context before processing
        {
            let mut proc = crate::input::processor::get_processor().write().unwrap();
            proc.set_reaper_context(keybind_context);
        }

        let commands = crate::input::processor::process_key(key, &behavior, raw_flags);

        // Handle the commands from the processor
        let mut handled = false;
        for command in &commands {
            match command {
                InputCommand::Action(action_id) => {
                    let (clean, route) = classify_action(action_id.as_str());
                    // In an editor, a plain global binding is passed through to
                    // the editor's native handling rather than run in main.
                    if Self::should_passthrough_to_editor(context, route) {
                        if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                            crate::trace_console_msg(format!(
                                "[DEBUG] Passthrough: action '{}' to native {} (no @Midi binding)\n",
                                clean, context_name
                            ));
                        }
                        return TranslateAccelResult::NotOurWindow;
                    }
                    let midi_section = route == ActionRoute::Midi;
                    if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                        let section = if midi_section { "midi" } else { "main" };
                        info!(mode = %crate::current_mode_label(), action = %clean, section = %section, context = %context_name, "[action]");
                        crate::trace_console_msg(format!(
                            "[DEBUG] Execute: action '{}' [{} section] in {} | {}\n",
                            clean,
                            section,
                            context_name,
                            Self::runtime_state_string()
                        ));
                    }
                    Self::execute_action(clean, midi_section);
                    handled = true;
                }
                InputCommand::ActionWithArgs { action, args } => {
                    let (clean, route) = classify_action(action.as_str());
                    if Self::should_passthrough_to_editor(context, route) {
                        if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                            crate::trace_console_msg(format!(
                                "[DEBUG] Passthrough: action '{}' to native {} (no @Midi binding)\n",
                                clean, context_name
                            ));
                        }
                        return TranslateAccelResult::NotOurWindow;
                    }
                    let midi_section = route == ActionRoute::Midi;
                    if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                        let section = if midi_section { "midi" } else { "main" };
                        info!(mode = %crate::current_mode_label(), action = %clean, section = %section, count = ?args.count, context = %context_name, "[action]");
                        crate::trace_console_msg(format!(
                            "[DEBUG] Execute: action '{}' [{} section] (count={:?}) in {} | {}\n",
                            clean,
                            section,
                            args.count,
                            context_name,
                            Self::runtime_state_string()
                        ));
                    }
                    // Execute the action, potentially repeating for count.
                    // Hard-cap the repeat: stray digit prefixes (or count
                    // overflow — digits wrapped to u32::MAX once) must never
                    // turn one keypress into billions of synchronous action
                    // executions on the main thread.
                    const MAX_COUNT: u32 = 99;
                    let count = args.count.unwrap_or(1);
                    let capped = count.min(MAX_COUNT);
                    if capped != count {
                        tracing::warn!(
                            action = %clean,
                            count,
                            capped,
                            "[action] repeat count capped"
                        );
                    }
                    for _ in 0..capped {
                        Self::execute_action(clean, midi_section);
                    }
                    handled = true;
                }
                InputCommand::Pending {
                    display: pending_display,
                } => {
                    // Prefer which-key tree metadata so leaf labels are preserved.
                    let tree_display = normalize_which_key_display(pending_display);
                    if let Some(continuations) = which_key_continuations_for_display(&tree_display)
                    {
                        crate::input::which_key_overlay::show_entries(&tree_display, continuations);
                        if let Some(action) = which_key_branch_action_for_display(&tree_display) {
                            let (clean, route) = classify_action(&action);
                            Self::execute_action(clean, route == ActionRoute::Midi);
                        }
                    } else {
                        let proc = crate::input::processor::get_processor().read().unwrap();
                        if let Some(trie) = proc.normal_keytrie() {
                            let pending_chords = pending_display_to_chords(pending_display);
                            let continuations =
                                crate::input::keybinds::bridge::trie_continuations_at(
                                    trie,
                                    &pending_chords,
                                );
                            if !continuations.is_empty() {
                                crate::input::which_key_overlay::show(
                                    pending_display,
                                    &continuations,
                                );
                            }
                        }
                        drop(proc);
                    }

                    if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                        info!(mode = %crate::current_mode_label(), pending = %pending_display, "[pending]");
                        crate::trace_console_msg(format!(
                            "[DEBUG] Pending: '{}' in {} | {}\n",
                            pending_display,
                            context_name,
                            Self::runtime_state_string()
                        ));
                    }
                    handled = true;
                }
                InputCommand::Unhandled(_) => {
                    if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                        info!(mode = %crate::current_mode_label(), key = %key_str, context = %context_name, "[unhandled]");
                        crate::trace_console_msg(format!(
                            "[DEBUG] Unhandled: '{}' in {} | {}\n",
                            key_str,
                            context_name,
                            Self::runtime_state_string()
                        ));
                    }
                    // Hide overlay if visible
                    if crate::input::which_key_overlay::is_visible() {
                        crate::input::which_key_overlay::hide();
                    }
                    // Not handled — fall through
                }
                InputCommand::SwitchMode(mode) => {
                    if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                        info!(mode = %mode, "[mode:switch]");
                    }
                    handled = true;
                }
                InputCommand::PushMode(mode) => {
                    if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                        info!(mode = %mode, "[mode:push]");
                    }
                    handled = true;
                }
                InputCommand::PopMode => {
                    if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                        info!("[mode:pop]");
                    }
                    handled = true;
                }
                InputCommand::InsertText(_) => {
                    // In REAPER context, insert text isn't used
                    handled = true;
                }
            }
        }

        if !handled {
            let display_key_str = crate::input::keybinds::bridge::translate_sequence(&key_str);
            // Fast path: the overlay is already live for this prefix
            // (typical for OS keyboard auto-repeat). Don't rebuild the
            // continuations list or call into the overlay layer again.
            if crate::input::which_key_overlay::is_showing_sequence(&display_key_str) {
                handled = true;
            } else if let Some(continuations) =
                which_key_prefix_continuations(&display_key_str).as_ref()
            {
                crate::trace_console_msg(format!(
                    "[DEBUG] WhichKey prefix matched: '{}' entries={} | {}\n",
                    display_key_str,
                    continuations.len(),
                    Self::runtime_state_string()
                ));
                crate::input::which_key_overlay::show_entries(
                    &display_key_str,
                    continuations.clone(),
                );
                handled = true;

                if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                    info!(pending = %display_key_str, "[pending:which-key-tree]");
                    crate::trace_console_msg(format!(
                        "[DEBUG] Pending: '{}' in {} | {} | source=which-key-tree\n",
                        display_key_str,
                        context_name,
                        Self::runtime_state_string()
                    ));
                }
            } else if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed)
                && matches!(key_str.as_str(), "z" | "v" | "a" | "f" | "o")
            {
                crate::trace_console_msg(format!(
                    "[DEBUG] WhichKey prefix miss: '{}' | {}\n",
                    key_str,
                    Self::runtime_state_string()
                ));
            }
        }

        let pending_from_state = if !commands
            .iter()
            .any(|c| matches!(c, InputCommand::Pending { .. }))
        {
            crate::input::processor::pending_display()
        } else {
            None
        };

        if let Some(pending_display) = pending_from_state.as_deref() {
            let tree_display = normalize_which_key_display(pending_display);
            // Fast path: overlay is already live for this exact sequence —
            // skip the rebuild dance entirely. Auto-repeated keydowns of a
            // held prefix can churn through this fallback dozens of times
            // per second; even with the idempotent guard inside
            // `show_entries`, avoiding the extra borrow + entry construction
            // keeps the hold-overlay rock steady.
            if crate::input::which_key_overlay::is_showing_sequence(&tree_display) {
                handled = true;
            } else if let Some(continuations) = which_key_continuations_for_display(&tree_display) {
                crate::input::which_key_overlay::show_entries(&tree_display, continuations);
                if let Some(action) = which_key_branch_action_for_display(&tree_display) {
                    let (clean, route) = classify_action(&action);
                    Self::execute_action(clean, route == ActionRoute::Midi);
                }
                handled = true;
            } else {
                let proc = crate::input::processor::get_processor().read().unwrap();
                if let Some(trie) = proc.normal_keytrie() {
                    let pending_chords = pending_display_to_chords(pending_display);
                    let continuations = crate::input::keybinds::bridge::trie_continuations_at(
                        trie,
                        &pending_chords,
                    );
                    if !continuations.is_empty() {
                        crate::input::which_key_overlay::show(pending_display, &continuations);
                        handled = true;
                    }
                }
                drop(proc);
            }

            if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
                info!(pending = %pending_display, "[pending:state]");
                crate::trace_console_msg(format!(
                    "[DEBUG] Pending: '{}' in {} | {} | source=state\n",
                    pending_display,
                    context_name,
                    Self::runtime_state_string()
                ));
            }
        }

        if handled {
            // Hide overlay if action was executed (not pending)
            if pending_from_state.is_none()
                && !commands
                    .iter()
                    .any(|c| matches!(c, InputCommand::Pending { .. }))
                && crate::input::which_key_overlay::is_visible()
            {
                crate::input::which_key_overlay::hide();
            }
            TranslateAccelResult::Eat
        } else if PASSTHROUGH_MODE.load(std::sync::atomic::Ordering::Relaxed) {
            TranslateAccelResult::NotOurWindow
        } else {
            TranslateAccelResult::Eat
        }
    }
}

fn which_key_prefix_continuations(key: &str) -> Option<Vec<OverlayEntry>> {
    which_key_continuations_for_display(key)
}

fn which_key_continuations_for_display(display: &str) -> Option<Vec<OverlayEntry>> {
    let display = normalize_which_key_display(display);
    let proc = crate::input::processor::get_processor().read().ok()?;

    // The merged keytrie is the single source of truth for "what's
    // actually bound right now". The base profile's which-key trees
    // only contribute *labels* — when an overlay rebinds entries under
    // a prefix (or wipes the namespace via the merge's prefix-root
    // shadowing), the overlay display must reflect the merged trie,
    // not the original tree metadata.
    //
    // Continuations come from every trie active in the CURRENT context —
    // matching context layers first (e.g. `@Midi` bindings while the MIDI
    // editor is focused), then the global keymap — mirroring the dispatch
    // priority in `lookup_in_mode`. In an editor, unrouted global entries
    // are dropped: those keys pass through to the editor's native handling,
    // so advertising them would lie.
    let prefix_chords = pending_display_to_chords(&display);
    let in_editor = matches!(
        proc.reaper_context(),
        KeybindContext::Midi | KeybindContext::MidiInline
    );
    let mut seen_keys = std::collections::HashSet::new();
    let mut trie_conts: Vec<(String, String, bool, Option<String>)> = Vec::new();
    for (trie, is_context_layer) in proc.active_tries() {
        for cont in
            crate::input::keybinds::bridge::trie_continuations_at_detailed(trie, &prefix_chords)
        {
            if !is_context_layer && in_editor {
                let routed = cont
                    .3
                    .as_deref()
                    .is_some_and(|a| classify_action(a).1 != ActionRoute::Plain);
                if !routed {
                    continue;
                }
            }
            if seen_keys.insert(cont.0.clone()) {
                trie_conts.push(cont);
            }
        }
    }
    if trie_conts.is_empty() {
        return None;
    }
    trie_conts.sort_by(|a, b| a.0.cmp(&b.0));

    // Canonical prefix for binding-desc lookups ("S-t 2" + child "4" →
    // "S-t 2 4").
    let canonical_prefix = prefix_chords
        .iter()
        .map(crate::input::keybinds::bridge::chord_to_display)
        .collect::<Vec<_>>()
        .join(" ");

    // Collapse `case_insensitive` anchor-modifier variants: when both `S-x`
    // and `x` continue to the same place (same action, or both branches),
    // show only the bare key — the modified variant exists so the user can
    // keep the anchor's modifiers held, not as a separate menu item.
    let trie_conts: Vec<_> = trie_conts
        .iter()
        .filter(|(key, _, is_branch, action)| {
            let Some((_mods, bare)) = key.rsplit_once('-') else {
                return true;
            };
            if bare.is_empty() {
                return true;
            }
            !trie_conts.iter().any(|(k2, _, b2, a2)| {
                k2 == bare && b2 == is_branch && (*is_branch || a2 == action)
            })
        })
        .cloned()
        .collect();

    // Walk the base which-key trees to the same prefix — gives us a
    // per-child label lookup. Missing entries fall back to either the
    // trie node's name (set by the bridge for prefix nodes) or a
    // best-effort label derived from the leaf's action.
    let tree_entries: Option<&[WhichKeyEntry]> = proc.current_trees().iter().find_map(|tree| {
        let normalized_prefix = crate::input::keybinds::bridge::translate_sequence(&tree.prefix);
        if tree.entries.is_empty() {
            return None;
        }
        let remainder = if display == normalized_prefix {
            ""
        } else {
            display.strip_prefix(&normalized_prefix)?
        };
        let mut entries: &[WhichKeyEntry] = tree.entries.as_slice();
        for key in display_remainder_tokens(remainder) {
            let next = entries.iter().find_map(|entry| match entry {
                WhichKeyEntry::Branch {
                    key: entry_key,
                    children,
                    ..
                } if crate::input::keybinds::bridge::translate_sequence(entry_key).as_str()
                    == key =>
                {
                    Some(children.as_slice())
                }
                _ => None,
            });
            entries = next?;
        }
        Some(entries)
    });

    let mut out = Vec::with_capacity(trie_conts.len());
    for (key, trie_label, is_branch, leaf_action) in &trie_conts {
        // Tree-derived label wins only when the base tree's leaf points
        // at the SAME action that's currently bound. Matching by key
        // alone leaks base labels onto overlay-replaced keys (e.g.
        // base `o a` → "Ripple All Tracks", overlay `o a` →
        // `_FTS_SESSION_ORGANIZE_EVERYTHING` — the label would have
        // wrongly stayed "Ripple All Tracks").
        //
        // Branches use the tree's branch label whenever the key
        // matches — there's no leaf action to compare against, and
        // the user probably wants the nested menu's heading.
        let tree_label_lookup = tree_entries.and_then(|es| {
            es.iter().find_map(|entry| match entry {
                WhichKeyEntry::Leaf {
                    key: ek,
                    label: l,
                    action: tree_action,
                } if crate::input::keybinds::bridge::translate_sequence(ek).as_str()
                    == key.as_str()
                    && leaf_action.as_deref() == Some(tree_action.as_str()) =>
                {
                    Some(l.clone())
                }
                WhichKeyEntry::Branch {
                    key: ek, label: l, ..
                } if *is_branch
                    && crate::input::keybinds::bridge::translate_sequence(ek).as_str()
                        == key.as_str() =>
                {
                    Some(l.clone())
                }
                _ => None,
            })
        });
        let mut label = tree_label_lookup.unwrap_or_else(|| trie_label.clone());

        // Plain `bindings(...)` sequences carry their `desc` in the
        // processor's side map (descs don't survive into the trie).
        if label.is_empty() {
            let canonical = if canonical_prefix.is_empty() {
                key.clone()
            } else {
                format!("{canonical_prefix} {key}")
            };
            if let Some(desc) = proc.binding_desc(&canonical) {
                label = desc.to_string();
            }
        }

        if label.is_empty()
            && !*is_branch
            && let Some(action) = leaf_action.as_ref()
        {
            label = humanize_action_id(action);
        }

        let available = if *is_branch {
            true
        } else {
            leaf_action
                .as_deref()
                .map(|a| marker_target_available(a).unwrap_or(true))
                .unwrap_or(true)
        };

        out.push(OverlayEntry {
            key: key.clone(),
            label,
            is_branch: *is_branch,
            available,
        });
    }
    Some(out)
}

/// Best-effort humanization for an action id when no `desc` / label
/// is available. `_FTS_SESSION_ORGANIZE_SELECTED_TRACKS` →
/// `Organize Selected Tracks`. Numeric / non-FTS ids pass through.
fn humanize_action_id(action: &str) -> String {
    let trimmed = action.trim_start_matches('_');
    let body = trimmed
        .strip_prefix("FTS_SESSION_")
        .or_else(|| trimmed.strip_prefix("FTS_INPUT_"))
        .or_else(|| trimmed.strip_prefix("FTS_"))
        .unwrap_or(trimmed);
    if body.chars().all(|c| c.is_ascii_digit()) {
        return action.to_string();
    }
    body.split('_')
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => {
                    c.to_ascii_uppercase().to_string()
                        + chars.as_str().to_ascii_lowercase().as_str()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn which_key_branch_action_for_display(display: &str) -> Option<String> {
    let display = normalize_which_key_display(display);
    let proc = crate::input::processor::get_processor().read().ok()?;
    let (root_entries, remainder) = proc.current_trees().iter().find_map(|tree| {
        let normalized_prefix = crate::input::keybinds::bridge::translate_sequence(&tree.prefix);
        if tree.entries.is_empty() {
            return None;
        }
        display
            .strip_prefix(&normalized_prefix)
            .map(|rest| (tree.entries.as_slice(), rest))
    })?;

    let mut entries = root_entries;
    let mut matched_action = None;
    for key in display_remainder_tokens(remainder) {
        let next = entries.iter().find_map(|entry| match entry {
            WhichKeyEntry::Branch {
                key: entry_key,
                action,
                children,
                ..
            } if crate::input::keybinds::bridge::translate_sequence(entry_key).as_str() == key => {
                Some((action.clone(), children.as_slice()))
            }
            _ => None,
        })?;
        matched_action = next.0;
        entries = next.1;
    }
    matched_action
}

fn normalize_which_key_display(display: &str) -> String {
    display
        .split_whitespace()
        .map(normalize_which_key_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_which_key_token(token: &str) -> String {
    if let Some(rest) = token.strip_prefix("S-") {
        format!("Shift+{}", rest)
    } else if let Some(rest) = token.strip_prefix("C-") {
        format!("Ctrl+{}", rest)
    } else if let Some(rest) = token.strip_prefix("A-") {
        format!("Alt+{}", rest)
    } else {
        token.to_string()
    }
}

/// Where a resolved action should run, decided by the *binding*, not by the
/// focused window. The keybind bridge encodes this as a prefix marker on the
/// action string (see [`mark_action`]); [`classify_action`] decodes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ActionRoute {
    /// `@Midi` binding → MIDI Editor section.
    Midi,
    /// `passthrough true` binding → main section, even from inside an editor.
    ForceMain,
    /// Ordinary binding → main section normally, but passed through to a
    /// focused editor (native handling) instead of run in main.
    Plain,
}

/// Prefix marker for `@Midi` bindings (run in the MIDI Editor section).
/// SOH control chars bracket the tag so it can't collide with a real action.
pub(crate) const MIDI_SECTION_MARK: &str = "\u{1}midi\u{1}";
/// Prefix marker for `passthrough` bindings (force the main section).
pub(crate) const MAIN_FORCE_MARK: &str = "\u{1}main\u{1}";

/// Tag `action` with its routing marker based on the binding's context and
/// its `passthrough` flag, so execution routes by the binding rather than by
/// whichever window has focus at fire time.
pub(crate) fn mark_action(action: &str, ctx: KeybindContext, passthrough: bool) -> String {
    // `passthrough` wins over MIDI-section routing: it means "run in the main
    // section," even for a `@Midi` binding. That's what custom FTS extension
    // actions (registered in the main section) need when bound to a MIDI key —
    // e.g. `f` → FTS_MIDI_INSERT_FLAM is active only in the editor (context
    // @Midi) yet must execute in main, since it targets the editor itself.
    if passthrough {
        format!("{MAIN_FORCE_MARK}{action}")
    } else if ctx == KeybindContext::Midi {
        format!("{MIDI_SECTION_MARK}{action}")
    } else {
        action.to_string()
    }
}

/// Decode a possibly-marked action into `(clean_action, route)`.
pub(crate) fn classify_action(action: &str) -> (&str, ActionRoute) {
    if let Some(rest) = action.strip_prefix(MIDI_SECTION_MARK) {
        (rest, ActionRoute::Midi)
    } else if let Some(rest) = action.strip_prefix(MAIN_FORCE_MARK) {
        (rest, ActionRoute::ForceMain)
    } else {
        (action, ActionRoute::Plain)
    }
}

fn marker_target_available(action: &str) -> Option<bool> {
    let target = marker_target_from_action(action)?;
    Some(
        project_marker_names()
            .iter()
            .any(|name| target.matches(name)),
    )
}

fn marker_target_from_action(action: &str) -> Option<MarkerTarget> {
    let mut normalized = action.trim().to_ascii_uppercase();
    normalized = normalized.replace('.', "_");
    let suffix = normalized
        .strip_prefix("FTS_SESSION_INSERT_")
        .or_else(|| normalized.strip_prefix("FTS_SESSION_GOTO_"))?;
    let name = suffix
        .strip_suffix("_REGION")
        .or_else(|| suffix.strip_suffix("_MARKER"))
        .unwrap_or(suffix);

    let canonical = match name {
        "INTRO" => "IN",
        "VERSE" => "VS",
        "PRE_CHORUS" => "PC",
        "CHORUS" => "CH",
        "BRIDGE" => "BR",
        "OUTRO" => "OUT",
        "INSTRUMENTAL" => "INST",
        "SOLO" => "SOLO",
        "HITS" => "HITS",
        "INTERLUDE" => "INT",
        "BREAKDOWN" => "BD",
        "VAMP" => "VAMP",
        "COUNT_IN" => "COUNT-IN",
        "END" => "END",
        "START" => "=START",
        "SONGSTART" => "SONGSTART",
        "SONGEND" => "SONGEND",
        other => other,
    };

    Some(MarkerTarget {
        canonical: canonical.to_string(),
    })
}

struct MarkerTarget {
    canonical: String,
}

impl MarkerTarget {
    fn matches(&self, name: &str) -> bool {
        let normalized = normalize_marker_name(name);
        normalized == self.canonical
            || normalized
                .strip_prefix(&self.canonical)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    }
}

fn normalize_marker_name(name: &str) -> String {
    name.trim()
        .to_ascii_uppercase()
        .replace('_', "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn project_marker_names() -> Vec<String> {
    let reaper = Reaper::get();
    let low = reaper.medium_reaper().low();
    let mut names = Vec::new();
    let mut idx = 0;

    loop {
        let mut is_region = false;
        let mut pos = 0.0;
        let mut end = 0.0;
        let mut name_ptr: *const std::ffi::c_char = std::ptr::null();
        let mut marker_idx = 0;
        let result = unsafe {
            low.EnumProjectMarkers3(
                std::ptr::null_mut(),
                idx,
                &mut is_region,
                &mut pos,
                &mut end,
                &mut name_ptr,
                &mut marker_idx,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            break;
        }
        idx += 1;
        if !name_ptr.is_null()
            && let Ok(name) = unsafe { CStr::from_ptr(name_ptr) }.to_str()
        {
            names.push(name.to_string());
        }
    }

    names
}

fn goto_marker_target(target: &MarkerTarget) {
    let reaper = Reaper::get();
    let low = reaper.medium_reaper().low();
    let mut idx = 0;

    loop {
        let mut is_region = false;
        let mut pos = 0.0;
        let mut end = 0.0;
        let mut name_ptr: *const std::ffi::c_char = std::ptr::null();
        let mut marker_idx = 0;
        let result = unsafe {
            low.EnumProjectMarkers3(
                std::ptr::null_mut(),
                idx,
                &mut is_region,
                &mut pos,
                &mut end,
                &mut name_ptr,
                &mut marker_idx,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            break;
        }
        idx += 1;

        let name = if name_ptr.is_null() {
            ""
        } else {
            unsafe { CStr::from_ptr(name_ptr) }.to_str().unwrap_or("")
        };
        if target.matches(name) {
            low.SetEditCurPos(pos, true, false);
            return;
        }
    }
}

fn display_remainder_tokens(remainder: &str) -> Vec<String> {
    let trimmed = remainder.trim();
    if trimmed.is_empty() {
        Vec::new()
    } else if trimmed.contains(char::is_whitespace) {
        trimmed.split_whitespace().map(str::to_string).collect()
    } else {
        trimmed.chars().map(|ch| ch.to_string()).collect()
    }
}

/// Convert a pending display string back to KeyChords for trie lookup.
///
/// Accepts both display formats in play:
/// - compact processor notation: "v", "fe", "C-s", "S-nd" (Shift+n then d)
/// - normalized which-key notation from [`normalize_which_key_display`]:
///   "Shift+n", "Ctrl+Shift+s", "Shift+nd" (Shift+n then d)
///
/// The normalized form previously parsed as garbage character chords
/// ("Shift+n" → [s,h,i,f,t,+,n]), which silently broke trie lookups — and
/// with them which-key labels/headers — for every modifier-anchored prefix
/// like `<S-n>`. This is a best-effort parse for common cases.
pub(crate) fn pending_display_to_chords(display: &str) -> Vec<input::key::KeyChord> {
    use input::key::{KeyChord, KeyCode, Modifiers};

    let mut chords = Vec::new();
    let mut rest = display;

    while !rest.is_empty() {
        // Skip whitespace between chords (space-separated sequences).
        let trimmed = rest.trim_start();
        if trimmed.len() != rest.len() {
            rest = trimmed;
            continue;
        }

        // Accumulate modifier prefixes, long form ("Shift+") and compact
        // form ("S-") alike, in any combination/order.
        let mut mods = Modifiers::NONE;
        loop {
            if let Some(r) = rest.strip_prefix("Ctrl+") {
                mods.ctrl = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Meta+") {
                mods.meta = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Shift+") {
                mods.shift = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Alt+") {
                mods.alt = true;
                rest = r;
            } else if rest.len() >= 2 && rest.as_bytes()[1] == b'-' {
                match rest.as_bytes()[0] {
                    b'C' => mods.ctrl = true,
                    b'M' => mods.meta = true,
                    b'S' => mods.shift = true,
                    b'A' => mods.alt = true,
                    _ => break,
                }
                rest = &rest[2..];
            } else {
                break;
            }
        }

        // The next character is the key. Consecutive unmodified characters
        // are separate chords ("fe" → f, e; "S-nd" → Shift+n, d).
        let Some(c) = rest.chars().next() else { break };
        rest = &rest[c.len_utf8()..];
        chords.push(KeyChord::new(
            KeyCode::Character(c.to_lowercase().to_string()),
            mods,
        ));
    }

    chords
}

impl InputHandler {
    /// Handle mouse wheel events
    fn handle_mouse_wheel(args: TranslateAccelArgs, message_type: u32) -> TranslateAccelResult {
        let raw_msg = args.msg.raw();
        let delta = (raw_msg.wParam as i32 >> 16) as i16;
        let is_horizontal = message_type == raw::WM_MOUSEHWHEEL;
        let (_context, context_name, _window_title) = Self::determine_context();

        let direction = if delta > 0 { "up" } else { "down" };
        let wheel_type = if is_horizontal {
            "horizontal wheel"
        } else {
            "wheel"
        };

        if DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed) {
            info!(
                mode = %crate::current_mode_label(),
                wheel = %wheel_type,
                direction = %direction,
                context = %context_name,
                "[wheel]"
            );
        }
        crate::trace_console_msg(format!(
            "Mouse {} {} in {}\n",
            wheel_type, direction, context_name
        ));

        // Wheel over an app panel belongs to the panel: its own wndproc
        // zooms and pans with it. NotOurWindow lets REAPER route the
        // wheel to the window under the cursor as usual.
        let mouse_ctx = crate::input::mouse_context::determine_mouse_context(
            Reaper::get().medium_reaper(),
            crate::input::mouse_context::DetectionMode::minimal(),
        );
        if mouse_ctx.context == Context::ExpressionEditor {
            return TranslateAccelResult::NotOurWindow;
        }

        if PASSTHROUGH_MODE.load(std::sync::atomic::Ordering::Relaxed) {
            TranslateAccelResult::NotOurWindow
        } else {
            TranslateAccelResult::Eat
        }
    }

    /// Check if interception is enabled
    pub fn is_enabled() -> bool {
        INTERCEPTION_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Set interception enabled state
    pub fn set_enabled(enabled: bool) {
        let was_enabled = INTERCEPTION_ENABLED.load(std::sync::atomic::Ordering::Relaxed);
        INTERCEPTION_ENABLED.store(enabled, std::sync::atomic::Ordering::Relaxed);

        if enabled && !was_enabled {
            if !HANDLER_REGISTERED.load(std::sync::atomic::Ordering::Relaxed) {
                if let Err(e) = register_input_handler() {
                    tracing::warn!("Failed to register input handler: {}", e);
                } else {
                    HANDLER_REGISTERED.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }

            if let Err(e) = crate::input::wheel_hook::install_main_window_hook() {
                tracing::warn!("Failed to install wheel hook: {}", e);
            }
            if let Err(e) = crate::input::wheel_hook::install_arrange_view_hook() {
                tracing::warn!("Failed to install arrange view hook: {}", e);
            }
            crate::input::wheel_hook::check_and_hook_midi_editors();

            info!("FTS-Input interception enabled");
        } else if !enabled && was_enabled {
            info!(
                "FTS-Input interception disabled (handler remains registered but returns NotOurWindow for all keys)"
            );
        }
    }

    /// Toggle interception enabled state
    pub fn toggle() -> bool {
        let new_state = !Self::is_enabled();
        Self::set_enabled(new_state);
        new_state
    }

    /// Check if passthrough mode is enabled
    pub fn is_passthrough() -> bool {
        PASSTHROUGH_MODE.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Set passthrough mode
    pub fn set_passthrough(enabled: bool) {
        PASSTHROUGH_MODE.store(enabled, std::sync::atomic::Ordering::Relaxed);
        info!(
            "FTS-Input passthrough mode {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Toggle passthrough mode
    pub fn toggle_passthrough() -> bool {
        let new_state = !Self::is_passthrough();
        Self::set_passthrough(new_state);
        new_state
    }

    /// Check if debug logging is enabled
    pub fn is_debug_logging() -> bool {
        DEBUG_LOGGING.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Set debug logging mode
    pub fn set_debug_logging(enabled: bool) {
        DEBUG_LOGGING.store(enabled, std::sync::atomic::Ordering::Relaxed);
        info!(
            "FTS-Input debug logging {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Toggle debug logging mode
    pub fn toggle_debug_logging() -> bool {
        let new_state = !Self::is_debug_logging();
        Self::set_debug_logging(new_state);
        new_state
    }

    /// Convert internal Context to KeybindContext
    pub fn context_to_keybind_context(context: &Context) -> KeybindContext {
        match context {
            Context::Main => KeybindContext::Main,
            Context::Midi => KeybindContext::Midi,
            Context::MidiEventListEditor => KeybindContext::Midi,
            Context::MidiInlineEditor => KeybindContext::MidiInline,
            Context::MediaExplorer => KeybindContext::MediaExplorer,
            Context::CrossfadeEditor => KeybindContext::Main,
            Context::ExpressionEditor => KeybindContext::Custom(
                crate::input::window_detection::EXPRESSION_EDITOR_TAG.to_string(),
            ),
            Context::Global => KeybindContext::Global,
        }
    }

    /// Execute an action by its command ID (either numeric or named).
    ///
    /// Whether a resolved binding should be passed through to a focused
    /// special editor's native handling rather than executed. True only for
    /// plain (non-`@Midi`, non-`passthrough`) bindings while a MIDI editor is
    /// focused — so the editor "owns" its keys and FTS only overrides them via
    /// explicit `@Midi` bindings (or `passthrough true` to force main).
    fn should_passthrough_to_editor(context: Context, route: ActionRoute) -> bool {
        matches!(context, Context::Midi | Context::MidiEventListEditor)
            && route == ActionRoute::Plain
    }

    /// `midi_section` decides which REAPER section the command runs in: the
    /// MIDI Editor section (via `MIDIEditor_LastFocused_OnCommand`) when true,
    /// otherwise the main section. The caller derives this from the binding's
    /// route, so e.g. a global `backspace` never gets reinterpreted as the
    /// MIDI section's "insert note". `action` must already be marker-free.
    fn execute_action(action: &str, midi_section: bool) {
        let reaper = Reaper::get();
        let medium_reaper = reaper.medium_reaper();

        if let Some(target) = marker_target_from_action(action)
            && action.to_ascii_uppercase().contains("_GOTO_")
        {
            crate::input::handler::goto_marker_target(&target);
            return;
        }

        let run = |cmd_id: i32| {
            if midi_section {
                medium_reaper
                    .low()
                    .MIDIEditor_LastFocused_OnCommand(cmd_id, false);
            } else {
                medium_reaper.low().Main_OnCommand(cmd_id, 0);
            }
        };

        // Try parsing as numeric action ID first
        if let Ok(cmd_id) = action.parse::<u32>() {
            debug!(action = %action, cmd_id = cmd_id, midi = midi_section, "Executing numeric action");
            run(cmd_id as i32);
            return;
        }

        // Try looking up named command
        if let Some(cmd_id) = medium_reaper.named_command_lookup(action) {
            debug!(action = %action, cmd_id = ?cmd_id, midi = midi_section, "Executing named action");
            run(cmd_id.get() as i32);
            return;
        }

        // Also try with underscore prefix (REAPER convention)
        let prefixed = format!("_{}", action);
        if let Some(cmd_id) = medium_reaper.named_command_lookup(prefixed.as_str()) {
            debug!(action = %action, cmd_id = ?cmd_id, midi = midi_section, "Executing named action (prefixed)");
            run(cmd_id.get() as i32);
            return;
        }

        tracing::warn!(action = %action, "Could not find action to execute");
    }
}

/// Register the input handler
/// This should only be called when FTS-input is enabled
pub fn register_input_handler() -> Result<(), Box<dyn std::error::Error>> {
    if HANDLER_REGISTERED.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }

    info!("Registering FTS-Input handler");

    // Initialize the input processor with defaults
    crate::input::processor::init();

    // Initialize the mouse modifier manager with default profiles
    super::mouse_modifiers::manager::init();

    let reaper = Reaper::get();
    let handler = Box::new(InputHandler::new());

    reaper
        .medium_session()
        .plugin_register_add_accelerator_register(handler, AcceleratorPosition::Front)?;

    HANDLER_REGISTERED.store(true, std::sync::atomic::Ordering::Relaxed);
    info!("FTS-Input handler registered successfully");

    Ok(())
}

#[cfg(test)]
mod pending_display_tests {
    use super::pending_display_to_chords;
    use input::key::{KeyChord, KeyCode, Modifiers};

    fn plain(c: &str) -> KeyChord {
        KeyChord::plain(KeyCode::Character(c.to_string()))
    }

    fn shifted(c: &str) -> KeyChord {
        let mut mods = Modifiers::NONE;
        mods.shift = true;
        KeyChord::new(KeyCode::Character(c.to_string()), mods)
    }

    #[test]
    fn compact_plain_sequence() {
        assert_eq!(
            pending_display_to_chords("fe"),
            vec![plain("f"), plain("e")]
        );
    }

    #[test]
    fn compact_modifier_chord() {
        assert_eq!(pending_display_to_chords("S-n"), vec![shifted("n")]);
    }

    #[test]
    fn compact_modifier_then_plain() {
        // `<S-n>` anchor followed by a bare `d` — the Create New Track menu.
        assert_eq!(
            pending_display_to_chords("S-nd"),
            vec![shifted("n"), plain("d")]
        );
    }

    #[test]
    fn normalized_modifier_chord() {
        // normalize_which_key_display output ("S-n" → "Shift+n") must parse
        // to the same chords as the compact form.
        assert_eq!(pending_display_to_chords("Shift+n"), vec![shifted("n")]);
        assert_eq!(
            pending_display_to_chords("Shift+nd"),
            vec![shifted("n"), plain("d")]
        );
    }

    #[test]
    fn normalized_multi_modifier() {
        let mut mods = Modifiers::NONE;
        mods.ctrl = true;
        mods.shift = true;
        let expected = vec![KeyChord::new(KeyCode::Character("s".into()), mods)];
        assert_eq!(pending_display_to_chords("Ctrl+Shift+s"), expected);
        assert_eq!(pending_display_to_chords("C-S-s"), expected);
    }

    #[test]
    fn uppercase_key_lowercased() {
        assert_eq!(pending_display_to_chords("S"), vec![plain("s")]);
    }
}
