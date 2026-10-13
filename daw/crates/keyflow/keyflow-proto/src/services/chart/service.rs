//! Charts trait.
//!
//! Sync trait decorated with `#[architect::rpc]` — the macro derives
//! the async vox client + `serve` function. Backends impl
//! `Charts` directly; remote callers reach it via
//! [`ChartsClient`].

use crate::ParseError;

#[architect::rpc]
pub trait Charts {
    /// Get chart by ID.
    fn get_chart(&self, chart_id: String) -> Option<crate::Chart>;

    /// Parse chart from keyflow syntax.
    fn parse(&self, text: String) -> Result<crate::Chart, ParseError>;
}
