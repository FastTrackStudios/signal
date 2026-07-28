//! Melodyne-style pitch editor surface (piano-roll note blobs).
//!
//! A dumb, wasm-clean view over the offline pitch document
//! (`tune_dsp::model::PitchDoc` on the host side): the host converts
//! blobs to [`BlobView`]s and applies [`BlobEdit`]s back to the doc +
//! render pipeline. Interaction follows the eq-surface idiom (drag
//! shield, element-coordinate mapping), styling is inline-safe for the
//! eventual Blitz native build.
//!
//! Deliberately lean — the DSP carries the product; this surface is
//! drag-to-transpose, selection, and the drift/vibrato/robot controls.

use dioxus::prelude::*;

/// One note blob as the view sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct BlobView {
    /// Analysis frame range (x axis).
    pub start_frame: usize,
    pub end_frame: usize,
    /// Edited pitch center in MIDI.
    pub center_midi: f64,
    /// Downsampled edited contour: (frame, midi) points.
    pub curve: Vec<(usize, f64)>,
    /// 0..1 — display weight (from analysis RMS).
    pub weight: f64,
    pub drift_amount: f64,
    pub modulation_amount: f64,
}

/// Edits the surface emits; the host applies them to the PitchDoc.
#[derive(Clone, Debug, PartialEq)]
pub enum BlobEdit {
    /// Set the blob's pitch center (already snapped if snapping is on).
    SetCenter { index: usize, center_midi: f64 },
    SetDriftAmount { index: usize, amount: f64 },
    SetModulationAmount { index: usize, amount: f64 },
}

const H: f64 = 480.0;

/// Frames→x and midi→y mapping over the visible ranges.
#[derive(Clone, Copy, PartialEq)]
struct ViewMap {
    frame_lo: f64,
    frame_hi: f64,
    midi_lo: f64,
    midi_hi: f64,
    w: f64,
}

impl ViewMap {
    fn x(&self, frame: f64) -> f64 {
        (frame - self.frame_lo) / (self.frame_hi - self.frame_lo).max(1.0) * self.w
    }
    fn y(&self, midi: f64) -> f64 {
        (1.0 - (midi - self.midi_lo) / (self.midi_hi - self.midi_lo).max(0.1)) * H
    }
    fn dy_to_midi(&self, dy_px: f64, h_px: f64) -> f64 {
        -dy_px / h_px.max(1.0) * (self.midi_hi - self.midi_lo)
    }
}

fn is_black_key(midi: i32) -> bool {
    matches!(midi.rem_euclid(12), 1 | 3 | 6 | 8 | 10)
}

