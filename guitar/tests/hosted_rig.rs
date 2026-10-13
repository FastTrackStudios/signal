//! A rig played by a host: the rig service built `hosted`, its audio
//! played a host block at a time (as a plugin on a DAW's track plays it),
//! while the service — what the switches and the browser drive — changes
//! what plays. No audio device is opened.

use guitar::instrument::Instrument;
use guitar::proto::rig::Rig;
use guitar::{GuitarRigBackend, RigHome};

const RATE: u32 = 48_000;
const BLOCK: usize = 480;

/// The host's output level over `blocks` blocks of a low E (41 Hz) at 0.3,
/// the last half of them.
fn level(host: &guitar::RigHost, from: usize, blocks: usize) -> f32 {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut sum = 0.0_f32;
    let mut n = 0_usize;
    for b in from..from + blocks {
        let input: Vec<f32> = (0..BLOCK)
            .map(|i| {
                let t = (b * BLOCK + i) as f32 / RATE as f32;
                0.3 * (std::f32::consts::TAU * 41.2 * t).sin()
            })
            .collect();
        assert!(
            host.process(&input, &input, &mut l, &mut r),
            "the host plays the rig"
        );
        assert!(l.iter().chain(&r).all(|s| s.is_finite()));
        if b >= from + blocks / 2 {
            sum += l.iter().map(|s| s * s).sum::<f32>();
            n += BLOCK;
        }
    }
    (sum / n.max(1) as f32).sqrt()
}

#[test]
fn a_hosted_rig_plays_its_hosts_blocks_and_its_service_switches_them() {
    let root = std::env::temp_dir().join(format!("hosted-rig-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the only test in this binary, before any rig starts.
    unsafe {
        std::env::remove_var("SIGNAL_RIG_DESIGN");
        std::env::remove_var("SIGNAL_RIG_DIR");
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
        std::env::set_var("SIGNAL_RELOAD_LOG", root.join("config-reload.log"));
        std::env::set_var("SIGNAL_DROP_LOG", root.join("dropouts.log"));
    }
    let (rig, host) =
        GuitarRigBackend::hosted(RigHome::new(Instrument::Bass, root.join("bass")), RATE);
    assert!(rig.is_hosted());
    assert!(!host.is_open(), "nothing to play before it opens");
    rig.open_blocking();
    assert!(host.is_open(), "opened for its host");

    assert!(
        Rig::select_profile(&rig, "Metal".into()).ok,
        "the bass's Metal profile"
    );
    let patches = Rig::patches(&rig);
    let index = |name: &str| {
        patches
            .iter()
            .position(|p| p.name == name)
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or_else(|| {
                panic!(
                    "{name} in {:?}",
                    patches.iter().map(|p| &p.name).collect::<Vec<_>>()
                )
            })
    };
    assert!(Rig::select_patch(&rig, index("Clean")).ok);
    let clean = level(&host, 0, 100);
    assert!(
        clean > 1e-3,
        "the clean patch plays the host's input: {clean}"
    );
    assert!(Rig::select_patch(&rig, index("Drive")).ok);
    let drive = level(&host, 100, 100);
    assert!(drive > 1e-3, "and the drive patch: {drive}");
    assert!(
        (drive / clean - 1.0).abs() > 0.05,
        "a different sound: {clean} vs {drive}"
    );
}
