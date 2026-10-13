//! The domain model, built twice — as a guitar rig and as a keys rig.
//!
//! ```console
//! cargo run -p signal-sampler --example domain_tour
//! ```
//!
//! # Why both rigs
//!
//! They are the two extremes the model has to cover, and they look nothing
//! alike to a player:
//!
//! - The **guitar rig** is one tone at a time. A serial chain: pedals into an
//!   amp into the time effects. Switching patches swaps the whole thing.
//! - The **keys rig** is a *mixer*. Several Engines sound at once, each
//!   holding Layers that split the keyboard; a "stack" is a named scene over
//!   that mixer.
//!
//! Between them they use every level of the model, which is the point of
//! touring both rather than either.
//!
//! # The one primitive
//!
//! They are not two data structures. They are the same one.
//!
//! **Only a Block does anything.** It is the single leaf; Module, Layer,
//! Engine and Preset are all containers, holding other nodes and nothing of
//! their own. So the tree needs exactly two cases:
//!
//! ```text
//! RigNode = Block            a leaf processor — the only thing that runs DSP
//!         | Container        a named folder of RigNodes   ← recursive
//! ```
//!
//! A [`Container`] carries a [`Role`] — `Preset`, `Engine`, `Layer`,
//! `Module` — and that role is a **label**. It names what the node means to a
//! player and drives how a UI draws it. The audio behaviour comes from
//! [`Combine`]: `Serial` chains the children, `Parallel` sums them. Nothing in
//! the renderer branches on `Role`.
//!
//! So "Engine", "Layer" and "Module" are not four types with four sets of
//! operations. They are one type wearing four labels, nestable to any depth —
//! a Module inside a Module inside a Layer is just a tree.

use signal_proto::block::BlockType;
use signal_sampler::rig_node::{Combine, Container, RigNode, Role};

fn main() {
    let guitar = worship_guitar();
    let keys = worship_keys();

    banner("GUITAR — one tone at a time (serial)");
    print_tree(&guitar, 0);

    banner("KEYS — many parts at once (parallel)");
    print_tree(&keys, 0);

    banner("THE SAME PRIMITIVE");
    for (label, tree) in [("guitar", &guitar), ("keys", &keys)] {
        let (containers, blocks, depth) = census(tree, 1);
        println!(
            "  {label:<7} {containers:>3} containers · {blocks:>3} blocks · {depth} levels deep"
        );
    }
    println!(
        "\n  Two instruments, one `RigNode`. What differs is the Combine rule\n  \
         and the Role label — not the type, and not the renderer."
    );
}

// ── The guitar rig ───────────────────────────────────────────────────────────

/// The worship guitar rig: a serial chain, grouped into the modules a player
/// actually reaches for.
///
/// Every child is serial, because a guitar signal is one path. The grouping
/// into "Drive board" / "Amp" / "Time" is not cosmetic — a Module is the unit
/// a preset attaches to, so the whole drive board can be recalled as one
/// named thing independently of the amp in front of it.
fn worship_guitar() -> Container {
    Container::preset("Worship — Ambient")
        .add(
            Container::module("Front of chain")
                .block(BlockType::Compressor, "Compressor")
                .block(BlockType::Volume, "Volume Pedal"),
        )
        .add(
            // Three drive slots, each a NAM capture of a real pedal. They are
            // Drive blocks that happen to be realized as `Nam` — BlockType is
            // the role, BlockKind the realization, and nothing here cares.
            Container::module("Drive board")
                .block(BlockType::Boost, "Boost")
                .block(BlockType::Drive, "Drive 1 — King of Tone")
                .block(BlockType::Drive, "Drive 2 — Morning Glory")
                .block(BlockType::Drive, "Drive 3"),
        )
        .add(
            Container::module("Amp")
                .block(BlockType::Amp, "AC30 Top Boost")
                .block(BlockType::Gate, "Gate")
                .block(BlockType::Eq, "Amp EQ"),
        )
        .add(
            // A Module nested inside a Module — the model does not stop at a
            // fixed number of levels, so "the two delays" can be one recallable
            // thing inside the wider time section.
            Container::module("Time")
                .add(
                    Container::module("Delays")
                        .block(BlockType::Delay, "DLY 1")
                        .block(BlockType::Delay, "DLY 2"),
                )
                .block(BlockType::Reverb, "VERB 1")
                .block(BlockType::Reverb, "VERB 2"),
        )
}

// ── The keys rig ─────────────────────────────────────────────────────────────

