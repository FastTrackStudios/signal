use std::collections::BTreeMap;

#[derive(Debug)]
struct Args {
    actual: String,
    expected: String,
    abs_tol: f64,
    rel_tol: f64,
    json: bool,
}

#[derive(Debug, serde::Serialize)]
struct DiffResult {
    equal: bool,
    actual_path: String,
    expected_path: String,
    abs_tol: f64,
    rel_tol: f64,
    first_mismatch: Option<Mismatch>,
}

#[derive(Debug, serde::Serialize)]
struct Mismatch {
    node_path: String,
    field: String,
    detail: String,
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(err) => {
            eprintln!("svg_structure_diff error: {err}");
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

    let actual_doc =
        roxmltree::Document::parse(&actual).map_err(|e| format!("invalid actual SVG: {e}"))?;
    let expected_doc =
        roxmltree::Document::parse(&expected).map_err(|e| format!("invalid expected SVG: {e}"))?;

    let mismatch = compare_node(
        actual_doc.root_element(),
        expected_doc.root_element(),
        "/svg",
        args.abs_tol,
        args.rel_tol,
    );

    let result = DiffResult {
        equal: mismatch.is_none(),
        actual_path: args.actual,
        expected_path: args.expected,
        abs_tol: args.abs_tol,
        rel_tol: args.rel_tol,
        first_mismatch: mismatch,
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string(&result)
                .map_err(|e| format!("failed serializing result as JSON: {e}"))?
        );
    } else {
        println!(
            "equal={} abs_tol={} rel_tol={}",
            result.equal, result.abs_tol, result.rel_tol
        );
        if let Some(m) = &result.first_mismatch {
            println!("node={}", m.node_path);
            println!("field={}", m.field);
            println!("detail={}", m.detail);
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
        "usage: svg_structure_diff <actual.svg> <expected.svg> [--abs-tol N] [--rel-tol N] [--json]"
            .to_string()
    })?;
    let expected = args.next().ok_or_else(|| {
        "usage: svg_structure_diff <actual.svg> <expected.svg> [--abs-tol N] [--rel-tol N] [--json]"
            .to_string()
    })?;

    let mut abs_tol = 0.0005_f64;
    let mut rel_tol = 0.0_f64;
    let mut json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--abs-tol" => {
                abs_tol = args
                    .next()
                    .ok_or_else(|| "--abs-tol requires a value".to_string())?
                    .parse::<f64>()
                    .map_err(|e| format!("invalid --abs-tol value: {e}"))?;
            }
            "--rel-tol" => {
                rel_tol = args
                    .next()
                    .ok_or_else(|| "--rel-tol requires a value".to_string())?
                    .parse::<f64>()
                    .map_err(|e| format!("invalid --rel-tol value: {e}"))?;
            }
            "--json" => json = true,
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    Ok(Args {
        actual,
        expected,
        abs_tol,
        rel_tol,
        json,
    })
}

fn compare_node(
    actual: roxmltree::Node<'_, '_>,
    expected: roxmltree::Node<'_, '_>,
    node_path: &str,
    abs_tol: f64,
    rel_tol: f64,
) -> Option<Mismatch> {
    if actual.tag_name().name() != expected.tag_name().name() {
        return Some(Mismatch {
            node_path: node_path.to_string(),
            field: "tag".to_string(),
            detail: format!(
                "tag mismatch actual={} expected={}",
                actual.tag_name().name(),
                expected.tag_name().name()
            ),
        });
    }

    let actual_attrs = attrs_map(actual);
    let expected_attrs = attrs_map(expected);
    if actual_attrs.len() != expected_attrs.len() {
        return Some(Mismatch {
            node_path: node_path.to_string(),
            field: "attributes".to_string(),
            detail: format!(
                "attribute count mismatch actual={} expected={}",
                actual_attrs.len(),
                expected_attrs.len()
            ),
        });
    }

    for (k, ev) in &expected_attrs {
        let Some(av) = actual_attrs.get(k) else {
            return Some(Mismatch {
                node_path: node_path.to_string(),
                field: format!("attr:{k}"),
                detail: "missing attribute in actual".to_string(),
            });
        };
        if let Some(detail) = compare_attr_values(k, av, ev, abs_tol, rel_tol) {
            return Some(Mismatch {
                node_path: node_path.to_string(),
                field: format!("attr:{k}"),
                detail,
            });
        }
    }

    let actual_text = child_text(actual);
    let expected_text = child_text(expected);
    if actual_text != expected_text {
        return Some(Mismatch {
            node_path: node_path.to_string(),
            field: "text".to_string(),
            detail: format!("text mismatch actual={actual_text:?} expected={expected_text:?}"),
        });
    }

    let actual_children: Vec<_> = actual.children().filter(|n| n.is_element()).collect();
    let expected_children: Vec<_> = expected.children().filter(|n| n.is_element()).collect();
    if actual_children.len() != expected_children.len() {
        return Some(Mismatch {
            node_path: node_path.to_string(),
            field: "children".to_string(),
            detail: format!(
                "child count mismatch actual={} expected={}",
                actual_children.len(),
                expected_children.len()
            ),
        });
    }

    for (i, (a, e)) in actual_children.iter().zip(expected_children.iter()).enumerate() {
        let child_path = format!("{node_path}/{}[{i}]", e.tag_name().name());
        if let Some(m) = compare_node(*a, *e, &child_path, abs_tol, rel_tol) {
            return Some(m);
        }
    }

    None
}

