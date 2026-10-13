//! Parser service — chart text parse request/response.

use facet::Facet;

/// Parse request.
#[derive(Clone, Facet)]
pub struct ParseRequest {
    pub text: String,
}

/// Parse response.
#[derive(Clone, Facet)]
pub struct ParseResponse {
    pub chart: crate::Chart,
}

#[architect::rpc]
pub trait Parsers {
    /// Parse chart text.
    fn parse_chart(&self, request: ParseRequest) -> Result<ParseResponse, crate::ParseError>;
}