/// The worship keys rig: a mixer of Engines that sound together.
///
/// The shape mirrors `signal_keys::KeysProfile::build_tree_with`, and the one
/// subtlety is worth spelling out. An Engine is **serial**, but the Layers
/// inside it hang off a **parallel** "Voices" bag:
///
/// ```text
/// Engine "Keys"        serial    ─ so engine FX can sit after the sum
/// └─ "Keys Voices"     parallel  ─ the lanes are voices played together
///    ├─ Layer "Keys 1"
///    └─ Layer "Keys 2"
/// ```
///
/// A serial Engine would let the last lane overwrite the ones before it — the
/// layers would chain instead of stack. That distinction is the whole reason
/// `Combine` is a separate axis from `Role`.
///
/// Lane names track the live rig's mixer strip, so a player moving off it
/// reaches for the same fader by the same name.
fn worship_keys() -> Container {
    /// One lane: the sounds it loads, and its fader. An empty lane is a real
    /// state — the strip keeps its shape while a part is parked.
    fn lane(name: &str, gain_db: f32, sounds: &[&str]) -> Container {
        let mut layer = Container::layer(name).volume(gain_db);
        for sound in sounds {
            layer = layer.sample_block(*sound, format!("packs/{}.styx", slug(sound)));
        }
        layer
    }

    /// An Engine and its lanes. The Engine is serial so its FX sit after the
    /// sum; the lanes hang off a parallel bag so they stack.
    fn engine(name: &str, lanes: Vec<Container>) -> Container {
        let mut voices = Container::parallel(format!("{name} Voices"));
        for l in lanes {
            voices = voices.add(l);
        }
        Container::engine(name).add(voices)
    }

    // The live rig's mixer strip, lane for lane — see
    // `signal_keys::profile::worship_profile`.
    let engines = Container::parallel("Engines")
        .add(engine(
            "Keys",
            vec![
                // The piano under everything.
                lane("Keys 1", 0.0, &["The Grandeur - Piano"]),
                lane("Keys 2", 0.0, &["Double Felt Grand"]),
                // Empty: the Arturia is parked, but the lane stays so the
                // strip still reads the same.
                lane("Keys 3", 0.0, &[]),
            ],
        ))
        .add(engine(
            "Pad",
            vec![
                // Two sounds in one lane — a Layer loads as many as it needs.
                lane("Pad", -7.1, &["OB-8 PWM Big Strings", "Prophet 5 Classic"]),
                // Not a synth sparkle at all: a men's + women's choir, which
                // is why the wash sounds vocal rather than bright.
                lane(
                    "Shimmer",
                    -10.5,
                    &["Choir Men Ohs - mf", "Choir Women Oos - mf"],
                ),
            ],
        ))
        .add(engine(
            "Organ",
            vec![lane("Organ A", 0.0, &[]), lane("Organ B", 0.0, &[])],
        ))
        // Bass is its own Engine, not an Aux lane: it occupies a register
        // nothing else touches and must not be ducked by a pad swell, so it
        // wants its own fader, FX tail and place in a scene.
        .add(engine(
            "Bass",
            vec![lane("Bass", 0.0, &["Worship PHAT Bass"])],
        ))
        .add(engine(
            "Aux",
            vec![
                lane(
                    "Synth 1",
                    -9.9,
                    &["Dolceola ^ RR Lite", "Clavichord a ^ RR"],
                ),
                lane("Synth 2", -9.4, &["Big Berthas Lead"]),
            ],
        ))
        .add(engine("Drone", vec![lane("Drone", 0.0, &[])]))
        .add(engine(
            "SFX",
            vec![lane("SFX A", 0.0, &[]), lane("SFX B", 0.0, &[])],
        ));

    // The shared tail. A bus needs no new concept — it is a Module sitting
    // beside the engines.
    Container::preset("Worship").add(engines).add(
        Container::module("Global")
            .add(Container::module("Master Reverb").block(BlockType::Reverb, "Reverb")),
    )
}

/// Pack-stem spelling of a sound name, so the example's paths look like the
/// real library's without pretending to be it.
fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

// ── Printing ─────────────────────────────────────────────────────────────────

/// Render the tree, tagging each container with its role and combine rule so
/// the labels-versus-behaviour split is visible.
fn print_tree(container: &Container, depth: usize) {
    let pad = "  ".repeat(depth + 1);
    let combine = match container.combine {
        Combine::Serial => "serial",
        Combine::Parallel => "parallel ∑",
    };
    let zone = if container.zone.is_full() {
        String::new()
    } else {
        format!("  keys {}–{}", container.zone.key_lo, container.zone.key_hi)
    };
    let gain = if container.output_db == 0.0 {
        String::new()
    } else {
        format!("  {:+.1} dB", container.output_db)
    };
    println!(
        "{pad}[{role}] {name}  ({combine}){zone}{gain}",
        role = role_tag(container.role),
        name = container.name,
    );
    for send in &container.sends {
        println!("{pad}    → send to {}", send.target);
    }
    for child in &container.children {
        match child {
            RigNode::Container { container: c } => print_tree(c, depth + 1),
            RigNode::Block { block } => {
                println!(
                    "{pad}    · {} <{}>",
                    block.display_name(),
                    block.block_type.as_str()
                );
            }
        }
    }
}

const fn role_tag(role: Role) -> &'static str {
    match role {
        Role::Preset => "Preset",
        Role::Engine => "Engine",
        Role::Layer => "Layer ",
        Role::Module => "Module",
    }
}

/// Containers, blocks, and how deep the tree goes — the evidence that both
/// rigs are the same structure at different shapes.
fn census(container: &Container, depth: usize) -> (usize, usize, usize) {
    let mut containers = 1;
    let mut blocks = 0;
    let mut deepest = depth;
    for child in &container.children {
        match child {
            RigNode::Container { container: c } => {
                let (sub_c, sub_b, sub_d) = census(c, depth + 1);
                containers += sub_c;
                blocks += sub_b;
                deepest = deepest.max(sub_d);
            }
            RigNode::Block { .. } => blocks += 1,
        }
    }
    (containers, blocks, deepest)
}

fn banner(title: &str) {
    println!("\n── {title} ─────────────────────────────");
}
