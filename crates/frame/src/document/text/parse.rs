//! Markup → scene. Hand-written: a small lexer into an element tree, then
//! a builder that turns each element into a `Node`.

use std::collections::{BTreeMap, HashMap};

use crate::document::proto::{
    AlignSelf, Asset, AssetId, AutoLayout, Bind, BlendMode, BooleanOp, CornerRadii, CrossAxisAlign,
    LayoutDirection, MainAxisAlign, MaskKind, Node, NodeId, NodeKind, ResizeX, ResizeY, Scene,
    SourceRef, StrokeAlign, StrokeCap, StrokeJoin, TextAlign, TextCase, TextData, Variable,
    Variables,
};
use serde_json::Value;

use crate::document::text::error::ParseError;
use crate::document::text::schema::{
    BIND_ATTRS, LAYOUT_ATTRS, TAGS, VARIABLES_TAG, attrs_for, default_name, owner_of, suggest_attr,
    suggest_tag,
};
use crate::document::text::values::{
    Keyword, effects_from_json, from_json, paints_from_json, parse_hex, rotation_matrix,
    runs_from_json, transform_from_json,
};

type Result<T> = std::result::Result<T, ParseError>;

/// Parse a whole document: exactly one top-level element (normally
/// `<Document>`), which becomes `scene.root`. `assets` become the scene's
/// asset table (markup references assets by id, it never embeds them).
///
/// Elements without `id` get a fresh one.
///
/// # Errors
/// The first problem found, with its line and column.
pub fn parse(src: &str, assets: BTreeMap<AssetId, Asset>) -> Result<Scene> {
    let els = Lexer::new(src).document()?;
    let el = match els.as_slice() {
        [one] => one,
        [] => {
            return Err(ParseError::at(
                src,
                0,
                "empty document — expected one root element like <Document>…</Document>",
            ));
        }
        [_, second, ..] => {
            return Err(ParseError::at(
                src,
                second.pos,
                "a document has exactly one root element; wrap everything in one <Document>…</Document>",
            ));
        }
    };
    let mut b = Builder::new(src, false);
    let root = b.build(el, None)?;
    let nodes = b.nodes.into_iter().map(|n| (n.id, n)).collect();
    Ok(Scene {
        root,
        nodes,
        assets,
        variables: b.variables,
    })
}

/// Parse a fragment: one or more elements (no `<Document>`/`<Page>`), for
/// inserting into an existing scene. Returns every node in pre-order with
/// `parent`/`children` wired among themselves; top-level elements come
/// back with `parent: None`.
///
/// # Errors
/// The first problem found, with its line and column.
pub fn parse_fragment(src: &str) -> Result<Vec<Node>> {
    let els = Lexer::new(src).document()?;
    if els.is_empty() {
        return Err(ParseError::at(
            src,
            0,
            "empty fragment — expected one or more elements like <Frame>…</Frame>",
        ));
    }
    let mut b = Builder::new(src, true);
    for el in &els {
        b.build(el, None)?;
    }
    Ok(b.nodes)
}

// ── lexer ────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum AttrValue {
    /// `locked` (no `=`): true.
    Flag,
    Bare(String),
    Quoted(String),
    Json(Value),
}

#[derive(Debug)]
struct Attr {
    name: String,
    pos: usize,
    value: AttrValue,
    value_pos: usize,
}

#[derive(Debug)]
struct El {
    tag: String,
    pos: usize,
    attrs: Vec<Attr>,
    children: Vec<El>,
    /// Text content (already unescaped) and where it started.
    body: Option<(String, usize)>,
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn err<T>(&self, at: usize, msg: impl Into<String>) -> Result<T> {
        Err(ParseError::at(self.src, at, msg))
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn starts(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    fn bump(&mut self, n: usize) {
        self.pos += n;
    }

    fn skip_ws(&mut self) {
        let trimmed = self.rest().trim_start();
        self.pos = self.src.len() - trimmed.len();
    }

    /// Whitespace and `<!-- … -->` comments.
    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            self.skip_ws();
            if !self.skip_comment()? {
                return Ok(());
            }
        }
    }

    fn skip_comment(&mut self) -> Result<bool> {
        if !self.starts("<!--") {
            return Ok(false);
        }
        let start = self.pos;
        match self.rest().find("-->") {
            Some(i) => {
                self.bump(i + 3);
                Ok(true)
            }
            None => self.err(start, "unterminated comment — close it with `-->`"),
        }
    }

