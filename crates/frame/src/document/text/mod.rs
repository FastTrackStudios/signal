//! frame markup — the JSX-like text form of a [`Scene`].
//!
//! The binary Loro bundle is the canonical file; this is the form people
//! diff and models read and write. One element per node, nested in child
//! order, attributes for properties:
//!
//! ```text
//! <Document id="…">
//!   <Page name="Surface" id="…">
//!     <Frame name="Panel" id="…" w=320 h=200 fill="#1e1e1e" radius=8 layout="row" gap=8 padding=12>
//!       <Ellipse name="Knob" id="…" w=64 h=64 fill="#ff5a36" />
//!       <Text name="Label" id="…" w=64 h=16 size=12 fill="#ffffff">Gain</Text>
//!     </Frame>
//!   </Page>
//! </Document>
//! ```
//!
//! The law: for every scene `s`, `parse(&print(&s), s.assets.clone()) == Ok(s)`.
//! The printer omits properties equal to [`Node::new`]'s defaults and uses a
//! compact attribute form wherever that form is lossless, falling back to the
//! property's serde JSON in braces (`fill={[…]}`) otherwise.
//!
//! `REFERENCE.md` next to this module is the authoring reference fed to models.
//!
//! [`Node::new`]: crate::document::proto::Node::new

mod error;
mod parse;
mod print;
mod schema;
mod values;

pub use error::ParseError;
pub use parse::{parse, parse_fragment};
pub use print::{print, print_subtree};

#[doc(no_inline)]
pub use crate::document::proto::Scene;
