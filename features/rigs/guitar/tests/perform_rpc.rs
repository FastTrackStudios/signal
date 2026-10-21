//! The shared performance surface end to end: the guitar rig mounted under
//! its scope answers a scoped `RigPerform` client with its own state, and a
//! call through it plays on the rig.

use architect::rig::RigBackend as _;
use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::Rig;
use signal_rigs_proto::perform::service::RigPerformClient;

/// The tests set the process environment the library reads: one at a time.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_guitar_answers_the_shared_surface_under_its_scope() {
    let _env = ENV
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = std::env::temp_dir().join(format!("perform-rpc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the tests hold `ENV` while they touch the environment.
    unsafe {
        std::env::set_var("SIGNAL_RIG_DIR", root.join("rig"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
    }
    let rig = GuitarRigBackend::new();
    rig.open_blocking();

    let router = signal_rigs_proto::mount_perform!(
        rig.router(),
        signal_rigs_proto::Rig::Guitar,
        rig.clone()
    );
    let server = architect::LocalServer::serve(router, architect::Scope::new());
    let client: RigPerformClient = server.establish().await.expect("perform client");
    let client = architect::scope_client!(client, "guitar");

    // Its state is the guitar's.
    let state = client.state().await.expect("state");
    let perf = Rig::perf(&rig);
    assert_eq!(state.profile_name, perf.profile_name);
    assert_eq!(state.stacks.len(), perf.stacks.len());
    assert!(!state.stacks.is_empty(), "the shipped profile has stacks");

    // A call through it plays on the rig.
    let to = (perf.perform_mode + 1) % 3;
    client.set_perform_mode(to).await.expect("set mode");
    assert_eq!(Rig::perf(&rig).perform_mode, to);
    assert_eq!(client.state().await.expect("state").mode, to);
}
