//! Minimal XML string builder with proper attribute and text escaping.
//!
//! Writes indented XML without pulling in an extra crate.

pub struct XmlWriter {
    buf: String,
    indent: usize,
}

impl Default for XmlWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl XmlWriter {
    pub fn new() -> Self {
        Self {
            buf: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
            indent: 0,
        }
    }

    /// Write an opening tag with optional attributes, then indent children.
    pub fn open(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        self.push_indent();
        self.buf.push('<');
        self.buf.push_str(tag);
        self.push_attrs(attrs);
        self.buf.push_str(">\n");
        self.indent += 1;
    }

    /// Write a self-closing empty element.
    pub fn empty(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        self.push_indent();
        self.buf.push('<');
        self.buf.push_str(tag);
        self.push_attrs(attrs);
        self.buf.push_str(" />\n");
    }

    /// Close a previously opened tag.
    pub fn close(&mut self, tag: &str) {
        self.indent = self.indent.saturating_sub(1);
        self.push_indent();
        self.buf.push_str("</");
        self.buf.push_str(tag);
        self.buf.push_str(">\n");
    }

    /// Write an element with a text body (escaped).
    pub fn text_elem(&mut self, tag: &str, attrs: &[(&str, &str)], text: &str) {
        self.push_indent();
        self.buf.push('<');
        self.buf.push_str(tag);
        self.push_attrs(attrs);
        self.buf.push('>');
        escape_text(text, &mut self.buf);
        self.buf.push_str("</");
        self.buf.push_str(tag);
        self.buf.push_str(">\n");
    }

    /// Run a closure between open/close, returning `&mut self` for chaining.
    pub fn wrap<F: FnOnce(&mut Self)>(&mut self, tag: &str, attrs: &[(&str, &str)], f: F) {
        self.open(tag, attrs);
        f(self);
        self.close(tag);
    }

    /// Consume the writer and return the complete XML string.
    pub fn finish(self) -> String {
        self.buf
    }

    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            self.buf.push_str("  ");
        }
    }

    fn push_attrs(&mut self, attrs: &[(&str, &str)]) {
        for (k, v) in attrs {
            self.buf.push(' ');
            self.buf.push_str(k);
            self.buf.push_str("=\"");
            escape_attr(v, &mut self.buf);
            self.buf.push('"');
        }
    }
}

/// Escape characters that are illegal inside XML attribute values (double-quoted).
fn escape_attr(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

/// Escape characters that are illegal inside XML text content.
fn escape_text(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}
