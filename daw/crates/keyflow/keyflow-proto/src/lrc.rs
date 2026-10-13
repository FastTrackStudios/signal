//! Reading `.lrc` — the other direction from `keyflow_sync::karaoke::lyrics_lrc`.
//!
//! An LRC is a cheap, ubiquitous head start. Community databases
//! (LRCLIB and the aggregators built on it) carry line-level timings for
//! most of the popular catalogue, which gives two things at once: the
//! lyric *text*, and an anchor every few seconds saying roughly where
//! each line lands.
//!
//! Both matter for alignment. Forced alignment against known text is a
//! far easier problem than transcription, and line anchors bound the
//! search so a repeated chorus cannot slide onto the wrong repeat —
//! which is exactly where an unanchored aligner fails on pop music.
//!
//! Two dialects, and this reads both:
//!
//! ```text
//! [01:05.50]Bye bye bye              plain — one time per line
//! [01:05.50]<01:05.50>Bye <01:05.90>bye    enhanced — a time per word
//! ```
//!
//! Enhanced LRC is rarer but is the same shape as the output of
//! alignment, so a file that has it needs no aligner at all. What
//! neither dialect carries is syllables or melody; those are filled in
//! afterwards, and this is the scaffold they hang on.

/// One word within a line, when the file carries word times.
#[derive(Debug, Clone, PartialEq)]
pub struct LrcWord {
    pub start: f32,
    pub text: String,
}

/// One timed line.
#[derive(Debug, Clone, PartialEq)]
pub struct LrcLine {
    /// Seconds from the start of the recording.
    pub start: f32,
    pub text: String,
    /// Per-word times, empty for a plain LRC.
    pub words: Vec<LrcWord>,
}

/// A parsed LRC: its header tags and its lines.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lrc {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// The file's own claimed offset, in seconds, already applied to
    /// `lines`. Kept for reference rather than re-applied.
    pub offset: f32,
    pub lines: Vec<LrcLine>,
}

/// Parses `[mm:ss.cc]` / `[mm:ss:cc]` / `<mm:ss.cc>` at the head of
/// `text`, returning the seconds and what follows.
///
/// Colons and dots both appear as the fraction separator in the wild,
/// and a two-digit fraction means hundredths while three means
/// milliseconds — a file using milliseconds read as hundredths is out
/// by a factor of ten, which puts every line in the wrong place rather
/// than slightly off.
fn take_stamp(text: &str, open: char, close: char) -> Option<(f32, &str)> {
    let rest = text.strip_prefix(open)?;
    let (body, rest) = rest.split_once(close)?;
    let (minutes, tail) = body.split_once(':')?;
    let minutes: f32 = minutes.trim().parse().ok()?;

    let (seconds, fraction) = match tail.split_once(['.', ':']) {
        Some((s, f)) => (s, f),
        None => (tail, ""),
    };
    let seconds: f32 = seconds.trim().parse().ok()?;
    let fraction: f32 = if fraction.is_empty() {
        0.0
    } else {
        let digits = fraction.trim();
        let value: f32 = digits.parse().ok()?;
        value / 10f32.powi(digits.len() as i32)
    };
    Some((minutes * 60.0 + seconds + fraction, rest))
}

/// A `[tag:value]` header, if this is one rather than a timestamp.
fn take_tag(line: &str) -> Option<(&str, &str)> {
    let body = line.strip_prefix('[')?.strip_suffix(']')?;
    let (key, value) = body.split_once(':')?;
    // A timestamp's "key" is all digits; a tag's is a word.
    (!key.chars().all(|c| c.is_ascii_digit())).then_some((key.trim(), value.trim()))
}

/// Parses LRC text.
///
/// Unparseable lines are skipped rather than failing the file: these
/// come from community databases and one malformed line should not cost
/// the other sixty.
pub fn parse(text: &str) -> Lrc {
    let mut lrc = Lrc::default();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = take_tag(line) {
            match key.to_ascii_lowercase().as_str() {
                "ti" => lrc.title = Some(value.to_string()),
                "ar" => lrc.artist = Some(value.to_string()),
                "al" => lrc.album = Some(value.to_string()),
                "offset" => lrc.offset = value.parse::<f32>().unwrap_or(0.0) / 1000.0,
                _ => {}
            }
            continue;
        }

        // A line may carry several timestamps — that is how LRC says
        // "this line repeats at these times" for a chorus, and it must
        // become several lines rather than one.
        let mut stamps = Vec::new();
        let mut rest = line;
        while let Some((seconds, tail)) = take_stamp(rest, '[', ']') {
            stamps.push(seconds);
            rest = tail;
        }
        if stamps.is_empty() {
            continue;
        }

        let words = parse_words(rest);
        let text = if words.is_empty() {
            rest.trim().to_string()
        } else {
            words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        if text.is_empty() && words.is_empty() {
            // An empty timed line is a real thing in LRC — it marks the
            // end of the previous line, which is worth keeping because
            // it bounds the last word.
        }
        for start in stamps {
            lrc.lines.push(LrcLine {
                start,
                text: text.clone(),
                words: words.clone(),
            });
        }
    }

    lrc.lines.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    lrc
}

