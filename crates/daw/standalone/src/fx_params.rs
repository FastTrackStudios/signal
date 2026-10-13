//! `impl FxParams for Standalone` — post-architect::rpc port.
//!
//! Service boundary is **slot-indexed and normalized** (REAPER
//! semantics: `param_idx` = 0-based position, values 0.0–1.0). When
//! the FX has a live plugin instance, slots resolve to the plugin's
//! real parameters (CLAP param id + min/max range) and the stored
//! engine value is **plain** under the **param id** — exactly what
//! the render path forwards to `process_block`. Synthetic FX (no
//! instance) fall back to identity: key = slot, value = raw.
//!
//! Every `set` publishes `FxEvent::ParameterChanged` on the FX event
//! stream (normalized value) so control surfaces / UIs track edits.

use daw_proto::FxParams;
use daw_proto::fx::{FxEvent, FxStreamEvent};
use daw_proto::{DawError, DawResult, FxChainContext, FxParameter};

use crate::plugin::PluginParamInfo;
use crate::sync::{FxChainKey, ProjectState, Standalone};

fn resolve_project(daw: &Standalone) -> Option<String> {
    let state = daw.state.lock().ok()?;
    state.current_project_guid.clone()
}

fn no_project() -> DawError {
    DawError::not_found("Project", "current")
}

fn ctx_label(ctx: &FxChainContext) -> String {
    match ctx {
        FxChainContext::Track(g) => format!("Track({g})"),
        FxChainContext::Input(g) => format!("Input({g})"),
        FxChainContext::Monitoring => "Monitoring".to_string(),
    }
}

fn entry<'p>(
    p: &'p ProjectState,
    ctx: &FxChainContext,
    fx_idx: u32,
) -> Option<&'p crate::sync::FxEntry> {
    p.fx_chains
        .get(&FxChainKey::from(ctx))
        .and_then(|chain| chain.get(fx_idx as usize))
}

/// Snapshot of one live plugin parameter, resolved by slot.
struct SlotInfo {
    info: PluginParamInfo,
    /// Current PLAIN value (stored override, else plugin-reported,
    /// else default).
    plain: f64,
    /// Plugin-formatted display text for `plain`.
    text: Option<String>,
}

impl SlotInfo {
    fn normalized(&self) -> f64 {
        let span = self.info.max - self.info.min;
        if span.abs() < f64::EPSILON {
            0.0
        } else {
            ((self.plain - self.info.min) / span).clamp(0.0, 1.0)
        }
    }

    fn denormalize(&self, norm: f64) -> f64 {
        self.info.min + norm.clamp(0.0, 1.0) * (self.info.max - self.info.min)
    }
}

impl Standalone {
    /// Resolve `(fx guid, slot)` against the live plugin instance.
    /// `None` when the FX has no loaded instance (synthetic FX).
    fn plugin_slot(&self, fx_guid: &str, slot: u32, stored: Option<f64>) -> Option<SlotInfo> {
        let mut plugins = self
            .plugin_instances
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let plugin = plugins.get_mut(fx_guid)?;
        let info = plugin.params().into_iter().nth(slot as usize)?;
        let plain = stored
            .or_else(|| plugin.param_value(info.id))
            .unwrap_or(info.default);
        let text = plugin.value_to_text(info.id, plain);
        Some(SlotInfo { info, plain, text })
    }

    /// Number of parameters the live instance exposes (`None` =
    /// synthetic FX).
    fn plugin_param_count(&self, fx_guid: &str) -> Option<u32> {
        let mut plugins = self
            .plugin_instances
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let plugin = plugins.get_mut(fx_guid)?;
        Some(plugin.params().len() as u32)
    }

