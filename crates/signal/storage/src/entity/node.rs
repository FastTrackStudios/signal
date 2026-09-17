//! One row per [`Node`](signal_proto::node::Node).
//!
//! The tree is stored flat, exactly as [`NodeLibrary`](signal_proto::node::NodeLibrary)
//! holds it: a node referenced by six parents is one row, so editing it
//! changes all six. Nesting the tree in the schema would store it six times
//! and make "edit the preset" mean "edit six copies".
//!
//! `content_json` and `variants_json` carry the halves that are shaped rather
//! than scalar — the children (or the leaf's block) and the variants. Both are
//! read and written whole: a node is resolved as a unit, never queried into.
//! The columns that *are* scalar — name, role — are real columns, because
//! those are what a browser filters and sorts on.

use sea_orm::entity::prelude::*;
use signal_proto::node::NodeId;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "nodes")]
pub struct Model {
    /// The node's `UUIDv7`, as minted by the domain. Not auto-incremented:
    /// identity is assigned where the node is created, so that a node keeps
    /// it across machines.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    /// `Preset` / `Engine` / `Layer` / `Module` — the label, stored as a
    /// column so a browser can group by it without parsing every node.
    pub role: String,
    /// `serial` / `parallel`.
    pub combine: String,
    /// The leaf's block, or the child id list.
    pub content_json: String,
    /// Every variant: its selections and overrides.
    pub variants_json: String,
    pub default_variant: String,
}

impl Model {
    #[must_use]
    pub fn node_id(&self) -> NodeId {
        NodeId::from(self.id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
