//! Every reverb family, rendered by the real shader, to PNGs.
//!
//!     cargo run -p reverb-ui --features viz,probe --example family_sheet -- out/
//!
//! Eight machines, each of which has to be reachable to be looked at. Driving
//! the algorithm picker eight times in a running rig to photograph each one is
//! not a loop anybody runs twice, and the families nobody reaches are the ones
//! that ship unlooked-at. This reaches all eight without a window.

use reverb_ui::viz::{Family, ReverbUniforms, ReverbView};

const W: u32 = 760;
const H: u32 = 190;

fn view(family: Family, time: f32) -> ReverbView {
    ReverbView {
        // One decay for every machine, so the only thing that differs
        // between the frames is the machine.
        decay: 2.6,
        density: 0.55,
        predelay: 0.035,
        mix: 0.5,
        damp: 0.35,
        family,
        on: true,
        beat: 0.5,
        color: [167, 139, 250],
        time,
    }
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir).expect("output directory");

    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU available — nothing to render");
        std::process::exit(2);
    };

    for family in Family::ALL {
        // Two moments: a random space's whole character is that it is not
        // the same twice, and a still frame cannot say so.
        for (tag, time) in [("a", 0.4_f32), ("b", 2.3_f32)] {
            let v = view(family, time);
            let u = ReverbUniforms::of(&v, W as f32, H as f32);
            let frame = probe.render(reverb_ui::viz::SHADER, bytemuck::bytes_of(&u), W, H);
            let name = format!("{dir}/reverb-{}-{tag}.png", label(family));
            write_png(&name, &frame.rgba, W, H);
            println!("{name}  coverage {:.3}", frame.coverage());
        }
    }
}

fn label(f: Family) -> &'static str {
    match f {
        Family::Room => "room",
        Family::Hall => "hall",
        Family::Plate => "plate",
        Family::Spring => "spring",
        Family::Ambient => "ambient",
        Family::Random => "random",
        Family::Special => "special",
        Family::Convolution => "convolution",
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
