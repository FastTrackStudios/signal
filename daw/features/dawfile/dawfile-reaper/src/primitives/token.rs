//! Token parsing for RPP format
//!
//! This module implements token parsing that closely matches WDL's LineParser approach:
//! - Space/tab delimited tokens
//! - Three quote types: ", ', `
//! - Comment support (# and ;)
//! - Number parsing with hex support
//! - MIDI events with E/e prefix

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_while, take_while1},
    character::complete::{one_of, space0, space1},
    combinator::{map, opt, recognize},
    sequence::{delimited, preceded},
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Types of quotes used in RPP format (matching WDL)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuoteType {
    Double,   // "text"
    Single,   // 'text'
    Backtick, // `text`
}

impl fmt::Display for QuoteType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuoteType::Double => write!(f, "\""),
            QuoteType::Single => write!(f, "'"),
            QuoteType::Backtick => write!(f, "`"),
        }
    }
}

/// A parsed token from RPP format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Token {
    String(String, QuoteType),
    Integer(i64),
    Float(f64),
    HexInteger(u64),
    MidiEvent { time: i64, bytes: [u8; 3] },
    Identifier(String),
}

impl Token {
    /// Get the string value if this is a string token
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Token::String(s, _) => Some(s),
            Token::Identifier(s) => Some(s),
            _ => None,
        }
    }

    /// Get the numeric value if this is a number token
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Token::Integer(i) => Some(*i as f64),
            Token::Float(f) => Some(*f),
            Token::HexInteger(h) => Some(*h as f64),
            _ => None,
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::String(s, quote_type) => write!(f, "{}{}{}", quote_type, s, quote_type),
            Token::Integer(i) => write!(f, "{}", i),
            Token::Float(fl) => write!(f, "{}", fl),
            Token::HexInteger(h) => write!(f, "0x{:X}", h),
            Token::MidiEvent { time, bytes } => {
                write!(
                    f,
                    "E {} {:02X} {:02X} {:02X}",
                    time, bytes[0], bytes[1], bytes[2]
                )
            }
            Token::Identifier(id) => write!(f, "{}", id),
        }
    }
}

/// Parse a quoted string with any of the three quote types (matching WDL)
fn quoted_string(input: &str) -> IResult<&str, (String, QuoteType)> {
    alt((
        // Double quotes: "text" (including empty strings)
        map(
            delimited(tag("\""), take_while(|c| c != '"'), tag("\"")),
            |s: &str| (s.to_string(), QuoteType::Double),
        ),
        // Single quotes: 'text' (including empty strings)
        map(
            delimited(tag("'"), take_while(|c| c != '\''), tag("'")),
            |s: &str| (s.to_string(), QuoteType::Single),
        ),
        // Backticks: `text` (including empty strings)
        map(
            delimited(tag("`"), take_while(|c| c != '`'), tag("`")),
            |s: &str| (s.to_string(), QuoteType::Backtick),
        ),
    ))
    .parse(input)
}

/// Parse a hex byte (two hex digits)
fn hex_byte(input: &str) -> IResult<&str, u8> {
    map(take_while1(|c: char| c.is_ascii_hexdigit()), |s: &str| {
        u8::from_str_radix(s, 16).unwrap_or(0)
    })
    .parse(input)
}

/// Parse a MIDI event: E/e + time + 3 space-separated hex bytes
/// This matches the rppxml reference implementation
fn midi_event(input: &str) -> IResult<&str, Token> {
    // Look for E/e followed by space, then time and 3 space-separated hex bytes
    let (input, _event_type) = one_of("Ee")(input)?;
    let (input, _) = space1(input)?;
    let (input, time) = nom::character::complete::i64(input)?;
    let (input, _) = space1(input)?;
    let (input, byte1) = hex_byte(input)?;
    let (input, _) = space1(input)?;
    let (input, byte2) = hex_byte(input)?;
    let (input, _) = space1(input)?;
    let (input, byte3) = hex_byte(input)?;

    Ok((
        input,
        Token::MidiEvent {
            time,
            bytes: [byte1, byte2, byte3],
        },
    ))
}

