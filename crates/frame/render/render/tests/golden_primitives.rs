//! Golden paint primitives for every parity scenario.
//!
//! Each scenario's export is imported (`figma-import`, test-only) and its
//! first page is lowered with `build_scene_primitives`; the result must match
//! `features/render/tests/parity/golden/<scenario>.primitives.txt`.
//!
//! Golden format: one `{:?}` primitive per line, node ids replaced by their
//! source (Figma) ids (scene ids are random per import), and quoted strings
//! longer than 256 bytes (image / svg payloads, long path data) replaced by
//! their length and FNV-1a hash, floats rounded to 12 significant digits.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test -p render --test golden_primitives`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn parity_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/parity")
}

fn scenarios() -> Vec<(String, PathBuf)> {
    let dir = parity_dir().join("scenarios");
    let mut out: Vec<_> = std::fs::read_dir(&dir)
        .expect("parity scenarios dir")
        .filter_map(|e| {
            let d = e.ok()?.path();
            let json = [
                d.join("latest-bridge-export.json"),
                d.join("export/latest-bridge-export.json"),
            ]
            .into_iter()
            .find(|p| p.exists())?;
            Some((d.file_name()?.to_string_lossy().to_string(), json))
        })
        .collect();
    out.sort();
    out
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A float rounded to 12 significant digits: the JSON importer's last-bit
/// precision depends on workspace feature unification (`serde_json`'s
/// float parsing), which is not what this test pins.
fn round_float(token: &str) -> String {
    match token.parse::<f64>() {
        Ok(v) if v.is_finite() => {
            let rounded: f64 = format!("{v:.11e}").parse().unwrap();
            format!("{:?}", if rounded == 0.0 { 0.0 } else { rounded })
        }
        _ => token.to_string(),
    }
}

/// Canonical golden text: quoted strings longer than 256 bytes become
/// `"<len N fnv H>"`, floats outside strings are rounded (`round_float`).
fn canonicalize(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            let starts_number = (bytes[i].is_ascii_digit()
                || (bytes[i] == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)))
                && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'));
            if starts_number {
                let mut j = i + 1;
                while j < bytes.len()
                    && (bytes[j].is_ascii_digit()
                        || bytes[j] == b'.'
                        || bytes[j] == b'e'
                        || (bytes[j] == b'-' && bytes[j - 1] == b'e'))
                {
                    j += 1;
                }
                let token = &s[i..j];
                if token.contains(['.', 'e']) {
                    out.push_str(&round_float(token));
                } else {
                    out.push_str(token);
                }
                i = j;
            } else {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && bytes[j] != b'"' {
            j += if bytes[j] == b'\\' { 2 } else { 1 };
        }
        let body = &s[i + 1..j.min(bytes.len())];
        if body.len() > 256 {
            let _ = write!(
                out,
                "\"<len {} fnv {:016x}>\"",
                body.len(),
                fnv1a(body.as_bytes())
            );
        } else {
            out.push('"');
            out.push_str(body);
            out.push('"');
        }
        i = j + 1;
    }
    out
}

fn render_golden(json: &Path) -> String {
    let scene = figma_import::import_bytes(&std::fs::read(json).unwrap()).unwrap();
    let root = scene.pages().first().copied().unwrap_or(scene.root);
    let names: HashMap<String, String> = scene
        .nodes
        .values()
        .map(|n| {
            let src = n
                .source
                .as_ref()
                .map_or_else(|| "?".to_string(), |s| s.id.clone());
            (format!("NodeId({})", n.id.0), format!("NodeId({src:?})"))
        })
        .collect();
    let mut out = String::new();
    for p in render::build_scene_primitives(&scene, root) {
        let mut line = format!("{p:?}");
        while let Some(start) = line.find("NodeId(") {
            let end = start + line[start..].find(')').unwrap() + 1;
            let key = &line[start..end];
            let Some(name) = names.get(key) else {
                panic!("unknown node id {key}")
            };
            line.replace_range(start..end, &name.replacen("NodeId(", "NodeID(", 1));
        }
        out.push_str(&canonicalize(&line.replace("NodeID(", "NodeId(")));
        out.push('\n');
    }
    out
}

#[test]
fn scene_primitives_match_golden() {
    let all = scenarios();
    assert!(
        all.len() >= 20,
        "expected the parity corpus, found {} scenarios",
        all.len()
    );
    let golden_dir = parity_dir().join("golden");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut failures = Vec::new();
    for (name, json) in &all {
        let actual = render_golden(json);
        let path = golden_dir.join(format!("{name}.primitives.txt"));
        if update {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&path, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing golden {}: {e}", path.display()));
        if actual != expected {
            let at = actual
                .lines()
                .zip(expected.lines())
                .position(|(a, b)| a != b);
            failures.push(format!(
                "{name}: primitives differ (actual {} / golden {} lines) at line {}\n  golden: {:?}\n  actual: {:?}",
                actual.lines().count(),
                expected.lines().count(),
                at.map_or(actual.lines().count().min(expected.lines().count()), |a| a) + 1,
                at.and_then(|a| expected.lines().nth(a)),
                at.and_then(|a| actual.lines().nth(a)),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scenarios differ:\n{}",
        failures.len(),
        all.len(),
        failures.join("\n")
    );
}
