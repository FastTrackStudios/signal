//! Signal chain topology — recursive tree model for parallel routing.
//!
//! A [`SignalChain`] is an ordered sequence of [`SignalNode`]s. Each node is
//! either a single processing [`Block`](SignalNode::Block) or a
//! [`Split`](SignalNode::Split) that fans audio to parallel lanes and sums
//! them back. Splits can be nested arbitrarily.
//!
//! ```text
//! Drive → (Delay ∥ Reverb) → Cabinet
//!
//! SignalChain { nodes: [
//!     Block(Drive),
//!     Split { lanes: [
//!         SignalChain { nodes: [Block(Delay)] },
//!         SignalChain { nodes: [Block(Reverb)] },
//!     ]},
//!     Block(Cabinet),
//! ]}
//! ```

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ModuleBlock;

// ─── SignalNode ─────────────────────────────────────────────────

/// A single node in the signal processing tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum SignalNode {
    /// A single processing block in the chain.
    Block(Box<ModuleBlock>),
    /// Audio is copied to N parallel lanes, each processed independently,
    /// then summed back together.
    Split { lanes: Vec<SignalChain> },
}

impl SignalNode {
    /// Returns the inner block if this is a `Block` node.
    pub fn as_block(&self) -> Option<&ModuleBlock> {
        match self {
            Self::Block(b) => Some(b),
            Self::Split { .. } => None,
        }
    }

    /// Returns the inner block mutably if this is a `Block` node.
    pub fn as_block_mut(&mut self) -> Option<&mut ModuleBlock> {
        match self {
            Self::Block(b) => Some(b),
            Self::Split { .. } => None,
        }
    }

    /// Returns the parallel lanes if this is a `Split` node.
    pub fn as_split(&self) -> Option<&[SignalChain]> {
        match self {
            Self::Block(_) => None,
            Self::Split { lanes } => Some(lanes),
        }
    }
}

// ─── SignalChain ────────────────────────────────────────────────

/// An ordered sequence of signal processing nodes.
///
/// This is the core topology type: audio flows left-to-right through the
/// nodes. A chain with only `Block` nodes is a simple series path. A chain
/// containing `Split` nodes has parallel routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SignalChain {
    nodes: Vec<SignalNode>,
}

impl SignalChain {
    /// Create a chain from an explicit list of nodes.
    pub fn new(nodes: Vec<SignalNode>) -> Self {
        Self { nodes }
    }

    /// Create a pure series chain (no parallel routing).
    ///
    /// This is the backward-compatible path: `Module::from_blocks()` delegates here.
    pub fn serial(blocks: Vec<ModuleBlock>) -> Self {
        Self {
            nodes: blocks
                .into_iter()
                .map(|b| SignalNode::Block(Box::new(b)))
                .collect(),
        }
    }

    /// The ordered list of nodes in this chain.
    pub fn nodes(&self) -> &[SignalNode] {
        &self.nodes
    }

    /// Mutable access to the ordered list of nodes.
    pub fn nodes_mut(&mut self) -> &mut Vec<SignalNode> {
        &mut self.nodes
    }

    /// Collect all blocks in depth-first order (flattening splits).
    ///
    /// This is the backward-compatible accessor — callers that just need a
    /// flat list of blocks (e.g., block count, parameter iteration) use this.
    pub fn blocks(&self) -> Vec<&ModuleBlock> {
        let mut out = Vec::new();
        collect_blocks(&self.nodes, &mut out);
        out
    }

    /// Collect all blocks mutably in depth-first order.
    pub fn blocks_mut(&mut self) -> Vec<&mut ModuleBlock> {
        let mut out = Vec::new();
        collect_blocks_mut(&mut self.nodes, &mut out);
        out
    }

    /// True if this chain has no `Split` nodes (pure series).
    pub fn is_serial(&self) -> bool {
        self.nodes.iter().all(|n| matches!(n, SignalNode::Block(_)))
    }

    /// Number of nodes at the top level (not recursive).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if the chain has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

fn collect_blocks<'a>(nodes: &'a [SignalNode], out: &mut Vec<&'a ModuleBlock>) {
    for node in nodes {
        match node {
            SignalNode::Block(b) => out.push(b),
            SignalNode::Split { lanes } => {
                for lane in lanes {
                    collect_blocks(&lane.nodes, out);
                }
            }
        }
    }
}

