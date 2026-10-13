//! Unified keybind editor shell.
//!
//! Hosts the profile selector and tab bar, then mounts the per-tab panels
//! (keyboard visualizer, action list, …). Runs identically as a REAPER dock
//! panel and as the standalone `keybind-editor` Blitz binary — the only
//! difference is the data source, handled transparently by [`super::source`].

use reaper_dioxus::prelude::*;

use super::KeyboardPanel;
use super::actions_list::ActionsListPanel;
use super::editor_tabs::{ProfileTab, WheelMouseTab, WhichKeyTab, WorkflowsTab};
use super::source;
use crate::ui::tailwind::TailwindStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorTab {
    Keyboard,
    Actions,
    WheelMouse,
    WhichKey,
    Workflows,
    Profile,
}

impl EditorTab {
    fn label(self) -> &'static str {
        match self {
            Self::Keyboard => "Keyboard",
            Self::Actions => "Actions",
            Self::WheelMouse => "Wheel & Mouse",
            Self::WhichKey => "Which-Key",
            Self::Workflows => "Workflows",
            Self::Profile => "Profile",
        }
    }

    fn all() -> &'static [EditorTab] {
        &[
            Self::Keyboard,
            Self::Actions,
            Self::WheelMouse,
            Self::WhichKey,
            Self::Workflows,
            Self::Profile,
        ]
    }
}

/// Root component for the unified keybind editor.
#[component]
pub fn EditorApp() -> Element {
    let profiles = source::list_profiles();
    let initial = source::active_profile()
        .or_else(|| {
            profiles
                .iter()
                .find(|p| p.as_str() == "fasttrackstudio")
                .cloned()
        })
        .or_else(|| profiles.first().cloned());

    // Seed the global file-source scope once, synchronously, so the first paint
    // of the child panels is already scoped to the selected profile.
    use_hook(|| source::set_active_profile(initial.clone()));

    let mut active_profile = use_signal(|| initial.clone());
    let mut tab = use_signal(|| EditorTab::Keyboard);

    let current = active_profile.read().clone().unwrap_or_default();
    let current_tab = *tab.read();
    let mode = if source::is_standalone() {
        "standalone"
    } else {
        "live"
    };

    rsx! {
        TailwindStyle {}

        div { class: "w-screen h-screen flex flex-col bg-zinc-950 text-zinc-100 text-xs select-none overflow-hidden",

            // ── Top bar: title, profile selector, mode ──
            div { class: "flex items-center gap-3 px-3 py-2 border-b border-zinc-800 bg-zinc-900",
                span { class: "text-zinc-200 font-bold tracking-wide", "FTS Keybind Editor" }

                div { class: "flex items-center gap-1.5",
                    span { class: "text-zinc-500 uppercase tracking-wider", "Profile" }
                    select {
                        class: "bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-xs text-zinc-200 appearance-none",
                        value: "{current}",
                        onchange: move |e| {
                            let v = e.value();
                            source::set_active_profile(Some(v.clone()));
                            active_profile.set(Some(v));
                        },
                        for p in profiles.iter() {
                            option { value: "{p}", "{p}" }
                        }
                    }
                    if profiles.is_empty() {
                        span { class: "text-rose-400", "no profiles found" }
                    }
                }

                span { class: "ml-auto text-zinc-600 text-[10px] uppercase tracking-wider",
                    "{mode}"
                }
            }

            // ── Tab bar ──
            div { class: "flex items-center gap-1 px-2 py-1 border-b border-zinc-800 bg-zinc-900/60",
                for t in EditorTab::all() {
                    {
                        let t = *t;
                        let is_active = current_tab == t;
                        let cls = if is_active {
                            "bg-blue-600 text-white px-3 py-1 rounded text-xs cursor-pointer"
                        } else {
                            "bg-transparent text-zinc-400 px-3 py-1 rounded text-xs cursor-pointer hover:bg-zinc-800"
                        };
                        rsx! {
                            span {
                                class: "{cls}",
                                onclick: move |_| tab.set(t),
                                "{t.label()}"
                            }
                        }
                    }
                }
            }

            // ── Body ──
            div { class: "flex-1 min-h-0 overflow-hidden",
                match current_tab {
                    // `key` forces a remount when the profile changes so the
                    // panels re-read the (now re-scoped) file source.
                    EditorTab::Keyboard => rsx! { KeyboardPanel { key: "{current}" } },
                    EditorTab::Actions => rsx! { ActionsListPanel { key: "{current}" } },
                    EditorTab::WheelMouse => rsx! { WheelMouseTab { key: "{current}" } },
                    EditorTab::WhichKey => rsx! { WhichKeyTab { key: "{current}" } },
                    EditorTab::Workflows => rsx! { WorkflowsTab { key: "{current}" } },
                    EditorTab::Profile => rsx! { ProfileTab { key: "{current}" } },
                }
            }
        }
    }
}
