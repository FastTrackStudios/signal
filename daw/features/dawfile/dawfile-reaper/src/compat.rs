//! Lua-style compatibility facade for ReaTeam_RPP-Parser parity.
//!
//! This layer mirrors the canonical Lua function names so callers can
//! port scripts/workflows incrementally while using Rust types.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::RppResult;
use crate::rpp_tree::{
    RChunk, RNode, RNodeTree, RToken, add_rchunk, add_rnode, add_rtoken, create_rchunk,
    create_rnode_from_line, create_rnode_from_tokens, create_rpp, create_rtokens, read_rpp,
    read_rpp_chunk, read_rpp_lines, stringify_rpp_node, write_rpp,
};

/// Input for [`CreateRNode`], equivalent to Lua `CreateRNode(var)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateNodeInput {
    /// A full raw line.
    Line(String),
    /// A token sequence.
    Tokens(Vec<String>),
}

/// Public mapping table used by docs/tests to validate API presence.
pub const LUA_API_MATRIX: &[(&str, &str)] = &[
    ("ReadRPP", "ReadRPP"),
    ("ReadRPPChunk", "ReadRPPChunk"),
    ("ReadRPPChunk(lines)", "ReadRPPChunkLines"),
    ("CreateRPP", "CreateRPP"),
    ("CreateRTokens", "CreateRTokens"),
    ("CreateRChunk", "CreateRChunk"),
    ("CreateRNode", "CreateRNode"),
    ("AddRChunk", "AddRChunk"),
    ("AddRNode", "AddRNode"),
    ("AddRToken", "AddRToken"),
    ("StringifyRPPNode", "StringifyRPPNode"),
    ("WriteRPP", "WriteRPP"),
];

#[allow(non_snake_case)]
pub fn ReadRPP(path: impl AsRef<Path>) -> RppResult<RChunk> {
    read_rpp(path)
}

#[allow(non_snake_case)]
pub fn ReadRPPChunk(input: &str) -> RppResult<RChunk> {
    read_rpp_chunk(input)
}

#[allow(non_snake_case)]
pub fn ReadRPPChunkLines<T: AsRef<str>>(lines: &[T]) -> RppResult<RChunk> {
    read_rpp_lines(lines)
}

#[allow(non_snake_case)]
pub fn CreateRPP(version: Option<f64>, system: Option<&str>, time: Option<i64>) -> RChunk {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    create_rpp(
        version.unwrap_or(0.1),
        system.unwrap_or("6.21/win64"),
        time.unwrap_or(now),
    )
}

#[allow(non_snake_case)]
pub fn CreateRTokens(tokens: &[impl AsRef<str>]) -> Vec<RToken> {
    create_rtokens(tokens)
}

#[allow(non_snake_case)]
pub fn CreateRChunk(tokens: Vec<String>) -> RChunk {
    create_rchunk(tokens)
}

#[allow(non_snake_case)]
pub fn CreateRNode(input: CreateNodeInput) -> RNode {
    match input {
        CreateNodeInput::Line(line) => create_rnode_from_line(line),
        CreateNodeInput::Tokens(tokens) => create_rnode_from_tokens(tokens),
    }
}

#[allow(non_snake_case)]
pub fn AddRChunk(parent: &mut RChunk, tokens: Vec<String>) {
    add_rchunk(parent, tokens);
}

#[allow(non_snake_case)]
pub fn AddRNode(parent: &mut RChunk, tokens: Vec<String>) {
    add_rnode(parent, tokens);
}

#[allow(non_snake_case)]
pub fn AddRToken(node: &mut RNode, token: impl Into<String>) {
    add_rtoken(node, token);
}

#[allow(non_snake_case)]
pub fn StringifyRPPNode(node: &RNodeTree) -> String {
    stringify_rpp_node(node)
}

#[allow(non_snake_case)]
pub fn WriteRPP(path: impl AsRef<Path>, root: &RChunk) -> RppResult<()> {
    write_rpp(path, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_surface_create_parse_roundtrip() {
        let mut root = CreateRPP(Some(0.1), Some("7.0/x64"), Some(123));
        AddRNode(
            &mut root,
            vec!["RIPPLE".to_string(), "0".to_string(), "0".to_string()],
        );
        AddRChunk(&mut root, vec!["TRACK".to_string()]);
        assert_eq!(root.name().as_deref(), Some("REAPER_PROJECT"));
        assert_eq!(LUA_API_MATRIX.len(), 12);
    }
}
