//! The value readout under / beside a control, and the text entry it opens.
//!
//! One component so every control types values the same way
//! (`fx.control.text-entry`): click the number → it becomes an input,
//! pre-filled and selected; Enter commits, Escape cancels, Tab commits,
//! focus-out commits; unparseable text flashes and stays open. Parsing goes
//! through [`crate::gesture::parse_typed`] so `1k`, `A4`, `2x`, `50%` work
//! on every readout in every plugin.

use crate::gesture::parse_typed;
use crate::param::ParamHandle;
use dioxus::prelude::*;

/// Inline readout that turns into a text field on click.
///
/// `open` lets a parent open it from elsewhere (Enter on the focused
/// control, right-click on a control without its own readout): pass a
/// signal, set it `true`; the readout sets it back to `false` when it closes.
// r[impl fx.control.text-entry]
#[component]
pub fn ValueReadout(
    handle: ParamHandle,
    /// Extra inline CSS for the idle span (font size, colour, width…).
    #[props(default)]
    style: Option<String>,
    #[props(default)] disabled: bool,
    /// External open request (see above). Optional.
    #[props(default)]
    open: Option<Signal<bool>>,
    /// Test id stem; the span gets `{testid}-readout`, the input
    /// `{testid}-input`.
    #[props(default)]
    testid: Option<String>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut invalid = use_signal(|| false);
    let mut draft = use_signal(String::new);
    // True until the first keystroke: the pre-filled value acts selected, so
    // typing replaces it (`fx.control.text-entry`).
    let mut pristine = use_signal(|| true);

    // Honour an external open request (in an effect: never write signals
    // during render).
    {
        let handle = handle.clone();
        use_effect(move || {
            if let Some(ext) = open {
                if *ext.read() && !*editing.peek() {
                    draft.set(handle.display_value());
                    invalid.set(false);
                    pristine.set(true);
                    editing.set(true);
                }
            }
        });
    }

    let display_value = handle.display_value();
    let is_editing = *editing.read();
    let is_invalid = *invalid.read();
    let base_style = style.unwrap_or_else(|| {
        "font-size:11px; color:var(--foreground); font-variant-numeric:tabular-nums; \
         font-weight:600; min-width:52px; text-align:center; letter-spacing:-0.01em;"
            .to_string()
    });
    let testid = testid.unwrap_or_else(|| "value".to_string());

    let mut close = move || {
        editing.set(false);
        invalid.set(false);
        if let Some(mut ext) = open {
            ext.set(false);
        }
    };

    let commit = {
        let handle = handle.clone();
        move |text: &str| -> bool {
            match parse_typed(&handle, text) {
                Some(n) => {
                    handle.set_as_gesture(n);
                    close();
                    true
                }
                None => {
                    invalid.set(true);
                    false
                }
            }
        }
    };

    if is_editing {
        let border = if is_invalid {
            "var(--destructive, #e5484d)"
        } else {
            "var(--ring, var(--primary))"
        };
        let mut commit_key = commit.clone();
        let mut commit_change = commit.clone();
        let mut commit_blur = commit;
        rsx! {
            input {
                "data-testid": "{testid}-input",
                r#type: "text",
                style: format!(
                    "font-size:11px; color:var(--foreground); \
                     background:var(--card, var(--background)); \
                     border:1px solid {border}; \
                     border-radius:4px; min-width:52px; width:64px; \
                     text-align:center; padding:2px 4px; outline:none; \
                     font-variant-numeric:tabular-nums; \
                     font-family:var(--font-sans, ui-monospace);"
                ),
                value: "{draft}",
                autofocus: true,
                onmounted: move |evt: MountedEvent| {
                    // Blitz queues focus; awaiting is the documented pattern
                    // (libs/ui/docs/blitz-diagnosis.md). Selecting the text is
                    // not exposed, so the draft is the whole value and typing
                    // replaces it on the first keystroke below.
                    spawn(async move {
                        let _ = evt.set_focus(true).await;
                    });
                },
                // The draft is managed here, from keydown, not by the input
                // element: Blitz's native text editing is not reliable across
                // hosts (audiocore-gui grew a hand-rolled caret over this),
                // and the headless test harness types by key events. Each
                // handled key is prevent_default'ed so native editing — where
                // it does work — cannot double the character.
                onkeydown: move |evt: KeyboardEvent| {
                    match evt.key() {
                        Key::Enter | Key::Tab => {
                            evt.prevent_default();
                            let text = draft.read().clone();
                            commit_key(&text);
                        }
                        Key::Escape => {
                            evt.prevent_default();
                            close();
                        }
                        Key::Backspace => {
                            evt.prevent_default();
                            // On the pristine (selected) value, backspace
                            // clears everything, like deleting a selection.
                            let mut d = if *pristine.read() {
                                String::new()
                            } else {
                                draft.read().clone()
                            };
                            pristine.set(false);
                            d.pop();
                            draft.set(d);
                            invalid.set(false);
                        }
                        Key::Character(c) => {
                            evt.prevent_default();
                            // The pre-filled value is replaced by the first
                            // keystroke (the "fully selected" behaviour —
                            // Blitz exposes no selection API).
                            let mut d = if *pristine.read() {
                                String::new()
                            } else {
                                draft.read().clone()
                            };
                            pristine.set(false);
                            d.push_str(&c);
                            draft.set(d);
                            invalid.set(false);
                        }
                        _ => {}
                    }
                },
                onchange: move |evt: FormEvent| {
                    // Native editing path, when the host supports it.
                    if !evt.value().is_empty() {
                        commit_change(&evt.value());
                    }
                },
                onfocusout: move |_| {
                    let text = draft.read().clone();
                    if !commit_blur(&text) {
                        close();
                    }
                },
            }
        }
    } else {
        rsx! {
            span {
                "data-testid": "{testid}-readout",
                style: format!("{base_style} cursor:{};", if disabled { "default" } else { "text" }),
                onclick: move |evt: MouseEvent| {
                    if disabled {
                        return;
                    }
                    evt.stop_propagation();
                    draft.set(handle.display_value());
                    invalid.set(false);
                    pristine.set(true);
                    editing.set(true);
                },
                "{display_value}"
            }
        }
    }
}
