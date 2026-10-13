//! `impl FxChains for Standalone` — post-architect::rpc port.
//!
//! Backed by `ProjectState::fx_chains: HashMap<FxChainKey, Vec<FxEntry>>`.
//! Each `FxChainContext` maps to a `FxChainKey` for storage; the owning
//! project is resolved through the current project guid.

use daw_proto::FxChains;
use daw_proto::{DawError, DawResult, Fx, FxChainContext};

use crate::sync::{FxChainKey, FxEntry, Standalone};

fn resolve_project(daw: &Standalone) -> Option<String> {
    let state = daw.state.lock().ok()?;
    state.current_project_guid.clone()
}

fn no_project() -> DawError {
    DawError::not_found("Project", "current")
}

fn chain_key(ctx: &FxChainContext) -> FxChainKey {
    FxChainKey::from(ctx)
}

impl FxChains for Standalone {
    fn list(&self, ctx: FxChainContext) -> Vec<Fx> {
        let Some(guid) = resolve_project(self) else {
            return Vec::new();
        };
        self.with_project(&guid, |p| {
            p.fx_chains
                .get(&chain_key(&ctx))
                .map(|chain| chain.iter().map(|e| e.fx.clone()).collect())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn count(&self, ctx: FxChainContext) -> u32 {
        let Some(guid) = resolve_project(self) else {
            return 0;
        };
        self.with_project(&guid, |p| {
            p.fx_chains
                .get(&chain_key(&ctx))
                .map(|c| c.len() as u32)
                .unwrap_or(0)
        })
        .unwrap_or(0)
    }

    fn get(&self, ctx: FxChainContext, fx_idx: u32) -> Option<Fx> {
        let guid = resolve_project(self)?;
        self.with_project(&guid, |p| {
            p.fx_chains
                .get(&chain_key(&ctx))?
                .get(fx_idx as usize)
                .map(|e| e.fx.clone())
        })
        .ok()
        .flatten()
    }

    fn name(&self, ctx: FxChainContext, fx_idx: u32) -> Option<String> {
        <Self as FxChains>::get(self, ctx, fx_idx).map(|fx| fx.name)
    }

    fn add(&self, ctx: FxChainContext, name: &str) -> DawResult<u32> {
        let guid = resolve_project(self).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            let counter = p.next_fx_counter;
            p.next_fx_counter += 1;
            let chain = p.fx_chains.entry(chain_key(&ctx)).or_default();
            let mut fx = Fx::default();
            fx.guid = format!("standalone-fx-{counter}");
            fx.name = name.to_string();
            fx.index = chain.len() as u32;
            let entry = FxEntry {
                fx,
                state_chunk: String::new(),
                params: Default::default(),
                ranges: Vec::new(),
            };
            chain.push(entry);
            chain.len() as u32 - 1
        })
    }

    fn remove(&self, ctx: FxChainContext, fx_idx: u32) -> DawResult<()> {
        let guid = resolve_project(self).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            let chain = p
                .fx_chains
                .get_mut(&chain_key(&ctx))
                .ok_or_else(|| DawError::not_found("FxChain", "context"))?;
            let i = fx_idx as usize;
            if i >= chain.len() {
                return Err(DawError::out_of_range(fx_idx, chain.len() as u32, "fx_idx"));
            }
            chain.remove(i);
            Ok::<(), DawError>(())
        })?
    }

    fn move_to(&self, ctx: FxChainContext, from_idx: u32, to_idx: u32) -> DawResult<()> {
        let guid = resolve_project(self).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            let chain = p
                .fx_chains
                .get_mut(&chain_key(&ctx))
                .ok_or_else(|| DawError::not_found("FxChain", "context"))?;
            let len = chain.len() as u32;
            if from_idx >= len {
                return Err(DawError::out_of_range(from_idx, len, "from_idx"));
            }
            if to_idx >= len {
                return Err(DawError::out_of_range(to_idx, len, "to_idx"));
            }
            let entry = chain.remove(from_idx as usize);
            chain.insert(to_idx as usize, entry);
            Ok::<(), DawError>(())
        })?
    }

    fn rename(&self, ctx: FxChainContext, fx_idx: u32, name: &str) -> DawResult<()> {
        let guid = resolve_project(self).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            let chain = p
                .fx_chains
                .get_mut(&chain_key(&ctx))
                .ok_or_else(|| DawError::not_found("FxChain", "context"))?;
            let len = chain.len() as u32;
            let e = chain
                .get_mut(fx_idx as usize)
                .ok_or_else(|| DawError::out_of_range(fx_idx, len, "fx_idx"))?;
            e.fx.name = name.to_string();
            Ok::<(), DawError>(())
        })?
    }

    fn set_enabled(&self, ctx: FxChainContext, fx_idx: u32, enabled: bool) -> DawResult<()> {
        let guid = resolve_project(self).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            let chain = p
                .fx_chains
                .get_mut(&chain_key(&ctx))
                .ok_or_else(|| DawError::not_found("FxChain", "context"))?;
            let len = chain.len() as u32;
            let e = chain
                .get_mut(fx_idx as usize)
                .ok_or_else(|| DawError::out_of_range(fx_idx, len, "fx_idx"))?;
            e.fx.enabled = enabled;
            Ok::<(), DawError>(())
        })?
    }

    fn set_online(&self, ctx: FxChainContext, fx_idx: u32, online: bool) -> DawResult<()> {
        let guid = resolve_project(self).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            let chain = p
                .fx_chains
                .get_mut(&chain_key(&ctx))
                .ok_or_else(|| DawError::not_found("FxChain", "context"))?;
            let len = chain.len() as u32;
            let e = chain
                .get_mut(fx_idx as usize)
                .ok_or_else(|| DawError::out_of_range(fx_idx, len, "fx_idx"))?;
            e.fx.offline = !online;
            Ok::<(), DawError>(())
        })?
    }

    fn set_show_ui(&self, _ctx: FxChainContext, _fx_idx: u32, _show: bool) -> DawResult<()> {
        // Standalone has no UI to toggle; no-op.
        Ok(())
    }

    fn state_chunk(&self, ctx: FxChainContext, fx_idx: u32) -> Option<String> {
        let guid = resolve_project(self)?;
        self.with_project(&guid, |p| {
            p.fx_chains
                .get(&chain_key(&ctx))?
                .get(fx_idx as usize)
                .map(|e| e.state_chunk.clone())
        })
        .ok()
        .flatten()
    }

    fn set_state_chunk(&self, ctx: FxChainContext, fx_idx: u32, chunk: &str) -> DawResult<()> {
        let guid = resolve_project(self).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            let chain = p
                .fx_chains
                .get_mut(&chain_key(&ctx))
                .ok_or_else(|| DawError::not_found("FxChain", "context"))?;
            let len = chain.len() as u32;
            let e = chain
                .get_mut(fx_idx as usize)
                .ok_or_else(|| DawError::out_of_range(fx_idx, len, "fx_idx"))?;
            e.state_chunk = chunk.to_string();
            Ok::<(), DawError>(())
        })?
    }
}
