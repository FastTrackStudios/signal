//! DawModule implementation for reaper-input.

use daw::module::{ActionDef, DawModule, ModuleContext, PanelDef};

use crate::input::actions::{
    get_dynamic_preset_action_defs, get_dynamic_workflow_action_defs, get_input_action_defs,
};

pub struct InputModule;

impl DawModule for InputModule {
    fn name(&self) -> &str {
        "input"
    }
    fn display_name(&self) -> &str {
        "FTS Input"
    }

    fn actions(&self) -> Vec<ActionDef> {
        // Static action defs are owned `'static` strings; the dynamic
        // (preset + workflow) defs are owned `String`s. Both shapes
        // adapt cleanly into a `daw::module::ActionDef`.
        let static_iter = get_input_action_defs().into_iter().map(|def| {
            let handler = def.handler;
            let appears_in_menu = def.appears_in_menu;
            let is_toggleable = def.toggle_state.is_some();
            let mut action =
                ActionDef::new(def.command_id.to_string(), def.display_name, move || {
                    handler()
                });
            if appears_in_menu {
                action = action.in_menu();
            }
            if is_toggleable {
                action = action.toggleable();
            }
            action
        });

        let dyn_iter = get_dynamic_preset_action_defs()
            .into_iter()
            .chain(get_dynamic_workflow_action_defs())
            .map(|def| {
                let handler = def.handler;
                let appears_in_menu = def.appears_in_menu;
                let is_toggleable = def.toggle_state.is_some();
                let mut action =
                    ActionDef::new(def.command_id, def.display_name, move || handler());
                if appears_in_menu {
                    action = action.in_menu();
                }
                if is_toggleable {
                    action = action.toggleable();
                }
                action
            });

        static_iter.chain(dyn_iter).collect()
    }

    fn panels(&self) -> Vec<PanelDef> {
        Vec::new()
    }

    fn init(&self, _ctx: &ModuleContext) {
        crate::bootstrap_in_process_runtime();
        daw::register_timer(crate::check_config_reload);
        // Re-hook arrange + MIDI-editor windows every tick. MIDI editors open
        // and close after enable-time, so their wheel hook must be (re)checked
        // periodically — otherwise scrolls there bypass the plugin entirely.
        daw::register_timer(crate::check_and_hook_windows);
        crate::trace_console_msg("FTS Input reload watcher armed\n");
        tracing::info!("[input] Module initialized");
    }
}

pub fn module() -> Box<dyn DawModule> {
    Box::new(InputModule)
}
