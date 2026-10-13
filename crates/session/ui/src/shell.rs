//! Session Shell
//!
//! The complete session UI shell: top bar with branding and connection badge,
//! dock layout with Performance over Transport, and
//! connection state screens (connecting / disconnected).
//!
//! Both the standalone session apps and the full `FastTrackStudio` app use this
//! shell — they just provide the connection state from their own backends.

use std::rc::Rc;

use dock_dioxus::{DockProvider, DockRoot, PanelRenderer, PanelRendererRegistry, DOCK_WORKSPACE};
use dock_proto::builder::DockLayoutBuilder as B;
use dock_proto::panel::PanelId;
use dock_proto::workspace::{DockWorkspace, WindowBounds};
use dock_proto::PanelRegistry;

use crate::layouts::top_bar::{ConnectionState, VERSION};
use crate::prelude::*;
use crate::signals::Session;

/// The main session shell component.
///
/// Renders the full session UI: top bar, connection states, and dock layout.
/// The caller is responsible for driving the `connection_state` signal from
/// whatever backend they use (REAPER socket, WebSocket, etc.).
#[component]
pub fn SessionShell(
    /// Reactive connection state driven by the host app.
    connection_state: Signal<ConnectionState>,
) -> Element {
    let mut show_connection_info = use_signal(|| false);

    // Initialize dock layout once
    use_hook(|| {
        // Performance over transport, full width.
        //
        // The navigator used to take the left fifth. It is gone from
        // the performance surfaces — the view is a stage now, and a
        // list of songs down the side is not what belongs on it. The
        // panel itself still exists and is still registered, so a host
        // that wants one can dock it.
        let layout = B::vertical()
            .top(B::tile(PanelId::Performance))
            .bottom(B::tile(PanelId::Transport))
            .ratio(80.0)
            .build();

        let mut registry = PanelRegistry::new();
        PanelId::register_all(&mut registry);

        let workspace = DockWorkspace::with_main_window(
            layout,
            "Session",
            WindowBounds::new(0.0, 0.0, 1400.0, 900.0, "main"),
            registry,
        );

        *DOCK_WORKSPACE.write() = workspace;
    });

    // Panel renderer — register session panels
    let render_panel = use_hook(|| {
        let mut registry = PanelRendererRegistry::new();
        crate::register_panels(&mut registry);

        let registry = Rc::new(registry);
        PanelRenderer::new(move |panel_id| registry.render(panel_id))
    });

    rsx! {
        div {
            class: "h-screen flex flex-col bg-background text-foreground",
            tabindex: 0,
            onkeydown: move |evt: KeyboardEvent| {
                handle_hotkey(&evt);
            },

            // ── Top bar ────────────────────────────────────────────
            div {
                class: "h-10 flex-shrink-0 border-b border-border bg-card flex items-center px-4",

                // Left: branding
                div { class: "flex items-center gap-1.5",
                    span { class: "text-sm font-bold bg-gradient-to-r from-blue-400 to-violet-400 bg-clip-text text-transparent", "FTS" }
                    span { class: "text-sm italic text-card-foreground", "Session" }
                }

                div { class: "ml-auto" }

                // Right: version + connection badge
                div { class: "flex items-center gap-3",
                    span { class: "text-xs text-muted-foreground font-mono", "{VERSION}" }
                    ConnectionBadge {
                        state: connection_state(),
                        on_click: move |()| show_connection_info.toggle(),
                    }
                }
            }

            // ── Connection info overlay ───────────────────────────
            if show_connection_info() {
                ConnectionInfoPanel {
                    state: connection_state(),
                    on_close: move |()| show_connection_info.set(false),
                }
            }

            // ── Main content ───────────────────────────────────────
            if connection_state() == ConnectionState::Connected {
                DockProvider { render_panel: render_panel,
                    div { class: "flex-1 overflow-hidden relative",
                        DockRoot {}
                    }
                }
            } else if connection_state() == ConnectionState::Disconnected {
                div { class: "flex-1 flex flex-col items-center justify-center gap-4",
                    div { class: "w-6 h-6 rounded-full bg-red-400/20 flex items-center justify-center",
                        div { class: "w-2.5 h-2.5 rounded-full bg-red-400" }
                    }
                    p { class: "text-sm text-red-400 font-medium", "Disconnected" }
                    p { class: "text-xs text-muted-foreground", "Waiting for connection\u{2026}" }
                }
            } else {
                // Connecting
                div { class: "flex-1 flex flex-col items-center justify-center gap-4",
                    div {
                        class: "w-8 h-8 border-2 border-yellow-400 border-t-transparent rounded-full animate-spin",
                    }
                    p { class: "text-sm text-muted-foreground", "Connecting\u{2026}" }
                }
            }
        }
    }
}

