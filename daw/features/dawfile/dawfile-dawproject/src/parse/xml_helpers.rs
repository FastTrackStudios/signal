//! XML helper utilities for parsing DawProject XML.

use roxmltree::Node;

/// Get the first child element with the given tag name.
pub fn child<'a>(node: Node<'a, '_>, name: &str) -> Option<Node<'a, 'a>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

/// Iterate over all direct child elements with the given tag name.
pub fn children<'a>(node: Node<'a, 'a>, name: &str) -> impl Iterator<Item = Node<'a, 'a>> {
    node.children()
        .filter(move |n| n.is_element() && n.tag_name().name() == name)
}

/// Iterate over all direct child elements.
pub fn child_elements<'a>(node: Node<'a, 'a>) -> impl Iterator<Item = Node<'a, 'a>> {
    node.children().filter(|n| n.is_element())
}

/// Get an attribute value, returning `None` if missing.
pub fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute(name)
}

/// Get an attribute value as `f64`, returning a default if missing or unparseable.
pub fn attr_f64(node: Node<'_, '_>, name: &str, default: f64) -> f64 {
    node.attribute(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Get an attribute value as `u32`, returning a default if missing or unparseable.
pub fn attr_u32(node: Node<'_, '_>, name: &str, default: u32) -> u32 {
    node.attribute(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Get an attribute value as `u8`, returning a default if missing or unparseable.
pub fn attr_u8(node: Node<'_, '_>, name: &str, default: u8) -> u8 {
    node.attribute(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Get an attribute as a bool ("true"/"false"), returning a default if missing.
pub fn attr_bool(node: Node<'_, '_>, name: &str, default: bool) -> bool {
    match node.attribute(name) {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}
