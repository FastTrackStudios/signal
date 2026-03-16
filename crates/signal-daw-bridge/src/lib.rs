//! DAW-to-Signal bridge -- infers signal domain structure from REAPER FX trees.
//!
//! Translates a REAPER track's FX container hierarchy (`FxTree` from `daw-proto`)
//! into signal domain concepts: modules, blocks, signal chains, and routing
//! topology. This is the primary mechanism for discovering an existing rig's
//! structure from a live DAW session.
//!
//! # Architecture position
//!
//! ```text
//! daw-proto + signal-proto + signal-import
//!                |
//!                v
//!     signal-daw-bridge (this crate)
//!                |
//!                v
//!          signal-ui
//! ```
//!
//! **Depends on**: `daw-proto`, `signal-proto`, `signal-import`
//!
//! **Depended on by**: `signal-ui`
//!
//! # Key types and functions
//!
//! - [`infer_chain_from_fx_tree`] -- core inference: converts an `FxTree` into an
//!   [`InferredChain`] of modules and standalone blocks
//! - [`InferredChain`] -- result containing inferred modules and standalone blocks
//! - [`InferredModule`] -- a module inferred from a top-level FX container, with
//!   its signal chain and routing mode
//! - [`InferredBlock`] -- a standalone block inferred from a top-level flat plugin
//!
//! # Inference rules
//!
//! - Depth-0 flat plugin -> standalone block
//! - Depth-0 container -> Module (type inferred from `[M]`/`[B]` naming convention)
//! - Parallel routing -> `SignalChain` with a `Split` node
//! - Plugin names matched against the FabFilter registry and FTS naming conventions

use daw::service::fx::tree::{FxNode, FxNodeKind, FxRoutingMode, FxTree};
use signal_proto::plugin_block::FxRole;
use signal_proto::{
    Block, BlockType, ModuleBlock, ModuleBlockSource, ModuleType, SignalChain, SignalNode,
};

// ─── Result types ──────────────────────────────────────────────

/// Result of inferring a signal chain from a REAPER FX tree.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferredChain {
    /// Modules (from top-level containers).
    pub modules: Vec<InferredModule>,
    /// Standalone blocks (top-level flat plugins, not inside any container).
    pub standalone_blocks: Vec<InferredBlock>,
}

/// A module inferred from a top-level FX container.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferredModule {
    pub name: String,
    pub module_type: ModuleType,
    /// Full signal chain — may be serial or parallel (Split).
    pub chain: SignalChain,
    pub enabled: bool,
    /// Raw REAPER plugin identifiers, parallel to `chain.blocks()` order.
    pub block_plugin_names: Vec<String>,
}

/// A standalone block inferred from a top-level flat plugin.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferredBlock {
    pub name: String,
    pub block_type: BlockType,
    /// Raw REAPER plugin identifier (e.g. "CLAP: Pro-Q 4 (FabFilter)").
    pub plugin_name: String,
    pub enabled: bool,
}

// ─── Core inference ────────────────────────────────────────────

/// Infer the signal chain structure from a REAPER FX tree.
///
/// Mapping rules:
/// - Depth-0 **flat plugin** → standalone block
/// - Depth-0 **container** → Module (type inferred from container name)
///   - Depth-1 **plugin** → Block within that module
///   - Depth-1 **sub-container** → Block (multi-FX); its children are ignored
/// - `FxRoutingMode::Parallel` → `SignalChain` with a `Split` node
/// - `FxRoutingMode::Serial` → sequential `SignalChain`
pub fn infer_chain_from_fx_tree(tree: &FxTree) -> InferredChain {
    let mut modules = Vec::new();
    let mut standalone_blocks = Vec::new();

    for node in &tree.nodes {
        match &node.kind {
            FxNodeKind::Plugin(fx) => {
                standalone_blocks.push(InferredBlock {
                    name: fx.name.clone(),
                    block_type: infer_block_type(&fx.plugin_name, &fx.name),
                    plugin_name: fx.plugin_name.clone(),
                    enabled: node.enabled,
                });
            }
            FxNodeKind::Container {
                name,
                routing,
                children,
                ..
            } => {
                let module_type = infer_module_type(name);
                let (chain, block_plugin_names) = build_module_chain(children, routing);
                modules.push(InferredModule {
                    name: clean_container_name(name),
                    module_type,
                    chain,
                    enabled: node.enabled,
                    block_plugin_names,
                });
            }
        }
    }

    InferredChain {
        modules,
        standalone_blocks,
    }
}

