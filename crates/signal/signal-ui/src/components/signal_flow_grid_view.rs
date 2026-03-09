//! Interactive signal flow grid view — renders blocks on a CSS grid with
//! routing connections, widget visualizations, bypass toggles, and an
//! "Add Block" button.
//!
//! Ported from legacy `signal-ui/components/rig_grid/signal_flow_grid.rs`.
//! Uses `signal::BlockType` and `audio_controls` widgets directly.

use dioxus::prelude::*;
use uuid::Uuid;

use signal::BlockType;

use super::grid_model::{BlockWidget, GridBlock, GridJack, SignalFlowGrid, GRID_COLS, GRID_ROWS};

use audio_controls::widgets::{
    CompressorGraph, CompressorParams, EqBand, EqBandShape, EqGraph, GateGraph, GateParams, Knob,
};

/// Cell size in pixels for the grid.
const CELL_SIZE: u32 = 64;
/// Gap between cells in pixels.
const CELL_GAP: u32 = 4;

/// Data for building a grid from engine-level data.
#[derive(Clone, PartialEq)]
pub struct EngineGridData {
    pub name: String,
    pub layers: Vec<LayerGridData>,
}

#[derive(Clone, PartialEq)]
pub struct LayerGridData {
    pub name: String,
    pub module_chains: Vec<ModuleChainGridData>,
}

#[derive(Clone, PartialEq)]
pub struct ModuleChainGridData {
    pub name: String,
    pub chain: signal::SignalChain,
}

// region: --- SignalFlowGridView

#[component]
pub fn SignalFlowGridView(
    /// The grid data to render.
    grid: SignalFlowGrid,
    /// Callback when a block is clicked.
    #[props(default)]
    on_block_click: Option<Callback<Uuid>>,
    /// Callback when a block's bypass is toggled.
    #[props(default)]
    on_block_bypass: Option<Callback<Uuid>>,
    /// Callback when a new block type is selected from the add modal.
    #[props(default)]
    on_add_block: Option<Callback<BlockType>>,
) -> Element {
    let mut show_add_modal = use_signal(|| false);

    let total_width = GRID_COLS as u32 * (CELL_SIZE + CELL_GAP) + CELL_GAP;
    let total_height = GRID_ROWS as u32 * (CELL_SIZE + CELL_GAP) + CELL_GAP;

    let col_template = format!("repeat({GRID_COLS}, {CELL_SIZE}px)");
    let row_template = format!("repeat({GRID_ROWS}, {CELL_SIZE}px)");

    rsx! {
        div { class: "relative w-full overflow-auto",
            // Grid container
            div {
                class: "relative mx-auto",
                style: "width: {total_width}px; min-height: {total_height}px;",

                // I/O jacks — left side (inputs)
                div {
                    class: "absolute left-0 top-0 bottom-0 w-8 flex flex-col justify-start gap-2 pt-2",
                    for jack in &grid.inputs {
                        JackLabel { jack: jack.clone() }
                    }
                }

                // I/O jacks — right side (outputs)
                div {
                    class: "absolute right-0 top-0 bottom-0 w-8 flex flex-col justify-start gap-2 pt-2",
                    for jack in &grid.outputs {
                        JackLabel { jack: jack.clone() }
                    }
                }

                // CSS Grid layout for blocks
                div {
                    class: "inline-grid",
                    style: "grid-template-columns: {col_template}; \
                            grid-template-rows: {row_template}; \
                            gap: {CELL_GAP}px; \
                            padding: {CELL_GAP}px;",

                    for block in &grid.blocks {
                        GridBlockCell {
                            block: block.clone(),
                            on_click: on_block_click.clone(),
                            on_bypass: on_block_bypass.clone(),
                        }
                    }
                }

                // Add block button (bottom-right corner)
                if on_add_block.is_some() {
                    div {
                        class: "absolute bottom-2 right-10",
                        button {
                            class: "flex items-center gap-1.5 px-3 py-1.5 rounded-lg \
                                    bg-zinc-800 hover:bg-zinc-700 border border-zinc-600 \
                                    text-zinc-300 hover:text-zinc-100 text-xs font-medium \
                                    transition-all shadow-lg",
                            onclick: move |_| show_add_modal.set(true),
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M12 4v16m8-8H4",
                                }
                            }
                            "Add Block"
                        }
                    }
                }
            }

            // Module browser modal
            if let Some(ref cb) = on_add_block {
                {
                    let cb = cb.clone();
                    rsx! {
                        ModuleBrowserModal {
                            is_open: show_add_modal(),
                            on_close: move |_| show_add_modal.set(false),
                            on_add_module: move |bt: BlockType| {
                                show_add_modal.set(false);
                                cb.call(bt);
                            },
                        }
                    }
                }
            }
        }
    }
}

