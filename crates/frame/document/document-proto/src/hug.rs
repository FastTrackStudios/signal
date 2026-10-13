//! "Hug contents": auto-layout containers flagged `hug_main`/`hug_cross`
//! size themselves to their children. An authoring rule, applied by editors
//! as part of each edit (so it's undoable and synced like any other change);
//! renderers just use the stored sizes.

use crate::model::{LayoutDirection, NodeId, Scene, Size};
use crate::ops::{Op, Prop};

impl Scene {
    /// `Set Size` ops that bring every hugging container under `root` to its
    /// content size (children first, so nested hugs compose).
    #[must_use]
    pub fn hug_ops(&self, root: NodeId) -> Vec<Op> {
        let mut sizes: std::collections::HashMap<NodeId, Size> = std::collections::HashMap::new();
        let mut ops = Vec::new();
        // Post-order: reverse of a pre-order walk visits children before parents.
        for id in self.walk(root).into_iter().rev() {
            let Some(node) = self.node(id) else { continue };
            let Some(al) = &node.layout else { continue };
            if !(al.hug_main || al.hug_cross) {
                continue;
            }
            let kids: Vec<Size> = node
                .children
                .iter()
                .filter_map(|c| self.node(*c))
                .filter(|c| c.visible && !c.layout_child.absolute)
                .map(|c| sizes.get(&c.id).copied().unwrap_or(c.size))
                .collect();
            let gaps = al.gap * kids.len().saturating_sub(1) as f64;
            let row = matches!(al.direction, LayoutDirection::Horizontal);
            let along: f64 = kids.iter().map(|s| if row { s.width } else { s.height }).sum::<f64>() + gaps;
            let across = kids.iter().map(|s| if row { s.height } else { s.width }).fold(0.0, f64::max);
            let (pad_w, pad_h) = (al.padding_left + al.padding_right, al.padding_top + al.padding_bottom);
            let mut size = node.size;
            let (main, cross) = if row { (along + pad_w, across + pad_h) } else { (along + pad_h, across + pad_w) };
            if al.hug_main {
                if row { size.width = main } else { size.height = main }
            }
            if al.hug_cross {
                if row { size.height = cross } else { size.width = cross }
            }
            sizes.insert(id, size);
            if size != node.size {
                ops.push(Op::Set { id, prop: Prop::Size(size) });
            }
        }
        ops
    }
}
