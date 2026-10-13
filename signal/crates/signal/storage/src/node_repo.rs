//! Node repository — the canonical composition model, in the database.
//!
//! The five old repositories (`block_repo`, `module_repo`, `layer_repo`,
//! `engine_repo`, `rig_repo`) each persist one level of a hierarchy that is
//! now one type. This is their replacement: one table, because there is one
//! kind of thing.
//!
//! # Where this is, and is not, used
//!
//! Not on stage. The live rig keeps its library as styx — hand-editable,
//! git-trackable, and unable to fail to open in a way a person cannot fix at
//! nine on a Sunday morning. This is for the desktop browser and editor,
//! where a library of thousands wants queries rather than a file read whole.
//! A [`NodeLibrary`] serializes to either, and neither is the source of truth
//! for the other.
//!
//! # Flat, like the library it mirrors
//!
//! A node referenced by six parents is one row. Nesting the tree in the
//! schema would store it six times, and "edit the preset" would mean "edit
//! six copies" — the same reason [`Content::Children`] holds ids.

use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use sea_orm::{ConnectionTrait, Schema};
use serde::{Deserialize, Serialize};
use signal_proto::model::EngineType;
use signal_proto::node::{Content, Node, NodeId, NodeLibrary, Role, Variant, VariantId};
use signal_proto::node_routing::{AudioSend, ModRoute, Setting, Zone};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

#[async_trait::async_trait]
pub trait NodeRepo: Send + Sync + 'static {
    /// Every node, as the library the domain resolves against.
    async fn load_library(&self) -> StorageResult<NodeLibrary>;
    /// One node by id.
    async fn load_node(&self, id: &NodeId) -> StorageResult<Option<Node>>;
    /// Insert or replace one node.
    async fn save_node(&self, node: Node) -> StorageResult<()>;
    /// Insert or replace every node in a library.
    async fn save_library(&self, library: &NodeLibrary) -> StorageResult<()>;
    /// Remove a node. Does **not** remove references to it — see
    /// [`NodeLibrary::users_of`]; a dangling child resolves to a hole with a
    /// reason, which is recoverable, where a cascade would silently delete
    /// someone else's rig.
    async fn delete_node(&self, id: &NodeId) -> StorageResult<()>;
}

#[derive(Clone)]
pub struct NodeRepoLive {
    db: DatabaseConnection,
}

impl NodeRepoLive {
    #[must_use]
    pub const fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Create the table if it is not there.
    ///
    /// # Errors
    ///
    /// Returns an error if the statement fails.
    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);
        let mut nodes = schema.create_table_from_entity(entity::node::Entity);
        nodes.if_not_exists();
        self.db.execute(backend.build(&nodes)).await?;
        Ok(())
    }
}

/// The node fields that are stored together as JSON.
///
/// Every field defaults, so a row written before this column existed — or by
/// an older build that did not know about one of them — loads as a node with
/// the neutral value rather than failing the whole library.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Attrs {
    #[serde(default)]
    engine_type: EngineType,
    #[serde(default)]
    input_db: f32,
    #[serde(default)]
    output_db: f32,
    #[serde(default)]
    modulators: Vec<NodeId>,
    #[serde(default)]
    sends: Vec<AudioSend>,
    #[serde(default)]
    mod_routes: Vec<ModRoute>,
    #[serde(default)]
    settings: Vec<Setting>,
    #[serde(default)]
    zone: Option<Zone>,
    #[serde(default)]
    bypassed: bool,
}

/// A node as its row.
fn to_row(node: &Node) -> StorageResult<entity::node::ActiveModel> {
    let content =
        serde_json::to_string(&node.content).map_err(|e| StorageError::Data(e.to_string()))?;
    let variants =
        serde_json::to_string(&node.variants).map_err(|e| StorageError::Data(e.to_string()))?;
    let attrs = serde_json::to_string(&Attrs {
        engine_type: node.engine_type,
        input_db: node.input_db,
        output_db: node.output_db,
        modulators: node.modulators.clone(),
        sends: node.sends.clone(),
        mod_routes: node.mod_routes.clone(),
        settings: node.settings.clone(),
        zone: Some(node.zone),
        bypassed: node.bypassed,
    })
    .map_err(|e| StorageError::Data(e.to_string()))?;
    Ok(entity::node::ActiveModel {
        id: Set(node.id.as_str().to_string()),
        name: Set(node.name.clone()),
        role: Set(role_str(node.role).to_string()),
        combine: Set(combine_str(node.combine).to_string()),
        content_json: Set(content),
        variants_json: Set(variants),
        default_variant: Set(node.default_variant.as_str().to_string()),
        attrs_json: Set(attrs),
    })
}