// ─── Internal helpers ──────────────────────────────────────────

fn build_module_chain(
    children: &[FxNode],
    routing: &FxRoutingMode,
) -> (SignalChain, Vec<String>) {
    let mut blocks = Vec::new();
    let mut plugin_names = Vec::new();

    for child in children {
        match &child.kind {
            FxNodeKind::Plugin(fx) => {
                blocks.push(ModuleBlock::new(
                    &fx.guid,
                    &fx.name,
                    infer_block_type(&fx.plugin_name, &fx.name),
                    ModuleBlockSource::Inline {
                        block: Block::default(),
                    },
                ));
                plugin_names.push(fx.plugin_name.clone());
            }
            FxNodeKind::Container { name, .. } => {
                blocks.push(ModuleBlock::new(
                    name,
                    &clean_container_name(name),
                    infer_block_type_from_name(name),
                    ModuleBlockSource::Inline {
                        block: Block::default(),
                    },
                ));
                // Sub-containers are loaded as REAPER Container FX.
                plugin_names.push("Container".to_string());
            }
        }
    }

    let chain = match routing {
        FxRoutingMode::Parallel => SignalChain::new(vec![SignalNode::Split {
            lanes: blocks
                .into_iter()
                .map(|b| SignalChain::new(vec![SignalNode::Block(b)]))
                .collect(),
        }]),
        FxRoutingMode::Serial => {
            SignalChain::new(blocks.into_iter().map(SignalNode::Block).collect())
        }
    };
    (chain, plugin_names)
}

/// Infer block type from REAPER plugin identifiers.
///
/// Priority:
/// 1. FabFilter registry (matches "CLAP: Pro-Q 4 (FabFilter)" etc.)
/// 2. FTS naming convention ("EQ Block: ...")
/// 3. Fallback → Custom
fn infer_block_type(plugin_name: &str, display_name: &str) -> BlockType {
    // 1. FabFilter registry lookup
    if let Some(entry) = signal_import::fabfilter::registry::lookup_by_reaper_name(plugin_name) {
        return entry.block_type;
    }
    // 2. FTS "Type Block: name" convention
    if let FxRole::Block { block_type, .. } = FxRole::parse(display_name) {
        return block_type;
    }
    // 3. FTS "[B] Type: name" convention — strip prefix, parse type before `:`
    let stripped = strip_role_prefix(display_name);
    if let Some(block_type) = parse_type_colon_prefix(stripped) {
        return block_type;
    }
    // 4. Unknown
    BlockType::Custom
}

/// Infer block type from a container name (depth-1 sub-containers acting as
/// multi-FX blocks).
fn infer_block_type_from_name(container_name: &str) -> BlockType {
    // Try FxRole::parse directly — handles "[B] Type Block: name"
    match FxRole::parse(container_name) {
        FxRole::Block { block_type, .. } => return block_type,
        _ => {}
    }
    // Fallback: strip [B] prefix, parse "Type: name" where prefix is the block type
    let stripped = strip_role_prefix(container_name);
    if let Some(block_type) = parse_type_colon_prefix(stripped) {
        return block_type;
    }
    BlockType::Custom
}

