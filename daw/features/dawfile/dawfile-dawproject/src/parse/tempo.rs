//! Parse transport settings (tempo, time signature).

use super::xml_helpers::*;
use crate::types::Transport;
use roxmltree::Node;

/// Parse the `<Transport>` element.
pub fn parse_transport(transport: Node<'_, '_>) -> Transport {
    let mut result = Transport::default();

    if let Some(tempo_node) = child(transport, "Tempo") {
        result.tempo = attr_f64(tempo_node, "value", 120.0);
    }

    if let Some(ts_node) = child(transport, "TimeSignature") {
        result.numerator = attr_u8(ts_node, "numerator", 4);
        result.denominator = attr_u8(ts_node, "denominator", 4);
    }

    result
}
