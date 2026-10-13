//! DAW Tracks provider — live track data from the current project.

use architect_launcher_core::{
    ActionModifier, ActivationResult, Item, ItemAction, Provider, ProviderConfig,
};

pub struct DawTracksProvider {
    config: ProviderConfig,
    tracks: Vec<Item>,
}

impl Default for DawTracksProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DawTracksProvider {
    pub fn new() -> Self {
        Self {
            config: ProviderConfig {
                name: "tracks".into(),
                icon: "T".into(),
                prefix: Some('t'),
                default_tags: vec!["reaper/tracks".into()],
                ..Default::default()
            },
            tracks: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn refresh(&mut self) {
        let Some(daw) = daw::get() else {
            return;
        };

        let tracks = daw::block_on(async {
            let project = daw.current_project().await.ok()?;
            project.tracks().all().await.ok()
        })
        .flatten();

        let Some(tracks) = tracks else {
            return;
        };

        self.tracks = tracks
            .iter()
            .map(|track| {
                let status = match (track.muted, track.soloed, track.armed) {
                    (true, _, _) => " [muted]",
                    (_, true, _) => " [solo]",
                    (_, _, true) => " [armed]",
                    _ => "",
                };
                let sub = format!(
                    "Track {}{}{}",
                    track.index + 1,
                    if track.is_folder { " (folder)" } else { "" },
                    status
                );

                Item::new(&track.guid, &track.name, "tracks")
                    .with_sub(&sub)
                    .with_icon(if track.is_folder { "F" } else { "T" })
                    .with_search_fields(vec![track.name.clone()])
                    .with_actions(vec![
                        ItemAction::new("Select", format!("daw:track-select:{}", track.guid)),
                        ItemAction::new("Solo", format!("daw:track-solo:{}", track.guid))
                            .with_modifier(ActionModifier::Shift)
                            .with_keep_open(),
                        ItemAction::new("Mute", format!("daw:track-mute:{}", track.guid))
                            .with_modifier(ActionModifier::Ctrl)
                            .with_keep_open(),
                        ItemAction::new("Show FX", format!("daw:track-fx:{}", track.guid))
                            .with_modifier(ActionModifier::Alt),
                    ])
            })
            .collect();

        tracing::info!(count = self.tracks.len(), "Refreshed tracks from DAW");
    }
}

impl Provider for DawTracksProvider {
    fn name(&self) -> &str {
        "tracks"
    }
    fn config(&self) -> &ProviderConfig {
        &self.config
    }
    fn config_mut(&mut self) -> &mut ProviderConfig {
        &mut self.config
    }

    fn setup(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!("Skipping blocking track refresh during launcher setup");
        Ok(())
    }

    fn query(
        &self,
        _q: &str,
        _exact: bool,
    ) -> Result<Vec<Item>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.tracks.clone())
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
        let parts: Vec<&str> = exec.splitn(3, ':').collect();
        if parts.len() < 3 {
            return Ok(ActivationResult::Close);
        }
        let (cmd, guid) = (parts[1], parts[2]);

        let keep_open = daw::block_on(async {
            let project = daw.current_project().await?;
            let track = project.tracks().by_guid(guid).await?;
            let Some(track) = track else {
                return Ok::<bool, daw::rpc::Error>(false);
            };
            match cmd {
                "track-select" => {
                    track.select().await?;
                    Ok(false)
                }
                "track-solo" => {
                    track.toggle_solo().await?;
                    Ok(true)
                }
                "track-mute" => {
                    track.toggle_mute().await?;
                    Ok(true)
                }
                "track-fx" => {
                    // Select the track first, then show FX via action
                    track.select_exclusive().await?;
                    daw.action_registry().run_action(40291).await?; // Show FX chain for selected track
                    Ok(false)
                }
                _ => Ok(false),
            }
        });

        match keep_open {
            Some(Ok(true)) => Ok(ActivationResult::KeepOpen),
            _ => Ok(ActivationResult::Close),
        }
    }
}
