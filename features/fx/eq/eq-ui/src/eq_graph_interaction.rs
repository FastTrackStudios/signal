//! Coordinate mapping and hit-testing helpers for the EQ graph.

use super::eq_graph_model::{EqBand, EqBandShape};
use fts_ui_audio::axis::{DbAxis, FreqAxis};

#[derive(Clone, Copy, Debug)]
pub struct GraphMapper {
    pub min_freq: f64,
    pub max_freq: f64,
    pub db_range: f64,
    pub width: f64,
    pub height: f64,
    pub padding: f64,
}

impl GraphMapper {
    pub fn new(
        min_freq: f64,
        max_freq: f64,
        db_range: f64,
        width: f64,
        height: f64,
        padding: f64,
    ) -> Self {
        Self {
            min_freq,
            max_freq,
            db_range,
            width,
            height,
            padding,
        }
    }

    pub fn freq_to_x(&self, freq: f64) -> f64 {
        FreqAxis::new(self.min_freq, self.max_freq).freq_to_x(
            freq,
            self.padding,
            self.padding + self.width,
        )
    }

    pub fn x_to_freq(&self, x: f64) -> f64 {
        FreqAxis::new(self.min_freq, self.max_freq).x_to_freq(
            x,
            self.padding,
            self.padding + self.width,
        )
    }

    pub fn db_to_y(&self, db: f64) -> f64 {
        DbAxis::symmetric(self.db_range).db_to_y(db, self.padding, self.padding + self.height)
    }

    pub fn y_to_db(&self, y: f64) -> f64 {
        DbAxis::symmetric(self.db_range).y_to_db(y, self.padding, self.padding + self.height)
    }

    pub fn is_inside(&self, x: f64, y: f64) -> bool {
        x >= self.padding
            && x <= self.padding + self.width
            && y >= self.padding
            && y <= self.padding + self.height
    }
}

pub fn filter_type_for_position(freq: f64, gain: f64, db_range: f64) -> EqBandShape {
    let gain_near_zero = gain.abs() < db_range * 0.2;
    let near_bottom = gain < -db_range * 0.82;

    if near_bottom {
        EqBandShape::Notch
    } else if freq < 30.0 && gain_near_zero {
        EqBandShape::LowCut
    } else if freq > 15000.0 && gain_near_zero {
        EqBandShape::HighCut
    } else if freq < 80.0 {
        EqBandShape::LowShelf
    } else if freq > 8000.0 {
        EqBandShape::HighShelf
    } else {
        EqBandShape::Bell
    }
}

pub fn drag_gain_for_shape(shape: EqBandShape, current_gain: f32, pointer_gain: f64) -> f32 {
    if !shape.uses_gain() {
        0.0
    } else {
        let max_gain = if matches!(
            shape,
            EqBandShape::LowShelf
                | EqBandShape::HighShelf
                | EqBandShape::TiltShelf
                | EqBandShape::FlatTilt
        ) {
            15.0
        } else {
            30.0
        };
        let next = pointer_gain.clamp(-max_gain, max_gain) as f32;
        if shape == EqBandShape::BandPass {
            current_gain
        } else {
            next
        }
    }
}

pub fn wheel_q_for_shape(shape: EqBandShape, q: f32, delta_y: f64, slope_mode: bool) -> f32 {
    if shape.uses_slope() || slope_mode {
        let current = super::eq_graph_model::q_to_slope_db(q);
        let step = if delta_y < 0.0 { 6.0 } else { -6.0 };
        super::eq_graph_model::slope_db_to_q((current + step).clamp(6.0, 96.0))
    } else {
        let q_mul = if delta_y < 0.0 { 1.15 } else { 0.87 };
        (q * q_mul).clamp(0.1, 18.0)
    }
}

