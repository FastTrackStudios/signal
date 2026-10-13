//! `--screenshot`: the player loop offscreen, with optional scripted input
//! that goes through the same window → surface mapping as real events.
//!
//! With `--library … --updates N` it keeps running after the first
//! screenshot, following the surface live in real time, and writes
//! `OUT-<k>.png` after each update it applies or rejects — the same
//! process before and after, so it shows an edit arriving without a
//! relaunch.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use surface_runtime::Modifiers;

use crate::input::View;
use crate::libview::{LibraryView, Outcome};
use crate::player::{Player, banner};
use crate::{Args, Script};

/// Frame period of the simulated clock (60 fps).
const FRAME_MS: f64 = 1000.0 / 60.0;
/// Scripted drag: press on frame 1, move up for `DRAG_STEPS` frames, release.
const DRAG_STEPS: u32 = 12;
/// Surface px moved up per step (0.3 of a full knob sweep in total).
const DRAG_STEP_PX: f64 = 5.0;
/// `--updates`: give up waiting after this long.
const UPDATES_TIMEOUT: Duration = Duration::from_secs(120);

struct Shot<'a> {
    args: &'a Args,
    player: Player,
    library: Option<LibraryView>,
    error: Option<String>,
    offscreen: render::offscreen::Offscreen,
    overlay: vello::Scene,
}

impl Shot<'_> {
    fn view(&self) -> View {
        let (w, h) = self.args.size;
        let content = match &self.library {
            Some(lib) if lib.browsing() => lib.content(),
            _ => self.player.content(),
        };
        View::fit(content, f64::from(w), f64::from(h))
    }

    /// Render the frame at `t` ms.
    fn frame(&mut self, t: f64) -> Result<render::offscreen::Pixels, String> {
        let (w, h) = self.args.size;
        let view = self.view();
        self.player.tick(t);
        self.overlay.reset();
        let message = self
            .error
            .as_deref()
            .map(|e| self.args.source.short_error(e))
            .or_else(|| self.library.as_ref().and_then(LibraryView::banner))
            .or_else(|| self.player.engine_banner());
        if let Some((b, page)) = message.and_then(|m| banner(&m, f64::from(w))) {
            render::paint_scene_into(&mut self.overlay, &b, page, kurbo::Affine::IDENTITY, Some(crate::player::fonts()));
        }
        if let Some((toast, page, tw, th)) = self.library.as_mut().and_then(|l| l.toast(t, f64::from(w))) {
            let at = kurbo::Affine::translate(((f64::from(w) - tw) / 2.0, f64::from(h) - th - 18.0));
            render::paint_scene_into(&mut self.overlay, &toast, page, at, Some(crate::player::fonts()));
        }
        let browsing = self.library.as_ref().is_some_and(LibraryView::browsing);
        let (doc, page, base) = match self.library.as_mut().and_then(LibraryView::document) {
            Some((scene, page)) => (scene.clone(), *page, crate::browser::BACKGROUND),
            None => {
                let (doc, page) = self.player.document(t);
                (doc, page, self.player.background())
            }
        };
        let values = if browsing { Default::default() } else { self.player.control_positions() };
        let inputs = if browsing { Default::default() } else { self.player.shader_inputs() };
        // Time advances with the simulated clock, so a flicker animates.
        self.offscreen.render_with_effects(&render::fx::Frame {
            transform: view.affine(),
            base,
            fonts: Some(crate::player::fonts()),
            mode: self.args.fx,
            time_s: t / 1000.0,
            values: Some(&values),
            inputs: Some(&inputs),
            overlay: Some(&self.overlay),
            ..render::fx::Frame::new(&doc, page, w, h)
        })
    }
}

