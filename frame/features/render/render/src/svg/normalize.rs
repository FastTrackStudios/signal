#[cfg(feature = "svg_export")]
pub(crate) struct NormalizeOptions {
    pub compact_paths: bool,
}

#[cfg(feature = "svg_export")]
impl Default for NormalizeOptions {
    fn default() -> Self {
        Self { compact_paths: true }
    }
}

pub(crate) fn normalize_svg_for_figma_parity(svg: &str, width: u32, height: u32, opts: &NormalizeOptions) -> String {
    let mut out = Vec::new();
    let mut body = Vec::new();
    let root = if svg.contains("xlink:") {
        format!(
            r#"<svg width="{width}" height="{height}" viewBox="0 0 {width} {height}" fill="none" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">"#
        )
    } else {
        format!(
            r#"<svg width="{width}" height="{height}" viewBox="0 0 {width} {height}" fill="none" xmlns="http://www.w3.org/2000/svg">"#
        )
    };
    out.push(root);

    let mut saw_root = false;
    for line in svg.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("<svg ") {
            saw_root = true;
            continue;
        }
        if t == "</svg>" {
            continue;
        }
        if let Some(norm) = normalize_rect_line(t) {
            body.push(norm);
            continue;
        }
        if let Some(norm) = normalize_mask_line(t) {
            body.push(norm);
            continue;
        }
        if let Some(norm) = normalize_circle_or_ellipse_line(t) {
            body.push(norm);
            continue;
        }
        if let Some(norm) = normalize_line_element(t) {
            body.push(norm);
            continue;
        }
        if let Some(norm) = normalize_translated_path_line(t, opts.compact_paths) {
            body.push(norm);
            continue;
        }
        if let Some(norm) = normalize_group_line(t) {
            body.push(norm);
            continue;
        }
        // Figma often emits keyword black for solid text/shape paths.
        body.push(
            t.replace("fill=\"#000000\"", "fill=\"black\"")
                .replace("stroke=\"#000000\"", "stroke=\"black\""),
        );
    }
    let body = wrap_top_level_after_first_filtered_group(body);
    let body = flatten_plain_groups(body);
    out.extend(body);
    if !saw_root {
        // fallback: maintain well-formed output even if parser changes
        out.push(format!(r##"<rect width="{width}" height="{height}" fill="#FFFFFF"/>"##));
    }
    out.push("</svg>".to_string());
    out.join("\n") + "\n"
}

#[cfg(feature = "svg_export")]
fn wrap_top_level_after_first_filtered_group(lines: Vec<String>) -> Vec<String> {
    if lines.is_empty() || !lines[0].starts_with("<g filter=") {
        return lines;
    }
    let mut depth: i32 = 0;
    let mut close_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("<g ") && !t.ends_with("/>") {
            depth += 1;
        }
        if t == "</g>" {
            depth -= 1;
            if depth == 0 {
                close_idx = Some(i);
                break;
            }
        }
    }
    let Some(close_idx) = close_idx else {
        return lines;
    };
    if close_idx + 1 >= lines.len() {
        return lines;
    }
    let mut defs_start = lines.len();
    for (i, line) in lines.iter().enumerate().skip(close_idx + 1) {
        if line.trim_start().starts_with("<defs") {
            defs_start = i;
            break;
        }
    }
    if close_idx + 1 >= defs_start {
        return lines;
    }

    let mut out = Vec::with_capacity(lines.len());
    out.extend_from_slice(&lines[0..close_idx]);
    out.extend_from_slice(&lines[close_idx + 1..defs_start]);
    out.push(lines[close_idx].clone());
    out.extend_from_slice(&lines[defs_start..]);
    out
}

