//! In-memory [`DockHosting`] adapter for unit tests.
//!
//! Records every call into a `Vec<DockOp>` so tests can assert exact
//! sequences ("show was called once for this handle", "register_dock for
//! id X happened exactly once", etc.) without spinning up REAPER, GPU
//! state, or any real window.
//!
//! Pulled in via `cargo features = ["test-utils"]` so production builds
//! never link this code.

use crate::dock_host::{DockEvent, DockHandle, DockHosting, DockKind, PanelPixels, UiEventDto};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Record of a single operation invoked against [`MockDockHost`].
///
/// Tests inspect the recorded sequence with [`MockDockHost::ops`].
#[derive(Debug, Clone, PartialEq)]
pub enum DockOp {
    Register {
        handle: DockHandle,
        id: String,
        title: String,
        kind: DockKind,
    },
    Unregister(DockHandle),
    Show(DockHandle),
    Hide(DockHandle),
    Toggle(DockHandle),
    SaveLayout,
    RestoreLayout,
    Subscribe,
    InjectUiEvent {
        handle: DockHandle,
        event: UiEventDto,
    },
}

/// Snapshot of one mock dock's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockDock {
    pub id: String,
    pub title: String,
    pub kind: DockKind,
    pub visible: bool,
}

/// Pure in-memory [`DockHosting`].
///
/// Stores docks in a `HashMap<DockHandle, MockDock>` and tracks every
/// operation in a `Vec<DockOp>` for assertion. Subscribers receive
/// [`DockEvent`]s synchronously through their `Tx<DockEvent>` (the
/// channel's own send loop runs the event home).
#[derive(Clone, Default)]
pub struct MockDockHost {
    inner: Arc<Mutex<MockState>>,
}

#[derive(Default)]
struct MockState {
    docks: HashMap<DockHandle, MockDock>,
    by_id: HashMap<String, DockHandle>,
    next_id: u64,
    ops: Vec<DockOp>,
    /// Layout blob set by `restore_layout`, returned by next `save_layout`.
    /// Not a faithful round-trip — just a stable shape so tests can assert
    /// "the blob the host gave back is the one we set."
    layout_blob: Vec<u8>,
    /// Stub pixel buffers per handle, set via [`MockDockHost::set_pixels`].
    pixels: HashMap<DockHandle, PanelPixels>,
}

impl MockDockHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of the recorded operation log. Cheap to call repeatedly.
    pub fn ops(&self) -> Vec<DockOp> {
        self.inner.lock().unwrap().ops.clone()
    }

    /// Reset the operation log without touching dock state. Useful in
    /// "given/when/then" tests where setup operations clutter the log.
    pub fn clear_ops(&self) {
        self.inner.lock().unwrap().ops.clear();
    }

    /// Snapshot a dock by handle, or `None` if not registered.
    pub fn dock(&self, handle: DockHandle) -> Option<MockDock> {
        self.inner.lock().unwrap().docks.get(&handle).cloned()
    }

    /// Look up a dock by string id (the form callers passed to
    /// `register_dock`). Returns the assigned handle.
    pub fn handle_for(&self, id: &str) -> Option<DockHandle> {
        self.inner.lock().unwrap().by_id.get(id).copied()
    }

    /// Pre-load a stub pixel buffer the next call to
    /// [`capture_panel_pixels`](DockHosting::capture_panel_pixels)
    /// will return for `handle`.
    pub fn set_pixels(&self, handle: DockHandle, pixels: PanelPixels) {
        self.inner.lock().unwrap().pixels.insert(handle, pixels);
    }

    /// Number of currently-visible docks.
    pub fn visible_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap()
            .docks
            .values()
            .filter(|d| d.visible)
            .count()
    }

    fn record(state: &mut MockState, op: DockOp) {
        state.ops.push(op);
    }

    /// Mock does not actually deliver events — daw-proto stays
    /// `tokio`-free for WASM compatibility, so we don't bring in a
    /// broadcast channel here. Tests that need delivery should assert on
    /// [`MockDockHost::ops`] / [`MockDockHost::dock`] state, or wrap a
    /// real adapter (e.g. `ReaperDockHost`) instead.
    fn emit(_state: &MockState, _event: DockEvent) {}
}