/// Infer module type from a container name.
///
/// Handles two naming conventions:
/// - `"[M] DRIVE Module: Hype"` → parsed by FxRole::parse directly
/// - `"[M] Drive: Hype"` → strip prefix, parse type before `:`
/// - `"DRIVE"` → synthetic "DRIVE Module: _" parse
fn infer_module_type(container_name: &str) -> ModuleType {
    // 1. Try FxRole::parse directly — handles "[M] TYPE Module: name"
    match FxRole::parse(container_name) {
        FxRole::Module { module_type, .. } => return module_type,
        _ => {}
    }
    // 2. Strip [M]/[B] prefix, then parse "Type: name" where prefix is the module type
    let stripped = strip_role_prefix(container_name);
    if let Some((type_part, _)) = stripped.split_once(':') {
        let prefix = type_part.trim().to_uppercase();
        if !prefix.is_empty() {
            // Reuse FxRole's infer_module_type via synthetic parse
            let synthetic = format!("{prefix} Module: _");
            if let FxRole::Module { module_type, .. } = FxRole::parse(&synthetic) {
                return module_type;
            }
        }
    }
    // 3. Bare name without colon (e.g. "DRIVE") — try synthetic
    let synthetic = format!("{} Module: _", stripped.trim().to_uppercase());
    if let FxRole::Module { module_type, .. } = FxRole::parse(&synthetic) {
        return module_type;
    }
    ModuleType::Custom
}

/// Strip the `[M] ` or `[B] ` prefix if present.
fn strip_role_prefix(name: &str) -> &str {
    name.strip_prefix("[M] ")
        .or_else(|| name.strip_prefix("[B] "))
        .unwrap_or(name)
}

/// Parse "Type: name" and return the BlockType from the prefix before `:`.
fn parse_type_colon_prefix(s: &str) -> Option<BlockType> {
    let (type_part, _) = s.split_once(':')?;
    let prefix = type_part.trim();
    if prefix.is_empty() {
        return None;
    }
    BlockType::from_str_lenient(&prefix.to_lowercase())
}

