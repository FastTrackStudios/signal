//! Layout components — sections, headers, status bars, control groups.

pub mod header;
pub mod section;

pub use header::{Header, StatusBar};
pub use section::{ActionButton, ControlGroup, Divider, Section, SectionLabel};
