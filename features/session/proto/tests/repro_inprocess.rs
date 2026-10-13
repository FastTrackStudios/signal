//! In-process vox repro: drive a `#[vox::service]` over a memory link
//! (no REAPER, no architect bridge) and compare a unit-returning method
//! against a complex-returning one. Mirrors vox's own
//! `service_macro_shared` end-to-end harness (rev 27eef573).

use std::time::Duration;

use session_proto::setlist::Setlist;
use vox::memory_link_pair;

#[vox::service]
trait Probe {
    /// Control: unit return (this shape works against the live extension).
    async fn ping(&self) -> u32;
    /// Suspect: complex return (this shape hangs against the live extension).
    async fn fetch(&self) -> Setlist;
    /// Isolation probe: large Vec<u8> payload — does the JIT codec survive a
    /// big byte vector (like the 43KB schema CborPayload that's getting lost)?
    async fn blob(&self, n: u32) -> Vec<u8>;
}

#[derive(Clone)]
struct ProbeBackend;

impl Probe for ProbeBackend {
    async fn ping(&self) -> u32 {
        42
    }
    async fn blob(&self, n: u32) -> Vec<u8> {
        (0..n).map(|i| u8::try_from(i % 251).unwrap_or(0)).collect()
    }
    async fn fetch(&self) -> Setlist {
        use session_proto::song::{Chart, Section, Song};
        use session_proto::{SectionId, SongId};
        let section = Section {
            section_id: SectionId::default(),
            id: Some(1),
            name: "Verse".into(),
            comment: None,
            section_type: keyflow::sections::SectionType::Verse,
            start_seconds: 0.0,
            end_seconds: 16.0,
            number: Some(1),
            color: Some(0x0033_6699),
        };
        let song = Song {
            id: SongId::default(),
            name: "Repro Song".into(),
            project_guid: "guid-1".into(),
            start_seconds: 0.0,
            end_seconds: 120.0,
            count_in_seconds: Some(4.0),
            sections: vec![section],
            comments: vec![],
            tempo: Some(120.0),
            time_signature: None,
            measure_positions: vec![],
            chart_text: Some("|: C G :|".into()),
            parsed_chart: Some(Chart::new()),
            detected_chords: vec![],
            chart_fingerprint: Some("fp".into()),
            advance_mode: None,
            color: Some(0x0011_2233),
        };
        Setlist {
            id: Some("repro".into()),
            name: "Repro Setlist".into(),
            songs: vec![song],
            ..Default::default()
        }
    }
}

async fn with_client<F, Fut>(body: F) -> Result<(), String>
where
    F: FnOnce(ProbeClient) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (a, b) = memory_link_pair(64);

    // vox 0.10 lane model: the acceptor hands every inbound lane to the
    // Probe dispatcher; the initiator opens the Probe lane, which yields
    // the generated client directly (it implements `FromVoxLane`).
    let server = tokio::task::spawn(async move {
        let lane_acceptor = vox::lane_acceptor_fn(move |_req, lane: vox::PendingLane| {
            lane.handle_with(ProbeDispatcher::new(ProbeBackend));
            Ok(())
        });
        let Ok(_connection) = vox::acceptor_on(b)
            .on_lane(lane_acceptor)
            .establish_connection()
            .await
        else {
            return;
        };
        std::future::pending::<()>().await;
    });

    let connection = vox::initiator_on(a)
        .establish_connection()
        .await
        .map_err(|e| format!("client handshake: {e:?}"))?;
    let client = connection
        .open_lane::<ProbeClient>()
        .await
        .map_err(|e| format!("open Probe lane: {e:?}"))?;
    body(client).await;
    server.abort();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unit_return_roundtrips() {
    with_client(|client| async move {
        match tokio::time::timeout(Duration::from_secs(5), client.ping()).await {
            Ok(v) => assert_eq!(v.expect("ping rpc"), 42),
            Err(elapsed) => panic!("ping (unit return) HUNG: {elapsed}"),
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_blob_roundtrips() {
    with_client(|client| async move {
        match tokio::time::timeout(Duration::from_secs(5), client.blob(50_000)).await {
            Ok(v) => {
                let b = v.expect("blob rpc");
                assert_eq!(b.len(), 50_000, "blob truncated: got {}", b.len());
                assert_eq!(
                    b[49_999],
                    u8::try_from(49_999u32 % 251).unwrap_or(0),
                    "blob corrupted"
                );
            }
            Err(elapsed) => panic!("blob HUNG: {elapsed}"),
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn complex_return_roundtrips() {
    with_client(|client| async move {
        match tokio::time::timeout(Duration::from_secs(5), client.fetch()).await {
            Ok(v) => {
                let s = v.expect("fetch rpc");
                assert_eq!(s.name, "Repro Setlist");
            }
            Err(elapsed) => {
                panic!("fetch (complex return) HUNG — reproduced the live bug: {elapsed}")
            }
        }
    })
    .await
    .unwrap();
}