fn attrs_map(node: roxmltree::Node<'_, '_>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for a in node.attributes() {
        out.insert(a.name().to_string(), collapse_ws(a.value()));
    }
    out
}

fn child_text(node: roxmltree::Node<'_, '_>) -> String {
    let mut out = String::new();
    for c in node.children() {
        if c.is_text() {
            let t = collapse_ws(c.text().unwrap_or_default());
            if !t.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&t);
            }
        }
    }
    out
}

fn compare_attr_values(
    key: &str,
    actual: &str,
    expected: &str,
    abs_tol: f64,
    rel_tol: f64,
) -> Option<String> {
    if key == "data-figma-gradient-fill" {
        return None;
    }
    if is_numeric_attr(key) {
        return compare_numeric_text(actual, expected, abs_tol, rel_tol);
    }
    if actual != expected {
        return Some(format!("value mismatch actual={actual:?} expected={expected:?}"));
    }
    None
}

fn is_numeric_attr(key: &str) -> bool {
    matches!(
        key,
        "d"
            | "viewBox"
            | "x"
            | "y"
            | "width"
            | "height"
            | "opacity"
            | "fill-opacity"
            | "stroke-opacity"
            | "stroke-width"
            | "rx"
            | "ry"
            | "transform"
    )
}

#[derive(Debug, PartialEq)]
enum NumTok {
    Num(f64),
    Text(String),
}

fn compare_numeric_text(actual: &str, expected: &str, abs_tol: f64, rel_tol: f64) -> Option<String> {
    let at = tokenize_numeric_text(actual);
    let et = tokenize_numeric_text(expected);
    if at.len() != et.len() {
        return Some(format!(
            "token length mismatch actual={} expected={}",
            at.len(),
            et.len()
        ));
    }

    for (idx, (a, e)) in at.iter().zip(et.iter()).enumerate() {
        match (a, e) {
            (NumTok::Text(ta), NumTok::Text(te)) => {
                if ta != te {
                    return Some(format!(
                        "text token mismatch at {idx}: actual={ta:?} expected={te:?}"
                    ));
                }
            }
            (NumTok::Num(na), NumTok::Num(ne)) => {
                if !approx_eq(*na, *ne, abs_tol, rel_tol) {
                    return Some(format!(
                        "numeric token mismatch at {idx}: actual={na} expected={ne} (abs diff={})",
                        (na - ne).abs()
                    ));
                }
            }
            _ => {
                return Some(format!("token kind mismatch at {idx}: actual={a:?} expected={e:?}"));
            }
        }
    }

    None
}

fn tokenize_numeric_text(input: &str) -> Vec<NumTok> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let bytes = input.as_bytes();
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
                out.push(NumTok::Num(if num == -0.0 { 0.0 } else { num }));
            } else {
                out.push(NumTok::Text(token.to_string()));
            }
            continue;
        }

        let start = i;
        i += 1;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if is_number_start(ch) {
                break;
            }
            i += 1;
        }
        out.push(NumTok::Text(collapse_ws(&input[start..i])));
    }
    out
}

fn approx_eq(a: f64, b: f64, abs_tol: f64, rel_tol: f64) -> bool {
    let diff = (a - b).abs();
    if diff <= abs_tol {
        return true;
    }
    let scale = a.abs().max(b.abs()).max(1.0);
    diff <= rel_tol * scale
}

fn is_number_start(c: char) -> bool {
    c.is_ascii_digit() || c == '-' || c == '+' || c == '.'
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
