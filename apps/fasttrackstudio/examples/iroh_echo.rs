//! Minimal cross-machine iroh transport check — raw framed echo over
//! `architect::iroh_link`, no vox on top.
//!
//! ```bash
//! # host A (server; prints its endpoint id)
//! cargo run -p fasttrackstudio --example iroh_echo --no-default-features \
//!     --features signal-guitar -- serve
//! # host B
//! cargo run -p fasttrackstudio --example iroh_echo --no-default-features \
//!     --features signal-guitar -- dial <endpoint-id>
//! ```

use architect::iroh_link::{self, iroh, IrohLink};
use vox::{Link as _, LinkRx as _, LinkTx as _};

/// Minimal real vox service over iroh (`serve-vox` mode) — a stub
/// PackLibrary with one fake pack, no engine host around it.
#[derive(Clone, architect::HasDispatcher)]
struct StubPacks;

impl signal_packs_proto::packs::PackLibrary for StubPacks {
    fn packs(&self) -> Vec<signal_packs_proto::PackInfo> {
        vec![signal_packs_proto::PackInfo {
            name: "stub".into(),
            category: "Test".into(),
            variant: "proxy".into(),
            size_bytes: 4,
            sha256: String::new(),
        }]
    }

    async fn read(
        &self,
        _name: String,
        _variant: String,
        start: u64,
        tx: vox::Tx<signal_packs_proto::PackChunk>,
    ) -> Result<(), signal_packs_proto::PackError> {
        let _ = tx
            .send(signal_packs_proto::PackChunk { offset: start, bytes: vec![1, 2, 3, 4] })
            .await;
        Ok(())
    }
}

impl architect::Services for StubPacks {
    fn layers() -> impl architect::Layer<Self> {
        architect::layers![signal_packs_proto::packs::Service]
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "serve".into());

    match mode.as_str() {
        "serve" => {
            let ep = iroh_link::bind_endpoint(iroh::SecretKey::generate())
                .await
                .map_err(|e| eyre::eyre!("bind: {e:?}"))?;
            println!("iroh_echo serving — endpoint id: {}", ep.id());
            while let Some(incoming) = ep.accept().await {
                tokio::spawn(async move {
                    let Ok(connection) = incoming.await else { return };
                    println!("connection from {:?}", connection.remote_id());
                    loop {
                        match connection.accept_bi().await {
                            Ok((send, recv)) => {
                                let link = IrohLink::new(connection.clone(), send, recv);
                                tokio::spawn(async move {
                                    let (tx, mut rx) = link.split();
                                    let mut n = 0u64;
                                    while let Ok(Some(frame)) = rx.recv().await {
                                        n += 1;
                                        if tx.send(frame.as_bytes().to_vec()).await.is_err() {
                                            break;
                                        }
                                    }
                                    println!("stream done after {n} frames");
                                });
                            }
                            Err(e) => {
                                println!("connection closed: {e}");
                                return;
                            }
                        }
                    }
                });
            }
        }
        "serve-vox" => {
            use architect::Services as _;
            let ep = iroh_link::bind_endpoint(iroh::SecretKey::generate())
                .await
                .map_err(|e| eyre::eyre!("bind: {e:?}"))?;
            println!("iroh_echo serving VOX (StubPacks) — endpoint id: {}", ep.id());
            iroh_link::serve_router(&ep, StubPacks.into_router()).await;
        }
        "dial" => {
            let id: iroh::EndpointId = args
                .next()
                .ok_or_else(|| eyre::eyre!("usage: iroh_echo dial <endpoint-id>"))?
                .trim()
                .parse()
                .map_err(|e| eyre::eyre!("bad id: {e:?}"))?;
            let ep = iroh_link::bind_endpoint(iroh::SecretKey::generate())
                .await
                .map_err(|e| eyre::eyre!("bind: {e:?}"))?;
            let link = iroh_link::connect(&ep, id)
                .await
                .map_err(|e| eyre::eyre!("connect: {e:?}"))?;
            let (tx, mut rx) = link.split();
            for (i, size) in [5usize, 100_000, 1_000_000, 0].into_iter().enumerate() {
                let frame = vec![0xA5u8; size];
                tx.send(frame.clone()).await.map_err(|e| eyre::eyre!("send {i}: {e:?}"))?;
                let echoed = rx
                    .recv()
                    .await
                    .map_err(|e| eyre::eyre!("recv {i}: {e:?}"))?
                    .ok_or_else(|| eyre::eyre!("stream closed at frame {i}"))?;
                eyre::ensure!(echoed.as_bytes() == frame.as_slice(), "frame {i} mismatch");
                println!("frame {i} ({size} B) echoed ✓");
            }
            tx.close().await.ok();
            println!("iroh transport OK cross-machine");
        }
        other => eyre::bail!("unknown mode {other}"),
    }
    Ok(())
}
