//! Three-panel editor layout shell.
//!
//! A pure layout component shared by all entity editors.
//! Provides: accent strip (optional), left/center/right panels, and status bar.
//! Each editor slots in its own panel content -- the EntityEditor only owns
//! the outer layout structure.

use dioxus::prelude::*;

#[component]
pub fn EntityEditor(
    /// Left panel content (browser/list).
    left: Element,
    /// Center panel content (main editor area).
    center: Element,
    /// Optional right panel content (picker/guide). Omit for 2-panel layouts.
    #[props(default)]
    right: Option<Element>,
    /// Status bar content (fully custom -- typically a dot + text span).
    status: Element,
    /// Left panel width CSS class. Default: `"w-56"`.
    #[props(default = "w-56".to_string())]
    left_width: String,
    /// Right panel width CSS class. Default: `"w-56"`.
    #[props(default = "w-56".to_string())]
    right_width: String,
    /// Optional accent strip gradient (e.g. `"from-orange-500 via-amber-400 to-cyan-500"`).
    #[props(default)]
    accent_gradient: Option<String>,
) -> Element {
    rsx! {
        div { class: "h-full w-full flex flex-col overflow-hidden",
            // Accent strip
            if let Some(ref gradient) = accent_gradient {
                div { class: "h-[2px] w-full bg-gradient-to-r {gradient} flex-shrink-0" }
            }

            // Main 3-panel row
            div { class: "flex-1 flex min-h-0 overflow-hidden",
                // Left panel
                div { class: "{left_width} flex-shrink-0 border-r border-border/50 flex flex-col min-h-0 bg-zinc-950/50",
                    {left}
                }
                // Center panel
                div { class: "flex-1 flex flex-col min-h-0 min-w-0 overflow-hidden",
                    {center}
                }
                // Right panel (optional)
                if let Some(right_content) = right {
                    div { class: "{right_width} flex-shrink-0 border-l border-border/50 flex flex-col min-h-0 bg-zinc-950/40",
                        {right_content}
                    }
                }
            }

            // Status bar
            div { class: "px-4 py-1.5 border-t border-border/30 flex items-center gap-3 flex-shrink-0 bg-zinc-950/60",
                {status}
            }
        }
    }
}
