//! Any frame embed as a nice-plug `EmbeddedEditor`: REAPER's TCP/MCP
//! inline UI (`REAPER_FXEMBED`, via nice-plug's CLAP `cockos.reaper_embedui`
//! shim).
//!
//! REAPER owns the slot. It asks for size hints (`GETMINMAXINFO`), then
//! paints on its UI timer — about 30 Hz, with `PAINT_OPTIONAL` set when
//! nothing forced it — into a LICE bitmap the size of the slot in device
//! pixels (BGRA words, `0xAARRGGBB`, a row span, possibly bottom-up), with
//! the DPI as 24.8 fixed point. Mouse messages arrive with slot-local
//! device-pixel coordinates; capture is automatic between down and up;
//! returning `INVALIDATE` forces a non-optional repaint.
//!
//! This editor rasterises on the CPU, on REAPER's thread, straight into
//! that bitmap: an MCP-sized frame costs well under 2 ms (release; see
//! `tests/harness.rs`), less than a GPU round trip before its readback. It
//! skips optional paints when nothing changed, and relayouts only when the
//! slot's size does.
//!
//! Parameter edits leave through `on_calls`: the plugin turns them into
//! its own param changes (a `ParamSetter`, its shared params) — nice-plug
//! gives an embedded editor no `GuiContext` of its own. Values and feeds
//! come in through [`EmbedHandle`], cloned into the plugin.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use nice_plug_core::editor::embedded::{
    EmbedBitmap, EmbedContext, EmbedDrawInfo, EmbedMouseEvent, EmbedSizeHints, EmbeddedEditor,
    embed_flags,
};
use surface_proto::{ParamValue, StreamFrame};
use surface_runtime::{EngineCall, Modifiers};

use crate::cpu::{Pixels, Rasterizer};
use crate::embed::{Button, Embed, EmbedView, Pointer};
use crate::form::EmbedHost;

/// Where engine calls go.
pub type CallSink = Arc<dyn Fn(Vec<EngineCall>) + Send + Sync>;

struct State {
    embed: Embed,
    raster: Rasterizer,
    /// The last frame painted, and the device size it was painted at.
    last: Option<Pixels>,
    /// Something moved that the last frame does not show.
    stale: bool,
    context: EmbedContext,
    scale: f64,
}

/// Values and feeds for the editor, from the plugin (any thread).
#[derive(Clone)]
pub struct EmbedHandle(Arc<Mutex<State>>);

impl EmbedHandle {
    pub fn apply_values(&self, values: impl IntoIterator<Item = ParamValue>) {
        let mut s = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        s.embed.apply_values(values);
        s.stale = true;
    }

    pub fn apply_frame(&self, frame: StreamFrame, t_ms: f64) {
        let mut s = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        s.embed.apply_frame(frame, t_ms);
        s.stale = true;
    }

    /// Anything else the embed offers (curve overrides, say).
    pub fn with<R>(&self, f: impl FnOnce(&mut Embed) -> R) -> R {
        let mut s = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        s.stale = true;
        f(&mut s.embed)
    }
}

/// See the module docs.
pub struct FrameEmbeddedEditor {
    state: Arc<Mutex<State>>,
    on_calls: CallSink,
    start: Instant,
}

impl FrameEmbeddedEditor {
    /// `view` in REAPER's slots; edits go to `on_calls`.
    pub fn new(
        view: impl EmbedView + 'static,
        on_calls: impl Fn(Vec<EngineCall>) + Send + Sync + 'static,
    ) -> Self {
        let state = State {
            embed: Embed::new(view, EmbedHost::Mcp),
            raster: Rasterizer::new(),
            last: None,
            stale: true,
            context: EmbedContext::Mcp,
            scale: 1.0,
        };
        Self {
            state: Arc::new(Mutex::new(state)),
            on_calls: Arc::new(on_calls),
            start: Instant::now(),
        }
    }

