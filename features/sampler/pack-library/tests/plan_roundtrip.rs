//! In-process vox roundtrip for the W7 additions: `pack_plan` +
//! `read_range` over a memory link — the exact wire path the browser
//! client uses, minus the WebSocket. Guards the Facet schema of the new
//! reply types (a writer/reader schema mismatch here is invisible to
//! `cargo check`).

use std::io::Write as _;
use std::time::Duration;

use signal_pack_library::PackLibraryBackend;
use signal_packs_proto::packs::PackLibraryClient;
use signal_packs_proto::PackRange;
use vox::memory_link_pair;

/// Build a tiny zoned `.signalpack` under `root/Proxy/` so the backend
/// lists it as the "proxy" variant.
fn build_library(root: &std::path::Path) {
    let samples = root.join("src-samples");
    std::fs::create_dir_all(&samples).expect("samples dir");
    let spec = samples.join("library.styx");
    let mut zones = String::from("name \"rt\"\nzones (\n");
    let files = ["n48.wav", "n60.wav", "n72.wav"];
    for (i, f) in files.iter().enumerate() {
        let key = 48 + 12 * i;
        // Small mono wavs.
        let spec_wav = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(samples.join(f), spec_wav).expect("wav");
        for n in 0..512 {
            w.write_sample(((n % 32) as i16) << 8).expect("sample");
        }
        w.finalize().expect("finalize");
        zones.push_str(&format!(
            "  {{\n    file \"{f}\"\n    key_min {key}\n    key_max {key}\n    root_key {key}\n  }}\n"
        ));
    }
    zones.push_str(")\n");
    let mut sf = std::fs::File::create(&spec).expect("spec file");
    sf.write_all(zones.as_bytes()).expect("spec write");

    let pack = root.join("Proxy").join("rt.signalpack");
    let paths: Vec<std::path::PathBuf> = files.iter().map(|f| samples.join(f)).collect();
    signal_sampler::engine::cache::create_signal_pack_with(
        &pack,
        signal_sampler::engine::cache::PackSpecSource::Path(&spec),
        &samples,
        paths.iter().map(|p| p.as_path()),
        signal_sampler::engine::cache::PackCodec::OggVorbis { quality: 0.4 },
    )
    .expect("pack build");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plan_and_ranges_roundtrip_the_wire() {
    let dir = tempfile::tempdir().expect("tempdir");
    build_library(dir.path());
    let backend = PackLibraryBackend::with_root(dir.path());
    let router = backend.router();

    let (a, b) = memory_link_pair(64);
    let server = tokio::task::spawn(async move {
        let lane_acceptor = vox::lane_acceptor_fn(move |_req, lane: vox::PendingLane| {
            lane.handle_with(router.clone());
            Ok(())
        });
        let _connection = vox::acceptor_on(b)
            .on_lane(lane_acceptor)
            .establish_connection()
            .await
            .expect("server handshake");
        std::future::pending::<()>().await;
    });

    let connection = vox::initiator_on(a)
        .establish_connection()
        .await
        .expect("client handshake");
    let client = connection
        .open_lane::<PackLibraryClient>()
        .await
        .expect("open PackLibrary lane");

    // The listing sees the proxy pack.
    let packs = tokio::time::timeout(Duration::from_secs(5), client.packs())
        .await
        .expect("packs() hung")
        .expect("packs rpc");
    let info = packs.iter().find(|p| p.name == "rt").expect("rt listed");
    assert_eq!(info.variant, "proxy");
    let total = info.size_bytes;

    // pack_plan crosses the wire intact — THE regression this test guards.
    let (ptx, mut prx) = vox::channel::<signal_packs_proto::PackChunk>();
    let plan_call = client.pack_plan("rt".into(), "proxy".into(), 0, ptx);
    let plan_drain = async {
        let mut bytes = Vec::new();
        while let Ok(Some(chunk)) = prx.recv().await {
            bytes.extend_from_slice(&chunk.get().bytes);
        }
        bytes
    };
    let (plan_result, json_bytes) = tokio::time::timeout(
        Duration::from_secs(5),
        futures_util::future::join(plan_call, plan_drain),
    )
    .await
    .expect("pack_plan hung");
    plan_result.expect("pack_plan rpc");
    let plan_json = String::from_utf8(json_bytes).expect("plan utf8");
    let plan: Vec<signal_packs_proto::PackSegment> =
        facet_json::from_str(&plan_json).expect("plan json parses");
    assert!(plan
        .iter()
        .any(|s| s.rank == 0 && s.start == 0 && s.len == 64));
    let covered: u64 = plan.iter().map(|s| s.len).sum();
    assert_eq!(covered, total, "segments tile the pack exactly once");

    // read_range returns exactly the requested span, offsets absolute.
    let seg = plan.iter().find(|s| s.rank > 0).expect("a sample segment");
    let (tx, mut rx) = vox::channel::<signal_packs_proto::PackChunk>();
    let call = client.read_range(
        "rt".into(),
        "proxy".into(),
        PackRange {
            start: seg.start,
            len: seg.len,
        }
        .to_string(),
        tx,
    );
    let drain = async {
        let mut got = Vec::new();
        let mut at = seg.start;
        while let Ok(Some(chunk)) = rx.recv().await {
            let chunk = chunk.get();
            assert_eq!(chunk.offset, at, "contiguous absolute offsets");
            got.extend_from_slice(&chunk.bytes);
            at += chunk.bytes.len() as u64;
        }
        got
    };
    let (call_result, got) = tokio::time::timeout(
        Duration::from_secs(5),
        futures_util::future::join(call, drain),
    )
    .await
    .expect("read_range hung");
    call_result.expect("read_range rpc");
    assert_eq!(got.len() as u64, seg.len);

    // And it matches the file bytes.
    let disk = std::fs::read(dir.path().join("Proxy").join("rt.signalpack")).expect("read pack");
    assert_eq!(
        &disk[seg.start as usize..(seg.start + seg.len) as usize],
        &got[..],
        "range bytes identical to the file"
    );

    // The VIRTUAL-NAME route through `read` — what the browser actually
    // drives (see the proto docs): plan + range over the proven signature.
    let (vtx, mut vrx) = vox::channel::<signal_packs_proto::PackChunk>();
    let vcall = client.read("plan:rt".into(), "proxy".into(), 0, vtx);
    let vdrain = async {
        let mut bytes = Vec::new();
        while let Ok(Some(chunk)) = vrx.recv().await {
            bytes.extend_from_slice(&chunk.get().bytes);
        }
        bytes
    };
    let (vres, vbytes) = tokio::time::timeout(
        Duration::from_secs(5),
        futures_util::future::join(vcall, vdrain),
    )
    .await
    .expect("virtual plan hung");
    vres.expect("virtual plan rpc");
    assert_eq!(
        vbytes,
        plan_json.as_bytes(),
        "virtual plan == dedicated plan"
    );

    let (rtx, mut rrx) = vox::channel::<signal_packs_proto::PackChunk>();
    let rcall = client.read("range:0+64:rt".into(), "proxy".into(), 0, rtx);
    let rdrain = async {
        let mut bytes = Vec::new();
        while let Ok(Some(chunk)) = rrx.recv().await {
            assert_eq!(chunk.get().offset, bytes.len() as u64, "absolute offsets");
            bytes.extend_from_slice(&chunk.get().bytes);
        }
        bytes
    };
    let (rres, rbytes) = tokio::time::timeout(
        Duration::from_secs(5),
        futures_util::future::join(rcall, rdrain),
    )
    .await
    .expect("virtual range hung");
    rres.expect("virtual range rpc");
    assert_eq!(rbytes, &disk[..64], "virtual range serves the header");

    server.abort();
}
