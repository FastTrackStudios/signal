//! Frame layout model aligned with Figma auto layout semantics.

pub use figma_api::models::LayoutConstraint as Constraint;
pub use figma_api::models::LayoutGrid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLayoutMode {
    None,
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLayoutSizingMode {
    Fixed,
    Hug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLayoutPrimaryAlign {
    Min,
    Center,
    Max,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLayoutCounterAlign {
    Min,
    Center,
    Max,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLayoutPositioning {
    Auto,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLayoutWrap {
    NoWrap,
    Wrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLayoutAlignSelf {
    Auto,
    Min,
    Center,
    Max,
    Stretch,
    Inherit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AutoLayout {
    pub mode: Option<AutoLayoutMode>,
    pub primary_axis_sizing_mode: Option<AutoLayoutSizingMode>,
    pub counter_axis_sizing_mode: Option<AutoLayoutSizingMode>,
    pub primary_axis_align_items: Option<AutoLayoutPrimaryAlign>,
    pub counter_axis_align_items: Option<AutoLayoutCounterAlign>,
    pub item_spacing: Option<f64>,
    pub counter_axis_spacing: Option<f64>,
    pub padding_left: Option<f64>,
    pub padding_right: Option<f64>,
    pub padding_top: Option<f64>,
    pub padding_bottom: Option<f64>,
    pub wrap: Option<AutoLayoutWrap>,
    pub positioning: Option<AutoLayoutPositioning>,
    pub align_self: Option<AutoLayoutAlignSelf>,
    pub grow: Option<f64>,
    pub min_width: Option<f64>,
    pub max_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
}
