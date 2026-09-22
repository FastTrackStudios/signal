#![cfg(not(target_arch = "wasm32"))]
//! Dragging an EQ node sends a parameter write per pointer event per field.
//! Fired unbounded — one spawned RPC each — a drag saturates vox's 64
//! in-flight requests, and the server closes the connection ("max_concurrent_
//! requests exceeded"): the UI loses the rig. This drives the real backend
//! over a real (in-process) vox link the way a hard drag does.

use architect::rig::RigBackend as _;
use architect::{LocalServer, Scope};
use signal_guitar::GuitarRigBackend;
use signal_guitar_proto::rig::RigClient;
use signal_proto::block::BlockType;

async fn rig() -> (RigClient, &'static LocalServer) {
    // SAFETY: set before the backend (or any thread of it) exists.
    unsafe { std::env::set_var("SIGNAL_RIG_DESIGN", "1") };
    let backend = GuitarRigBackend::new();
    backend.open_blocking();
    let server: &'static LocalServer =
        Box::leak(Box::new(LocalServer::serve(backend.router(), Scope::new())));
    (server.establish().await.expect("rig client"), server)
}

async fn eq_block(rig: &RigClient) -> String {
    let chain = rig.chain().await.expect("chain");
    chain
        .iter()
        .find(|b| b.block_type == BlockType::Eq)
        .map(|b| b.id.clone())
        .expect("the chain has an EQ")
}

/// A drag the way the surface used to send it: every event, every field,
/// all at once. Kept as the reproduction of the failure.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "reproduces the vox saturation the ParamWriter exists to prevent"]
async fn an_unbounded_drag_flood_breaks_the_link() {
    let (rig, _server) = rig().await;
    let id = eq_block(&rig).await;
    let mut calls = Vec::new();
    for i in 0..2000 {
        let (rig, id) = (rig.clone(), id.clone());
        calls.push(tokio::spawn(async move {
            rig.set_block_param(id, format!("band{}_freq", i % 4), 100.0 + i as f32)
                .await
        }));
    }
    let failed = futures::future::join_all(calls)
        .await
        .into_iter()
        .filter(|r| !matches!(r, Ok(Ok(()))))
        .count();
    assert_eq!(failed, 0, "{failed} of 2000 writes failed");
    assert!(rig.chain().await.is_ok(), "the link survived");
}

/// The same drag, through the writer every surface now uses: the link
/// survives, and every band lands where the drag ended.
#[tokio::test(flavor = "current_thread")]
async fn a_drag_through_the_param_writer_keeps_the_link() {
    use signal_guitar_ui::param_writer::{Edit, ParamWriter};

    let (rig, _server) = rig().await;
    let id = eq_block(&rig).await;
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let writer = {
                let rig = rig.clone();
                ParamWriter::new(
                    move |(block, param, value): Edit| {
                        let rig = rig.clone();
                        Box::pin(async move {
                            rig.set_block_param(block, param, value)
                                .await
                                .expect("write answered");
                        })
                    },
                    |task| {
                        tokio::task::spawn_local(task);
                    },
                )
            };
            // Four bands dragged together, 3000 pointer events, frequency
            // and gain each — as fast as events can be made.
            for step in 0..3000u32 {
                let band = step % 4 + 1;
                writer.set(id.clone(), format!("b{band}_freq"), 100.0 + step as f32);
                writer.set(
                    id.clone(),
                    format!("b{band}_gain"),
                    (step % 24) as f32 - 12.0,
                );
                if step % 50 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            while writer.outstanding() > 0 {
                tokio::task::yield_now().await;
            }
        })
        .await;

    let chain = rig.chain().await.expect("the link survived the drag");
    let eq = chain.iter().find(|b| b.id == id).expect("the EQ");
    for band in 1..=4u32 {
        let last = (0..3000u32).rev().find(|s| s % 4 + 1 == band).unwrap();
        let freq = eq
            .params
            .iter()
            .find(|p| p.name == format!("b{band}_freq"))
            .map(|p| p.value);
        assert_eq!(freq, Some(100.0 + last as f32), "band {band} frequency");
    }
}
