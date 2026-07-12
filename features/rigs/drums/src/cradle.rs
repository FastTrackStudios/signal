//! Reader for **GGD Cradle** preset snapshots — the text format GGD Modern &
//! Massive 2 (and its siblings) save their mixer + FX in. A snapshot is a
//! Lua-table (`key = value`, `{ … }` tables, comma-separated, `--** … **--`
//! comment banners), obtained by exporting a `.preset` from the plugin or by
//! decoding the VST3 chunk out of a Reaper RPP.
//!
//! We parse the `audio.mixer` section: per-strip `level`/`pan`/`mute`/`solo`/
//! `phase`, the `cables` mic→strip routing, and each strip's `sends` + `fx`
//! chain — so MM2's per-piece mix can be replicated with our own FX on our own
//! samples. See the `mm2-cradle-preset-format` memory for the on-disk details.

use std::collections::BTreeMap;

/// A parsed Cradle (Lua-table) value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Lua `nil` (appears as placeholder elements in sparse arrays).
    Null,
    Str(String),
    Num(f64),
    Bool(bool),
    /// A positional array (`{ a, b, c }`).
    Arr(Vec<Value>),
    /// A keyed table (`{ k = v, … }`). Insertion order is not preserved; use
    /// [`Value::get`].
    Map(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m.get(key),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }
}

/// One mixer strip (a mic or a bus) as MM2 stores it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Strip {
    pub name: String,
    /// Linear fader gain (MM2 `level`; ~0..1+). Convert to dB with 20·log10.
    pub level: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    /// Polarity/phase rotation (0..1 in MM2; 1.0 ≈ inverted).
    pub phase: f32,
    /// FX chain as raw Cradle values (EQ / comp / verb) — mapped to our
    /// signal-fx by the importer once a populated example is available.
    pub fx: Vec<Value>,
    /// Sends as raw Cradle values.
    pub sends: Vec<Value>,
}

/// One FX-chain slot on a strip: the effect type, its bypass, the factory
/// preset name it came from, and the raw `fxData` params (typed access via the
/// helpers — MM2's param scaling is mapped onto our signal-fx at import time).
#[derive(Debug, Clone, PartialEq)]
pub struct FxSlot {
    /// "EQ", "Modern Compressor", "Vintage Compressor", "Transient", "Drive",
    /// "Reverb", "Limiter".
    pub fx_type: String,
    pub bypass: bool,
    /// `presetInfo.name`, e.g. "Drum Bus - Smacky".
    pub preset_name: String,
    /// Raw `fxData` (nested tables + numbers/strings).
    pub data: Value,
}

