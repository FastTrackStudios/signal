//! Envelope data structures and parsing for REAPER

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::primitives::{RppBlock, Token};

/// Envelope point shapes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopePointShape {
    Linear = 0,
    Square = 1,
    SlowStartEnd = 2,
    FastStart = 3,
    FastEnd = 4,
    Bezier = 5,
    Default = -1, // Use envelope default
}

impl fmt::Display for EnvelopePointShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvelopePointShape::Linear => write!(f, "Linear"),
            EnvelopePointShape::Square => write!(f, "Square"),
            EnvelopePointShape::SlowStartEnd => write!(f, "Slow Start/End"),
            EnvelopePointShape::FastStart => write!(f, "Fast Start"),
            EnvelopePointShape::FastEnd => write!(f, "Fast End"),
            EnvelopePointShape::Bezier => write!(f, "Bezier"),
            EnvelopePointShape::Default => write!(f, "Default"),
        }
    }
}

/// Automation Item (AI) properties within an envelope
///
/// Automation items are reusable automation patterns that can be pooled and shared
/// across multiple envelope instances. They contain their own envelope points and
/// can be looped, have different play rates, and baseline/amplitude adjustments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationItem {
    /// Pool index - pooled instances have the same index
    /// (greater by 1 than the one displayed by default in the Name field of AI Properties window)
    pub pool_index: i32,

    /// Position in seconds
    pub position: f64,

    /// Length in seconds
    pub length: f64,

    /// Start offset in seconds
    pub start_offset: f64,

    /// Play rate (1.0 = normal speed)
    pub play_rate: f64,

    /// Selected state (bool)
    pub selected: bool,

    /// Baseline value (0 = -100%, 0.5 = 0%, 1 = 100%)
    pub baseline: f64,

    /// Amplitude (-2 = -200%, 1 = 100%, 2 = 200%)
    /// Baseline/amplitude affects pooled copies
    pub amplitude: f64,

    /// Loop enabled (bool)
    pub loop_enabled: bool,

    /// Position in quarter notes (only used in certain contexts)
    pub position_qn: f64,

    /// Length in quarter notes (only used in certain contexts)
    pub length_qn: f64,

    /// 1-based index of AI since starting the project, incremented even if
    /// older AIs were deleted and regardless of the AI being pooled
    pub instance_index: i32,

    /// Muted state (bool)
    pub muted: bool,

    /// Start offset in quarter notes (only used in certain contexts)
    pub start_offset_qn: f64,

    /// Transition time in seconds
    pub transition_time: f64,

    /// Volume envelope maximum when this instance was created
    /// (matches the 1|4 bits of the "volenvrange" config variable:
    /// 0=+6dB, 1=+0dB, 4=+12dB, 5=+24dB)
    pub volume_envelope_max: i32,
}

/// A pooled automation item's source — a project-level `<POOLEDENV>`: the
/// curve every instance of the pool (an envelope's `POOLEDENVINST` line
/// naming its `ID`) plays.
///
/// ```text
/// <POOLEDENV
///   ID 1
///   NAME "Chorus"
///   SRCLEN 8
///   PT 0 0.5 0
///   PT 8 0.8 0
/// >
/// ```
///
/// Point times are seconds into the source; values are the envelope's own
/// file values. Lines this type does not read (an LFO's settings) are kept
/// in `other` and written back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PooledEnvelope {
    pub id: i32,
    pub name: String,
    /// The source's length in seconds — what a looped instance repeats.
    pub source_length: f64,
    pub points: Vec<EnvelopePoint>,
    /// Lines kept as they were, inner indentation stripped.
    pub other: Vec<String>,
}

impl PooledEnvelope {
    /// Every `<POOLEDENV>` directly inside the project in `rpp` text.
    pub fn all_in(rpp: &str) -> Vec<Self> {
        let mut out = Vec::new();
        let mut depth = 0_i32;
        let mut open: Option<Self> = None;
        for line in rpp.lines() {
            let t = line.trim();
            if let Some(pool) = open.as_mut() {
                if t == ">" {
                    out.extend(open.take());
                } else if let Some(v) = t.strip_prefix("ID ") {
                    pool.id = v.trim().parse().unwrap_or(0);
                } else if let Some(v) = t.strip_prefix("NAME ") {
                    pool.name = v.trim().trim_matches('"').to_string();
                } else if let Some(v) = t.strip_prefix("SRCLEN ") {
                    pool.source_length = v.trim().parse().unwrap_or(0.0);
                } else if t.starts_with("PT ") {
                    match EnvelopePoint::from_rpp_line(t) {
                        Ok(p) => pool.points.push(p),
                        Err(_) => pool.other.push(t.to_string()),
                    }
                } else if !t.is_empty() {
                    pool.other.push(t.to_string());
                }
                continue;
            }
            if t.starts_with('<') {
                if depth == 1 && t == "<POOLEDENV" {
                    open = Some(Self {
                        id: 0,
                        name: String::new(),
                        source_length: 0.0,
                        points: Vec::new(),
                        other: Vec::new(),
                    });
                    continue;
                }
                depth += 1;
            } else if t == ">" {
                depth -= 1;
            }
        }
        out
    }
}

