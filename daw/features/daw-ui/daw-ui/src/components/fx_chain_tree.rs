//! FX Chain Tree — hierarchical view of FX containers and plugins.
//!
//! Displays the FX chain as a tree with collapsible containers,
//! routing mode indicators, enable/bypass toggles, and a context menu
//! for mutations (bypass, delete, move, container create/enclose/explode/rename).

use crate::prelude::*;
use daw_control::{FxNode, FxNodeId, FxNodeKind, FxRoutingMode, FxTree};
use std::collections::HashSet;

// ── Context Menu State ──────────────────────────────────────────────

/// What was right-clicked in the FX tree.
#[derive(Clone, Debug, PartialEq)]
enum FxContextTarget {
    /// A plugin node (identified by FxNodeId, which wraps the GUID for plugins).
    Plugin {
        node_id: FxNodeId,
        guid: String,
        enabled: bool,
    },
    /// A container node.
    Container {
        node_id: FxNodeId,
        name: String,
        enabled: bool,
        routing: FxRoutingMode,
    },
}

/// State for the context menu popup.
#[derive(Clone, Debug, PartialEq)]
struct FxContextMenu {
    x: f64,
    y: f64,
    target: FxContextTarget,
}

/// State for the inline rename prompt.
#[derive(Clone, Debug, PartialEq)]
struct RenameState {
    node_id: FxNodeId,
    current_name: String,
}

/// State for the "Create Container" prompt.
#[derive(Clone, Debug, PartialEq)]
struct CreateContainerState {
    name: String,
}

// ── Helper: get FxChain from track_guid ─────────────────────────────

/// Spawn an async mutation, refresh the tree after completion.
/// `track_guid` is needed to re-fetch the tree. `tree` signal is updated.
fn spawn_fx_mutation(
    track_guid: Signal<Option<String>>,
    mut tree: Signal<FxTree>,
    fut: impl std::future::Future<Output = ()> + 'static,
) {
    spawn(async move {
        fut.await;
        // Brief delay for REAPER to process the change
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        // Refresh tree
        if let Some(daw) = daw_control::Daw::try_get()
            && let Some(guid) = track_guid.read().clone()
            && let Ok(project) = daw.current_project().await
            && let Ok(Some(th)) = project.tracks().by_guid(&guid).await
            && let Ok(new_tree) = th.fx_chain().tree().await
        {
            tree.set(new_tree);
        }
    });
}

// ── Main Component ──────────────────────────────────────────────────

