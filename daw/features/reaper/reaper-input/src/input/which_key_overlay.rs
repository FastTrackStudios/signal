//! Which-Key transparent overlay window.
//!
//! Shows available key continuations in a single-column popup anchored to the
//! bottom-right of REAPER's arrange view — similar to which-key.nvim.
//!
//! Renders via `reaper_dioxus::DioxusOverlay` using Dioxus components with
//! inline CSS styling, powered by the blitz rendering engine (dioxus-native).
//!
//! The overlay opens as soon as a partial match occurs so prefix keys like `z`
//! visibly enter a which-key tree before the next key is pressed.

use reaper_embed::VelloTextRenderer;
use reaper_high::Reaper;
use reaper_low::Swell;
use reaper_low::raw::RECT;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, warn};

use crate::input::keybinds::which_key::WhichKeyEntry;
use crate::input::reaper_windows::get_arrange_wnd;
use crate::input::which_key_component::{
    OverlayEntry, WhichKeyOverlay as WhichKeyComponent, WhichKeyState,
};
use reaper_dioxus::DioxusOverlay;

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

// Design baseline (scale = 1.0). The component multiplies every pixel
// value by the runtime `WhichKeyState.scale` so the same numbers stay
// consistent between the geometry-prediction here and the inline-style
// rendering in `which_key_component.rs`.
const FONT_SIZE: f32 = 11.0;
const PADDING_X: f64 = 12.0;
const MARGIN: f64 = 12.0;

const HEADER_HEIGHT: f64 = 30.0;
const CONTENT_PADDING_Y: f64 = 16.0;
const ROW_HEIGHT: f64 = 18.0;
const ROW_GAP: f64 = 4.0;
const WINDOW_CHROME_PAD: f64 = 4.0;

const MIN_POPUP_WIDTH: f64 = 180.0;
const WIDTH_PADDING: f64 = 48.0;

/// Monitor height (logical points) the baseline design was tuned for
/// (a 1080p display at scale 1.0). The popup scales with the monitor the
/// anchor window lives on, clamped to a readable range.
const SCALE_ANCHOR_SCREEN_HEIGHT: f64 = 1080.0;
const SCALE_MIN: f32 = 0.85;
const SCALE_MAX: f32 = 1.60;
/// Hard floor when shrinking a tall menu to fit a cramped anchor window —
/// below this the text is unreadable and overflowing is the lesser evil.
const FIT_SCALE_FLOOR: f64 = 0.70;

/// Derive a multiplicative size factor for the overlay from the monitor
/// containing the anchor window.
///
/// Deliberately NOT based on the anchor window's own height: dockers and
/// editors resize the arrange constantly, which made the HUD change size
/// every time the layout changed. The monitor is stable — same screen,
/// same-sized popup — and per-popup fitting is handled separately by
/// `compute_popup_layout`'s shrink-to-fit pass.
fn compute_responsive_scale() -> f32 {
    let Some((_, _, _, mh)) = monitor_bounds_for_anchor() else {
        return 1.0;
    };
    let raw = (mh as f64 / SCALE_ANCHOR_SCREEN_HEIGHT) as f32;
    raw.clamp(SCALE_MIN, SCALE_MAX)
}

/// Work-area bounds of the monitor containing the anchor window.
fn monitor_bounds_for_anchor() -> Option<(i32, i32, u32, u32)> {
    let (ax, ay, aw, ah) = get_anchor_bounds()?;
    let src = RECT {
        left: ax,
        top: ay,
        right: ax + aw as i32,
        bottom: ay + ah as i32,
    };
    let mut out = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        Swell::get().SWELL_GetViewPort(&mut out, &src, true);
    }
    let w = (out.right - out.left).unsigned_abs();
    let h = (out.bottom - out.top).unsigned_abs();
    (w > 0 && h > 0).then_some((out.left, out.top, w, h))
}

/// Delay before the overlay window appears (ms).
///
/// Prefix keys are part of the interaction model, so this stays at zero:
/// pressing a prefix such as `z` should immediately show the available
/// continuations.
const SHOW_DELAY_MS: u64 = 0;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

