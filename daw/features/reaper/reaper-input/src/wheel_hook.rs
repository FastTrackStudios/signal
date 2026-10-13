use reaper_high::Reaper;
use reaper_low::Swell;
use reaper_low::raw::{
    GWL_WNDPROC, HWND, LPARAM, LRESULT, UINT, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEHWHEEL,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WPARAM,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

static MAIN_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static ORIGINAL_PROCS: RefCell<HashMap<HWND, unsafe extern "C" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT>> = RefCell::new(HashMap::new());
    static HOOKED_WINDOWS: RefCell<HashSet<HWND>> = RefCell::new(HashSet::new());
}

unsafe extern "C" fn hook_proc(hwnd: HWND, msg: UINT, w: WPARAM, l: LPARAM) -> LRESULT {
    if !crate::is_enabled() {
        return unsafe { call_original_proc(hwnd, msg, w, l) };
    }

    match msg {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            if crate::is_debug_logging() {
                let button = match msg {
                    WM_LBUTTONDOWN => "left",
                    WM_RBUTTONDOWN => "right",
                    WM_MBUTTONDOWN => "middle",
                    _ => "unknown",
                };
                crate::trace_console_msg(format!("[DEBUG] Mouse click: {}\n", button));
            }
            if crate::is_passthrough() || !crate::is_intercepting() {
                unsafe { call_original_proc(hwnd, msg, w, l) }
            } else {
                0
            }
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let delta = ((w as u32) >> 16) as i16;
            let direction = if delta > 0 { "up/right" } else { "down/left" };
            let kind = if msg == WM_MOUSEHWHEEL {
                "horizontal wheel"
            } else {
                "wheel"
            };

            if crate::is_debug_logging() {
                crate::trace_console_msg(format!(
                    "[DEBUG] Mouse {} {} (delta={})\n",
                    kind, direction, delta
                ));
            }

            if crate::is_passthrough() || !crate::is_intercepting() {
                unsafe { call_original_proc(hwnd, msg, w, l) }
            } else {
                0
            }
        }
        _ => unsafe { call_original_proc(hwnd, msg, w, l) },
    }
}

unsafe fn call_original_proc(hwnd: HWND, msg: UINT, w: WPARAM, l: LPARAM) -> LRESULT {
    match ORIGINAL_PROCS.try_with(|orig_map| orig_map.borrow().get(&hwnd).cloned()) {
        Ok(Some(orig_fn)) => unsafe { orig_fn(hwnd, msg, w, l) },
        Ok(None) | Err(_) => unsafe { Swell::get().DefWindowProc(hwnd, msg, w, l) },
    }
}

pub fn install_wheel_hook(hwnd: HWND) -> Result<(), Box<dyn std::error::Error>> {
    let swell = Swell::get();

    let already_hooked = HOOKED_WINDOWS
        .try_with(|hooked| hooked.borrow().contains(&hwnd))
        .unwrap_or(false);
    if already_hooked {
        return Ok(());
    }

    match ORIGINAL_PROCS.try_with(|m| {
        let mut map = m.borrow_mut();
        if map.contains_key(&hwnd) {
            return Ok(());
        }

        unsafe {
            let get_window_long = swell
                .pointers()
                .GetWindowLong
                .ok_or("GetWindowLong not available")?;
            let old_ptr = get_window_long(hwnd, GWL_WNDPROC);
            let orig_fn: unsafe extern "C" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT =
                mem::transmute(old_ptr);

            map.insert(hwnd, orig_fn);

            let set_window_long = swell
                .pointers()
                .SetWindowLong
                .ok_or("SetWindowLong not available")?;
            set_window_long(hwnd, GWL_WNDPROC, hook_proc as *const () as isize);
        }

        let _ = HOOKED_WINDOWS.try_with(|hooked| {
            hooked.borrow_mut().insert(hwnd);
        });

        Ok(())
    }) {
        Ok(result) => result,
        Err(_) => Err("RefCell already borrowed".into()),
    }
}

pub fn install_main_window_hook() -> Result<(), Box<dyn std::error::Error>> {
    if MAIN_HOOK_INSTALLED.load(Ordering::Relaxed) {
        return Ok(());
    }

    let main_hwnd = Reaper::get().medium_reaper().get_main_hwnd();
    install_wheel_hook(main_hwnd.as_ptr())?;
    MAIN_HOOK_INSTALLED.store(true, Ordering::Relaxed);
    info!("input-reaper mouse hook installed on main window");
    Ok(())
}

pub fn check_and_hook_midi_editors() {
    if !crate::is_enabled() {
        return;
    }

    let medium = Reaper::get().medium_reaper();
    if let Some(midi_editor_hwnd) = medium.midi_editor_get_active() {
        let hwnd = midi_editor_hwnd.as_ptr();
        let already_hooked = HOOKED_WINDOWS
            .try_with(|hooked| hooked.borrow().contains(&hwnd))
            .unwrap_or(false);
        if !already_hooked {
            if let Err(error) = install_wheel_hook(hwnd) {
                warn!(%error, "Failed to hook MIDI editor window");
            } else {
                info!("input-reaper mouse hook installed on MIDI editor window");
            }
        }
    }
}
