use std::collections::BTreeMap;

#[derive(Debug)]
struct Args {
    actual: String,
    expected: String,
    precision: usize,
    json: bool,
}

#[derive(Debug, serde::Serialize)]
struct DiffResult {
    equal: bool,
    actual_path: String,
    expected_path: String,
    precision: usize,
    actual_elements: usize,
    expected_elements: usize,
    actual_paths: usize,
    expected_paths: usize,
    actual_canonical_len: usize,
    expected_canonical_len: usize,
    first_diff_offset: Option<usize>,
    first_diff_actual: Option<String>,
    first_diff_expected: Option<String>,
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(err) => {
            eprintln!("svg_canonical_diff error: {err}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let actual = std::fs::read_to_string(&args.actual)
        .map_err(|e| format!("failed reading actual SVG {}: {e}", args.actual))?;
    let expected = std::fs::read_to_string(&args.expected)
        .map_err(|e| format!("failed reading expected SVG {}: {e}", args.expected))?;

    let (actual_sig, actual_elements, actual_paths) = canonicalize_svg(&actual, args.precision)?;
    let (expected_sig, expected_elements, expected_paths) =
        canonicalize_svg(&expected, args.precision)?;

    let (first_diff_offset, first_diff_actual, first_diff_expected) =
        first_diff(&actual_sig, &expected_sig, 80);

    let result = DiffResult {
        equal: actual_sig == expected_sig,
        actual_path: args.actual,
        expected_path: args.expected,
        precision: args.precision,
        actual_elements,
        expected_elements,
        actual_paths,
        expected_paths,
        actual_canonical_len: actual_sig.len(),
        expected_canonical_len: expected_sig.len(),
        first_diff_offset,
        first_diff_actual,
        first_diff_expected,
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string(&result)
                .map_err(|e| format!("failed serializing result as JSON: {e}"))?
        );
    } else {
        println!(
            "equal={} elements={}/{} paths={}/{} len={}/{}",
            result.equal,
            result.actual_elements,
            result.expected_elements,
            result.actual_paths,
            result.expected_paths,
            result.actual_canonical_len,
            result.expected_canonical_len
        );
        if let Some(offset) = result.first_diff_offset {
            println!("first_diff_offset={offset}");
            if let Some(actual) = &result.first_diff_actual {
                println!("actual_snippet={actual}");
            }
            if let Some(expected) = &result.first_diff_expected {
                println!("expected_snippet={expected}");
            }
        }
    }

    if result.equal {
        Ok(())
    } else {
        std::process::exit(2)
    }
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let actual = args.next().ok_or_else(|| {
        "usage: svg_canonical_diff <actual.svg> <expected.svg> [--precision N] [--json]".to_string()
    })?;
    let expected = args.next().ok_or_else(|| {
        "usage: svg_canonical_diff <actual.svg> <expected.svg> [--precision N] [--json]".to_string()
    })?;

    let mut precision = 4usize;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--precision" => {
                let Some(raw) = args.next() else {
                    return Err("--precision requires an integer value".to_string());
                };
                precision = raw
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --precision value: {e}"))?;
            }
            "--json" => json = true,
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    Ok(Args {
        actual,
        expected,
        precision,
        json,
    })
}

fn canonicalize_svg(input: &str, precision: usize) -> Result<(String, usize, usize), String> {
    let doc =
        roxmltree::Document::parse(input).map_err(|e| format!("invalid XML/SVG document: {e}"))?;
    let root = doc.root_element();
    let mut sig = String::new();
    let mut element_count = 0usize;
    let mut path_count = 0usize;

    canonicalize_node(
        root,
        precision,
        &mut sig,
        &mut element_count,
        &mut path_count,
    );

    Ok((sig, element_count, path_count))
}

fn canonicalize_node(
    node: roxmltree::Node<'_, '_>,
    precision: usize,
    out: &mut String,
    element_count: &mut usize,
    path_count: &mut usize,
) {
    if !node.is_element() {
        return;
    }

    *element_count += 1;
    if node.tag_name().name() == "path" {
        *path_count += 1;
    }

    out.push('<');
    out.push_str(node.tag_name().name());

    let mut attrs: BTreeMap<String, String> = BTreeMap::new();
    for attr in node.attributes() {
        let key = attr.name().to_string();
        let value = normalize_attr_value(attr.name(), attr.value(), precision);
        attrs.insert(key, value);
    }
    for (k, v) in attrs {
        out.push('|');
        out.push_str(&k);
        out.push('=');
        out.push_str(&v);
    }

    out.push('>');

    for child in node.children() {
        if child.is_element() {
            canonicalize_node(child, precision, out, element_count, path_count);
        } else if child.is_text() {
            let text = collapse_ws(child.text().unwrap_or_default());
            if !text.is_empty() {
                out.push_str("#text:");
                out.push_str(&text);
                out.push(';');
            }
        }
    }

    out.push_str("</");
    out.push_str(node.tag_name().name());
    out.push('>');
}

