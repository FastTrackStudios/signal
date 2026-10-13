//! Chord extensions - 9th, 11th, and 13th
//!
//! Represents extended harmony beyond the seventh

use crate::chord::degree::ChordDegree;
use crate::parsing::{ParseError, Token, TokenType};
use crate::primitives::Interval;
use facet::Facet;
use tracing::{debug, instrument, trace};

/// Type of extension quality
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum ExtensionQuality {
    /// Natural extension (9, 11, 13)
    Natural,
    /// Flat extension (b9, b13)
    Flat,
    /// Sharp extension (#9, #11)
    Sharp,
}

/// Extensions on a chord (9th, 11th, 13th)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Facet)]
pub struct Extensions {
    /// Ninth extension (if present)
    pub ninth: Option<ExtensionQuality>,
    /// Eleventh extension (if present)
    pub eleventh: Option<ExtensionQuality>,
    /// Thirteenth extension (if present)
    pub thirteenth: Option<ExtensionQuality>,
}

impl Extensions {
    /// Create empty extensions (no extensions)
    pub fn none() -> Self {
        Self {
            ninth: None,
            eleventh: None,
            thirteenth: None,
        }
    }

    /// Create extensions with just a ninth
    pub fn with_ninth(quality: ExtensionQuality) -> Self {
        Self {
            ninth: Some(quality),
            eleventh: None,
            thirteenth: None,
        }
    }

    /// Create extensions with ninth and eleventh
    pub fn with_eleventh(ninth: ExtensionQuality, eleventh: ExtensionQuality) -> Self {
        Self {
            ninth: Some(ninth),
            eleventh: Some(eleventh),
            thirteenth: None,
        }
    }

    /// Create extensions with ninth, eleventh, and thirteenth
    pub fn with_thirteenth(
        ninth: ExtensionQuality,
        eleventh: ExtensionQuality,
        thirteenth: ExtensionQuality,
    ) -> Self {
        Self {
            ninth: Some(ninth),
            eleventh: Some(eleventh),
            thirteenth: Some(thirteenth),
        }
    }

    /// Check if there are any extensions
    pub fn has_any(&self) -> bool {
        self.ninth.is_some() || self.eleventh.is_some() || self.thirteenth.is_some()
    }

    /// Check if there is any natural (unaltered) extension
    /// Returns true if at least one extension is natural
    ///
    /// The highest natural extension masks the seventh in chord notation.
    ///
    /// Examples:
    /// - Dm9 (natural 9) → true (has natural extension, masks 7)
    /// - Dm9#11 (natural 9, sharp 11) → true (has natural 9, masks 7 at 9)
    /// - Dm11b13 (natural 9, natural 11, flat 13) → true (has natural 11, masks 7 at 11)
    /// - D7#9 (sharp 9) → false (no natural extensions, must show 7)
    /// - D7#9b13 (sharp 9, flat 13) → false (no natural extensions, must show 7)
    pub fn has_natural(&self) -> bool {
        matches!(self.ninth, Some(ExtensionQuality::Natural))
            || matches!(self.eleventh, Some(ExtensionQuality::Natural))
            || matches!(self.thirteenth, Some(ExtensionQuality::Natural))
    }

    /// Get the interval for the ninth (if present)
    pub fn ninth_interval(&self) -> Option<Interval> {
        self.ninth.map(|q| match q {
            ExtensionQuality::Natural => Interval::Ninth,
            ExtensionQuality::Flat => Interval::FlatNinth,
            ExtensionQuality::Sharp => Interval::SharpNinth,
        })
    }

    /// Get the interval for the eleventh (if present)
    pub fn eleventh_interval(&self) -> Option<Interval> {
        self.eleventh.map(|q| match q {
            ExtensionQuality::Natural => Interval::Eleventh,
            ExtensionQuality::Sharp => Interval::SharpEleventh,
            ExtensionQuality::Flat => {
                // Flat 11th is uncommon, but treat as natural 4th octave up
                Interval::Eleventh
            }
        })
    }

