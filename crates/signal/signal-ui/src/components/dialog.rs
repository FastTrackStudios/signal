//! Dialog component — modal overlay following WAI-ARIA Dialog pattern.
//!
//! Uses shadcn design tokens for consistent styling with lumen-blocks.
//! Renders an overlay + centered content panel with close button.

use dioxus::prelude::*;

/// Root container for a dialog. Controls open/close state.
#[derive(Props, Clone, PartialEq)]
pub struct DialogProps {
    /// Whether the dialog is open.
    open: bool,

    /// Callback when the dialog should close.
    #[props(default)]
    on_close: Option<Callback<()>>,

    /// Extra CSS classes for the content panel.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn Dialog(props: DialogProps) -> Element {
    if !props.open {
        return rsx! {};
    }

    rsx! {
        // Overlay
        div {
            class: "fixed inset-0 z-50 bg-black/80 animate-fade-in",
            "data-state": "open",
            onclick: move |_| {
                if let Some(cb) = &props.on_close {
                    cb.call(());
                }
            },
        }

        // Content
        div {
            class: format!(
                "fixed z-50 grid w-full max-w-lg gap-4 border border-border bg-background p-6 shadow-lg sm:rounded-lg animate-scale-in {}",
                props.class
            ),
            style: "left: 50%; top: 50%; transform: translate(-50%, -50%);",
            role: "dialog",
            aria_modal: "true",
            onclick: move |evt: MouseEvent| {
                evt.stop_propagation();
            },
            {props.children}
        }
    }
}

/// Dialog header section (title + optional description).
#[derive(Props, Clone, PartialEq)]
pub struct DialogHeaderProps {
    /// Extra CSS classes.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn DialogHeader(props: DialogHeaderProps) -> Element {
    rsx! {
        div {
            class: format!("flex flex-col space-y-1.5 text-center sm:text-left {}", props.class),
            {props.children}
        }
    }
}

/// Dialog title.
#[derive(Props, Clone, PartialEq)]
pub struct DialogTitleProps {
    /// Extra CSS classes.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn DialogTitle(props: DialogTitleProps) -> Element {
    rsx! {
        h2 {
            class: format!("text-lg font-semibold leading-none tracking-tight {}", props.class),
            {props.children}
        }
    }
}

/// Dialog description text.
#[derive(Props, Clone, PartialEq)]
pub struct DialogDescriptionProps {
    /// Extra CSS classes.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn DialogDescription(props: DialogDescriptionProps) -> Element {
    rsx! {
        p {
            class: format!("text-sm text-muted-foreground {}", props.class),
            {props.children}
        }
    }
}

/// Dialog footer section (action buttons).
#[derive(Props, Clone, PartialEq)]
pub struct DialogFooterProps {
    /// Extra CSS classes.
    #[props(default)]
    class: String,

    children: Element,
}

#[component]
pub fn DialogFooter(props: DialogFooterProps) -> Element {
    rsx! {
        div {
            class: format!("flex flex-col-reverse sm:flex-row sm:justify-end sm:space-x-2 {}", props.class),
            {props.children}
        }
    }
}

/// Close button for the dialog (X icon in top-right corner).
#[derive(Props, Clone, PartialEq)]
pub struct DialogCloseProps {
    /// Callback when clicked.
    on_click: Callback<()>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn DialogClose(props: DialogCloseProps) -> Element {
    rsx! {
        button {
            class: format!(
                "absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 {}",
                props.class
            ),
            r#type: "button",
            onclick: move |_| {
                props.on_click.call(());
            },
            // X icon using Unicode
            span {
                class: "h-4 w-4 text-lg leading-none",
                aria_hidden: "true",
                "\u{2715}"
            }
            span {
                class: "sr-only",
                "Close"
            }
        }
    }
}