// endregion: --- SignalFlowGridView

// region: --- Jack label

#[component]
fn JackLabel(jack: GridJack) -> Element {
    let row_offset = jack.row as u32 * (CELL_SIZE + CELL_GAP) + CELL_GAP;
    let side_class = if jack.is_input { "left-0" } else { "right-0" };

    rsx! {
        div {
            class: "absolute {side_class} flex items-center",
            style: "top: {row_offset}px; height: {CELL_SIZE}px;",
            div {
                class: "px-1 py-0.5 text-[10px] font-mono text-zinc-400 \
                        bg-zinc-800/80 rounded whitespace-nowrap",
                "{jack.label}"
            }
        }
    }
}

// endregion: --- Jack label

// region: --- GridBlockCell

#[component]
fn GridBlockCell(
    block: GridBlock,
    #[props(default)] on_click: Option<Callback<Uuid>>,
    #[props(default)] on_bypass: Option<Callback<Uuid>>,
) -> Element {
    let col_start = block.position.col + 1;
    let col_end = col_start + block.size.width;
    let row_start = block.position.row + 1;
    let row_end = row_start + block.size.height;

    let color = block.block_type.color();
    let (bg, fg, border) = if block.bypassed {
        ("#27272A", "#71717A", "#3F3F46") // zinc-800/zinc-500/zinc-700
    } else {
        (color.bg, color.fg, color.border)
    };

    let block_id = block.id;

    let px_w = block.size.width as u32 * CELL_SIZE
        + (block.size.width as u32).saturating_sub(1) * CELL_GAP;
    let px_h = block.size.height as u32 * CELL_SIZE
        + (block.size.height as u32).saturating_sub(1) * CELL_GAP;

    rsx! {
        div {
            class: "relative rounded-lg border-2 overflow-hidden cursor-pointer \
                    hover:brightness-110 transition-all duration-150 flex flex-col",
            style: "grid-column: {col_start} / {col_end}; \
                    grid-row: {row_start} / {row_end}; \
                    background-color: {bg}; color: {fg}; border-color: {border};",
            onclick: move |_| {
                if let Some(cb) = &on_click {
                    cb.call(block_id);
                }
            },

            BlockHeader {
                name: block.name.clone(),
                short_label: block.short_label.clone(),
                bypassed: block.bypassed,
                block_id,
                on_bypass: on_bypass.clone(),
                compact: block.size.width == 1 && block.size.height == 1,
            }

            div { class: "flex-1 min-h-0 overflow-hidden",
                BlockContent {
                    widget: block.widget,
                    width: px_w,
                    height: px_h.saturating_sub(24),
                    bypassed: block.bypassed,
                }
            }
        }
    }
}

// endregion: --- GridBlockCell

// region: --- Block header

#[component]
fn BlockHeader(
    name: String,
    short_label: String,
    bypassed: bool,
    block_id: Uuid,
    #[props(default)] on_bypass: Option<Callback<Uuid>>,
    #[props(default)] compact: bool,
) -> Element {
    if compact {
        rsx! {
            div {
                class: "flex items-center justify-center h-full text-xs font-bold select-none",
                style: if bypassed { "opacity: 0.5;" } else { "" },
                "{short_label}"
            }
        }
    } else {
        rsx! {
            div {
                class: "flex items-center justify-between px-2 py-1 \
                        text-[11px] font-semibold select-none",
                span {
                    style: if bypassed { "opacity: 0.5;" } else { "" },
                    "{name}"
                }
                button {
                    class: "w-2.5 h-2.5 rounded-full border border-current/30 \
                            hover:scale-125 transition-transform",
                    style: if bypassed {
                        "background: transparent; opacity: 0.4;"
                    } else {
                        "background: currentColor;"
                    },
                    title: if bypassed { "Enable" } else { "Bypass" },
                    onclick: move |e| {
                        e.stop_propagation();
                        if let Some(cb) = &on_bypass {
                            cb.call(block_id);
                        }
                    },
                }
            }
        }
    }
}

