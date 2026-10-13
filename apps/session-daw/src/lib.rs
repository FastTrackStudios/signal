//! The Session DAW: the arrangement, the mixer and the panels around them.
//!
//! The arrangement is painted by one thing, [`widget::ArrangementWidget`],
//! mounted by [`panel::use_arrangement_panel`] and [`panel::PanelChrome`]
//! wherever the app shows it — the desktop shell on dioxus-native, the
//! browser through `web_host`. The headless tools (`bin/bench`,
//! `bin/blitz_shot`) draw through the same widget, so a benchmark or a
//! screenshot is a picture of the window rather than of a copy of it.

pub mod animate;
pub mod arrange_edit;
pub mod arrangement;
pub mod art;
pub mod audio_mode;
pub mod automation;
#[cfg(feature = "native")]
pub mod automation_actions;
pub mod automation_draw;
pub mod automation_edit;
#[cfg(feature = "native")]
pub mod automation_menu;
pub mod balance;
pub mod bar_step;
pub mod beats;
#[cfg(feature = "native")]
pub mod chart_editor;
pub mod chart_panel;
pub mod closeup;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab_bar;
#[cfg(feature = "native")]
pub mod collab_pointers;
#[cfg(feature = "native")]
pub mod comp;
pub mod compact;
pub mod cursor;
#[cfg(feature = "native")]
pub mod drum_group;
pub mod editor_view;
pub mod engine;
pub mod expression;
pub mod faces;
pub mod folder;
pub mod folder_item;
pub mod folds;
pub mod fps;
#[cfg(feature = "native")]
pub mod fx_actions;
pub mod fx_view;
pub mod gesture;
pub mod ghosts;
#[cfg(feature = "native")]
pub mod grow;
pub mod guide_instrument;
#[cfg(feature = "native")]
pub mod headless;
pub mod hit;
pub mod icons;
#[cfg(feature = "native")]
pub mod instrument_tracks;
pub mod jobs;
pub mod keys;
pub mod lanes;
pub mod layout;
pub mod leveler;
pub mod live;
pub mod loading;
pub mod lyrics_panel;
pub mod macros;
pub mod marks_strip;
pub mod mcp;
pub mod midi;
#[cfg(any(feature = "native", feature = "web"))]
pub mod mix_guide;
pub mod mixer_panel;
pub mod mousemap;
pub mod navigator;
pub mod notice;
pub mod num;
#[cfg(feature = "native")]
pub mod open;
pub mod open_core;
pub mod options;
#[cfg(feature = "native")]
pub mod organize;
pub mod play_return;
pub mod plugin_faces;
pub mod popup;
#[cfg(feature = "native")]
pub mod project_file;
#[cfg(feature = "native")]
pub mod project_new;
#[cfg(feature = "native")]
pub mod projects;
pub mod rig;
#[cfg(feature = "signal")]
pub mod rig_actions;
pub mod song_modes;
pub mod song_stream;
#[cfg(feature = "native")]
pub mod stream_in;
#[cfg(feature = "native")]
pub mod stream_set;
#[cfg(feature = "native")]
pub mod task_account;
#[cfg(any(feature = "native", feature = "web"))]
pub mod task_set;
pub mod track_regions;
#[cfg(feature = "native")]
pub mod versions;
#[cfg(feature = "native")]
pub mod vocal_level;
#[cfg(feature = "native")]
pub mod wave_pool;
/// On the web there is no in-process tokio runtime: the engine clients
/// (`engine::Transport`, `Applier`, `Meters`, …) find none and run
/// read-only, the way they do natively before a session is open. The web
/// host drives the engine its own way (`web_host`).
#[cfg(not(feature = "native"))]
pub mod open {
    pub use crate::open_core::*;

    /// Never constructed: [`runtime`] has none to give.
    pub struct Runtime(());

    impl Runtime {
        pub fn block_on<F: core::future::Future>(&self, _future: F) -> F::Output {
            unreachable!("no runtime is ever handed out on the web")
        }
    }

