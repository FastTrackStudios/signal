//! Song/Setlist Editor — combined song + setlist editor with drag-to-reorder.
//!
//! Each song section references a rig scene + profile patch with metadata
//! (tempo, key, notes). Setlist is an ordered list of songs for performance flow.

use dioxus::prelude::*;

/// A section within a song.
#[derive(Clone, PartialEq)]
pub struct SectionEntry {
    pub id: String,
    pub name: String,
    pub rig_scene_name: Option<String>,
    pub profile_patch_name: Option<String>,
    pub tempo: Option<u32>,
    pub key_signature: Option<String>,
    pub notes: Option<String>,
    /// Whether this section's source belongs to the song's base profile.
    /// `None` = song has no base profile, `Some(true)` = matches, `Some(false)` = overridden.
    pub is_base_profile_section: Option<bool>,
}

/// A song in the setlist.
#[derive(Clone, PartialEq)]
pub struct SongEntry {
    pub id: String,
    pub name: String,
    pub section_count: usize,
    pub duration_display: Option<String>,
}

/// A setlist entry.
#[derive(Clone, PartialEq)]
pub struct SetlistEntry {
    pub id: String,
    pub name: String,
    pub song_count: usize,
}

/// Song section editor.
#[derive(Props, Clone, PartialEq)]
pub struct SongEditorProps {
    /// Song name.
    song_name: String,

    /// Sections in order.
    sections: Vec<SectionEntry>,

    /// Currently selected section ID.
    #[props(default)]
    selected_section_id: Option<String>,

    /// Callback when a section is selected.
    #[props(default)]
    on_select_section: Option<Callback<String>>,

    /// Callback to add a new section.
    #[props(default)]
    on_add_section: Option<Callback<()>>,

    /// Callback to reorder sections.
    #[props(default)]
    on_reorder: Option<Callback<Vec<String>>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn SongEditor(props: SongEditorProps) -> Element {
    rsx! {
        div {
            class: format!("flex flex-col gap-1.5 p-2 {}", props.class),

            // Header
            div {
                class: "flex items-center justify-between",
                if !props.song_name.is_empty() {
                    h3 {
                        class: "text-xs uppercase tracking-widest text-muted-foreground font-semibold",
                        "{props.song_name}"
                    }
                }
                button {
                    class: "px-2 py-0.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_add_section {
                            cb.call(());
                        }
                    },
                    "+ Section"
                }
            }

            // Section list
            div {
                class: "flex flex-col gap-1",
                for (i, section) in props.sections.iter().enumerate() {
                    {
                        let section_id = section.id.clone();
                        let is_selected = props.selected_section_id.as_ref() == Some(&section.id);
                        let selected_class = if is_selected {
                            "border-primary bg-primary/5"
                        } else {
                            "border-border hover:border-border/80"
                        };
                        rsx! {
                            div {
                                class: format!(
                                    "flex items-start gap-2 px-2 py-1.5 rounded border cursor-pointer transition-colors {selected_class}"
                                ),
                                onclick: move |_| {
                                    if let Some(cb) = &props.on_select_section {
                                        cb.call(section_id.clone());
                                    }
                                },

                                // Section number
                                span {
                                    class: "text-xs text-muted-foreground w-4 text-right pt-0.5",
                                    {(i + 1).to_string()}
                                }

                                // Section info
                                div {
                                    class: "flex-1 flex flex-col gap-0.5",
                                    div {
                                        class: "text-xs font-medium",
                                        "{section.name}"
                                    }
                                    div {
                                        class: "flex gap-2 text-[10px] text-muted-foreground",
                                        if let Some(scene) = &section.rig_scene_name {
                                            span { "Scene: {scene}" }
                                        }
                                        if let Some(patch) = &section.profile_patch_name {
                                            span { "Patch: {patch}" }
                                        }
                                    }
                                    div {
                                        class: "flex gap-2 text-[10px] text-muted-foreground",
                                        if let Some(tempo) = section.tempo {
                                            span { "\u{266A} {tempo} BPM" }
                                        }
                                        if let Some(key) = &section.key_signature {
                                            span { "Key: {key}" }
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
}

/// Setlist editor — ordered list of songs for a performance.
#[derive(Props, Clone, PartialEq)]
pub struct SetlistEditorProps {
    /// Setlist name.
    setlist_name: String,

    /// Songs in order.
    songs: Vec<SongEntry>,

    /// Currently selected song ID.
    #[props(default)]
    selected_song_id: Option<String>,

    /// Callback when a song is selected.
    #[props(default)]
    on_select_song: Option<Callback<String>>,

    /// Callback to add a song.
    #[props(default)]
    on_add_song: Option<Callback<()>>,

    /// Callback to reorder songs.
    #[props(default)]
    on_reorder: Option<Callback<Vec<String>>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn SetlistEditor(props: SetlistEditorProps) -> Element {
    rsx! {
        div {
            class: format!("flex flex-col gap-3 p-4 {}", props.class),

            // Header
            div {
                class: "flex items-center justify-between",
                h3 {
                    class: "text-xs uppercase tracking-widest text-muted-foreground font-semibold",
                    "{props.setlist_name}"
                }
                button {
                    class: "px-2 py-0.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_add_song {
                            cb.call(());
                        }
                    },
                    "+ Song"
                }
            }

            // Song list
            div {
                class: "flex flex-col gap-1",
                for (i, song) in props.songs.iter().enumerate() {
                    {
                        let song_id = song.id.clone();
                        let is_selected = props.selected_song_id.as_ref() == Some(&song.id);
                        let selected_class = if is_selected {
                            "bg-accent text-accent-foreground"
                        } else {
                            "hover:bg-muted"
                        };
                        rsx! {
                            div {
                                class: format!(
                                    "flex items-center gap-3 px-3 py-2 rounded cursor-pointer transition-colors {selected_class}"
                                ),
                                onclick: move |_| {
                                    if let Some(cb) = &props.on_select_song {
                                        cb.call(song_id.clone());
                                    }
                                },
                                span {
                                    class: "text-xs text-muted-foreground w-5 text-right",
                                    {(i + 1).to_string()}
                                }
                                div {
                                    class: "flex-1",
                                    div {
                                        class: "text-xs font-medium",
                                        "{song.name}"
                                    }
                                    div {
                                        class: "text-[10px] text-muted-foreground",
                                        "{song.section_count} sections",
                                        if let Some(dur) = &song.duration_display {
                                            " \u{2022} {dur}"
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
}