    fn document(&mut self) -> Result<Vec<El>> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia()?;
            match self.peek() {
                None => return Ok(out),
                Some('<') if self.starts("</") => {
                    let name = self.rest()[2..]
                        .split(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or("");
                    return self.err(
                        self.pos,
                        format!("closing tag </{name}> has no matching opening tag"),
                    );
                }
                Some('<') => out.push(self.element()?),
                Some(c) => {
                    return self.err(
                        self.pos,
                        format!("unexpected `{c}` — expected an element like <Frame …>"),
                    );
                }
            }
        }
    }

    fn ident(&mut self, attr: bool) -> &'a str {
        let rest = self.rest();
        let len = rest
            .char_indices()
            .find(|(i, c)| {
                let ok = c.is_ascii_alphanumeric()
                    || *c == '_'
                    || (attr && *i > 0 && (*c == '-' || *c == ':'));
                !ok
            })
            .map_or(rest.len(), |(i, _)| i);
        self.bump(len);
        &rest[..len]
    }

    fn element(&mut self) -> Result<El> {
        let pos = self.pos;
        self.bump(1); // <
        let tag = self.ident(false).to_owned();
        if tag.is_empty() {
            let found = self
                .peek()
                .map_or("end of input".to_owned(), |c| format!("`{c}`"));
            return self.err(
                self.pos,
                format!("expected an element name after `<`, found {found}"),
            );
        }
        let mut el = El {
            tag,
            pos,
            attrs: Vec::new(),
            children: Vec::new(),
            body: None,
        };

        // Attributes.
        loop {
            self.skip_ws();
            if self.starts("/>") {
                self.bump(2);
                return Ok(el);
            }
            if self.starts(">") {
                self.bump(1);
                break;
            }
            let apos = self.pos;
            let name = self.ident(true).to_owned();
            if name.is_empty() {
                return match self.peek() {
                    None => self.err(
                        pos,
                        format!("<{}> is never closed — end it with `>` or `/>`", el.tag),
                    ),
                    Some(c) => self.err(
                        apos,
                        format!(
                            "unexpected `{c}` in <{}> — expected an attribute name, `/>` or `>`",
                            el.tag
                        ),
                    ),
                };
            }
            self.skip_ws();
            let (value, value_pos) = if self.starts("=") {
                self.bump(1);
                self.skip_ws();
                let vpos = self.pos;
                (self.attr_value(&name)?, vpos)
            } else {
                (AttrValue::Flag, apos)
            };
            el.attrs.push(Attr {
                name,
                pos: apos,
                value,
                value_pos,
            });
        }

        // Content, up to the closing tag.
        let mut text = String::new();
        let mut text_pos = None;
        let mut has_text = false;
        loop {
            let seg_start = self.pos;
            let seg_len = self.rest().find('<').unwrap_or(self.rest().len());
            if seg_len > 0 {
                text_pos.get_or_insert(seg_start);
                let seg = &self.rest()[..seg_len];
                has_text |= !seg.trim().is_empty();
                text.push_str(seg);
                self.bump(seg_len);
            }
            if self.peek().is_none() {
                return self.err(
                    pos,
                    format!("<{}> is never closed — add </{}>", el.tag, el.tag),
                );
            }
            if self.skip_comment()? {
                continue;
            }
            if self.starts("</") {
                let cpos = self.pos;
                self.bump(2);
                let name = self.ident(false);
                self.skip_ws();
                if !self.starts(">") {
                    return self.err(self.pos, format!("expected `>` to finish </{name}>"));
                }
                self.bump(1);
                if name != el.tag {
                    let (l, c) = crate::document::text::error::line_col(self.src, pos);
                    return self.err(
                        cpos,
                        format!(
                            "</{name}> doesn't match <{}> opened at {l}:{c} — close it with </{}>",
                            el.tag, el.tag
                        ),
                    );
                }
                break;
            }
            el.children.push(self.element()?);
        }

        if has_text {
            let at = text_pos.unwrap_or(pos);
            if el.tag != "Text" {
                let snippet: String = text.trim().chars().take(24).collect();
                return self.err(
                    at,
                    format!(
                        "text `{snippet}` inside <{}> — only <Text> holds text; wrap it: <Text>{snippet}</Text>",
                        el.tag
                    ),
                );
            }
            if !el.children.is_empty() {
                return self.err(
                    at,
                    "a <Text> with child elements can't also have body text — use text=\"…\"",
                );
            }
        }
        if el.tag == "Text" && el.children.is_empty() && !text.is_empty() {
            el.body = Some((unescape(&jsx_whitespace(&text)), text_pos.unwrap_or(pos)));
        }
        Ok(el)
    }

    fn attr_value(&mut self, name: &str) -> Result<AttrValue> {
        match self.peek() {
            Some('"') => {
                let start = self.pos;
                let bytes = self.src.as_bytes();
                let mut i = start + 1;
                loop {
                    match bytes.get(i) {
                        None => {
                            return self.err(
                                start,
                                format!("unterminated string in `{name}=` — add the closing `\"`"),
                            );
                        }
                        Some(b'\\') => i += 2,
                        Some(b'"') => break,
                        Some(_) => i += 1,
                    }
                }
                let lit = &self.src[start..=i];
                let s: String = serde_json::from_str(lit).map_err(|e| {
                    ParseError::at(
                        self.src,
                        start,
                        format!("bad string in `{name}=`: {}", strip_serde_pos(&e)),
                    )
                })?;
                self.pos = i + 1;
                Ok(AttrValue::Quoted(s))
            }
            Some('{') => {
                let open = self.pos;
                let body_start = open + 1;
                let mut stream = serde_json::Deserializer::from_str(&self.src[body_start..])
                    .into_iter::<Value>();
                match stream.next() {
                    Some(Ok(v)) => {
                        self.pos = body_start + stream.byte_offset();
                        self.skip_ws();
                        if !self.starts("}") {
                            return self.err(
                                self.pos,
                                format!("expected `}}` to close the `{{…}}` value of `{name}`"),
                            );
                        }
                        self.bump(1);
                        Ok(AttrValue::Json(v))
                    }
                    Some(Err(e)) => {
                        let at =
                            offset_of(&self.src[body_start..], e.line(), e.column()) + body_start;
                        self.err(
                            at,
                            format!("invalid JSON in `{name}={{…}}`: {}", strip_serde_pos(&e)),
                        )
                    }
                    None => self.err(open, format!("expected a JSON value inside `{name}={{…}}`")),
                }
            }
            _ => {
                let rest = self.rest();
                let len = rest
                    .char_indices()
                    .find(|(i, c)| {
                        c.is_whitespace()
                            || *c == '>'
                            || (*c == '/' && rest[*i..].starts_with("/>"))
                    })
                    .map_or(rest.len(), |(i, _)| i);
                if len == 0 {
                    return self.err(self.pos, format!("missing value after `{name}=`"));
                }
                let v = rest[..len].to_owned();
                self.bump(len);
                Ok(AttrValue::Bare(v))
            }
        }
    }
}