    #[must_use]
    pub fn runtime() -> Option<&'static Runtime> {
        None
    }

    /// The song on screen: a page shows one ([`set_current_song`]).
    #[must_use]
    pub fn current_song() -> Option<String> {
        CURRENT.read().ok().and_then(|slot| slot.clone())
    }

    static CURRENT: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

    /// The page shows `project` now.
    pub fn set_current_song(project: &str) {
        if let Ok(mut slot) = CURRENT.write() {
            *slot = Some(project.to_owned());
        }
    }

    /// Make `project_guid` the song on screen — what picking a setlist tab
    /// does; on a page, [`crate::web_engine::switch_song`].
    pub fn switch_song(project_guid: &str) {
        #[cfg(feature = "web")]
        crate::web_engine::switch_song(project_guid);
        #[cfg(not(feature = "web"))]
        set_current_song(project_guid);
    }

    /// What needs the local engine's own handle (as natively).
    #[derive(Clone, Copy, Debug)]
    pub enum LocalOnly {
        ChartRebuild,
        SessionSave,
        /// daw-transport-sync's per-buffer backend: a page's audio loop has
        /// none yet, so it follows a shared transport by the coarse
        /// commands (a play, a seek) rather than to the sample.
        TransportSync,
    }

    /// The local engine's handle, for what needs it — none on a page yet.
    pub fn with_local_engine<R>(
        _what: LocalOnly,
        _f: impl FnOnce(&daw::standalone::Standalone) -> R,
    ) -> Option<R> {
        None
    }
}
#[cfg(feature = "native")]
pub mod actions;
#[cfg(feature = "signal")]
pub mod bass_rig;
pub mod browser;
pub mod device_audio;
pub mod drum_edit;
#[cfg(feature = "native")]
pub mod drum_lanes;
pub mod drum_level;
pub mod drum_reference;
#[cfg(feature = "signal")]
pub mod drum_samples;
pub mod drum_settings;
pub mod fundamental;
pub mod gain_stage;
pub mod grid_in_force;
pub mod groove;
pub mod hit_overlay;
#[cfg(feature = "native")]
pub mod hits;
#[cfg(test)]
mod hook_rules;
pub mod kit_gate;
#[cfg(feature = "native")]
pub mod mcp_server;
pub mod mix_chains;
pub mod mix_export;
pub mod overlay;
pub mod panel;
#[cfg(feature = "native")]
pub mod patch_list;
pub mod plan;
pub mod poi;
pub mod pointer;
pub mod prepare;
pub mod profile;
pub mod progress;
#[cfg(feature = "native")]
pub mod quantize_panel;
pub mod rails;
#[cfg(feature = "native")]
pub mod record;
pub mod record_view;
pub mod reference;
#[cfg(feature = "native")]
pub mod reference_play;
pub mod rename;
pub mod routes;
pub mod row;
pub mod ruler;
pub mod section_colors;
#[cfg(feature = "native")]
pub mod session_file;
pub mod setlist;
pub mod settings;
pub mod setup;
pub mod shell;
pub mod simulate;
pub mod skin;
pub mod song_bars;
pub mod song_chart;
pub mod strip;
pub mod studio;
pub mod swipe;
pub mod take_rating;
pub mod take_window;
pub mod tcp;
pub mod text;
#[cfg(feature = "native")]
pub mod timeline_actions;
pub mod tone;
#[cfg(feature = "native")]
pub mod tone_bind;
pub mod tone_read;
pub mod tool;
pub mod toolbar;
pub mod touch;
pub mod track_icon;
pub mod transport_bar;
pub mod wave_detail;
#[cfg(feature = "web")]
pub mod web_audio;
#[cfg(feature = "web")]
pub mod web_engine;
#[cfg(feature = "web")]
pub mod web_host;
pub mod which_key;
pub mod widget;
pub mod window_cursor;
pub mod written;
pub mod zoom;
