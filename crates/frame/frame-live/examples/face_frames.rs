//! Render a face through `LiveSurface` (the path a host takes, caches and
//! all) at intervals, and write each frame out: what actually moves.
//!
//! `cargo run --release -p frame-live --example face_frames -- <face.frame> <out-dir> [w h frames every_ms] [addr=value…]`
//!
//! Writes `<out-dir>/frame-NNN.ppm` and prints, per frame, how many pixels
//! changed from the one before.

fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let (width, rows) = (texture.width(), texture.height());
    let unpadded = width * 4;
    let padded = unpadded.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("face-frames-readback"),
        size: u64::from(padded) * u64::from(rows),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(rows),
            },
        },
        wgpu::Extent3d {
            width,
            height: rows,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let mapped = slice.get_mapped_range();
    let mut rgba = vec![0_u8; (unpadded * rows) as usize];
    for row in 0..rows as usize {
        rgba[row * unpadded as usize..(row + 1) * unpadded as usize].copy_from_slice(
            &mapped[row * padded as usize..row * padded as usize + unpadded as usize],
        );
    }
    rgba
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: face_frames <face.frame> <out-dir> [w h frames every_ms] [addr=value…]";
    let markup = args.next().ok_or(usage)?;
    let out = std::path::PathBuf::from(args.next().ok_or(usage)?);
    let rest: Vec<String> = args.collect();
    let num = |i: usize, d: u32| rest.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
    let (w, h, frames, every) = (num(0, 960), num(1, 164), num(2, 12), num(3, 100));
    let values: Vec<(String, f64)> = rest
        .iter()
        .filter_map(|a| {
            a.split_once('=')
                .and_then(|(k, v)| v.parse().ok().map(|v| (k.to_string(), v)))
        })
        .collect();
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;

    let path = std::path::PathBuf::from(&markup);
    let dir = path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (markup, catalog) = frame_live::face_paths(&dir, &stem);
    let mut live = frame_live::LiveSurface::open(&markup, catalog)?;
    live.apply_real(values);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| format!("adapter: {e}"))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| format!("device: {e}"))?;
    live.attach_gpu(device.clone(), queue.clone())?;
    live.set_max_fps(
        std::env::var("FPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    );
    live.set_stretch(std::env::var_os("STRETCH").is_some());
    live.set_responsive(std::env::var_os("RESPONSIVE").is_some());

    let mut last: Option<Vec<u8>> = None;
    let mut prev: Option<wgpu::Texture> = None;
    for i in 0..frames {
        let (moving, redraw) = (live.is_moving(), live.needs_redraw());
        let (texture, fresh) = live.render(w, h, 1.0)?;
        let px = read(&device, &queue, &texture);
        let changed = last.as_ref().map_or(0, |l| {
            l.chunks(4)
                .zip(px.chunks(4))
                .filter(|(a, b)| a != b)
                .count()
        });
        println!(
            "frame {i:3}: moving {moving} redraw {redraw} fresh {fresh} same texture {} changed {changed} px",
            prev.as_ref().is_some_and(|p| *p == texture)
        );
        prev = Some(texture.clone());
        let mut ppm = format!("P6\n{} {}\n255\n", texture.width(), texture.height()).into_bytes();
        ppm.extend(px.chunks(4).flat_map(|p| [p[0], p[1], p[2]]));
        std::fs::write(out.join(format!("frame-{i:03}.ppm")), ppm).map_err(|e| e.to_string())?;
        last = Some(px);
        std::thread::sleep(std::time::Duration::from_millis(u64::from(every)));
    }
    Ok(())
}
