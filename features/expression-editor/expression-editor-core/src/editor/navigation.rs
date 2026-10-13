//! Editor navigation behavior.
use super::*;

impl Editor {
    // ── the edit cursor, and what follows it ─────────────────────────
    //
    // `hjkl` as the FTS REAPER profile defines it
    // (`reaper-input/config/.../navigation.styx` and `midi.styx`): `h`
    // and `l` walk the edit cursor, `j` and `k` change track. The
    // modifiers stack meanings on the horizontal pair — Ctrl for a grid
    // step instead of a measure, Ctrl+Shift to drag a time selection
    // along behind it.
    //
    // Note that `h`/`l` are *not* note movement in that profile, and
    // deliberately: the arrows are. Moving the cursor is what you do
    // constantly and moving notes is what you do on purpose.

    /// Where the edit cursor is, or the left edge of the view.
    ///
    /// The cursor is `Option` because a document does not necessarily
    /// have one; every command that walks it has to start somewhere, and
    /// what is on screen is the least surprising place.
    pub fn cursor(&self) -> f64 {
        self.playhead.unwrap_or_else(|| self.camera.t_at(0.0))
    }

    /// Move the edit cursor by `delta`, clamped to the document.
    pub fn move_cursor(&mut self, delta: f64) -> bool {
        let to = (self.cursor() + delta).clamp(self.doc.start, self.doc.end);
        let moved = self.playhead != Some(to);
        self.playhead = Some(to);
        moved
    }

    /// Move the cursor and drag the time selection with it.
    ///
    /// Anchored on where the selection already starts, so pressing the
    /// key repeatedly grows one range rather than making a new one each
    /// time — and reversing direction shrinks it back rather than
    /// flipping to a fresh range on the other side.
    pub fn move_cursor_extending(&mut self, delta: f64) -> bool {
        let from = self.time_selection.map(|(a, _)| a).unwrap_or(self.cursor());
        if !self.move_cursor(delta) {
            return false;
        }
        let to = self.cursor();
        self.time_selection = Some(if from <= to { (from, to) } else { (to, from) });
        true
    }

    /// One measure, in document units — what `h` and `l` step by.
    pub fn measure(&self) -> f64 {
        self.units_per_bar()
    }

    /// The grid step, or a beat when the grid is free.
    pub fn grid_step(&self) -> f64 {
        let step = self.grid.step(self.units_per_beat());
        if step > 0.0 {
            step
        } else {
            self.units_per_beat()
        }
    }

    pub fn clear_time_selection(&mut self) -> bool {
        self.time_selection.take().is_some()
    }

    /// Lengthen or shorten the selected notes by `delta`.
    ///
    /// Never past nothing: a note dragged shorter than zero would read
    /// as one that starts after it ends, and the shortening key is held
    /// down as readily as any other.
    pub fn nudge_note_lengths(&mut self, delta: f64) -> bool {
        let notes = self.selection.notes.clone();
        if notes.is_empty() {
            return false;
        }
        let floor = self.grid_step() * 0.25;
        self.begin_gesture();
        let mut ok = false;
        for id in notes {
            let Some(n) = self.doc.note(id) else { continue };
            let (start, end) = (n.start, n.end);
            let next = (end + delta).max(start + floor);
            if (next - end).abs() > f64::EPSILON {
                ok |= self.apply_live(&Edit::Resize {
                    note: id,
                    start,
                    end: next,
                });
            }
        }
        ok
    }

    pub fn content(&self) -> Content {
        content_of(&self.doc)
    }

    /// The Reset View camera for the current content.
    pub fn reset_camera(&self) -> Camera {
        camera::reset_view(
            self.content(),
            self.viewport,
            CUSHION,
            PAD,
            self.camera.fold,
        )
    }

    /// `V` — snap directly to Reset View, no interpolation, no magnets.
    pub fn reset_view(&mut self) {
        self.camera = self.reset_camera();
        // Reset View is the one gesture that re-fits lanes. Everything
        // else leaves them exactly where they are, including edits that
        // push content out of view.
        self.fit_lanes();
    }