fn strip_serde_pos(e: &serde_json::Error) -> String {
    let s = e.to_string();
    match s.rfind(" at line ") {
        Some(i) => s[..i].to_owned(),
        None => s,
    }
}

/// Byte offset of 1-based (line, column) in `s`.
fn offset_of(s: &str, line: usize, col: usize) -> usize {
    let mut off = 0;
    for (i, l) in s.split_inclusive('\n').enumerate() {
        if i + 1 == line {
            return off + col.saturating_sub(1).min(l.len());
        }
        off += l.len();
    }
    s.len()
}

/// JSX's rule: body text spanning lines is trimmed per line, blank lines
/// dropped, and the rest joined with single spaces. Single-line text is
/// kept verbatim.
fn jsx_whitespace(raw: &str) -> String {
    if !raw.contains('\n') {
        return raw.to_owned();
    }
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|&j| j <= 10).and_then(|j| {
            let ent = &rest[1..j];
            let ch = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lbrace" => Some('{'),
                "rbrace" => Some('}'),
                _ => ent
                    .strip_prefix("#x")
                    .or_else(|| ent.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            ch.map(|c| (c, j))
        });
        match decoded {
            Some((c, j)) => {
                out.push(c);
                rest = &rest[j + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

// ── builder ──────────────────────────────────────────────────────────────

struct Builder<'a> {
    src: &'a str,
    fragment: bool,
    nodes: Vec<Node>,
    seen: HashMap<NodeId, usize>,
    variables: Variables,
}

/// One element's attributes, taken one by one; the conversion helpers
/// position their errors at the offending value.
struct Attrs<'e> {
    src: &'e str,
    tag: &'e str,
    map: HashMap<&'e str, &'e Attr>,
}

impl<'e> Attrs<'e> {
    fn err<T>(&self, a: &Attr, msg: impl Into<String>) -> Result<T> {
        Err(ParseError::at(
            self.src,
            a.value_pos,
            format!("`{}` on <{}>: {}", a.name, self.tag, msg.into()),
        ))
    }

    fn get(&self, name: &str) -> Option<&'e Attr> {
        self.map.get(name).copied()
    }

    fn num(&self, name: &str) -> Result<Option<f64>> {
        let Some(a) = self.get(name) else {
            return Ok(None);
        };
        let parsed = match &a.value {
            AttrValue::Bare(s) | AttrValue::Quoted(s) => s.trim().parse::<f64>().ok(),
            AttrValue::Json(Value::Number(n)) => n.as_f64(),
            _ => None,
        };
        match parsed {
            Some(v) => Ok(Some(v)),
            None => self.err(
                a,
                format!("expected a number like {name}=12, got {}", show(&a.value)),
            ),
        }
    }

    fn bool(&self, name: &str) -> Result<Option<bool>> {
        let Some(a) = self.get(name) else {
            return Ok(None);
        };
        match &a.value {
            AttrValue::Flag | AttrValue::Json(Value::Bool(true)) => Ok(Some(true)),
            AttrValue::Json(Value::Bool(false)) => Ok(Some(false)),
            AttrValue::Bare(s) | AttrValue::Quoted(s) if s == "true" => Ok(Some(true)),
            AttrValue::Bare(s) | AttrValue::Quoted(s) if s == "false" => Ok(Some(false)),
            v => self.err(a, format!("expected true or false, got {}", show(v))),
        }
    }

    fn str(&self, name: &str) -> Result<Option<String>> {
        let Some(a) = self.get(name) else {
            return Ok(None);
        };
        match &a.value {
            AttrValue::Bare(s) | AttrValue::Quoted(s) | AttrValue::Json(Value::String(s)) => {
                Ok(Some(s.clone()))
            }
            v => self.err(
                a,
                format!("expected a string like {name}=\"…\", got {}", show(v)),
            ),
        }
    }

    fn keyword<K: Keyword>(&self, name: &str) -> Result<Option<K>> {
        let Some(s) = self.str(name)? else {
            return Ok(None);
        };
        match K::from_keyword(&s) {
            Some(k) => Ok(Some(k)),
            None => self.err(
                self.map[name],
                format!("unknown value \"{s}\" — expected one of {}", K::expected()),
            ),
        }
    }

    fn id(&self, name: &str) -> Result<Option<NodeId>> {
        let Some(s) = self.str(name)? else {
            return Ok(None);
        };
        match uuid::Uuid::parse_str(s.trim()) {
            Ok(u) => Ok(Some(NodeId(u))),
            Err(_) => self.err(
                self.map[name],
                format!("\"{s}\" is not a UUID (omit `{name}` to generate one)"),
            ),
        }
    }

    /// Space- or comma-separated numbers (or a JSON array / single number).
    fn nums(&self, name: &str, allowed: &[usize], example: &str) -> Result<Option<Vec<f64>>> {
        let Some(a) = self.get(name) else {
            return Ok(None);
        };
        let parsed: Option<Vec<f64>> = match &a.value {
            AttrValue::Bare(s) | AttrValue::Quoted(s) => s
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|p| !p.is_empty())
                .map(|p| p.parse().ok())
                .collect(),
            AttrValue::Json(Value::Number(n)) => n.as_f64().map(|v| vec![v]),
            AttrValue::Json(Value::Array(xs)) => xs.iter().map(Value::as_f64).collect(),
            AttrValue::Flag | AttrValue::Json(_) => None,
        };
        match parsed {
            Some(v) if allowed.is_empty() || allowed.contains(&v.len()) => Ok(Some(v)),
            _ => self.err(a, format!("expected {example}, got {}", show(&a.value))),
        }
    }

    /// A `{json}` value (or a quoted/bare string where `strings` allows).
    fn json(&self, name: &str) -> Option<(&'e Attr, Value)> {
        let a = self.get(name)?;
        let v = match &a.value {
            AttrValue::Json(v) => v.clone(),
            AttrValue::Bare(s) | AttrValue::Quoted(s) => Value::String(s.clone()),
            AttrValue::Flag => Value::Bool(true),
        };
        Some((a, v))
    }

    fn convert<T>(
        &self,
        name: &str,
        f: impl FnOnce(Value) -> std::result::Result<T, String>,
    ) -> Result<Option<T>> {
        let Some((a, v)) = self.json(name) else {
            return Ok(None);
        };
        match f(v) {
            Ok(t) => Ok(Some(t)),
            Err(e) => self.err(a, e),
        }
    }
}