impl fmt::Display for PooledEnvelope {
    /// The block at the project's indentation (two spaces).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  <POOLEDENV")?;
        writeln!(f, "    ID {}", self.id)?;
        writeln!(f, "    NAME \"{}\"", self.name.replace('"', "'"))?;
        writeln!(f, "    SRCLEN {:.6}", self.source_length)?;
        for line in &self.other {
            writeln!(f, "    {line}")?;
        }
        for p in &self.points {
            writeln!(f, "    {}", p.to_string().trim_end())?;
        }
        writeln!(f, "  >")
    }
}

/// Extension-specific persistent data block within an envelope
///
/// These blocks contain arbitrary data that can be added by extensions
/// using the GetSetEnvelopeInfo_String() function with 'P_EXT' parameter.
/// The structure is unknown and can vary, so we preserve the raw content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionData {
    /// The parameter name (field 1)
    pub parmname: String,

    /// The string data (field 2)
    pub string_data: String,

    /// Raw content of the entire EXT block for preservation
    pub raw_content: String,
}

impl ExtensionData {
    /// Create a new extension data block from a raw RPP EXT block
    /// Expected format: "<EXT\n  parmname string_data\n>"
    pub fn from_rpp_block(block_content: &str) -> Result<Self, String> {
        let lines: Vec<&str> = block_content.lines().collect();

        if lines.len() < 3 {
            return Err(format!(
                "EXT block must have at least 3 lines (start, content, end), got {}",
                lines.len()
            ));
        }

        // First line should be "<EXT"
        if !lines[0].trim().starts_with("<EXT") {
            return Err(format!(
                "Expected EXT block to start with '<EXT', got: '{}'",
                lines[0]
            ));
        }

        // Last line should be ">"
        if lines.last().unwrap().trim() != ">" {
            return Err(format!(
                "Expected EXT block to end with '>', got: '{}'",
                lines.last().unwrap()
            ));
        }

        // Parse the content line (should be the middle line)
        if lines.len() == 3 {
            let content_line = lines[1].trim();
            let tokens: Vec<&str> = content_line.split_whitespace().collect();

            if tokens.len() < 2 {
                return Err(format!(
                    "EXT block content line must have at least 2 tokens, got: '{}'",
                    content_line
                ));
            }

            let parmname = tokens[0].to_string();
            let string_data = tokens[1..].join(" ");

            Ok(ExtensionData {
                parmname,
                string_data,
                raw_content: block_content.to_string(),
            })
        } else {
            // Handle multi-line content
            let content_lines = &lines[1..lines.len() - 1];
            let _content = content_lines.join("\n");

            // Try to parse the first line for parmname and string_data
            let first_content_line = content_lines[0].trim();
            let tokens: Vec<&str> = first_content_line.split_whitespace().collect();

            if tokens.len() < 2 {
                return Err(format!(
                    "EXT block first content line must have at least 2 tokens, got: '{}'",
                    first_content_line
                ));
            }

            let parmname = tokens[0].to_string();
            let string_data = tokens[1..].join(" ");

            Ok(ExtensionData {
                parmname,
                string_data,
                raw_content: block_content.to_string(),
            })
        }
    }
}

impl fmt::Display for ExtensionData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw_content)
    }
}

impl AutomationItem {
    /// Create a new automation item from tokens
    /// Expected format: POOLEDENVINST pool_index position length start_offset play_rate selected baseline amplitude loop_enabled position_qn length_qn instance_index muted start_offset_qn transition_time volume_envelope_max
    pub fn from_tokens(tokens: &[Token]) -> Result<Self, String> {
        if tokens.len() < 17 {
            return Err(format!(
                "AutomationItem expects 17 tokens, got {}",
                tokens.len()
            ));
        }

        // Parse pool_index (field 1)
        let pool_index = match &tokens[1] {
            Token::Integer(i) => *i as i32,
            _ => return Err("Expected integer for pool_index".to_string()),
        };

        // Parse position (field 2)
        let position = match &tokens[2] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for position".to_string()),
        };

