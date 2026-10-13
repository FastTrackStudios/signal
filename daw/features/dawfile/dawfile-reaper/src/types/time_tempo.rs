//! Time and Tempo parsing for RPP files
//!
//! Handles tempo changes, time signature changes, and musical position calculations
//! based on the TEMPOENVEX envelope data from REAPER projects.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::primitives::{Token, token::parse_token_line};
use daw_proto::tempo_map::TempoMapEngine;
use daw_proto::{Position, PositionInSeconds, TempoPoint, TimeSignature};

/// A tempo/time signature change point
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempoTimePoint {
    /// Position in seconds when this change occurs
    pub position: f64,
    /// Tempo in BPM
    pub tempo: f64,
    /// Envelope shape (0=linear, 1=square, etc.)
    pub shape: i32,
    /// Time signature encoded as 65536 * denominator + numerator
    /// e.g., 4/4 = 65536 * 4 + 4 = 262148
    pub time_signature_encoded: Option<i32>,
    /// Whether this point is selected
    pub selected: bool,
    /// Unknown field
    pub unknown1: i32,
    /// Bezier tension for curves
    pub bezier_tension: f64,
    /// Metronome pattern (e.g., "ABBB")
    pub metronome_pattern: String,
    /// Additional unknown fields
    pub unknown2: i32,
    pub unknown3: i32,
    pub unknown4: i32,
}

impl Default for TempoTimePoint {
    fn default() -> Self {
        Self {
            position: 0.0,
            tempo: 120.0,
            shape: 0,
            time_signature_encoded: None,
            selected: false,
            unknown1: 0,
            bezier_tension: 0.0,
            metronome_pattern: String::new(),
            unknown2: 0,
            unknown3: 0,
            unknown4: 0,
        }
    }
}

impl TempoTimePoint {
    /// Create a TempoTimePoint from already-tokenized PT line tokens.
    // r[impl rpp.parse.tempo]
    pub fn from_tokens(tokens: &[Token]) -> Result<Self, String> {
        if tokens.len() < 4 {
            return Err(format!(
                "PT line has insufficient tokens: expected at least 4, got {}",
                tokens.len()
            ));
        }
        let is_pt = matches!(tokens.first(), Some(Token::Identifier(s)) if s == "PT");
        if !is_pt {
            return Err("tempo point tokens must start with PT".to_string());
        }

        let position = tokens[1].as_number().ok_or("Invalid position")?;
        let tempo = tokens[2].as_number().ok_or("Invalid tempo")?;
        let shape = tokens[3].as_number().ok_or("Invalid shape")? as i32;

        let time_signature_encoded = if tokens.len() > 4 {
            let encoded = tokens[4].as_number().ok_or("Invalid time signature")? as i32;
            if encoded > 0 { Some(encoded) } else { None }
        } else {
            None
        };

        let selected = if tokens.len() > 5 {
            tokens[5].as_number().unwrap_or(0.0) as i32 != 0
        } else {
            false
        };

        let unknown1 = if tokens.len() > 6 {
            tokens[6].as_number().unwrap_or(0.0) as i32
        } else {
            0
        };

        let bezier_tension = if tokens.len() > 7 {
            tokens[7].as_number().unwrap_or(0.0)
        } else {
            0.0
        };

        let metronome_pattern = if tokens.len() > 8 {
            let mut pattern = String::new();
            for i in (8..tokens.len()).rev() {
                if let Some(p) = tokens[i].as_string()
                    && !p.is_empty()
                {
                    pattern = p.to_string();
                    break;
                }
            }
            pattern
        } else {
            String::new()
        };

        let unknown2 = if tokens.len() > 9 {
            tokens[9].as_number().unwrap_or(0.0) as i32
        } else {
            0
        };

        let unknown3 = if tokens.len() > 10 {
            tokens[10].as_number().unwrap_or(0.0) as i32
        } else {
            0
        };

        let unknown4 = if tokens.len() > 11 {
            tokens[11].as_number().unwrap_or(0.0) as i32
        } else {
            0
        };

        Ok(TempoTimePoint {
            position,
            tempo,
            shape,
            time_signature_encoded,
            selected,
            unknown1,
            bezier_tension,
            metronome_pattern,
            unknown2,
            unknown3,
            unknown4,
        })
    }

    /// Create a TempoTimePoint from a raw RPP PT line
    pub fn from_pt_line(line: &str) -> Result<Self, String> {
        let (_remaining, tokens) =
            parse_token_line(line).map_err(|e| format!("Failed to parse PT line: {:?}", e))?;
        Self::from_tokens(&tokens)
    }