fn show(v: &AttrValue) -> String {
    match v {
        AttrValue::Flag => "no value".into(),
        AttrValue::Bare(s) => format!("`{s}`"),
        AttrValue::Quoted(s) => format!("\"{s}\""),
        AttrValue::Json(j) => format!("{{{j}}}"),
    }
}

impl<'a> Builder<'a> {
    fn new(src: &'a str, fragment: bool) -> Self {
        Self {
            src,
            fragment,
            nodes: Vec::new(),
            seen: HashMap::new(),
            variables: Variables::default(),
        }
    }

    fn err<T>(&self, at: usize, msg: impl Into<String>) -> Result<T> {
        Err(ParseError::at(self.src, at, msg))
    }

    fn build(&mut self, el: &El, parent: Option<NodeId>) -> Result<NodeId> {
        let tag = el.tag.as_str();
        if !TAGS.contains(&tag) {
            let hint =
                suggest_tag(tag).map_or(String::new(), |t| format!(" — did you mean <{t}>?"));
            return self.err(
                el.pos,
                format!(
                    "unknown element <{tag}>{hint} (elements: {})",
                    TAGS.join(", ")
                ),
            );
        }
        if self.fragment && (tag == "Document" || tag == "Page") {
            return self.err(
                el.pos,
                format!("<{tag}> can't appear in a fragment — send only the elements to insert (e.g. <Frame>…</Frame>)"),
            );
        }

        // Validate attribute names.
        let valid = attrs_for(tag);
        let mut map: HashMap<&str, &Attr> = HashMap::new();
        for a in &el.attrs {
            let name = a.name.as_str();
            let arg = name.strip_prefix(crate::document::text::schema::ARG_PREFIX);
            if arg.is_some() && !crate::document::text::schema::takes_args(tag) {
                return self.err(
                    a.pos,
                    format!("`{name}`: only a <Component> or an <Instance> takes arguments"),
                );
            }
            if arg.is_none() && !valid.contains(&name) {
                let msg = if let Some(owner) = owner_of(name) {
                    format!("attribute `{name}` is only valid on <{owner}>, not <{tag}>")
                } else {
                    let hint = suggest_attr(name, &valid)
                        .map_or(String::new(), |s| format!(" — did you mean `{s}`?"));
                    format!("unknown attribute `{name}` on <{tag}>{hint}")
                };
                return self.err(a.pos, msg);
            }
            if map.insert(name, a).is_some() {
                return self.err(a.pos, format!("attribute `{name}` given twice on <{tag}>"));
            }
        }
        let at = Attrs {
            src: self.src,
            tag,
            map,
        };

        let kind = self.kind(el, &at)?;
        let id = match at.id("id")? {
            Some(id) => id,
            None => NodeId::new(),
        };
        if let Some(prev) = self.seen.get(&id) {
            let (l, c) = crate::document::text::error::line_col(self.src, *prev);
            let pos = at.get("id").map_or(el.pos, |a| a.value_pos);
            return self.err(pos, format!("duplicate id \"{id}\" (first used at {l}:{c}) — ids must be unique; omit `id` on new elements"));
        }
        self.seen.insert(id, el.pos);

        let mut node = Node::new(kind, "");
        node.id = id;
        node.parent = parent;
        node.name = at
            .str("name")?
            .unwrap_or_else(|| default_name(&node.kind).to_owned());
        apply_common(&at, &mut node)?;

        let index = self.nodes.len();
        self.nodes.push(node);
        let mut children = Vec::with_capacity(el.children.len());
        for c in &el.children {
            if c.tag == VARIABLES_TAG {
                if tag != "Document" {
                    return self.err(
                        c.pos,
                        format!(
                            "<{VARIABLES_TAG}> belongs directly inside <Document>, not <{tag}>"
                        ),
                    );
                }
                self.variables(c)?;
                continue;
            }
            children.push(self.build(c, Some(id))?);
        }
        self.nodes[index].children = children;
        Ok(id)
    }