        // Parse length (field 3)
        let length = match &tokens[3] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for length".to_string()),
        };

        // Parse start_offset (field 4)
        let start_offset = match &tokens[4] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for start_offset".to_string()),
        };

        // Parse play_rate (field 5)
        let play_rate = match &tokens[5] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for play_rate".to_string()),
        };

        // Parse selected (field 6)
        let selected = match &tokens[6] {
            Token::Integer(i) => *i != 0,
            _ => return Err("Expected integer for selected".to_string()),
        };

        // Parse baseline (field 7)
        let baseline = match &tokens[7] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for baseline".to_string()),
        };

        // Parse amplitude (field 8)
        let amplitude = match &tokens[8] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for amplitude".to_string()),
        };

        // Parse loop_enabled (field 9)
        let loop_enabled = match &tokens[9] {
            Token::Integer(i) => *i != 0,
            _ => return Err("Expected integer for loop_enabled".to_string()),
        };

        // Parse position_qn (field 10)
        let position_qn = match &tokens[10] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for position_qn".to_string()),
        };

        // Parse length_qn (field 11)
        let length_qn = match &tokens[11] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for length_qn".to_string()),
        };

        // Parse instance_index (field 12)
        let instance_index = match &tokens[12] {
            Token::Integer(i) => *i as i32,
            _ => return Err("Expected integer for instance_index".to_string()),
        };

        // Parse muted (field 13)
        let muted = match &tokens[13] {
            Token::Integer(i) => *i != 0,
            _ => return Err("Expected integer for muted".to_string()),
        };

        // Parse start_offset_qn (field 14)
        let start_offset_qn = match &tokens[14] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for start_offset_qn".to_string()),
        };

        // Parse transition_time (field 15)
        let transition_time = match &tokens[15] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float or integer for transition_time".to_string()),
        };

        // Parse volume_envelope_max (field 16)
        let volume_envelope_max = match &tokens[16] {
            Token::Integer(i) => *i as i32,
            _ => return Err("Expected integer for volume_envelope_max".to_string()),
        };

        Ok(AutomationItem {
            pool_index,
            position,
            length,
            start_offset,
            play_rate,
            selected,
            baseline,
            amplitude,
            loop_enabled,
            position_qn,
            length_qn,
            instance_index,
            muted,
            start_offset_qn,
            transition_time,
            volume_envelope_max,
        })
    }

    /// Create a new automation item from a raw RPP line
    /// Expected format: "POOLEDENVINST 1 5 1.5 0 1 1 0.5 1 1 0 0 1 0 0 0.012 0"
    pub fn from_rpp_line(line: &str) -> Result<Self, String> {
        use crate::primitives::token::parse_token_line;

        let tokens = parse_token_line(line)
            .map_err(|e| format!("Failed to parse line '{}': {:?}", line, e))?
            .1;

        Self::from_tokens(&tokens)
    }
}

impl fmt::Display for AutomationItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "POOLEDENVINST {} {:.6} {:.6} {:.6} {:.6} {} {:.6} {:.6} {} {:.6} {:.6} {} {} {:.6} {:.6} {}",
            self.pool_index,
            self.position,
            self.length,
            self.start_offset,
            self.play_rate,
            if self.selected { 1 } else { 0 },
            self.baseline,
            self.amplitude,
            if self.loop_enabled { 1 } else { 0 },
            self.position_qn,
            self.length_qn,
            self.instance_index,
            if self.muted { 1 } else { 0 },
            self.start_offset_qn,
            self.transition_time,
            self.volume_envelope_max
        )
    }
}

impl From<i32> for EnvelopePointShape {
    fn from(value: i32) -> Self {
        match value {
            0 => EnvelopePointShape::Linear,
            1 => EnvelopePointShape::Square,
            2 => EnvelopePointShape::SlowStartEnd,
            3 => EnvelopePointShape::FastStart,
            4 => EnvelopePointShape::FastEnd,
            5 => EnvelopePointShape::Bezier,
            -1 => EnvelopePointShape::Default,
            _ => EnvelopePointShape::Default,
        }
    }
}

