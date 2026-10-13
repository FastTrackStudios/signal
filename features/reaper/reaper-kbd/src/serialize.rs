//! Serialization logic for writing REAPER keyboard mapping files.

use crate::types::*;

impl KeyMap {
    /// Serialize this `KeyMap` back to the `reaper-kb.ini` text format.
    ///
    /// Each line is terminated with `\r\n` (CR-LF) to match REAPER's format.
    pub fn serialize(&self) -> String {
        let mut output = String::new();
        for entry in &self.entries {
            match entry {
                Entry::Key(kb) => serialize_key(kb, &mut output),
                Entry::Action(act) => serialize_act(act, &mut output),
                Entry::Script(scr) => serialize_scr(scr, &mut output),
            }
        }
        output
    }
}

fn serialize_command_id(cmd: &CommandId) -> String {
    match cmd {
        CommandId::Native(n) => n.to_string(),
        CommandId::Named(s) => s.clone(),
    }
}

fn serialize_key(kb: &KeyBinding, out: &mut String) {
    let section_raw = kb.section.to_raw();
    out.push_str(&format!(
        "KEY {} {} {} {}\r\n",
        kb.modifier,
        kb.key,
        serialize_command_id(&kb.command),
        section_raw,
    ));

    // If global, emit the paired global-flag line.
    if let Some(scope) = &kb.global {
        let global_section = match kb.section {
            Section::MainAlt => 103,
            _ => 102,
        };
        out.push_str(&format!(
            "KEY {} {} {} {}\r\n",
            kb.modifier,
            kb.key,
            scope.to_raw(),
            global_section,
        ));
    }
}

fn serialize_act(act: &CustomAction, out: &mut String) {
    let actions_str: Vec<String> = act.actions.iter().map(serialize_command_id).collect();
    out.push_str(&format!(
        "ACT {} {} \"{}\" \"{}\" {}\r\n",
        act.flags.bits(),
        act.section.to_raw(),
        act.command_id,
        act.description,
        actions_str.join(" "),
    ));
}

fn serialize_scr(scr: &ScriptBinding, out: &mut String) {
    out.push_str(&format!(
        "SCR {} {} {} \"{}\" {}\r\n",
        scr.flags.bits(),
        scr.section.to_raw(),
        scr.command_id,
        scr.description,
        scr.script_file,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_key_native() {
        let km = KeyMap {
            entries: vec![Entry::Key(KeyBinding {
                modifier: 1,
                key: 65,
                command: CommandId::Native(40001),
                section: Section::Main,
                global: None,
            })],
        };
        assert_eq!(km.serialize(), "KEY 1 65 40001 0\r\n");
    }

    #[test]
    fn serialize_key_named() {
        let km = KeyMap {
            entries: vec![Entry::Key(KeyBinding {
                modifier: 0,
                key: 83,
                command: CommandId::Named("_SWS_SAVE".to_string()),
                section: Section::Main,
                global: None,
            })],
        };
        assert_eq!(km.serialize(), "KEY 0 83 _SWS_SAVE 0\r\n");
    }

    #[test]
    fn serialize_key_global() {
        let km = KeyMap {
            entries: vec![Entry::Key(KeyBinding {
                modifier: 1,
                key: 65,
                command: CommandId::Native(40001),
                section: Section::Main,
                global: Some(GlobalScope::Global),
            })],
        };
        assert_eq!(km.serialize(), "KEY 1 65 40001 0\r\nKEY 1 65 1 102\r\n");
    }

    #[test]
    fn serialize_key_global_text_fields_main_alt() {
        let km = KeyMap {
            entries: vec![Entry::Key(KeyBinding {
                modifier: 4,
                key: 66,
                command: CommandId::Native(40002),
                section: Section::MainAlt,
                global: Some(GlobalScope::GlobalTextFields),
            })],
        };
        assert_eq!(km.serialize(), "KEY 4 66 40002 100\r\nKEY 4 66 101 103\r\n");
    }

    #[test]
    fn serialize_act() {
        let km = KeyMap {
            entries: vec![Entry::Action(CustomAction {
                flags: ActionFlags::CONSOLIDATE_UNDO | ActionFlags::SHOW_IN_MENU,
                section: Section::Main,
                command_id: "_custom_action_1".to_string(),
                description: "My Custom Action".to_string(),
                actions: vec![
                    CommandId::Native(40001),
                    CommandId::Named("_SWS_SAVE".to_string()),
                    CommandId::Native(40002),
                ],
            })],
        };
        assert_eq!(
            km.serialize(),
            "ACT 3 0 \"_custom_action_1\" \"My Custom Action\" 40001 _SWS_SAVE 40002\r\n"
        );
    }

    #[test]
    fn serialize_scr() {
        let km = KeyMap {
            entries: vec![Entry::Script(ScriptBinding {
                flags: ScriptFlags::DIALOG_IF_RUNNING,
                section: Section::Main,
                command_id: "my_script_id".to_string(),
                description: "Custom: My Script".to_string(),
                script_file: "myscript.lua".to_string(),
            })],
        };
        assert_eq!(
            km.serialize(),
            "SCR 4 0 my_script_id \"Custom: My Script\" myscript.lua\r\n"
        );
    }
}
