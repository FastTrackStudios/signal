//! Horizontal zoom gestures for the stacked timeline. Lane heights stay intact.
use expression_editor_core::Editor;

#[derive(Clone, Copy)]
pub struct TimeZoom {
    pub origin: f64,
    pub current: f64,
    pub marquee: bool,
    base_units: f64,
    anchor: f64,
}

impl TimeZoom {
    pub fn begin(editor: &Editor, x: f64, marquee: bool) -> Self {
        Self {
            origin: x,
            current: x,
            marquee,
            base_units: editor.camera.units_per_px,
            anchor: editor.camera.t_at(x),
        }
    }

    pub fn update(&mut self, editor: &mut Editor, x: f64, fine: bool) {
        self.current = x;
        if !self.marquee {
            let gain = if fine { 800.0 } else { 200.0 };
            editor.camera.units_per_px =
                (self.base_units / ((x - self.origin) / gain).exp()).max(1e-9);
            editor.camera.t0 = self.anchor - self.origin * editor.camera.units_per_px;
            editor.settle_camera();
        }
    }

    pub fn finish(self, editor: &mut Editor) {
        if self.marquee && (self.current - self.origin).abs() > 3.0 {
            let start =
                self.anchor + (self.current.min(self.origin) - self.origin) * self.base_units;
            let span = (self.current - self.origin).abs() * self.base_units;
            editor.camera.units_per_px = (span / editor.viewport.w.max(1.0)).max(1e-9);
            editor.camera.t0 = start;
            editor.settle_camera();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expression_editor_core::{ExpressionDoc, TimeBase, Viewport};

    fn editor() -> Editor {
        let mut editor = Editor::new(
            ExpressionDoc::new(TimeBase::Frames { frame_rate: 100.0 }, 0.0, 10_000.0),
            Viewport::new(1000.0, 400.0),
        );
        editor.camera.units_per_px = 1.0;
        editor.camera.t0 = 1000.0;
        editor
    }

    #[test]
    fn continuous_zoom_keeps_the_press_time_anchored() {
        let mut ed = editor();
        let anchor = ed.camera.t_at(300.0);
        let row_height = ed.camera.vertical.px_per_row;
        let mut drag = TimeZoom::begin(&ed, 300.0, false);
        drag.update(&mut ed, 500.0, false);
        assert!((ed.camera.t_at(300.0) - anchor).abs() < 1e-8);
        assert_eq!(ed.camera.vertical.px_per_row, row_height);
    }

    #[test]
    fn reversed_marquee_frames_the_same_range_and_click_does_not_zoom() {
        let mut a = editor();
        let mut b = editor();
        let mut forward = TimeZoom::begin(&a, 200.0, true);
        forward.update(&mut a, 700.0, false);
        forward.finish(&mut a);
        let mut backward = TimeZoom::begin(&b, 700.0, true);
        backward.update(&mut b, 200.0, false);
        backward.finish(&mut b);
        assert_eq!(a.camera.t0, b.camera.t0);
        assert_eq!(a.camera.units_per_px, b.camera.units_per_px);
        let before = a.camera.units_per_px;
        TimeZoom::begin(&a, 400.0, true).finish(&mut a);
        assert_eq!(a.camera.units_per_px, before);
    }
}
