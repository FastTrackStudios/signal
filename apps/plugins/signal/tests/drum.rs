//! The `drum` block with a real library piece: `SIGNAL_TEST_DRUMS=<a
//! .signalengine>` (a Modern and Massive 2 kick, say; libraries are never
//! committed).

use signal_core::block::{Backend, BlockFactory, BuildContext};
use signal_core::host::{AssetRef, NoHost};
use signal_core::state::BlockSpec;
use signal_plugin::drum::DrumFactory;

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;

#[test]
#[ignore = "needs SIGNAL_TEST_DRUMS, a .signalengine"]
fn a_library_piece_plays_on_its_hit() {
    let engine =
        std::env::var("SIGNAL_TEST_DRUMS").expect("SIGNAL_TEST_DRUMS names a .signalengine");
    let spec = BlockSpec::signal("kick", "drum").asset(AssetRef::path(engine));
    let cx = BuildContext {
        sample_rate: SR,
        max_block: BLOCK,
        host: &NoHost,
    };
    let built = DrumFactory.build(&spec, &cx).expect("the piece loads");
    let Backend::Linked(mut drum) = built.backend else {
        panic!("a compiled-in block");
    };
    // The block loaded the whole piece when it was built: it plays at once.

    // A hit 100 frames into the 4th piece, at full velocity, then one soft
    // one a second later.
    let pieces = 48_000 * 2 / BLOCK;
    let mut out = Vec::with_capacity(pieces * BLOCK);
    for p in 0..pieces {
        if p == 3 {
            drum.hit(100, 1.0);
        }
        if p == 3 + 48_000 / BLOCK {
            drum.hit(0, 0.3);
        }
        let (mut l, mut r) = (vec![0.0_f32; BLOCK], vec![0.0_f32; BLOCK]);
        drum.process(&mut l, &mut r);
        out.extend(l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)));
    }
    let onset = out
        .iter()
        .position(|s| s.abs() > 1e-4)
        .expect("the piece sounds");
    let hit = 3 * BLOCK + 100;
    // On the hit, give or take the recording's own lead-in (an MM2 kick's
    // first 3 frames are below -80 dB, a snare's 5).
    assert!(
        (hit..hit + 8).contains(&onset),
        "on the hit ({hit}): {onset}"
    );
    let peak = |a: usize, b: usize| out[a..b].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    let loud = peak(hit, hit + 24_000);
    let soft_at = (3 + 48_000 / BLOCK) * BLOCK;
    let soft = peak(soft_at, soft_at + 24_000);
    // Normalized: the hardest hit at 0 dBFS, `level` -6 dB under (another
    // round robin than the one measured, so near, not exact).
    assert!(
        (0.3..0.8).contains(&loud),
        "a full hit near -6 dBFS: {loud}"
    );
    assert!(soft < loud * 0.8, "a soft hit is softer: {soft} vs {loud}");
}
