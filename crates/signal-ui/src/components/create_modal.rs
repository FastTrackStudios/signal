//! Create entity modal -- a generic form dialog for creating named entities.
//!
//! Provides a modal with fields for name, category, tags, and description,
//! plus optional template card selection. Domain-agnostic: the caller
//! configures labels and placeholders via [`ModalConfig`].

use dioxus::prelude::*;

// region: --- Config & Data Types

/// Configuration for the create modal's labels and theming.
#[derive(Clone, PartialEq)]
pub struct ModalConfig {
    /// Modal title (e.g. "New Preset", "New Profile").
    pub title: String,
    /// Placeholder text for the name field.
    pub name_placeholder: String,
    /// Placeholder text for the category field.
    pub category_placeholder: String,
    /// Tailwind accent color name (e.g. "blue", "purple", "emerald").
    pub accent: String,
}

impl Default for ModalConfig {
    fn default() -> Self {
        Self {
            title: "New Item".to_string(),
            name_placeholder: "Enter a name...".to_string(),
            category_placeholder: "Enter a category...".to_string(),
            accent: "blue".to_string(),
        }
    }
}

/// Data submitted when the user clicks Create.
#[derive(Clone, Debug, PartialEq)]
pub struct CreateModalData {
    pub name: String,
    pub category: String,
    pub description: String,
    pub tags: Vec<String>,
    /// Index of the selected template (if templates are provided).
    pub template_index: Option<usize>,
}

/// A selectable template card option.
#[derive(Clone, PartialEq)]
pub struct TemplateOption {
    /// Display name for the template.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Icon character or emoji.
    pub icon: String,
}

// endregion: --- Config & Data Types

// region: --- TemplateCard

#[component]
fn TemplateCard(
    name: String,
    description: String,
    icon: String,
    is_selected: bool,
    on_click: EventHandler<()>,
) -> Element {
    let border = if is_selected {
        "border-blue-500/60 bg-blue-500/10"
    } else {
        "border-zinc-700/40 hover:border-zinc-500/60 hover:bg-zinc-800/40"
    };

    rsx! {
        button {
            class: "flex items-center gap-3 w-full p-3 rounded-lg border {border} \
                    text-left transition-all duration-150 cursor-pointer",
            onclick: move |_| on_click.call(()),
            span { class: "text-2xl flex-shrink-0 w-9 text-center", "{icon}" }
            div { class: "flex-1 min-w-0",
                div { class: "text-sm font-medium text-zinc-200", "{name}" }
                div { class: "text-[11px] text-zinc-500 mt-0.5", "{description}" }
            }
            if is_selected {
                div { class: "w-5 h-5 rounded-full bg-blue-500 flex items-center justify-center flex-shrink-0",
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "12", height: "12",
                        view_box: "0 0 24 24",
                        fill: "none", stroke: "white", stroke_width: "3",
                        stroke_linecap: "round", stroke_linejoin: "round",
                        polyline { points: "20 6 9 17 4 12" }
                    }
                }
            }
        }
    }
}

// endregion: --- TemplateCard

// region: --- CreateModal

