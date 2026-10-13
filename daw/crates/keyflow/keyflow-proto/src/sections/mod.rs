//! Sections Module
//!
//! Song section management and numbering

pub mod colors;
pub mod measure_expr;
pub mod numbering;
pub mod section;
pub mod section_type;

pub use colors::{SectionColors, colors_for_section_type};
pub use measure_expr::MeasureExpression;
pub use numbering::SectionNumberer;
pub use section::Section;
pub use section_type::{ParsedSection, SectionType};
