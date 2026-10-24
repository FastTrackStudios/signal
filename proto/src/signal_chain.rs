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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub const fn new(nodes: Vec<SignalNode>) -> Self {
        Self { nodes }
    }

    /// Create a pure series chain (no parallel routing).
    ///
    /// This is the backward-compatible path: `Module::from_blocks()` delegates here.
    #[must_use]
    pub fn serial(blocks: Vec<ModuleBlock>) -> Self {
        Self {
            nodes: blocks
                .into_iter()
                .map(|b| SignalNode::Block(Box::new(b)))
                .collect(),
        }
    }

    /// The ordered list of nodes in this chain.
    #[must_use]
    pub fn nodes(&self) -> &[SignalNode] {
        &self.nodes
    }

    /// Mutable access to the ordered list of nodes.
    pub const fn nodes_mut(&mut self) -> &mut Vec<SignalNode> {
        &mut self.nodes
    }

    /// Collect all blocks in depth-first order (flattening splits).
    ///
    /// This is the backward-compatible accessor — callers that just need a
    /// flat list of blocks (e.g., block count, parameter iteration) use this.
    #[must_use]
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
    #[must_use]
    pub fn is_serial(&self) -> bool {
        self.nodes.iter().all(|n| matches!(n, SignalNode::Block(_)))
    }

    /// Number of nodes at the top level (not recursive).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if the chain has no nodes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
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
