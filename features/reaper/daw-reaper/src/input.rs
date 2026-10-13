//! `impl Input for Reaper` — TranslateAccel keyboard hook + action exec.

use daw_control::lock::RwLockExt;
use daw_proto::{Input, KeyCode, KeyFilter, KeyModifiers, KeyMsgKind};
use reaper_high::Reaper;
use reaper_medium::{
    AccelMsgKind, AcceleratorBehavior, AcceleratorPosition, TranslateAccel, TranslateAccelArgs,
    TranslateAccelResult,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use tracing::{debug, info};

struct InputState {
    enabled: AtomicBool,
    filter: RwLock<KeyFilter>,
}

impl InputState {
    fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            filter: RwLock::new(KeyFilter::PassAll),
        }
    }
}

static INPUT_STATE: OnceLock<Arc<InputState>> = OnceLock::new();

fn state() -> &'static Arc<InputState> {
    INPUT_STATE.get_or_init(|| Arc::new(InputState::new()))
}

struct InputAccelHandler {
    state: Arc<InputState>,
}

impl TranslateAccel for InputAccelHandler {
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        if !self.state.enabled.load(Ordering::Relaxed) {
            return TranslateAccelResult::NotOurWindow;
        }

        let msg = args.msg;
        let _msg_kind = match msg.message() {
            AccelMsgKind::KeyDown => KeyMsgKind::KeyDown,
            AccelMsgKind::KeyUp => KeyMsgKind::KeyUp,
            AccelMsgKind::SysKeyDown => KeyMsgKind::SysKeyDown,
            AccelMsgKind::SysKeyUp => KeyMsgKind::SysKeyUp,
            AccelMsgKind::Char => KeyMsgKind::Char,
            _ => return TranslateAccelResult::NotOurWindow,
        };

        let behavior = msg.behavior();
        let modifiers = KeyModifiers {
            ctrl: behavior.contains(AcceleratorBehavior::Control),
            alt: behavior.contains(AcceleratorBehavior::Alt),
            shift: behavior.contains(AcceleratorBehavior::Shift),
        };
        let vk_code = msg.key().get() as u32;
        let key = match vk_to_keycode(vk_code) {
            Some(k) => k,
            None => return TranslateAccelResult::NotOurWindow,
        };
        if is_text_field_focused() {
            return TranslateAccelResult::NotOurWindow;
        }
        let should_eat = {
            let filter = self.state.filter.read_recoverable("input");
            match &*filter {
                KeyFilter::PassAll => false,
                KeyFilter::EatAll => true,
                KeyFilter::EatMatching { patterns } => {
                    patterns.iter().any(|p| matches_filter(p, &key, &modifiers))
                }
            }
        };
        if !should_eat {
            return TranslateAccelResult::NotOurWindow;
        }
        TranslateAccelResult::Eat
    }
}

/// Register the TranslateAccel handler with REAPER. Call once during
/// extension init (main thread). Streaming subscription retired.
pub fn install_input_hook() {
    let handler = Box::new(InputAccelHandler {
        state: state().clone(),
    });
    let reaper = Reaper::get();
    let mut session = reaper.medium_session();
    match session.plugin_register_add_accelerator_register(handler, AcceleratorPosition::Front) {
        Ok(_handle) => info!("Input TranslateAccel handler registered"),
        Err(e) => tracing::error!("Failed to register TranslateAccel handler: {e}"),
    }
}

fn is_text_field_focused() -> bool {
    unsafe {
        let swell = reaper_low::Swell::get();
        let focused = swell.GetFocus();
        if focused.is_null() {
            return false;
        }
        let mut class_name = [0u8; 64];
        let len = swell.GetClassName(
            focused,
            class_name.as_mut_ptr() as *mut _,
            class_name.len() as i32,
        );
        if len <= 0 {
            return false;
        }
        let name = std::str::from_utf8_unchecked(&class_name[..len as usize]);
        name.eq_ignore_ascii_case("Edit")
            || name.starts_with("RichEdit")
            || name.eq_ignore_ascii_case("RICHEDIT50W")
    }
}