// endregion: --- Block header

// region: --- Block content (widget routing)

#[component]
fn BlockContent(widget: BlockWidget, width: u32, height: u32, bypassed: bool) -> Element {
    if bypassed {
        return rsx! {
            div { class: "flex items-center justify-center h-full text-xs opacity-30",
                "BYPASSED"
            }
        };
    }

    let has_room = width >= 100 && height >= 60;

    match widget {
        BlockWidget::EqGraph if has_room => rsx! { EqGraphBlock {} },
        BlockWidget::CompressorGraph if has_room => rsx! { CompressorGraphBlock {} },
        BlockWidget::GateGraph if has_room => rsx! { GateGraphBlock {} },
        BlockWidget::AmpCab if has_room => rsx! { AmpCabBlock {} },
        BlockWidget::DelayGraph if has_room => rsx! { TimeEffectBlock { label: "DLY" } },
        BlockWidget::ReverbGraph if has_room => rsx! { TimeEffectBlock { label: "REV" } },
        BlockWidget::ModulationGraph if has_room => rsx! { ModulationBlock {} },
        BlockWidget::DriveGraph if has_room => rsx! { DriveBlock {} },
        BlockWidget::Tuner => rsx! { TunerBlock {} },
        BlockWidget::Looper => rsx! { LooperBlock {} },
        _ => rsx! {},
    }
}

// endregion: --- Block content

// region: --- Widget blocks

#[component]
fn EqGraphBlock() -> Element {
    let bands = use_signal(|| {
        vec![
            EqBand {
                index: 0,
                used: true,
                enabled: true,
                frequency: 100.0,
                gain: 3.0,
                q: 0.7,
                shape: EqBandShape::LowShelf,
                ..Default::default()
            },
            EqBand {
                index: 1,
                used: true,
                enabled: true,
                frequency: 800.0,
                gain: -2.0,
                q: 1.4,
                shape: EqBandShape::Bell,
                ..Default::default()
            },
            EqBand {
                index: 2,
                used: true,
                enabled: true,
                frequency: 3500.0,
                gain: 4.0,
                q: 1.0,
                shape: EqBandShape::Bell,
                ..Default::default()
            },
            EqBand {
                index: 3,
                used: true,
                enabled: true,
                frequency: 10000.0,
                gain: -1.5,
                q: 0.7,
                shape: EqBandShape::HighShelf,
                ..Default::default()
            },
        ]
    });

    rsx! {
        div {
            class: "p-1 h-full",
            EqGraph {
                bands,
                show_freq_labels: false,
                show_db_labels: false,
            }
        }
    }
}

#[component]
fn CompressorGraphBlock() -> Element {
    let params = CompressorParams::default();

    rsx! {
        div {
            class: "flex items-center justify-center h-full p-1",
            CompressorGraph {
                params,
                show_grid: false,
                show_gr_meter: false,
                show_levels: false,
            }
        }
    }
}

#[component]
fn GateGraphBlock() -> Element {
    let params = use_signal(GateParams::default);

    rsx! {
        div {
            class: "flex items-center justify-center h-full p-1",
            GateGraph {
                params,
                show_grid: false,
                show_gr_meter: false,
            }
        }
    }
}

#[component]
fn AmpCabBlock() -> Element {
    let mut gain = use_signal(|| 0.5f32);
    let mut tone = use_signal(|| 0.5f32);

    rsx! {
        div {
            class: "flex flex-col items-center justify-center h-full gap-1 p-1",
            div { class: "flex gap-2",
                Knob { value: gain, size: 32, label: Some("Gain".to_string()),
                    on_change: move |v| gain.set(v),
                }
                Knob { value: tone, size: 32, label: Some("Tone".to_string()),
                    on_change: move |v| tone.set(v),
                }
            }
        }
    }
}

