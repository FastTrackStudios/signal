//! Chain strip — the active patch's FX chain as a horizontal row of block
//! chips. The compact edit surface for remotes that don't mount the full
//! module/wire grid (browser, small screens); click a chip to toggle bypass.

use dioxus::prelude::*;

use signal_proto::block::BlockType;
use signal_guitar_proto::LiveBlock;

/// Chip accent color by block type.
fn block_accent(bt: BlockType) -> &'static str {
    match bt {
        BlockType::Amp => "#f97316",                            // orange
        BlockType::Compressor => "#22c55e",                     // green
        BlockType::Delay => "#38bdf8",                          // light blue
        BlockType::Reverb => "#818cf8",                         // indigo
        BlockType::Chorus | BlockType::Flanger | BlockType::Phaser => "#e879f9", // magenta
        BlockType::Trem | BlockType::Vibrato | BlockType::Rotary => "#fbbf24",   // amber
        BlockType::Drive => "#ef4444",                          // red
        BlockType::Eq => "#2dd4bf",                             // teal
        _ => "#a1a1aa",                                         // zinc fallback
    }
}

/// The live FX chain as clickable chips. `on_toggle` receives the block id.
#[component]
pub fn ChainStrip(blocks: Vec<LiveBlock>, on_toggle: Callback<String>) -> Element {
    rsx! {
        // One row, horizontally scrollable — the strip is a quick bypass bar
        // under the graph, not a layout of its own.
        div { class: "flex flex-nowrap items-center gap-2 overflow-x-auto pb-1",
            if blocks.is_empty() {
                span { class: "text-sm text-muted-foreground italic", "No active chain." }
            }
            for (i, b) in blocks.iter().enumerate() {
                {
                    let accent = block_accent(b.block_type);
                    let id = b.id.clone();
                    let state_cls = if b.bypassed {
                        "opacity-40 saturate-50"
                    } else {
                        "opacity-100"
                    };
                    let param = b.param_name.as_ref().map(|n| {
                        format!("{n} {:.2}", b.param_value)
                    });
                    rsx! {
                        button {
                            key: "{i}",
                            class: format!(
                                "flex flex-col items-start gap-0.5 rounded-lg border border-border bg-card px-3 py-2 text-left transition-all hover:ring-2 hover:ring-white/30 shrink-0 {state_cls}"
                            ),
                            onclick: move |_| on_toggle.call(id.clone()),
                            div { class: "flex items-center gap-1.5",
                                span { class: "w-2 h-2 rounded-full",
                                    style: "background-color: {accent};" }
                                span { class: "text-sm font-semibold", "{b.name}" }
                            }
                            span { class: "text-[10px] text-muted-foreground",
                                if b.bypassed { "bypassed" } else {
                                    if let Some(p) = param { "{p}" } else { "active" }
                                }
                            }
                        }
                        if i + 1 < blocks.len() {
                            span { class: "text-muted-foreground/50 text-xs", "→" }
                        }
                    }
                }
            }
        }
    }
}
