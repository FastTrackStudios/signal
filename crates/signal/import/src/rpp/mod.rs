//! Reading, editing and writing REAPER `.rpp` project files.
//!
//! The point of this module is FX substitution: find every instance of a
//! foreign plugin in a project, translate its state, and write an FTS plugin
//! in its place. That job has one hard requirement the rest of the design
//! follows from — **everything the converter does not touch must come back
//! byte for byte**. A project file is the session; a converter that reflows
//! whitespace or drops an unrecognised token is not a converter, it is a
//! corruption with a good excuse.
//!
//! So the model keeps raw lines. A [`Block`] holds the literal header line,
//! the literal footer line, and its children in order; a [`Node::Line`] is
//! the literal line. [`Document::to_string`] joins them back with the
//! original newline, and `parse -> to_string` on an untouched file is the
//! identity. Only the blocks the converter replaces are ever re-rendered.
//!
//! ## Grammar
//!
//! `.rpp` is a line-oriented tree. A line whose first non-space character is
//! `<` opens a block and names it with the token that follows; a line that is
//! exactly `>` closes the innermost one. Everything else is a leaf. Base64
//! payloads (a `<VST>` body, a `<STATE>` body) are leaves like any other —
//! they are just lines, which is why the parser needs no knowledge of them.
//!
//! REAPER also emits `<` inside quoted strings, so the open test only fires
//! when the `<` is the first non-space character on the line.

use std::fmt::Write as _;

pub mod chunk;
pub mod convert;
pub mod fts_eq;

/// One node of the project tree: either a literal line or a nested block.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A leaf line, stored exactly as it was read (indentation included).
    Line(String),
    Block(Block),
}

/// A `<TOKEN ...> ... >` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The opening line, verbatim: `    <VST "VST3: …" … ""`.
    pub header: String,
    pub children: Vec<Node>,
    /// The closing line, verbatim (`    >`). Empty only for an unterminated
    /// block at end of file, which we tolerate rather than reject.
    pub footer: String,
}

impl Block {
    /// The block's name — the token right after the `<`.
    pub fn token(&self) -> &str {
        let s = self.header.trim_start();
        let s = s.strip_prefix('<').unwrap_or(s);
        s.split_whitespace().next().unwrap_or("")
    }

    /// The leading whitespace of the header line, so a replacement block can
    /// be rendered at the same depth.
    pub fn indent(&self) -> &str {
        let end = self
            .header
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(self.header.len());
        &self.header[..end]
    }

    /// The base64 body of a state block: every child line, trimmed.
    ///
    /// A `<VST>` block's children are exactly its base64 lines; a `<CLAP>`
    /// block wraps them one level deeper in `<STATE>`, so callers reach for
    /// [`Self::child_block`] first.
    pub fn base64_lines(&self) -> Vec<String> {
        self.children
            .iter()
            .filter_map(|n| match n {
                Node::Line(l) => Some(l.trim().to_string()),
                Node::Block(_) => None,
            })
            .filter(|l| !l.is_empty())
            .collect()
    }

    /// The first direct child block named `token`.
    pub fn child_block(&self, token: &str) -> Option<&Block> {
        self.children.iter().find_map(|n| match n {
            Node::Block(b) if b.token() == token => Some(b),
            _ => None,
        })
    }

    fn render(&self, out: &mut String) {
        let _ = writeln!(out, "{}", self.header);
        for child in &self.children {
            match child {
                Node::Line(l) => {
                    let _ = writeln!(out, "{l}");
                }
                Node::Block(b) => b.render(out),
            }
        }
        if !self.footer.is_empty() {
            let _ = writeln!(out, "{}", self.footer);
        }
    }
}

/// A parsed project file.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Top-level nodes. A well-formed `.rpp` has exactly one — the `<REAPER_PROJECT>`
    /// block — but the same parser reads `.RfxChain` and `.RTrackTemplate`
    /// files, which have several, so nothing here assumes one.
    pub nodes: Vec<Node>,
    /// Whether the file used CRLF. Preserved so a Windows-authored project
    /// does not come back with its line endings rewritten.
    crlf: bool,
}

impl Document {
    pub fn parse(text: &str) -> Document {
        let crlf = text.contains("\r\n");
        let body = text.replace("\r\n", "\n");
        let mut lines = body.lines();
        let mut nodes = Vec::new();
        parse_into(&mut lines, &mut nodes);
        Document { nodes, crlf }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for node in &self.nodes {
            match node {
                Node::Line(l) => {
                    let _ = writeln!(out, "{l}");
                }
                Node::Block(b) => b.render(&mut out),
            }
        }
        if self.crlf {
            out = out.replace('\n', "\r\n");
        }
        out
    }

    /// Every block in the tree, depth-first, with the path of enclosing block
    /// tokens that leads to it.
    pub fn walk(&self) -> Vec<BlockRef<'_>> {
        let mut out = Vec::new();
        walk_nodes(&self.nodes, &mut Vec::new(), &mut out);
        out
    }
}

/// A block found by [`Document::walk`], with enough context to report where
/// it lives — the enclosing block tokens, outermost first.
#[derive(Debug, Clone)]
pub struct BlockRef<'a> {
    pub block: &'a Block,
    pub path: Vec<&'a str>,
}

fn walk_nodes<'a>(nodes: &'a [Node], path: &mut Vec<&'a str>, out: &mut Vec<BlockRef<'a>>) {
    for n in nodes {
        if let Node::Block(b) = n {
            out.push(BlockRef {
                block: b,
                path: path.clone(),
            });
            path.push(b.token());
            walk_nodes(&b.children, path, out);
            path.pop();
        }
    }
}

/// True when this line opens a block: its first non-space character is `<`.
fn opens_block(line: &str) -> bool {
    line.trim_start().starts_with('<')
}

/// True when this line closes a block: it is a bare `>`.
fn closes_block(line: &str) -> bool {
    line.trim() == ">"
}

fn parse_into(lines: &mut std::str::Lines<'_>, out: &mut Vec<Node>) -> Option<String> {
    while let Some(line) = lines.next() {
        if closes_block(line) {
            return Some(line.to_string());
        }
        if opens_block(line) {
            let mut children = Vec::new();
            let footer = parse_into(lines, &mut children).unwrap_or_default();
            out.push(Node::Block(Block {
                header: line.to_string(),
                children,
                footer,
            }));
        } else {
            out.push(Node::Line(line.to_string()));
        }
    }
    None
}

/// Split a `.rpp` header line into its whitespace-separated fields, honouring
/// REAPER's three quote characters.
///
/// REAPER picks a quote the value does not contain — `"` normally, `'` if the
/// value has a double quote, a backtick if it has both — so a field is not
/// splittable on whitespace alone. Returned fields keep their quotes, because
/// the converter's job is to reproduce them.
pub fn split_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' || c == '`' => {
                quote = Some(c);
                cur.push(c);
            }
            None if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Strip one layer of REAPER quoting from a field.
pub fn unquote(field: &str) -> &str {
    let mut chars = field.chars();
    match (chars.next(), field.chars().last()) {
        (Some(a), Some(b)) if a == b && (a == '"' || a == '\'' || a == '`') && field.len() >= 2 => {
            &field[1..field.len() - 1]
        }
        _ => field,
    }
}

/// Quote a value the way REAPER does: pick a delimiter the value lacks.
pub fn quote(value: &str) -> String {
    let q = if !value.contains('"') {
        '"'
    } else if !value.contains('\'') {
        '\''
    } else {
        '`'
    };
    format!("{q}{value}{q}")
}