#[component]
fn DriveBlock() -> Element {
    let mut drive = use_signal(|| 0.4f32);

    rsx! {
        div {
            class: "flex flex-col items-center justify-center h-full gap-1 p-1",
            Knob { value: drive, size: 40, label: Some("Drive".to_string()),
                on_change: move |v| drive.set(v),
            }
        }
    }
}

#[component]
fn ModulationBlock() -> Element {
    let mut rate = use_signal(|| 0.3f32);
    let mut depth = use_signal(|| 0.5f32);

    rsx! {
        div {
            class: "flex items-center justify-center h-full gap-2 p-1",
            Knob { value: rate, size: 28, label: Some("Rate".to_string()),
                on_change: move |v| rate.set(v),
            }
            Knob { value: depth, size: 28, label: Some("Depth".to_string()),
                on_change: move |v| depth.set(v),
            }
        }
    }
}

#[component]
fn TimeEffectBlock(label: &'static str) -> Element {
    let mut time = use_signal(|| 0.3f32);
    let mut mix = use_signal(|| 0.3f32);

    rsx! {
        div {
            class: "flex items-center justify-center h-full gap-2 p-1",
            Knob { value: time, size: 28, label: Some("Time".to_string()),
                on_change: move |v| time.set(v),
            }
            Knob { value: mix, size: 28, label: Some("Mix".to_string()),
                on_change: move |v| mix.set(v),
            }
        }
    }
}

#[component]
fn TunerBlock() -> Element {
    rsx! {
        div { class: "flex items-center justify-center h-full text-[10px] font-mono",
            "A 440"
        }
    }
}

#[component]
fn LooperBlock() -> Element {
    rsx! {
        div { class: "flex items-center justify-center h-full gap-1 text-[10px]",
            button { class: "px-1 py-0.5 rounded bg-black/20 hover:bg-black/30", "REC" }
            button { class: "px-1 py-0.5 rounded bg-black/20 hover:bg-black/30", "PLAY" }
        }
    }
}

// endregion: --- Widget blocks

// region: --- ModuleBrowserModal

/// Module category for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModuleCategory {
    #[default]
    All,
    Drive,
    Amp,
    Modulation,
    Time,
    Utility,
}

impl ModuleCategory {
    pub const ALL: &[ModuleCategory] = &[
        ModuleCategory::All,
        ModuleCategory::Drive,
        ModuleCategory::Amp,
        ModuleCategory::Modulation,
        ModuleCategory::Time,
        ModuleCategory::Utility,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Drive => "Drive",
            Self::Amp => "Amp & Cab",
            Self::Modulation => "Modulation",
            Self::Time => "Time",
            Self::Utility => "Utility",
        }
    }

    pub fn matches(&self, block_type: BlockType) -> bool {
        match self {
            Self::All => true,
            Self::Drive => matches!(block_type, BlockType::Drive | BlockType::Saturator),
            Self::Amp => matches!(block_type, BlockType::Amp | BlockType::Cabinet),
            Self::Modulation => matches!(
                block_type,
                BlockType::Modulation | BlockType::Pitch | BlockType::Trem
            ),
            Self::Time => matches!(
                block_type,
                BlockType::Delay | BlockType::Reverb | BlockType::Freeze
            ),
            Self::Utility => matches!(
                block_type,
                BlockType::Compressor
                    | BlockType::Eq
                    | BlockType::Gate
                    | BlockType::Limiter
                    | BlockType::Volume
                    | BlockType::DeEsser
                    | BlockType::Tuner
                    | BlockType::Send
            ),
        }
    }
}

/// Module info for display in the browser.
#[derive(Debug, Clone, PartialEq)]
struct ModuleInfo {
    name: &'static str,
    description: &'static str,
    block_type: BlockType,
    icon: &'static str,
}