fn normalize_attr_value(name: &str, value: &str, precision: usize) -> String {
    let collapsed = collapse_ws(value);
    match name {
        "d" => normalize_path_data(&collapsed, precision),
        "viewBox" | "x" | "y" | "width" | "height" | "opacity" | "fill-opacity"
        | "stroke-opacity" | "stroke-width" | "rx" | "ry" => {
            normalize_numbers(&collapsed, precision)
        }
        _ => collapsed,
    }
}

fn normalize_path_data(input: &str, precision: usize) -> String {
    canonicalize_path_data(input, precision).unwrap_or_else(|| normalize_numbers(input, precision))
}

fn canonicalize_path_data(input: &str, precision: usize) -> Option<String> {
    let tokens = tokenize_path(input);
    if tokens.is_empty() {
        return Some(String::new());
    }

    let mut i = 0usize;
    let mut out = String::new();
    let mut curr_x = 0.0f64;
    let mut curr_y = 0.0f64;
    let mut subpath_start_x = 0.0f64;
    let mut subpath_start_y = 0.0f64;

    while i < tokens.len() {
        let cmd_tok = tokens.get(i)?;
        let cmd = cmd_tok.chars().next()?;
        if !is_path_cmd(cmd) {
            return None;
        }
        i += 1;
        let rel = cmd.is_ascii_lowercase();
        let upper = cmd.to_ascii_uppercase();

        let mut nums = Vec::<f64>::new();
        while i < tokens.len() {
            let t = tokens.get(i)?;
            let c = t.chars().next()?;
            if is_path_cmd(c) {
                break;
            }
            nums.push(t.parse::<f64>().ok()?);
            i += 1;
        }

        match upper {
            'M' => {
                if nums.len() < 2 || nums.len() % 2 != 0 {
                    return None;
                }
                for (pair_idx, pair) in nums.chunks_exact(2).enumerate() {
                    let mut x = pair[0];
                    let mut y = pair[1];
                    if rel {
                        x += curr_x;
                        y += curr_y;
                    }
                    if pair_idx == 0 {
                        push_cmd(&mut out, 'M', &[x, y], precision);
                        subpath_start_x = x;
                        subpath_start_y = y;
                    } else {
                        push_cmd(&mut out, 'L', &[x, y], precision);
                    }
                    curr_x = x;
                    curr_y = y;
                }
            }
            'L' => {
                if nums.len() % 2 != 0 {
                    return None;
                }
                for pair in nums.chunks_exact(2) {
                    let mut x = pair[0];
                    let mut y = pair[1];
                    if rel {
                        x += curr_x;
                        y += curr_y;
                    }
                    push_cmd(&mut out, 'L', &[x, y], precision);
                    curr_x = x;
                    curr_y = y;
                }
            }
            'H' => {
                for value in nums {
                    let x = if rel { curr_x + value } else { value };
                    push_cmd(&mut out, 'L', &[x, curr_y], precision);
                    curr_x = x;
                }
            }
            'V' => {
                for value in nums {
                    let y = if rel { curr_y + value } else { value };
                    push_cmd(&mut out, 'L', &[curr_x, y], precision);
                    curr_y = y;
                }
            }
            'C' => {
                if nums.len() % 6 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(6) {
                    let mut vals = [seg[0], seg[1], seg[2], seg[3], seg[4], seg[5]];
                    if rel {
                        vals[0] += curr_x;
                        vals[1] += curr_y;
                        vals[2] += curr_x;
                        vals[3] += curr_y;
                        vals[4] += curr_x;
                        vals[5] += curr_y;
                    }
                    push_cmd(&mut out, 'C', &vals, precision);
                    curr_x = vals[4];
                    curr_y = vals[5];
                }
            }
            'S' => {
                if nums.len() % 4 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(4) {
                    let mut vals = [seg[0], seg[1], seg[2], seg[3]];
                    if rel {
                        vals[0] += curr_x;
                        vals[1] += curr_y;
                        vals[2] += curr_x;
                        vals[3] += curr_y;
                    }
                    push_cmd(&mut out, 'S', &vals, precision);
                    curr_x = vals[2];
                    curr_y = vals[3];
                }
            }
            'Q' => {
                if nums.len() % 4 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(4) {
                    let mut vals = [seg[0], seg[1], seg[2], seg[3]];
                    if rel {
                        vals[0] += curr_x;
                        vals[1] += curr_y;
                        vals[2] += curr_x;
                        vals[3] += curr_y;
                    }
                    push_cmd(&mut out, 'Q', &vals, precision);
                    curr_x = vals[2];
                    curr_y = vals[3];
                }
            }
            'T' => {
                if nums.len() % 2 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(2) {
                    let mut x = seg[0];
                    let mut y = seg[1];
                    if rel {
                        x += curr_x;
                        y += curr_y;
                    }
                    push_cmd(&mut out, 'T', &[x, y], precision);
                    curr_x = x;
                    curr_y = y;
                }
            }
            'A' => {
                if nums.len() % 7 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(7) {
                    let mut vals = [seg[0], seg[1], seg[2], seg[3], seg[4], seg[5], seg[6]];
                    if rel {
                        vals[5] += curr_x;
                        vals[6] += curr_y;
                    }
                    push_cmd(&mut out, 'A', &vals, precision);
                    curr_x = vals[5];
                    curr_y = vals[6];
                }
            }
            'Z' => {
                push_cmd(&mut out, 'Z', &[], precision);
                curr_x = subpath_start_x;
                curr_y = subpath_start_y;
            }
            _ => return None,
        }
    }

    Some(out)
}

