//! DAW FX provider — installed plugins from the DAW's FX database.

use architect_launcher_core::{
    ActionModifier, ActivationResult, Item, ItemAction, Provider, ProviderConfig,
};

pub struct DawFxProvider {
    config: ProviderConfig,
    plugins: Vec<Item>,
}

impl Default for DawFxProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DawFxProvider {
    pub fn new() -> Self {
        Self {
            config: ProviderConfig {
                name: "fx".into(),
                icon: "F".into(),
                prefix: Some('f'),
                ..Default::default()
            },
            plugins: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn refresh(&mut self) {
        let Some(daw) = daw::get() else {
            return;
        };

        let installed = daw::block_on(async { daw.installed_plugins().await.ok() }).flatten();
        let Some(installed) = installed else {
            return;
        };

        self.plugins = installed
            .iter()
            .map(|fx| {
                let (format, _name, developer) = parse_fx_ident(&fx.ident);

                let is_instrument = fx.ident.to_lowercase().contains("instrument")
                    || fx.ident.to_lowercase().contains("synth")
                    || fx.ident.to_lowercase().contains("vsti");

                let base_tag = if is_instrument {
                    "audio/instruments"
                } else {
                    "audio/effects"
                };
                let format_tag = match format.to_lowercase().as_str() {
                    "vst" | "vst2" => "audio/effects/plugin/vst",
                    "vst3" => "audio/effects/plugin/vst3",
                    "clap" => "audio/effects/plugin/clap",
                    "js" | "jsfx" => "audio/effects/plugin/jsfx",
                    "au" => "audio/effects/plugin/au",
                    _ => "audio/effects/plugin",
                };

                let id = format!("fx-{}", fx.ident.replace([' ', ':'], "-").to_lowercase());

                Item::new(&id, &fx.name, "fx")
                    .with_sub(format!("{developer} ({format})"))
                    .with_icon("F")
                    .with_tags(&[base_tag, format_tag])
                    .with_search_fields(vec![fx.name.clone(), developer.clone(), format.clone()])
                    .with_actions(vec![
                        ItemAction::new("Add to track", format!("daw:fx-add:{}", fx.ident)),
                        ItemAction::new(
                            "Add to new track",
                            format!("daw:fx-new-track:{}", fx.ident),
                        )
                        .with_modifier(ActionModifier::Shift),
                        ItemAction::new("Replace chain", format!("daw:fx-replace:{}", fx.ident))
                            .with_modifier(ActionModifier::CtrlShift),
                    ])
            })
            .collect();

        tracing::info!(count = self.plugins.len(), "Refreshed FX from DAW");
    }
}

impl Provider for DawFxProvider {
    fn name(&self) -> &str {
        "fx"
    }
    fn config(&self) -> &ProviderConfig {
        &self.config
    }
    fn config_mut(&mut self) -> &mut ProviderConfig {
        &mut self.config
    }

    fn setup(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!("Skipping blocking FX refresh during launcher setup");
        Ok(())
    }

    fn query(
        &self,
        _q: &str,
        _exact: bool,
    ) -> Result<Vec<Item>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.plugins.clone())
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
        let (cmd, fx_ident) = (parts[1], parts[2]);

        daw::block_on(async {
            let project = daw.current_project().await?;
            match cmd {
                "fx-add" => {
                    let selected = project.tracks().selected().await?;
                    if let Some(track) = selected.first() {
                        track.fx_chain().add(fx_ident).await?;
                    }
                }
                "fx-new-track" => {
                    let count = project.tracks().count().await?;
                    let new_track = project.tracks().add(&item.label, Some(count)).await?;
                    new_track.fx_chain().add(fx_ident).await?;
                }
                "fx-replace" => {
                    let selected = project.tracks().selected().await?;
                    for track in &selected {
                        let chain = track.fx_chain();
                        let count = chain.count().await?;
                        for i in (0..count).rev() {
                            if let Some(fx) = chain.by_index(i).await? {
                                fx.remove().await?;
                            }
                        }
                        chain.add(fx_ident).await?;
                    }
                }
                _ => {}
            }
            Ok::<(), daw::rpc::Error>(())
        });

        Ok(ActivationResult::Close)
    }
}

#[allow(dead_code)]
fn parse_fx_ident(ident: &str) -> (String, String, String) {
    let (format, rest) = ident.split_once(": ").unwrap_or(("Unknown", ident));
    let (name, developer) = if let Some(paren_start) = rest.rfind('(') {
        (
            rest[..paren_start].trim().to_string(),
            rest[paren_start + 1..]
                .trim_end_matches(')')
                .trim()
                .to_string(),
        )
    } else {
        (rest.to_string(), String::new())
    };
    (format.to_string(), name, developer)
}