fn get_all_modules() -> Vec<ModuleInfo> {
    vec![
        // Drive
        ModuleInfo {
            name: "Overdrive",
            description: "Classic tube-style overdrive",
            block_type: BlockType::Drive,
            icon: "fire",
        },
        ModuleInfo {
            name: "Distortion",
            description: "High-gain distortion pedal",
            block_type: BlockType::Drive,
            icon: "zap",
        },
        ModuleInfo {
            name: "Fuzz",
            description: "Vintage fuzz tones",
            block_type: BlockType::Drive,
            icon: "boom",
        },
        ModuleInfo {
            name: "Saturator",
            description: "Tape-style saturation",
            block_type: BlockType::Saturator,
            icon: "tape",
        },
        // Amp & Cab
        ModuleInfo {
            name: "Amp",
            description: "Guitar amplifier simulation",
            block_type: BlockType::Amp,
            icon: "amp",
        },
        ModuleInfo {
            name: "Cabinet",
            description: "Speaker cabinet IR",
            block_type: BlockType::Cabinet,
            icon: "cab",
        },
        // Modulation
        ModuleInfo {
            name: "Chorus",
            description: "Lush chorus effect",
            block_type: BlockType::Modulation,
            icon: "wave",
        },
        ModuleInfo {
            name: "Phaser",
            description: "Classic phaser sweeps",
            block_type: BlockType::Modulation,
            icon: "phase",
        },
        ModuleInfo {
            name: "Flanger",
            description: "Jet-like flanging",
            block_type: BlockType::Modulation,
            icon: "jet",
        },
        ModuleInfo {
            name: "Tremolo",
            description: "Amplitude modulation",
            block_type: BlockType::Trem,
            icon: "trem",
        },
        ModuleInfo {
            name: "Pitch Shift",
            description: "Pitch shifting and harmony",
            block_type: BlockType::Pitch,
            icon: "pitch",
        },
        // Time
        ModuleInfo {
            name: "Delay",
            description: "Digital delay with tap tempo",
            block_type: BlockType::Delay,
            icon: "delay",
        },
        ModuleInfo {
            name: "Reverb",
            description: "Room, hall, and plate reverbs",
            block_type: BlockType::Reverb,
            icon: "reverb",
        },
        ModuleInfo {
            name: "Freeze",
            description: "Infinite sustain/freeze effect",
            block_type: BlockType::Freeze,
            icon: "freeze",
        },
        // Utility
        ModuleInfo {
            name: "Compressor",
            description: "Dynamic range compression",
            block_type: BlockType::Compressor,
            icon: "comp",
        },
        ModuleInfo {
            name: "EQ",
            description: "Parametric equalizer",
            block_type: BlockType::Eq,
            icon: "eq",
        },
        ModuleInfo {
            name: "Gate",
            description: "Noise gate",
            block_type: BlockType::Gate,
            icon: "gate",
        },
        ModuleInfo {
            name: "Limiter",
            description: "Brick-wall limiter",
            block_type: BlockType::Limiter,
            icon: "limit",
        },
        ModuleInfo {
            name: "Volume",
            description: "Volume/gain control",
            block_type: BlockType::Volume,
            icon: "vol",
        },
        ModuleInfo {
            name: "De-Esser",
            description: "Sibilance reduction",
            block_type: BlockType::DeEsser,
            icon: "deess",
        },
        ModuleInfo {
            name: "Tuner",
            description: "Chromatic tuner",
            block_type: BlockType::Tuner,
            icon: "tuner",
        },
        ModuleInfo {
            name: "Send",
            description: "Parallel routing send",
            block_type: BlockType::Send,
            icon: "send",
        },
    ]
}

