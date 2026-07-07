//! Profile view — renders a hardware profile's controls as a themed GUI.
//!
//! Takes a `Profile` definition and renders the appropriate knobs,
//! switches, and stepped selectors. The active theme determines the
//! visual presentation (skeuomorphic, minimal, FastTrack branded, etc.).

use eq_profiles::api_550a::Api550aProfile;
use eq_profiles::core::{ParamMapping, Profile};
use eq_profiles::neve_1073::Neve1073Profile;
use eq_profiles::pultec::PultecProfile;
use eq_profiles::ssl_e::SslEProfile;
use eq_profiles::ssl_g::SslGProfile;
use nice_plug_dioxus::prelude::*;

fn control_range_label(mapping: &ParamMapping) -> String {
    match mapping {
        ParamMapping::Direct { range, .. } => {
            format!("{:.1} - {:.1}", range.start(), range.end())
        }
        ParamMapping::Stepped { labels, .. } => labels.join("  "),
        ParamMapping::Compound { range, .. } => {
            format!("{:.1} - {:.1}", range.start(), range.end())
        }
    }
}

#[component]
pub fn PultecProfileView() -> Element {
    static PROFILE: PultecProfile = PultecProfile;

    hardware_profile_layout(
        PROFILE.name(),
        PROFILE.id(),
        PROFILE.controls(),
        "Low shelf pair",
        "Boost and attenuation stay separated so the classic curve shape can be mapped cleanly.",
        "High boost band",
        "Frequency and bandwidth are exposed as stepped/detented controls.",
    )
}

#[component]
pub fn Neve1073ProfileView() -> Element {
    static PROFILE: Neve1073Profile = Neve1073Profile;

    hardware_profile_layout(
        PROFILE.name(),
        PROFILE.id(),
        PROFILE.controls(),
        "Inductor-style EQ",
        "Stepped low and mid frequencies match the expected 1073 control surface.",
        "Reference: eq1979",
        "The local GPL JSFX clone is used only as behavior/reference material, not copied code.",
    )
}

#[component]
pub fn Api550aProfileView() -> Element {
    static PROFILE: Api550aProfile = Api550aProfile;

    hardware_profile_layout(
        PROFILE.name(),
        PROFILE.id(),
        PROFILE.controls(),
        "Proportional Q",
        "Boost and cut widen or tighten with gain, matching the API workflow target.",
        "Three stepped bands",
        "The profile starts from 550A-style low, mid, and high frequency selections.",
    )
}

#[component]
pub fn SslEProfileView() -> Element {
    static PROFILE: SslEProfile = SslEProfile;

    hardware_profile_layout(
        PROFILE.name(),
        PROFILE.id(),
        PROFILE.controls(),
        "Channel EQ",
        "HPF, LPF, and four bands make this the sharper SSL channel model target.",
        "E-series behavior",
        "Calibration should preserve the more assertive bell/shelf character.",
    )
}

#[component]
pub fn SslGProfileView() -> Element {
    static PROFILE: SslGProfile = SslGProfile;

    hardware_profile_layout(
        PROFILE.name(),
        PROFILE.id(),
        PROFILE.controls(),
        "Console tone",
        "G-series mode is the smoother SSL target with broader musical shaping.",
        "Four-band EQ",
        "Calibration can share control topology with SSL E while fitting different curves.",
    )
}

fn hardware_profile_layout(
    name: &'static str,
    id: &'static str,
    controls: &'static [eq_profiles::ProfileControl],
    low_label: &'static str,
    low_note: &'static str,
    high_label: &'static str,
    high_note: &'static str,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-3",
            div { class: "flex items-baseline justify-between gap-2",
                div { class: "text-xs font-semibold uppercase tracking-wider text-foreground", "{name}" }
                div { class: "text-[10px] text-muted-foreground", "{id}" }
            }

            div {
                class: "rounded-md border border-border bg-background/40 p-3",
                div { class: "grid grid-cols-2 gap-3",
                    for (idx, control) in controls.iter().enumerate() {
                        {
                            let accent = match idx {
                                0..=2 => "border-emerald-400/60",
                                _ => "border-sky-400/60",
                            };
                            let range = control_range_label(&control.mapping);
                            rsx! {
                                div {
                                    key: "{control.id}",
                                    class: format!(
                                        "min-h-20 rounded-md border {accent} bg-card/70 p-2 flex flex-col justify-between"
                                    ),
                                    div {
                                        class: "text-[10px] uppercase tracking-wider text-muted-foreground",
                                        "{control.label}"
                                    }
                                    div {
                                        class: "text-lg font-semibold tabular-nums text-foreground",
                                        if matches!(control.mapping, ParamMapping::Stepped { .. }) {
                                            "Step"
                                        } else {
                                            "Knob"
                                        }
                                    }
                                    div {
                                        class: "text-[10px] leading-tight text-muted-foreground",
                                        "{range}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div {
                class: "grid grid-cols-2 gap-2 text-[10px] text-muted-foreground",
                div { class: "rounded border border-border bg-muted/30 p-2",
                    div { class: "font-semibold text-foreground", "{low_label}" }
                    div { "{low_note}" }
                }
                div { class: "rounded border border-border bg-muted/30 p-2",
                    div { class: "font-semibold text-foreground", "{high_label}" }
                    div { "{high_note}" }
                }
            }
        }
    }
}