/// FX Chain Tree panel that polls the DAW for FX tree state.
///
/// Uses the same poll-wait pattern as MixerPanel and TrackControlPanel.
#[component]
pub fn FxChainTree() -> Element {
    let mut tree = use_signal(FxTree::new);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut connected = use_signal(|| false);
    let mut collapsed = use_signal(HashSet::<String>::new);
    let selected = use_signal(|| Option::<String>::None);
    let mut track_guid = use_signal(|| Option::<String>::None);
    let mut track_name = use_signal(|| Option::<String>::None);
    let mut context_menu = use_signal(|| Option::<FxContextMenu>::None);
    let mut rename_state = use_signal(|| Option::<RenameState>::None);
    let mut create_container = use_signal(|| Option::<CreateContainerState>::None);

    // Poll for FX tree — follows the currently selected track in REAPER
    use_future(move || async move {
        // Poll-wait for DAW
        loop {
            if daw_control::Daw::try_get().is_some() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        let daw = daw_control::Daw::get();
        connected.set(true);

        // Periodic refresh loop
        loop {
            match daw.current_project().await {
                Ok(project) => {
                    // Follow REAPER's selected track
                    let sel_tracks = project.tracks().selected().await.unwrap_or_default();
                    let sel_track = sel_tracks.into_iter().next();

                    if let Some(th) = sel_track {
                        let guid = th.guid().to_string();
                        let name = th.info().await.map(|t| t.name).unwrap_or_default();

                        // Update track name for header display
                        let prev_guid = track_guid.read().clone();
                        if prev_guid.as_deref() != Some(&guid) {
                            // Track changed — reset collapsed state
                            collapsed.write().clear();
                        }
                        track_guid.set(Some(guid.clone()));
                        track_name.set(Some(name));

                        // Fetch FX tree for this track
                        match project.tracks().by_guid(&guid).await {
                            Ok(Some(track_handle)) => match track_handle.fx_chain().tree().await {
                                Ok(fx_tree) => {
                                    tree.set(fx_tree);
                                    error_msg.set(None);
                                }
                                Err(e) => {
                                    error_msg.set(Some(format!("FX tree error: {:?}", e)));
                                }
                            },
                            Ok(None) => {
                                track_guid.set(None);
                                track_name.set(None);
                                tree.set(FxTree::new());
                            }
                            Err(e) => {
                                error_msg.set(Some(format!("Track error: {:?}", e)));
                            }
                        }
                    } else {
                        // No track selected
                        track_guid.set(None);
                        track_name.set(None);
                        tree.set(FxTree::new());
                        error_msg.set(None);
                    }
                }
                Err(e) => {
                    error_msg.set(Some(format!("Project error: {:?}", e)));
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    });

    let is_connected = *connected.read();

    if !is_connected {
        return rsx! {
            div { class: "h-full w-full flex items-center justify-center bg-card text-muted-foreground text-sm",
                "Waiting for DAW connection..."
            }
        };
    }

    {
        let err = error_msg.read();
        if let Some(msg) = err.as_ref() {
            let msg = msg.clone();
            return rsx! {
                div { class: "h-full w-full flex items-center justify-center bg-card text-red-400 text-sm p-4",
                    "{msg}"
                }
            };
        }
    }

    // Clone the tree so we don't hold a read guard across rsx!
    let fx_tree = tree.read().clone();
    let total = fx_tree.total_count();
    let node_count = fx_tree.nodes.len() as u32;
    let tname = track_name.read().clone();
    let has_track = track_guid.read().is_some();

    rsx! {
        div { class: "h-full w-full flex flex-col bg-card overflow-hidden",
            // Header with track name and "+" button
            div { class: "px-3 py-2 border-b border-border flex items-center justify-between",
                div { class: "flex items-center gap-2 min-w-0",
                    h2 { class: "text-sm font-semibold text-foreground whitespace-nowrap", "FX Chain" }
                    if let Some(name) = &tname {
                        span { class: "text-xs text-muted-foreground truncate", "{name}" }
                    }
                }
                if has_track {
                    div { class: "flex items-center gap-2 flex-shrink-0",
                        button {
                            class: "text-[10px] text-muted-foreground hover:text-foreground px-1.5 py-0.5 rounded hover:bg-accent/50 transition-colors",
                            title: "Create Container",
                            onclick: move |_| {
                                create_container.set(Some(CreateContainerState {
                                    name: String::new(),
                                }));
                            },
                            "+ Container"
                        }
                        span { class: "text-[10px] text-muted-foreground", "{total} FX" }
                    }
                }
            }

            // Create Container prompt (inline)
            if create_container.read().is_some() {
                CreateContainerPrompt {
                    on_create: move |name: String| {
                        create_container.set(None);
                        let tg = track_guid;
                        let t = tree;
                        let count = node_count;
                        spawn_fx_mutation(tg, t, async move {
                            if let Some(daw) = daw_control::Daw::try_get()
                                && let Some(guid) = tg.read().clone()
                                    && let Ok(project) = daw.current_project().await
                                        && let Ok(Some(th)) = project.tracks().by_guid(&guid).await {
                                            let _ = th.fx_chain().create_container(&name, count).await;
                                        }
                        });
                    },
                    on_cancel: move |_| {
                        create_container.set(None);
                    },
                }
            }

            // Rename prompt (inline)
            if let Some(rs) = rename_state.read().clone() {
                RenamePrompt {
                    current_name: rs.current_name.clone(),
                    on_rename: {
                        let node_id = rs.node_id.clone();
                        move |new_name: String| {
                            rename_state.set(None);
                            let tg = track_guid;
                            let t = tree;
                            let nid = node_id.clone();
                            spawn_fx_mutation(tg, t, async move {
                                if let Some(daw) = daw_control::Daw::try_get()
                                    && let Some(guid) = tg.read().clone()
                                        && let Ok(project) = daw.current_project().await
                                            && let Ok(Some(th)) = project.tracks().by_guid(&guid).await {
                                                let _ = th.fx_chain().rename_container(&nid, &new_name).await;
                                            }
                            });
                        }
                    },
                    on_cancel: move |_| {
                        rename_state.set(None);
                    },
                }
            }

            // Tree content
            div { class: "flex-1 overflow-y-auto px-1 py-1",
                if !has_track {
                    div { class: "text-center text-muted-foreground text-xs py-8",
                        "Select a track in REAPER"
                    }
                } else if fx_tree.nodes.is_empty() {
                    div { class: "text-center text-muted-foreground text-xs py-8",
                        "No FX in chain"
                    }
                } else {
                    for node in fx_tree.nodes.iter() {
                        FxTreeNode {
                            key: "{node.id.as_str()}",
                            node: node.clone(),
                            depth: 0,
                            collapsed: collapsed,
                            selected: selected,
                            context_menu: context_menu,
                        }
                    }
                }
            }
        }

        // Context menu overlay
        if let Some(menu) = context_menu.read().clone() {
            FxContextMenuPopup {
                menu: menu,
                track_guid: track_guid,
                tree: tree,
                on_close: move |_| {
                    context_menu.set(None);
                },
                on_rename: move |rs: RenameState| {
                    context_menu.set(None);
                    rename_state.set(Some(rs));
                },
            }
        }
    }
}

// ── Create Container Prompt ─────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct CreateContainerPromptProps {
    on_create: EventHandler<String>,
    on_cancel: EventHandler<()>,
}

#[component]
fn CreateContainerPrompt(props: CreateContainerPromptProps) -> Element {
    let mut name = use_signal(|| "New Container".to_string());

    rsx! {
        div { class: "px-3 py-2 border-b border-border bg-accent/30 flex items-center gap-2",
            span { class: "text-[10px] text-muted-foreground whitespace-nowrap", "Name:" }
            input {
                class: "flex-1 bg-background border border-border rounded px-2 py-0.5 text-xs text-foreground outline-none focus:border-primary",
                r#type: "text",
                value: "{name}",
                autofocus: true,
                oninput: move |evt| {
                    name.set(evt.value().clone());
                },
                onkeydown: move |evt| {
                    if evt.key() == Key::Enter {
                        let n = name.read().clone();
                        if !n.trim().is_empty() {
                            props.on_create.call(n);
                        }
                    } else if evt.key() == Key::Escape {
                        props.on_cancel.call(());
                    }
                },
            }
            button {
                class: "text-[10px] text-foreground bg-primary/80 hover:bg-primary rounded px-2 py-0.5 transition-colors",
                onclick: move |_| {
                    let n = name.read().clone();
                    if !n.trim().is_empty() {
                        props.on_create.call(n);
                    }
                },
                "Create"
            }
            button {
                class: "text-[10px] text-muted-foreground hover:text-foreground transition-colors",
                onclick: move |_| props.on_cancel.call(()),
                "Cancel"
            }
        }
    }
}

// ── Rename Prompt ───────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct RenamePromptProps {
    current_name: String,
    on_rename: EventHandler<String>,
    on_cancel: EventHandler<()>,
}

#[component]
fn RenamePrompt(props: RenamePromptProps) -> Element {
    let mut name = use_signal(|| props.current_name.clone());

    rsx! {
        div { class: "px-3 py-2 border-b border-border bg-accent/30 flex items-center gap-2",
            span { class: "text-[10px] text-muted-foreground whitespace-nowrap", "Rename:" }
            input {
                class: "flex-1 bg-background border border-border rounded px-2 py-0.5 text-xs text-foreground outline-none focus:border-primary",
                r#type: "text",
                value: "{name}",
                autofocus: true,
                oninput: move |evt| {
                    name.set(evt.value().clone());
                },
                onkeydown: move |evt| {
                    if evt.key() == Key::Enter {
                        let n = name.read().clone();
                        if !n.trim().is_empty() {
                            props.on_rename.call(n);
                        }
                    } else if evt.key() == Key::Escape {
                        props.on_cancel.call(());
                    }
                },
            }
            button {
                class: "text-[10px] text-foreground bg-primary/80 hover:bg-primary rounded px-2 py-0.5 transition-colors",
                onclick: move |_| {
                    let n = name.read().clone();
                    if !n.trim().is_empty() {
                        props.on_rename.call(n);
                    }
                },
                "Rename"
            }
            button {
                class: "text-[10px] text-muted-foreground hover:text-foreground transition-colors",
                onclick: move |_| props.on_cancel.call(()),
                "Cancel"
            }
        }
    }
}

// ── Context Menu Popup ──────────────────────────────────────────────

#[derive(Props, Clone)]
struct FxContextMenuPopupProps {
    menu: FxContextMenu,
    track_guid: Signal<Option<String>>,
    tree: Signal<FxTree>,
    on_close: EventHandler<()>,
    on_rename: EventHandler<RenameState>,
}

impl PartialEq for FxContextMenuPopupProps {
    fn eq(&self, other: &Self) -> bool {
        self.menu == other.menu
    }
}

#[component]
fn FxContextMenuPopup(props: FxContextMenuPopupProps) -> Element {
    let x = props.menu.x;
    let y = props.menu.y;
    let target = &props.menu.target;
    let track_guid = props.track_guid;
    let tree = props.tree;

    let is_container = matches!(target, FxContextTarget::Container { .. });
    let is_enabled = match target {
        FxContextTarget::Plugin { enabled, .. } => *enabled,
        FxContextTarget::Container { enabled, .. } => *enabled,
    };
    let bypass_label = if is_enabled { "Bypass" } else { "Enable" };

    let node_id = match target {
        FxContextTarget::Plugin { node_id, .. } => node_id.clone(),
        FxContextTarget::Container { node_id, .. } => node_id.clone(),
    };

    // For plugins: GUID-based bypass via FxHandle
    let plugin_guid = match target {
        FxContextTarget::Plugin { guid, .. } => Some(guid.clone()),
        _ => None,
    };

    // For containers: routing mode toggle
    let container_routing = match target {
        FxContextTarget::Container { routing, .. } => Some(*routing),
        _ => None,
    };

    // For rename
    let container_name = match target {
        FxContextTarget::Container { name, .. } => Some(name.clone()),
        _ => None,
    };

    let on_close = props.on_close;
    let on_close2 = props.on_close;
    let on_close3 = props.on_close;
    let on_close4 = props.on_close;
    let on_close5 = props.on_close;
    let on_close6 = props.on_close;
    let on_close7 = props.on_close;
    let on_rename = props.on_rename;

    let nid_bypass = node_id.clone();
    let nid_delete = node_id.clone();
    let nid_enclose = node_id.clone();
    let nid_explode = node_id.clone();
    let nid_routing = node_id.clone();

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-40",
            onclick: move |_| on_close.call(()),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                on_close2.call(());
            },
        }

        // Menu popup
        div {
            class: "fixed z-50 py-1 rounded-lg shadow-xl border border-border min-w-[180px] bg-popover",
            style: "left: {x}px; top: {y}px;",

            // Bypass / Enable
            FxMenuItem {
                label: bypass_label,
                on_click: {
                    let guid = plugin_guid.clone();
                    move |_| {
                        on_close3.call(());
                        let guid = guid.clone();
                        let nid = nid_bypass.clone();
                        spawn_fx_mutation(track_guid, tree, async move {
                            if let Some(daw) = daw_control::Daw::try_get()
                                && let Some(tguid) = track_guid.read().clone()
                                    && let Ok(project) = daw.current_project().await
                                        && let Ok(Some(th)) = project.tracks().by_guid(&tguid).await {
                                            if let Some(g) = &guid {
                                                // Plugin: use FxHandle toggle
                                                if let Ok(Some(fh)) = th.fx_chain().by_guid(g).await {
                                                    let _ = fh.toggle().await;
                                                }
                                            } else {
                                                // Container: toggle via FxHandle by resolving guid isn't possible,
                                                // but we can set enabled via the tree. For now, containers don't
                                                // have a direct toggle API — the user can bypass from REAPER.
                                                let _ = nid; // acknowledge
                                            }
                                        }
                        });
                    }
                },
            }

            // Toggle routing mode (containers only)
            if let Some(routing) = container_routing {
                FxMenuItem {
                    label: if routing == FxRoutingMode::Serial { "Switch to Parallel" } else { "Switch to Serial" },
                    on_click: move |_| {
                        on_close4.call(());
                        let nid = nid_routing.clone();
                        let new_mode = if routing == FxRoutingMode::Serial {
                            FxRoutingMode::Parallel
                        } else {
                            FxRoutingMode::Serial
                        };
                        spawn_fx_mutation(track_guid, tree, async move {
                            if let Some(daw) = daw_control::Daw::try_get()
                                && let Some(tguid) = track_guid.read().clone()
                                    && let Ok(project) = daw.current_project().await
                                        && let Ok(Some(th)) = project.tracks().by_guid(&tguid).await {
                                            let _ = th.fx_chain().set_routing_mode(&nid, new_mode).await;
                                        }
                        });
                    },
                }
            }

            // Rename (containers only)
            if let Some(cname) = &container_name {
                FxMenuItem {
                    label: "Rename",
                    on_click: {
                        let cname = cname.clone();
                        let nid = node_id.clone();
                        move |_| {
                            on_rename.call(RenameState {
                                node_id: nid.clone(),
                                current_name: cname.clone(),
                            });
                        }
                    },
                }
            }

            // Enclose in Container (plugins only — wraps this plugin in a new container)
            if !is_container {
                FxMenuItem {
                    label: "Enclose in Container",
                    on_click: move |_| {
                        on_close5.call(());
                        let nid = nid_enclose.clone();
                        spawn_fx_mutation(track_guid, tree, async move {
                            if let Some(daw) = daw_control::Daw::try_get()
                                && let Some(tguid) = track_guid.read().clone()
                                    && let Ok(project) = daw.current_project().await
                                        && let Ok(Some(th)) = project.tracks().by_guid(&tguid).await {
                                            let _ = th.fx_chain().enclose_in_container(&[nid], "Container").await;
                                        }
                        });
                    },
                }
            }

            // Explode Container (containers only — moves children out, deletes container)
            if is_container {
                FxMenuItem {
                    label: "Explode Container",
                    on_click: move |_| {
                        on_close6.call(());
                        let nid = nid_explode.clone();
                        spawn_fx_mutation(track_guid, tree, async move {
                            if let Some(daw) = daw_control::Daw::try_get()
                                && let Some(tguid) = track_guid.read().clone()
                                    && let Ok(project) = daw.current_project().await
                                        && let Ok(Some(th)) = project.tracks().by_guid(&tguid).await {
                                            let _ = th.fx_chain().explode_container(&nid).await;
                                        }
                        });
                    },
                }
            }

            // Separator
            div { class: "my-1 border-t border-border" }

            // Delete
            FxMenuItem {
                label: "Delete",
                danger: true,
                on_click: move |_| {
                    on_close7.call(());
                    let guid = plugin_guid.clone();
                    let nid = nid_delete.clone();
                    spawn_fx_mutation(track_guid, tree, async move {
                        if let Some(daw) = daw_control::Daw::try_get()
                            && let Some(tguid) = track_guid.read().clone()
                                && let Ok(project) = daw.current_project().await
                                    && let Ok(Some(th)) = project.tracks().by_guid(&tguid).await {
                                        if let Some(g) = &guid {
                                            // Plugin: remove by GUID
                                            if let Ok(Some(fh)) = th.fx_chain().by_guid(g).await {
                                                let _ = fh.remove().await;
                                            }
                                        } else {
                                            // Container: explode then the container slot is gone
                                            let _ = th.fx_chain().explode_container(&nid).await;
                                        }
                                    }
                    });
                },
            }
        }
    }
}

