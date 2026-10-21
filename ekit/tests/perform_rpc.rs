//! The e-kit on the shared performance surface, under its own scope: a
//! first start saves the pads as `Kit 1` and seeds a profile from it; kits
//! are saved, copied, renamed, played and kept in the e-kit's own place.

use ekit::EkitBackend;
use signal_rigs_proto::Rig;
use signal_rigs_proto::perform::service::RigPerformClient;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_ekit_keeps_kits_of_its_own() {
    let root = std::env::temp_dir().join(format!("ekit-perform-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // SAFETY: the only test in this binary; set before any backend exists.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &root);
    }
    let ekit = EkitBackend::new();
    let router = signal_rigs_proto::mount_perform!(ekit.router(), Rig::Ekit, ekit.clone());
    let server = architect::LocalServer::serve(router, architect::Scope::new());
    let c: RigPerformClient = server.establish().await.expect("client");
    let c = architect::scope_client!(c, Rig::Ekit.slug());

    // A first start: the pads saved as Kit 1, a profile with its stack.
    let s = c.state().await.expect("state");
    assert_eq!(
        s.stacks.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
        ["Kit 1"]
    );

    // Saved, copied, renamed — and the copy plays.
    c.save_variation("Kit 1".into(), "Fill".into())
        .await
        .expect("save");
    c.duplicate_preset("Kit 1".into(), "Kit 2".into())
        .await
        .expect("copy");
    c.rename_preset("Kit 2".into(), "Trap".into())
        .await
        .expect("rename");
    let comp = c.compositions().await.expect("compositions");
    let kits: Vec<(&str, usize)> = comp
        .presets
        .iter()
        .map(|p| (p.name.as_str(), p.snapshots.len()))
        .collect();
    assert_eq!(kits, [("Kit 1", 2), ("Trap", 2)]);
    c.choose_preset("Trap".into(), "Fill".into())
        .await
        .expect("choose");
    let comp = c.compositions().await.expect("compositions");
    assert_eq!(
        (comp.active_preset.as_str(), comp.active_snapshot.as_str()),
        ("Trap", "Fill")
    );

    // The playing kit stays; another goes.
    c.delete_preset("Trap".into()).await.expect("delete");
    assert_eq!(
        c.compositions().await.expect("compositions").presets.len(),
        2
    );
    c.delete_preset("Kit 1".into()).await.expect("delete");
    assert_eq!(
        c.compositions().await.expect("compositions").presets.len(),
        1
    );

    // Kept in the e-kit's own folder.
    let dir = root.join("signal/ekit/perform");
    assert!(dir.join("kits.styx").exists() && dir.join("profiles.styx").exists());
}
