//! Performance View — stage-ready live performance interface.
//!
//! Large touch-friendly tiles, high-contrast text for stage lighting,
//! morph slider, snapshot slot bank, and song/section navigation.

use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A scene tile in the performance grid.
#[derive(Clone, PartialEq)]
pub struct PerfSceneTile {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    /// Summary line (e.g. "4 engines · 12 layers").
    pub summary: String,
}

/// A snapshot slot in the 8-slot bank.
#[derive(Clone, PartialEq)]
pub struct SnapshotSlot {
    pub index: usize,
    pub name: Option<String>,
    /// True if this is the A slot, false if B.
    pub is_a: bool,
    pub is_active: bool,
}

/// Song/section navigation state.
#[derive(Clone, PartialEq)]
pub struct SongNavState {
    pub song_name: String,
    pub section_name: String,
    pub section_index: usize,
    pub section_count: usize,
    pub tempo: Option<u32>,
    pub key_signature: Option<String>,
}

/// Current rig status summary.
#[derive(Clone, PartialEq)]
pub struct RigStatus {
    pub rig_name: String,
    pub engine_count: usize,
    pub layer_count: usize,
    pub active_scene_name: String,
}

// ---------------------------------------------------------------------------
// Scene Grid (touch-friendly)
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct PerfSceneGridProps {
    scenes: Vec<PerfSceneTile>,

    #[props(default)]
    on_scene_select: Option<Callback<String>>,

    #[props(default)]
    class: String,
}