/// Parse a hex integer with 0x prefix (matching WDL's gettoken_int)
fn hex_integer(input: &str) -> IResult<&str, Token> {
    map(
        preceded(tag("0x"), take_while1(|c: char| c.is_ascii_hexdigit())),
        |s: &str| Token::HexInteger(u64::from_str_radix(s, 16).unwrap_or(0)),
    )
    .parse(input)
}

/// Parse a number (integer or float) - this handles the ambiguity between integers and floats
fn number(input: &str) -> IResult<&str, Token> {
    // First, recognize a complete number (including decimal point)
    let (remaining, num_str) = recognize((
        opt(nom::character::complete::char('-')),
        nom::character::complete::digit1,
        opt(nom::sequence::preceded(
            nom::character::complete::one_of(".,"),
            nom::character::complete::digit1,
        )),
        opt(nom::sequence::preceded(
            nom::character::complete::one_of("eE"),
            (
                opt(nom::character::complete::one_of("+-")),
                nom::character::complete::digit1,
            ),
        )),
    ))
    .parse(input)?;

    // Check if this is actually a standalone number (followed by whitespace or end)
    if !remaining.is_empty() && !remaining.chars().next().unwrap().is_whitespace() {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    }

    // Convert commas to dots like WDL does
    let normalized = num_str.replace(',', ".");

    // Try to parse as integer first, then as float
    if let Ok(int_val) = normalized.parse::<i64>() {
        // Check if it's actually a float (has decimal point or scientific notation)
        if num_str.contains('.')
            || num_str.contains(',')
            || num_str.contains('e')
            || num_str.contains('E')
        {
            Ok((remaining, Token::Float(int_val as f64)))
        } else {
            Ok((remaining, Token::Integer(int_val)))
        }
    } else {
        // Must be a float
        let float_val = normalized.parse::<f64>().unwrap_or(0.0);
        Ok((remaining, Token::Float(float_val)))
    }
}

/// Parse an identifier (unquoted word, matching WDL)
/// This should not match single characters that could be MIDI event markers
fn identifier(input: &str) -> IResult<&str, Token> {
    // First check if this could be a MIDI event (E/e followed by space)
    if input.len() >= 2
        && (input.starts_with('E') || input.starts_with('e'))
        && (input.chars().nth(1) == Some(' ') || input.chars().nth(1) == Some('\t'))
    {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    }

    map(
        take_while1(|c: char| {
            !c.is_whitespace()
                && c != '>'
                && c != '<'
                && c != '"'
                && c != '\''
                && c != '`'
                && c != '#'
                && c != ';'
        }),
        |s: &str| Token::Identifier(s.to_string()),
    )
    .parse(input)
}

/// Parse a plugin state token: a vendor id with an angle-bracketed hex blob
/// glued straight onto it, as in `1919247985<5653547265657172656571…>`.
///
/// `<` normally opens a block and `>` closes one, so without this the token
/// splits mid-line and the rest of a `<VST …` header is swallowed into a
/// phantom nested block. REAPER's own `LineParser` splits only on whitespace
/// and quotes, so an angle bracket inside a word is just part of the word.
///
/// The bracket group must be glued to a non-empty, quote-free, single-line
/// prefix, which is what keeps a genuine block opener (`<TRACK`, where the
/// prefix is empty) from matching here.
fn state_token(input: &str) -> IResult<&str, Token> {
    fn reject(input: &str) -> nom::Err<nom::error::Error<&str>> {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Tag))
    }

    let line = input.split('\n').next().unwrap_or(input);
    // `open > 0` keeps a real block opener (`<TRACK`, empty prefix) out.
    let open = line
        .find('<')
        .filter(|open| *open > 0)
        .ok_or(reject(input))?;
    let (prefix, bracketed) = line.split_at(open);
    if prefix.contains(['"', '\'', '`', '#', ';', '>']) || prefix.contains(char::is_whitespace) {
        return Err(reject(input));
    }
    let close = bracketed.find('>').ok_or(reject(input))?;
    let hex = bracketed.get(1..close).ok_or(reject(input))?;
    if hex.contains(char::is_whitespace) {
        return Err(reject(input));
    }
    let end = open + close + 1;
    let (token, rest) = input.split_at(end);
    Ok((rest, Token::Identifier(token.to_string())))
}