    /// Get the interval for the thirteenth (if present)
    pub fn thirteenth_interval(&self) -> Option<Interval> {
        self.thirteenth.map(|q| match q {
            ExtensionQuality::Natural => Interval::Thirteenth,
            ExtensionQuality::Flat => Interval::FlatThirteenth,
            ExtensionQuality::Sharp => {
                // Sharp 13th is uncommon, but treat as natural
                Interval::Thirteenth
            }
        })
    }

    /// Get all extension degrees present
    pub fn degrees(&self) -> Vec<ChordDegree> {
        let mut degrees = Vec::new();
        if self.ninth.is_some() {
            degrees.push(ChordDegree::Ninth);
        }
        if self.eleventh.is_some() {
            degrees.push(ChordDegree::Eleventh);
        }
        if self.thirteenth.is_some() {
            degrees.push(ChordDegree::Thirteenth);
        }
        degrees
    }

    /// Get all extension intervals
    pub fn intervals(&self) -> Vec<Interval> {
        let mut intervals = Vec::new();
        if let Some(interval) = self.ninth_interval() {
            intervals.push(interval);
        }
        if let Some(interval) = self.eleventh_interval() {
            intervals.push(interval);
        }
        if let Some(interval) = self.thirteenth_interval() {
            intervals.push(interval);
        }
        intervals
    }

    /// Get the highest extension present (for display purposes)
    /// Returns None if no extensions
    pub fn highest(&self) -> Option<ChordDegree> {
        if self.thirteenth.is_some() {
            Some(ChordDegree::Thirteenth)
        } else if self.eleventh.is_some() {
            Some(ChordDegree::Eleventh)
        } else if self.ninth.is_some() {
            Some(ChordDegree::Ninth)
        } else {
            None
        }
    }

    /// Build a symbol string for these extensions
    pub fn symbol(&self) -> String {
        let mut parts = Vec::new();

        if let Some(quality) = self.ninth {
            parts.push(match quality {
                ExtensionQuality::Natural => "9".to_string(),
                ExtensionQuality::Flat => "b9".to_string(),
                ExtensionQuality::Sharp => "#9".to_string(),
            });
        }

        if let Some(quality) = self.eleventh {
            parts.push(match quality {
                ExtensionQuality::Natural => "11".to_string(),
                ExtensionQuality::Sharp => "#11".to_string(),
                ExtensionQuality::Flat => "11".to_string(),
            });
        }

        if let Some(quality) = self.thirteenth {
            parts.push(match quality {
                ExtensionQuality::Natural => "13".to_string(),
                ExtensionQuality::Flat => "b13".to_string(),
                ExtensionQuality::Sharp => "13".to_string(),
            });
        }

        parts.join(" ")
    }

