//! Default dock presets — built-in screensets that ship with the app.

use crate::builder::DockLayoutBuilder as B;
use crate::panel::PanelId::*;
use crate::preset::{DockPreset, PresetCollection};

/// Create the default dock presets.
///
/// These correspond to screensets (like REAPER's docking system):
/// - **Performance** (F5): Navigator + [Performance | ChartPreview] tabbed + Transport
/// - **Chart Focus** (F6): Navigator + [ChartPreview | Performance] tabbed + Transport
/// - **Edit** (F7): Setlist + ChartEditor / Transport
/// - **Mixer** (F8): FX chain tree + DAW mixer + FX parameter browser
/// - **Full** (F9): Multi-panel overview with all key views
pub fn default_presets() -> PresetCollection {
    PresetCollection::new(vec![
        // F5: Performance — navigator sidebar + [Performance | ChartPreview] tabbed + transport
        DockPreset::builtin(
            "Performance",
            B::horizontal()
                .left(B::tile(Navigator))
                .right(
                    B::vertical()
                        .top(B::tabbed(vec![Performance, ChartPreview]))
                        .bottom(B::tile(Transport))
                        .ratio(75.0)
                        .build_node(),
                )
                .ratio(20.0)
                .build(),
        )
        .with_hotkey("F5"),
        // F6: Chart Focus — navigator sidebar + [ChartPreview | Performance] tabbed + transport
        DockPreset::builtin(
            "Chart Focus",
            B::horizontal()
                .left(B::tile(Navigator))
                .right(
                    B::vertical()
                        .top(B::tabbed(vec![ChartPreview, Performance]))
                        .bottom(B::tile(Transport))
                        .ratio(75.0)
                        .build_node(),
                )
                .ratio(20.0)
                .build(),
        )
        .with_hotkey("F6"),
        // F7: Edit — setlist sidebar + chart editor with transport below
        DockPreset::builtin(
            "Edit",
            B::horizontal()
                .left(B::tile(Setlist))
                .right(
                    B::vertical()
                        .top(B::tile(ChartEditor))
                        .bottom(B::tile(Transport))
                        .ratio(85.0)
                        .build_node(),
                )
                .ratio(20.0)
                .build(),
        )
        .with_hotkey("F7"),
        // F8: Mixer — FX chain tree + DAW mixer + FX parameter browser
        DockPreset::builtin(
            "Mixer",
            B::horizontal()
                .left(B::tile(FxChainTree))
                .right(
                    B::horizontal()
                        .left(B::tile(Mixer))
                        .right(B::tile(FxBrowser))
                        .ratio(70.0)
                        .build_node(),
                )
                .ratio(20.0)
                .build(),
        )
        .with_hotkey("F8"),
        // F9: DAW — track control panel + arrangement + mixer
        DockPreset::builtin(
            "DAW",
            B::vertical()
                .top(
                    B::horizontal()
                        .left(B::tile(TrackControlPanel))
                        .right(B::tile(ArrangementView))
                        .ratio(15.0)
                        .build_node(),
                )
                .bottom(B::tile(Mixer))
                .ratio(75.0)
                .build(),
        )
        .with_hotkey("F9"),
        // F10: Full — multi-panel overview with all key views
        DockPreset::builtin(
            "Full",
            B::horizontal()
                .left(
                    B::vertical()
                        .top(B::tabbed(vec![ChartPreview, Performance]))
                        .bottom(
                            B::horizontal()
                                .left(B::tile(Navigator))
                                .right(B::tile(Transport))
                                .ratio(50.0)
                                .build_node(),
                        )
                        .ratio(70.0)
                        .build_node(),
                )
                .right(
                    B::vertical()
                        .top(B::tile(ChartEditor))
                        .bottom(B::tile(RigGrid))
                        .ratio(60.0)
                        .build_node(),
                )
                .ratio(50.0)
                .build(),
        )
        .with_hotkey("F10"),
    ])
}

/// Rig-specific dock presets — screensets for the rig tab's own dock system.
pub fn rig_presets() -> PresetCollection {
    PresetCollection::new(vec![
        // Rig Edit: Browser + Graph/Grid + Detail editor below
        DockPreset::builtin(
            "Edit",
            B::vertical()
                .top(
                    B::horizontal()
                        .left(B::tile(PresetBrowser))
                        .right(B::tabbed(vec![RigNodeGraph, RigGridEditor]))
                        .ratio(20.0)
                        .build_node(),
                )
                .bottom(B::tile(RigDetailEditor))
                .ratio(70.0)
                .build(),
        ),
        // Rig Compact: Browser + Graph/Grid (no detail editor)
        DockPreset::builtin(
            "Compact",
            B::horizontal()
                .left(B::tile(PresetBrowser))
                .right(B::tabbed(vec![RigNodeGraph, RigGridEditor]))
                .ratio(20.0)
                .build(),
        ),
        // Rig Full: Browser + Graph/Grid + Detail editor + Songs sidebar
        DockPreset::builtin(
            "Full",
            B::vertical()
                .top(
                    B::horizontal()
                        .left(B::tile(PresetBrowser))
                        .right(
                            B::horizontal()
                                .left(B::tabbed(vec![RigNodeGraph, RigGridEditor]))
                                .right(B::tabbed(vec![SongParts, SongSelector]))
                                .ratio(75.0)
                                .build_node(),
                        )
                        .ratio(20.0)
                        .build_node(),
                )
                .bottom(B::tile(RigDetailEditor))
                .ratio(70.0)
                .build(),
        ),
    ])
}