    /// The handle the plugin feeds values and frames through.
    #[must_use]
    pub fn handle(&self) -> EmbedHandle {
        EmbedHandle(self.state.clone())
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The embed host for REAPER's context.
#[must_use]
pub fn host_of(context: EmbedContext) -> EmbedHost {
    match context {
        EmbedContext::Tcp => EmbedHost::Tcp,
        EmbedContext::Mcp | EmbedContext::Unknown => EmbedHost::Mcp,
    }
}

/// Logical px per device px for a draw.
fn scale_of(info: &EmbedDrawInfo) -> f64 {
    let dpi = f64::from(info.dpi);
    if dpi.is_finite() && dpi > 0.1 {
        dpi
    } else {
        1.0
    }
}

impl EmbeddedEditor for FrameEmbeddedEditor {
    fn size_hints(&self, context: EmbedContext, dpi: f32) -> Option<EmbedSizeHints> {
        // The view's own hints for the slot (a single unit knows its aspect).
        let h = self.lock().embed.hints_for(host_of(context));
        let k = f64::from(if dpi > 0.1 { dpi } else { 1.0 });
        let px = |v: f64| (v * k).round() as i32;
        Some(EmbedSizeHints {
            preferred_aspect: h.preferred_aspect as f32,
            minimum_aspect: h.minimum_aspect as f32,
            min_width: px(h.min.0),
            min_height: px(h.min.1),
            max_width: px(h.max.0),
            max_height: px(h.max.1),
        })
    }

    fn paint(&self, bitmap: &mut EmbedBitmap<'_>, info: &EmbedDrawInfo) -> bool {
        let t = self.now();
        let mut s = self.lock();
        let (dw, dh) = (bitmap.width, bitmap.height);
        if dw == 0 || dh == 0 {
            return false;
        }
        let scale = scale_of(info);
        if s.context != info.context {
            s.context = info.context;
            s.stale = true;
        }
        let resized = s
            .embed
            .set_size(f64::from(dw) / scale, f64::from(dh) / scale)
            || (s.scale - scale).abs() > 1e-6;
        s.scale = scale;
        let fresh = s
            .last
            .as_ref()
            .is_some_and(|p| (p.width, p.height) == (dw, dh));
        let moving = s.embed.animating() || s.embed.needs_record() || s.stale;
        if info.is_paint_optional() && fresh && !resized && !moving {
            return false;
        }
        let calls = s.embed.poll(t);
        let st = &mut *s;
        let px = st
            .raster
            .render(dw, dh, scale, crate::cpu::STRIP_GROUND, |c, at| {
                st.embed.paint(c, at, t)
            });
        px.write_bgra(bitmap.bits, bitmap.row_span as usize, bitmap.flipped);
        st.last = Some(px);
        st.stale = false;
        drop(s);
        if !calls.is_empty() {
            (self.on_calls)(calls);
        }
        true
    }

    fn mouse_event(&self, event: EmbedMouseEvent, info: &EmbedDrawInfo) -> u32 {
        let t = self.now();
        let k = 1.0 / scale_of(info);
        let (x, y) = (f64::from(info.mouse_x) * k, f64::from(info.mouse_y) * k);
        let mods = Modifiers::default();
        let pointer = match event {
            EmbedMouseEvent::Move => Pointer::Move { x, y },
            EmbedMouseEvent::LeftDown => Pointer::Down {
                x,
                y,
                button: Button::Left,
                mods,
            },
            EmbedMouseEvent::LeftUp => Pointer::Up {
                x,
                y,
                button: Button::Left,
            },
            EmbedMouseEvent::LeftDoubleClick => Pointer::DoubleClick {
                x,
                y,
                button: Button::Left,
            },
            EmbedMouseEvent::RightDown => Pointer::Down {
                x,
                y,
                button: Button::Right,
                mods,
            },
            EmbedMouseEvent::RightUp => Pointer::Up {
                x,
                y,
                button: Button::Right,
            },
            EmbedMouseEvent::RightDoubleClick => Pointer::DoubleClick {
                x,
                y,
                button: Button::Right,
            },
            // 120 per step is one notch.
            EmbedMouseEvent::Wheel { amount } => Pointer::Wheel {
                x,
                y,
                notches: f64::from(amount) / 120.0,
                mods,
            },
        };
        let out = {
            let mut s = self.lock();
            let out = s.embed.pointer(pointer, t);
            if out.redraw {
                s.stale = true;
            }
            out
        };
        if !out.calls.is_empty() {
            (self.on_calls)(out.calls);
        }
        // A right click is REAPER's (its FX menu): not handled.
        if out.passthrough {
            return 0;
        }
        let mut flags = 0;
        if out.handled {
            flags |= embed_flags::HANDLED;
        }
        if out.redraw {
            flags |= embed_flags::INVALIDATE;
        }
        flags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tone::{TonePanel, Which};
    use nice_plug_core::editor::embedded::draw_info_flags;

    fn info(w: i32, h: i32, dpi: f32, x: i32, y: i32, flags: u32) -> EmbedDrawInfo {
        EmbedDrawInfo {
            context: EmbedContext::Mcp,
            dpi,
            width: w,
            height: h,
            mouse_x: x,
            mouse_y: y,
            flags,
            mousewheel_amt: 0,
        }
    }

    /// REAPER's calls, simulated: hints, paint (forced and optional),
    /// a drag on the threshold, a right click.
    #[test]
    fn reaper_drives_it() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let sink = calls.clone();
        let ed = FrameEmbeddedEditor::new(TonePanel::new(Which::Comp), move |c| {
            sink.lock().unwrap().extend(c)
        });
        let hints = ed.size_hints(EmbedContext::Mcp, 2.0).expect("hints");
        assert!(
            hints.min_width >= 32 && hints.preferred_aspect > 0.0,
            "{hints:?}"
        );

        for (w, h, dpi) in [
            (60, 120, 1.0),
            (96, 200, 1.0),
            (192, 400, 2.0),
            (300, 40, 1.0),
        ] {
            let mut bits = vec![0u32; (w * h) as usize];
            let mut bmp = unsafe { EmbedBitmap::new(&mut bits, w, h, w, false) };
            assert!(
                ed.paint(&mut bmp, &info(w as i32, h as i32, dpi, 0, 0, 0)),
                "a forced paint draws"
            );
            assert!(bits.iter().all(|p| p >> 24 == 0xff), "opaque");
            let mut bmp = unsafe { EmbedBitmap::new(&mut bits, w, h, w, false) };
            assert!(
                !ed.paint(
                    &mut bmp,
                    &info(
                        w as i32,
                        h as i32,
                        dpi,
                        0,
                        0,
                        draw_info_flags::PAINT_OPTIONAL
                    )
                ),
                "nothing moved: skipped"
            );
        }

        // 96 x 170 at 2x: the threshold line at −18 dB is 58.8 logical px down.
        let (w, h) = (192, 340);
        let mut bits = vec![0u32; (w * h) as usize];
        let mut bmp = unsafe { EmbedBitmap::new(&mut bits, w, h, w, true) };
        assert!(ed.paint(&mut bmp, &info(w as i32, h as i32, 2.0, 0, 0, 0)));
        let y = (58.8 * 2.0) as i32;
        let f = ed.mouse_event(
            EmbedMouseEvent::LeftDown,
            &info(w as i32, h as i32, 2.0, 120, y, 0),
        );
        assert!(f & embed_flags::HANDLED != 0);
        for i in 1..=6 {
            ed.mouse_event(
                EmbedMouseEvent::Move,
                &info(
                    w as i32,
                    h as i32,
                    2.0,
                    120,
                    y + 10 * i,
                    draw_info_flags::LBUTTON_CAPTURED,
                ),
            );
        }
        let f = ed.mouse_event(
            EmbedMouseEvent::LeftUp,
            &info(w as i32, h as i32, 2.0, 120, y + 60, 0),
        );
        assert!(f & embed_flags::INVALIDATE != 0 || !calls.lock().unwrap().is_empty());
        let mut bmp = unsafe { EmbedBitmap::new(&mut bits, w, h, w, true) };
        assert!(
            ed.paint(
                &mut bmp,
                &info(
                    w as i32,
                    h as i32,
                    2.0,
                    0,
                    0,
                    draw_info_flags::PAINT_OPTIONAL
                )
            ),
            "an edit repaints"
        );
        assert!(
            calls.lock().unwrap().iter().any(
                |c| matches!(c, EngineCall::SetParam { addr, .. } if addr.0 == "comp/threshold")
            )
        );
        assert!(ed.handle().with(|e| e.value("comp/threshold").unwrap()) < -25.0);
        assert_eq!(
            ed.mouse_event(
                EmbedMouseEvent::RightDown,
                &info(w as i32, h as i32, 2.0, 120, y, 0)
            ),
            0,
            "REAPER's menu"
        );
    }
}
