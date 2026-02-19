//! Empty grid cell component — placeholder with hover-to-reveal "+" button.

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub(super) struct EmptyGridCellProps {
    pub col: usize,
    pub row: usize,
    pub is_drag_target: bool,
    pub is_any_drag: bool,
    pub picker_open_here: bool,
    pub on_click: EventHandler<MouseEvent>,
}

#[component]
pub(super) fn EmptyGridCell(props: EmptyGridCellProps) -> Element {
    let col = props.col;
    let row = props.row;

    rsx! {
        div {
            key: "empty-{col}-{row}",
            class: "relative aspect-square",
            if props.is_drag_target {
                div {
                    class: "absolute inset-0 flex items-center justify-center \
                         rounded-lg border-2 border-dashed border-cyan-400/60 \
                         bg-cyan-400/10",
                    span {
                        class: "text-cyan-400/60 text-xs font-mono",
                        "drop"
                    }
                }
            } else if props.is_any_drag {
                div {
                    class: "absolute inset-0 flex items-center justify-center \
                         rounded-lg border border-dashed \
                         border-zinc-700/30 bg-zinc-800/5",
                }
            } else {
                div {
                    class: if props.picker_open_here {
                        "group absolute inset-0 flex items-center justify-center \
                         rounded-lg border border-dashed cursor-pointer \
                         border-zinc-600/40 bg-zinc-800/20"
                    } else {
                        "group absolute inset-0 flex items-center justify-center \
                         rounded-lg border border-dashed cursor-pointer \
                         border-transparent bg-transparent \
                         hover:border-zinc-600/40 hover:bg-zinc-800/20"
                    },
                    onclick: move |evt| props.on_click.call(evt),
                    span {
                        class: if props.picker_open_here {
                            "text-zinc-600 text-sm opacity-70"
                        } else {
                            "text-zinc-600 text-sm opacity-0 group-hover:opacity-70"
                        },
                        "+"
                    }
                }
            }
        }
    }
}
