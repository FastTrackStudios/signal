//! Minimal XML parser for the Spectrasonics "AmberPart" dialect —
//! attribute-only elements, IEEE-754 hex-bit floats, basic entities.

// ── Minimal XML ──────────────────────────────────────────────────────────────

/// One parsed element. The dialect uses no text content — only nested
/// elements and attributes — so text is ignored.
#[derive(Debug, Clone)]
pub struct XmlNode {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
}

impl XmlNode {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Decode a numeric attribute (hex-bits float or decimal integer).
    pub fn num(&self, name: &str) -> Option<f32> {
        self.attr(name).map(omni_num)
    }

    pub fn child(&self, tag: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.tag == tag)
    }

    pub fn children_tagged<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a XmlNode> {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    /// Depth-first search for the first element with `tag`.
    pub fn find(&self, tag: &str) -> Option<&XmlNode> {
        if self.tag == tag {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(tag))
    }
}

/// Decode an attribute value: 8 hex digits → `f32` from bits; otherwise a
/// plain decimal number; otherwise 0.
pub fn omni_num(s: &str) -> f32 {
    let t = s.trim();
    if t.len() == 8 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        if let Ok(bits) = u32::from_str_radix(t, 16) {
            let f = f32::from_bits(bits);
            if f.is_finite() {
                return f;
            }
        }
    }
    t.parse::<f32>().unwrap_or(0.0)
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let end = match rest.find(';') {
            Some(e) if e <= 12 => e,
            _ => {
                out.push('&');
                rest = &rest[1..];
                continue;
            }
        };
        let ent = &rest[1..end];
        match ent {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if ent.starts_with('#') => {
                let n = if let Some(hex) = ent.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    ent[1..].parse::<u32>().ok()
                };
                match n.and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[..=end]),
                }
            }
            _ => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Parse the AmberPart XML dialect (elements + attributes only; comments,
/// PIs and text content are skipped).
pub fn parse_xml(input: &str) -> Result<XmlNode, String> {
    let b = input.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;

    fn skip_ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    }

    while i < b.len() {
        // Find the next tag; ignore any stray text between elements.
        match b[i..].iter().position(|&c| c == b'<') {
            Some(off) => i += off,
            None => break,
        }
        if b[i..].starts_with(b"<?") || b[i..].starts_with(b"<!--") {
            let close: &[u8] = if b[i..].starts_with(b"<?") {
                b"?>"
            } else {
                b"-->"
            };
            match b[i..].windows(close.len()).position(|w| w == close) {
                Some(off) => {
                    i += off + close.len();
                    continue;
                }
                None => return Err("unterminated <? or <!--".into()),
            }
        }
        if b[i..].starts_with(b"</") {
            // Closing tag: pop.
            let end = b[i..]
                .iter()
                .position(|&c| c == b'>')
                .ok_or("unterminated close tag")?;
            i += end + 1;
            let done = stack.pop().ok_or("unbalanced close tag")?;
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => {
                    root = Some(done);
                    break;
                }
            }
            continue;
        }
        // Opening tag.
        i += 1;
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        let tag = std::str::from_utf8(&b[start..i])
            .map_err(|_| "bad utf8 in tag")?
            .to_string();
        let mut node = XmlNode {
            tag,
            attrs: Vec::new(),
            children: Vec::new(),
        };
        // Attributes.
        loop {
            skip_ws(b, &mut i);
            if i >= b.len() {
                return Err("unterminated tag".into());
            }
            if b[i] == b'>' {
                i += 1;
                stack.push(node);
                break;
            }
            if b[i] == b'/' {
                // Self-closing.
                i += 1;
                if i < b.len() && b[i] == b'>' {
                    i += 1;
                }
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => root = Some(node),
                }
                break;
            }
            let astart = i;
            while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
                i += 1;
            }
            let name = std::str::from_utf8(&b[astart..i])
                .map_err(|_| "bad utf8 in attr")?
                .to_string();
            skip_ws(b, &mut i);
            if i < b.len() && b[i] == b'=' {
                i += 1;
                skip_ws(b, &mut i);
                if i >= b.len() || b[i] != b'"' {
                    return Err(format!("attr {name} missing quote"));
                }
                i += 1;
                let vstart = i;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
                let value =
                    std::str::from_utf8(&b[vstart..i]).map_err(|_| "bad utf8 in attr value")?;
                i += 1; // closing quote
                node.attrs.push((name, decode_entities(value)));
            } else {
                node.attrs.push((name, String::new()));
            }
        }
        if root.is_some() {
            break;
        }
    }
    root.or_else(|| stack.pop())
        .ok_or_else(|| "no root element".into())
}
