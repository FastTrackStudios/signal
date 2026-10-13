//! Wire-compat regression net for the batch surface.
//!
//! Every op payload type must survive vox schema exchange (phon can't
//! lower tuples/foreign std types — see `#[ops(skip)]` on the
//! tempo-map tuple methods), and BatchExecution must stay mounted on
//! Standalone's bundle: when it wasn't, the UnknownMethod error reply
//! (`Result<(), VoxError>`) surfaced as an opaque schema-kind
//! mismatch at the client.

#![cfg(all(feature = "cli", feature = "vox"))]

use architect::{Layer, Scope, Services};
use daw::service as proto;

mod probe {
    use facet::Facet;

    use super::proto;

    /// Nested `Result` inside a payload — phon handles it; keep it
    /// covered since batch outputs embed `DawResult` fields.
    #[derive(Clone, Debug, Facet)]
    pub struct Wrapped {
        pub inner: Result<u32, String>,
    }

    #[architect::rpc]
    pub trait Probe {
        fn wrapped(&self) -> Wrapped;
        fn outcome(&self) -> proto::batch::StepOutcome;
        fn response(&self) -> proto::batch::BatchResponse;
        fn op_roundtrip(&self, op: proto::batch::BatchOp) -> proto::batch::BatchOp;
        fn markers_out(&self) -> proto::marker::MarkersOpOutput;
        fn tracks_out(&self) -> proto::track::TracksOpOutput;
        fn transport_out(&self) -> proto::transport::TransportOpOutput;
        fn projects_out(&self) -> proto::project::ProjectsOpOutput;
        fn fx_out(&self) -> proto::fx::EffectsOpOutput;
        fn routing_out(&self) -> proto::routing::RoutingOpOutput;
        fn region_out(&self) -> proto::region::RegionsOpOutput;
        fn tempo_out(&self) -> proto::tempo_map::TempoMapOpOutput;
        fn ext_out(&self) -> proto::ext_state::ExtStateOpOutput;
        fn item_out(&self) -> proto::item::ItemsOpOutput;
        fn take_out(&self) -> proto::take::TakesOpOutput;
    }

    #[derive(Clone, architect::HasDispatcher)]
    pub struct Backend;

    impl Probe for Backend {
        fn wrapped(&self) -> Wrapped {
            Wrapped { inner: Ok(9) }
        }
        fn outcome(&self) -> proto::batch::StepOutcome {
            proto::batch::StepOutcome::Skipped(1)
        }
        fn response(&self) -> proto::batch::BatchResponse {
            proto::batch::BatchResponse { results: vec![] }
        }
        fn op_roundtrip(&self, op: proto::batch::BatchOp) -> proto::batch::BatchOp {
            op
        }
        fn markers_out(&self) -> proto::marker::MarkersOpOutput {
            proto::marker::MarkersOpOutput::Count(0)
        }
        fn tracks_out(&self) -> proto::track::TracksOpOutput {
            proto::track::TracksOpOutput::Count(0)
        }
        fn transport_out(&self) -> proto::transport::TransportOpOutput {
            proto::transport::TransportOpOutput::GetPosition(0.0)
        }
        fn projects_out(&self) -> proto::project::ProjectsOpOutput {
            proto::project::ProjectsOpOutput::List(vec![])
        }
        fn fx_out(&self) -> proto::fx::EffectsOpOutput {
            proto::fx::EffectsOpOutput::Count(0)
        }
        fn routing_out(&self) -> proto::routing::RoutingOpOutput {
            proto::routing::RoutingOpOutput::Sends(vec![])
        }
        fn region_out(&self) -> proto::region::RegionsOpOutput {
            proto::region::RegionsOpOutput::Count(0)
        }
        fn tempo_out(&self) -> proto::tempo_map::TempoMapOpOutput {
            proto::tempo_map::TempoMapOpOutput::TempoPointCount(0)
        }
        fn ext_out(&self) -> proto::ext_state::ExtStateOpOutput {
            proto::ext_state::ExtStateOpOutput::Has(false)
        }
        fn item_out(&self) -> proto::item::ItemsOpOutput {
            proto::item::ItemsOpOutput::ItemCount(0)
        }
        fn take_out(&self) -> proto::take::TakesOpOutput {
            proto::take::TakesOpOutput::TakeCount(0)
        }
    }
}

#[tokio::test]
async fn every_op_payload_type_crosses_the_wire() -> eyre::Result<()> {
    let scope = Scope::new();
    let router = architect::layers![probe::Service].provide(probe::Backend);
    let server = architect::LocalServer::serve(router, scope.clone());
    let caller = server.caller().await?;
    let client = probe::ProbeClient::new(caller);

    macro_rules! check {
        ($name:literal, $fut:expr) => {
            assert!(
                $fut.await.is_ok(),
                concat!($name, " failed schema exchange")
            );
        };
    }

    check!("wrapped", client.wrapped());
    check!("outcome", client.outcome());
    check!("response", client.response());
    check!(
        "op_roundtrip",
        client.op_roundtrip(proto::batch::BatchOp::Marker(
            proto::marker::MarkersOp::Count {
                project: proto::batch::ProjectArg::Literal(proto::ProjectContext::Current),
            }
        ))
    );
    check!("markers_out", client.markers_out());
    check!("tracks_out", client.tracks_out());
    check!("transport_out", client.transport_out());
    check!("projects_out", client.projects_out());
    check!("fx_out", client.fx_out());
    check!("routing_out", client.routing_out());
    check!("region_out", client.region_out());
    check!("tempo_out", client.tempo_out());
    check!("ext_out", client.ext_out());
    check!("item_out", client.item_out());
    check!("take_out", client.take_out());
    Ok(())
}

/// BatchExecution stays mounted on Standalone's Services bundle.
#[tokio::test]
async fn batch_execution_is_mounted_on_standalone_bundle() -> eyre::Result<()> {
    let standalone = daw_standalone::sync::Standalone::new();
    let scope = Scope::new();
    let server = architect::LocalServer::serve(standalone.into_router(), scope.clone());
    let caller = server.caller().await?;
    let client = proto::batch::BatchExecutionClient::new(caller);

    let req = proto::batch::BatchRequest {
        instructions: vec![],
        options: proto::batch::BatchOptions::default(),
    };
    let response = client.execute(req).await?;
    assert!(response.results.is_empty());
    Ok(())
}