fn push_cmd(out: &mut String, cmd: char, values: &[f64], precision: usize) {
    out.push(cmd);
    for (idx, value) in values.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        out.push_str(&format_num(*value, precision));
    }
}

fn tokenize_path(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        if is_path_cmd(c) {
            out.push(c.to_string());
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < chars.len() {
            let ch = chars[i];
            if is_path_cmd(ch) || ch.is_ascii_whitespace() || ch == ',' {
                break;
            }
            i += 1;
        }
        out.push(chars[start..i].iter().collect::<String>());
    }

    out
}

const fn is_path_cmd(c: char) -> bool {
    matches!(
        c,
        'M' | 'm'
            | 'L'
            | 'l'
            | 'H'
            | 'h'
            | 'V'
            | 'v'
            | 'C'
            | 'c'
            | 'S'
            | 's'
            | 'Q'
            | 'q'
            | 'T'
            | 't'
            | 'A'
            | 'a'
            | 'Z'
            | 'z'
    )
}

fn normalize_numbers(input: &str, precision: usize) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        let c = bytes[i] as char;
        if is_number_start(c) {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let ch = bytes[i] as char;
                if ch.is_ascii_digit() || matches!(ch, '.' | 'e' | 'E' | '+' | '-') {
                    i += 1;
                } else {
                    break;
                }
            }
            let token = &input[start..i];
            if let Ok(num) = token.parse::<f64>() {
                out.push_str(&format_num(num, precision));
            } else {
                out.push_str(token);
            }
            continue;
        }

        out.push(c);
        i += 1;
    }

    collapse_ws(&out)
}

fn is_number_start(c: char) -> bool {
    c.is_ascii_digit() || c == '-' || c == '+' || c == '.'
}

fn format_num(mut value: f64, precision: usize) -> String {
    if value == -0.0 {
        value = 0.0;
    }
    let scale = 10f64.powi(precision as i32);
    let rounded = (value * scale).round() / scale;
    let s = format!("{rounded:.precision$}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn collapse_ws(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut saw_ws = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            saw_ws = true;
            continue;
        }
        if saw_ws && !out.is_empty() {
            out.push(' ');
        }
        saw_ws = false;
        out.push(ch);
    }
    out
}

fn first_diff(
    actual: &str,
    expected: &str,
    context: usize,
) -> (Option<usize>, Option<String>, Option<String>) {
    let a = actual.as_bytes();
    let e = expected.as_bytes();
    let min_len = a.len().min(e.len());
    let mut idx = 0usize;
    while idx < min_len && a[idx] == e[idx] {
        idx += 1;
    }

    if idx == min_len && a.len() == e.len() {
        return (None, None, None);
    }

    let start = idx.saturating_sub(context);
    let end_a = (idx + context).min(actual.len());
    let end_e = (idx + context).min(expected.len());

    (
        Some(idx),
        Some(actual[start..end_a].to_string()),
        Some(expected[start..end_e].to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_numbers() {
        let got = normalize_numbers("M25.53409 47.09090L0.50000 1", 4);
        assert_eq!(got, "M25.5341 47.0909L0.5 1");
    }

    #[test]
    fn canonical_ignores_attribute_order() {
        let a = r#"<svg><path fill="black" d="M0 0 L1 1"/></svg>"#;
        let b = r#"<svg><path d="M0 0 L1 1" fill="black"/></svg>"#;
        let (sa, _, _) = canonicalize_svg(a, 4).unwrap();
        let (sb, _, _) = canonicalize_svg(b, 4).unwrap();
        assert_eq!(sa, sb);
    }
}
