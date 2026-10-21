//! A drum kit from a host to a device, end to end over vox: the host finds
//! the kit (preset → engines → packs, and the library's mixes and space),
//! the device fetches it into the host's layout, byte for byte; a second
//! fetch finds it all there and moves nothing.

use futures_util::StreamExt as _;
use signal_content_client::{TransferEvent, download_bundle};
use signal_content_library::ContentLibraryBackend;
use signal_content_proto::content::ContentLibraryClient;

fn write(path: &std::path::Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A studio library: one kit, its proxy twin, a stray recording.
fn studio(root: &std::path::Path) -> Vec<u8> {
    let lib = root.join("Proxy/Drum Kits/MM2");
    let pack: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    write(&lib.join("Presets/Rock.signalpreset"), b"name \"Rock\"\nengines (\n  {\n    id \"kick\"\n    engine \"../Engines/Kick A.signalengine\"\n  }\n)\n");
    write(
        &lib.join("Engines/Kick A.signalengine"),
        b"block {\n    pack \"../Packs/Kick/Kick A.signalpack\"\n}\n",
    );
    write(&lib.join("Packs/Kick/Kick A.signalpack"), &pack);
    write(&lib.join("Mixes/Angr.preset"), b"mix");
    write(&lib.join("Space/pieces.space/space.json"), b"{}");
    write(&root.join("Sampled/Kick/hit.wav"), b"not offered");
    pack
}

async fn run(
    client: ContentLibraryClient,
    bundle: signal_content_proto::BundleInfo,
    dest: std::path::PathBuf,
) -> (Vec<TransferEvent>, TransferEvent) {
    let (mut rx, _cancel) = download_bundle(client, bundle, dest);
    let mut events = Vec::new();
    while let Some(ev) = rx.next().await {
        let end = matches!(
            ev,
            TransferEvent::Done(_) | TransferEvent::Failed(_) | TransferEvent::Paused { .. }
        );
        events.push(ev.clone());
        if end {
            return (events, ev);
        }
    }
    panic!("the transfer ended without saying how");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kit_comes_down_in_the_hosts_layout_once() {
    let host_dir = tempfile::tempdir().unwrap();
    let device_dir = tempfile::tempdir().unwrap();
    let pack = studio(host_dir.path());
    let host = ContentLibraryBackend::with_root(host_dir.path());
    host.hash_all();
    let server = architect::LocalServer::serve(host.router(), architect::Scope::new());
    let client: ContentLibraryClient = server.establish().await.expect("client");

    // The host offers the kit, and not the stray recording.
    let catalog = client.catalog(String::new()).await.expect("catalog");
    assert!(catalog.iter().all(|c| !c.id.starts_with("Sampled/")));
    let kits = client.bundles("drum-kit".into()).await.expect("bundles");
    assert_eq!(kits.len(), 1);
    let kit = kits[0].clone();
    assert_eq!(
        (
            kit.name.as_str(),
            kit.library.as_str(),
            kit.variant.as_str(),
            kit.files
        ),
        ("Rock", "Drum Kits/MM2", "proxy", 5)
    );

    // Down it comes, laid out as on the host.
    let (_, end) = run(client.clone(), kit.clone(), device_dir.path().to_path_buf()).await;
    assert_eq!(
        end,
        TransferEvent::Done(device_dir.path().join("Drum Kits").join("MM2"))
    );
    let lib = device_dir.path().join("Drum Kits/MM2");
    assert_eq!(
        std::fs::read(lib.join("Packs/Kick/Kick A.signalpack")).unwrap(),
        pack
    );
    assert!(
        lib.join("Presets/Rock.signalpreset").exists() && lib.join("Mixes/Angr.preset").exists()
    );
    assert!(lib.join("Space/pieces.space/space.json").exists());

    // Again: all here already — nothing fetched.
    let (events, end) = run(client, kit, device_dir.path().to_path_buf()).await;
    assert!(matches!(end, TransferEvent::Done(_)));
    let fetched = events
        .iter()
        .filter(|e| matches!(e, TransferEvent::Progress { .. }))
        .count();
    assert_eq!(
        fetched, 5,
        "one progress per file skipped, no chunk progress: {events:?}"
    );
}