impl FxSlot {
    /// A numeric `fxData` field.
    pub fn num(&self, key: &str) -> Option<f64> {
        self.data.get(key).and_then(Value::as_f64)
    }
    /// A string `fxData` field (discrete params like comp attack "Fast").
    pub fn text(&self, key: &str) -> Option<&str> {
        self.data.get(key).and_then(Value::as_str)
    }
    /// EQ bands (`filters`), if this is an EQ.
    pub fn eq_bands(&self) -> Vec<EqBand> {
        self.data
            .get("filters")
            .and_then(Value::as_arr)
            .map(|arr| {
                arr.iter()
                    .map(|b| EqBand {
                        enabled: b.get("enabled").and_then(Value::as_f64).unwrap_or(1.0) != 0.0,
                        freq: b.get("frequency").and_then(Value::as_f64).unwrap_or(1000.0) as f32,
                        gain: b.get("gain").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        q: b.get("Q").and_then(Value::as_f64).unwrap_or(0.707) as f32,
                        mode: b.get("mode").and_then(Value::as_str).unwrap_or("bell").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// One parametric EQ band.
#[derive(Debug, Clone, PartialEq)]
pub struct EqBand {
    pub enabled: bool,
    pub freq: f32,
    pub gain: f32,
    pub q: f32,
    /// "bell", "lowShelf", "highShelf", "lowPass", "highPass", …
    pub mode: String,
}

impl Strip {
    /// The strip's FX chain as typed slots (order preserved).
    pub fn fx_slots(&self) -> Vec<FxSlot> {
        self.fx
            .iter()
            .filter_map(|v| {
                Some(FxSlot {
                    fx_type: v.get("fxType").and_then(Value::as_str)?.to_string(),
                    bypass: v.get("bypass").and_then(Value::as_f64).unwrap_or(0.0) != 0.0,
                    preset_name: v
                        .get("presetInfo")
                        .and_then(|p| p.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    data: v.get("fxData").cloned().unwrap_or(Value::Arr(Vec::new())),
                })
            })
            .collect()
    }
}

/// The parsed MM2 mixer: strips + the mic-input→strip routing.
#[derive(Debug, Clone, Default)]
pub struct Mixer {
    pub strips: Vec<Strip>,
    /// `(input_idx, strip_idx)` cables — which mic feeds which strip.
    pub cables: Vec<(u32, u32)>,
}

/// Parse the `{ … }` table that follows a `--** <banner>: **--` marker in a
/// Cradle snapshot. A snapshot is a sequence of such blocks (`Metadata:`,
/// `Project info:`, `Script state:`); the live mixer lives under
/// `Script state:`.
pub fn parse_block(text: &str, banner: &str) -> Result<Value, String> {
    let start = text.find(banner).ok_or_else(|| format!("no '{banner}' block"))?;
    let brace = text[start..].find('{').ok_or("no table after banner")? + start;
    let mut p = Parser { b: text.as_bytes(), i: brace };
    p.value()
}

/// Parse a whole snapshot and extract its mixer (from the `Script state:` block).
pub fn parse_mixer(text: &str) -> Result<Mixer, String> {
    let info = parse_block(text, "Script state:")?;
    let mixer = info.get("audio").and_then(|a| a.get("mixer")).ok_or("no audio.mixer")?;

    let mut strips = Vec::new();
    if let Some(arr) = mixer.get("strips").and_then(Value::as_arr) {
        for s in arr {
            strips.push(Strip {
                name: s.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                level: s.get("level").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                pan: s.get("pan").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                mute: s.get("mute").and_then(Value::as_f64).unwrap_or(0.0) != 0.0,
                solo: s.get("solo").and_then(Value::as_f64).unwrap_or(0.0) != 0.0,
                phase: s.get("phase").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                fx: s.get("fx").and_then(Value::as_arr).map(<[_]>::to_vec).unwrap_or_default(),
                sends: s.get("sends").and_then(Value::as_arr).map(<[_]>::to_vec).unwrap_or_default(),
            });
        }
    }

    let mut cables = Vec::new();
    if let Some(arr) = mixer.get("cables").and_then(|c| c.get("inputs")).and_then(Value::as_arr) {
        for c in arr {
            let inp = c.get("inputIdx").and_then(Value::as_f64).unwrap_or(0.0) as u32;
            let strip = c.get("stripIdx").and_then(Value::as_f64).unwrap_or(0.0) as u32;
            cables.push((inp, strip));
        }
    }

    Ok(Mixer { strips, cables })
}

// ── Minimal Lua-table parser ────────────────────────────────────────────────

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.table(),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') | Some(b'f') => self.boolean(),
            Some(b'n') => {
                // Lua `nil`.
                if self.b[self.i..].starts_with(b"nil") {
                    self.i += 3;
                    Ok(Value::Null)
                } else {
                    self.number()
                }
            }
            Some(_) => self.number(),
            None => Err("unexpected end of input".into()),
        }
    }

    fn table(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut arr: Vec<Value> = Vec::new();
        let mut map: BTreeMap<String, Value> = BTreeMap::new();
        loop {
            self.ws();
            match self.peek() {
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                None => return Err("unterminated table".into()),
                _ => {}
            }
            // A keyed entry is `key =` / `["key"] =` / `[n] =`; otherwise it's a
            // positional array element.
            if let Some(key) = self.try_key()? {
                let v = self.value()?;
                map.insert(key, v);
            } else {
                arr.push(self.value()?);
            }
            self.ws();
            if self.peek() == Some(b',') {
                self.i += 1;
            }
        }
        if map.is_empty() {
            Ok(Value::Arr(arr))
        } else {
            // Cradle tables are either all-keyed or all-array; if somehow mixed,
            // keyed wins (arr entries are ignored — not seen in practice).
            Ok(Value::Map(map))
        }
    }

    /// Try to parse a `key =` prefix at the current position; restore on miss.
    fn try_key(&mut self) -> Result<Option<String>, String> {
        let save = self.i;
        self.ws();
        let key = match self.peek() {
            Some(b'[') => {
                // `["name"]` or `[123]`
                self.i += 1;
                self.ws();
                let k = if self.peek() == Some(b'"') {
                    self.string()?
                } else {
                    // numeric index
                    let start = self.i;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                        self.i += 1;
                    }
                    String::from_utf8_lossy(&self.b[start..self.i]).into_owned()
                };
                self.ws();
                if self.peek() != Some(b']') {
                    self.i = save;
                    return Ok(None);
                }
                self.i += 1;
                k
            }
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                let start = self.i;
                while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') {
                    self.i += 1;
                }
                String::from_utf8_lossy(&self.b[start..self.i]).into_owned()
            }
            _ => {
                self.i = save;
                return Ok(None);
            }
        };
        self.ws();
        if self.peek() == Some(b'=') {
            self.i += 1;
            Ok(Some(key))
        } else {
            self.i = save;
            Ok(None)
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.next() {
                Some(b'"') => break,
                Some(b'\\') => match self.next() {
                    Some(b'n') => out.push('\n'),
                    Some(b't') => out.push('\t'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'"') => out.push('"'),
                    Some(c) => out.push(c as char),
                    None => return Err("unterminated escape".into()),
                },
                Some(c) => out.push(c as char),
                None => return Err("unterminated string".into()),
            }
        }
        Ok(out)
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || matches!(c, b'-'|b'+'|b'.'|b'e'|b'E')) {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        s.parse::<f64>().map(Value::Num).map_err(|_| {
            let ctx = String::from_utf8_lossy(&self.b[start..(start + 40).min(self.b.len())]);
            format!("bad number {s:?} at {start}; context: {ctx:?}")
        })
    }

    fn boolean(&mut self) -> Result<Value, String> {
        if self.b[self.i..].starts_with(b"true") {
            self.i += 4;
            Ok(Value::Bool(true))
        } else if self.b[self.i..].starts_with(b"false") {
            self.i += 5;
            Ok(Value::Bool(false))
        } else {
            Err("bad literal".into())
        }
    }

    // ── cursor helpers ──
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.i += 1;
        }
        c
    }
    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected {:?} at {}", c as char, self.i))
        }
    }
    /// Skip whitespace and `--` line comments (Lua-style; covers the `--** … **--` banners).
    fn ws(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => self.i += 1,
                Some(b'-') if self.b.get(self.i + 1) == Some(&b'-') => {
                    while !matches!(self.peek(), Some(b'\n') | None) {
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAP: &str = "--** Cradle snapshot 0.7.10 **--\n\
--** Project info: **--\n{ name = \"x\" }\n\
,--** Script state: **--\n{\n\
   audio = {\n\
      mixer = {\n\
         cables = { inputs = { { inputIdx = 1, stripIdx = 2 }, { inputIdx = 2, stripIdx = 3 } } },\n\
         strips = {\n\
            { name = \"Master Bus\", level = 0.75, pan = 0, mute = 0, solo = 0, phase = 0, fx = {}, sends = {} },\n\
            { name = \"Kick In 1\", level = 0.7071, pan = -0.5, mute = 1, solo = 0, phase = 0.7, fx = {}, sends = {} }\n\
         }\n\
      }\n\
   }\n\
}\n";

    #[test]
    fn parses_mixer_strips_and_cables() {
        let m = parse_mixer(SNAP).expect("parse");
        assert_eq!(m.cables, vec![(1, 2), (2, 3)]);
        assert_eq!(m.strips.len(), 2);
        assert_eq!(m.strips[0].name, "Master Bus");
        assert!((m.strips[1].level - 0.7071).abs() < 1e-4);
        assert!((m.strips[1].pan + 0.5).abs() < 1e-6);
        assert!(m.strips[1].mute);
        assert!(!m.strips[1].solo);
    }
}