impl From<i64> for EnvelopePointShape {
    fn from(value: i64) -> Self {
        Self::from(value as i32)
    }
}

/// An envelope point - the lowest level component in RPP files
///
/// PT 3.000000 -0.2 5 0 0 0 -0.7
/// field 1, float, position (seconds)
/// field 2, float, value
/// field 3, int, shape (-1 = envelope default?)
/// field 4, int, optional, ?? (TEMPOENVEX time sig = 65536 *
/// time signature denominator + time signature numerator)
/// field 5, int (bool), selected (optional)
/// field 6, int, ?? (optional)
/// field 7, float, bezier tension (optional)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopePoint {
    pub position: f64,                // field 1: position in seconds
    pub value: f64,                   // field 2: envelope value
    pub shape: EnvelopePointShape,    // field 3: point shape
    pub time_sig: Option<i32>,        // field 4: time signature (TEMPOENVEX only)
    pub selected: Option<bool>,       // field 5: selected state
    pub unknown_field_6: Option<i32>, // field 6: unknown field
    pub bezier_tension: Option<f64>,  // field 7: bezier tension
}

impl fmt::Display for EnvelopePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PT {:.6} {:.6} {} ",
            self.position, self.value, self.shape as i32
        )?;

        if let Some(ts) = self.time_sig {
            write!(f, "{} ", ts)?;
        }
        if let Some(sel) = self.selected {
            write!(f, "{} ", if sel { 1 } else { 0 })?;
        }
        if let Some(unk) = self.unknown_field_6 {
            write!(f, "{} ", unk)?;
        }
        if let Some(tension) = self.bezier_tension {
            write!(f, "{:.6}", tension)?;
        }

        Ok(())
    }
}

impl EnvelopePoint {
    /// Create a new envelope point from a raw RPP line
    /// Expected format: "PT 3.000000 -0.2 5 0 0 0 -0.7"
    pub fn from_rpp_line(line: &str) -> Result<Self, String> {
        use crate::primitives::token::parse_token_line;

        let tokens = parse_token_line(line)
            .map_err(|e| format!("Failed to parse line '{}': {:?}", line, e))?
            .1;

        Self::from_tokens(&tokens)
    }

    /// Create a new envelope point from tokens
    /// Expected format: `PT position value shape [time_sig] [selected] [unknown] [bezier_tension]`
    pub fn from_tokens(tokens: &[Token]) -> Result<Self, String> {
        if tokens.len() < 4 {
            return Err(format!(
                "Envelope point needs at least 4 tokens, got {}",
                tokens.len()
            ));
        }

        // Parse position (field 1)
        let position = match &tokens[1] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float for position".to_string()),
        };

        // Parse value (field 2)
        let value = match &tokens[2] {
            Token::Float(f) => *f,
            Token::Integer(i) => *i as f64,
            _ => return Err("Expected float for value".to_string()),
        };

        // Parse shape (field 3)
        let shape = match &tokens[3] {
            Token::Integer(i) => EnvelopePointShape::from(*i),
            _ => return Err("Expected integer for shape".to_string()),
        };

        // Parse optional fields
        let mut time_sig = None;
        let mut selected = None;
        let mut unknown_field_6 = None;
        let mut bezier_tension = None;

        if tokens.len() > 4 {
            time_sig = match &tokens[4] {
                Token::Integer(i) => Some(*i as i32),
                _ => None,
            };
        }

        if tokens.len() > 5 {
            selected = match &tokens[5] {
                Token::Integer(i) => Some(*i != 0),
                _ => None,
            };
        }

        if tokens.len() > 6 {
            unknown_field_6 = match &tokens[6] {
                Token::Integer(i) => Some(*i as i32),
                _ => None,
            };
        }

        if tokens.len() > 7 {
            bezier_tension = match &tokens[7] {
                Token::Float(f) => Some(*f),
                Token::Integer(i) => Some(*i as f64),
                _ => None,
            };
        }

        Ok(EnvelopePoint {
            position,
            value,
            shape,
            time_sig,
            selected,
            unknown_field_6,
            bezier_tension,
        })
    }
}

/// A REAPER envelope
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub envelope_type: String,
    pub guid: String,
    pub active: bool,
    pub visible: bool,
    pub show_in_lane: bool,
    pub lane_height: i32,
    pub armed: bool,
    pub default_shape: i32,
    pub points: Vec<EnvelopePoint>,
    pub automation_items: Vec<AutomationItem>,
    pub extension_data: Vec<ExtensionData>,
}