/// Extract the user-facing name from a container's raw name.
///
/// Handles:
/// - `"[M] DRIVE Module: Hype"` → `"Hype"`
/// - `"[M] Drive: Hype"` → `"Hype"`
/// - `"DRIVE"` → `"DRIVE"`
fn clean_container_name(raw: &str) -> String {
    // FxRole::parse handles both "[M] TYPE Module: name" and "[M] Type: name" (as Unknown)
    match FxRole::parse(raw) {
        FxRole::Module { name, .. } | FxRole::Block { name, .. } | FxRole::GenericModule { name } => {
            return name;
        }
        FxRole::Unknown { .. } => {}
    }
    // Fallback: strip prefix, then take everything after `:` if present
    let stripped = strip_role_prefix(raw);
    if let Some((_, name_part)) = stripped.split_once(':') {
        let name = name_part.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    stripped.trim().to_string()
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use daw::service::fx::tree::{FxContainerChannelConfig, FxNodeId};
    use daw::service::fx::Fx;

    fn plugin_node(guid: &str, name: &str, plugin_name: &str) -> FxNode {
        FxNode {
            id: FxNodeId(guid.to_string()),
            kind: FxNodeKind::Plugin(Fx {
                guid: guid.to_string(),
                index: 0,
                name: name.to_string(),
                plugin_name: plugin_name.to_string(),
                plugin_type: daw::service::fx::FxType::default(),
                enabled: true,
                offline: false,
                window_open: false,
                parameter_count: 0,
                preset_name: None,
            }),
            enabled: true,
            parent_id: None,
        }
    }

    fn container_node(name: &str, routing: FxRoutingMode, children: Vec<FxNode>) -> FxNode {
        FxNode {
            id: FxNodeId(format!("container:{name}")),
            kind: FxNodeKind::Container {
                name: name.to_string(),
                children,
                routing,
                channel_config: FxContainerChannelConfig::default(),
            },
            enabled: true,
            parent_id: None,
        }
    }

    #[test]
    fn standalone_plugin_becomes_block() {
        let tree = FxTree {
            nodes: vec![plugin_node("g1", "My Plugin", "VST: My Plugin")],
        };
        let chain = infer_chain_from_fx_tree(&tree);
        assert_eq!(chain.modules.len(), 0);
        assert_eq!(chain.standalone_blocks.len(), 1);
        assert_eq!(chain.standalone_blocks[0].name, "My Plugin");
        assert_eq!(chain.standalone_blocks[0].block_type, BlockType::Custom);
    }

    #[test]
    fn container_becomes_module_with_blocks() {
        let tree = FxTree {
            nodes: vec![container_node(
                "DRIVE",
                FxRoutingMode::Serial,
                vec![
                    plugin_node("g1", "Drive Block: Screamer", "VST: TS808"),
                    plugin_node("g2", "Drive Block: Boost", "VST: Boost"),
                ],
            )],
        };
        let chain = infer_chain_from_fx_tree(&tree);
        assert_eq!(chain.modules.len(), 1);
        assert_eq!(chain.modules[0].name, "DRIVE");
        assert_eq!(chain.modules[0].module_type, ModuleType::Drive);
        assert_eq!(chain.modules[0].chain.blocks().len(), 2);
    }

    #[test]
    fn parallel_routing_creates_split() {
        let tree = FxTree {
            nodes: vec![container_node(
                "EQ",
                FxRoutingMode::Parallel,
                vec![
                    plugin_node("g1", "EQ Block: Low", "VST: EQ"),
                    plugin_node("g2", "EQ Block: High", "VST: EQ"),
                ],
            )],
        };
        let chain = infer_chain_from_fx_tree(&tree);
        let module = &chain.modules[0];
        assert!(!module.chain.is_serial());
    }

    #[test]
    fn fabfilter_plugin_infers_block_type() {
        let bt = infer_block_type("CLAP: Pro-Q 4 (FabFilter)", "Some Name");
        assert_eq!(bt, BlockType::Eq);
    }

    #[test]
    fn module_type_inference() {
        // Bare names
        assert_eq!(infer_module_type("DRIVE"), ModuleType::Drive);
        assert_eq!(infer_module_type("AMP"), ModuleType::Amp);
        assert_eq!(infer_module_type("TIME"), ModuleType::Time);
        assert_eq!(infer_module_type("My Custom"), ModuleType::Custom);
        // [M] prefix + colon format (real REAPER container names)
        assert_eq!(infer_module_type("[M] Drive: Hype"), ModuleType::Drive);
        assert_eq!(infer_module_type("[M] AMP: Fender Deluxe"), ModuleType::Amp);
        assert_eq!(infer_module_type("[M] TIME: Timeless + BigSky"), ModuleType::Time);
        assert_eq!(infer_module_type("[M] Modulation: Chorus"), ModuleType::Modulation);
        assert_eq!(infer_module_type("[M] Master: Light Polish"), ModuleType::Master);
        assert_eq!(infer_module_type("[M] Dynamics: Compressor"), ModuleType::Dynamics);
        assert_eq!(infer_module_type("[M] Input: Default"), ModuleType::Input);
        // Full "Module:" keyword format
        assert_eq!(infer_module_type("DRIVE Module: Main"), ModuleType::Drive);
        assert_eq!(infer_module_type("[M] DRIVE Module: Main"), ModuleType::Drive);
    }

    #[test]
    fn clean_container_name_extracts_name() {
        // "Type Module: Name" → "Name"
        assert_eq!(clean_container_name("DRIVE Module: Main"), "Main");
        // "[M] Type: Name" → "Name"
        assert_eq!(clean_container_name("[M] Drive: Hype"), "Hype");
        assert_eq!(clean_container_name("[M] AMP: Fender Deluxe"), "Fender Deluxe");
        // Bare name
        assert_eq!(clean_container_name("AMP"), "AMP");
    }

    #[test]
    fn block_type_from_display_name() {
        // [B] prefix blocks
        assert_eq!(infer_block_type("VST: Whatever", "[B] Drive: Klone - Medium"), BlockType::Drive);
        assert_eq!(infer_block_type("VST: Whatever", "[B] Reverb: Pro R2"), BlockType::Reverb);
        assert_eq!(infer_block_type("VST: Whatever", "[B] Delay: Timeless 3 - Tape"), BlockType::Delay);
        assert_eq!(infer_block_type("VST: Whatever", "[B] Amp: Fender NAM"), BlockType::Amp);
        // "Block:" keyword format still works
        assert_eq!(infer_block_type("VST: Whatever", "Drive Block: Screamer"), BlockType::Drive);
    }
}