/// The editor. `snap` quantizes drags to semitones (drag emits
/// `SetCenter` continuously; the host may re-snap to a scale).
#[component]
pub fn PitchEditor(
    blobs: Vec<BlobView>,
    on_edit: EventHandler<BlobEdit>,
    #[props(default)] snap: bool,
) -> Element {
    let mut selected = use_signal(|| None::<usize>);
    // Drag: (blob index, start client y, original center, element h).
    let mut dragging = use_signal(|| None::<(usize, f64, f64, f64)>);
    let mut root_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut vb_w = use_signal(|| 960.0f64);

    // Visible ranges from the data (padded).
    let (f_lo, f_hi, m_lo, m_hi) = {
        let mut f_lo = usize::MAX;
        let mut f_hi = 0usize;
        let mut m_lo = f64::MAX;
        let mut m_hi = f64::MIN;
        for b in &blobs {
            f_lo = f_lo.min(b.start_frame);
            f_hi = f_hi.max(b.end_frame);
            for &(_, m) in &b.curve {
                m_lo = m_lo.min(m);
                m_hi = m_hi.max(m);
            }
        }
        if f_lo > f_hi {
            (0.0, 100.0, 48.0, 72.0)
        } else {
            (
                f_lo as f64,
                (f_hi as f64).max(f_lo as f64 + 1.0),
                (m_lo - 4.0).floor(),
                (m_hi + 4.0).ceil(),
            )
        }
    };
    let map = ViewMap {
        frame_lo: f_lo,
        frame_hi: f_hi,
        midi_lo: m_lo,
        midi_hi: m_hi,
        w: vb_w(),
    };

    let sel = selected().filter(|&i| i < blobs.len());
    let sel_blob = sel.and_then(|i| blobs.get(i).cloned());

    rsx! {
        div {
            style: "position: relative; display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0; background: #101014;",
            svg {
                style: "width: 100%; flex: 1; min-height: 0; touch-action: none; user-select: none;",
                view_box: "0 0 {map.w:.0} {H:.0}",
                preserve_aspect_ratio: "none",
                onmounted: move |e| {
                    let data = e.data();
                    root_el.set(Some(data.clone()));
                    spawn(async move {
                        if let Ok(rect) = data.get_client_rect().await {
                            if rect.height() > 1.0 {
                                vb_w.set((H * rect.width() / rect.height()).max(240.0));
                            }
                        }
                    });
                },
                onpointerdown: move |_| selected.set(None),

                // Piano-roll rows.
                for midi in (m_lo as i32)..=(m_hi as i32) {
                    rect {
                        key: "r{midi}",
                        x: "0",
                        y: "{map.y(midi as f64 + 0.5):.1}",
                        width: "{map.w:.0}",
                        height: "{(H / (m_hi - m_lo)).abs():.2}",
                        fill: if is_black_key(midi) { "#16161c" } else { "#1c1c24" },
                    }
                }
                // Octave lines (C).
                for midi in (m_lo as i32)..=(m_hi as i32) {
                    if midi.rem_euclid(12) == 0 {
                        line {
                            key: "c{midi}",
                            x1: "0",
                            y1: "{map.y(midi as f64 - 0.5):.1}",
                            x2: "{map.w:.0}",
                            y2: "{map.y(midi as f64 - 0.5):.1}",
                            stroke: "#2e2e3a",
                            stroke_width: "1",
                        }
                    }
                }

                // Blobs.
                for (i, b) in blobs.iter().enumerate() {
                    {
                        let is_sel = sel == Some(i);
                        let x0 = map.x(b.start_frame as f64);
                        let x1 = map.x(b.end_frame as f64);
                        let yc = map.y(b.center_midi);
                        let row_h = (H / (m_hi - m_lo)).abs();
                        let poly: String = b
                            .curve
                            .iter()
                            .map(|&(f, m)| format!("{:.1},{:.1} ", map.x(f as f64), map.y(m)))
                            .collect();
                        let alpha = 0.35 + 0.4 * b.weight.clamp(0.0, 1.0);
                        let center0 = b.center_midi;
                        rsx! {
                            // Blob body: a rounded bar one semitone tall
                            // at the center pitch.
                            rect {
                                key: "b{i}",
                                x: "{x0:.1}",
                                y: "{yc - row_h * 0.5:.1}",
                                width: "{(x1 - x0).max(2.0):.1}",
                                height: "{row_h:.1}",
                                rx: "{row_h * 0.35:.1}",
                                fill: if is_sel { "#7dd3fc" } else { "#38bdf8" },
                                fill_opacity: "{alpha:.2}",
                                stroke: if is_sel { "#e0f2fe" } else { "#38bdf8" },
                                stroke_width: if is_sel { "2" } else { "1" },
                                style: "cursor: grab;",
                                onpointerdown: {
                                    move |e: PointerEvent| {
                                        e.stop_propagation();
                                        let c = e.client_coordinates();
                                        let el = root_el();
                                        let center = center0;
                                        spawn(async move {
                                            let h_px = if let Some(el) = el {
                                                el.get_client_rect()
                                                    .await
                                                    .map(|r| r.height())
                                                    .unwrap_or(H)
                                            } else {
                                                H
                                            };
                                            selected.set(Some(i));
                                            dragging.set(Some((i, c.y, center, h_px)));
                                        });
                                    }
                                },
                            }
                            // The edited pitch contour through the blob.
                            polyline {
                                points: "{poly}",
                                fill: "none",
                                stroke: if is_sel { "#f0f9ff" } else { "#bae6fd" },
                                stroke_width: "1.5",
                                stroke_opacity: "0.9",
                                pointer_events: "none",
                            }
                        }
                    }
                }
            }

            // Drag shield: vertical drag = transpose.
            if dragging().is_some() {
                div {
                    style: "position: fixed; inset: 0; z-index: 1000; cursor: grabbing;",
                    onpointermove: {
                        let on_edit = on_edit;
                        move |e: PointerEvent| {
                            let Some((i, y0, center0, h_px)) = dragging() else { return };
                            let dy = e.client_coordinates().y - y0;
                            let mut center = center0 + map.dy_to_midi(dy, h_px);
                            if snap {
                                center = center.round();
                            }
                            on_edit.call(BlobEdit::SetCenter { index: i, center_midi: center });
                        }
                    },
                    onpointerup: move |_| dragging.set(None),
                }
            }

            // Selection rail: drift / vibrato / robot.
            if let Some(b) = sel_blob {
                div {
                    style: "position: absolute; bottom: 6px; left: 6px; z-index: 10; display: flex; align-items: center; gap: 10px; background: #18181fee; border: 1px solid #2e2e3a; border-radius: 6px; padding: 6px 10px; color: #cbd5e1; font-size: 11px;",
                    span { "note {b.center_midi:.2}" }
                    label {
                        style: "display: flex; align-items: center; gap: 4px;",
                        "drift"
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            value: "{(b.drift_amount * 100.0) as i32}",
                            style: "width: 90px;",
                            oninput: {
                                let on_edit = on_edit;
                                move |e: FormEvent| {
                                    if let (Some(i), Ok(v)) = (sel, e.value().parse::<f64>()) {
                                        on_edit.call(BlobEdit::SetDriftAmount {
                                            index: i,
                                            amount: v / 100.0,
                                        });
                                    }
                                }
                            },
                        }
                    }
                    label {
                        style: "display: flex; align-items: center; gap: 4px;",
                        "vibrato"
                        input {
                            r#type: "range",
                            min: "0",
                            max: "150",
                            value: "{(b.modulation_amount * 100.0) as i32}",
                            style: "width: 90px;",
                            oninput: {
                                let on_edit = on_edit;
                                move |e: FormEvent| {
                                    if let (Some(i), Ok(v)) = (sel, e.value().parse::<f64>()) {
                                        on_edit.call(BlobEdit::SetModulationAmount {
                                            index: i,
                                            amount: v / 100.0,
                                        });
                                    }
                                }
                            },
                        }
                    }
                    button {
                        style: "background: #27272f; border: 1px solid #3f3f4a; border-radius: 4px; color: #e2e8f0; padding: 2px 8px; cursor: pointer;",
                        onclick: {
                            let on_edit = on_edit;
                            move |_| {
                                if let Some(i) = sel {
                                    on_edit.call(BlobEdit::SetDriftAmount { index: i, amount: 0.0 });
                                    on_edit.call(BlobEdit::SetModulationAmount {
                                        index: i,
                                        amount: 0.0,
                                    });
                                }
                            }
                        },
                        "Robot"
                    }
                }
            }
        }
    }
}