impl fmt::Display for Envelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Envelope({}) - {} points, {} automation items, {} extension blocks",
            self.envelope_type,
            self.points.len(),
            self.automation_items.len(),
            self.extension_data.len()
        )
    }
}

type ParseEnvelopeResult =
    Result<(Vec<EnvelopePoint>, Vec<AutomationItem>, Vec<ExtensionData>), String>;

impl Envelope {
    /// Create a new envelope from an RPP block: header lines
    /// (EGUID/ACT/VIS/LANEHEIGHT/ARM/DEFSHAPE), the `PT` point list,
    /// and `POOLEDENVINST` automation items.
    pub fn from_block(block: &RppBlock) -> Result<Self, String> {
        use crate::primitives::RppBlockContent;
        let mut env = Envelope {
            envelope_type: block.name.clone(),
            guid: String::new(),
            // REAPER omits ACT/VIS on some old envelopes — both
            // default to on.
            active: true,
            visible: true,
            show_in_lane: false,
            lane_height: 0,
            armed: false,
            default_shape: 0,
            points: Vec::new(),
            automation_items: Vec::new(),
            extension_data: Vec::new(),
        };
        for child in &block.children {
            // Nested blocks (`<EXT …>`) carry extension data we don't
            // decode here yet.
            if matches!(child, RppBlockContent::Block(_)) {
                continue;
            }
            let line = child.to_string();
            let line = line.trim();
            let mut it = line.split_whitespace();
            let Some(key) = it.next() else { continue };
            let mut next_i = |default: i64| -> i64 {
                it.next()
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(default)
            };
            match key {
                "EGUID" => env.guid = line[5..].trim().to_string(),
                "ACT" => env.active = next_i(1) != 0,
                "VIS" => {
                    env.visible = next_i(1) != 0;
                    env.show_in_lane = next_i(0) != 0;
                }
                "LANEHEIGHT" => env.lane_height = next_i(0) as i32,
                "ARM" => env.armed = next_i(0) != 0,
                "DEFSHAPE" => env.default_shape = next_i(0) as i32,
                "PT" => {
                    if let Ok(pt) = EnvelopePoint::from_rpp_line(line) {
                        env.points.push(pt);
                    }
                }
                "POOLEDENVINST" => {
                    if let Ok(ai) = AutomationItem::from_rpp_line(line) {
                        env.automation_items.push(ai);
                    }
                }
                _ => {}
            }
        }
        Ok(env)
    }

    /// Parse envelope points, automation items, and extension data from a raw RPP envelope block string
    ///
    /// # Example
    /// ```
    /// use dawfile_reaper::Envelope;
    ///
    /// let rpp_content = r#"<VOLENV2
    ///   EGUID {E1A0A26B-6BF4-C6BD-873D-DB57E2C20775}
    ///   ACT 1 -1
    ///   VIS 1 1 1
    ///   LANEHEIGHT 113 0
    ///   ARM 1
    ///   DEFSHAPE 0 -1 -1
    ///   VOLTYPE 1
    ///   <EXT
    ///   my_extension "some data"
    ///   >
    ///   POOLEDENVINST 1 5 1.5 0 1 1 0.5 1 1 0 0 1 0 0 0.012 0
    ///   PT 0 1 0
    ///   PT 2 0.02073425 5 1 1 0 0.3030303
    ///   PT 3.98 0.25703958 0
    ///   PT 4 0.22266414 0
    ///   PT 4.02 0.25703958 0
    /// >"#;
    ///
    /// let (points, automation_items, extension_data) = Envelope::parse_envelope_content(rpp_content).unwrap();
    /// ```
    pub fn parse_envelope_content(rpp_content: &str) -> ParseEnvelopeResult {
        let mut points = Vec::new();
        let mut automation_items = Vec::new();
        let mut extension_data = Vec::new();

        let lines: Vec<&str> = rpp_content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with("//") {
                i += 1;
                continue;
            }

            // Skip block start/end markers (but not EXT blocks)
            if (line.starts_with('<') && !line.starts_with("<EXT")) || line == ">" {
                i += 1;
                continue;
            }

            // Look for envelope point lines (PT)
            if line.starts_with("PT ") {
                let point = EnvelopePoint::from_rpp_line(line)?;
                points.push(point);
                i += 1;
            }
            // Look for automation item lines (POOLEDENVINST)
            else if line.starts_with("POOLEDENVINST ") {
                let automation_item = AutomationItem::from_rpp_line(line)?;
                automation_items.push(automation_item);
                i += 1;
            }
            // Look for EXT blocks
            else if line.starts_with("<EXT") {
                // Find the end of the EXT block
                let mut ext_lines = Vec::new();
                ext_lines.push(line);
                i += 1;

                while i < lines.len() {
                    let current_line = lines[i].trim();
                    ext_lines.push(current_line);

                    if current_line == ">" {
                        break;
                    }
                    i += 1;
                }

                if i >= lines.len() {
                    return Err("EXT block not properly closed with '>'".to_string());
                }

                let ext_block_content = ext_lines.join("\n");
                let extension_block = ExtensionData::from_rpp_block(&ext_block_content)?;
                extension_data.push(extension_block);
                i += 1;
            } else {
                // Skip unknown lines
                i += 1;
            }
        }