pub fn nearest_band(
    bands: &[EqBand],
    mapper: GraphMapper,
    x: f64,
    y: f64,
    radius: f64,
) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (idx, band) in bands.iter().enumerate() {
        if !band.used {
            continue;
        }
        let bx = mapper.freq_to_x(band.frequency as f64);
        let by = mapper.db_to_y(band.gain as f64);
        let d = ((x - bx).powi(2) + (y - by).powi(2)).sqrt();
        if d < radius && (best.is_none() || d < best.unwrap().1) {
            best = Some((idx, d));
        }
    }
    best
}

pub fn bands_in_rect(
    bands: &[EqBand],
    mapper: GraphMapper,
    sx: f64,
    sy: f64,
    x: f64,
    y: f64,
) -> Vec<usize> {
    let (mnx, mxx) = (sx.min(x), sx.max(x));
    let (mny, mxy) = (sy.min(y), sy.max(y));
    bands
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            if !b.used {
                return false;
            }
            let bx = mapper.freq_to_x(b.frequency as f64);
            let by = mapper.db_to_y(b.gain as f64);
            bx >= mnx && bx <= mxx && by >= mny && by <= mxy
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapper() -> GraphMapper {
        GraphMapper::new(20.0, 20_000.0, 24.0, 800.0, 360.0, 0.0)
    }

    #[test]
    fn mapper_round_trips_frequency_and_db() {
        let m = mapper();
        let x = m.freq_to_x(1000.0);
        let freq = m.x_to_freq(x);
        assert!((freq - 1000.0).abs() < 1e-6);

        let y = m.db_to_y(6.0);
        let db = m.y_to_db(y);
        assert!((db - 6.0).abs() < 1e-9);
    }

    #[test]
    fn filter_type_uses_edges_and_gain_position() {
        assert_eq!(
            filter_type_for_position(25.0, 0.0, 24.0),
            EqBandShape::LowCut
        );
        assert_eq!(
            filter_type_for_position(18_000.0, 0.0, 24.0),
            EqBandShape::HighCut
        );
        assert_eq!(
            filter_type_for_position(60.0, 8.0, 24.0),
            EqBandShape::LowShelf
        );
        assert_eq!(
            filter_type_for_position(1000.0, 3.0, 24.0),
            EqBandShape::Bell
        );
        assert_eq!(
            filter_type_for_position(1000.0, -22.0, 24.0),
            EqBandShape::Notch
        );
    }

    #[test]
    fn drag_gain_respects_filter_shape() {
        assert_eq!(drag_gain_for_shape(EqBandShape::LowCut, 3.0, 12.0), 0.0);
        assert_eq!(drag_gain_for_shape(EqBandShape::Notch, -4.0, 9.0), 0.0);
        assert_eq!(drag_gain_for_shape(EqBandShape::LowShelf, 0.0, 24.0), 15.0);
        assert_eq!(drag_gain_for_shape(EqBandShape::Bell, 0.0, 24.0), 24.0);
    }

    #[test]
    fn wheel_q_supports_q_and_slope_modes() {
        assert!(wheel_q_for_shape(EqBandShape::Bell, 1.0, -1.0, false) > 1.0);
        assert_eq!(
            wheel_q_for_shape(EqBandShape::LowCut, 1.0, -1.0, false),
            1.5
        );
        assert_eq!(wheel_q_for_shape(EqBandShape::Bell, 1.0, -1.0, true), 1.5);
    }

    #[test]
    fn nearest_band_and_rect_selection_ignore_unused_bands() {
        let m = mapper();
        let bands = vec![
            EqBand {
                used: true,
                enabled: true,
                frequency: 1000.0,
                gain: 0.0,
                ..Default::default()
            },
            EqBand {
                used: false,
                enabled: true,
                frequency: 1100.0,
                gain: 0.0,
                ..Default::default()
            },
        ];
        let x = m.freq_to_x(1000.0);
        let y = m.db_to_y(0.0);
        assert_eq!(nearest_band(&bands, m, x, y, 10.0).map(|(i, _)| i), Some(0));
        assert_eq!(
            bands_in_rect(&bands, m, x - 5.0, y - 5.0, x + 5.0, y + 5.0),
            vec![0]
        );
    }
}
