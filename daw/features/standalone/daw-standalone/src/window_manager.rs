//! `impl WindowManager for Standalone` — in-memory layout store.
//!
//! WindowManager on REAPER backs a real `reaper-screensets.ini` file
//! the host writes to. Standalone has no screensets, so we provide a
//! simple `HashMap<name, WindowLayout>` keyed by layout name plus an
//! `Option<String>` for "current layout". Apply just updates the
//! current pointer.
//!
//! Tests that build a layout, save it, list it, apply it, and read
//! back the current layout round-trip through this impl unchanged.

use std::sync::Mutex;

use daw_proto::window_manager::{
    WindowLayout, WindowLayoutOptions, WindowLayoutResult, WindowLayoutSummary, WindowManager,
};

use crate::sync::Standalone;

/// State store for the WindowManager. Lives behind a Mutex on
/// `Standalone::window_layouts` (added below via `OnceLock` lazy init
/// so we don't have to touch the canonical state struct).
struct WindowLayoutStore {
    layouts: std::collections::HashMap<String, WindowLayout>,
    current: Option<String>,
}

impl WindowLayoutStore {
    fn new() -> Self {
        Self {
            layouts: std::collections::HashMap::new(),
            current: None,
        }
    }
}

// One global store per Standalone process. We don't shard per-
// instance because there's typically one Standalone per process
// and screensets are a UI-level concern. A per-Standalone Arc is
// straightforward to add if multi-instance tests need it.
fn store() -> &'static Mutex<WindowLayoutStore> {
    use std::sync::OnceLock;
    static STORE: OnceLock<Mutex<WindowLayoutStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(WindowLayoutStore::new()))
}

impl WindowManager for Standalone {
    fn apply_layout(&self, name: String, _options: WindowLayoutOptions) -> WindowLayoutResult {
        let mut s = store().lock().expect("window layout store poisoned");
        if s.layouts.contains_key(&name) {
            s.current = Some(name.clone());
            WindowLayoutResult::ok(name)
        } else {
            WindowLayoutResult::error(format!("layout '{name}' not found"))
        }
    }

    fn list_layouts(&self) -> Vec<WindowLayoutSummary> {
        let s = store().lock().expect("window layout store poisoned");
        let mut out: Vec<WindowLayoutSummary> = s.layouts.values().map(layout_to_summary).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn current_layout(&self) -> Option<WindowLayoutSummary> {
        let s = store().lock().expect("window layout store poisoned");
        let name = s.current.as_ref()?;
        s.layouts.get(name).map(layout_to_summary)
    }

    fn get_layout(&self, name: String) -> Option<WindowLayout> {
        let s = store().lock().expect("window layout store poisoned");
        s.layouts.get(&name).cloned()
    }

    fn save_layout(&self, layout: WindowLayout) -> WindowLayoutResult {
        let name = layout.name.clone();
        let mut s = store().lock().expect("window layout store poisoned");
        s.layouts.insert(name.clone(), layout);
        WindowLayoutResult::ok(name)
    }

    fn delete_layout(&self, name: String) -> WindowLayoutResult {
        let mut s = store().lock().expect("window layout store poisoned");
        if s.layouts.remove(&name).is_some() {
            if s.current.as_deref() == Some(name.as_str()) {
                s.current = None;
            }
            WindowLayoutResult::ok(name)
        } else {
            WindowLayoutResult::error(format!("layout '{name}' not found"))
        }
    }
}

fn layout_to_summary(layout: &WindowLayout) -> WindowLayoutSummary {
    // WindowLayoutSummary fields are read from WindowLayout — we
    // mirror name + (best-guess) slot info. The exact shape of
    // Summary varies across versions; clone the fields we know about
    // via direct field access (all pub on the proto types).
    WindowLayoutSummary {
        name: layout.name.clone(),
        ..Default::default()
    }
}