    /// Contextual zoom and scroll — MeMagic, applied to this document.
    ///
    /// One entry point for every region, so a host binds a single action
    /// and the region decides what it means. See [`memagic`] for the
    /// design and where it comes from.
    ///
    /// Returns whether anything moved, so a caller can fall through to
    /// another binding when the gesture had nothing to say (a
    /// `ScrollToAnchor` with no row under the pointer, say).
    pub fn memagic(&mut self, region: memagic::Region, anchor: memagic::Anchor) -> bool {
        self.memagic_with(region.modes(), anchor, &memagic::Config::default())
    }

    /// The same, with the mode pair and tuning spelled out.
    pub fn memagic_with(
        &mut self,
        modes: memagic::Modes,
        anchor: memagic::Anchor,
        cfg: &memagic::Config,
    ) -> bool {
        let content = self.content();
        let upb = self.units_per_bar();
        let view = self.camera.time_span(self.viewport);
        let mut moved = false;

        // Horizontal first: the vertical `InView` scope reads the time
        // window, so it has to see the one the gesture is producing
        // rather than the one it is replacing.
        if let Some((t0, len)) =
            memagic::horizontal_span(&self.doc, content, modes.horizontal, anchor, upb, view, cfg)
        {
            self.camera.t0 = t0;
            self.camera.units_per_px = (len / self.viewport.w.max(1.0)).max(1e-9);
            moved = true;
        }

        let view = self.camera.time_span(self.viewport);
        if let Some((lo, hi)) =
            memagic::vertical_range(&self.doc, content, modes.vertical, anchor, view, cfg)
        {
            let rows = (hi - lo + 1.0).max(1.0);
            self.camera.vertical.center = (lo + hi) / 2.0;
            // The row-height ceiling is what stops a two-note passage
            // filling the lane with two enormous rows.
            self.camera.vertical.px_per_row =
                (self.viewport.h / rows).min(cfg.max_px_per_row).max(1e-6);
            moved = true;
        }

        if moved {
            self.settle_camera();
        }
        moved
    }

    pub fn bounds(&self) -> Bounds {
        let c = self.content();
        let span = (c.t_end - c.t_start).max(1.0);
        // The row range comes from the mode's row space, not from a
        // constant: a drum map is twenty lanes and a pitch roll is 128,
        // and fitting the roll to "128" in drums mode would leave a
        // screen of empty rows under the kit.
        let (row_min, row_max) = self.doc.row_space.bounds();
        Bounds {
            t_min: c.t_start - span * CUSHION,
            t_max: c.t_end + span * CUSHION,
            row_min: row_min as f64,
            row_max: row_max as f64,
            ..Bounds::default()
        }
    }

    /// Zoom in around the pointer, with the edge magnet applied in the
    /// same pass — never as a second mutation.
    pub fn zoom_in_at(&mut self, mouse_x: f64, mouse_y: f64, factor: f64) {
        let content = self.content();
        let anchor_t = self.camera.t_at(mouse_x);
        let anchor_pitch = self.camera.pitch_at(mouse_y, self.viewport);

        let mut base = self.camera;
        base.zoom_time_about(anchor_t, factor);
        base.zoom_pitch_about(anchor_pitch, factor, self.viewport);

        let mut influences = Vec::new();
        if let Some(edge) = camera::edge_magnet(
            base,
            anchor_t,
            content,
            self.viewport,
            EDGE_DEAD_ZONE,
            EDGE_WHITESPACE,
        ) {
            influences.push(edge);
        }
        influences.extend(camera::pitch_focus(
            base,
            self.local_pitch(anchor_t),
            anchor_pitch,
            LOCAL_PITCH_WEIGHT,
            MOUSE_PITCH_WEIGHT,
        ));
        if let Some(deep) = camera::deep_zoom_center(
            base,
            content,
            DEEP_ZOOM_ONSET,
            Bounds::default().max_px_per_semitone,
        ) {
            influences.push(deep);
        }

        self.camera = camera::blend(base, &influences);
        self.settle_camera();
    }