    /// Parse extensions from tokens
    /// Returns (Extensions, tokens_consumed)
    ///
    /// Handles: 9, b9, #9, 11, #11, 13, b13, maj9, maj11, maj13
    /// When a higher extension is found (e.g., 13), it implies all lower extensions (9, 11)
    /// "maj9", "maj11", "maj13" indicate major seventh with extensions
    #[instrument(level = "debug", skip(tokens), fields(token_count = tokens.len()))]
    pub fn parse(tokens: &[Token]) -> Result<(Extensions, usize), ParseError> {
        if tokens.is_empty() {
            return Ok((Extensions::none(), 0));
        }

        let mut consumed = 0;
        let mut ninth = None;
        let mut eleventh = None;
        let mut thirteenth = None;

        trace!("Starting extension parsing with {} tokens", tokens.len());

        while consumed < tokens.len() {
            trace!(
                "Loop iteration: consumed={}, tokens[{}]={:?}",
                consumed, consumed, tokens[consumed].token_type
            );

            // Check for accidental (b or #)
            let quality_mod = if consumed < tokens.len() {
                match tokens[consumed].token_type {
                    TokenType::Flat | TokenType::Letter('b') => {
                        trace!("Found flat at position {}", consumed);
                        consumed += 1;
                        Some(ExtensionQuality::Flat)
                    }
                    TokenType::Sharp => {
                        trace!("Found sharp at position {}", consumed);
                        consumed += 1;
                        Some(ExtensionQuality::Sharp)
                    }
                    _ => Some(ExtensionQuality::Natural),
                }
            } else {
                Some(ExtensionQuality::Natural)
            };

            // Check for number
            if consumed < tokens.len() {
                trace!(
                    "Checking for number at position {}: {:?}",
                    consumed, tokens[consumed].token_type
                );
                if let TokenType::Number(n) = &tokens[consumed].token_type {
                    let quality = quality_mod.unwrap();
                    trace!("Found number '{}' with quality {:?}", n, quality);
                    match n.as_str() {
                        "9" => {
                            debug!("Parsed 9th extension: {:?}", quality);
                            ninth = Some(quality);
                            consumed += 1;
                        }
                        "11" => {
                            debug!("Parsed 11th extension: {:?}", quality);
                            // Don't add implied 9th - preserve exactly what was written
                            // The compute_intervals() method handles implied notes for voicing
                            eleventh = Some(quality);
                            consumed += 1;
                        }
                        "13" => {
                            debug!("Parsed 13th extension: {:?}", quality);
                            // Don't add implied 9th/11th - preserve exactly what was written
                            // The compute_intervals() method handles implied notes for voicing
                            thirteenth = Some(quality);
                            consumed += 1;
                        }
                        _ => {
                            trace!("Number '{}' is not an extension, stopping", n);
                            // Not an extension number, stop parsing
                            // If we consumed an accidental, back up
                            if quality != ExtensionQuality::Natural {
                                consumed -= 1;
                            }
                            break;
                        }
                    }
                } else {
                    trace!("No number found at position {}, stopping", consumed);
                    // No number found after accidental, back up if we consumed one
                    if quality_mod.unwrap() != ExtensionQuality::Natural {
                        consumed -= 1;
                    }
                    break;
                }
            } else {
                trace!("Reached end of tokens");
                break;
            }
        }

        debug!(
            "Extension parsing complete: ninth={:?}, eleventh={:?}, thirteenth={:?}, consumed={}",
            ninth, eleventh, thirteenth, consumed
        );

        let extensions = Extensions {
            ninth,
            eleventh,
            thirteenth,
        };

        Ok((extensions, consumed))
    }
}

