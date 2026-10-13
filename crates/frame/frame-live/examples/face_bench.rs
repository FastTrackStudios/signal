//! Render a face over and over, moving, and time it: what one animated
//! face costs a host per frame.
//!
//! `cargo run --release -p frame-live --example face_bench -- <face.frame> [w h frames] [addr=value…]`

use std::time::Instant;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let markup = args.next().ok_or("usage: face_bench <face.frame> [w h frames] [addr=value…]")?;
    let rest: Vec<String> = args.collect();
    let num = |i: usize, d: u32| rest.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
    let (w, h, frames) = (num(0, 600), num(1, 400), num(2, 480));
    let values: Vec<(String, f64)> = rest
        .iter()
        .filter_map(|a| a.split_once('=').and_then(|(k, v)| v.parse().ok().map(|v| (k.to_string(), v))))
        .collect();

    let path = std::path::PathBuf::from(&markup);
    let dir = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let (markup, catalog) = frame_live::face_paths(&dir, &stem);
    let mut live = frame_live::LiveSurface::open(&markup, catalog)?;
    live.apply_real(values);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .map_err(|e| format!("adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .map_err(|e| format!("device: {e}"))?;
    live.attach_gpu(device.clone(), queue)?;
    live.set_max_fps(0);

    // CPU: building and submitting a frame (what the host's thread pays);
    // total: that and the GPU finishing it.
    let (mut cpu, mut total) = (Vec::with_capacity(frames as usize), Vec::with_capacity(frames as usize));
    let mut moving = 0;
    for _ in 0..frames {
        moving += usize::from(live.is_moving());
        let at = Instant::now();
        live.render(w, h, 1.0)?;
        cpu.push(at.elapsed().as_secs_f64() * 1000.0);
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        total.push(at.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
    let stats = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        format!("mean {mean:.3} ms  p50 {:.3}  p95 {:.3}", v[v.len() / 2], v[v.len() * 95 / 100])
    };
    println!("{} {w}x{h}: {frames} frames ({moving} moving)", path.display());
    println!("  cpu   {}", stats(cpu));
    println!("  total {}", stats(total));
    Ok(())
}