/// The editing scripts, frame by frame. Events go through the window →
/// surface mapping like real ones (a press, a double-click 50 ms later, a
/// right-click, typed text).
fn edit_script(shot: &mut Shot<'_>, script: Script, i: u32, t: f64) -> Result<(), String> {
    let view = shot.view();
    let rt = shot.player.rt();
    let handles = shot.player.shown_handles();
    let handle = handles.get(2).or(handles.first()).copied().ok_or("the surface has no handle")?;
    let canvas = shot.player.canvas().ok_or("the surface has no canvas");
    let hb = rt.hit_box(handle).ok_or("the handle has no box")?;
    let hc = (hb.x + hb.width / 2.0, hb.y + hb.height / 2.0);
    let m = Modifiers::default();
    // Window px (what the host would see), and back.
    let press = |shot: &mut Shot<'_>, at: (f64, f64), t: f64| {
        let (wx, wy) = view.to_window(at.0, at.1);
        shot.player.pointer_down(view.to_surface(wx, wy), (wx, wy), m, t);
        shot.player.pointer_up(t + 5.0);
    };
    match (script, i) {
        (Script::SelectHandle, 1) => press(shot, hc, t),
        (Script::RenameHandle, 1 | 3) => press(shot, hc, t),
        (Script::RenameHandle, 5) => {
            shot.player.text("Kick");
        }
        (Script::AddOnCanvas, 1 | 3) => {
            let c = canvas?;
            press(shot, (c.x + c.width * 0.64, c.y + c.height * 0.3), t);
        }
        (Script::ContextHandle, 1) => shot.player.context(hc, t),
        (Script::Marquee, _) => {
            let c = canvas?;
            let from = (c.x + c.width * 0.2, c.y + 6.0);
            let to = (c.x + c.width * 0.62, c.y + c.height - 8.0);
            let k = f64::from(i.saturating_sub(1).min(10)) / 10.0;
            let at = (from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
            match i {
                1 => shot.player.pointer_down(at, view.to_window(at.0, at.1), m, t),
                2..=11 => shot.player.pointer_move(at, m, t),
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}

fn save(px: render::offscreen::Pixels, out: &Path) -> Result<usize, String> {
    let distinct = {
        let mut seen: Vec<[u8; 4]> = px.rgba.as_chunks::<4>().0.iter().step_by(97).copied().collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    image::RgbaImage::from_raw(px.width, px.height, px.rgba)
        .ok_or("bad pixel buffer")?
        .save(out)
        .map_err(|e| format!("writing {}: {e}", out.display()))?;
    Ok(distinct)
}

/// `OUT.png` → `OUT-<k>.png`.
fn numbered(out: &Path, k: u32) -> PathBuf {
    let stem = out.file_stem().map_or_else(|| "shot".into(), |s| s.to_string_lossy().into_owned());
    let ext = out.extension().map_or_else(|| "png".into(), |e| e.to_string_lossy().into_owned());
    out.with_file_name(format!("{stem}-{k}.{ext}"))
}

pub fn run(args: &Args) -> Result<(), String> {
    let out = args.screenshot.as_ref().ok_or("--screenshot needs a path")?;
    let crate::Started { engine, surface, error, library } = crate::start(args)?;
    let mut shot = Shot {
        args,
        player: Player::new(engine, surface)?,
        library,
        error,
        offscreen: render::offscreen::Offscreen::new()?,
        overlay: vello::Scene::new(),
    };

    // A knob is dragged up; a handle up and to the right.
    let (target, dx) = match args.script {
        Some(Script::DragFirstKnob) => (Some(shot.player.first_knob().ok_or("--script drag-first-knob: the surface has no knob")?), 0.0),
        Some(Script::DragFirstHandle) => (Some(shot.player.first_handle().ok_or("--script drag-first-handle: the surface has no handle")?), 1.0),
        _ => (None, 0.0),
    };
    let editing = args.script.filter(|s| !matches!(s, Script::DragFirstKnob | Script::DragFirstHandle | Script::OpenSelected));
    let knob = match target {
        Some((id, b)) => {
            let view = shot.view();
            let addr = shot.player.param_of(id).ok_or("the dragged control isn't bound to a param")?;
            let before = shot.player.engine_value(&addr).unwrap_or_default();
            if args.frames < DRAG_STEPS + 3 {
                eprintln!("frame: --frames {} is too few to finish the drag (need {})", args.frames, DRAG_STEPS + 3);
            }
            Some((addr, before, view.to_window(b.x + b.width / 2.0, b.y + b.height / 2.0)))
        }
        None => None,
    };

    let mut pixels = None;
    let mut spent = 0.0;
    for i in 0..args.frames {
        let t = f64::from(i) * FRAME_MS;
        let started = Instant::now();
        if i == 1
            && args.script == Some(Script::OpenSelected)
            && let Some(lib) = &mut shot.library
        {
            lib.open_selected(&mut shot.player);
        }
        if let Some((_, _, (wx, wy))) = &knob {
            let view = shot.view();
            // Scripted events arrive in window px, like winit's.
            let at = |dy: f64| view.to_surface(wx + dx * dy * view.scale, wy - dy * view.scale);
            let step = f64::from(i.saturating_sub(1).min(DRAG_STEPS)) * DRAG_STEP_PX;
            match i {
                1 => shot.player.pointer_down(at(0.0), (*wx, *wy), Modifiers::default(), t),
                i if (2..DRAG_STEPS + 2).contains(&i) => shot.player.pointer_move(at(step), Modifiers::default(), t),
                i if i == DRAG_STEPS + 2 => shot.player.pointer_up(t),
                _ => {}
            }
        }
        if let Some(script) = editing {
            edit_script(&mut shot, script, i, t)?;
        }
        if let Some(lib) = &mut shot.library {
            lib.tick(&mut shot.player);
        }
        pixels = Some(shot.frame(t)?);
        spent += started.elapsed().as_secs_f64() * 1000.0;
    }

    let px = pixels.ok_or("--frames must be at least 1")?;
    let distinct = save(px, out)?;
    let (w, h) = args.size;
    let avg = spent / f64::from(args.frames.max(1));
    let mut report = format!(
        "{{\"ok\":true,\"out\":\"{}\",\"width\":{w},\"height\":{h},\"frames\":{},\"avg_frame_ms\":{avg:.3},\"distinct_colors\":{distinct}",
        out.display(),
        args.frames,
    );
    if let Some((addr, before, _)) = &knob {
        let after = shot.player.engine_value(addr).unwrap_or_default();
        report += &format!(",\"param\":\"{}\",\"before\":{before:.4},\"after\":{after:.4}", addr.0);
    }
    if let Some(e) = &shot.error {
        report += &format!(",\"error\":{}", serde_json::to_string(e).unwrap_or_default());
    }
    if editing.is_some() {
        let rt = shot.player.rt();
        report += &format!(
            ",\"selected\":{},\"menus\":{},\"editing\":{}",
            rt.selection().len(),
            rt.open_menus().len(),
            serde_json::to_string(&rt.editing().map(|(_, s)| s.to_string())).unwrap_or_default()
        );
    }

    // `--updates N`: keep following in real time; a shot per update.
    if args.updates > 0 && shot.library.is_some() {
        eprintln!("frame: waiting for {} live update(s)…", args.updates);
        let clock = Instant::now();
        let t0 = f64::from(args.frames) * FRAME_MS;
        let mut shots = Vec::new();
        while shots.len() < args.updates as usize && clock.elapsed() < UPDATES_TIMEOUT {
            std::thread::sleep(Duration::from_millis(16));
            let t = t0 + clock.elapsed().as_secs_f64() * 1000.0;
            let outcomes = shot.library.as_mut().map(|l| l.tick(&mut shot.player)).unwrap_or_default();
            for o in outcomes {
                let what = match &o {
                    Outcome::Applied { id, version } => format!("{{\"applied\":\"{id}\",\"version\":{version}"),
                    Outcome::Rejected(e) => format!("{{\"rejected\":{}", serde_json::to_string(e).unwrap_or_default()),
                    Outcome::Changed => continue,
                };
                if matches!(o, Outcome::Applied { .. }) {
                    shot.error = None;
                }
                // A few frames so meters and the toast settle, then the shot.
                let mut px = None;
                for k in 0..6 {
                    px = Some(shot.frame(t + f64::from(k) * FRAME_MS)?);
                }
                let path = numbered(out, shots.len() as u32 + 1);
                save(px.ok_or("no frame")?, &path)?;
                eprintln!("frame: {} → {}", what.trim_start_matches('{'), path.display());
                shots.push(format!("{what},\"out\":\"{}\"}}", path.display()));
                if shots.len() >= args.updates as usize {
                    break;
                }
            }
        }
        report += &format!(",\"updates\":[{}]", shots.join(","));
        if shots.len() < args.updates as usize {
            println!("{report}}}");
            return Err(format!("only {} of {} updates arrived within {:?}", shots.len(), args.updates, UPDATES_TIMEOUT));
        }
    }
    println!("{report}}}");
    Ok(())
}
