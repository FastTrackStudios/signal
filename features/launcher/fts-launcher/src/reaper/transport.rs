//! DAW Transport provider — playback control actions.

use architect_launcher_core::{
    ActionModifier, ActivationResult, Item, ItemAction, Provider, ProviderConfig,
};

pub struct DawTransportProvider {
    config: ProviderConfig,
}

impl Default for DawTransportProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DawTransportProvider {
    pub fn new() -> Self {
        Self {
            config: ProviderConfig {
                name: "transport".into(),
                icon: "P".into(),
                prefix: None,
                default_tags: vec!["reaper/actions/transport".into()],
                ..Default::default()
            },
        }
    }
}

impl Provider for DawTransportProvider {
    fn name(&self) -> &str {
        "transport"
    }
    fn config(&self) -> &ProviderConfig {
        &self.config
    }
    fn config_mut(&mut self) -> &mut ProviderConfig {
        &mut self.config
    }

    fn query(
        &self,
        _query: &str,
        _exact: bool,
    ) -> Result<Vec<Item>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![
            Item::new("transport-play", "Play / Pause", "transport")
                .with_sub("Toggle playback")
                .with_icon("\u{25B6}")
                .with_search_fields(vec!["play".into(), "pause".into(), "playback".into()])
                .with_actions(vec![
                    ItemAction::new("Play/Pause", "daw:transport:play-pause"),
                    ItemAction::new("Stop", "daw:transport:stop")
                        .with_modifier(ActionModifier::Shift),
                ]),
            Item::new("transport-stop", "Stop", "transport")
                .with_sub("Stop playback and return to start")
                .with_icon("\u{25A0}")
                .with_search_fields(vec!["stop".into()])
                .with_actions(vec![ItemAction::new("Stop", "daw:transport:stop")]),
            Item::new("transport-record", "Record", "transport")
                .with_sub("Toggle recording")
                .with_icon("\u{2B24}")
                .with_search_fields(vec!["record".into(), "arm".into(), "rec".into()])
                .with_actions(vec![
                    ItemAction::new("Record", "daw:transport:record"),
                    ItemAction::new("Stop recording", "daw:transport:stop-recording")
                        .with_modifier(ActionModifier::Shift),
                ]),
            Item::new("transport-goto-start", "Go to Start", "transport")
                .with_sub("Move cursor to project start")
                .with_icon("\u{23EE}")
                .with_search_fields(vec!["start".into(), "beginning".into(), "rewind".into()])
                .with_actions(vec![ItemAction::new(
                    "Go to start",
                    "daw:transport:goto-start",
                )]),
            Item::new("transport-goto-end", "Go to End", "transport")
                .with_sub("Move cursor to project end")
                .with_icon("\u{23ED}")
                .with_search_fields(vec!["end".into(), "last".into()])
                .with_actions(vec![ItemAction::new("Go to end", "daw:transport:goto-end")]),
            Item::new("transport-repeat", "Toggle Repeat", "transport")
                .with_sub("Toggle loop/repeat mode")
                .with_icon("\u{1F501}")
                .with_search_fields(vec!["repeat".into(), "loop".into(), "cycle".into()])
                .with_actions(vec![ItemAction::new(
                    "Toggle",
                    "daw:transport:toggle-repeat",
                )]),
        ])
    }

    fn activate(
        &self,
        item: &Item,
        action: &str,
    ) -> Result<ActivationResult, Box<dyn std::error::Error + Send + Sync>> {
        let Some(daw) = daw::get() else {
            return Ok(ActivationResult::Close);
        };

        let exec = item
            .actions
            .iter()
            .find(|a| a.name == action)
            .map(|a| a.exec.as_str())
            .unwrap_or("");

        let cmd = exec.rsplit(':').next().unwrap_or("");

        daw::block_on(async {
            let project = daw.current_project().await?;
            let transport = project.transport();
            match cmd {
                "play-pause" => transport.play_pause().await?,
                "stop" => transport.stop().await?,
                "record" => transport.record().await?,
                "stop-recording" => transport.stop_recording().await?,
                "goto-start" => transport.goto_start().await?,
                "goto-end" => transport.goto_end().await?,
                "toggle-repeat" => {
                    transport.toggle_loop().await?;
                }
                _ => {}
            }
            Ok::<(), daw::rpc::Error>(())
        });

        Ok(ActivationResult::Close)
    }
}
