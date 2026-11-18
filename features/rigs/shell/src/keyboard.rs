//! A text field's hold on the keyboard, so global hotkeys stand aside
//! while it is typed into (`utils::keyboard`).
//!
//! ```ignore
//! let keys = use_text_focus();
//! rsx! { input { onfocus: move |_| keys.focus(), onblur: move |_| keys.blur(), … } }
//! ```
//!
//! The hold goes when the field loses focus or unmounts, whichever is
//! first: a field closed mid-edit never leaves the keyboard taken.
//!
//! On dioxus-native the renderer does this for every text input itself
//! (`app_shell::ui::keyboard::follow_text_fields`, called once before
//! launch); the hook stays for fields that are not text inputs to the
//! renderer, and for other hosts.

use dioxus::prelude::*;
use utils::keyboard::{Claim, claim};

/// One text field's hold on the keyboard.
#[derive(Clone, Copy)]
pub struct TextFocus(CopyValue<Option<Claim>>);

impl TextFocus {
    /// The field has focus: global hotkeys stand aside.
    pub fn focus(&self) {
        let mut held = self.0;
        if held.peek().is_none() {
            held.set(Some(claim()));
        }
    }

    /// The field lost focus: hotkeys act again (once nothing else holds the
    /// keyboard).
    pub fn blur(&self) {
        let mut held = self.0;
        held.set(None);
    }
}

/// A [`TextFocus`] for the field this component draws.
#[must_use]
pub fn use_text_focus() -> TextFocus {
    let held = use_hook(|| CopyValue::new(None::<Claim>));
    use_drop(move || {
        let mut held = held;
        held.set(None);
    });
    TextFocus(held)
}
