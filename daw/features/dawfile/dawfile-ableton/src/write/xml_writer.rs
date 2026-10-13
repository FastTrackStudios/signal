//! XML writing helpers for generating Ableton-compatible XML.
//!
//! Ableton's XML uses a consistent pattern:
//! - Scalar values: `<Element Value="..." />`
//! - Objects with identity: `<Element Id="N">...</Element>`
//! - Clips: `<MidiClip Id="N" Time="T">...</MidiClip>`
//!
//! This module provides ergonomic helpers built on `quick_xml::Writer`.

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::io::Write;

/// A wrapper around `quick_xml::Writer` with Ableton-specific helpers.
pub struct AbletonXmlWriter<W: Write> {
    writer: Writer<W>,
    /// Next available automation target ID.
    next_auto_id: i32,
}

impl<W: Write> AbletonXmlWriter<W> {
    pub fn new(inner: W) -> Self {
        let writer = Writer::new_with_indent(inner, b'\t', 1);
        Self {
            writer,
            next_auto_id: 1,
        }
    }

    /// Allocate the next automation target ID.
    pub fn next_id(&mut self) -> i32 {
        let id = self.next_auto_id;
        self.next_auto_id += 1;
        id
    }

    /// Write the XML declaration.
    pub fn write_declaration(&mut self) -> std::io::Result<()> {
        self.writer
            .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
    }

    /// Write `<Tag Value="v" />` (self-closing element with Value attribute).
    pub fn value_element(&mut self, tag: &str, value: &str) -> std::io::Result<()> {
        let mut elem = BytesStart::new(tag);
        elem.push_attribute(("Value", value));
        self.writer.write_event(Event::Empty(elem))
    }

    /// Write `<Tag Value="N" />` for an integer.
    pub fn value_int(&mut self, tag: &str, value: i64) -> std::io::Result<()> {
        self.value_element(tag, &value.to_string())
    }

    /// Write `<Tag Value="N" />` for a float (Ableton uses variable precision).
    pub fn value_float(&mut self, tag: &str, value: f64) -> std::io::Result<()> {
        // Ableton typically uses minimal precision
        let s = if value.fract() == 0.0 {
            format!("{value:.0}")
        } else {
            // Remove trailing zeros but keep at least one decimal
            let raw = format!("{value}");
            raw.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        self.value_element(tag, &s)
    }

    /// Write `<Tag Value="true|false" />`.
    pub fn value_bool(&mut self, tag: &str, value: bool) -> std::io::Result<()> {
        self.value_element(tag, if value { "true" } else { "false" })
    }

    /// Open an element: `<Tag>`.
    pub fn start(&mut self, tag: &str) -> std::io::Result<()> {
        self.writer.write_event(Event::Start(BytesStart::new(tag)))
    }

    /// Open an element with an Id attribute: `<Tag Id="N">`.
    pub fn start_with_id(&mut self, tag: &str, id: i32) -> std::io::Result<()> {
        let mut elem = BytesStart::new(tag);
        elem.push_attribute(("Id", id.to_string().as_str()));
        self.writer.write_event(Event::Start(elem))
    }

    /// Close an element: `</Tag>`.
    pub fn end(&mut self, tag: &str) -> std::io::Result<()> {
        self.writer.write_event(Event::End(BytesEnd::new(tag)))
    }

    /// Write an empty element: `<Tag />`.
    pub fn empty(&mut self, tag: &str) -> std::io::Result<()> {
        self.writer.write_event(Event::Empty(BytesStart::new(tag)))
    }

    /// Write an empty element with Id: `<Tag Id="N" />`.
    pub fn empty_with_id(&mut self, tag: &str, id: i32) -> std::io::Result<()> {
        let mut elem = BytesStart::new(tag);
        elem.push_attribute(("Id", id.to_string().as_str()));
        self.writer.write_event(Event::Empty(elem))
    }

    /// Write a `<Tag>text</Tag>` element.
    pub fn text_element(&mut self, tag: &str, text: &str) -> std::io::Result<()> {
        self.start(tag)?;
        self.writer.write_event(Event::Text(BytesText::new(text)))?;
        self.end(tag)
    }

    /// Write a standard Ableton automation target block.
    pub fn automation_target(&mut self, tag: &str) -> std::io::Result<i32> {
        let id = self.next_id();
        let mut elem = BytesStart::new(tag);
        elem.push_attribute(("Id", id.to_string().as_str()));
        self.writer.write_event(Event::Start(elem))?;
        self.value_int("LockEnvelope", 0)?;
        self.end(tag)?;
        Ok(id)
    }

    /// Write an element with custom attributes (for clip/event elements).
    pub fn start_with_attrs(&mut self, tag: &str, attrs: &[(&str, &str)]) -> std::io::Result<()> {
        let mut elem = BytesStart::new(tag);
        for (key, val) in attrs {
            elem.push_attribute((*key, *val));
        }
        self.writer.write_event(Event::Start(elem))
    }

    /// Write a self-closing element with custom attributes.
    pub fn empty_with_attrs(&mut self, tag: &str, attrs: &[(&str, &str)]) -> std::io::Result<()> {
        let mut elem = BytesStart::new(tag);
        for (key, val) in attrs {
            elem.push_attribute((*key, *val));
        }
        self.writer.write_event(Event::Empty(elem))
    }

    /// Write raw XML bytes directly (for passthrough of unparsed XML fragments).
    pub fn write_raw(&mut self, xml: &str) -> std::io::Result<()> {
        self.writer.get_mut().write_all(xml.as_bytes())
    }

    /// Consume and return the inner writer.
    pub fn into_inner(self) -> W {
        self.writer.into_inner()
    }
}
