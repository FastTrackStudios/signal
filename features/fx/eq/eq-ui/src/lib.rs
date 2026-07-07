//! EQ GUI — Dioxus components for the EQ plugin.
//!
//! Hosts the EQ-specific Dioxus root component, the EQ graph viz, the
//! nice_plug parameter tree, and the bridging glue from `nice_plug` parameters
//! to [`fts_ui_audio`] widgets. EQ-specific visualizations live here. General-
//! purpose widgets (knobs, sliders, meters) come from [`fts_ui_audio`];
//! general layout primitives come from [`fts_ui`].
//!
//! - [`control_view`]: Pro-Q style spectrum analyzer with draggable band nodes
//! - [`eq_graph`] / [`eq_graph_painter`]: vello-rendered frequency-response graph
//! - [`profile_view`]: profile-driven layouts (Pultec knob layout, etc.)
//! - [`params`]: nice_plug parameter tree + shared UI state
//! - [`param_adapter`]: nice_plug `ParamPtr` → [`fts_ui_audio::ParamHandle`]

pub mod cheatsheet;
pub mod control_view;
pub mod eq_graph;
pub mod eq_graph_interaction;
pub mod eq_graph_model;
pub mod eq_graph_painter;
pub mod eq_graph_popup;
pub mod eq_graph_response;
pub mod eq_graph_svg;
pub mod param_adapter;
pub mod params;
pub mod profile_view;
