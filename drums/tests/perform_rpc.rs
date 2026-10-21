//! The drums on the shared performance surface, under their own scope: two
//! rigs mounted on one router each answer with their own state, and the
//! drums keep songs and sets of their own.

use architect::rig::RigBackend as _;
use drums::DrumRigBackend;
use signal_rigs_proto::Rig;
use signal_rigs_proto::perform::service::RigPerformClient;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_rig_answers_its_own_scope() {
    // An empty library: no kits, nothing to load — the wiring is the point.
    let root = std::env::temp_dir().join(format!("drums-perform-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the only test in this binary; set before any backend exists.
    unsafe {
        std::env::set_var("SIGNAL_DRUM_LIBRARY", root.join("library"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }
    let drums = DrumRigBackend::new();
    let ekit = DrumRigBackend::new();
    let router = signal_rigs_proto::mount_perform!(drums.router(), Rig::Drums, drums.clone());
    let router = signal_rigs_proto::mount_perform!(router, Rig::Ekit, ekit.clone());
    let server = architect::LocalServer::serve(router, architect::Scope::new());
    let a: RigPerformClient = server.establish().await.expect("client");
    let b: RigPerformClient = server.establish().await.expect("client");
    let a = architect::scope_client!(a, Rig::Drums.slug());
    let b = architect::scope_client!(b, Rig::Ekit.slug());

    a.set_perform_mode(1).await.expect("mode");
    b.set_perform_mode(2).await.expect("mode");
    let (sa, sb) = (
        a.state().await.expect("state"),
        b.state().await.expect("state"),
    );
    assert_eq!((sa.mode, sb.mode), (1, 2), "each scope reaches its own rig");
    assert_eq!(sa.profile_name, "Default");

    // Songs and sets are the drums' own, kept in their own place.
    a.add_song("Holy".into(), "G".into(), 72)
        .await
        .expect("add_song");
    a.add_setlist_entry(0, "Holy".into())
        .await
        .expect("add entry");
    let s = a.state().await.expect("state");
    assert_eq!(
        s.songs.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
        ["Holy"]
    );
    assert!(root.join("config/signal/drums/perform/songs.styx").exists());
    assert!(!a.select_profile("Nope".into()).await.expect("profile").ok);
}