#[component]
pub fn ModuleBrowserModal(
    is_open: bool,
    on_close: Callback<()>,
    on_add_module: Callback<BlockType>,
) -> Element {
    if !is_open {
        return rsx! {};
    }

    let mut search_query = use_signal(String::new);
    let mut selected_category = use_signal(|| ModuleCategory::All);

    let all_modules = get_all_modules();

    // Filter by category + search text
    let filtered: Vec<&ModuleInfo> = all_modules
        .iter()
        .filter(|m| selected_category().matches(m.block_type))
        .filter(|m| {
            let q = search_query().to_ascii_lowercase();
            if q.is_empty() {
                return true;
            }
            m.name.to_ascii_lowercase().contains(&q)
                || m.description.to_ascii_lowercase().contains(&q)
        })
        .collect();

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            // Modal container
            div {
                class: "bg-zinc-900 rounded-xl border border-zinc-700 shadow-2xl w-[600px] max-h-[80vh] \
                        flex flex-col overflow-hidden",
                onclick: |e| e.stop_propagation(),

                // Header
                div { class: "flex items-center justify-between px-4 py-3 border-b border-zinc-800",
                    h2 { class: "text-lg font-semibold text-zinc-200", "Add Module" }
                    button {
                        class: "p-1 rounded hover:bg-zinc-800 text-zinc-400 hover:text-zinc-200 transition-colors",
                        onclick: move |_| on_close.call(()),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M6 18L18 6M6 6l12 12",
                            }
                        }
                    }
                }

                // Search input
                div { class: "px-4 py-3 border-b border-zinc-800",
                    div { class: "relative",
                        input {
                            class: "w-full px-3 py-2 pl-9 text-sm bg-zinc-800 border border-zinc-700 rounded-lg \
                                    placeholder:text-zinc-500 focus:outline-none focus:ring-1 focus:ring-zinc-600 text-zinc-300",
                            r#type: "text",
                            placeholder: "Search modules...",
                            value: "{search_query}",
                            oninput: move |e| search_query.set(e.value().clone()),
                        }
                        span { class: "absolute left-3 top-2.5 text-zinc-500 text-sm", ">" }
                    }
                }

                // Category tabs
                div { class: "px-4 py-2 border-b border-zinc-800 overflow-x-auto",
                    div { class: "flex gap-1",
                        for cat in ModuleCategory::ALL.iter() {
                            {
                                let c = *cat;
                                let is_active = selected_category() == c;
                                rsx! {
                                    button {
                                        key: "{c.label()}",
                                        class: if is_active {
                                            "px-3 py-1.5 rounded-lg text-sm font-medium bg-zinc-700 text-zinc-200 transition-colors"
                                        } else {
                                            "px-3 py-1.5 rounded-lg text-sm font-medium text-zinc-400 hover:text-zinc-200 \
                                             hover:bg-zinc-800 transition-colors"
                                        },
                                        onclick: move |_| selected_category.set(c),
                                        "{c.label()}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Module grid
                div { class: "flex-1 overflow-y-auto p-4",
                    if filtered.is_empty() {
                        div { class: "flex items-center justify-center h-32 text-zinc-500",
                            "No modules found"
                        }
                    } else {
                        div { class: "grid grid-cols-2 gap-3",
                            for module in filtered.iter() {
                                {
                                    let bt = module.block_type;
                                    let color = bt.color();
                                    let name = module.name;
                                    let desc = module.description;
                                    let icon_label = &module.icon[..2.min(module.icon.len())].to_uppercase();
                                    rsx! {
                                        div {
                                            key: "{name}",
                                            class: "flex items-center gap-3 p-3 rounded-lg bg-zinc-800 hover:bg-zinc-750 \
                                                    border border-zinc-700 hover:border-zinc-600 transition-all cursor-pointer group",
                                            onclick: move |_| on_add_module.call(bt),

                                            // Color icon
                                            div {
                                                class: "w-8 h-8 rounded flex items-center justify-center text-[10px] font-bold flex-shrink-0",
                                                style: "background-color: {color.bg}; color: {color.fg};",
                                                "{icon_label}"
                                            }

                                            div { class: "flex-1 min-w-0",
                                                h3 { class: "font-medium text-sm text-zinc-200 truncate", "{name}" }
                                                p { class: "text-xs text-zinc-500 truncate", "{desc}" }
                                            }

                                            // Add button (appears on hover)
                                            div {
                                                class: "opacity-0 group-hover:opacity-100 transition-opacity",
                                                button {
                                                    class: "p-1.5 rounded bg-green-600 hover:bg-green-500 text-white transition-colors",
                                                    onclick: move |e| {
                                                        e.stop_propagation();
                                                        on_add_module.call(bt);
                                                    },
                                                    svg {
                                                        class: "w-4 h-4",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                        view_box: "0 0 24 24",
                                                        path {
                                                            stroke_linecap: "round",
                                                            stroke_linejoin: "round",
                                                            d: "M12 4v16m8-8H4",
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// endregion: --- ModuleBrowserModal