struct PendingOverlay {
    entries: Vec<OverlayEntry>,
    sequence: String,
    header_label: String,
    requested_at: Instant,
}

struct LiveOverlay {
    overlay: DioxusOverlay,
    state: WhichKeyState,
}

enum OverlayState {
    Pending(PendingOverlay),
    Live(LiveOverlay),
}

// ---------------------------------------------------------------------------
// Thread-local state
// ---------------------------------------------------------------------------

// Leak the RefCells so their `Drop` impls never run during TLS teardown.
// `LiveOverlay` owns a `DioxusOverlay` whose `VirtualDom` reaches into
// `dioxus_core::runtime::Runtime`'s own thread_local during drop — and the
// destruction order at process exit is undefined, so the runtime's TLS slot
// is frequently already destroyed by the time we get here. Result: panic in
// a destructor during cleanup, which aborts the host. We don't need to free
// these on shutdown — the OS reclaims everything.
thread_local! {
    static OVERLAY: &'static RefCell<Option<OverlayState>> =
        Box::leak(Box::new(RefCell::new(None)));
    static CACHE: &'static RefCell<HashMap<String, LiveOverlay>> =
        Box::leak(Box::new(RefCell::new(HashMap::new())));
    // Pending prewarm queue: each entry is one overlay-spec waiting to be
    // built. We pop and build at most one per call to `prewarm_step` so the
    // host thread stays responsive instead of locking up while we allocate
    // dozens of wgpu surfaces back-to-back.
    static PREWARM_QUEUE: &'static RefCell<Vec<(String, String, Vec<OverlayEntry>)>> =
        Box::leak(Box::new(RefCell::new(Vec::new())));
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Request the which-key overlay with the given continuations.
///
/// Prefix keys should provide immediate visual feedback, so this opens the
/// overlay synchronously instead of waiting for the timer callback.
pub fn show(sequence: &str, continuations: &[(String, String, bool)]) {
    let entries: Vec<OverlayEntry> = continuations
        .iter()
        .map(|(k, l, is_branch)| OverlayEntry {
            key: k.clone(),
            label: l.clone(),
            is_branch: *is_branch,
            available: true,
        })
        .collect();
    show_entries(sequence, entries);
}

pub fn show_entries(sequence: &str, entries: Vec<OverlayEntry>) {
    let header_label = resolve_header_label(sequence);

    crate::trace_console_msg(format!(
        "[DEBUG] WhichKey show requested: sequence='{}' entries={} header='{}' labels=[{}]\n",
        sequence,
        entries.len(),
        header_label,
        entries
            .iter()
            .map(|entry| format!("{}:{}", entry.key, entry.label))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    // Idempotent guard: if the overlay is already live for this same
    // sequence with the same entries (same keys + labels + branch flags),
    // skip the hide/cache/show dance. Prevents flicker when OS auto-repeat
    // delivers redundant keydowns while the user holds a prefix anchor.
    let already_live = OVERLAY.with(|cell| {
        let borrow = cell.borrow();
        match &*borrow {
            Some(OverlayState::Live(live)) => {
                live.state.sequence == sequence
                    && live.state.header_label == header_label
                    && overlay_entries_match(&live.state.entries, &entries)
            }
            _ => false,
        }
    });
    if already_live {
        crate::trace_console_msg(format!(
            "[DEBUG] WhichKey show skipped (idempotent): sequence='{}'\n",
            sequence,
        ));
        return;
    }

    OVERLAY.with(|cell| {
        let mut borrow = cell.borrow_mut();

        if let Some(OverlayState::Live(mut live)) = borrow.take() {
            live.overlay.hide();
            CACHE.with(|cache| {
                cache.borrow_mut().insert(live.state.sequence.clone(), live);
            });
        }

        let pending = PendingOverlay {
            entries,
            sequence: sequence.to_string(),
            header_label,
            requested_at: Instant::now(),
        };

        // A cached window is only reusable if its rendered content matches —
        // the same sequence can map to different menus per context (e.g. `z`
        // is the arrange Zoom tree in main but the MeMagic zoom menu in the
        // MIDI editor). Reusing on sequence alone showed the stale menu.
        let cached = take_cached_overlay(sequence).and_then(|mut live| {
            // Scale must match too — the rendered styles are baked at build
            // time, so a layout change (docked editor opened, moved to
            // another monitor) needs a rebuild, not just a reposition.
            let (_, _, _, _, want_scale) = compute_popup_layout(&pending.entries);
            if overlay_entries_match(&live.state.entries, &pending.entries)
                && live.state.header_label == pending.header_label
                && (live.state.scale - want_scale).abs() < 0.01
            {
                Some(live)
            } else {
                crate::trace_console_msg(format!(
                    "[DEBUG] WhichKey cache content mismatch, rebuilding: sequence='{}'\n",
                    sequence
                ));
                live.overlay.close();
                None
            }
        });

        if let Some(mut live) = cached {
            reposition_overlay(&mut live);
            live.overlay.show();
            live.overlay.update();
            *borrow = Some(OverlayState::Live(live));
        } else if let Some(live) = open_overlay_window(&pending, false) {
            *borrow = Some(OverlayState::Live(live));
        } else {
            crate::trace_console_msg(format!(
                "[DEBUG] WhichKey show failed to open: sequence='{}'\n",
                sequence
            ));
            *borrow = Some(OverlayState::Pending(pending));
        }
    });
}

/// Show all registered which-key prefix trees (cheat sheet).
/// Opens immediately (no delay) since this is an explicit user action.
pub fn show_all_prefixes() {
    let proc = crate::input::processor::get_processor().read().unwrap();
    let roots = proc.all_root_prefixes();
    if roots.is_empty() {
        return;
    }

    let entries: Vec<OverlayEntry> = roots
        .iter()
        .map(|(k, l, is_branch)| OverlayEntry {
            key: k.clone(),
            label: l.clone(),
            is_branch: *is_branch,
            available: true,
        })
        .collect();

    OVERLAY.with(|cell| {
        let mut borrow = cell.borrow_mut();

        if let Some(OverlayState::Live(mut live)) = borrow.take() {
            live.overlay.hide();
            CACHE.with(|cache| {
                cache.borrow_mut().insert(live.state.sequence.clone(), live);
            });
        }

        // Open immediately
        let pending = PendingOverlay {
            entries,
            sequence: String::new(),
            header_label: "Which-Key Bindings".to_string(),
            requested_at: Instant::now(),
        };

        if let Some(mut live) = take_cached_overlay("") {
            reposition_overlay(&mut live);
            live.overlay.show();
            live.overlay.update();
            *borrow = Some(OverlayState::Live(live));
        } else if let Some(live) = open_overlay_window(&pending, false) {
            *borrow = Some(OverlayState::Live(live));
        }
    });
}

/// Hide and close the overlay (or cancel a pending show).
pub fn hide() {
    OVERLAY.with(|cell| {
        let prev = cell.borrow_mut().take();
        if let Some(OverlayState::Live(mut live)) = prev {
            live.overlay.hide();
            CACHE.with(|cache| {
                cache.borrow_mut().insert(live.state.sequence.clone(), live);
            });
            debug!("Which-key overlay closed");
        }
    });
}

/// Drop cached overlays. Call when the active key tree changes.
pub fn clear_cache() {
    OVERLAY.with(|cell| {
        if let Some(OverlayState::Live(mut live)) = cell.borrow_mut().take() {
            live.overlay.close();
        }
    });
    CACHE.with(|cache| {
        for (_, mut live) in cache.borrow_mut().drain() {
            live.overlay.close();
        }
    });
}

/// Build and render the current which-key overlays hidden for instant display.
///
/// Loads the full prefix list synchronously. Callers running on the host
/// main thread (REAPER timer) should prefer [`enqueue_prewarm`] +
/// [`prewarm_step`] to spread surface allocation across many ticks rather
/// than building dozens of overlays in one frame.
pub fn prewarm_current_prefixes() {
    enqueue_prewarm();
    while prewarm_step() {}
}

/// Populate the prewarm queue from the active processor's prefix trees.
///
/// Cheap — only reads keybind state and copies overlay specs. Does not
/// allocate wgpu surfaces. Idempotent: re-enqueuing replaces the queue
/// rather than appending, so a stale queue from a previous profile is
/// dropped when the prefix set changes.
pub fn enqueue_prewarm() {
    let Ok(proc) = crate::input::processor::get_processor().read() else {
        return;
    };

    // Only prewarm depth-0 (root) overlays. Each subtree overlay costs ~80ms
    // on the host main thread to allocate (X11 window + wgpu surface
    // configure + Vello renderer + Dioxus VDom), and prewarming all of them
    // for a typical config (~40 specs) sums to ~3s of unavoidable main-thread
    // work. Roots cover the common case; subtrees fall through to lazy
    // creation on first navigation, then stay warm in CACHE for the session.
    let mut specs = Vec::new();
    for tree in proc.current_trees() {
        specs.push((
            crate::input::keybinds::bridge::translate_sequence(&tree.prefix),
            tree.label.clone(),
            entries_to_overlay_entries(&tree.entries),
        ));
    }

    PREWARM_QUEUE.with(|q| {
        *q.borrow_mut() = specs;
    });
}

/// Build at most one queued overlay. Returns `true` if there is more work
/// to do (caller should call again on the next tick), `false` if the queue
/// is drained.
///
/// Allocating a single overlay (wgpu surface + Vello renderer + Dioxus VDom)
/// can take tens of milliseconds; spreading across ticks keeps the host
/// responsive.
pub fn prewarm_step() -> bool {
    let next = PREWARM_QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        if q.is_empty() {
            None
        } else {
            Some(q.remove(0))
        }
    });

    let Some((sequence, header_label, entries)) = next else {
        return false;
    };

    if !overlay_is_cached_or_active(&sequence) {
        let pending = PendingOverlay {
            entries,
            sequence: sequence.clone(),
            header_label,
            requested_at: Instant::now(),
        };

        if let Some(live) = open_overlay_window(&pending, true) {
            CACHE.with(|cache| {
                cache.borrow_mut().insert(sequence, live);
            });
        }
    }

    PREWARM_QUEUE.with(|q| !q.borrow().is_empty())
}

/// Refresh the overlay. Called from the timer callback (~30fps).
///
/// Promotes pending → live after SHOW_DELAY_MS. Repositions and re-renders
/// live overlays.
pub fn refresh() {
    OVERLAY.with(|cell| {
        let mut borrow = cell.borrow_mut();

        #[allow(
            clippy::absurd_extreme_comparisons,
            reason = "SHOW_DELAY_MS is a tunable delay currently set to 0 by design (see its doc comment); the comparison stays here so raising the constant later just works"
        )]
        let should_promote = matches!(
            borrow.as_ref(),
            Some(OverlayState::Pending(p)) if p.requested_at.elapsed().as_millis() as u64 >= SHOW_DELAY_MS
        );

        if should_promote {
            if let Some(OverlayState::Pending(pending)) = borrow.take()
                && let Some(live) = open_overlay_window(&pending, false) {
                    *borrow = Some(OverlayState::Live(live));
                }
        } else if let Some(OverlayState::Live(live)) = borrow.as_mut() {
            reposition_overlay(live);
            live.overlay.update();
        }
    });
}

