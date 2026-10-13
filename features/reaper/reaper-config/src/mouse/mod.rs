//! Parser and serializer for REAPER mouse modifier configuration files
//! (`reaper-mouse.ini` and `.ReaperMouseMap`).

mod parse;
mod serialize;
pub mod types;

pub use parse::ParseError;
pub use types::*;

impl MouseConfig {
    /// Parse the contents of a `reaper-mouse.ini` or `.ReaperMouseMap` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of context sections in this config.
    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    /// Return `true` if there are no context sections.
    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }

    /// Iterate over all context sections.
    pub fn iter(&self) -> impl Iterator<Item = &MouseContext> {
        self.contexts.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(input: &str) {
        let cfg = MouseConfig::parse(input).expect("parse failed");
        let output = cfg.serialize();
        assert_eq!(
            output, input,
            "round-trip mismatch\n-- got --\n{output}\n-- want --\n{input}"
        );
    }

    #[test]
    fn round_trip_empty_mouse_map() {
        round_trip("");
    }

    #[test]
    fn round_trip_single_context_mouse_behavior() {
        round_trip("[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\n\r\n");
    }

    #[test]
    fn round_trip_single_context_command_id() {
        round_trip("[MM_CTX_ITEM_CLK]\r\nmm_0=40001 c\r\n\r\n");
    }

    #[test]
    fn round_trip_single_context_named_action() {
        round_trip("[MM_CTX_ITEM_CLK]\r\nmm_4=_MY_ACTION\r\n\r\n");
    }

    #[test]
    fn round_trip_context_with_name() {
        round_trip("[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\nname=My Tooltip\r\n\r\n");
    }

    #[test]
    fn round_trip_mouse_ini_hasimported() {
        let input = "[hasimported]\r\n\
MM_CTX_ITEM=1\r\n\
MM_CTX_ITEM_CLK=1\r\n\
\r\n\
[MM_CTX_ITEM_CLK]\r\n\
mm_0=3 m\r\n\
\r\n";
        round_trip(input);
    }

    #[test]
    fn round_trip_multiple_contexts() {
        let input = "[MM_CTX_ITEM_CLK]\r\n\
mm_0=3 m\r\n\
mm_2=1 m\r\n\
\r\n\
[MM_CTX_ITEM]\r\n\
mm_0=1 m\r\n\
mm_4=4 m\r\n\
\r\n";
        round_trip(input);
    }

    #[test]
    fn round_trip_all_modifier_indices() {
        let mut input = String::from("[MM_CTX_ITEM_CLK]\r\n");
        for i in 0u8..=15 {
            input.push_str(&format!("mm_{i}={i} m\r\n"));
        }
        input.push_str("\r\n");
        round_trip(&input);
    }

    #[test]
    fn round_trip_negative_mouse_behavior() {
        round_trip("[MM_CTX_ITEM_CLK]\r\nmm_0=-1 m\r\n\r\n");
    }

    #[test]
    fn len_and_is_empty() {
        let cfg = MouseConfig::parse("").unwrap();
        assert!(cfg.is_empty());
        assert_eq!(cfg.len(), 0);

        let input = "[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\n\r\n";
        let cfg = MouseConfig::parse(input).unwrap();
        assert!(!cfg.is_empty());
        assert_eq!(cfg.len(), 1);
    }

    #[test]
    fn iter_contexts() {
        let input = "[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\n\r\n\
                     [MM_CTX_ITEM]\r\nmm_0=1 m\r\n\r\n";
        let cfg = MouseConfig::parse(input).unwrap();
        let kinds: Vec<&ContextKind> = cfg.iter().map(|c| &c.kind).collect();
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn context_lookup() {
        let input = "[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\n\r\n";
        let cfg = MouseConfig::parse(input).unwrap();
        assert!(cfg.context(&ContextKind::ItemClick).is_some());
        assert!(cfg.context(&ContextKind::Item).is_none());
    }

    #[test]
    fn binding_for_modifier() {
        let input = "[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\nmm_2=1 m\r\n\r\n";
        let cfg = MouseConfig::parse(input).unwrap();
        let ctx = cfg.context(&ContextKind::ItemClick).unwrap();

        let b = ctx.binding_for(ModifierIndex::NONE).unwrap();
        assert_eq!(b.action, ActionValue::MouseBehavior(3));

        let b = ctx.binding_for(ModifierIndex::CTRL).unwrap();
        assert_eq!(b.action, ActionValue::MouseBehavior(1));

        assert!(ctx.binding_for(ModifierIndex::ALT).is_none());
    }

    #[test]
    fn is_mouse_ini_flag() {
        let ini_input = "[hasimported]\r\nMM_CTX_ITEM=1\r\n\r\n";
        let ini = MouseConfig::parse(ini_input).unwrap();
        assert!(ini.is_mouse_ini());

        let map_input = "[MM_CTX_ITEM_CLK]\r\nmm_0=3 m\r\n\r\n";
        let map = MouseConfig::parse(map_input).unwrap();
        assert!(!map.is_mouse_ini());
    }

    #[test]
    fn new_mouse_ini_constructor() {
        let cfg = MouseConfig::new_mouse_ini();
        assert!(cfg.is_mouse_ini());
        assert!(cfg.contexts.is_empty());
    }

    #[test]
    fn new_mouse_map_constructor() {
        let cfg = MouseConfig::new_mouse_map();
        assert!(!cfg.is_mouse_ini());
        assert!(cfg.contexts.is_empty());
    }

    #[test]
    fn modifier_index_descriptions() {
        assert_eq!(ModifierIndex::NONE.description(), "Default");
        assert_eq!(ModifierIndex::SHIFT.description(), "Shift");
        assert_eq!(ModifierIndex::CTRL.description(), "Ctrl");
        assert_eq!(ModifierIndex::SHIFT_CTRL.description(), "Shift+Ctrl");
        assert_eq!(ModifierIndex::ALT.description(), "Alt");
        assert_eq!(
            ModifierIndex::SHIFT_CTRL_ALT_WIN.description(),
            "Shift+Ctrl+Alt+Win"
        );
    }

    #[test]
    fn factory_defaults_all_16_slots() {
        // Every known context must return exactly 16 defaults, all valid i32.
        let extra = [
            ContextKind::ArrangeA,
            ContextKind::ArrangeB,
            ContextKind::ArrangeC,
            ContextKind::ArrangeD,
        ];
        for kind in ContextKind::ALL.iter().chain(extra.iter()) {
            let defaults = kind.factory_defaults();
            assert_eq!(
                defaults.len(),
                16,
                "wrong length for {}",
                kind.to_reaper_string()
            );
        }
        // Spot-check a few known values from the factory dump.
        assert_eq!(ContextKind::Item.factory_defaults()[0], 13); // mm_0 = MoveItemIgnoringTimeSelection
        assert_eq!(ContextKind::ItemEdge.factory_defaults()[0], 9); // mm_0 = ResizeItemEdge
        assert_eq!(ContextKind::MidiNoteClick.factory_defaults()[0], 2); // mm_0 = SelectNoteAndMoveEditCursor
        assert_eq!(ContextKind::MidiNoteClick.factory_defaults()[4], 1); // mm_4(Alt) = SelectNote
        assert_eq!(ContextKind::ArrangeRightDrag.factory_defaults()[3], 34); // mm_3 = Shift+Ctrl
        assert_eq!(ContextKind::EnvelopeLane.factory_defaults()[0], 0); // mm_0 = no action
    }

    #[test]
    fn with_factory_defaults_populates_all_bindings() {
        let ctx = MouseContext::with_factory_defaults(ContextKind::Ruler);
        assert_eq!(ctx.bindings.len(), 16);
        assert_eq!(ctx.bindings[0].action, ActionValue::MouseBehavior(1));
        assert_eq!(ctx.bindings[1].action, ActionValue::MouseBehavior(3));
    }

    #[test]
    fn default_mouse_config_is_minimal() {
        let cfg = MouseConfig::default();
        assert!(cfg.is_mouse_ini(), "default should have [hasimported]");
        assert!(
            cfg.contexts.is_empty(),
            "default should have no explicit context sections"
        );
        let imported = cfg.has_imported.as_ref().unwrap();
        assert_eq!(imported.len(), 22);
    }

    #[test]
    fn context_kind_round_trip_all_known() {
        // Test round-trip for every entry in Context::ALL plus the A/B/C/D overrides.
        let extra = [
            ContextKind::ArrangeA,
            ContextKind::ArrangeB,
            ContextKind::ArrangeC,
            ContextKind::ArrangeD,
        ];
        for kind in ContextKind::ALL.iter().chain(extra.iter()) {
            let s = kind.to_reaper_string();
            let parsed = ContextKind::from_reaper_str(s);
            assert_eq!(&parsed, kind, "round-trip failed for {s}");
        }
    }
}
