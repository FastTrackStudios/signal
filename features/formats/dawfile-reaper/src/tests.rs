//! Integration tests that match the Python rppxml test suite
//!
//! These tests ensure our Rust parser behaves the same way as the reference
//! rppxml Python implementation.

use crate::primitives::token::{QuoteType, Token};
use crate::{RppProject, RppResult, parse_rpp_file};

/// Helper function to create a test RPP string
fn create_test_rpp() -> &'static str {
    r#"<OBJECT 0.1 "str" 256
  PARAM1 "" 1 2
  PARAM2 "a" analyze
  <SUBOBJECT my/dir ""
    something
    1 2 3 0 0 0 - - -
  >
  <NOTES 0 2
  >
>"#
}

#[test]
fn test_basic_parsing() {
    let xml = create_test_rpp();
    let result: RppResult<RppProject> = parse_rpp_file(xml);

    // For now, we expect this to fail since we haven't implemented full parsing yet
    // Once we implement the full parser, we should be able to parse this successfully
    assert!(result.is_err(), "Full parsing not yet implemented");
}

#[test]
fn test_token_parsing_basic() {
    // Test basic token parsing that should work now
    use crate::primitives::token::parse_token_line;

    // Test basic parameters
    let result = parse_token_line("0.1 \"str\" 256");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0], Token::Float(0.1));
    assert_eq!(
        tokens[1],
        Token::String("str".to_string(), QuoteType::Double)
    );
    assert_eq!(tokens[2], Token::Integer(256));
}

#[test]
fn test_token_parsing_with_empty_string() {
    use crate::primitives::token::{parse_token, parse_token_line};

    // Debug: Test individual token parsing
    let result1 = parse_token("PARAM1");
    assert!(result1.is_ok());
    let (remaining1, token1) = result1.unwrap();
    assert_eq!(remaining1, "");
    assert_eq!(token1, Token::Identifier("PARAM1".to_string()));

    // Test empty string token
    let result2 = parse_token("\"\"");
    assert!(result2.is_ok());
    let (remaining2, token2) = result2.unwrap();
    assert_eq!(remaining2, "");
    assert_eq!(token2, Token::String("".to_string(), QuoteType::Double));

    // Test the full line
    let result = parse_token_line("PARAM1 \"\" 1 2");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    // The parser should consume all input
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[0], Token::Identifier("PARAM1".to_string()));
    assert_eq!(tokens[1], Token::String("".to_string(), QuoteType::Double));
    assert_eq!(tokens[2], Token::Integer(1));
    assert_eq!(tokens[3], Token::Integer(2));
}

#[test]
fn test_token_parsing_with_identifiers() {
    use crate::primitives::token::parse_token_line;

    // Test identifier parameters
    let result = parse_token_line("PARAM2 \"a\" analyze");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0], Token::Identifier("PARAM2".to_string()));
    assert_eq!(tokens[1], Token::String("a".to_string(), QuoteType::Double));
    assert_eq!(tokens[2], Token::Identifier("analyze".to_string()));
}

#[test]
fn test_token_parsing_with_dashes() {
    use crate::primitives::token::parse_token_line;

    // Test parameters with dashes (common in RPP files)
    let result = parse_token_line("1 2 3 0 0 0 - - -");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 9);
    assert_eq!(tokens[0], Token::Integer(1));
    assert_eq!(tokens[1], Token::Integer(2));
    assert_eq!(tokens[2], Token::Integer(3));
    assert_eq!(tokens[3], Token::Integer(0));
    assert_eq!(tokens[4], Token::Integer(0));
    assert_eq!(tokens[5], Token::Integer(0));
    assert_eq!(tokens[6], Token::Identifier("-".to_string()));
    assert_eq!(tokens[7], Token::Identifier("-".to_string()));
    assert_eq!(tokens[8], Token::Identifier("-".to_string()));
}

#[test]
fn test_special_characters() {
    use crate::primitives::token::parse_token_line;

    // Test handling of special characters and whitespace (matching Python test)
    // Note: Our parser handles one line at a time, so we test without the newline
    let result = parse_token_line("0\t1        \"a b c\"");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0], Token::Integer(0));
    assert_eq!(tokens[1], Token::Integer(1));
    assert_eq!(
        tokens[2],
        Token::String("a b c".to_string(), QuoteType::Double)
    );

    // Test the second line separately
    let result2 = parse_token_line("2");
    assert!(result2.is_ok());
    let (remaining2, tokens2) = result2.unwrap();
    assert_eq!(remaining2, "");
    assert_eq!(tokens2.len(), 1);
    assert_eq!(tokens2[0], Token::Integer(2));
}

#[test]
fn test_midi_events_in_context() {
    use crate::primitives::token::parse_token_line;

    // Test MIDI events in a realistic context
    let result = parse_token_line("E 480 90 3f 60");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 1);
    assert_eq!(
        tokens[0],
        Token::MidiEvent {
            time: 480,
            bytes: [0x90, 0x3f, 0x60]
        }
    );
}

#[test]
fn test_hex_numbers() {
    use crate::primitives::token::parse_token_line;

    // Test hex numbers (common in RPP files)
    let result = parse_token_line("0xFF 0x00 0x3F");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0], Token::HexInteger(255));
    assert_eq!(tokens[1], Token::HexInteger(0));
    assert_eq!(tokens[2], Token::HexInteger(63));
}

#[test]
fn test_mixed_quotes() {
    use crate::primitives::token::parse_token_line;

    // Test different quote types
    let result = parse_token_line("\"double\" 'single' `backtick`");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 3);
    assert_eq!(
        tokens[0],
        Token::String("double".to_string(), QuoteType::Double)
    );
    assert_eq!(
        tokens[1],
        Token::String("single".to_string(), QuoteType::Single)
    );
    assert_eq!(
        tokens[2],
        Token::String("backtick".to_string(), QuoteType::Backtick)
    );
}

#[test]
fn test_floats_with_commas() {
    use crate::primitives::token::parse_token_line;

    // Test float parsing with commas (WDL converts commas to dots)
    let result = parse_token_line("3,14 2,5 1,0");
    assert!(result.is_ok());
    let (remaining, tokens) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0], Token::Float(3.14));
    assert_eq!(tokens[1], Token::Float(2.5));
    assert_eq!(tokens[2], Token::Float(1.0));
}

#[test]
fn test_display_implementations() {
    use crate::primitives::block::{BlockType, RppBlockContent};
    use crate::primitives::token::{QuoteType, Token};

    // Test Token Display
    let string_token = Token::String("Hello World".to_string(), QuoteType::Double);
    assert_eq!(string_token.to_string(), "\"Hello World\"");

    let midi_token = Token::MidiEvent {
        time: 480,
        bytes: [0x90, 0x3f, 0x60],
    };
    assert_eq!(midi_token.to_string(), "E 480 90 3F 60");

    let hex_token = Token::HexInteger(255);
    assert_eq!(hex_token.to_string(), "0xFF");

    // Test BlockType Display
    assert_eq!(BlockType::Track.to_string(), "TRACK");
    assert_eq!(BlockType::Other("CUSTOM".to_string()).to_string(), "CUSTOM");

    // Test RppBlockContent Display
    let content = RppBlockContent::Content(vec![
        Token::Identifier("NAME".to_string()),
        Token::String("Test Track".to_string(), QuoteType::Double),
    ]);
    assert_eq!(content.to_string(), "NAME \"Test Track\"");

    println!("✅ All Display implementations working correctly!");
}