/// A row as its node.
fn from_row(row: &entity::node::Model) -> StorageResult<Node> {
    let content: Content =
        serde_json::from_str(&row.content_json).map_err(|e| StorageError::Data(e.to_string()))?;
    let variants: Vec<Variant> =
        serde_json::from_str(&row.variants_json).map_err(|e| StorageError::Data(e.to_string()))?;
    // An unreadable attrs blob costs the fader and the zone, not the node.
    // The blocks are in `content_json`; refusing to load the node at all
    // would silence a rig over a field that has a neutral value.
    let attrs: Attrs = serde_json::from_str(&row.attrs_json).unwrap_or_default();
    Ok(Node {
        id: row.node_id(),
        name: row.name.clone(),
        role: role_from(&row.role),
        combine: combine_from(&row.combine),
        content,
        variants,
        default_variant: VariantId::from(row.default_variant.clone()),
        engine_type: attrs.engine_type,
        input_db: attrs.input_db,
        output_db: attrs.output_db,
        modulators: attrs.modulators,
        sends: attrs.sends,
        mod_routes: attrs.mod_routes,
        settings: attrs.settings,
        zone: attrs.zone.unwrap_or_else(Zone::full),
        bypassed: attrs.bypassed,
    })
}

const fn role_str(role: Role) -> &'static str {
    match role {
        Role::Preset => "preset",
        Role::Engine => "engine",
        Role::Layer => "layer",
        Role::Module => "module",
    }
}

/// An unknown role reads as `Module` rather than failing the load.
///
/// A row written by a newer build must not make the whole library
/// unopenable — a node whose meaning is unrecognised is a grouping, which is
/// exactly what `Module` says.
fn role_from(s: &str) -> Role {
    match s {
        "preset" => Role::Preset,
        "engine" => Role::Engine,
        "layer" => Role::Layer,
        _ => Role::Module,
    }
}

const fn combine_str(combine: signal_proto::node::Combine) -> &'static str {
    match combine {
        signal_proto::node::Combine::Serial => "serial",
        signal_proto::node::Combine::Parallel => "parallel",
    }
}

/// Unknown reads as serial — the safe default. A chain that sums when it
/// should have chained is louder than one that chains when it should have
/// summed, and neither is right, but only one is a surprise on stage.
fn combine_from(s: &str) -> signal_proto::node::Combine {
    if s == "parallel" {
        signal_proto::node::Combine::Parallel
    } else {
        signal_proto::node::Combine::Serial
    }
}

#[async_trait::async_trait]
impl NodeRepo for NodeRepoLive {
    async fn load_library(&self) -> StorageResult<NodeLibrary> {
        let rows = entity::node::Entity::find()
            .order_by_asc(entity::node::Column::Id)
            .all(&self.db)
            .await?;
        let mut library = NodeLibrary::new();
        for row in &rows {
            library.insert(from_row(row)?);
        }
        Ok(library)
    }