/// Enhanced-LRC word times, if the line has any.
fn parse_words(body: &str) -> Vec<LrcWord> {
    let mut words = Vec::new();
    let mut rest = body;
    // Anything before the first `<` belongs to no word time.
    while let Some(open) = rest.find('<') {
        let after = &rest[open..];
        let Some((start, tail)) = take_stamp(after, '<', '>') else {
            break;
        };
        let end = tail.find('<').unwrap_or(tail.len());
        let text = tail[..end].trim();
        if !text.is_empty() {
            words.push(LrcWord {
                start,
                text: text.to_string(),
            });
        }
        rest = &tail[end..];
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_plain_lrc() {
        let lrc = parse("[ti:Bye Bye Bye]\n[ar:NSYNC]\n[00:08.18]Line one\n[01:05.50]Line two\n");
        assert_eq!(lrc.title.as_deref(), Some("Bye Bye Bye"));
        assert_eq!(lrc.artist.as_deref(), Some("NSYNC"));
        assert_eq!(lrc.lines.len(), 2);
        assert!((lrc.lines[0].start - 8.18).abs() < 1e-4);
        assert!((lrc.lines[1].start - 65.5).abs() < 1e-4);
        assert_eq!(lrc.lines[1].text, "Line two");
        assert!(lrc.lines[1].words.is_empty());
    }

    /// Enhanced LRC carries a time per word, which is the same shape
    /// alignment produces — a file with it needs no aligner.
    #[test]
    fn reads_word_times_from_an_enhanced_lrc() {
        let lrc = parse("[01:05.50]<01:05.50>Bye <01:05.90>bye <01:06.30>bye\n");
        let line = &lrc.lines[0];
        assert_eq!(line.words.len(), 3);
        assert_eq!(line.text, "Bye bye bye");
        assert!((line.words[1].start - 65.9).abs() < 1e-4);
    }

    /// A repeated chorus is written as one line with several stamps.
    /// Read as one line it would show once and never come back.
    #[test]
    fn one_line_with_several_stamps_becomes_several_lines() {
        let lrc = parse("[00:30.00][01:30.00][02:30.00]Bye bye bye\n");
        assert_eq!(lrc.lines.len(), 3);
        assert_eq!(lrc.lines[2].start, 150.0);
        assert!(lrc.lines.iter().all(|l| l.text == "Bye bye bye"));
    }

    /// Both separators appear in the wild, and a three-digit fraction is
    /// milliseconds rather than hundredths — read wrongly it is out by a
    /// factor of ten, which misplaces every line rather than nudging it.
    #[test]
    fn accepts_both_fraction_separators_and_widths() {
        for (text, want) in [
            ("[00:10.50]x", 10.5),
            ("[00:10:50]x", 10.5),
            ("[00:10.500]x", 10.5),
            ("[00:10]x", 10.0),
        ] {
            let lrc = parse(text);
            assert!((lrc.lines[0].start - want).abs() < 1e-4, "{text}");
        }
    }

    /// These files come from community databases; one bad line should
    /// not cost the other sixty.
    #[test]
    fn skips_junk_without_losing_the_file() {
        let lrc = parse("[00:01.00]good\nnot a line at all\n[garbage]\n[00:02.00]also good\n");
        assert_eq!(lrc.lines.len(), 2);
    }

    /// An empty timed line is not junk — it is how LRC says "nothing is
    /// being sung here", which for a lyric screen means clear it rather
    /// than leave the last line hanging through an instrumental.
    #[test]
    fn keeps_empty_timed_lines() {
        let lrc = parse("[00:13.99] \n[00:17.67] Bye, bye\n[00:20.07]\n");
        assert_eq!(lrc.lines.len(), 3);
        assert!(lrc.lines[0].text.is_empty());
        assert_eq!(lrc.lines[1].text, "Bye, bye");
        assert!(lrc.lines[2].text.is_empty());
    }
}