/// A generic modal dialog for creating a named entity with optional templates.
///
/// Renders a backdrop overlay with a centered modal containing:
/// - Optional template card selection
/// - Name, category, tags, and description form fields
/// - Cancel / Create footer buttons
#[component]
pub fn CreateModal(
    /// Modal configuration (title, placeholders, accent).
    config: ModalConfig,
    /// Whether the modal is open.
    is_open: bool,
    /// Available templates to select from. Empty = no template section.
    #[props(default)]
    templates: Vec<TemplateOption>,
    /// Callback when the user submits the form.
    on_submit: EventHandler<CreateModalData>,
    /// Callback when the user closes the modal.
    on_close: EventHandler<()>,
) -> Element {
    if !is_open {
        return rsx! {};
    }

    let accent = &config.accent;
    let has_templates = !templates.is_empty();

    let mut name = use_signal(String::new);
    let mut category = use_signal(String::new);
    let mut tags_input = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut selected_template_idx = use_signal(|| 0usize);

    let name_val = name();
    let can_submit = !name_val.trim().is_empty();

    let border_class = format!("border-{accent}-500/40");
    let focus_border =
        format!("focus:border-{accent}-500/60 focus:ring-1 focus:ring-{accent}-500/20");
    let btn_class = format!(
        "bg-{accent}-600 hover:bg-{accent}-500 disabled:opacity-30 disabled:cursor-not-allowed"
    );

    let do_submit = {
        let on_submit = on_submit.clone();
        let has_templates = has_templates;
        move |_: ()| {
            if name().trim().is_empty() {
                return;
            }
            let tags: Vec<String> = tags_input()
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();

            let template_index = if has_templates {
                Some(selected_template_idx())
            } else {
                None
            };

            on_submit.call(CreateModalData {
                name: name().trim().to_string(),
                category: category().trim().to_string(),
                description: description().trim().to_string(),
                tags,
                template_index,
            });
            name.set(String::new());
            category.set(String::new());
            tags_input.set(String::new());
            description.set(String::new());
            selected_template_idx.set(0);
        }
    };

    let modal_width = if has_templates {
        "max-w-2xl w-[80vw]"
    } else {
        "max-w-lg w-[80vw]"
    };

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    on_close.call(());
                }
            },

            // Modal card
            div {
                class: "{modal_width} max-h-[85vh] flex flex-col \
                        bg-zinc-900 border border-zinc-700/60 rounded-xl shadow-2xl shadow-black/40 \
                        overflow-hidden",
                onclick: |e| e.stop_propagation(),
                onkeydown: |e| e.stop_propagation(),
                onkeyup: |e| e.stop_propagation(),

                // Header
                div { class: "flex items-center justify-between px-6 py-4 border-b border-zinc-800",
                    h2 { class: "text-lg font-semibold text-zinc-100", "{config.title}" }
                    button {
                        class: "w-8 h-8 flex items-center justify-center rounded-lg \
                                text-zinc-500 hover:text-zinc-200 hover:bg-zinc-800 transition-colors",
                        onclick: move |_| on_close.call(()),
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "18", height: "18",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor", stroke_width: "2",
                            stroke_linecap: "round", stroke_linejoin: "round",
                            line { x1: "18", y1: "6", x2: "6", y2: "18" }
                            line { x1: "6", y1: "6", x2: "18", y2: "18" }
                        }
                    }
                }

                // Body
                div { class: "flex-1 overflow-y-auto px-6 py-5 space-y-4",

                    // Template selection (if templates provided)
                    if has_templates {
                        div {
                            label { class: "block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2",
                                "Template"
                            }
                            div { class: "space-y-1.5",
                                for (i, opt) in templates.iter().enumerate() {
                                    TemplateCard {
                                        key: "{i}",
                                        name: opt.name.clone(),
                                        description: opt.description.clone(),
                                        icon: opt.icon.clone(),
                                        is_selected: selected_template_idx() == i,
                                        on_click: move |_| selected_template_idx.set(i),
                                    }
                                }
                            }
                        }
                    }

                    // Name field
                    div {
                        label { class: "block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5",
                            "Name"
                        }
                        input {
                            class: "w-full px-4 py-2.5 text-sm bg-zinc-800/80 border {border_class} \
                                    rounded-lg text-zinc-200 placeholder:text-zinc-600 outline-none \
                                    {focus_border} transition-colors",
                            r#type: "text",
                            placeholder: "{config.name_placeholder}",
                            value: "{name}",
                            autofocus: true,
                            oninput: move |e| name.set(e.value().clone()),
                            onkeydown: {
                                let mut do_submit = do_submit.clone();
                                move |e| {
                                    if e.key() == Key::Enter {
                                        do_submit(());
                                    }
                                }
                            },
                        }
                    }

                    // Category field
                    div {
                        label { class: "block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5",
                            "Category"
                        }
                        input {
                            class: "w-full px-4 py-2.5 text-sm bg-zinc-800/80 border {border_class} \
                                    rounded-lg text-zinc-200 placeholder:text-zinc-600 outline-none \
                                    {focus_border} transition-colors",
                            r#type: "text",
                            placeholder: "{config.category_placeholder}",
                            value: "{category}",
                            oninput: move |e| category.set(e.value().clone()),
                        }
                    }

                    // Tags field
                    div {
                        label { class: "block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5",
                            "Tags"
                        }
                        input {
                            class: "w-full px-4 py-2.5 text-sm bg-zinc-800/80 border {border_class} \
                                    rounded-lg text-zinc-200 placeholder:text-zinc-600 outline-none \
                                    {focus_border} transition-colors",
                            r#type: "text",
                            placeholder: "Comma-separated, e.g. favorite, worship, sunday",
                            value: "{tags_input}",
                            oninput: move |e| tags_input.set(e.value().clone()),
                        }
                        {
                            let tags: Vec<String> = tags_input()
                                .split(',')
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty())
                                .collect();
                            if !tags.is_empty() {
                                rsx! {
                                    div { class: "flex flex-wrap gap-1.5 mt-2",
                                        for tag in tags.iter() {
                                            span {
                                                key: "{tag}",
                                                class: "text-xs px-2 py-0.5 rounded-full bg-zinc-700/80 text-zinc-300",
                                                "{tag}"
                                            }
                                        }
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                    }

                    // Description field
                    div {
                        label { class: "block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5",
                            "Description"
                        }
                        textarea {
                            class: "w-full px-4 py-2.5 text-sm bg-zinc-800/80 border {border_class} \
                                    rounded-lg text-zinc-200 placeholder:text-zinc-600 outline-none \
                                    {focus_border} resize-none transition-colors",
                            rows: "3",
                            placeholder: "Optional description...",
                            value: "{description}",
                            oninput: move |e| description.set(e.value().clone()),
                        }
                    }
                }

                // Footer
                div { class: "flex items-center justify-end gap-3 px-6 py-4 border-t border-zinc-800",
                    button {
                        class: "px-4 py-2 text-sm font-medium rounded-lg \
                                text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition-colors",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-5 py-2 text-sm font-medium rounded-lg text-white \
                                {btn_class} transition-colors",
                        disabled: !can_submit,
                        onclick: {
                            let mut do_submit = do_submit.clone();
                            move |_| do_submit(())
                        },
                        "Create"
                    }
                }
            }
        }
    }
}

// endregion: --- CreateModal
