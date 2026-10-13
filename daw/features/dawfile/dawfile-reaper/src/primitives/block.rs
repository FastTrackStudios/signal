//! Block parsing for RPP format
//!
//! Handles the block structure of RPP files:
//! - Block starts: <TRACK NAME "Track 1" VOL 1.0>
//! - Block ends: >
//! - Content lines: NAME "Track 1" or VOL 1.0 0.0

use nom::{
    IResult, Parser,
    bytes::complete::tag,
    character::complete::{multispace0, space0},
    combinator::map,
};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::token::{Token, parse_token_line};

/// Types of RPP blocks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockType {
    Project,
    Track,
    Item,
    Envelope,
    FxChain,
    Source,
    Take,
    TempoEnvEx,
    Other(String),
}

impl BlockType {
    /// Create a BlockType from a string
    pub fn parse(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "REAPER_PROJECT" => BlockType::Project,
            "TRACK" => BlockType::Track,
            "ITEM" => BlockType::Item,
            "VOLENV" | "VOLENV2" | "VOLENV3" | "PANENV" | "PANENV2" | "MUTEENV" | "MUTEENV2"
            | "WIDTHENV" | "WIDTHENV2" | "AUXVOLENV" | "AUXPANENV" | "AUXMUTEENV" | "PARMENV" => {
                BlockType::Envelope
            }
            "FXCHAIN" => BlockType::FxChain,
            "SOURCE" => BlockType::Source,
            "TAKE" => BlockType::Take,
            "TEMPOENVEX" => BlockType::TempoEnvEx,
            _ => BlockType::Other(s.to_string()),
        }
    }
}

impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockType::Project => write!(f, "REAPER_PROJECT"),
            BlockType::Track => write!(f, "TRACK"),
            BlockType::Item => write!(f, "ITEM"),
            BlockType::Envelope => write!(f, "ENVELOPE"),
            BlockType::FxChain => write!(f, "FXCHAIN"),
            BlockType::Source => write!(f, "SOURCE"),
            BlockType::Take => write!(f, "TAKE"),
            BlockType::TempoEnvEx => write!(f, "TEMPOENVEX"),
            BlockType::Other(s) => write!(f, "{}", s),
        }
    }
}

/// A parsed RPP block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RppBlock {
    pub block_type: BlockType,
    pub name: String,
    pub params: Vec<Token>,
    pub children: Vec<RppBlockContent>,
}

/// Content within a block (either nested blocks or content lines)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RppBlockContent {
    Block(RppBlock),
    Content(Vec<Token>),
    /// Opaque raw line for fast parser paths where eager tokenization is skipped.
    /// Typed decoders can tokenize on demand when needed.
    RawLine(Box<str>),
}

impl fmt::Display for RppBlockContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RppBlockContent::Block(block) => write!(f, "{}", block),
            RppBlockContent::Content(tokens) => {
                let mut first = true;
                for token in tokens {
                    if !first {
                        write!(f, " ")?;
                    }
                    first = false;
                    write!(f, "{}", token)?;
                }
                Ok(())
            }
            RppBlockContent::RawLine(line) => write!(f, "{}", line),
        }
    }
}

impl fmt::Display for RppBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format block start
        write!(f, "<{}", self.name)?;

        // Add parameters if any
        if !self.params.is_empty() {
            for param in &self.params {
                write!(f, " {}", param)?;
            }
        }
        writeln!(f)?;

        // Add children with indentation
        for child in &self.children {
            writeln!(f, "  {}", child)?;
        }

        // Close block
        write!(f, ">")
    }
}

/// Parse a block start: <TRACK
fn block_start(input: &str) -> IResult<&str, (String, Vec<Token>)> {
    map((tag("<"), space0, parse_token_line), |(_, _, tokens)| {
        if tokens.is_empty() {
            ("".to_string(), vec![])
        } else {
            let name = tokens[0].as_string().unwrap_or("").to_string();
            let params = tokens[1..].to_vec();
            (name, params)
        }
    })
    .parse(input)
}

/// Parse a block end: >
fn block_end(input: &str) -> IResult<&str, ()> {
    map(tag(">"), |_| ()).parse(input)
}

/// Parse a complete RPP block with its content
pub fn parse_block(input: &str) -> IResult<&str, RppBlock> {
    // Skip leading whitespace/newlines before the block start
    let (input, _) = multispace0(input)?;
    let (input, (name, params)) = block_start(input)?;
    let block_type = BlockType::parse(&name);

    // Skip leading whitespace/newlines
    let (input, _) = multispace0(input)?;

    // Parse content lines until we hit the block end
    let mut children = Vec::new();
    let mut remaining_input = input;

    loop {
        // Skip leading whitespace/newlines
        let (input_after_ws, _) = multispace0(remaining_input)?;

        // Check if this is a block end
        if let Ok((input_after_end, _)) = block_end(input_after_ws) {
            remaining_input = input_after_end;
            break;
        }

        // Try to parse as a content line
        if let Ok((input_after_content, content_tokens)) = parse_token_line(input_after_ws) {
            children.push(RppBlockContent::Content(content_tokens));

            // Skip the newline after the content line
            let (input_after_newline, _) = multispace0(input_after_content)?;
            remaining_input = input_after_newline;
        } else {
            // Try to parse as a nested block
            if let Ok((input_after_nested, nested_block)) = parse_block(input_after_ws) {
                children.push(RppBlockContent::Block(nested_block));
                remaining_input = input_after_nested;
            } else {
                // If we can't parse as either content or nested block, break
                break;
            }
        }
    }

    Ok((
        remaining_input,
        RppBlock {
            block_type,
            name,
            params,
            children,
        },
    ))
}

/// Parse multiple blocks from input
pub fn parse_blocks(input: &str) -> IResult<&str, Vec<RppBlock>> {
    nom::multi::many0(parse_block).parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::token::{QuoteType, Token};

    #[test]
    fn test_block_start() {
        let result = block_start("<TRACK NAME \"Track 1\" VOL 1.0>");
        assert!(result.is_ok());

        let (remaining, (name, params)) = result.unwrap();
        assert_eq!(remaining, ">"); // The '>' is not consumed by block_start
        assert_eq!(name, "TRACK");
        assert_eq!(params.len(), 4);
        assert_eq!(params[0], Token::Identifier("NAME".to_string()));
        assert_eq!(
            params[1],
            Token::String("Track 1".to_string(), QuoteType::Double)
        );
        assert_eq!(params[2], Token::Identifier("VOL".to_string()));
        assert_eq!(params[3], Token::Identifier("1.0".to_string()));
    }

    #[test]
    fn test_block_end() {
        assert_eq!(block_end(">"), Ok(("", ())));
    }

    #[test]
    fn test_parse_block() {
        let input = r#"<TRACK
NAME "Track 1"
VOL 1.0 0.0
>"#;

        let result = parse_block(input);
        assert!(result.is_ok());

        let (remaining, block) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(block.name, "TRACK");
        assert_eq!(block.block_type, BlockType::Track);
        assert_eq!(block.params.len(), 0);
        assert_eq!(block.children.len(), 2);
    }

    #[test]
    fn test_block_type_from_str() {
        assert_eq!(BlockType::parse("TRACK"), BlockType::Track);
        assert_eq!(BlockType::parse("ITEM"), BlockType::Item);
        assert_eq!(BlockType::parse("VOLENV2"), BlockType::Envelope);
        assert_eq!(BlockType::parse("FXCHAIN"), BlockType::FxChain);
        assert_eq!(
            BlockType::parse("CUSTOM"),
            BlockType::Other("CUSTOM".to_string())
        );
    }
}
