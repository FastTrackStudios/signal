//! The compressor panel, rendered by the real shader, to PNGs.
//!
//!     cargo run -p comp-ui --features viz,probe --example comp_sheet -- out/
//!
//! Two states, because what the panel looks like at rest and what it looks
//! like while it is working are different pictures and only one of them gets
//! looked at by accident.

use comp_ui::viz::{CompUniforms, CompView, SHADER};

const W: u32 = 620;
const H: u32 = 520;

/// A plucked, decaying input — the shape the design rig actually plays, so
/// the traces have the steep attacks that show up edge artefacts.
fn trace(n: usize, offset: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * 4.0 + offset;
            let phase = t.fract();
            let env = (-phase * 3.2).exp();
            (env * 0.92).clamp(0.0, 1.0)
        })
        .collect()
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir).expect("output directory");
    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU available — nothing to render");
        std::process::exit(2);
    };

    for (tag, grabbable, gr) in [("rest", false, 0.0_f32), ("working", true, 7.5_f32)] {
        let view = CompView {
            threshold: -22.0,
            ratio: 4.0,
            knee: 6.0,
            in_db: -9.0,
            gr_db: gr,
            input: trace(120, 0.0),
            gr: trace(120, 0.35).iter().map(|v| v * 0.45).collect(),
            on: true,
            grabbable,
            color: [228, 228, 231],
            time: 1.4,
        };
        let u = CompUniforms::of(&view, W as f32, H as f32);
        let frame = probe.render(SHADER, bytemuck::bytes_of(&u), W, H);
        let name = format!("{dir}/comp-{tag}.png");
        write_png(&name, &frame.rgba, W, H);
        println!("{name}  coverage {:.3}", frame.coverage());
    }
}

/// The frames are premultiplied; undo it and lay them on the panel's ground.
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