impl std::fmt::Display for Extensions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // In standard chord notation:
        // - The highest natural extension becomes the chord name (C11, C13)
        // - Lower natural extensions are implied and not shown
        // - Altered extensions are always shown (C13#11, C11b9)
        //
        // Examples:
        // - 9 natural → "9"
        // - 11 natural (implies 9) → "11"
        // - 13 natural (implies 9, 11) → "13"
        // - 9 natural + 11 natural → "11" (9 is implied)
        // - 9 sharp + 11 natural → "11#9" (altered 9 shown)
        // - 9 natural + 11 sharp → "9#11" (show 9, then altered 11)

        let highest_natural = self.highest();

        // Show 9th if:
        // - It's the highest extension, OR
        // - It's altered (b9, #9)
        if let Some(quality) = self.ninth {
            let is_highest = matches!(highest_natural, Some(ChordDegree::Ninth));
            match quality {
                ExtensionQuality::Natural if is_highest => write!(f, "9")?,
                ExtensionQuality::Natural => {} // Implied by higher extension
                ExtensionQuality::Flat => write!(f, "b9")?,
                ExtensionQuality::Sharp => write!(f, "#9")?,
            }
        }

        // Show 11th if:
        // - It's the highest extension, OR
        // - It's altered (#11)
        if let Some(quality) = self.eleventh {
            let is_highest = matches!(highest_natural, Some(ChordDegree::Eleventh));
            match quality {
                ExtensionQuality::Natural if is_highest => write!(f, "11")?,
                ExtensionQuality::Natural => {} // Implied by 13th
                ExtensionQuality::Flat => write!(f, "b11")?,
                ExtensionQuality::Sharp => write!(f, "#11")?,
            }
        }

        // 13th is always shown if present (it's the highest possible)
        if let Some(quality) = self.thirteenth {
            match quality {
                ExtensionQuality::Natural => write!(f, "13")?,
                ExtensionQuality::Flat => write!(f, "b13")?,
                ExtensionQuality::Sharp => write!(f, "#13")?,
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none() {
        let ext = Extensions::none();
        assert!(!ext.has_any());
        assert_eq!(ext.degrees(), Vec::<ChordDegree>::new());
    }

    #[test]
    fn test_with_ninth() {
        let ext = Extensions::with_ninth(ExtensionQuality::Natural);
        assert!(ext.has_any());
        assert_eq!(ext.ninth, Some(ExtensionQuality::Natural));
        assert_eq!(ext.eleventh, None);
        assert_eq!(ext.thirteenth, None);
        assert_eq!(ext.ninth_interval(), Some(Interval::Ninth));
    }

    #[test]
    fn test_with_flat_ninth() {
        let ext = Extensions::with_ninth(ExtensionQuality::Flat);
        assert_eq!(ext.ninth_interval(), Some(Interval::FlatNinth));
    }

    #[test]
    fn test_with_sharp_ninth() {
        let ext = Extensions::with_ninth(ExtensionQuality::Sharp);
        assert_eq!(ext.ninth_interval(), Some(Interval::SharpNinth));
    }

    #[test]
    fn test_with_eleventh() {
        let ext = Extensions::with_eleventh(ExtensionQuality::Natural, ExtensionQuality::Natural);
        assert!(ext.has_any());
        assert_eq!(ext.ninth, Some(ExtensionQuality::Natural));
        assert_eq!(ext.eleventh, Some(ExtensionQuality::Natural));
        assert_eq!(ext.thirteenth, None);
    }

    #[test]
    fn test_with_thirteenth() {
        let ext = Extensions::with_thirteenth(
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
        );
        assert!(ext.has_any());
        assert_eq!(ext.ninth, Some(ExtensionQuality::Natural));
        assert_eq!(ext.eleventh, Some(ExtensionQuality::Natural));
        assert_eq!(ext.thirteenth, Some(ExtensionQuality::Natural));
    }

    #[test]
    fn test_degrees() {
        let ext = Extensions::with_thirteenth(
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
        );
        let degrees = ext.degrees();
        assert_eq!(degrees.len(), 3);
        assert!(degrees.contains(&ChordDegree::Ninth));
        assert!(degrees.contains(&ChordDegree::Eleventh));
        assert!(degrees.contains(&ChordDegree::Thirteenth));
    }

    #[test]
    fn test_highest() {
        let ext_none = Extensions::none();
        assert_eq!(ext_none.highest(), None);

        let ext_ninth = Extensions::with_ninth(ExtensionQuality::Natural);
        assert_eq!(ext_ninth.highest(), Some(ChordDegree::Ninth));

        let ext_thirteenth = Extensions::with_thirteenth(
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
        );
        assert_eq!(ext_thirteenth.highest(), Some(ChordDegree::Thirteenth));
    }

    #[test]
    fn test_intervals() {
        let ext = Extensions::with_thirteenth(
            ExtensionQuality::Natural,
            ExtensionQuality::Sharp,
            ExtensionQuality::Flat,
        );
        let intervals = ext.intervals();
        assert_eq!(intervals.len(), 3);
        assert!(intervals.contains(&Interval::Ninth));
        assert!(intervals.contains(&Interval::SharpEleventh));
        assert!(intervals.contains(&Interval::FlatThirteenth));
    }
}
