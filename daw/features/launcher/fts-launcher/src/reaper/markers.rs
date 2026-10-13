//! DAW Markers and Regions provider.

use architect_launcher_core::{ActivationResult, Item, ItemAction, Provider, ProviderConfig};

pub struct DawMarkersProvider {
    config: ProviderConfig,
    items: Vec<Item>,
}

impl Default for DawMarkersProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DawMarkersProvider {
    pub fn new() -> Self {
        Self {
            config: ProviderConfig {
                name: "markers".into(),
                icon: "M".into(),
                prefix: Some('m'),
                ..Default::default()
            },
            items: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn refresh(&mut self) {
        let Some(daw) = daw::get() else {
            return;
        };

        let result = daw::block_on(async {
            let project = daw.current_project().await?;
            let markers = project.markers().all().await?;
            let regions = project.regions().all().await?;
            Ok::<_, daw::rpc::Error>((markers, regions))
        })
        .and_then(|r| r.ok());

        let Some((markers, regions)) = result else {
            return;
        };
        self.items.clear();

        for marker in &markers {
            let id = marker
                .guid
                .clone()
                .unwrap_or_else(|| format!("marker-{}", marker.id.unwrap_or(0)));
            let pos = marker
                .position
                .time
                .map(|t| format!("{:.2}s", t.as_seconds()))
                .unwrap_or_default();

            self.items.push(
                Item::new(&id, &marker.name, "markers")
                    .with_sub(format!("Marker at {pos}"))
                    .with_icon("M")
                    .with_tags(&["reaper/markers/marker"])
                    .with_search_fields(vec![marker.name.clone()])
                    .with_actions(vec![ItemAction::new(
                        "Go to",
                        format!("daw:goto-marker:{id}"),
                    )]),
            );
        }

        for region in &regions {
            let id = region
                .guid
                .clone()
                .unwrap_or_else(|| format!("region-{}", region.id.unwrap_or(0)));
            let start = region
                .time_range
                .start
                .time
                .map(|t| format!("{:.2}s", t.as_seconds()))
                .unwrap_or_default();
            let end = region
                .time_range
                .end
                .time
                .map(|t| format!("{:.2}s", t.as_seconds()))
                .unwrap_or_default();

            self.items.push(
                Item::new(&id, &region.name, "markers")
                    .with_sub(format!("Region {start} - {end}"))
                    .with_icon("R")
                    .with_tags(&["reaper/markers/region"])
                    .with_search_fields(vec![region.name.clone()])
                    .with_actions(vec![ItemAction::new(
                        "Go to",
                        format!("daw:goto-region:{id}"),
                    )]),
            );
        }

        tracing::info!(
            markers = markers.len(),
            regions = regions.len(),
            "Refreshed markers/regions"
        );
    }
}

impl Provider for DawMarkersProvider {
    fn name(&self) -> &str {
        "markers"
    }
    fn config(&self) -> &ProviderConfig {
        &self.config
    }
    fn config_mut(&mut self) -> &mut ProviderConfig {
        &mut self.config
    }

    fn setup(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!("Skipping blocking marker refresh during launcher setup");
        Ok(())
    }

    fn query(
        &self,
        _q: &str,
        _exact: bool,
    ) -> Result<Vec<Item>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.items.clone())
    }

    fn activate(
        &self,
        _item: &Item,
        _action: &str,
    ) -> Result<ActivationResult, Box<dyn std::error::Error + Send + Sync>> {
        // TODO: navigate to marker/region via daw API transport.set_position()
        Ok(ActivationResult::Close)
    }
}