    /// Recount a track's two chains, store the result, and tell the track
    /// stream.
    ///
    /// Chain membership lives in `fx_chains`, but a strip reads
    /// `Track::fx_count` — and nothing kept the two in step, so a mixer's
    /// FX buttons were correct when it opened and wrong from the first
    /// plugin added. The counts are written back onto the stored `Track`
    /// as well as published, so a bulk read taken afterwards agrees with
    /// the event.
    ///
    /// A no-op for the monitoring chain, which belongs to no track.
    pub(crate) fn republish_fx_counts(
        &self,
        project_guid: &str,
        chain: &daw_proto::FxChainContext,
    ) {
        use daw_proto::FxChainContext;

        let track_guid = match chain {
            FxChainContext::Track(g) | FxChainContext::Input(g) => g.clone(),
            FxChainContext::Monitoring => return,
        };

        let counts = self.with_project_mut(project_guid, |p| {
            let count = |ctx: FxChainContext| {
                p.fx_chains
                    .get(&FxChainKey::from(&ctx))
                    .map(|c| c.len() as u32)
                    .unwrap_or(0)
            };
            let fx_count = count(FxChainContext::Track(track_guid.clone()));
            let input_fx_count = count(FxChainContext::Input(track_guid.clone()));
            let changed = p
                .tracks
                .iter_mut()
                .find(|t| t.guid == track_guid)
                .map(|t| {
                    let changed = t.fx_count != fx_count || t.input_fx_count != input_fx_count;
                    t.fx_count = fx_count;
                    t.input_fx_count = input_fx_count;
                    changed
                })
                .unwrap_or(false);
            (changed, fx_count, input_fx_count)
        });

        let Ok((true, fx_count, input_fx_count)) = counts else {
            return;
        };
        let event = daw_proto::track::TrackStreamEvent {
            project_guid: project_guid.to_string(),
            event: daw_proto::track::TrackEvent::FxCountChanged {
                guid: track_guid,
                fx_count,
                input_fx_count,
            },
        };
        self.bus_events
            .publish(daw_proto::event_bus::DawEvent::Track(event.clone()));
        self.track_events.publish(event);
    }

    pub(crate) fn publish_fx_event(&self, project_guid: &str, event: FxEvent) {
        // FX has no per-domain stream service yet — events reach
        // consumers via the cross-domain event-bus hub.
        self.bus_events
            .publish(daw_proto::event_bus::DawEvent::Fx(FxStreamEvent {
                project_guid: project_guid.to_string(),
                event,
            }));
    }

    /// Every parameter of a live plugin instance, slot-indexed and
    /// normalized, preferring engine-stored overrides (what's audible)
    /// over the plugin's reported values. `None` = no instance
    /// (synthetic FX — caller falls back).
    pub(crate) fn slot_parameters(
        &self,
        fx_guid: &str,
        stored: &std::collections::HashMap<u32, f64>,
    ) -> Option<Vec<FxParameter>> {
        let mut plugins = self
            .plugin_instances
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let plugin = plugins.get_mut(fx_guid)?;
        let infos = plugin.params();
        let mut out = Vec::with_capacity(infos.len());
        for (slot, info) in infos.into_iter().enumerate() {
            let plain = stored
                .get(&info.id)
                .copied()
                .or_else(|| plugin.param_value(info.id))
                .unwrap_or(info.default);
            let text = plugin.value_to_text(info.id, plain);
            let s = SlotInfo { info, plain, text };
            let mut p = FxParameter::new(slot as u32, s.info.name.clone(), s.normalized());
            p.formatted = s.text.clone().unwrap_or_else(|| format!("{:.3}", s.plain));
            let choices = plugin.param_choices(s.info.id);
            if !choices.is_empty() {
                p.step_count = Some(choices.len() as u32);
                p.step_labels = choices;
            }
            out.push(p);
        }
        Some(out)
    }
}

impl FxParams for Standalone {
    fn count(&self, ctx: FxChainContext, fx_idx: u32) -> u32 {
        let Some(guid) = resolve_project(self) else {
            return 0;
        };
        let Some((fx_guid, stored_count)) = self
            .with_project(&guid, |p| {
                entry(p, &ctx, fx_idx).map(|e| (e.fx.guid.clone(), e.fx.parameter_count))
            })
            .ok()
            .flatten()
        else {
            return 0;
        };
        self.plugin_param_count(&fx_guid).unwrap_or(stored_count)
    }