    /// Zoom out around the pointer. The reset magnet engages only in
    /// the final stretch — early engagement is what makes a zoom-out
    /// feel like it is being taken away from you.
    pub fn zoom_out_at(&mut self, mouse_x: f64, mouse_y: f64, factor: f64) {
        let anchor_t = self.camera.t_at(mouse_x);
        let anchor_pitch = self.camera.pitch_at(mouse_y, self.viewport);

        let mut base = self.camera;
        base.zoom_time_about(anchor_t, 1.0 / factor.max(1e-6));
        base.zoom_pitch_about(anchor_pitch, 1.0 / factor.max(1e-6), self.viewport);

        let reset = self.reset_camera();
        let mut influences = Vec::new();
        influences.extend(camera::pitch_focus(
            base,
            self.local_pitch(anchor_t),
            anchor_pitch,
            LOCAL_PITCH_WEIGHT,
            MOUSE_PITCH_WEIGHT,
        ));
        if let Some(tail) = camera::reset_tail(base, reset, RESET_TAIL_START) {
            influences.push(tail);
        }

        self.camera = camera::blend(base, &influences);
        self.settle_camera();
    }

    /// Zoom the time axis alone, about the pointer.
    ///
    /// Deliberately without the magnets [`zoom_in_at`](Self::zoom_in_at)
    /// blends in. Those aim the camera on *both* axes — edge, local pitch,
    /// deep-zoom centre — and a gesture the user asked to move one axis
    /// must not quietly move the other. The magnets belong to the
    /// both-axes zoom, where there is no such promise to keep.
    pub fn zoom_time_at(&mut self, mouse_x: f64, factor: f64) {
        let anchor_t = self.camera.t_at(mouse_x);
        self.camera.zoom_time_about(anchor_t, factor);
        self.settle_camera();
    }

    /// Zoom the pitch axis alone, about the pointer. See
    /// [`zoom_time_at`](Self::zoom_time_at) for why the magnets are absent.
    pub fn zoom_pitch_at(&mut self, mouse_y: f64, factor: f64) {
        let anchor_pitch = self.camera.pitch_at(mouse_y, self.viewport);
        self.camera
            .zoom_pitch_about(anchor_pitch, factor, self.viewport);
        self.settle_camera();
    }

    /// Settle the camera after a move, and let the grid follow it.
    ///
    /// Every camera change ends here — there were eight `constrain`
    /// calls and now there is one place they all go, which is what makes
    /// "the grid follows the zoom" true rather than true in the seven
    /// paths somebody remembered.
    ///
    /// The grid part is a no-op unless the user has asked for an
    /// adaptive density, and usually a no-op even then: a division only
    /// moves when the zoom crosses a power of two.
    ///
    /// **Public, and the required ending for any direct camera write.**
    /// It was private, which made the promise above impossible to keep:
    /// the interactive zooms live in `expression-editor-ui`, where they
    /// set `camera.units_per_px` by hand, and the one function that would
    /// have refitted the grid afterwards was not reachable from there. So
    /// the grid followed the wheel and not the zoom *tool* — which, since
    /// `z` became the tool, is most zooming. Anything that assigns to
    /// `camera` ends here.
    pub fn settle_camera(&mut self) {
        self.camera.constrain(self.bounds(), self.viewport);
        let bar_px = self.units_per_bar() / self.camera.units_per_px;
        self.grid.refit(bar_px);
    }

    /// Move the grid's ceiling, and refit to the view at once.
    ///
    /// Through the editor rather than on `Grid` directly, because
    /// refitting needs the camera and the tempo map and `Grid` has
    /// neither. A caller that reached past these would leave the readout
    /// showing a division that is not the one being snapped to.
    pub fn grid_coarser(&mut self) {
        self.grid.coarser();
        self.settle_camera();
    }

    /// Set the division outright, and refit to the view at once.
    ///
    /// The *ceiling*, like every other grid control: an adaptive grid
    /// may still show something coarser, which is what the readout says.
    pub fn set_grid_division(&mut self, division: f64) {
        self.grid.set_division(division);
        self.settle_camera();
    }

