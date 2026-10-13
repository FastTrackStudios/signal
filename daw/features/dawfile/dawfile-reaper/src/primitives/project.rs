//! Project-level parsing for RPP files
//!
//! Handles the top-level REAPER_PROJECT structure and coordinates
//! parsing of the entire RPP file.

use nom::{IResult, Parser, bytes::complete::tag, character::complete::space0, combinator::map};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::block::{BlockType, RppBlock, RppBlockContent};
use super::token::parse_token_line;

/// A complete RPP project
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RppProject {
    pub version: f64,
    pub version_string: String,
    pub timestamp: i64,
    pub blocks: Vec<RppBlock>,
}

impl fmt::Display for RppProject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "RPP Project v{} ({})", self.version, self.version_string)?;
        writeln!(f, "Timestamp: {}", self.timestamp)?;
        writeln!(f, "Blocks: {}", self.blocks.len())?;

        for (i, block) in self.blocks.iter().enumerate() {
            writeln!(f, "Block {}: {}", i + 1, block)?;
        }

        Ok(())
    }
}

/// Parse the REAPER_PROJECT header
pub fn parse_project_header(input: &str) -> IResult<&str, (f64, String, i64)> {
    map(
        (tag("<REAPER_PROJECT"), space0, parse_token_line),
        |(_, _, tokens)| {
            let version = tokens.first().and_then(|t| t.as_number()).unwrap_or(0.1);

            let version_string = tokens
                .get(1)
                .and_then(|t| t.as_string())
                .unwrap_or("")
                .to_string();

            let timestamp = tokens.get(2).and_then(|t| t.as_number()).unwrap_or(0.0) as i64;

            (version, version_string, timestamp)
        },
    )
    .parse(input)
}

/// Parse project content (properties and nested blocks)
fn parse_project_content(input: &str) -> IResult<&str, Vec<RppBlock>> {
    let mut blocks = Vec::new();
    let mut remaining_input = input;

    // Create a project block to hold all the project properties
    let mut project_block = RppBlock {
        block_type: BlockType::Project,
        name: "REAPER_PROJECT".to_string(),
        params: vec![],
        children: vec![],
    };

    loop {
        // Skip leading whitespace/newlines
        let (input_after_ws, _) = nom::character::complete::multispace0(remaining_input)?;

        // Check if this is the final closing '>'
        if let Ok((input_after_end, _)) =
            nom::bytes::complete::tag::<&str, &str, nom::error::Error<&str>>(">")(input_after_ws)
        {
            remaining_input = input_after_end;
            break;
        }

        // Try to parse as a content line (project property)
        if let Ok((input_after_content, content_tokens)) = parse_token_line(input_after_ws) {
            project_block
                .children
                .push(RppBlockContent::Content(content_tokens));

            // Skip the newline after the content line
            let (input_after_newline, _) =
                nom::character::complete::multispace0(input_after_content)?;
            remaining_input = input_after_newline;
        } else {
            // Try to parse as a nested block
            if let Ok((input_after_block, block)) = super::block::parse_block(input_after_ws) {
                blocks.push(block);
                remaining_input = input_after_block;
            } else {
                // If we can't parse as either content or block, break
                break;
            }
        }
    }

    // Add the project block with all its properties
    if !project_block.children.is_empty() {
        blocks.insert(0, project_block);
    }

    Ok((remaining_input, blocks))
}

/// Parse a complete RPP project
pub fn parse_rpp(input: &str) -> IResult<&str, RppProject> {
    let (input, (version, version_string, timestamp)) = parse_project_header(input)?;

    // Parse project content (properties and nested blocks)
    let (input, blocks) = parse_project_content(input)?;

    Ok((
        input,
        RppProject {
            version,
            version_string,
            timestamp,
            blocks,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::super::block::BlockType;
    use super::*;

    #[test]
    fn test_parse_project_header() {
        let input = r#"<REAPER_PROJECT 0.1 "6.75/linux-x86_64" 1681651369"#;
        let result = parse_project_header(input);
        assert!(result.is_ok());

        let (remaining, (version, version_string, timestamp)) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(version, 0.1);
        assert_eq!(version_string, "6.75/linux-x86_64");
        assert_eq!(timestamp, 1681651369);
    }

    #[test]
    fn test_parse_rpp() {
        let input = r#"<REAPER_PROJECT 0.1 "6.75/linux-x86_64" 1681651369
  <TRACK
    NAME "Track 1"
    VOL 1.0 0.0
  >
>"#;

        let result = parse_rpp(input);
        assert!(result.is_ok());

        let (remaining, project) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(project.version, 0.1);
        assert_eq!(project.version_string, "6.75/linux-x86_64");
        assert_eq!(project.timestamp, 1681651369);
        assert_eq!(project.blocks.len(), 1);

        let block = &project.blocks[0];
        assert_eq!(block.name, "TRACK");
        assert_eq!(block.block_type, BlockType::Track);
    }
}
