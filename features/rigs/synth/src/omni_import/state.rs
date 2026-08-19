//! **Omnisphere VST3 state chunks** — build and parse the plugin's state so
//! signal can drive a hosted Omnisphere directly (patch switching, A/B
//! calibration renders).
//!
//! The chunk is a JUCE plugin-state wrapper (verified byte-identical against
//! a real `save_state` dump from Omnisphere 3.0.0b10):
//!
//! ```text
//! bytes 0..4    "DAW3"
//! bytes 4..8    u32le: length of everything after byte 12
//! bytes 8..12   u32le: 999_999_999 (magic)
//! bytes 12..24  u32le × 3: 0, 1, 0
//! bytes 24..28  u32le: XML payload length (payload ends "… \0")
//! bytes 28..32  u32le: 0
//! bytes 32..    the SynthMaster Multi XML (same dialect as .mlt_omn)
//! trailer       20 zero bytes + "JUCEPrivateData\0" + 3 zero bytes
//! ```

const MAGIC: u32 = 999_999_999;
const TRAILER: &[u8] = &[
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 20 zeros
    b'J', b'U', b'C', b'E', b'P', b'r', b'i', b'v', b'a', b't', b'e', b'D', b'a', b't', b'a', 0, 0,
    0, 0,
];

/// Wrap a `SynthMaster` Multi XML (`.mlt_omn` content) into a VST3 state
/// chunk Omnisphere accepts via `load_state`.
pub fn build_state(multi_xml: &str) -> Vec<u8> {
    let mut payload = multi_xml
        .trim_end_matches(['\0', ' ', '\n'])
        .as_bytes()
        .to_vec();
    payload.extend_from_slice(b" \0");
    let counted_len = 20 + payload.len() + TRAILER.len();
    let mut out = Vec::with_capacity(12 + counted_len);
    out.extend_from_slice(b"DAW3");
    out.extend_from_slice(&(counted_len as u32).to_le_bytes());
    out.extend_from_slice(&MAGIC.to_le_bytes());
    for v in [0u32, 1, 0, payload.len() as u32, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&payload);
    out.extend_from_slice(TRAILER);
    out
}

/// Extract the Multi XML from a state chunk (the inverse of [`build_state`]).
///
/// Accepts both spellings of the chunk. `save_state` writes the full form
/// above; `IComponent::getState` — what a host stores, and so what comes out
/// of a Gig Performer file via [`crate::gig`] — omits the leading `"DAW3"` and
/// length words and starts at the magic. Same body either way.
pub fn parse_state(chunk: &[u8]) -> Result<String, String> {
    let body = if chunk.len() >= 12 && &chunk[0..4] == b"DAW3" {
        &chunk[8..]
    } else {
        chunk
    };
    if body.len() < 24 || u32::from_le_bytes(body[0..4].try_into().unwrap()) != MAGIC {
        return Err("not an Omnisphere state chunk".into());
    }
    let xml_len = u32::from_le_bytes(body[16..20].try_into().unwrap()) as usize;
    let xml = body.get(24..24 + xml_len).ok_or("truncated state chunk")?;
    Ok(String::from_utf8_lossy(xml)
        .trim_end_matches(['\0', ' '])
        .to_string())
}

/// Splice a patch's `<SynthEngine>` subtree into Part 1 of a template Multi
/// XML — how a single `.prt_omn` becomes loadable plugin state.
pub fn patch_into_multi(patch_xml: &str, template_multi_xml: &str) -> Result<String, String> {
    let engine = {
        let start = patch_xml
            .find("<SynthEngine")
            .ok_or("patch has no SynthEngine")?;
        let end = patch_xml
            .rfind("</SynthEngine>")
            .ok_or("patch SynthEngine unterminated")?
            + "</SynthEngine>".len();
        &patch_xml[start..end]
    };
    let sub = template_multi_xml
        .find("<SynthSubEngine")
        .ok_or("template has no SynthSubEngine")?;
    let start = template_multi_xml[sub..]
        .find("<SynthEngine")
        .map(|o| sub + o)
        .ok_or("template part 1 has no SynthEngine")?;
    let end = template_multi_xml[start..]
        .find("</SynthEngine>")
        .map(|o| start + o + "</SynthEngine>".len())
        .ok_or("template SynthEngine unterminated")?;
    Ok(format!(
        "{}{}{}",
        &template_multi_xml[..start],
        engine,
        &template_multi_xml[end..]
    ))
}

/// Rewrite every `attr="…"` occurrence to `attr="value"` — the calibration
/// sweep hook (values are raw attribute strings, IEEE-754 hex for floats).
pub fn rewrite_attr(xml: &str, attr: &str, value: &str) -> (String, usize) {
    let needle = format!("{attr}=\"");
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    let mut count = 0;
    while let Some(pos) = rest.find(&needle) {
        // Attribute boundary: start-of-input, whitespace or tag-open before
        // the name (so "rels" doesn't match inside "barrels").
        let boundary = pos == 0
            || rest[..pos]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace() || c == '<');
        let val_start = pos + needle.len();
        let Some(close) = rest[val_start..].find('"') else {
            break;
        };
        let val_end = val_start + close; // index of the closing quote
        if boundary {
            out.push_str(&rest[..val_start]);
            out.push_str(value);
            count += 1;
        } else {
            out.push_str(&rest[..val_end]);
        }
        rest = &rest[val_end..]; // closing quote onward
    }
    out.push_str(rest);
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrips() {
        let xml = r#"<SynthMaster vers="3.0.0b10"><SynthSubEngine><SynthEngine a="1"></SynthEngine></SynthSubEngine></SynthMaster>"#;
        let chunk = build_state(xml);
        assert_eq!(&chunk[0..4], b"DAW3");
        assert_eq!(
            u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as usize,
            chunk.len() - 12
        );
        assert_eq!(
            u32::from_le_bytes(chunk[8..12].try_into().unwrap()),
            999_999_999
        );
        assert!(
            chunk.ends_with(&[b'a', 0, 0, 0, 0][..])
                || chunk.windows(15).any(|w| w == b"JUCEPrivateData")
        );
        let back = parse_state(&chunk).unwrap();
        assert_eq!(back.trim_end(), xml);
        // Rebuilding from the parsed XML is byte-identical (stable format).
        assert_eq!(build_state(&back), chunk);
    }

    #[test]
    fn patch_splices_into_part_one() {
        let patch = r#"<AmberPart ><SynthEngine patched="yes"><SYNTHENG></SYNTHENG></SynthEngine></AmberPart>"#;
        let multi = r#"<SynthMaster ><SynthSubEngine><SynthEngine old="1"></SynthEngine></SynthSubEngine> <SynthSubEngine><SynthEngine old="2"></SynthEngine></SynthSubEngine></SynthMaster>"#;
        let out = patch_into_multi(patch, multi).unwrap();
        assert!(out.contains(r#"patched="yes""#), "part 1 replaced");
        assert!(!out.contains(r#"old="1""#), "old part 1 gone");
        assert!(out.contains(r#"old="2""#), "part 2 untouched");
    }

    #[test]
    fn attr_rewrites_all_boundary_matches() {
        let xml = r#"<A rels="1" ><B rels="2" barrels="3" ></B></A>"#;
        let (out, n) = rewrite_attr(xml, "rels", "3f000000");
        assert_eq!(n, 2, "two real sites; 'barrels' untouched");
        assert!(out.contains(r#"<A rels="3f000000" >"#));
        assert!(out.contains(r#"<B rels="3f000000" barrels="3" >"#));
    }
}
