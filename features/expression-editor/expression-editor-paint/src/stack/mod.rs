//! The stacked multitrack view — every track at once, on one timeline.
//!
//! Each track gets a horizontal lane and is drawn in *its own* mode: a
//! vocal as blobs, its reference MIDI as a roll, a guitar as tab, a kit
//! as its mics' audio with the detected hits marked on it. Time is
//! shared; vertical space is divided.
//!
//! This is the audio drum workflow's surface: kick, snare, toms and
//! everything else as role lanes, summed waveforms, and thousands of
//! hit markers that thin themselves as they crowd. It is the densest
//! thing the editor draws, and the one that gains most from being a
//! scene rather than elements — see [`paint`] for the measurements.
//!
//! `geometry` decides where everything goes, `paint` turns that into a
//! scene, `waveform` sums the members' peaks into columns, `zoom` is
//! the drag-to-zoom camera, and [`interact`] is the gesture state
//! machine a host drives — the same one for every renderer.

pub mod geometry;
pub mod interact;
pub mod paint;
pub mod waveform;
pub mod zoom;

pub use expression_editor_core::drum::HitGesture;
pub use geometry::{LaneNote, LaneView, SubLane, chrome_shelves, lanes, ruler_height};
pub use interact::Stack;
pub use waveform::summed_columns;

/// Replay one recorded command into a painter, under a transform.
///
/// `anyrender::Scene::append_scene` takes the scene by value; a cached
/// scene is replayed every frame and must not be cloned to do it.
pub fn submit(
    painter: &mut impl anyrender::PaintScene,
    cmd: &anyrender::recording::RenderCommand,
    at: kurbo::Affine,
) {
    use anyrender::recording::RenderCommand;
    match cmd {
        RenderCommand::Fill(fill) => painter.fill(
            fill.fill,
            compose(at, fill.transform),
            &fill.brush,
            fill.brush_transform,
            &fill.shape,
        ),
        RenderCommand::Stroke(stroke) => painter.stroke(
            &stroke.style,
            compose(at, stroke.transform),
            &stroke.brush,
            stroke.brush_transform,
            &stroke.shape,
        ),
        RenderCommand::GlyphRun(run) => painter.draw_glyphs(
            &run.font_data,
            run.font_size,
            run.hint,
            &run.normalized_coords,
            run.embolden,
            &run.style,
            &run.brush,
            run.brush_alpha,
            compose(at, run.transform),
            run.glyph_transform,
            run.glyphs.iter().copied(),
        ),
        RenderCommand::PushClipLayer(clip) => {
            painter.push_clip_layer(compose(at, clip.transform), &clip.clip)
        }
        RenderCommand::PushLayer(layer) => painter.push_layer(
            layer.blend,
            layer.alpha,
            compose(at, layer.transform),
            &layer.clip,
            layer.filter.clone(),
            layer.backdrop_filter.clone(),
        ),
        RenderCommand::PopLayer => painter.pop_layer(),
        RenderCommand::BoxShadow(shadow) => painter.draw_box_shadow(
            compose(at, shadow.transform),
            shadow.rect,
            shadow.brush,
            shadow.radius,
            shadow.std_dev,
        ),
    }
}

/// `outer` after `inner`.
fn compose(outer: kurbo::Affine, inner: kurbo::Affine) -> kurbo::Affine {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "affine composition; the operator is matrix multiplication"
    )]
    let composed = outer * inner;
    composed
}