    /// Triplet and dotted, which clear each other.
    pub fn set_grid_triplet(&mut self, on: bool) {
        self.grid.set_triplet(on);
        self.settle_camera();
    }

    pub fn set_grid_dotted(&mut self, on: bool) {
        self.grid.set_dotted(on);
        self.settle_camera();
    }

    pub fn grid_finer(&mut self) {
        self.grid.finer();
        self.settle_camera();
    }

    /// How tightly the grid packs its lines, or [`adaptive_grid::Density::Fixed`] to
    /// stop it following the zoom at all.
    pub fn set_grid_density(&mut self, density: adaptive_grid::Density) {
        self.grid.adaptive.density = density;
        self.settle_camera();
    }

    /// Frame a box of the document: `t0..t1` across, `row_lo..row_hi` down.
    ///
    /// What the zoom tool's Alt-sweep lands on, and the honest primitive
    /// behind "zoom to this". Written as a single assignment of the
    /// camera rather than a sequence of zoom steps, because a sequence
    /// has to decide an order and either order leaves the other axis
    /// anchored on the wrong thing.
    ///
    /// Degenerate boxes are refused rather than clamped: a zero-width
    /// sweep is a click that moved a pixel, and framing it would zoom to
    /// the maximum and lose the user's place for what looked like a
    /// misclick.
    pub fn zoom_to_box(&mut self, t0: f64, t1: f64, row_lo: f64, row_hi: f64) {
        let (t0, t1) = (t0.min(t1), t0.max(t1));
        let (lo, hi) = (row_lo.min(row_hi), row_lo.max(row_hi));
        let span_t = t1 - t0;
        let span_rows = hi - lo;
        if !(span_t.is_finite() && span_t > 0.0 && span_rows.is_finite() && span_rows > 0.0) {
            return;
        }
        self.camera.units_per_px = (span_t / self.viewport.w.max(1.0)).max(1e-9);
        self.camera.t0 = t0;
        self.camera.vertical.px_per_row = (self.viewport.h.max(1.0) / span_rows).max(1e-6);
        self.camera.vertical.center = (lo + hi) * 0.5;
        self.settle_camera();
    }

    /// Move the view `steps` pages of `bars_per_page` bars, keeping the
    /// zoom, and land on a bar line.
    ///
    /// Editing drums is done a phrase at a time: zoom to four bars, fix
    /// them, move on. Paging by a fixed number of *seconds* would drift
    /// out of phase with the music within a few pages and put the
    /// downbeat somewhere different every time — the thing you navigate
    /// by would be the thing that moves. So the view snaps to the bar
    /// line nearest where it already is, then counts bars from there.
    ///
    /// Returns whether the view moved. `false` when the host supplied no
    /// bar lines, or the page would run off either end of the take —
    /// paging past the last bar and landing on emptiness is worse than
    /// not moving, because it looks like the editor lost the project.
    // r[impl drums.view.page-bars]
    pub fn page_bars(&mut self, bars_per_page: usize, steps: i64) -> bool {
        let bars = self.doc.bars.clone();
        if bars.len() < 2 || bars_per_page == 0 || steps == 0 {
            return false;
        }
        let (t0, t1) = self.camera.time_span(self.viewport);
        let span = t1 - t0;
        if !(span.is_finite() && span > 0.0) {
            return false;
        }
        // Where the view starts now, as a bar index: the nearest bar
        // line, so a view nudged slightly off the grid re-aligns rather
        // than carrying its error forward through every page.
        let here = bars
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (*a - t0).abs().total_cmp(&(*b - t0).abs()))
            .map_or(0, |(i, _)| i);

