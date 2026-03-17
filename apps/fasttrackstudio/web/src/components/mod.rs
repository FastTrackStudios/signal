//! Web App Components
//!
//! Reusable UI components for the FastTrackStudio web app.

mod chart_editor;
mod chart_renderer;
mod source_viewer;
mod typewriter;

pub use chart_editor::{ChartEditor, DynamicChartRenderer, HighlightedEditor, PreviewMode, StaticChartRenderer};
pub use chart_renderer::{ChartRenderer, LayoutMode};
pub use source_viewer::SourceViewer;
pub use typewriter::{ChartTypewriter, Typewriter};