/// Check if the overlay window is currently open.
pub fn is_visible() -> bool {
    OVERLAY.with(|cell| matches!(*cell.borrow(), Some(OverlayState::Live(_))))
}

/// `true` when the overlay is currently live for the exact given sequence.
/// Cheap check (single TLS borrow + string compare) used by `handler` to
/// skip the redundant overlay refresh path while a held prefix is
/// auto-repeating.
pub fn is_showing_sequence(sequence: &str) -> bool {
    OVERLAY.with(|cell| {
        matches!(
            &*cell.borrow(),
            Some(OverlayState::Live(live)) if live.state.sequence == sequence,
        )
    })
}

// ---------------------------------------------------------------------------
// Window creation
// ---------------------------------------------------------------------------

fn take_cached_overlay(sequence: &str) -> Option<LiveOverlay> {
    CACHE.with(|cache| cache.borrow_mut().remove(sequence))
}

fn overlay_is_cached_or_active(sequence: &str) -> bool {
    let active = OVERLAY.with(|cell| {
        matches!(
            cell.borrow().as_ref(),
            Some(OverlayState::Live(live)) if live.state.sequence == sequence
        )
    });
    active || CACHE.with(|cache| cache.borrow().contains_key(sequence))
}

fn collect_overlay_specs(
    out: &mut Vec<(String, String, Vec<OverlayEntry>)>,
    sequence: &str,
    header_label: &str,
    entries: &[WhichKeyEntry],
) {
    out.push((
        sequence.to_string(),
        header_label.to_string(),
        entries_to_overlay_entries(entries),
    ));

    for entry in entries {
        if let WhichKeyEntry::Branch {
            key,
            label,
            children,
            ..
        } = entry
        {
            let child_sequence = format!(
                "{} {}",
                sequence,
                crate::input::keybinds::bridge::translate_sequence(key)
            );
            collect_overlay_specs(out, &child_sequence, label, children);
        }
    }
}

