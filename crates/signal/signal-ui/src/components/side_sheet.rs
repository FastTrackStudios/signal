//! Side sheet — slides in from the right edge.

use dioxus::prelude::*;

/// Side from which the sheet slides in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetSide {
    Right,
    Left,
}

/// A slide-in panel overlay.
#[derive(Props, Clone, PartialEq)]
pub struct SideSheetProps {
    /// Whether the sheet is open.
    open: bool,

    /// Title text.
    #[props(default)]
    title: Option<String>,

    /// Width of the sheet.
    #[props(default = "320px".to_string())]
    width: String,

    /// Which side it slides from.
    #[props(default = SheetSide::Right)]
    side: SheetSide,

    /// Callback to close.
    #[props(default)]
    on_close: Option<Callback<()>>,

    /// Sheet body content.
    children: Element,

    /// Extra CSS classes for the sheet panel.
    #[props(default)]
    class: String,
}

#[component]
pub fn SideSheet(props: SideSheetProps) -> Element {
    if !props.open {
        return rsx! {};
    }

    let (translate, slide_anim) = match props.side {
        SheetSide::Right => ("right: 0;", "animate-slide-in-from-right"),
        SheetSide::Left => ("left: 0;", "animate-slide-in-from-left"),
    };

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-40 bg-black/30 animate-fade-in",
            onclick: move |_| {
                if let Some(cb) = &props.on_close {
                    cb.call(());
                }
            },
        }

        // Sheet panel
        div {
            class: format!(
                "fixed top-0 bottom-0 z-50 flex flex-col bg-card border-l border-border shadow-xl {slide_anim} {}",
                props.class
            ),
            style: "{translate} width: {props.width};",
            onclick: move |evt| evt.stop_propagation(),

            // Header
            div {
                class: "flex items-center justify-between px-4 py-3 border-b border-border",

                if let Some(title) = &props.title {
                    h3 {
                        class: "text-sm font-semibold",
                        "{title}"
                    }
                }

                button {
                    class: "px-2 py-1 text-xs rounded hover:bg-muted text-muted-foreground",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_close {
                            cb.call(());
                        }
                    },
                    "\u{2715}"
                }
            }

            // Body
            div {
                class: "flex-1 overflow-y-auto",
                {props.children}
            }
        }
    }
}