fn vk_to_keycode(vk: u32) -> Option<KeyCode> {
    match vk {
        0x41..=0x5A => {
            let ch = (vk as u8) as char;
            Some(KeyCode::Character(ch.to_lowercase().to_string()))
        }
        0x30..=0x39 => {
            let ch = (vk as u8) as char;
            Some(KeyCode::Character(ch.to_string()))
        }
        0x70..=0x87 => Some(KeyCode::F((vk - 0x70 + 1) as u8)),
        0x25 => Some(KeyCode::ArrowLeft),
        0x26 => Some(KeyCode::ArrowUp),
        0x27 => Some(KeyCode::ArrowRight),
        0x28 => Some(KeyCode::ArrowDown),
        0x24 => Some(KeyCode::Home),
        0x23 => Some(KeyCode::End),
        0x21 => Some(KeyCode::PageUp),
        0x22 => Some(KeyCode::PageDown),
        0x2D => Some(KeyCode::Insert),
        0x0D => Some(KeyCode::Enter),
        0x1B => Some(KeyCode::Escape),
        0x09 => Some(KeyCode::Tab),
        0x08 => Some(KeyCode::Backspace),
        0x2E => Some(KeyCode::Delete),
        0x20 => Some(KeyCode::Character(" ".to_string())),
        0xBA => Some(KeyCode::Character(";".to_string())),
        0xBB => Some(KeyCode::Character("=".to_string())),
        0xBC => Some(KeyCode::Character(",".to_string())),
        0xBD => Some(KeyCode::Character("-".to_string())),
        0xBE => Some(KeyCode::Character(".".to_string())),
        0xBF => Some(KeyCode::Character("/".to_string())),
        0xC0 => Some(KeyCode::Character("`".to_string())),
        0xDB => Some(KeyCode::Character("[".to_string())),
        0xDC => Some(KeyCode::Character("\\".to_string())),
        0xDD => Some(KeyCode::Character("]".to_string())),
        0xDE => Some(KeyCode::Character("'".to_string())),
        _ => None,
    }
}

fn matches_filter(
    pattern: &daw_proto::KeyPattern,
    key: &KeyCode,
    modifiers: &KeyModifiers,
) -> bool {
    if pattern.key != *key {
        return false;
    }
    if pattern.exact_modifiers {
        pattern.modifiers.ctrl == modifiers.ctrl
            && pattern.modifiers.alt == modifiers.alt
            && pattern.modifiers.shift == modifiers.shift
    } else {
        (!pattern.modifiers.ctrl || modifiers.ctrl)
            && (!pattern.modifiers.alt || modifiers.alt)
            && (!pattern.modifiers.shift || modifiers.shift)
    }
}

impl Input for crate::Reaper {
    fn set_key_filter(&self, filter: KeyFilter) {
        let label = match &filter {
            KeyFilter::EatAll => "EatAll".to_string(),
            KeyFilter::PassAll => "PassAll".to_string(),
            KeyFilter::EatMatching { patterns } => {
                format!("EatMatching({} patterns)", patterns.len())
            }
        };
        *state().filter.write_recoverable("input") = filter;
        info!("Key filter updated: {label}");
    }

    fn get_key_filter(&self) -> KeyFilter {
        state().filter.read_recoverable("input").clone()
    }

    fn set_enabled(&self, enabled: bool) {
        state().enabled.store(enabled, Ordering::Relaxed);
        info!(
            "Input interception {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    fn is_enabled(&self) -> bool {
        state().enabled.load(Ordering::Relaxed)
    }

    fn execute_action(&self, action_id: &str) {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let cmd_id = if let Ok(numeric_id) = action_id.parse::<u32>() {
            Some(numeric_id)
        } else if let Some(id) = medium.named_command_lookup(action_id) {
            Some(id.get())
        } else {
            let prefixed = format!("_{action_id}");
            medium
                .named_command_lookup(prefixed.as_str())
                .map(|id| id.get())
        };
        match cmd_id {
            Some(id) if id > 0 => {
                medium.low().Main_OnCommand(id as i32, 0);
                debug!("Executed action '{}' (cmd_id={})", action_id, id);
            }
            _ => {
                debug!("Action not found: '{}'", action_id);
            }
        }
    }
}
