//! Layout Components
//!
//! Complete page layouts that apps can use directly.
//! These are higher-level components that combine smart components,
//! presentation components, and signals into full-page views.

pub mod performance;
pub mod top_bar;

pub use performance::{PerformanceLayout, PerformanceSidebar, TransportPanel};
pub use top_bar::{ConnectionState, VERSION};