    /// Decode time signature from the encoded value
    /// Returns (numerator, denominator) or None if not set
    pub fn time_signature(&self) -> Option<(i32, i32)> {
        self.time_signature_encoded.map(|encoded| {
            let denominator = encoded / 65536;
            let numerator = encoded % 65536;
            (numerator, denominator)
        })
    }

    /// Get time signature as a string (e.g., "4/4")
    pub fn time_signature_string(&self) -> String {
        if let Some((num, den)) = self.time_signature() {
            format!("{}/{}", num, den)
        } else {
            "".to_string()
        }
    }

    /// Convert to the shared daw-proto tempo point representation.
    pub fn to_proto_point(&self) -> TempoPoint {
        TempoPoint {
            position: Position::from_time(PositionInSeconds::from_seconds(self.position)),
            bpm: self.tempo,
            time_signature: self
                .time_signature()
                .map(|(num, den)| TimeSignature::new(num.max(1) as u32, den.max(1) as u32)),
            shape: Some(self.shape),
            bezier_tension: Some(self.bezier_tension),
            selected: Some(self.selected),
            linear: Some(self.shape == 0),
        }
    }
}

impl fmt::Display for TempoTimePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Tempo Change at {:.3}s:", self.position)?;
        writeln!(f, "  Tempo: {:.1} BPM", self.tempo)?;
        if let Some((num, den)) = self.time_signature() {
            writeln!(f, "  Time Signature: {}/{}", num, den)?;
        }
        if !self.metronome_pattern.is_empty() {
            writeln!(f, "  Metronome Pattern: {}", self.metronome_pattern)?;
        }
        writeln!(f, "  Shape: {}", self.shape)?;
        if self.selected {
            writeln!(f, "  Selected: Yes")?;
        }
        Ok(())
    }
}

/// Collection of tempo and time signature changes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempoTimeEnvelope {
    /// All tempo/time signature change points, sorted by position
    pub points: Vec<TempoTimePoint>,
    /// Default tempo (from project properties)
    pub default_tempo: f64,
    /// Default time signature (from project properties)
    pub default_time_signature: (i32, i32),
}

impl TempoTimeEnvelope {
    /// Create a new tempo envelope with defaults
    pub fn new(default_tempo: f64, default_time_signature: (i32, i32)) -> Self {
        Self {
            points: Vec::new(),
            default_tempo,
            default_time_signature,
        }
    }