/// Compare two overlay entry lists for visual equivalence. Used by
/// `show_entries` to skip redundant rebuilds when OS auto-repeat delivers
/// the same prefix keydown many times in a row.
fn overlay_entries_match(a: &[OverlayEntry], b: &[OverlayEntry]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.key == y.key
            && x.label == y.label
            && x.is_branch == y.is_branch
            && x.available == y.available
    })
}

fn entries_to_overlay_entries(entries: &[WhichKeyEntry]) -> Vec<OverlayEntry> {
    entries
        .iter()
        .map(|entry| match entry {
            WhichKeyEntry::Leaf { key, label, .. } => OverlayEntry {
                key: key.clone(),
                label: label.clone(),
                is_branch: false,
                available: true,
            },
            WhichKeyEntry::Branch { key, label, .. } => OverlayEntry {
                key: key.clone(),
                label: label.clone(),
                is_branch: true,
                available: true,
            },
        })
        .collect()
}

fn open_overlay_window(pending: &PendingOverlay, start_hidden: bool) -> Option<LiveOverlay> {
    // Frame and render scale come from the same computation so the window
    // size always matches the component's inline styles (the fit-shrink pass
    // can lower the scale below the responsive baseline).
    let (x, y, width, height, scale) = compute_popup_layout(&pending.entries);
    crate::trace_console_msg(format!(
        "[DEBUG] WhichKey opening overlay: sequence='{}' frame=({}, {}, {}, {}) scale={:.2} entries={}\n",
        pending.sequence,
        x,
        y,
        width,
        height,
        scale,
        pending.entries.len()
    ));

    let state = WhichKeyState {
        entries: pending.entries.clone(),
        sequence: pending.sequence.clone(),
        header_label: pending.header_label.clone(),
        scale,
    };

    match DioxusOverlay::builder(WhichKeyComponent, x, y, width, height)
        .with_context(state.clone())
        .start_hidden(start_hidden)
        .auto_fit(true)
        .build()
    {
        Ok(overlay) => {
            crate::trace_console_msg(format!(
                "[DEBUG] WhichKey overlay build returned OK: sequence='{}'\n",
                pending.sequence
            ));
            debug!("Which-key overlay opened (dioxus-native)");
            crate::trace_console_msg(format!(
                "[DEBUG] WhichKey overlay opened: sequence='{}'\n",
                pending.sequence
            ));
            Some(LiveOverlay { overlay, state })
        }
        Err(e) => {
            warn!("Failed to open which-key overlay: {}", e);
            crate::trace_console_msg(format!(
                "[DEBUG] WhichKey overlay open failed: sequence='{}' error={}\n",
                pending.sequence, e
            ));
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

fn resolve_header_label(sequence: &str) -> String {
    if sequence.is_empty() {
        return String::new();
    }
    let proc = crate::input::processor::get_processor().read().unwrap();
    // The which-key trees are main-window menus; in an editor context the
    // sequence comes from `@Midi` bindings, so a main tree's label (e.g.
    // "Scroll" for the arrange `z` tree) would mislabel the editor menu.
    let in_editor = matches!(
        proc.reaper_context(),
        crate::input::keybinds::KeybindContext::Midi
            | crate::input::keybinds::KeybindContext::MidiInline
    );
    if in_editor {
        return sequence.to_string();
    }
    proc.current_trees()
        .iter()
        .find(|tree| {
            let normalized_prefix =
                crate::input::keybinds::bridge::translate_sequence(&tree.prefix);
            sequence == normalized_prefix || sequence.starts_with(&normalized_prefix)
        })
        .map(|tree| tree.label.clone())
        .unwrap_or_else(|| sequence.to_string())
}

/// Window bounds the overlay should anchor to: the active MIDI editor when
/// the current key context is an editor, otherwise the arrange view. Keeps
/// the popup (and its responsive scale) on the window the user is actually
/// working in instead of always hugging the arrange's bottom-right.
fn get_anchor_bounds() -> Option<(i32, i32, u32, u32)> {
    let in_editor = {
        let proc = crate::input::processor::get_processor().read().ok();
        proc.is_some_and(|p| {
            matches!(
                p.reaper_context(),
                crate::input::keybinds::KeybindContext::Midi
                    | crate::input::keybinds::KeybindContext::MidiInline
            )
        })
    };
    if in_editor && let Some(bounds) = get_midi_editor_bounds() {
        return Some(bounds);
    }
    get_arrange_bounds()
}

/// Screen bounds of the active MIDI editor's client area.
fn get_midi_editor_bounds() -> Option<(i32, i32, u32, u32)> {
    let reaper = Reaper::get();
    let editor_hwnd = reaper.medium_reaper().midi_editor_get_active()?;
    client_bounds_on_screen(editor_hwnd.as_ptr())
}

/// Screen-coordinate client bounds of a window (Win32-style top-left origin,
/// slightly inset).
fn client_bounds_on_screen(hwnd: reaper_low::raw::HWND) -> Option<(i32, i32, u32, u32)> {
    use reaper_low::raw::POINT;

    if hwnd.is_null() {
        return None;
    }
    let swell = Swell::get();

    let mut client_rect = RECT {
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetClientRect(hwnd, &mut client_rect);
    }

    let mut top_left = POINT {
        x: client_rect.left,
        y: client_rect.top,
    };
    unsafe {
        swell.ClientToScreen(hwnd, &mut top_left);
    }

    let width = (client_rect.right - client_rect.left).unsigned_abs();
    let height = (client_rect.bottom - client_rect.top).unsigned_abs();
    if width == 0 || height == 0 {
        return None;
    }

    Some((
        top_left.x,
        top_left.y,
        width.saturating_sub(16),
        height.saturating_sub(16),
    ))
}

fn get_arrange_bounds() -> Option<(i32, i32, u32, u32)> {
    use reaper_low::raw::POINT;

    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();
    let arrange_hwnd = get_arrange_wnd(medium_reaper)?;
    let swell = Swell::get();

    let mut client_rect = RECT {
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetClientRect(arrange_hwnd, &mut client_rect);
    }

    let mut top_left = POINT {
        x: client_rect.left,
        y: client_rect.top,
    };
    unsafe {
        swell.ClientToScreen(arrange_hwnd, &mut top_left);
    }

    let width = (client_rect.right - client_rect.left).unsigned_abs();
    let height = (client_rect.bottom - client_rect.top).unsigned_abs();

    let width = width.saturating_sub(16);
    let height = height.saturating_sub(16);

    // Always return Win32-style top-left-origin coordinates. The
    // platform window layer (daw-reaper-embed::TransparentWindow on
    // macOS) already does the Cocoa flip internally — flipping here
    // too lands the overlay at the bottom of the screen instead of the
    // bottom of the arrange view.
    #[cfg(target_os = "macos")]
    {
        let sh = reaper_embed::main_screen_height();
        tracing::info!(
            client_left = client_rect.left,
            client_top = client_rect.top,
            client_right = client_rect.right,
            client_bottom = client_rect.bottom,
            top_left_x = top_left.x,
            top_left_y = top_left.y,
            width,
            height,
            screen_height = sh,
            "[debug] get_arrange_bounds"
        );
    }
    Some((top_left.x, top_left.y, width, height))
}

/// Compute popup geometry — single column, dynamic size, bottom-right of arrange.
///
/// All dimensions are in logical points (CSS px). The underlying transparent
/// window layer is responsible for translating those into physical backing
/// pixels for the GPU surface, so the popup looks consistent across HiDPI
/// displays without any per-call scaling here.
fn compute_popup_geometry(entries: &[OverlayEntry]) -> (i32, i32, u32, u32) {
    let (x, y, w, h, _scale) = compute_popup_layout(entries);
    (x, y, w, h)
}

/// Compute the popup frame AND the render scale together so the predicted
/// window size and the component's inline styles always agree.
///
/// Scale starts from the monitor-based responsive factor, then shrinks (down
/// to `FIT_SCALE_FLOOR`) when the menu would be taller than the anchor
/// window — long menus stay fully visible inside a short docked-editor pane
/// instead of spilling over the arrange. Finally the frame is clamped into
/// the monitor's work area so it can never land off-screen.
fn compute_popup_layout(entries: &[OverlayEntry]) -> (i32, i32, u32, u32, f32) {
    let text_renderer = VelloTextRenderer::new();
    let mut scale = compute_responsive_scale() as f64;

    let entry_count = entries.len() as f64;
    let gaps = entries.len().saturating_sub(1) as f64;
    // Every term except the window chrome scales linearly.
    let scalable_height_at_1 =
        HEADER_HEIGHT + CONTENT_PADDING_Y + ROW_HEIGHT * entry_count + ROW_GAP * gaps;

    let anchor = get_anchor_bounds();

    // Shrink-to-fit: keep the whole menu inside the anchor window.
    if let Some((_, _, _, ah)) = anchor {
        let allowed = ah as f64 - 2.0 * MARGIN - WINDOW_CHROME_PAD;
        if allowed > 0.0 && scalable_height_at_1 * scale > allowed {
            scale = (allowed / scalable_height_at_1).max(FIT_SCALE_FLOOR);
        }
    }

    let scaled_font = (FONT_SIZE as f64 * scale) as f32;
    let mut max_entry_width = 0.0f64;
    for entry in entries {
        let prefix = if entry.is_branch { "+" } else { "" };
        let text = format!("{}  {}{}", entry.key, prefix, entry.label);
        let w = text_renderer.measure_text(&text, scaled_font);
        max_entry_width = max_entry_width.max(w);
    }

    let popup_width = (max_entry_width + PADDING_X * scale * 2.0 + WIDTH_PADDING * scale)
        .max(MIN_POPUP_WIDTH * scale) as u32;
    let popup_height = (scalable_height_at_1 * scale + WINDOW_CHROME_PAD)
        .ceil()
        .max(48.0) as u32;

    let margin = (MARGIN * scale) as i32;
    let (mut x, mut y) = if let Some((ax, ay, aw, ah)) = anchor {
        (
            ax + (aw as i32) - (popup_width as i32) - margin,
            ay + (ah as i32) - (popup_height as i32) - margin,
        )
    } else {
        (400, 400)
    };

    // Never off-screen: clamp into the monitor work area.
    if let Some((mx, my, mw, mh)) = monitor_bounds_for_anchor() {
        let max_x = mx + mw as i32 - popup_width as i32;
        let max_y = my + mh as i32 - popup_height as i32;
        x = x.clamp(mx.min(max_x), max_x.max(mx));
        y = y.clamp(my.min(max_y), max_y.max(my));
    }

    (x, y, popup_width, popup_height, scale as f32)
}

// ---------------------------------------------------------------------------
// Positioning
// ---------------------------------------------------------------------------

fn reposition_overlay(live: &mut LiveOverlay) {
    let (x, y, width, height) = compute_popup_geometry(&live.state.entries);

    live.overlay.set_frame(x, y, width, height);
}
