//! Resizable split pane with drag handle.
//!
//! Renders two children side-by-side (horizontal) or top/bottom (vertical)
//! with a draggable resize handle. Uses percentage-based sizing.
//!
//! Resize uses the Mosaic pattern: on mousedown, JS installs document-level
//! mousemove/mouseup listeners that directly update child element sizes for
//! smooth, synchronous tracking. A transparent overlay captures events so the
//! cursor never escapes. On mouseup, JS dispatches a custom event carrying
//! the final ratio back to Dioxus.

use crate::hooks::use_dock_actions;
use crate::prelude::*;
use crate::signals::DOCK_RESIZING;
use dock_proto::{NodeId, SplitDirection};

/// Minimum percentage for any pane (prevents invisible tiles).
const MIN_RATIO: f64 = 10.0;
/// Maximum percentage for any pane.
const MAX_RATIO: f64 = 90.0;

/// Resizable split pane component.
#[component]
pub fn SplitPane(
    node_id: NodeId,
    direction: SplitDirection,
    ratio: f64,
    first: Element,
    second: Element,
) -> Element {
    let mut local_ratio = use_signal(move || ratio);
    let mut dragging = use_signal(|| false);
    let actions = use_dock_actions();

    // Unique ID for this split container
    let container_id = use_memo(move || format!("split-{}", node_id));

    // Sync with external ratio changes (e.g. preset load)
    use_effect(move || {
        local_ratio.set(ratio);
    });

    let is_horizontal = direction == SplitDirection::Horizontal;

    let first_style = if is_horizontal {
        format!("width: calc({}% - 4px); height: 100%;", local_ratio())
    } else {
        format!("height: calc({}% - 4px); width: 100%;", local_ratio())
    };

    let second_style = if is_horizontal {
        format!(
            "width: calc({}% - 4px); height: 100%;",
            100.0 - local_ratio()
        )
    } else {
        format!(
            "height: calc({}% - 4px); width: 100%;",
            100.0 - local_ratio()
        )
    };

    let container_class = if is_horizontal {
        "flex flex-row h-full w-full"
    } else {
        "flex flex-col h-full w-full"
    };

    let handle_cursor = if is_horizontal {
        "cursor-col-resize"
    } else {
        "cursor-row-resize"
    };

    let handle_size = if is_horizontal {
        "w-2 h-full"
    } else {
        "h-2 w-full"
    };

    rsx! {
        div {
            id: "{container_id}",
            class: "{container_class}",

            // First child
            div {
                class: "overflow-hidden flex-shrink-0",
                style: "{first_style}",
                {first}
            }

            // Drag handle
            div {
                class: "{handle_size} {handle_cursor} bg-border hover:bg-blue-500/50 active:bg-blue-500/70 transition-colors flex-shrink-0 relative group",
                onmousedown: {
                    let container_id = container_id;
                    move |e: MouseEvent| {
                        e.prevent_default();
                        dragging.set(true);
                        *DOCK_RESIZING.write() = Some(node_id);

                        // Install document-level listeners via JS (Mosaic pattern).
                        // JS handles mousemove/mouseup on document so we never lose
                        // tracking. It directly updates child element sizes for smooth
                        // real-time feedback. On mouseup, it stores the final ratio
                        // in a dataset attribute for Dioxus to read.
                        let cid = container_id.to_string();
                        let js = format!(
                            r#"(function() {{
    var el = document.getElementById('{cid}');
    if (!el) return;
    var rect = el.getBoundingClientRect();
    var origin = rect.{origin_prop};
    var size = rect.{size_prop};
    var first = el.children[0];
    var second = el.children[2];

    var overlay = document.createElement('div');
    overlay.style.cssText = 'position:fixed;inset:0;z-index:99999;cursor:{cursor};user-select:none;-webkit-user-select:none;';
    document.body.appendChild(overlay);
    document.body.style.userSelect = 'none';

    function onMove(e) {{
        e.preventDefault();
        var pos = e.{page_coord} - origin;
        var ratio = pos / size * 100;
        if (ratio < {min}) ratio = {min};
        if (ratio > {max}) ratio = {max};
        first.style.{size_prop} = 'calc(' + ratio + '% - 4px)';
        second.style.{size_prop} = 'calc(' + (100 - ratio) + '% - 4px)';
        el.dataset.ratio = ratio;
    }}

    function onUp() {{
        document.removeEventListener('mousemove', onMove, true);
        document.removeEventListener('mouseup', onUp, true);
        if (overlay.parentNode) overlay.remove();
        document.body.style.userSelect = '';
    }}

    document.addEventListener('mousemove', onMove, true);
    document.addEventListener('mouseup', onUp, true);
}})();"#,
                            cid = cid,
                            origin_prop = if is_horizontal { "left" } else { "top" },
                            size_prop = if is_horizontal { "width" } else { "height" },
                            cursor = handle_cursor,
                            page_coord = if is_horizontal { "pageX" } else { "pageY" },
                            min = MIN_RATIO,
                            max = MAX_RATIO,
                        );
                        document::eval(&js);

                        // Wait for drag completion via JS Promise that resolves on mouseup,
                        // then read the final ratio back into Dioxus signals.
                        let cid2 = container_id.to_string();
                        spawn(async move {
                            // This JS Promise resolves when the mouseup fires
                            let wait_js = format!(
                                r#"new Promise(function(resolve) {{
    function check() {{
        if (!document.querySelector('div[style*="z-index:99999"]')) {{
            resolve(parseFloat(document.getElementById('{}')?.dataset?.ratio || '{}'));
        }} else {{
            setTimeout(check, 30);
        }}
    }}
    check();
}})"#,
                                cid2,
                                local_ratio.peek()
                            );
                            if let Ok(result) = document::eval(&wait_js).await {
                                let s = result.to_string();
                                if let Ok(r) = s.parse::<f64>() {
                                    let r = r.clamp(MIN_RATIO, MAX_RATIO);
                                    local_ratio.set(r);
                                    actions.update_split_ratio.call((node_id, r));
                                }
                            }

                            dragging.set(false);
                            *DOCK_RESIZING.write() = None;
                        });
                    }
                },

                // Visual indicator line in the center
                div {
                    class: if is_horizontal {
                        "absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-px h-4 bg-muted-foreground group-hover:bg-blue-400 transition-colors"
                    } else {
                        "absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 h-px w-4 bg-muted-foreground group-hover:bg-blue-400 transition-colors"
                    },
                }
            }

            // Second child
            div {
                class: "overflow-hidden flex-shrink-0",
                style: "{second_style}",
                {second}
            }
        }
    }
}