impl DockHosting for MockDockHost {
    fn register_dock(&self, id: &str, title: &str, kind: DockKind) -> DockHandle {
        let mut st = self.inner.lock().unwrap();
        if let Some(&existing) = st.by_id.get(id) {
            Self::record(
                &mut st,
                DockOp::Register {
                    handle: existing,
                    id: id.to_string(),
                    title: title.to_string(),
                    kind,
                },
            );
            return existing;
        }
        st.next_id += 1;
        let handle = DockHandle(st.next_id);
        st.docks.insert(
            handle,
            MockDock {
                id: id.to_string(),
                title: title.to_string(),
                kind,
                visible: false,
            },
        );
        st.by_id.insert(id.to_string(), handle);
        Self::record(
            &mut st,
            DockOp::Register {
                handle,
                id: id.to_string(),
                title: title.to_string(),
                kind,
            },
        );
        handle
    }

    fn unregister_dock(&self, handle: DockHandle) -> bool {
        let mut st = self.inner.lock().unwrap();
        Self::record(&mut st, DockOp::Unregister(handle));
        if let Some(dock) = st.docks.remove(&handle) {
            st.by_id.remove(&dock.id);
            true
        } else {
            false
        }
    }

    fn show(&self, handle: DockHandle) {
        let mut st = self.inner.lock().unwrap();
        Self::record(&mut st, DockOp::Show(handle));
        if let Some(dock) = st.docks.get_mut(&handle) {
            dock.visible = true;
            Self::emit(&st, DockEvent::Shown(handle));
        }
    }

    fn hide(&self, handle: DockHandle) {
        let mut st = self.inner.lock().unwrap();
        Self::record(&mut st, DockOp::Hide(handle));
        if let Some(dock) = st.docks.get_mut(&handle) {
            dock.visible = false;
            Self::emit(&st, DockEvent::Hidden(handle));
        }
    }

    fn toggle(&self, handle: DockHandle) -> bool {
        let mut st = self.inner.lock().unwrap();
        Self::record(&mut st, DockOp::Toggle(handle));
        let visible = if let Some(dock) = st.docks.get_mut(&handle) {
            dock.visible = !dock.visible;
            dock.visible
        } else {
            return false;
        };
        Self::emit(
            &st,
            if visible {
                DockEvent::Shown(handle)
            } else {
                DockEvent::Hidden(handle)
            },
        );
        visible
    }

    fn is_visible(&self, handle: DockHandle) -> bool {
        self.inner
            .lock()
            .unwrap()
            .docks
            .get(&handle)
            .is_some_and(|d| d.visible)
    }

    fn save_layout(&self) -> Vec<u8> {
        let mut st = self.inner.lock().unwrap();
        Self::record(&mut st, DockOp::SaveLayout);
        st.layout_blob.clone()
    }

    fn restore_layout(&self, blob: &[u8]) -> bool {
        let mut st = self.inner.lock().unwrap();
        Self::record(&mut st, DockOp::RestoreLayout);
        st.layout_blob = blob.to_vec();
        Self::emit(&st, DockEvent::LayoutChanged);
        true
    }

    fn capture_panel_pixels(&self, handle: DockHandle) -> Option<PanelPixels> {
        self.inner.lock().unwrap().pixels.get(&handle).cloned()
    }

    fn inject_ui_event(&self, handle: DockHandle, event: UiEventDto) -> bool {
        let mut st = self.inner.lock().unwrap();
        let known = st.docks.contains_key(&handle);
        Self::record(&mut st, DockOp::InjectUiEvent { handle, event });
        known
    }
}