// ── Menu Item ───────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct FxMenuItemProps {
    label: String,
    #[props(default)]
    danger: bool,
    on_click: EventHandler<()>,
}

#[component]
fn FxMenuItem(props: FxMenuItemProps) -> Element {
    let text_class = if props.danger {
        "text-red-400 hover:text-red-300"
    } else {
        "text-foreground/80 hover:text-foreground"
    };

    rsx! {
        button {
            class: "w-full flex items-center px-3 py-1.5 text-xs hover:bg-accent/50 transition-colors {text_class}",
            onclick: move |_| props.on_click.call(()),
            "{props.label}"
        }
    }
}

// ── Tree Node ───────────────────────────────────────────────────────

#[derive(Props, Clone)]
struct FxTreeNodeProps {
    node: FxNode,
    depth: u32,
    collapsed: Signal<HashSet<String>>,
    selected: Signal<Option<String>>,
    context_menu: Signal<Option<FxContextMenu>>,
}

impl PartialEq for FxTreeNodeProps {
    fn eq(&self, other: &Self) -> bool {
        self.node.id == other.node.id
            && self.depth == other.depth
            && self.node.enabled == other.node.enabled
    }
}

#[component]
fn FxTreeNode(props: FxTreeNodeProps) -> Element {
    let node = &props.node;
    let depth = props.depth;
    let mut collapsed = props.collapsed;
    let mut selected = props.selected;
    let mut context_menu = props.context_menu;
    let node_id = node.id.as_str().to_string();
    let is_selected = selected.read().as_deref() == Some(node_id.as_str());

    let indent_px = depth * 20;

    match &node.kind {
        FxNodeKind::Container {
            name,
            children,
            routing,
            ..
        } => {
            let is_collapsed = collapsed.read().contains(&node_id);
            let child_count = children.len();
            let routing_label = match routing {
                FxRoutingMode::Serial => "S",
                FxRoutingMode::Parallel => "P",
            };
            let routing_color = match routing {
                FxRoutingMode::Serial => "text-blue-400",
                FxRoutingMode::Parallel => "text-amber-400",
            };
            let enabled_class = if node.enabled {
                "text-foreground"
            } else {
                "text-muted-foreground line-through"
            };
            let selected_bg = if is_selected {
                "bg-accent"
            } else {
                "hover:bg-accent/50"
            };
            let chevron = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };

            let toggle_id = node_id.clone();
            let select_id = node_id.clone();

            // Context menu data
            let ctx_node_id = node.id.clone();
            let ctx_name = name.clone();
            let ctx_enabled = node.enabled;
            let ctx_routing = *routing;

            rsx! {
                // Container row
                div {
                    class: "flex items-center gap-1 px-1 py-0.5 rounded cursor-pointer text-xs {selected_bg}",
                    style: "padding-left: {indent_px}px",
                    onclick: move |_| {
                        selected.set(Some(select_id.clone()));
                    },
                    oncontextmenu: move |evt| {
                        evt.prevent_default();
                        evt.stop_propagation();
                        context_menu.set(Some(FxContextMenu {
                            x: evt.client_coordinates().x,
                            y: evt.client_coordinates().y,
                            target: FxContextTarget::Container {
                                node_id: ctx_node_id.clone(),
                                name: ctx_name.clone(),
                                enabled: ctx_enabled,
                                routing: ctx_routing,
                            },
                        }));
                    },

                    // Collapse toggle
                    button {
                        class: "w-4 h-4 flex items-center justify-center text-muted-foreground hover:text-foreground text-[8px]",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            let mut set = collapsed.write();
                            if set.contains(&toggle_id) {
                                set.remove(&toggle_id);
                            } else {
                                set.insert(toggle_id.clone());
                            }
                        },
                        "{chevron}"
                    }

                    // Container icon (routing badge)
                    span { class: "text-[10px] {routing_color} font-bold w-4 text-center",
                        "{routing_label}"
                    }

                    // Name
                    span { class: "flex-1 truncate font-medium {enabled_class}", "{name}" }

                    // Child count badge
                    span { class: "text-[9px] text-muted-foreground bg-muted px-1 rounded", "{child_count}" }

                    // Enable indicator
                    span {
                        class: if node.enabled { "w-2 h-2 rounded-full bg-green-500" } else { "w-2 h-2 rounded-full bg-neutral-600" },
                    }
                }

                // Children (if not collapsed) — wrapped in a bordered container for visual nesting
                if !is_collapsed {
                    div {
                        class: "ml-2 border-l border-border/50",
                        style: "margin-left: {indent_px + 10}px",
                        for child in children.iter() {
                            FxTreeNode {
                                key: "{child.id.as_str()}",
                                node: child.clone(),
                                depth: depth + 1,
                                collapsed: collapsed,
                                selected: selected,
                                context_menu: context_menu,
                            }
                        }
                    }
                }
            }
        }

        FxNodeKind::Plugin(fx) => {
            let enabled_class = if node.enabled {
                "text-foreground"
            } else {
                "text-muted-foreground line-through"
            };
            let selected_bg = if is_selected {
                "bg-accent"
            } else {
                "hover:bg-accent/50"
            };
            let select_id = node_id.clone();
            let preset = fx.preset_name.as_deref().unwrap_or("");

            // Context menu data
            let ctx_node_id = node.id.clone();
            let ctx_guid = fx.guid.clone();
            let ctx_enabled = node.enabled;

            rsx! {
                div {
                    class: "flex items-center gap-1 px-1 py-0.5 rounded cursor-pointer text-xs {selected_bg}",
                    style: "padding-left: {indent_px}px",
                    onclick: move |_| {
                        selected.set(Some(select_id.clone()));
                    },
                    oncontextmenu: move |evt| {
                        evt.prevent_default();
                        evt.stop_propagation();
                        context_menu.set(Some(FxContextMenu {
                            x: evt.client_coordinates().x,
                            y: evt.client_coordinates().y,
                            target: FxContextTarget::Plugin {
                                node_id: ctx_node_id.clone(),
                                guid: ctx_guid.clone(),
                                enabled: ctx_enabled,
                            },
                        }));
                    },

                    // Spacer (no collapse toggle for plugins)
                    span { class: "w-4" }

                    // Plugin type badge
                    span { class: "text-[9px] text-muted-foreground font-mono w-4 text-center",
                        match fx.plugin_type {
                            daw_proto::FxType::Vst3 => "V3",
                            daw_proto::FxType::Vst2 => "V2",
                            daw_proto::FxType::Au => "AU",
                            daw_proto::FxType::Js => "JS",
                            daw_proto::FxType::Clap => "CL",
                            _ => "FX",
                        }
                    }

                    // Name
                    span { class: "flex-1 truncate {enabled_class}", "{fx.name}" }

                    // Preset
                    if !preset.is_empty() {
                        span { class: "text-[9px] text-muted-foreground truncate max-w-[80px]",
                            "{preset}"
                        }
                    }

                    // Enable indicator
                    span {
                        class: if node.enabled { "w-2 h-2 rounded-full bg-green-500" } else { "w-2 h-2 rounded-full bg-neutral-600" },
                    }
                }
            }
        }
    }
}
