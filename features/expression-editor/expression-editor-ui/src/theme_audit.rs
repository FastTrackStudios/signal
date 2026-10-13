//! The colour-literal audit over this crate's own modules.
//!
//! The rule and the detector live in `expression_editor_paint::theme`;
//! this only lists the files that stayed on the Dioxus side.

#[cfg(test)]
mod tests {
    #[test]
    fn no_module_writes_a_hex_literal() {
        const MODULES: [(&str, &str); 6] = [
            ("lib.rs", include_str!("lib.rs")),
            ("drawer.rs", include_str!("drawer.rs")),
            ("inspector.rs", include_str!("inspector.rs")),
            ("multitool_ui.rs", include_str!("multitool_ui.rs")),
            ("toolbar.rs", include_str!("toolbar.rs")),
            ("widgets.rs", include_str!("widgets.rs")),
        ];
        let found = expression_editor_paint::theme::hex_literals(MODULES);
        assert!(
            found.is_empty(),
            "hex literals must come from daw_theme::defaults:\n  {}",
            found.join("\n  ")
        );
    }
}