    async fn load_node(&self, id: &NodeId) -> StorageResult<Option<Node>> {
        let row = entity::node::Entity::find()
            .filter(entity::node::Column::Id.eq(id.as_str()))
            .one(&self.db)
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    async fn save_node(&self, node: Node) -> StorageResult<()> {
        let row = to_row(&node)?;
        // Replace rather than insert: saving a node that exists is an edit,
        // which is the common case.
        match entity::node::Entity::find()
            .filter(entity::node::Column::Id.eq(node.id.as_str()))
            .one(&self.db)
            .await?
        {
            Some(_) => {
                row.update(&self.db).await?;
            }
            None => {
                row.insert(&self.db).await?;
            }
        }
        Ok(())
    }

    async fn save_library(&self, library: &NodeLibrary) -> StorageResult<()> {
        for node in &library.nodes {
            self.save_node(node.clone()).await?;
        }
        Ok(())
    }

    async fn delete_node(&self, id: &NodeId) -> StorageResult<()> {
        entity::node::Entity::delete_many()
            .filter(entity::node::Column::Id.eq(id.as_str()))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use signal_proto::block::BlockType;
    use signal_proto::model::Block;
    use signal_proto::node::Combine;
    use signal_proto::node_resolve::resolve;
    use signal_proto::overrides::{NodePath, NodePathSegment, Override};

    async fn repo() -> NodeRepoLive {
        let db = Database::connect("sqlite::memory:").await.expect("sqlite");
        let repo = NodeRepoLive::new(db);
        repo.init_schema().await.expect("schema");
        repo
    }

    /// A chain with one amp and a variant that bends it — enough to prove the
    /// halves that are JSON survive.
    fn a_rig() -> (NodeLibrary, NodeId, VariantId) {
        let mut lib = NodeLibrary::new();
        let amp = Node::leaf("AC30", BlockType::Amp, Block::from_parameters(Vec::new()));
        let amp_id = amp.id.clone();

        let mut lead = Variant::new("Lead");
        lead.overrides.push(Override::bypass(
            NodePath::new(vec![NodePathSegment::Block {
                id: amp_id.as_str().to_string(),
            }]),
            true,
        ));
        let lead_id = lead.id.clone();

        let chain = Node::container("Worship", Role::Preset, Combine::Serial)
            .with_child(&amp)
            .with_variant(lead);
        let chain_id = chain.id.clone();
        lib.insert(amp);
        lib.insert(chain);
        (lib, chain_id, lead_id)
    }

    #[tokio::test]
    async fn a_library_survives_the_database() {
        let repo = repo().await;
        let (lib, chain, lead) = a_rig();
        repo.save_library(&lib).await.expect("saves");

        let back = repo.load_library().await.expect("loads");
        assert_eq!(back.nodes.len(), lib.nodes.len());

        // The proof that matters is not field equality but that it still
        // plays: resolve the same variant and get the same tree.
        let (before, _) = resolve(&lib, &chain, Some(&lead)).expect("resolves");
        let (after, report) = resolve(&back, &chain, Some(&lead)).expect("resolves");
        assert!(
            report.is_clean(),
            "no dangling references after a round trip"
        );
        assert_eq!(before.leaves().len(), after.leaves().len());
        assert_eq!(
            after.leaves()[0].bypassed,
            before.leaves()[0].bypassed,
            "the variant's override survived"
        );
    }

    /// Ids are assigned by the domain, so saving twice is an edit, not a
    /// duplicate — the thing that would quietly double a library.
    #[tokio::test]
    async fn saving_a_node_twice_edits_it() {
        let repo = repo().await;
        let (lib, chain, _) = a_rig();
        repo.save_library(&lib).await.expect("saves");
        repo.save_library(&lib).await.expect("saves again");

        let back = repo.load_library().await.expect("loads");
        assert_eq!(back.nodes.len(), lib.nodes.len(), "not doubled");

        let mut renamed = back.get(&chain).expect("present").clone();
        renamed.name = "Worship 2026".to_string();
        repo.save_node(renamed).await.expect("saves the edit");

        let back = repo.load_library().await.expect("loads");
        assert_eq!(back.nodes.len(), lib.nodes.len());
        assert_eq!(
            back.get(&chain).map(|n| n.name.as_str()),
            Some("Worship 2026")
        );
    }

    /// Deleting leaves references dangling on purpose. A resolve reports the
    /// hole and plays the rest; a cascade would have deleted whatever else
    /// pointed at it.
    #[tokio::test]
    async fn deleting_a_node_leaves_a_recoverable_hole() {
        let repo = repo().await;
        let (lib, chain, _) = a_rig();
        repo.save_library(&lib).await.expect("saves");

        let amp = lib
            .nodes
            .iter()
            .find(|n| n.name == "AC30")
            .expect("amp")
            .id
            .clone();
        repo.delete_node(&amp).await.expect("deletes");

        let back = repo.load_library().await.expect("loads");
        let (resolved, report) = resolve(&back, &chain, None).expect("still resolves");
        assert_eq!(report.missing, vec![amp], "the hole is named");
        assert!(resolved.leaves().is_empty(), "and the rest still plays");
    }

    #[tokio::test]
    async fn one_node_loads_without_the_library() {
        let repo = repo().await;
        let (lib, chain, _) = a_rig();
        repo.save_library(&lib).await.expect("saves");

        let one = repo
            .load_node(&chain)
            .await
            .expect("loads")
            .expect("present");
        assert_eq!(one.name, "Worship");
        assert_eq!(one.role, Role::Preset);
        assert!(
            repo.load_node(&NodeId::new())
                .await
                .expect("query")
                .is_none()
        );
    }
}
