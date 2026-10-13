//! A rig played by a host: the device-free rig, its audio driven one host
//! block at a time ([`GuitarRig::hosted_audio`]) — what a plugin holding a
//! signal rig on a DAW's track does — while the control side switches what
//! plays.
//!
//! What it pins:
//! - the host's input is what the chain plays, at the host's block size;
//! - a switch from the control side is heard in the host's next blocks;
//! - the rig's input meter sees the host's input;
//! - once hosted, the control side no longer renders the rig itself.

use sampler::{GuitarRig, RigBlock};
use signal_proto::block::BlockType;

const SR: u32 = 48_000;
/// Not a power of two, and not the rig's own block: a host's.
const HOST_BLOCK: usize = 480;

fn gain(db: f32) -> RigBlock {
    RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", db.to_string())
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// `blocks` host blocks of a 110 Hz sine at 0.25 through `audio`, the last
/// half's output (left).
fn play(audio: &sampler::HostedAudio, from: usize, blocks: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![0.0; HOST_BLOCK], vec![0.0; HOST_BLOCK]);
    let (mut input, mut heard) = (Vec::new(), Vec::new());
    for b in from..from + blocks {
        let sine: Vec<f32> = (0..HOST_BLOCK)
            .map(|i| {
                let t = (b * HOST_BLOCK + i) as f32 / SR as f32;
                0.25 * (std::f32::consts::TAU * 110.0 * t).sin()
            })
            .collect();
        assert!(audio.process(&sine, &sine, &mut l, &mut r), "rendered");
        if b >= from + blocks / 2 {
            input.extend_from_slice(&sine);
            heard.extend_from_slice(&l);
        }
    }
    (input, heard)
}

#[test]
fn a_hosted_rig_plays_the_hosts_input_and_its_switches() {
    let mut rig = GuitarRig::open_offline(SR).expect("offline rig");
    let audio = rig.hosted_audio().expect("a device-free rig is hostable");
    let quiet = rig.install_chain(&[gain(-12.0)]).expect("chain builds");
    let unity = rig.install_chain(&[gain(0.0)]).expect("chain builds");

    rig.set_active(Some(quiet));
    let (input, heard) = play(&audio, 0, 40);
    let ratio = rms(&heard) / rms(&input);
    assert!(
        (ratio - 0.251).abs() < 0.02,
        "the -12 dB chain plays the host's input: {ratio}"
    );
    assert!(
        rig.input_peak() > 0.2,
        "the input meter hears the host: {}",
        rig.input_peak()
    );

    // Switched from the control side: heard in the host's next blocks.
    rig.set_active(Some(unity));
    let (input, heard) = play(&audio, 40, 40);
    let ratio = rms(&heard) / rms(&input);
    assert!(
        (ratio - 1.0).abs() < 0.05,
        "the unity chain after the switch: {ratio}"
    );

    // Hosted, the control side does not render it: a capture armed and an
    // offline render asked for leave it unfilled until the host plays.
    rig.arm_heard_capture(1_000);
    rig.render_offline(1_000);
    assert_eq!(
        rig.output_capture_remaining(),
        1_000,
        "the host renders, not the control side"
    );
    let _ = play(&audio, 80, 4);
    assert_eq!(
        rig.output_capture_remaining(),
        0,
        "the host's blocks filled it"
    );
}
