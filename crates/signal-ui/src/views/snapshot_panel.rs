//! DAW Snapshot Panel — capture and recall DAW parameter/state snapshots.
//!
//! Two capture modes:
//! - **Quick Capture**: Parameters only (lightweight, for A/B morphing)
//! - **Full Capture**: Parameters + binary state chunks (exact plugin state restore)

use dioxus::prelude::*;

/// A snapshot entry in the list.
#[derive(Clone, PartialEq)]
pub struct SnapshotEntry {
    pub id: String,
    pub name: String,
    pub capture_type: CaptureType,
    pub created_at: String,
    pub param_count: usize,
    pub chunk_count: usize,
}

/// Type of capture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaptureType {
    Quick,
    Full,
}

impl CaptureType {
    fn label(self) -> &'static str {
        match self {
            Self::Quick => "Quick",
            Self::Full => "Full",
        }
    }

    fn badge_class(self) -> &'static str {
        match self {
            Self::Quick => "bg-signal-quick/20 text-signal-quick",
            Self::Full => "bg-signal-full/20 text-signal-full",
        }
    }
}

/// The snapshot panel view.
#[derive(Props, Clone, PartialEq)]
pub struct SnapshotPanelProps {
    /// List of existing snapshots.
    snapshots: Vec<SnapshotEntry>,

    /// Callback: quick-capture current DAW state.
    #[props(default)]
    on_quick_capture: Option<Callback<String>>,

    /// Callback: full-capture current DAW state.
    #[props(default)]
    on_full_capture: Option<Callback<String>>,

    /// Callback: recall a snapshot by ID.
    #[props(default)]
    on_recall: Option<Callback<String>>,

    /// Callback: delete a snapshot by ID.
    #[props(default)]
    on_delete: Option<Callback<String>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn SnapshotPanel(props: SnapshotPanelProps) -> Element {
    let mut capture_name = use_signal(|| String::from("Snapshot"));

    rsx! {
        div {
            class: format!("flex flex-col gap-3 p-4 {}", props.class),

            // Header
            h3 {
                class: "text-xs uppercase tracking-widest text-muted-foreground font-semibold",
                "DAW Snapshots"
            }

            // Capture controls
            div {
                class: "flex gap-2",

                input {
                    class: "flex-1 px-2 py-1 text-xs rounded border border-input bg-background text-foreground placeholder:text-muted-foreground",
                    placeholder: "Snapshot name...",
                    value: "{capture_name}",
                    oninput: move |evt: FormEvent| {
                        capture_name.set(evt.value().clone());
                    },
                }

                button {
                    class: "px-2 py-1 text-xs rounded bg-signal-quick text-white hover:bg-signal-quick/90",
                    title: "Quick Capture (params only)",
                    onclick: {
                        let name = capture_name.read().clone();
                        move |_| {
                            if let Some(cb) = &props.on_quick_capture {
                                cb.call(name.clone());
                            }
                        }
                    },
                    "Quick"
                }

                button {
                    class: "px-2 py-1 text-xs rounded bg-signal-full text-white hover:bg-signal-full/90",
                    title: "Full Capture (params + state chunks)",
                    onclick: {
                        let name = capture_name.read().clone();
                        move |_| {
                            if let Some(cb) = &props.on_full_capture {
                                cb.call(name.clone());
                            }
                        }
                    },
                    "Full"
                }
            }

            // Snapshot list
            div {
                class: "flex flex-col gap-1 max-h-80 overflow-y-auto",

                if props.snapshots.is_empty() {
                    div {
                        class: "text-xs text-muted-foreground text-center py-4",
                        "No snapshots yet. Capture one to get started."
                    }
                }

                for snap in props.snapshots.iter() {
                    {
                        let snap_id = snap.id.clone();
                        let snap_id2 = snap.id.clone();
                        rsx! {
                            div {
                                class: "flex items-center justify-between px-2 py-1.5 rounded hover:bg-muted group",

                                div {
                                    class: "flex items-center gap-2 flex-1 min-w-0",

                                    // Type badge
                                    span {
                                        class: format!(
                                            "px-1.5 py-0.5 rounded text-[10px] font-medium {}",
                                            snap.capture_type.badge_class()
                                        ),
                                        {snap.capture_type.label()}
                                    }

                                    // Name
                                    span {
                                        class: "text-xs font-medium truncate",
                                        "{snap.name}"
                                    }

                                    // Stats
                                    span {
                                        class: "text-[10px] text-muted-foreground",
                                        "{snap.param_count}p",
                                        if snap.chunk_count > 0 {
                                            " {snap.chunk_count}c"
                                        }
                                    }
                                }

                                // Actions (visible on hover)
                                div {
                                    class: "flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity",

                                    button {
                                        class: "px-1.5 py-0.5 text-[10px] rounded bg-primary text-primary-foreground hover:bg-primary/90",
                                        onclick: move |_| {
                                            if let Some(cb) = &props.on_recall {
                                                cb.call(snap_id.clone());
                                            }
                                        },
                                        "Recall"
                                    }

                                    button {
                                        class: "px-1.5 py-0.5 text-[10px] rounded bg-destructive/20 text-destructive hover:bg-destructive/30",
                                        onclick: move |_| {
                                            if let Some(cb) = &props.on_delete {
                                                cb.call(snap_id2.clone());
                                            }
                                        },
                                        "\u{2715}"
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