/// Parse a single token from input (matching WDL's token parsing order)
pub fn parse_token(input: &str) -> IResult<&str, Token> {
    alt((
        // MIDI events must come first to avoid conflicts
        midi_event,
        // Quoted strings
        map(quoted_string, |(s, qt)| Token::String(s, qt)),
        // `vendorid<hex>` before the number parsers, which would otherwise
        // stop at the '<' and leave the bracket group to open a phantom block
        state_token,
        // Numbers - try hex first, then general number parser
        hex_integer,
        number,
        // Identifiers last
        identifier,
    ))
    .parse(input)
}

/// Parse a line of space-separated tokens (matching WDL's LineParser)
pub fn parse_token_line(input: &str) -> IResult<&str, Vec<Token>> {
    // Skip leading whitespace
    let (input, _) = space0(input)?;

    // Parse tokens separated by whitespace
    let (input, tokens) = nom::multi::separated_list1(space1, parse_token).parse(input)?;

    // Skip trailing whitespace
    let (input, _) = space0(input)?;

    Ok((input, tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quoted_strings() {
        assert_eq!(
            parse_token("\"hello world\""),
            Ok((
                "",
                Token::String("hello world".to_string(), QuoteType::Double)
            ))
        );

        assert_eq!(
            parse_token("'single quotes'"),
            Ok((
                "",
                Token::String("single quotes".to_string(), QuoteType::Single)
            ))
        );

        assert_eq!(
            parse_token("`backticks`"),
            Ok((
                "",
                Token::String("backticks".to_string(), QuoteType::Backtick)
            ))
        );
    }

    #[test]
    fn test_numbers() {
        assert_eq!(parse_token("42"), Ok(("", Token::Integer(42))));
        assert_eq!(parse_token("-17"), Ok(("", Token::Integer(-17))));
        assert_eq!(parse_token("1.5"), Ok(("", Token::Float(1.5))));
        assert_eq!(parse_token("1,5"), Ok(("", Token::Float(1.5)))); // WDL converts commas to dots
        assert_eq!(parse_token("0xFF"), Ok(("", Token::HexInteger(255))));
    }

    #[test]
    fn test_midi_events() {
        assert_eq!(
            parse_token("E 480 90 3f 60"),
            Ok((
                "",
                Token::MidiEvent {
                    time: 480,
                    bytes: [0x90, 0x3f, 0x60]
                }
            ))
        );

        assert_eq!(
            parse_token("e 120 80 3f 00"),
            Ok((
                "",
                Token::MidiEvent {
                    time: 120,
                    bytes: [0x80, 0x3f, 0x00]
                }
            ))
        );
    }

    #[test]
    fn test_hex_byte() {
        assert_eq!(hex_byte("90"), Ok(("", 0x90)));
        assert_eq!(hex_byte("3f"), Ok(("", 0x3f)));
        assert_eq!(hex_byte("60"), Ok(("", 0x60)));
    }

    #[test]
    fn test_identifiers() {
        assert_eq!(
            parse_token("TRACK"),
            Ok(("", Token::Identifier("TRACK".to_string())))
        );

        assert_eq!(
            parse_token("VOL"),
            Ok(("", Token::Identifier("VOL".to_string())))
        );
    }

    #[test]
    fn test_token_line() {
        let result = parse_token_line("NAME \"Track 1\" VOL 1.0 0.0");
        assert!(result.is_ok());

        let (remaining, tokens) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(tokens.len(), 5);

        assert_eq!(tokens[0], Token::Identifier("NAME".to_string()));
        assert_eq!(
            tokens[1],
            Token::String("Track 1".to_string(), QuoteType::Double)
        );
        assert_eq!(tokens[2], Token::Identifier("VOL".to_string()));
        assert_eq!(tokens[3], Token::Float(1.0));
        assert_eq!(tokens[4], Token::Float(0.0));
    }

    #[test]
    fn test_whitespace_handling() {
        let result = parse_token_line("  NAME  \"Track 1\"  ");
        assert!(result.is_ok());

        let (remaining, tokens) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], Token::Identifier("NAME".to_string()));
        assert_eq!(
            tokens[1],
            Token::String("Track 1".to_string(), QuoteType::Double)
        );
    }
}