fn collect_blocks_mut<'a>(nodes: &'a mut [SignalNode], out: &mut Vec<&'a mut ModuleBlock>) {
    for node in nodes {
        match node {
            SignalNode::Block(b) => out.push(b),
            SignalNode::Split { lanes } => {
                for lane in lanes {
                    collect_blocks_mut(&mut lane.nodes, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockType, ModuleBlock, ModuleBlockSource, PresetId};

    fn test_block(id: &str, block_type: BlockType) -> ModuleBlock {
        ModuleBlock::new(
            id,
            id,
            block_type,
            ModuleBlockSource::PresetDefault {
                preset_id: PresetId::new(),
                saved_at_version: None,
            },
        )
    }

    #[test]
    fn serial_chain_from_blocks() {
        let chain = SignalChain::serial(vec![
            test_block("drive", BlockType::Drive),
            test_block("amp", BlockType::Amp),
            test_block("cab", BlockType::Cabinet),
        ]);

        assert_eq!(chain.len(), 3);
        assert!(chain.is_serial());
        assert_eq!(chain.blocks().len(), 3);
        assert_eq!(chain.blocks()[0].id(), "drive");
        assert_eq!(chain.blocks()[2].id(), "cab");
    }

    #[test]
    fn parallel_split() {
        let chain = SignalChain::new(vec![
            SignalNode::Block(Box::new(test_block("drive", BlockType::Drive))),
            SignalNode::Split {
                lanes: vec![
                    SignalChain::serial(vec![test_block("delay", BlockType::Delay)]),
                    SignalChain::serial(vec![test_block("reverb", BlockType::Reverb)]),
                ],
            },
            SignalNode::Block(Box::new(test_block("cab", BlockType::Cabinet))),
        ]);

        assert_eq!(chain.len(), 3);
        assert!(!chain.is_serial());

        // Flat block traversal: drive, delay, reverb, cab
        let blocks = chain.blocks();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].id(), "drive");
        assert_eq!(blocks[1].id(), "delay");
        assert_eq!(blocks[2].id(), "reverb");
        assert_eq!(blocks[3].id(), "cab");
    }

    #[test]
    fn nested_split() {
        // Drive → (A: Delay → (A1: Chorus ∥ A2: Flanger) ∥ B: Reverb) → Cab
        let chain = SignalChain::new(vec![
            SignalNode::Block(Box::new(test_block("drive", BlockType::Drive))),
            SignalNode::Split {
                lanes: vec![
                    // Lane A: Delay → nested split
                    SignalChain::new(vec![
                        SignalNode::Block(Box::new(test_block("delay", BlockType::Delay))),
                        SignalNode::Split {
                            lanes: vec![
                                SignalChain::serial(vec![test_block("chorus", BlockType::Chorus)]),
                                SignalChain::serial(vec![test_block(
                                    "flanger",
                                    BlockType::Flanger,
                                )]),
                            ],
                        },
                    ]),
                    // Lane B: Reverb
                    SignalChain::serial(vec![test_block("reverb", BlockType::Reverb)]),
                ],
            },
            SignalNode::Block(Box::new(test_block("cab", BlockType::Cabinet))),
        ]);

        // Flat: drive, delay, chorus, flanger, reverb, cab
        let blocks = chain.blocks();
        assert_eq!(blocks.len(), 6);
        assert_eq!(blocks[0].id(), "drive");
        assert_eq!(blocks[1].id(), "delay");
        assert_eq!(blocks[2].id(), "chorus");
        assert_eq!(blocks[3].id(), "flanger");
        assert_eq!(blocks[4].id(), "reverb");
        assert_eq!(blocks[5].id(), "cab");
    }

    #[test]
    fn empty_chain() {
        let chain = SignalChain::new(vec![]);
        assert!(chain.is_empty());
        assert!(chain.is_serial());
        assert_eq!(chain.blocks().len(), 0);
    }

    #[test]
    fn signal_node_accessors() {
        let block_node = SignalNode::Block(Box::new(test_block("drive", BlockType::Drive)));
        assert!(block_node.as_block().is_some());
        assert!(block_node.as_split().is_none());

        let split_node = SignalNode::Split {
            lanes: vec![SignalChain::serial(vec![test_block(
                "delay",
                BlockType::Delay,
            )])],
        };
        assert!(split_node.as_block().is_none());
        assert!(split_node.as_split().is_some());
        assert_eq!(split_node.as_split().unwrap().len(), 1);
    }

    #[test]
    fn blocks_mut_can_modify() {
        let mut chain = SignalChain::new(vec![
            SignalNode::Block(Box::new(test_block("drive", BlockType::Drive))),
            SignalNode::Split {
                lanes: vec![SignalChain::serial(vec![test_block(
                    "delay",
                    BlockType::Delay,
                )])],
            },
        ]);

        let blocks = chain.blocks_mut();
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn serde_round_trip_serial() {
        let chain = SignalChain::serial(vec![
            test_block("drive", BlockType::Drive),
            test_block("amp", BlockType::Amp),
        ]);

        let json = serde_json::to_string(&chain).unwrap();
        let deserialized: SignalChain = serde_json::from_str(&json).unwrap();
        assert_eq!(chain, deserialized);
    }

    #[test]
    fn serde_round_trip_parallel() {
        let chain = SignalChain::new(vec![
            SignalNode::Block(Box::new(test_block("drive", BlockType::Drive))),
            SignalNode::Split {
                lanes: vec![
                    SignalChain::serial(vec![test_block("delay", BlockType::Delay)]),
                    SignalChain::serial(vec![test_block("reverb", BlockType::Reverb)]),
                ],
            },
        ]);

        let json = serde_json::to_string(&chain).unwrap();
        let deserialized: SignalChain = serde_json::from_str(&json).unwrap();
        assert_eq!(chain, deserialized);
    }
}
