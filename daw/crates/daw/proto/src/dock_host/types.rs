//! Dock host data types — handles, events, kinds, pixel buffers.

use facet::Facet;

/// Opaque handle returned by `register_dock`. Stable for the lifetime
/// of the registration; callers pass it back to show/hide/is_visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct DockHandle(pub u64);

/// Events emitted by the dock host as the user manipulates docks.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum DockEvent {
    Shown(DockHandle),
    Hidden(DockHandle),
    /// User moved or resized a dock; layout has been persisted.
    LayoutChanged,
}

/// Synthetic UI event injected into a panel for interaction tests.
///
/// Coordinates are panel-local pixels (origin at the top-left of the
/// dock window's client area).
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
pub enum UiEventDto {
    PointerMove {
        x: f32,
        y: f32,
    },
    PointerDown {
        x: f32,
        y: f32,
        /// 0 = main (left), 1 = aux (middle), 2 = secondary (right).
        button: u8,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: u8,
    },
    Wheel {
        x: f32,
        y: f32,
        delta_x: f64,
        delta_y: f64,
    },
    KeyDown {
        /// `keyboard_types::Key` rendered as a string.
        key: String,
    },
    KeyUp {
        key: String,
    },
}

/// Pixel buffer captured from a live dock panel.
///
/// `bgra` is BGRA8, length must equal `width * height * 4`.
#[derive(Debug, Clone, Facet)]
pub struct PanelPixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Hint about the kind of host window the dock should produce.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
pub enum DockKind {
    #[default]
    Tabbed,
    Floating,
    Embedded,
}