    /// `<Variables modes="dark light">` and its `<Var name=… value=… dark=…>`
    /// children: each attribute other than `name`/`value` is a mode's value.
    fn variables(&mut self, el: &El) -> Result<()> {
        if !self.variables.is_empty() {
            return self.err(
                el.pos,
                format!("a document has one <{VARIABLES_TAG}> block"),
            );
        }
        for a in &el.attrs {
            match (a.name.as_str(), &a.value) {
                ("modes", AttrValue::Bare(v) | AttrValue::Quoted(v)) => {
                    self.variables.modes = v
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .filter(|m| !m.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                (name, _) => {
                    return self.err(
                        a.pos,
                        format!(
                            "unknown attribute `{name}` on <{VARIABLES_TAG}> (it takes modes=\"…\")"
                        ),
                    );
                }
            }
        }
        for v in &el.children {
            if v.tag != "Var" {
                return self.err(
                    v.pos,
                    format!(
                        "<{VARIABLES_TAG}> holds <Var name=… value=…> elements, not <{}>",
                        v.tag
                    ),
                );
            }
            let mut name = None;
            let mut var = Variable::default();
            for a in &v.attrs {
                let text = match &a.value {
                    AttrValue::Bare(s) | AttrValue::Quoted(s) => s.clone(),
                    AttrValue::Json(j) => j.to_string(),
                    AttrValue::Flag => "true".to_owned(),
                };
                match a.name.as_str() {
                    "name" => name = Some(text),
                    "value" => var.value = text,
                    mode if self.variables.modes.iter().any(|m| m == mode) => {
                        var.modes.insert(mode.to_owned(), text);
                    }
                    other => {
                        return self.err(
                            a.pos,
                            format!(
                                "`{other}` on <Var> is not a mode (modes: {})",
                                self.variables.modes.join(", ")
                            ),
                        );
                    }
                }
            }
            let Some(name) = name else {
                return self.err(v.pos, "<Var> needs name=\"…\"");
            };
            if var.value.is_empty() {
                // The first mode's value stands in for modes that don't say.
                if let Some(first) = self.variables.modes.first().and_then(|m| var.modes.get(m)) {
                    var.value = first.clone();
                } else {
                    return self.err(
                        v.pos,
                        format!("<Var name=\"{name}\"> needs value=… or a value for each mode"),
                    );
                }
            }
            if self.variables.vars.insert(name.clone(), var).is_some() {
                return self.err(v.pos, format!("variable `{name}` defined twice"));
            }
        }
        Ok(())
    }

    fn kind(&self, el: &El, at: &Attrs<'_>) -> Result<NodeKind> {
        Ok(match el.tag.as_str() {
            "Document" => NodeKind::Document,
            "Page" => NodeKind::Page,
            "Frame" => NodeKind::Frame,
            "Group" => NodeKind::Group,
            "Component" => NodeKind::Component,
            "ComponentSet" => NodeKind::ComponentSet,
            "Instance" => NodeKind::Instance {
                component: at.id("component")?,
            },
            "Rectangle" => NodeKind::Rectangle,
            "Ellipse" => NodeKind::Ellipse,
            "Line" => NodeKind::Line,
            "Polygon" => NodeKind::Polygon,
            "Star" => NodeKind::Star,
            "Vector" => NodeKind::Vector,
            "BooleanOperation" => NodeKind::BooleanOperation {
                op: at.keyword::<BooleanOp>("op")?,
            },
            "Utility" => NodeKind::Utility,
            "Other" => match at.str("tag")? {
                Some(tag) => NodeKind::Other { tag },
                None => {
                    return self.err(
                        el.pos,
                        "<Other> needs tag=\"…\" naming the source node type",
                    );
                }
            },
            "Text" => NodeKind::Text(text_data(el, at)?),
            _ => unreachable!("tag validated against TAGS"),
        })
    }
}

fn text_data(el: &El, at: &Attrs<'_>) -> Result<TextData> {
    let characters = match (at.str("text")?, &el.body) {
        (Some(_), Some((_, pos))) => {
            return Err(ParseError::at(
                at.src,
                *pos,
                "<Text> has both text=\"…\" and body text — use one",
            ));
        }
        (Some(s), None) => s,
        (None, Some((s, _))) => s.clone(),
        (None, None) => String::new(),
    };
    let mut t = TextData {
        characters,
        ..TextData::default()
    };
    t.font_family = at.str("font")?;
    if let Some(v) = at.num("size")? {
        t.font_size = v;
    }
    t.font_weight = at.num("weight")?;
    t.font_style = at.str("style")?;
    t.font_width = at.num("stretch")?;
    t.line_height = at.num("line-height")?;
    if let Some(v) = at.num("letter-spacing")? {
        t.letter_spacing = v;
    }
    if let Some(v) = at.keyword::<TextCase>("case")? {
        t.text_case = v;
    }
    if let Some(v) = at.keyword::<TextAlign>("text-align")? {
        t.align = v;
    }
    if let Some(v) = at.convert("runs", runs_from_json)? {
        t.runs = v;
    }
    Ok(t)
}

fn apply_common(at: &Attrs<'_>, n: &mut Node) -> Result<()> {
    if let Some(v) = at.bool("visible")? {
        n.visible = v;
    }
    if let Some(v) = at.bool("locked")? {
        n.locked = v;
    }

    // Transform: `transform` (full matrix) or `rotation`, then x/y.
    if let (Some(_), Some(r)) = (at.get("transform"), at.get("rotation")) {
        return at.err(r, "use either `rotation` or `transform`, not both");
    }
    if let Some(t) = at.convert("transform", transform_from_json)? {
        n.transform = t;
    }
    if let Some(deg) = at.num("rotation")? {
        (n.transform.a, n.transform.b, n.transform.c, n.transform.d) = rotation_matrix(deg);
    }
    if let Some(v) = at.num("x")? {
        n.transform.tx = v;
    }
    if let Some(v) = at.num("y")? {
        n.transform.ty = v;
    }

    if let Some(v) = at.num("w")? {
        n.size.width = v;
    }
    if let Some(v) = at.num("h")? {
        n.size.height = v;
    }
    if let Some(v) = at.num("opacity")? {
        n.opacity = v;
    }
    if let Some(v) = at.keyword::<BlendMode>("blend")? {
        n.blend = v;
    }
    if let Some(r) = at.nums("radius", &[1, 4], "radius=8 or radius=\"tl tr br bl\"")? {
        n.corner_radii = match r.as_slice() {
            [u] => CornerRadii::uniform(*u),
            [tl, tr, br, bl] => CornerRadii {
                top_left: *tl,
                top_right: *tr,
                bottom_right: *br,
                bottom_left: *bl,
            },
            _ => unreachable!("arity checked"),
        };
    }
    if let Some(v) = at.bool("clip")? {
        n.clips_content = v;
    }
    n.mask = at.keyword::<MaskKind>("mask")?;
    if let Some(v) = at.convert("fill", paints_value)? {
        n.fills = v;
    }

    if let Some(v) = at.convert("stroke", paints_value)? {
        n.stroke.paints = v;
    }
    if let Some(v) = at.num("stroke-weight")? {
        n.stroke.weight = v;
    }
    if let Some(v) = at.keyword::<StrokeAlign>("stroke-align")? {
        n.stroke.align = v;
    }
    if let Some(v) = at.keyword::<StrokeCap>("stroke-cap")? {
        n.stroke.cap = v;
    }
    if let Some(v) = at.keyword::<StrokeJoin>("stroke-join")? {
        n.stroke.join = v;
    }
    if let Some(v) = at.num("stroke-miter")? {
        n.stroke.miter_limit = v;
    }
    if let Some(v) = at.nums("stroke-dash", &[], "stroke-dash=\"4 2\" (dash gap …)")? {
        n.stroke.dash_pattern = v;
    }
    if let Some(v) = at.num("stroke-dash-offset")? {
        n.stroke.dash_offset = v;
    }
    if let Some(v) = at.convert("effects", effects_from_json)? {
        n.effects = v;
    }

    // Auto layout.
    match at.str("layout")? {
        Some(dir) => {
            let direction = match dir.to_ascii_lowercase().as_str() {
                "row" | "horizontal" | "h" => LayoutDirection::Horizontal,
                "column" | "col" | "vertical" | "v" => LayoutDirection::Vertical,
                "none" => {
                    return at.err(
                        at.map["layout"],
                        "omit `layout` for no auto layout (\"none\" isn't a value)",
                    );
                }
                _ => {
                    return at.err(
                        at.map["layout"],
                        format!("unknown value \"{dir}\" — expected \"row\" or \"column\""),
                    );
                }
            };
            let mut l = AutoLayout {
                direction,
                gap: at.num("gap")?.unwrap_or(0.0),
                padding_left: 0.0,
                padding_right: 0.0,
                padding_top: 0.0,
                padding_bottom: 0.0,
                wrap: at.bool("wrap")?.unwrap_or(false),
                main_align: at.keyword::<MainAxisAlign>("justify")?,
                cross_align: at.keyword::<CrossAxisAlign>("align")?,
                hug_main: false,
                hug_cross: false,
            };
            let hug = match at.bool("hug") {
                Ok(Some(true)) => Some("both".to_string()),
                Ok(Some(false)) => None,
                _ => at.str("hug")?,
            };
            match hug.as_deref() {
                None => {}
                Some("both") => (l.hug_main, l.hug_cross) = (true, true),
                Some("main") => l.hug_main = true,
                Some("cross") => l.hug_cross = true,
                Some(other) => {
                    return at.err(at.map["hug"], format!("unknown value \"{other}\" — expected `hug` (both axes), \"main\" or \"cross\""));
                }
            }
            if let Some(p) = at.nums(
                "padding",
                &[1, 2, 3, 4],
                "padding=12 or padding=\"top right bottom left\"",
            )? {
                (
                    l.padding_top,
                    l.padding_right,
                    l.padding_bottom,
                    l.padding_left,
                ) = match p.as_slice() {
                    [u] => (*u, *u, *u, *u),
                    [v, h] => (*v, *h, *v, *h),
                    [t, h, b] => (*t, *h, *b, *h),
                    [t, r, b, l] => (*t, *r, *b, *l),
                    _ => unreachable!("arity checked"),
                };
            }
            n.layout = Some(l);
        }
        None => {
            if let Some(name) = LAYOUT_ATTRS.iter().find(|a| at.get(a).is_some()) {
                return at.err(
                    at.map[name],
                    "only applies to auto-layout containers — add layout=\"row\" or layout=\"column\" to this element",
                );
            }
        }
    }

    let lc = &mut n.layout_child;
    lc.grow = at.num("grow")?;
    lc.align_self = at.keyword::<AlignSelf>("align-self")?;
    lc.absolute = at.bool("absolute")?.unwrap_or(false);
    lc.min_width = at.num("min-w")?;
    lc.min_height = at.num("min-h")?;
    lc.max_width = at.num("max-w")?;
    lc.max_height = at.num("max-h")?;
    lc.resize_x = at.keyword::<ResizeX>("resize-x")?;
    lc.resize_y = at.keyword::<ResizeY>("resize-y")?;

    for name in BIND_ATTRS {
        if let Some(expr) = at.str(name)? {
            n.bind.push(Bind {
                prop: name.trim_start_matches("bind:").to_owned(),
                expr,
            });
        }
    }
    n.of = at.str("of")?;
    // In the order written: a component's arguments read best as declared.
    let mut args: Vec<(&str, usize)> = at
        .map
        .iter()
        .filter_map(|(k, a)| {
            k.strip_prefix(crate::document::text::schema::ARG_PREFIX)
                .map(|name| (name, a.pos))
        })
        .collect();
    args.sort_by_key(|(_, pos)| *pos);
    for (name, _) in args {
        if let Some(expr) = at.str(&format!(
            "{}{name}",
            crate::document::text::schema::ARG_PREFIX
        ))? {
            n.args.push(crate::document::proto::Arg {
                name: name.to_owned(),
                expr,
            });
        }
    }

    if let Some(v) = at.convert("geometry", geometry_value)? {
        n.fill_geometry = v;
    }
    if let Some(v) = at.convert("stroke-geometry", geometry_value)? {
        n.stroke_geometry = v;
    }
    n.svg_snapshot = at.str("svg-snapshot")?.map(AssetId);
    n.control = at.convert("control", from_json)?;
    n.emission = at.convert("emission", from_json)?;
    n.shader = at.convert("shader", from_json)?;
    n.motion = at.convert("motion", motion_value)?.unwrap_or_default();
    n.source = at.convert("source", source_value)?;
    Ok(())
}

fn paints_value(v: Value) -> std::result::Result<Vec<crate::document::proto::Paint>, String> {
    match v {
        Value::String(s) if s == "none" || s.is_empty() => Ok(Vec::new()),
        Value::String(s) if parse_hex(&s).is_none() => Err(format!(
            "\"{s}\" is not a color — write \"#rrggbb\" / \"#rrggbbaa\", or a paint list {{[…]}}"
        )),
        v => paints_from_json(v),
    }
}

/// `motion={[…]}` (a list) or `motion={{…}}` (one motion). Expressions
/// are checked by lint (`motion` rule), not here, so any scene prints and
/// parses back.
fn motion_value(
    v: Value,
) -> std::result::Result<Vec<crate::document::proto::motion::Motion>, String> {
    match v {
        Value::Array(_) => from_json(v),
        v => Ok(vec![from_json(v)?]),
    }
}

fn geometry_value(v: Value) -> std::result::Result<Vec<String>, String> {
    match v {
        Value::String(s) => Ok(vec![s]),
        v => serde_json::from_value(v)
            .map_err(|_| "expected SVG path data \"M0 0 L…\" or a list {[\"…\", …]}".to_owned()),
    }
}

fn source_value(v: Value) -> std::result::Result<SourceRef, String> {
    match v {
        Value::String(s) => {
            let mut parts = s.splitn(3, '/');
            match (parts.next(), parts.next(), parts.next()) {
                (Some(tool), Some(node_type), Some(id)) => Ok(SourceRef {
                    tool: tool.to_owned(),
                    node_type: node_type.to_owned(),
                    id: id.to_owned(),
                    bounds: None,
                }),
                _ => Err(format!(
                    "expected \"tool/TYPE/id\" (e.g. \"figma/FRAME/1:2\"), got \"{s}\""
                )),
            }
        }
        v => from_json(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_rule() {
        assert_eq!(jsx_whitespace("  Gain "), "  Gain ");
        assert_eq!(jsx_whitespace("\n   Hello\n   world\n  "), "Hello world");
    }

    #[test]
    fn entities() {
        assert_eq!(
            unescape("a &amp; b &lt;c&gt; &lbrace; &#65;&#x42; & ;"),
            "a & b <c> { AB & ;"
        );
    }
}
