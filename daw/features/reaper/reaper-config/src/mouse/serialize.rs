//! Serializer: converts a [`MouseConfig`] back to the `reaper-mouse.ini` /
//! `.ReaperMouseMap` text format.

use super::types::{MouseConfig, MouseContext};

impl MouseConfig {
    /// Serialize this config to the INI text format.
    pub fn serialize(&self) -> String {
        let mut out = String::new();

        if let Some(ref entries) = self.has_imported {
            out.push_str("[hasimported]\r\n");
            for entry in entries {
                out.push_str(&format!("{}={}\r\n", entry.key, entry.value));
            }
            out.push_str("\r\n");
        }

        for ctx in &self.contexts {
            serialize_context(ctx, &mut out);
        }

        out
    }
}

fn serialize_context(ctx: &MouseContext, out: &mut String) {
    out.push_str(&format!("[{}]\r\n", ctx.kind.to_reaper_string()));

    for binding in &ctx.bindings {
        out.push_str(&format!(
            "mm_{}={}\r\n",
            binding.modifier.0,
            binding.action.to_ini_string(),
        ));
    }

    if let Some(ref name) = ctx.name {
        out.push_str(&format!("name={name}\r\n"));
    }

    out.push_str("\r\n");
}

#[cfg(test)]
mod tests {
    use super::super::types::{
        ActionValue, ContextKind, HasImportedEntry, ModifierIndex, MouseBinding, MouseConfig,
        MouseContext,
    };
    use super::*;

    #[test]
    fn serialize_empty_mouse_map() {
        let cfg = MouseConfig::new_mouse_map();
        assert_eq!(cfg.serialize(), "");
    }

    #[test]
    fn serialize_empty_mouse_ini() {
        let cfg = MouseConfig::new_mouse_ini();
        assert_eq!(cfg.serialize(), "[hasimported]\r\n\r\n");
    }

    #[test]
    fn serialize_hasimported_with_entries() {
        let mut cfg = MouseConfig::new_mouse_ini();
        cfg.has_imported = Some(vec![
            HasImportedEntry {
                key: "MM_CTX_ITEM".to_string(),
                value: 1,
            },
            HasImportedEntry {
                key: "MM_CTX_ITEM_CLK".to_string(),
                value: 1,
            },
        ]);
        let s = cfg.serialize();
        assert!(s.contains("[hasimported]\r\n"));
        assert!(s.contains("MM_CTX_ITEM=1\r\n"));
        assert!(s.contains("MM_CTX_ITEM_CLK=1\r\n"));
    }

    #[test]
    fn serialize_context_mouse_behavior() {
        let mut cfg = MouseConfig::new_mouse_map();
        let mut ctx = MouseContext::new(ContextKind::ItemClick);
        ctx.bindings.push(MouseBinding {
            modifier: ModifierIndex::NONE,
            action: ActionValue::MouseBehavior(3),
        });
        cfg.contexts.push(ctx);

        let s = cfg.serialize();
        assert!(s.contains("[MM_CTX_ITEM_CLK]\r\n"));
        assert!(s.contains("mm_0=3 m\r\n"));
    }

    #[test]
    fn serialize_context_command_id() {
        let mut cfg = MouseConfig::new_mouse_map();
        let mut ctx = MouseContext::new(ContextKind::ItemClick);
        ctx.bindings.push(MouseBinding {
            modifier: ModifierIndex::CTRL,
            action: ActionValue::CommandId(40001),
        });
        cfg.contexts.push(ctx);

        let s = cfg.serialize();
        assert!(s.contains("mm_2=40001 c\r\n"));
    }

    #[test]
    fn serialize_context_named_action() {
        let mut cfg = MouseConfig::new_mouse_map();
        let mut ctx = MouseContext::new(ContextKind::ItemClick);
        ctx.bindings.push(MouseBinding {
            modifier: ModifierIndex::ALT,
            action: ActionValue::Named("_MY_ACTION".to_string()),
        });
        cfg.contexts.push(ctx);

        let s = cfg.serialize();
        assert!(s.contains("mm_4=_MY_ACTION\r\n"));
    }

    #[test]
    fn serialize_context_with_name() {
        let mut cfg = MouseConfig::new_mouse_map();
        let mut ctx = MouseContext::new(ContextKind::ItemClick);
        ctx.name = Some("My Tooltip".to_string());
        cfg.contexts.push(ctx);

        let s = cfg.serialize();
        assert!(s.contains("name=My Tooltip\r\n"));
    }

    #[test]
    fn serialize_multiple_contexts() {
        let mut cfg = MouseConfig::new_mouse_map();
        cfg.contexts.push(MouseContext::new(ContextKind::ItemClick));
        cfg.contexts.push(MouseContext::new(ContextKind::Item));

        let s = cfg.serialize();
        let pos_clk = s.find("[MM_CTX_ITEM_CLK]").unwrap();
        let pos_item = s.find("[MM_CTX_ITEM]").unwrap();
        assert!(
            pos_clk < pos_item,
            "contexts should appear in insertion order"
        );
    }

    #[test]
    fn serialize_sections_separated_by_blank_line() {
        let mut cfg = MouseConfig::new_mouse_map();
        let mut ctx = MouseContext::new(ContextKind::ItemClick);
        ctx.bindings.push(MouseBinding {
            modifier: ModifierIndex::NONE,
            action: ActionValue::MouseBehavior(1),
        });
        cfg.contexts.push(ctx.clone());
        cfg.contexts.push(MouseContext::new(ContextKind::Item));

        let s = cfg.serialize();
        assert!(s.contains("\r\n\r\n[MM_CTX_ITEM]"));
    }
}