#[cfg(feature = "svg_export")]
fn flatten_plain_groups(lines: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    let mut plain_group_stack: Vec<bool> = Vec::new();

    for line in lines {
        let t = line.trim();
        if t == "<g>" {
            plain_group_stack.push(true);
            continue;
        }
        if t.starts_with("<g ") && !t.ends_with("/>") {
            plain_group_stack.push(false);
            out.push(line);
            continue;
        }
        if t == "</g>" {
            let is_plain = plain_group_stack.pop().unwrap_or(false);
            if !is_plain {
                out.push(line);
            }
            continue;
        }
        out.push(line);
    }

    out
}

#[cfg(feature = "svg_export")]
const SVG_NUM_PRECISION_DP: u32 = 4;

#[cfg(feature = "svg_export")]
fn normalize_rect_line(line: &str) -> Option<String> {
    if !line.starts_with("<rect ") {
        return None;
    }
    let x = extract_xml_attr(line, "x");
    let y = extract_xml_attr(line, "y");
    let w = extract_xml_attr(line, "width")?;
    let h = extract_xml_attr(line, "height")?;
    let opacity = extract_xml_attr(line, "opacity");
    let style = extract_xml_attr(line, "style");
    let transform = extract_xml_attr(line, "transform");
    let rx = extract_xml_attr(line, "rx");
    let ry = extract_xml_attr(line, "ry");
    let fill = extract_xml_attr(line, "fill");
    let fill_opacity = extract_xml_attr(line, "fill-opacity");
    let stroke = extract_xml_attr(line, "stroke");
    let stroke_opacity = extract_xml_attr(line, "stroke-opacity");
    let stroke_width = extract_xml_attr(line, "stroke-width");
    let linecap = extract_xml_attr(line, "stroke-linecap");
    let linejoin = extract_xml_attr(line, "stroke-linejoin");
    let figma_gradient_fill = extract_xml_attr(line, "data-figma-gradient-fill");
    let figma_bg_blur_radius = extract_xml_attr(line, "data-figma-bg-blur-radius");

    let mut attrs: Vec<String> = Vec::new();
    if let Some(opacity) = opacity {
        let val = trim_decimal(&opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"opacity="{val}""#));
        }
    }
    if let Some(x) = x {
        let val = trim_decimal(&x);
        if val != "0" {
            attrs.push(format!(r#"x="{val}""#));
        }
    }
    if let Some(y) = y {
        let val = trim_decimal(&y);
        if val != "0" {
            attrs.push(format!(r#"y="{val}""#));
        }
    }
    attrs.push(format!(r#"width="{}""#, trim_decimal(&w)));
    attrs.push(format!(r#"height="{}""#, trim_decimal(&h)));
    if let Some(transform) = transform {
        attrs.push(format!(r#"transform="{transform}""#));
    }
    if let Some(rx) = rx {
        attrs.push(format!(r#"rx="{}""#, trim_decimal(&rx)));
    }
    if let Some(ry) = ry {
        attrs.push(format!(r#"ry="{}""#, trim_decimal(&ry)));
    }
    if let Some(mut fill) = fill {
        fill = normalize_svg_color_keyword(fill);
        attrs.push(format!(r#"fill="{fill}""#));
    }
    if let Some(style) = style {
        attrs.push(format!(r#"style="{style}""#));
    }
    if let Some(opacity) = fill_opacity {
        let val = trim_decimal(&opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"fill-opacity="{val}""#));
        }
    }
    if let Some(stroke) = stroke {
        let stroke = normalize_svg_color_keyword(stroke);
        attrs.push(format!(r#"stroke="{stroke}""#));
    }
    if let Some(opacity) = stroke_opacity {
        let val = trim_decimal(&opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"stroke-opacity="{val}""#));
        }
    }
    if let Some(stroke_width) = stroke_width {
        let val = trim_decimal(&stroke_width);
        if val != "1" {
            attrs.push(format!(r#"stroke-width="{val}""#));
        }
    }
    if let Some(linecap) = linecap {
        if linecap != "butt" {
            attrs.push(format!(r#"stroke-linecap="{linecap}""#));
        }
    }
    if let Some(linejoin) = linejoin {
        if linejoin != "miter" {
            attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
        }
    }
    if let Some(payload) = figma_gradient_fill {
        attrs.push(format!(r#"data-figma-gradient-fill="{payload}""#));
    }
    if let Some(radius) = figma_bg_blur_radius {
        attrs.push(format!(r#"data-figma-bg-blur-radius="{}""#, trim_decimal(&radius)));
    }
    Some(format!("<rect {}/>", attrs.join(" ")))
}

#[cfg(feature = "svg_export")]
fn normalize_mask_line(line: &str) -> Option<String> {
    if !line.starts_with("<mask ") {
        return None;
    }
    let id = extract_xml_attr(line, "id")?;
    let style = extract_xml_attr(line, "style");
    let units = extract_xml_attr(line, "maskUnits");
    let x = extract_xml_attr(line, "x");
    let y = extract_xml_attr(line, "y");
    let width = extract_xml_attr(line, "width");
    let height = extract_xml_attr(line, "height");
    let mut attrs: Vec<String> = vec![format!(r#"id="{id}""#)];
    if let Some(style) = style {
        attrs.push(format!(r#"style="{style}""#));
    }
    if let Some(units) = units {
        attrs.push(format!(r#"maskUnits="{units}""#));
    }
    if let Some(x) = x {
        attrs.push(format!(r#"x="{}""#, trim_decimal(&x)));
    }
    if let Some(y) = y {
        attrs.push(format!(r#"y="{}""#, trim_decimal(&y)));
    }
    if let Some(width) = width {
        attrs.push(format!(r#"width="{}""#, trim_decimal(&width)));
    }
    if let Some(height) = height {
        attrs.push(format!(r#"height="{}""#, trim_decimal(&height)));
    }
    Some(format!("<mask {}>", attrs.join(" ")))
}

/// Normalize `<circle>` or `<ellipse>` tags: reorder attributes, trim decimals.
#[cfg(feature = "svg_export")]
fn normalize_circle_or_ellipse_line(line: &str) -> Option<String> {
    let (tag, is_circle) = if line.starts_with("<circle ") {
        ("circle", true)
    } else if line.starts_with("<ellipse ") {
        ("ellipse", false)
    } else {
        return None;
    };

    let cx = extract_xml_attr(line, "cx")?;
    let cy = extract_xml_attr(line, "cy")?;

    let mut attrs: Vec<String> = vec![
        format!(r#"cx="{}""#, trim_decimal(&cx)),
        format!(r#"cy="{}""#, trim_decimal(&cy)),
    ];
    if is_circle {
        let r = extract_xml_attr(line, "r")?;
        attrs.push(format!(r#"r="{}""#, trim_decimal(&r)));
    } else {
        let rx = extract_xml_attr(line, "rx")?;
        let ry = extract_xml_attr(line, "ry")?;
        attrs.push(format!(r#"rx="{}""#, trim_decimal(&rx)));
        attrs.push(format!(r#"ry="{}""#, trim_decimal(&ry)));
    }

    if let Some(transform) = extract_xml_attr(line, "transform") {
        attrs.push(format!(r#"transform="{transform}""#));
    }
    if let Some(fill) = extract_xml_attr(line, "fill") {
        let fill = normalize_svg_color_keyword(fill);
        attrs.push(format!(r#"fill="{fill}""#));
    }
    if let Some(fill_opacity) = extract_xml_attr(line, "fill-opacity") {
        let val = trim_decimal(&fill_opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"fill-opacity="{val}""#));
        }
    }
    if let Some(style) = extract_xml_attr(line, "style") {
        attrs.push(format!(r#"style="{style}""#));
    }
    if let Some(stroke) = extract_xml_attr(line, "stroke") {
        let stroke = normalize_svg_color_keyword(stroke);
        attrs.push(format!(r#"stroke="{stroke}""#));
    }
    if let Some(stroke_opacity) = extract_xml_attr(line, "stroke-opacity") {
        let val = trim_decimal(&stroke_opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"stroke-opacity="{val}""#));
        }
    }
    if let Some(stroke_width) = extract_xml_attr(line, "stroke-width") {
        attrs.push(format!(r#"stroke-width="{}""#, trim_decimal(&stroke_width)));
    }
    if let Some(payload) = extract_xml_attr(line, "data-figma-gradient-fill") {
        attrs.push(format!(r#"data-figma-gradient-fill="{payload}""#));
    }
    Some(format!("<{tag} {}/>", attrs.join(" ")))
}

/// Normalize `<line>` elements: reorder attributes, trim decimals, omit default stroke-width.
#[cfg(feature = "svg_export")]
fn normalize_line_element(line: &str) -> Option<String> {
    if !line.starts_with("<line ") {
        return None;
    }
    let x1 = extract_xml_attr(line, "x1")?;
    let y1 = extract_xml_attr(line, "y1")?;
    let x2 = extract_xml_attr(line, "x2")?;
    let y2 = extract_xml_attr(line, "y2")?;

    let mut attrs: Vec<String> = vec![
        format!(r#"x1="{}""#, trim_decimal(&x1)),
        format!(r#"y1="{}""#, trim_decimal(&y1)),
        format!(r#"x2="{}""#, trim_decimal(&x2)),
        format!(r#"y2="{}""#, trim_decimal(&y2)),
    ];
    if let Some(stroke) = extract_xml_attr(line, "stroke") {
        let stroke = normalize_svg_color_keyword(stroke);
        attrs.push(format!(r#"stroke="{stroke}""#));
    }
    if let Some(stroke_opacity) = extract_xml_attr(line, "stroke-opacity") {
        let val = trim_decimal(&stroke_opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"stroke-opacity="{val}""#));
        }
    }
    // Omit stroke-width="1" (SVG default).
    if let Some(stroke_width) = extract_xml_attr(line, "stroke-width") {
        let val = trim_decimal(&stroke_width);
        if val != "1" {
            attrs.push(format!(r#"stroke-width="{val}""#));
        }
    }
    if let Some(stroke_linecap) = extract_xml_attr(line, "stroke-linecap") {
        attrs.push(format!(r#"stroke-linecap="{stroke_linecap}""#));
    }
    if let Some(stroke_linejoin) = extract_xml_attr(line, "stroke-linejoin") {
        attrs.push(format!(r#"stroke-linejoin="{stroke_linejoin}""#));
    }
    if let Some(stroke_dasharray) = extract_xml_attr(line, "stroke-dasharray") {
        attrs.push(format!(r#"stroke-dasharray="{stroke_dasharray}""#));
    }
    if let Some(stroke_dashoffset) = extract_xml_attr(line, "stroke-dashoffset") {
        attrs.push(format!(r#"stroke-dashoffset="{}""#, trim_decimal(&stroke_dashoffset)));
    }
    Some(format!("<line {}/>", attrs.join(" ")))
}

/// Reorder attributes on `<g>` opening tags to match Figma convention:
/// opacity, filter, style (mix-blend-mode).
#[cfg(feature = "svg_export")]
fn normalize_group_line(line: &str) -> Option<String> {
    if !line.starts_with("<g ") || line.ends_with("/>") {
        return None;
    }
    let opacity = extract_xml_attr(line, "opacity");
    let filter = extract_xml_attr(line, "filter");
    // Only normalize groups that have both opacity and filter (attribute ordering matters there).
    if opacity.is_none() || filter.is_none() {
        return None;
    }
    let style = extract_xml_attr(line, "style");
    let clip_path = extract_xml_attr(line, "clip-path");
    let mut attrs: Vec<String> = Vec::new();
    if let Some(opacity) = opacity {
        let val = trim_decimal(&opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"opacity="{val}""#));
        }
    }
    if let Some(filter) = filter {
        attrs.push(format!(r#"filter="{filter}""#));
    }
    if let Some(clip_path) = clip_path {
        attrs.push(format!(r#"clip-path="{clip_path}""#));
    }
    if let Some(style) = style {
        attrs.push(format!(r#"style="{style}""#));
    }
    Some(format!("<g {}>", attrs.join(" ")))
}

#[cfg(feature = "svg_export")]
fn parse_translate_decimal(transform: &str) -> Option<(rust_decimal::Decimal, rust_decimal::Decimal)> {
    let raw = transform.strip_prefix("translate(")?.strip_suffix(')')?;
    let mut parts = raw.split(',');
    let x = parse_decimal(parts.next()?.trim())?;
    let y = parse_decimal(parts.next()?.trim())?;
    Some((x, y))
}

#[cfg(feature = "svg_export")]
fn parse_decimal(v: &str) -> Option<rust_decimal::Decimal> {
    if let Ok(d) = rust_decimal::Decimal::from_str_exact(v) {
        return Some(d);
    }
    if let Ok(d) = v.parse::<rust_decimal::Decimal>() {
        return Some(d);
    }
    if let Ok(f) = v.parse::<f64>() {
        return rust_decimal::Decimal::from_f64_retain(f);
    }
    None
}

#[cfg(feature = "svg_export")]
fn extract_xml_attr(line: &str, key: &str) -> Option<String> {
    let needle = format!(r#"{key}=""#);
    let mut search_from = 0usize;
    while let Some(found) = line[search_from..].find(&needle) {
        let idx = search_from + found;
        let prev = line[..idx].chars().next_back();
        let is_exact_name = !matches!(
            prev,
            Some(c) if c.is_ascii_alphanumeric() || c == '-' || c == '_'
        );
        if is_exact_name {
            let start = idx + needle.len();
            let rest = &line[start..];
            let end = rest.find('"')?;
            return Some(rest[..end].to_string());
        }
        search_from = idx + needle.len();
    }
    None
}

#[cfg(feature = "svg_export")]
fn trim_decimal(v: &str) -> String {
    parse_decimal(v)
        .map(|d| d.normalize().to_string())
        .unwrap_or_else(|| v.to_string())
}

#[cfg(feature = "svg_export")]
fn normalize_svg_color_keyword(value: String) -> String {
    if value.eq_ignore_ascii_case("#000000") {
        "black".to_string()
    } else if value.eq_ignore_ascii_case("#FFFFFF") {
        "white".to_string()
    } else {
        value
    }
}

#[cfg(feature = "svg_export")]
fn should_emit_opacity(v: &str) -> bool {
    parse_decimal(v)
        .map(|x| !format_decimal(x).eq("1"))
        .unwrap_or(true)
}

#[cfg(feature = "svg_export")]
fn normalize_translated_path_line(line: &str, compact_paths: bool) -> Option<String> {
    if !line.starts_with("<path ") {
        return None;
    }
    let d = extract_xml_attr(line, "d")?;
    let transform = extract_xml_attr(line, "transform")?;
    let (tx, ty) = parse_translate_decimal(&transform)?;
    let translated = if tx.is_zero() && ty.is_zero() {
        d
    } else {
        translate_and_compact_path_data(&d, tx, ty, compact_paths)?
    };

    let mut attrs = vec![format!(r#"d="{translated}""#)];

    if let Some(fill) = extract_xml_attr(line, "fill") {
        let fill = normalize_svg_color_keyword(fill);
        attrs.push(format!(r#"fill="{fill}""#));
    }
    if let Some(style) = extract_xml_attr(line, "style") {
        attrs.push(format!(r#"style="{style}""#));
    }
    if let Some(fill_opacity) = extract_xml_attr(line, "fill-opacity") {
        let val = trim_decimal(&fill_opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"fill-opacity="{val}""#));
        }
    }
    if let Some(stroke) = extract_xml_attr(line, "stroke") {
        let stroke = normalize_svg_color_keyword(stroke);
        attrs.push(format!(r#"stroke="{stroke}""#));
    }
    if let Some(stroke_opacity) = extract_xml_attr(line, "stroke-opacity") {
        let val = trim_decimal(&stroke_opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"stroke-opacity="{val}""#));
        }
    }
    if let Some(stroke_width) = extract_xml_attr(line, "stroke-width") {
        attrs.push(format!(r#"stroke-width="{}""#, trim_decimal(&stroke_width)));
    }
    if let Some(linecap) = extract_xml_attr(line, "stroke-linecap") {
        attrs.push(format!(r#"stroke-linecap="{linecap}""#));
    }
    if let Some(linejoin) = extract_xml_attr(line, "stroke-linejoin") {
        attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
    }
    if let Some(clip_path) = extract_xml_attr(line, "clip-path") {
        attrs.push(format!(r#"clip-path="{clip_path}""#));
    }

    Some(format!("<path {}/>", attrs.join(" ")))
}

#[cfg(feature = "svg_export")]
fn translate_and_compact_path_data(
    d: &str,
    tx: rust_decimal::Decimal,
    ty: rust_decimal::Decimal,
    compact_paths: bool,
) -> Option<String> {
    use rust_decimal::Decimal;

    let mut tokens = Vec::<String>::new();
    let chars: Vec<char> = d.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphabetic() {
            tokens.push(c.to_string());
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
            if ch.is_ascii_whitespace() || ch == ',' {
                break;
            }
            if ch.is_ascii_alphabetic() {
                // Keep scientific notation exponents (e.g. 1.2e-7) inside numeric tokens.
                if !(ch == 'e' || ch == 'E') {
                    break;
                }
            }
            if (ch == '+' || ch == '-') && chars[i - 1] != 'e' && chars[i - 1] != 'E' {
                break;
            }
            i += 1;
        }
        tokens.push(chars[start..i].iter().collect::<String>());
    }

    let mut out = String::new();
    let mut idx = 0usize;
    let mut first = true;
    let mut curr_x = Decimal::ZERO;
    let mut curr_y = Decimal::ZERO;
    let mut subpath_x = Decimal::ZERO;
    let mut subpath_y = Decimal::ZERO;

    while idx < tokens.len() {
        let cmd = tokens.get(idx)?.chars().next()?;
        if !cmd.is_ascii_alphabetic() {
            return None;
        }
        idx += 1;
        let next_is_cmd = |pos: usize| -> bool {
            tokens
                .get(pos)
                .and_then(|t| t.chars().next())
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false)
        };
        let mut nums = Vec::<Decimal>::new();
        while idx < tokens.len() && !next_is_cmd(idx) {
            nums.push(parse_decimal(&tokens[idx])?);
            idx += 1;
        }

        let emit = |ch: char, values: &[Decimal], out: &mut String, first: &mut bool| {
            if *first {
                *first = false;
            }
            out.push(ch);
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&fmt_path_num(*value));
            }
        };

        match cmd {
            'M' => {
                if nums.len() < 2 || nums.len() % 2 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for (i, pair) in nums.chunks_exact(2).enumerate() {
                    let x = pair[0] + tx;
                    let y = pair[1] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                    if i == 0 {
                        subpath_x = x;
                        subpath_y = y;
                    }
                }
                emit('M', &vals, &mut out, &mut first);
            }
            'L' => {
                if nums.len() % 2 != 0 {
                    return None;
                }
                for pair in nums.chunks_exact(2) {
                    let x = pair[0] + tx;
                    let y = pair[1] + ty;
                    if compact_paths && y == curr_y {
                        emit('H', &[x], &mut out, &mut first);
                    } else if compact_paths && x == curr_x {
                        emit('V', &[y], &mut out, &mut first);
                    } else {
                        emit('L', &[x, y], &mut out, &mut first);
                    }
                    curr_x = x;
                    curr_y = y;
                }
            }
            'C' => {
                if nums.len() % 6 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for seg in nums.chunks_exact(6) {
                    vals.push(seg[0] + tx);
                    vals.push(seg[1] + ty);
                    vals.push(seg[2] + tx);
                    vals.push(seg[3] + ty);
                    let x = seg[4] + tx;
                    let y = seg[5] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                }
                emit('C', &vals, &mut out, &mut first);
            }
            'Q' | 'S' => {
                if nums.len() % 4 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for seg in nums.chunks_exact(4) {
                    vals.push(seg[0] + tx);
                    vals.push(seg[1] + ty);
                    let x = seg[2] + tx;
                    let y = seg[3] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                }
                emit(cmd, &vals, &mut out, &mut first);
            }
            'T' => {
                if nums.len() % 2 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for seg in nums.chunks_exact(2) {
                    let x = seg[0] + tx;
                    let y = seg[1] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                }
                emit('T', &vals, &mut out, &mut first);
            }
            'A' => {
                if nums.len() % 7 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for seg in nums.chunks_exact(7) {
                    vals.push(seg[0]);
                    vals.push(seg[1]);
                    vals.push(seg[2]);
                    vals.push(seg[3]);
                    vals.push(seg[4]);
                    let x = seg[5] + tx;
                    let y = seg[6] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                }
                emit('A', &vals, &mut out, &mut first);
            }
            'H' => {
                let mut vals = Vec::with_capacity(nums.len());
                for x in nums {
                    let x = x + tx;
                    vals.push(x);
                    curr_x = x;
                }
                emit('H', &vals, &mut out, &mut first);
            }
            'V' => {
                let mut vals = Vec::with_capacity(nums.len());
                for y in nums {
                    let y = y + ty;
                    vals.push(y);
                    curr_y = y;
                }
                emit('V', &vals, &mut out, &mut first);
            }
            'Z' | 'z' => {
                out.push('Z');
                curr_x = subpath_x;
                curr_y = subpath_y;
            }
            _ => return None,
        }
    }

    Some(out)
}

#[cfg(feature = "svg_export")]
fn fmt_path_num(v: rust_decimal::Decimal) -> String {
    format_decimal(v)
}

#[cfg(feature = "svg_export")]
fn format_decimal(v: rust_decimal::Decimal) -> String {
    use rust_decimal::RoundingStrategy;

    if v.is_zero() {
        return "0".to_string();
    }
    let rounded =
        v.round_dp_with_strategy(SVG_NUM_PRECISION_DP, RoundingStrategy::MidpointAwayFromZero);
    if rounded.is_zero() {
        return "0".to_string();
    }
    rounded.normalize().to_string()
}

#[cfg(all(feature = "svg_export", test))]
mod svg_normalize_tests {
    use super::*;

    #[test]
    fn zero_translate_preserves_path_data() {
        let line = r##"<path d="M1 2L3 4Z" transform="translate(0,0)" fill="#000000"/>"##;
        let out = normalize_translated_path_line(line, true).expect("normalized");
        assert!(out.contains(r#"d="M1 2L3 4Z""#));
        assert!(out.contains(r#"fill="black""#));
    }

    #[test]
    fn translate_uses_decimal_math() {
        let line = r##"<path d="M29.7955 31.9489" transform="translate(0.1,0.2)" fill="#111111"/>"##;
        let out = normalize_translated_path_line(line, true).expect("normalized");
        assert!(out.contains(r#"d="M29.8955 32.1489""#), "{out}");
    }

    #[test]
    fn translate_preserves_command_shape() {
        let line = r##"<path d="M0 0 10 0 10 10" transform="translate(5,6)" fill="#111111"/>"##;
        let out = normalize_translated_path_line(line, true).expect("normalized");
        assert!(out.contains(r#"d="M5 6 15 6 15 16""#), "{out}");
        assert!(!out.contains("L"), "{out}");
    }

    #[test]
    fn no_compact_keeps_l_commands() {
        let line = r##"<path d="M0 0L10 0L10 10" transform="translate(5,6)" fill="#111111"/>"##;
        let out = normalize_translated_path_line(line, false).expect("normalized");
        assert!(out.contains(r#"d="M5 6L15 6L15 16""#), "{out}");
        assert!(!out.contains("H"), "{out}");
        assert!(!out.contains("V"), "{out}");
    }
}
