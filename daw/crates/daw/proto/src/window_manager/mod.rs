//! WindowManager — named workspace layouts (toolbars + docking).

mod service;
mod types;

pub use service::*;
pub use types::{
    LayoutPlacement, LayoutToolbar, ModeDockerLayout, MonitorRect, WindowLayout,
    WindowLayoutOptions, WindowLayoutResult, WindowLayoutSummary,
};