#[component]
pub fn PerfSceneGrid(props: PerfSceneGridProps) -> Element {
    rsx! {
        div {
            class: format!("grid grid-cols-2 gap-3 {}", props.class),

            for scene in props.scenes.iter() {
                {
                    let scene_id = scene.id.clone();
                    let active_class = if scene.is_active {
                        "border-primary bg-primary/20 shadow-lg border-2"
                    } else {
                        "border-border/50 bg-card hover:bg-accent/10 shadow-md hover:shadow-lg border-2"
                    };
                    rsx! {
                        button {
                            class: format!(
                                "flex flex-col items-center justify-center p-6 rounded-xl transition-all min-h-[100px] {active_class}"
                            ),
                            onclick: move |_| {
                                if let Some(cb) = &props.on_scene_select {
                                    cb.call(scene_id.clone());
                                }
                            },
                            span {
                                class: "text-lg font-bold tracking-wide",
                                "{scene.name}"
                            }
                            span {
                                class: "text-xs text-muted-foreground mt-1",
                                "{scene.summary}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Morph Slider (full-width, touch-optimized)
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct MorphSliderProps {
    /// Morph position 0.0–1.0 between scene A and B.
    #[props(default = 0.0)]
    value: f64,

    /// Scene A name.
    #[props(default = "A".to_string())]
    scene_a: String,

    /// Scene B name.
    #[props(default = "B".to_string())]
    scene_b: String,

    #[props(default)]
    on_change: Option<Callback<f64>>,

    #[props(default)]
    class: String,
}

#[component]
pub fn MorphSlider(props: MorphSliderProps) -> Element {
    let pct = (props.value.clamp(0.0, 1.0) * 100.0) as u32;

    rsx! {
        div {
            class: format!("flex flex-col gap-2 {}", props.class),

            // Labels
            div {
                class: "flex items-center justify-between px-1",
                span {
                    class: "text-sm font-semibold text-primary",
                    "{props.scene_a}"
                }
                span {
                    class: "text-xs text-muted-foreground",
                    {format!("{}%", pct)}
                }
                span {
                    class: "text-sm font-semibold text-primary",
                    "{props.scene_b}"
                }
            }

            // Track
            div {
                class: "relative h-12 rounded-lg bg-muted overflow-hidden",

                // Fill
                div {
                    class: "absolute inset-y-0 left-0 bg-gradient-to-r from-primary/40 to-primary/80 transition-all",
                    style: "width: {pct}%;",
                }

                // Hidden input for interaction
                input {
                    r#type: "range",
                    class: "absolute inset-0 w-full h-full opacity-0 cursor-pointer",
                    min: "0",
                    max: "100",
                    value: "{pct}",
                    oninput: move |evt: FormEvent| {
                        if let Some(cb) = &props.on_change {
                            if let Ok(v) = evt.value().parse::<f64>() {
                                cb.call(v / 100.0);
                            }
                        }
                    },
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot Slot Bank (8-slot grid)
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct SnapshotBankProps {
    slots: Vec<SnapshotSlot>,

    #[props(default)]
    on_slot_select: Option<Callback<usize>>,

    #[props(default)]
    on_slot_store: Option<Callback<usize>>,

    #[props(default)]
    class: String,
}

#[component]
pub fn SnapshotBank(props: SnapshotBankProps) -> Element {
    rsx! {
        div {
            class: format!("grid grid-cols-4 gap-2 {}", props.class),

            for slot in props.slots.iter() {
                {
                    let idx = slot.index;
                    let idx2 = slot.index;
                    let active_class = if slot.is_active {
                        "border-primary bg-primary/20"
                    } else if slot.name.is_some() {
                        "border-border bg-card"
                    } else {
                        "border-border/30 bg-muted/30"
                    };
                    let ab_badge = if slot.is_a { "A" } else { "B" };
                    let ab_color = if slot.is_a {
                        "text-signal-slot-a"
                    } else {
                        "text-signal-slot-b"
                    };
                    rsx! {
                        button {
                            class: format!(
                                "relative flex flex-col items-center justify-center p-2 rounded-lg border transition-all min-h-[60px] {active_class}"
                            ),
                            onclick: move |_| {
                                if let Some(cb) = &props.on_slot_select {
                                    cb.call(idx);
                                }
                            },
                            ondoubleclick: move |_| {
                                if let Some(cb) = &props.on_slot_store {
                                    cb.call(idx2);
                                }
                            },

                            // A/B badge
                            span {
                                class: format!("absolute top-0.5 right-1 text-[9px] font-bold {ab_color}"),
                                "{ab_badge}"
                            }

                            // Slot number
                            span {
                                class: "text-sm font-bold",
                                {(slot.index + 1).to_string()}
                            }

                            // Name (if occupied)
                            if let Some(name) = &slot.name {
                                span {
                                    class: "text-[10px] text-muted-foreground truncate max-w-full",
                                    "{name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Song/Section Navigation
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct SongNavProps {
    state: SongNavState,

    #[props(default)]
    on_prev_section: Option<Callback<()>>,

    #[props(default)]
    on_next_section: Option<Callback<()>>,

    #[props(default)]
    class: String,
}

#[component]
pub fn SongNav(props: SongNavProps) -> Element {
    let s = &props.state;
    let section_display = format!("{}/{}", s.section_index + 1, s.section_count);

    rsx! {
        div {
            class: format!("flex items-center gap-4 px-4 py-3 rounded-lg bg-card border border-border {}", props.class),

            // Prev button
            button {
                class: "px-3 py-2 rounded bg-muted hover:bg-accent text-lg font-bold disabled:opacity-30",
                disabled: s.section_index == 0,
                onclick: move |_| {
                    if let Some(cb) = &props.on_prev_section {
                        cb.call(());
                    }
                },
                "\u{25C0}"
            }

            // Song + section info
            div {
                class: "flex-1 text-center",
                div {
                    class: "text-lg font-bold tracking-wide",
                    "{s.song_name}"
                }
                div {
                    class: "flex items-center justify-center gap-3 text-sm text-muted-foreground",
                    span {
                        class: "font-semibold text-foreground",
                        "{s.section_name}"
                    }
                    span { class: "font-mono", "{section_display}" }
                    if let Some(tempo) = s.tempo {
                        span { class: "font-mono", {format!("\u{266A} {} BPM", tempo)} }
                    }
                    if let Some(key) = &s.key_signature {
                        span { "Key: {key}" }
                    }
                }
            }

            // Next button
            button {
                class: "px-3 py-2 rounded bg-muted hover:bg-accent text-lg font-bold disabled:opacity-30",
                disabled: s.section_index + 1 >= s.section_count,
                onclick: move |_| {
                    if let Some(cb) = &props.on_next_section {
                        cb.call(());
                    }
                },
                "\u{25B6}"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rig Status Banner
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct RigStatusBannerProps {
    status: RigStatus,

    #[props(default)]
    class: String,
}

#[component]
pub fn RigStatusBanner(props: RigStatusBannerProps) -> Element {
    let s = &props.status;
    rsx! {
        div {
            class: format!("flex items-center justify-between px-4 py-2 rounded-lg bg-background border border-border border-l-2 border-l-primary {}", props.class),

            div {
                class: "flex items-center gap-3",
                span {
                    class: "text-sm font-bold",
                    "{s.rig_name}"
                }
                span {
                    class: "text-xs text-muted-foreground",
                    {format!("{} engines \u{00B7} {} layers", s.engine_count, s.layer_count)}
                }
            }

            div {
                class: "flex items-center gap-2",
                span {
                    class: "text-[10px] text-muted-foreground uppercase tracking-wider",
                    "Active"
                }
                span {
                    class: "text-sm font-semibold text-primary font-mono tracking-wider",
                    "{s.active_scene_name}"
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Combined Performance View Layout
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct PerformanceViewProps {
    /// Rig status summary.
    status: RigStatus,

    /// Scene tiles.
    scenes: Vec<PerfSceneTile>,

    /// Morph position (0.0–1.0).
    #[props(default = 0.0)]
    morph_value: f64,

    /// Morph scene A name.
    #[props(default = "A".to_string())]
    morph_scene_a: String,

    /// Morph scene B name.
    #[props(default = "B".to_string())]
    morph_scene_b: String,

    /// Snapshot slots.
    #[props(default)]
    snapshot_slots: Vec<SnapshotSlot>,

    /// Song navigation state.
    #[props(default)]
    song_nav: Option<SongNavState>,

    // Callbacks
    #[props(default)]
    on_scene_select: Option<Callback<String>>,

    #[props(default)]
    on_morph_change: Option<Callback<f64>>,

    #[props(default)]
    on_slot_select: Option<Callback<usize>>,

    #[props(default)]
    on_slot_store: Option<Callback<usize>>,

    #[props(default)]
    on_prev_section: Option<Callback<()>>,

    #[props(default)]
    on_next_section: Option<Callback<()>>,

    #[props(default)]
    class: String,
}

#[component]
pub fn PerformanceView(props: PerformanceViewProps) -> Element {
    rsx! {
        div {
            class: format!("flex flex-col gap-4 p-4 h-full {}", props.class),

            // Rig status
            RigStatusBanner {
                status: props.status.clone(),
            }

            // Scene grid (takes most space)
            div {
                class: "flex-1 overflow-y-auto",
                PerfSceneGrid {
                    scenes: props.scenes.clone(),
                    on_scene_select: props.on_scene_select.clone(),
                }
            }

            // Morph slider (full width, prominent)
            MorphSlider {
                value: props.morph_value,
                scene_a: props.morph_scene_a.clone(),
                scene_b: props.morph_scene_b.clone(),
                on_change: props.on_morph_change.clone(),
            }

            // Snapshot bank
            if !props.snapshot_slots.is_empty() {
                SnapshotBank {
                    slots: props.snapshot_slots.clone(),
                    on_slot_select: props.on_slot_select.clone(),
                    on_slot_store: props.on_slot_store.clone(),
                }
            }

            // Song navigation
            if let Some(nav) = &props.song_nav {
                SongNav {
                    state: nav.clone(),
                    on_prev_section: props.on_prev_section.clone(),
                    on_next_section: props.on_next_section.clone(),
                }
            }
        }
    }
}