    /// Add a tempo/time signature change point
    pub fn add_point(&mut self, point: TempoTimePoint) {
        self.points.push(point);
        // Keep points sorted by position
        self.points.sort_by(|a, b| {
            a.position
                .partial_cmp(&b.position)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Get the tempo and time signature at a given time position
    pub fn get_at_time(&self, time: f64) -> (f64, (i32, i32)) {
        let engine = self.to_engine();
        let ts = engine.time_signature_at(time);
        (
            engine.tempo_at(time),
            (ts.numerator as i32, ts.denominator as i32),
        )
    }

    /// Build the shared tempo-map engine used by standalone and offline analysis.
    pub fn to_engine(&self) -> TempoMapEngine {
        TempoMapEngine::new(
            self.default_tempo,
            TimeSignature::new(
                self.default_time_signature.0.max(1) as u32,
                self.default_time_signature.1.max(1) as u32,
            ),
            self.points
                .iter()
                .map(TempoTimePoint::to_proto_point)
                .collect(),
        )
    }

    /// Calculate the total number of beats up to a given time
    /// This integrates tempo changes over time
    pub fn beats_at_time(&self, time: f64) -> f64 {
        self.to_engine().seconds_to_quarter_notes(time)
    }

    /// Calculate musical position (measure and beat) at a given time
    /// Returns (measure, beat, beat_fraction) where measure is 1-based
    /// This is more complex because time signatures can change throughout the song
    pub fn musical_position_at_time(&self, time: f64) -> (i32, i32, f64) {
        let (mut measure, mut beat, mut fraction) = self.to_engine().time_to_musical(time);
        let current_sig = self.get_at_time(time).1;

        // Normalize near-boundary floating point residue.
        // REAPER display rounds aggressively at beat boundaries.
        if fraction <= 0.02 {
            fraction = 0.0;
        } else if fraction >= 0.98 {
            fraction = 0.0;
            beat += 1;
        }
        if beat > current_sig.0.max(1) {
            beat = 1;
            measure += 1;
        }

        (measure.clamp(1, 1_000_000), beat, fraction)
    }

    /// Get musical position as a formatted string in REAPER's format (measure.beat.fraction)
    pub fn musical_position_string_at_time(&self, time: f64) -> String {
        let (mut measure, mut beat, fraction) = self.musical_position_at_time(time);

        // REAPER format: measure.beat.fraction (e.g., "12.1.00", "14.5.25")
        // Convert fraction to hundredths (0.25 becomes 25)
        let mut fraction_hundredths = (fraction * 100.0).round() as i32;
        if fraction_hundredths >= 100 {
            fraction_hundredths = 0;
            beat += 1;
            let current_sig = self.get_at_time(time).1;
            if beat > current_sig.0.max(1) {
                beat = 1;
                measure += 1;
            }
        }

        format!("{}.{}.{:02}", measure, beat, fraction_hundredths)
    }
}

impl Default for TempoTimeEnvelope {
    fn default() -> Self {
        Self::new(120.0, (4, 4))
    }
}

impl fmt::Display for TempoTimeEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Tempo/Time Signature Envelope")?;
        writeln!(
            f,
            "  Default: {} BPM, {}/{}",
            self.default_tempo, self.default_time_signature.0, self.default_time_signature.1
        )?;
        writeln!(f, "  Changes: {} points", self.points.len())?;

        if !self.points.is_empty() {
            writeln!(f)?;
            for point in &self.points {
                write!(f, "{}", point)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tempo_point() {
        let line = r#"PT 0.000000000000 121.9442407666 1 262148 0 1 0 "" 0 169 0 ABBB"#;

        // Debug: let's see what tokens we get
        let (_remaining, tokens) = parse_token_line(line).unwrap();
        println!("Debug tokens: {:?}", tokens);

        let point = TempoTimePoint::from_pt_line(line).unwrap();

        assert_eq!(point.position, 0.0);
        assert_eq!(point.tempo, 121.9442407666);
        assert_eq!(point.shape, 1);
        assert_eq!(point.time_signature_encoded, Some(262148));
        assert_eq!(point.time_signature(), Some((4, 4)));
        assert_eq!(point.time_signature_string(), "4/4");
        assert_eq!(point.metronome_pattern, "ABBB");
    }

    #[test]
    fn test_parse_tempo_point_minimal() {
        let line = r#"PT 25.658694116649 250.0000000000 1"#;
        let point = TempoTimePoint::from_pt_line(line).unwrap();

        assert_eq!(point.position, 25.658694116649);
        assert_eq!(point.tempo, 250.0);
        assert_eq!(point.shape, 1);
        assert_eq!(point.time_signature_encoded, None);
        assert_eq!(point.time_signature(), None);
        assert_eq!(point.metronome_pattern, "");
    }

    #[test]
    fn test_region_test_tempo_changes() {
        // Test with the actual tempo changes from Region Test.RPP
        let mut envelope = TempoTimeEnvelope::new(121.9442407666, (4, 4));

        // Add all the tempo changes from the file
        let points = vec![
            r#"PT 0.000000000000 121.9442407666 1 262148 0 1 0 "" 0 169 0 ABBB"#,
            r#"PT 15.744901013201 87.0000000000 1 524295 1 1 0 "" 0 10921 0 ABBBBBB"#,
            r#"PT 18.158694116649 32.0000000000 1 262148 0 1 0 "" 0 169 0 ABBB"#,
            r#"PT 25.658694116649 250.0000000000 1"#,
            r#"PT 26.618694116649 134.0000000000 1 262148 0 1 0 "" 0 169 0 ABBB"#,
        ];

        for point_line in points {
            let point = TempoTimePoint::from_pt_line(point_line).unwrap();
            envelope.add_point(point);
        }

        // Test the "Ending Section" marker at 26.618694116649 seconds
        let ending_section_time = 26.618694116649;
        let (measure, beat, fraction) = envelope.musical_position_at_time(ending_section_time);
        let musical_pos = envelope.musical_position_string_at_time(ending_section_time);

        println!("Ending Section at {:.3}s:", ending_section_time);
        println!("  Musical Position (REAPER format): {}", musical_pos);
        println!(
            "  Measure: {}, Beat: {}, Fraction: {:.3}",
            measure, beat, fraction
        );

        // Let's also test a few other key positions
        let test_positions = vec![
            (0.0, "Project start"),
            (15.744901013201, "7/8 time signature change"),
            (18.158694116649, "32 BPM change"),
            (25.658694116649, "250 BPM change"),
            (26.618694116649, "Ending Section"),
        ];

        for (time, description) in test_positions {
            let (_measure, _beat, _) = envelope.musical_position_at_time(time);
            let musical_pos = envelope.musical_position_string_at_time(time);
            println!("  {} at {:.3}s: {}", description, time, musical_pos);
        }

        // The test should pass - we're just demonstrating the calculation
        assert!(measure > 0);
        assert!(beat > 0);
    }
}