    fn get(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<f64> {
        let guid = resolve_project(self)?;
        let (fx_guid, raw) = self
            .with_project(&guid, |p| {
                entry(p, &ctx, fx_idx).map(|e| (e.fx.guid.clone(), e.params.clone()))
            })
            .ok()
            .flatten()?;
        match self.plugin_slot(&fx_guid, param_idx, None) {
            Some(slot) => {
                let stored = raw.get(&slot.info.id).copied();
                let slot = SlotInfo {
                    plain: stored.unwrap_or(slot.plain),
                    ..slot
                };
                Some(slot.normalized())
            }
            // Synthetic FX: identity (key = slot, value = raw).
            None => raw.get(&param_idx).copied(),
        }
    }

    fn set(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32, value: f64) -> DawResult<()> {
        let guid = resolve_project(self).ok_or_else(no_project)?;
        let fx_guid = self
            .with_project(&guid, |p| entry(p, &ctx, fx_idx).map(|e| e.fx.guid.clone()))?
            .ok_or_else(|| DawError::not_found("Fx", &fx_idx.to_string()))?;

        // Live plugin: translate slot → param id, normalized → plain.
        let (key, stored_value) = match self.plugin_slot(&fx_guid, param_idx, None) {
            Some(slot) => (slot.info.id, slot.denormalize(value)),
            None => (param_idx, value),
        };

        self.with_project_mut(&guid, |p| {
            let chain = p
                .fx_chains
                .get_mut(&FxChainKey::from(&ctx))
                .ok_or_else(|| DawError::not_found("FxChain", &ctx_label(&ctx)))?;
            let e = chain
                .get_mut(fx_idx as usize)
                .ok_or_else(|| DawError::not_found("Fx", &fx_idx.to_string()))?;
            e.params.insert(key, stored_value);
            if param_idx + 1 > e.fx.parameter_count {
                e.fx.parameter_count = param_idx + 1;
            }
            Ok::<(), DawError>(())
        })??;

        self.publish_fx_event(
            &guid,
            FxEvent::ParameterChanged {
                context: ctx,
                fx_guid,
                param_index: param_idx,
                value,
            },
        );
        Ok(())
    }

    fn name(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<String> {
        let guid = resolve_project(self)?;
        let fx_guid = self
            .with_project(&guid, |p| entry(p, &ctx, fx_idx).map(|e| e.fx.guid.clone()))
            .ok()
            .flatten()?;
        match self.plugin_slot(&fx_guid, param_idx, None) {
            Some(slot) => Some(slot.info.name),
            None => Some(format!("Param {param_idx}")),
        }
    }

    fn info(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<FxParameter> {
        let guid = resolve_project(self)?;
        let (fx_guid, raw) = self
            .with_project(&guid, |p| {
                entry(p, &ctx, fx_idx).map(|e| (e.fx.guid.clone(), e.params.clone()))
            })
            .ok()
            .flatten()?;
        match self.plugin_slot(&fx_guid, param_idx, None) {
            Some(slot) => {
                // Prefer the engine-stored override (it's what's
                // audible) over the plugin's reported value.
                let slot = SlotInfo {
                    plain: raw.get(&slot.info.id).copied().unwrap_or(slot.plain),
                    ..slot
                };
                let mut p = FxParameter::new(param_idx, slot.info.name.clone(), slot.normalized());
                p.formatted = slot
                    .text
                    .clone()
                    .unwrap_or_else(|| format!("{:.3}", slot.plain));
                Some(p)
            }
            None => {
                let value = raw.get(&param_idx).copied().unwrap_or(0.0);
                Some(FxParameter::new(
                    param_idx,
                    format!("Param {param_idx}"),
                    value,
                ))
            }
        }
    }
}