/// Handle global keyboard shortcuts.
fn handle_hotkey(evt: &KeyboardEvent) {
    use dioxus::prelude::Key;

    match evt.key() {
        Key::Character(ref c) if c == " " => {
            // Space → toggle play/pause
            evt.prevent_default();
            spawn(async move {
                let _ = Session::get().setlist().toggle_playback().await;
            });
        }
        Key::ArrowLeft => {
            // Left arrow → previous section
            evt.prevent_default();
            spawn(async move {
                let _ = Session::get().setlist().previous_section().await;
            });
        }
        Key::ArrowRight => {
            // Right arrow → next section
            evt.prevent_default();
            spawn(async move {
                let _ = Session::get().setlist().next_section().await;
            });
        }
        Key::ArrowUp => {
            // Up arrow → previous song
            evt.prevent_default();
            spawn(async move {
                let _ = Session::get().setlist().previous_song().await;
            });
        }
        Key::ArrowDown => {
            // Down arrow → next song
            evt.prevent_default();
            spawn(async move {
                let _ = Session::get().setlist().next_song().await;
            });
        }
        _ => {}
    }
}

/// Connection status badge with spinner for connecting state.
#[component]
pub fn ConnectionBadge(state: ConnectionState, on_click: EventHandler) -> Element {
    let (bg_class, text_class, dot_class, label) = match state {
        ConnectionState::Connected => (
            "bg-green-500/20",
            "text-green-400",
            "bg-green-400",
            "Connected",
        ),
        ConnectionState::Connecting => (
            "bg-yellow-500/20",
            "text-yellow-400",
            "",
            "Connecting\u{2026}",
        ),
        ConnectionState::Disconnected => (
            "bg-red-500/20",
            "text-red-400",
            "bg-red-400",
            "Disconnected",
        ),
    };

    rsx! {
        div {
            class: "flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium {bg_class} {text_class} cursor-pointer hover:brightness-125 transition-all",
            onclick: move |_| on_click.call(()),

            match state {
                ConnectionState::Connecting => rsx! {
                    div {
                        class: "w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin",
                    }
                },
                _ => rsx! {
                    div { class: "w-2 h-2 rounded-full {dot_class}" }
                },
            }

            "{label}"
        }
    }
}

/// Connection info panel — shown when clicking the connection badge.
#[component]
fn ConnectionInfoPanel(state: ConnectionState, on_close: EventHandler) -> Element {
    // Get local network info
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".to_string());

    rsx! {
        div {
            class: "absolute top-10 right-4 z-50 w-72 bg-card border border-border rounded-lg shadow-xl p-4",
            onclick: move |evt: MouseEvent| evt.stop_propagation(),

            div { class: "flex items-center justify-between mb-3",
                span { class: "text-sm font-medium", "Connection Details" }
                button {
                    class: "text-muted-foreground hover:text-foreground text-xs cursor-pointer",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            div { class: "flex flex-col gap-2 text-xs",
                div { class: "flex justify-between",
                    span { class: "text-muted-foreground", "Status" }
                    span { class: match state {
                        ConnectionState::Connected => "text-green-400",
                        ConnectionState::Connecting => "text-yellow-400",
                        ConnectionState::Disconnected => "text-red-400",
                    }, "{state:?}" }
                }
                div { class: "flex justify-between",
                    span { class: "text-muted-foreground", "Host" }
                    span { class: "text-foreground font-mono", "{hostname}" }
                }
            }

            div { class: "mt-3 pt-3 border-t border-border text-[10px] text-muted-foreground",
                "Keyboard shortcuts: Space (play/pause), ←→ (sections), ↑↓ (songs)"
            }
        }
    }
}