        let target = here as i64 + steps * bars_per_page as i64;
        // The last page starts at the last bar that still has a full
        // page behind it, so paging forward at the end stops rather than
        // scrolling off into nothing.
        let last_start = (bars.len() - 1).saturating_sub(bars_per_page);
        let target = target.clamp(0, last_start as i64) as usize;
        if target == here {
            return false;
        }
        let start = bars[target];
        // Keep the zoom: the page is as wide as the view already was,
        // so paging never silently changes how much is on screen.
        let (lo, hi) = self.camera.pitch_span(self.viewport);
        self.zoom_to_box(start, start + span, lo, hi);
        true
    }

    /// Frame exactly `bars_per_page` bars starting at the bar nearest
    /// the view's left edge — what a "zoom to four bars" key does.
    // r[impl drums.view.page-bars]
    pub fn frame_bars(&mut self, bars_per_page: usize) -> bool {
        let bars = self.doc.bars.clone();
        if bars.len() < 2 || bars_per_page == 0 {
            return false;
        }
        let (t0, _) = self.camera.time_span(self.viewport);
        let here = bars
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (*a - t0).abs().total_cmp(&(*b - t0).abs()))
            .map_or(0, |(i, _)| i);
        let end = (here + bars_per_page).min(bars.len() - 1);
        if end <= here {
            return false;
        }
        let (lo, hi) = self.camera.pitch_span(self.viewport);
        self.zoom_to_box(bars[here], bars[end], lo, hi);
        true
    }

    pub fn pan_px(&mut self, dx: f64, dy: f64) {
        self.camera.pan_px(dx, dy);
        self.settle_camera();
    }

    pub fn resize(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.settle_camera();
    }

    /// Weighted pitch center of notes near `t` — what the vertical
    /// magnet aims at.
    pub fn local_pitch(&self, t: f64) -> Option<f64> {
        let window = self.viewport.w * self.camera.units_per_px * 0.25;
        let mut sum = 0.0;
        let mut weight = 0.0;
        for n in &self.doc.notes {
            let distance = if t < n.start {
                n.start - t
            } else if t > n.end {
                t - n.end
            } else {
                0.0
            };
            if distance > window {
                continue;
            }
            let w = (1.0 - distance / window.max(1e-9)) * n.weight.max(0.05);
            sum += n.row as f64 * w;
            weight += w;
        }
        (weight > 1e-9).then(|| sum / weight)
    }

    /// Document units per beat at the current tempo.
    pub fn units_per_beat(&self) -> f64 {
        self.doc.time_base.units_per_beat(self.bpm)
    }

    /// Document units per bar.
    pub fn units_per_bar(&self) -> f64 {
        self.units_per_beat() * self.beats_per_bar.max(1.0)
    }

    /// `(bar, beat)` at `t`, both 1-based — what the ruler prints.
    pub fn bar_beat(&self, t: f64) -> (i64, i64) {
        let beats = (t - self.doc.start) / self.units_per_beat();
        let bpb = self.beats_per_bar.max(1.0);
        let bar = (beats / bpb).floor();
        (bar as i64 + 1, (beats - bar * bpb).floor() as i64 + 1)
    }

    /// Snap a time to the local grid.
    pub fn snap_time(&self, t: f64) -> f64 {
        self.grid.snap(t, self.doc.start, self.units_per_beat())
    }

    /// Notes reduced to what zoom cares about.
    pub fn zoom_spans(&self) -> Vec<zoom::Span> {
        self.doc
            .notes
            .iter()
            .map(|n| zoom::Span {
                start: n.start,
                end: n.end,
                row: n.row,
            })
            .collect()
    }

    /// Contextual zoom: one gesture, and where the pointer is decides
    /// what "zoom" means. See [`zoom`].
    pub fn smart_zoom(&mut self, modes: ZoomModes, anchor_t: f64, anchor_row: f64) {
        let spans = self.zoom_spans();
        let content = self.content();
        let bar = self.units_per_bar();
        self.camera = zoom::apply_horizontal(
            self.camera,
            modes.horizontal,
            &spans,
            anchor_t,
            content,
            self.viewport,
            bar,
            self.smart_zoom,
        );
        // Vertical runs against the *new* horizontal span, so
        // "notes in view" means notes in the view we just produced —
        // not the one we started from.
        let view = self.camera.time_span(self.viewport);
        self.camera = zoom::apply_vertical(
            self.camera,
            modes.vertical,
            &spans,
            anchor_row,
            self.viewport,
            view,
            self.smart_zoom,
        );
        self.settle_camera();
    }
}
