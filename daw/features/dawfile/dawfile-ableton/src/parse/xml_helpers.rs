//! XML navigation helpers for the Ableton XML schema.
//!
//! Ableton's XML uses a consistent pattern where scalar values are stored as
//! `<Element Value="..." />` attributes. These helpers simplify navigating
//! that structure.

use roxmltree::Node;

/// Get the `Value` attribute of a named child element as a string.
pub fn child_value<'a>(node: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|n| n.has_tag_name(name))
        .and_then(|n| n.attribute("Value"))
}

/// Get the `Value` attribute of a named child element, parsed as `T`.
pub fn child_value_parse<T: std::str::FromStr>(node: Node<'_, '_>, name: &str) -> Option<T> {
    child_value(node, name).and_then(|v| v.parse().ok())
}

/// Get the `Value` attribute as f64 from a named child.
pub fn child_f64(node: Node<'_, '_>, name: &str) -> Option<f64> {
    child_value_parse(node, name)
}

/// Get the `Value` attribute as i32 from a named child.
pub fn child_i32(node: Node<'_, '_>, name: &str) -> Option<i32> {
    child_value_parse(node, name)
}

/// Get the `Value` attribute as bool from a named child.
pub fn child_bool(node: Node<'_, '_>, name: &str) -> Option<bool> {
    child_value(node, name).map(|v| v == "true")
}

/// Find a named child element.
pub fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|n| n.has_tag_name(name))
}

/// Find all children with a given tag name.
pub fn children_with_tag<'a, 'input>(
    node: Node<'a, 'input>,
    name: &str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(move |n| n.has_tag_name(name))
}

/// Navigate a dot-separated path of element names.
/// e.g., `descend(root, "LiveSet.MasterTrack.DeviceChain.Mixer")`
pub fn descend<'a, 'input>(node: Node<'a, 'input>, path: &str) -> Option<Node<'a, 'input>> {
    let mut current = node;
    for segment in path.split('.') {
        current = child(current, segment)?;
    }
    Some(current)
}

/// Get the `Id` attribute as i32 from a node.
pub fn id_attr(node: Node<'_, '_>) -> i32 {
    node.attribute("Id")
        .and_then(|v| v.parse().ok())
        .unwrap_or(-1)
}

/// Get the `Time` attribute as f64 from a node.
pub fn time_attr(node: Node<'_, '_>) -> f64 {
    node.attribute("Time")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Collect all `CurrentEnd` values from the entire document to find
/// the furthest bar position.
pub fn collect_max_current_end(node: Node<'_, '_>) -> f64 {
    let mut max = 0.0f64;
    for descendant in node.descendants() {
        if descendant.has_tag_name("CurrentEnd")
            && let Some(v) = descendant
                .attribute("Value")
                .and_then(|v| v.parse::<f64>().ok())
        {
            max = max.max(v);
        }
    }
    max
}