        Ok((points, automation_items, extension_data))
    }

    /// Parse envelope points from a raw RPP envelope block string (legacy method)
    ///
    /// # Example
    /// ```
    /// use dawfile_reaper::Envelope;
    ///
    /// let rpp_content = r#"<VOLENV2
    ///   EGUID {E1A0A26B-6BF4-C6BD-873D-DB57E2C20775}
    ///   ACT 1 -1
    ///   VIS 1 1 1
    ///   LANEHEIGHT 113 0
    ///   ARM 1
    ///   DEFSHAPE 0 -1 -1
    ///   VOLTYPE 1
    ///   PT 0 1 0
    ///   PT 2 0.02073425 5 1 1 0 0.3030303
    ///   PT 3.98 0.25703958 0
    ///   PT 4 0.22266414 0
    ///   PT 4.02 0.25703958 0
    /// >"#;
    ///
    /// let points = Envelope::parse_envelope_points(rpp_content).unwrap();
    /// ```
    pub fn parse_envelope_points(rpp_content: &str) -> Result<Vec<EnvelopePoint>, String> {
        let (points, _, _) = Self::parse_envelope_content(rpp_content)?;
        Ok(points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_envelope_content_with_automation_items() {
        let rpp_content = r#"<VOLENV2
  EGUID {E1A0A26B-6BF4-C6BD-873D-DB57E2C20775}
  ACT 1 -1
  VIS 1 1 1
  LANEHEIGHT 113 0
  ARM 1
  DEFSHAPE 0 -1 -1
  VOLTYPE 1
  POOLEDENVINST 1 5 1.5 0 1 1 0.5 1 1 0 0 1 0 0 0.012 0
  PT 0 1 0
  PT 2 0.02073425 5 1 1 0 0.3030303
  PT 3.98 0.25703958 0
  PT 4 0.22266414 0
  PT 4.02 0.25703958 0
>"#;

        let (points, automation_items, extension_data) =
            Envelope::parse_envelope_content(rpp_content).unwrap();

        println!("\n🎵 Parsed Envelope Content from Raw RPP:");
        println!("{}", "=".repeat(60));

        // Display extension data
        for (i, ext_data) in extension_data.iter().enumerate() {
            println!("Extension Data {}: {}", i + 1, ext_data);
            println!("  Parameter Name: {}", ext_data.parmname);
            println!("  String Data: {}", ext_data.string_data);
            println!("  Raw Content: {}", ext_data.raw_content);
            println!();
        }

        // Display automation items
        for (i, automation_item) in automation_items.iter().enumerate() {
            println!("Automation Item {}: {}", i + 1, automation_item);
            println!("  Pool Index: {}", automation_item.pool_index);
            println!(
                "  Position: {:.6}s, Length: {:.6}s",
                automation_item.position, automation_item.length
            );
            println!(
                "  Start Offset: {:.6}s, Play Rate: {:.6}",
                automation_item.start_offset, automation_item.play_rate
            );
            println!(
                "  Selected: {}, Muted: {}",
                automation_item.selected, automation_item.muted
            );
            println!(
                "  Baseline: {:.6}, Amplitude: {:.6}",
                automation_item.baseline, automation_item.amplitude
            );
            println!("  Loop Enabled: {}", automation_item.loop_enabled);
            println!("  Transition Time: {:.6}s", automation_item.transition_time);
            println!(
                "  Volume Envelope Max: {}",
                automation_item.volume_envelope_max
            );
            println!();
        }

        // Display envelope points
        for (i, point) in points.iter().enumerate() {
            println!("Point {}: {}", i + 1, point);
            println!("  Shape: {}", point.shape);
            println!(
                "  Position: {:.6}s, Value: {:.6}",
                point.position, point.value
            );
            if let Some(ts) = point.time_sig {
                println!("  Time Signature: {}", ts);
            }
            if let Some(sel) = point.selected {
                println!("  Selected: {}", sel);
            }
            if let Some(unk) = point.unknown_field_6 {
                println!("  Unknown Field 6: {}", unk);
            }
            if let Some(tension) = point.bezier_tension {
                println!("  Bezier Tension: {:.6}", tension);
            }
            println!();
        }

        // Verify we got the expected number of extension data blocks
        assert_eq!(extension_data.len(), 0);

        // Verify we got the expected number of automation items
        assert_eq!(automation_items.len(), 1);

        // Verify the automation item
        let ai = &automation_items[0];
        assert_eq!(ai.pool_index, 1);
        assert_eq!(ai.position, 5.0);
        assert_eq!(ai.length, 1.5);
        assert_eq!(ai.start_offset, 0.0);
        assert_eq!(ai.play_rate, 1.0);
        assert!(ai.selected);
        assert_eq!(ai.baseline, 0.5);
        assert_eq!(ai.amplitude, 1.0);
        assert!(ai.loop_enabled);
        assert_eq!(ai.position_qn, 0.0);
        assert_eq!(ai.length_qn, 0.0);
        assert_eq!(ai.instance_index, 1);
        assert!(!ai.muted);
        assert_eq!(ai.start_offset_qn, 0.0);
        assert_eq!(ai.transition_time, 0.012);
        assert_eq!(ai.volume_envelope_max, 0);

        // Verify we got the expected number of points
        assert_eq!(points.len(), 5);

        // Verify the first point
        assert_eq!(points[0].position, 0.0);
        assert_eq!(points[0].value, 1.0);
        assert_eq!(points[0].shape, EnvelopePointShape::Linear);

        // Verify the second point (with all fields)
        assert_eq!(points[1].position, 2.0);
        assert_eq!(points[1].value, 0.02073425);
        assert_eq!(points[1].shape, EnvelopePointShape::Bezier);
        assert_eq!(points[1].time_sig, Some(1));
        assert_eq!(points[1].selected, Some(true));
        assert_eq!(points[1].unknown_field_6, Some(0));
        assert_eq!(points[1].bezier_tension, Some(0.3030303));

        println!(
            "✅ Successfully parsed {} extension blocks, {} automation items, and {} envelope points from raw RPP!",
            extension_data.len(),
            automation_items.len(),
            points.len()
        );
    }

    #[test]
    fn test_automation_item_parsing() {
        // Test parsing a single automation item line
        let line = "POOLEDENVINST 1 5 1.5 0 1 1 0.5 1 1 0 0 1 0 0 0.012 0";

        let automation_item = AutomationItem::from_rpp_line(line).unwrap();

        println!("\n🤖 Parsed Automation Item:");
        println!("{}", "=".repeat(40));
        println!("Raw line: {}", line);
        println!("Parsed: {}", automation_item);
        println!("Pool Index: {}", automation_item.pool_index);
        println!(
            "Position: {:.6}s, Length: {:.6}s",
            automation_item.position, automation_item.length
        );
        println!(
            "Start Offset: {:.6}s, Play Rate: {:.6}",
            automation_item.start_offset, automation_item.play_rate
        );
        println!(
            "Selected: {}, Muted: {}",
            automation_item.selected, automation_item.muted
        );
        println!(
            "Baseline: {:.6}, Amplitude: {:.6}",
            automation_item.baseline, automation_item.amplitude
        );
        println!("Loop Enabled: {}", automation_item.loop_enabled);
        println!("Transition Time: {:.6}s", automation_item.transition_time);
        println!(
            "Volume Envelope Max: {}",
            automation_item.volume_envelope_max
        );

        // Verify all fields
        assert_eq!(automation_item.pool_index, 1);
        assert_eq!(automation_item.position, 5.0);
        assert_eq!(automation_item.length, 1.5);
        assert_eq!(automation_item.start_offset, 0.0);
        assert_eq!(automation_item.play_rate, 1.0);
        assert!(automation_item.selected);
        assert_eq!(automation_item.baseline, 0.5);
        assert_eq!(automation_item.amplitude, 1.0);
        assert!(automation_item.loop_enabled);
        assert_eq!(automation_item.position_qn, 0.0);
        assert_eq!(automation_item.length_qn, 0.0);
        assert_eq!(automation_item.instance_index, 1);
        assert!(!automation_item.muted);
        assert_eq!(automation_item.start_offset_qn, 0.0);
        assert_eq!(automation_item.transition_time, 0.012);
        assert_eq!(automation_item.volume_envelope_max, 0);

        println!("✅ Automation item parsing successful!");
    }

    #[test]
    fn test_extension_data_parsing() {
        // Test parsing EXT blocks
        let ext_block_content = r#"<EXT
my_extension "some data with spaces"
>"#;

        let extension_data = ExtensionData::from_rpp_block(ext_block_content).unwrap();

        println!("\n🔧 Parsed Extension Data:");
        println!("{}", "=".repeat(40));
        println!("Raw block:\n{}", ext_block_content);
        println!("Parsed: {}", extension_data);
        println!("Parameter Name: {}", extension_data.parmname);
        println!("String Data: {}", extension_data.string_data);
        println!("Raw Content: {}", extension_data.raw_content);

        // Verify the extension data
        assert_eq!(extension_data.parmname, "my_extension");
        assert_eq!(extension_data.string_data, "\"some data with spaces\"");
        assert_eq!(extension_data.raw_content, ext_block_content);

        println!("✅ Extension data parsing successful!");
    }

    #[test]
    fn test_envelope_content_with_ext_blocks() {
        let rpp_content = r#"<VOLENV2
  EGUID {E1A0A26B-6BF4-C6BD-873D-DB57E2C20775}
  ACT 1 -1
  VIS 1 1 1
  LANEHEIGHT 113 0
  ARM 1
  DEFSHAPE 0 -1 -1
  VOLTYPE 1
  <EXT
  my_extension "some extension data"
  >
  POOLEDENVINST 1 5 1.5 0 1 1 0.5 1 1 0 0 1 0 0 0.012 0
  PT 0 1 0
  PT 2 0.02073425 5 1 1 0 0.3030303
  PT 3.98 0.25703958 0
  PT 4 0.22266414 0
  PT 4.02 0.25703958 0
>"#;

        let (points, automation_items, extension_data) =
            Envelope::parse_envelope_content(rpp_content).unwrap();

        println!("\n🎵 Parsed Envelope Content with EXT Blocks:");
        println!("{}", "=".repeat(60));

        // Display extension data
        for (i, ext_data) in extension_data.iter().enumerate() {
            println!("Extension Data {}: {}", i + 1, ext_data);
            println!("  Parameter Name: {}", ext_data.parmname);
            println!("  String Data: {}", ext_data.string_data);
            println!();
        }

        // Display automation items
        for (i, automation_item) in automation_items.iter().enumerate() {
            println!("Automation Item {}: {}", i + 1, automation_item);
            println!("  Pool Index: {}", automation_item.pool_index);
            println!();
        }

        // Display envelope points
        for (i, point) in points.iter().enumerate() {
            println!("Point {}: {}", i + 1, point);
            println!();
        }

        // Verify we got the expected counts
        assert_eq!(extension_data.len(), 1);
        assert_eq!(automation_items.len(), 1);
        assert_eq!(points.len(), 5);

        // Verify the extension data
        let ext = &extension_data[0];
        assert_eq!(ext.parmname, "my_extension");
        assert_eq!(ext.string_data, "\"some extension data\"");

        println!(
            "✅ Successfully parsed {} extension blocks, {} automation items, and {} envelope points!",
            extension_data.len(),
            automation_items.len(),
            points.len()
        );
    }
}

#[cfg(test)]
mod pooled_tests {
    use super::*;

    /// A pool written out reads back as it was, and only the project's own
    /// `<POOLEDENV>`s are read — not one nested deeper.
    #[test]
    fn a_pool_round_trips() {
        let pool = PooledEnvelope {
            id: 3,
            name: "Chorus".into(),
            source_length: 8.0,
            points: vec![
                EnvelopePoint::from_rpp_line("PT 0 0.5 2").expect("a point"),
                EnvelopePoint::from_rpp_line("PT 8 0.8 0").expect("a point"),
            ],
            other: vec!["LFO 0".into()],
        };
        let text = format!("<REAPER_PROJECT 0.1\n{pool}  <TRACK\n    <POOLEDENV\n    >\n  >\n>\n");
        let read = PooledEnvelope::all_in(&text);
        assert_eq!(read, vec![pool]);
    }
}
