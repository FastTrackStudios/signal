//! Groove pool parsing.
//!
//! Grooves live under `LiveSet.GroovePool.Grooves`:
//! ```xml
//! <GroovePool>
//!   <Grooves>
//!     <Groove Id="0">
//!       <Name Value="Swing 8-52" />
//!       <FileRef>
//!         <Path Value="..." />
//!       </FileRef>
//!       <Base Value="0.5" />
//!       <QuantizeAmount Value="0.0" />
//!       <TimingAmount Value="1.0" />
//!       <RandomAmount Value="0.0" />
//!       <VelocityAmount Value="0.0" />
//!     </Groove>
//!   </Grooves>
//! </GroovePool>
//! ```

use super::xml_helpers::*;
use crate::types::Groove;
use roxmltree::Node;

/// Parse all grooves from a `GroovePool` node.
pub fn parse_groove_pool(groove_pool: Node<'_, '_>) -> Vec<Groove> {
    let grooves_node = match child(groove_pool, "Grooves") {
        Some(g) => g,
        None => return Vec::new(),
    };

    grooves_node
        .children()
        .filter(|n| n.has_tag_name("Groove"))
        .map(|groove| {
            let id = id_attr(groove);
            let name = child_value(groove, "Name").unwrap_or("").to_string();
            let path = child(groove, "FileRef")
                .and_then(|fr| child_value(fr, "Path"))
                .unwrap_or("")
                .to_string();
            let base = child_f64(groove, "Base").unwrap_or(0.5);
            let quantize_amount = child_f64(groove, "QuantizeAmount").unwrap_or(0.0);
            let timing_amount = child_f64(groove, "TimingAmount").unwrap_or(0.0);
            let random_amount = child_f64(groove, "RandomAmount").unwrap_or(0.0);
            let velocity_amount = child_f64(groove, "VelocityAmount").unwrap_or(0.0);

            Groove {
                id,
                name,
                path,
                base,
                quantize_amount,
                timing_amount,
                random_amount,
                velocity_amount,
            }
        })
        .collect()
}
