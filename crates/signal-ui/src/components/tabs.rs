//! Tabs component — accessible tabbed interface following WAI-ARIA Tabs pattern.
//!
//! Uses shadcn design tokens (bg-muted, text-muted-foreground, etc.) for
//! consistent styling with lumen-blocks.

use dioxus::prelude::*;

/// Root container for a tabbed interface.
///
/// Manages the selected tab state and provides it to children via context.
#[derive(Props, Clone, PartialEq)]
pub struct TabsProps {
    /// The key of the initially selected tab.
    #[props(default = String::new())]
    default_value: String,

    /// Callback when the selected tab changes.
    #[props(default)]
    on_change: Option<Callback<String>>,

    /// Extra CSS classes for the outer container.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn Tabs(props: TabsProps) -> Element {
    let selected = use_signal(|| props.default_value.clone());

    // Provide tab context to children
    use_context_provider(|| TabContext {
        selected,
        on_change: props.on_change,
    });

    rsx! {
        div {
            class: format!("flex flex-col {}", props.class),
            "data-orientation": "horizontal",
            {props.children}
        }
    }
}

/// Internal context shared between Tabs, TabList, TabTrigger, TabContent.
#[derive(Clone, Copy)]
struct TabContext {
    selected: Signal<String>,
    on_change: Option<Callback<String>>,
}

/// Horizontal list of tab triggers.
#[derive(Props, Clone, PartialEq)]
pub struct TabListProps {
    /// Extra CSS classes.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn TabList(props: TabListProps) -> Element {
    rsx! {
        div {
            role: "tablist",
            class: format!(
                "inline-flex h-9 items-center justify-start gap-1 rounded-lg bg-muted p-1 text-muted-foreground {}",
                props.class
            ),
            {props.children}
        }
    }
}

/// A single tab trigger button.
#[derive(Props, Clone, PartialEq)]
pub struct TabTriggerProps {
    /// Unique key matching a `TabContent` value.
    value: String,

    /// Extra CSS classes.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn TabTrigger(props: TabTriggerProps) -> Element {
    let mut ctx: TabContext = use_context();
    let is_selected = *ctx.selected.read() == props.value;

    let active_class = if is_selected {
        "bg-background text-foreground shadow-sm"
    } else {
        "hover:bg-background/50 hover:text-foreground/80"
    };

    let value = props.value.clone();

    rsx! {
        button {
            role: "tab",
            r#type: "button",
            class: format!(
                "inline-flex items-center justify-center whitespace-nowrap rounded-md px-3 py-1 text-sm font-medium transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 {active_class} {}",
                props.class
            ),
            aria_selected: is_selected.to_string(),
            "data-state": if is_selected { "active" } else { "inactive" },
            tabindex: if is_selected { "0" } else { "-1" },
            onclick: move |_| {
                let val = value.clone();
                ctx.selected.set(val.clone());
                if let Some(cb) = &ctx.on_change {
                    cb.call(val);
                }
            },
            {props.children}
        }
    }
}

/// Content panel shown when its `value` matches the selected tab.
#[derive(Props, Clone, PartialEq)]
pub struct TabContentProps {
    /// Key matching a `TabTrigger` value.
    value: String,

    /// Extra CSS classes.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn TabContent(props: TabContentProps) -> Element {
    let ctx: TabContext = use_context();
    let is_selected = *ctx.selected.read() == props.value;

    if !is_selected {
        return rsx! {};
    }

    rsx! {
        div {
            role: "tabpanel",
            class: format!(
                "mt-2 ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 {}",
                props.class
            ),
            "data-state": "active",
            tabindex: "0",
            {props.children}
        }
    }
}
