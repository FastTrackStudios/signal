//! Every modulation engine, rendered by the real shader, to PNGs.
//!
//!     cargo run -p modulation-ui --features viz,probe --example engine_sheet -- out/
//!
//! Six engines, and the only other place to see one is a running rig with
//! that engine loaded. See `fts_audio_ui::shader::probe` for why that is not
//! a loop worth running six times.

use fts_audio_ui::shader::Uniforms;
use modulation_ui::viz::{Engine, SHADER, engine_index};

const W: u32 = 760;
const H: u32 = 220;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir).expect("output directory");

    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU available — nothing to render");
        std::process::exit(2);
    };

    for engine in Engine::ALL {
        // The modulation group is cyan-led and motion pink-led, and some of
        // these engines appear in both — so each is drawn in both colours,
        // which is also how they are actually mounted in a rig.
        for (tag, color) in [
            ("mod", [34.0_f32, 211.0, 238.0]),
            ("motion", [244.0, 114.0, 182.0]),
        ] {
            // Two moments, because every one of these is a moving picture and
            // a still frame is half the story.
            for (moment, time) in [("a", 0.6_f32), ("b", 2.4_f32)] {
                let u = Uniforms {
                    frame: [W as f32, H as f32, time, engine_index(engine)],
                    // A rate and depth that let each engine show its shape:
                    // slow enough to read, deep enough to be doing something.
                    params: [0.9, 0.75, 0.5, 1.0],
                    color: [color[0] / 255.0, color[1] / 255.0, color[2] / 255.0, 1.0],
                };
                let frame = probe.render(SHADER, bytemuck::bytes_of(&u), W, H);
                let name = format!("{dir}/{tag}-{}-{moment}.png", label(engine));
                write_png(&name, &frame.rgba, W, H);
                println!("{name}  coverage {:.3}", frame.coverage());
            }
        }
    }
}

fn label(e: Engine) -> &'static str {
    match e {
        Engine::Chorus => "chorus",
        Engine::Flanger => "flanger",
        Engine::Phaser => "phaser",
        Engine::Tremolo => "tremolo",
        Engine::Vibrato => "vibrato",
        Engine::Rotary => "rotary",
    }
}

/// The frames are premultiplied, which a PNG viewer is not expecting; undo it
/// and lay them on the panel's own ground so the sheet looks like the rack.
fn write_png(path: &str, rgba: &[u8], w: u32, h: u32) {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let a = f32::from(px[3]) / 255.0;
        let over = |c: u8| -> u8 {
            let lit = if a > 0.001 { f32::from(c) / a } else { 0.0 };
            (lit.mul_add(a, 8.0 * (1.0 - a))).clamp(0.0, 255.0) as u8
        };
        out.extend_from_slice(&[over(px[0]), over(px[1]), over(px[2]), 255]);
    }
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&out)
        .expect("png data");
}
