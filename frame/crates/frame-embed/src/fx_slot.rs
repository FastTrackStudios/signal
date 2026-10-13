//! The shape of an embedded-FX provider, for hosts with a slot seam.
//!
//! session's `daw-ui` expands an FX slot into its plugin's embedded GUI
//! through a registry in context — `EmbeddedFxGuis`, keyed by a case-blind
//! fragment of the plugin name, each provider a
//! `fn(EmbeddedFx { fx, width, height }) -> Element`. frame-embed does not
//! link Dioxus; this is the provider's framework-free half, so the Dioxus
//! glue in session is a few lines (see the crate docs):
//!
//! - [`FxProviders`] maps a plugin-name fragment to a [`FxProvider`] (a
//!   function building the frame view for that FX), first match wins,
//!   specific before general — `EmbeddedFxGuis`' own rule.
//! - [`FxSlot`] is one expanded slot's state: its [`crate::Embed`], sized
//!   to the cell, with a cached record pass the component replays.

use std::sync::Arc;

use crate::canvas::Recording;
use crate::embed::{Embed, EmbedView};
use crate::form::EmbedHost;

/// Builds the view for one FX (its catalog comes with it).
pub type FxProvider = Arc<dyn Fn(&str) -> Box<dyn EmbedView> + Send + Sync>;

/// Plugin-name fragment → provider.
#[derive(Clone, Default)]
pub struct FxProviders {
    providers: Vec<(String, FxProvider)>,
}

impl std::fmt::Debug for FxProviders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.providers.iter().map(|(k, _)| k))
            .finish()
    }
}

impl FxProviders {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider for plugins whose name contains `fragment`.
    pub fn register(
        &mut self,
        fragment: impl Into<String>,
        provider: impl Fn(&str) -> Box<dyn EmbedView> + Send + Sync + 'static,
    ) {
        self.providers.push((fragment.into(), Arc::new(provider)));
    }

    /// The provider for `plugin_name` (case-blind substring, first wins).
    #[must_use]
    pub fn provider_for(&self, plugin_name: &str) -> Option<FxProvider> {
        let name = plugin_name.to_ascii_lowercase();
        self.providers
            .iter()
            .find(|(k, _)| name.contains(&k.to_ascii_lowercase()))
            .map(|(_, p)| p.clone())
    }

    /// The Tone rack's units as providers: "FTS EQ" → the EQ panel, and so
    /// on — what session registers for its own plugins.
    #[must_use]
    pub fn tone() -> Self {
        use crate::tone::{TonePanel, Which};
        let mut r = Self::new();
        for (frag, which) in [
            ("FTS EQ", Which::Eq),
            ("FTS Comp", Which::Comp),
            ("FTS Gate", Which::Gate),
            ("FTS Saturat", Which::Sat),
            ("FTS De-Ess", Which::DeEss),
            ("FTS Delay", Which::Delay),
            ("FTS Reverb", Which::Reverb),
        ] {
            r.register(frag, move |_| Box::new(TonePanel::new(which)));
        }
        r
    }
}

/// Lets a boxed view be an embed's view.
struct Boxed(Box<dyn EmbedView>);

impl EmbedView for Boxed {
    fn catalog(&self) -> surface_proto::Catalog {
        self.0.catalog()
    }
    fn build(&self, ctx: &crate::embed::BuildCtx) -> crate::embed::Built {
        self.0.build(ctx)
    }
    fn hints(&self, host: EmbedHost) -> crate::form::SizeHints {
        self.0.hints(host)
    }
    fn height_for(&self, width: f64, values: &crate::embed::Values) -> Option<f64> {
        self.0.height_for(width, values)
    }
    fn curves(&self, registry: &mut surface_runtime::CurveRegistry) {
        self.0.curves(registry);
    }
}

/// One expanded FX slot: an embed in a `width × height` cell, and its
/// record pass cached until a param moves.
pub struct FxSlot {
    pub embed: Embed,
    cached: Option<Recording>,
}

impl std::fmt::Debug for FxSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FxSlot")
            .field("embed", &self.embed)
            .finish_non_exhaustive()
    }
}

impl FxSlot {
    /// The slot for `plugin_name` in a `width × height` cell, if a provider
    /// claims it.
    #[must_use]
    pub fn open(
        providers: &FxProviders,
        plugin_name: &str,
        width: f64,
        height: f64,
    ) -> Option<Self> {
        let view = providers.provider_for(plugin_name)?(plugin_name);
        let mut embed = Embed::new(Boxed(view), EmbedHost::Strip);
        embed.set_size(width, height);
        Some(Self {
            embed,
            cached: None,
        })
    }

    /// Resize to the cell.
    pub fn resize(&mut self, width: f64, height: f64) {
        if self.embed.set_size(width, height) {
            self.cached = None;
        }
    }

    /// The record pass, re-recorded only when stale.
    pub fn recording(&mut self) -> &Recording {
        if self.cached.is_none() || self.embed.needs_record() {
            self.cached = Some(self.embed.recording());
        }
        self.cached.get_or_insert_with(Recording::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_match_like_the_registry() {
        let r = FxProviders::tone();
        assert!(r.provider_for("VST3: FTS EQ (FastTrackStudio)").is_some());
        assert!(r.provider_for("CLAP: fts comp").is_some());
        assert!(r.provider_for("ReaEQ").is_none());
        let mut slot = FxSlot::open(&r, "FTS Comp", 96.0, 170.0).expect("claimed");
        assert!(!slot.recording().is_empty());
    }
}
