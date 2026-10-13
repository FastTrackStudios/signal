//! Rendering utilities for highlighted spans.
//!
//! Provides output formatters for HTML, ANSI terminal colors,
//! and styled spans for UI framework integration.

use super::{HighlightKind, HighlightSpan, Theme};
use crate::highlighting::theme::Style;

/// A styled text span for UI framework integration.
///
/// Contains the text content along with styling information
/// that can be applied by any rendering system.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledSpan {
    /// The text content
    pub text: String,
    /// The highlight kind (for CSS class generation)
    pub kind: HighlightKind,
    /// The computed style from the theme
    pub style: Style,
}

impl StyledSpan {
    /// Create a new styled span.
    #[must_use]
    pub fn new(text: String, kind: HighlightKind, style: Style) -> Self {
        Self { text, kind, style }
    }
}

/// Render highlighted spans to various output formats.
pub struct Renderer;

impl Renderer {
    /// Render highlighted spans to HTML with inline styles.
    ///
    /// Produces a `<pre>` block with `<span>` elements for each
    /// highlighted region, using inline CSS styles from the theme.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let spans = Highlighter::highlight_line("Gmaj7");
    /// let html = Renderer::to_html("Gmaj7", &spans, &Theme::default_dark());
    /// ```
    #[must_use]
    pub fn to_html(source: &str, spans: &[HighlightSpan], theme: &Theme) -> String {
        let mut html = String::with_capacity(source.len() * 3);

        html.push_str(&format!(
            "<pre style=\"background:{}; color:{}; font-family:monospace; padding:1em;\">",
            theme.background.to_css(),
            theme.foreground.to_css()
        ));

        Self::render_spans_html(source, spans, theme, &mut html);

        html.push_str("</pre>");
        html
    }

    /// Render highlighted spans to HTML without wrapper.
    ///
    /// Produces only the styled `<span>` elements without the
    /// surrounding `<pre>` block, useful for embedding.
    #[must_use]
    pub fn to_html_inline(source: &str, spans: &[HighlightSpan], theme: &Theme) -> String {
        let mut html = String::with_capacity(source.len() * 2);
        Self::render_spans_html(source, spans, theme, &mut html);
        html
    }

    /// Internal helper to render spans to HTML.
    fn render_spans_html(source: &str, spans: &[HighlightSpan], theme: &Theme, html: &mut String) {
        let mut pos = 0;

        for span in spans {
            // Add unstyled text before this span
            if span.span.start > pos {
                let unstyled = &source[pos..span.span.start];
                html.push_str(&Self::escape_html(unstyled));
            }

            // Add the styled span
            if let Some(text) = span.extract(source) {
                let style = theme.style_for(span.kind);
                let css = Self::style_to_css(style);

                html.push_str(&format!(
                    "<span class=\"{}\" style=\"{}\">{}</span>",
                    span.kind.css_class(),
                    css,
                    Self::escape_html(text)
                ));
            }

            pos = span.span.end();
        }

        // Add any remaining unstyled text
        if pos < source.len() {
            html.push_str(&Self::escape_html(&source[pos..]));
        }
    }

    /// Render highlighted spans to HTML using CSS classes only.
    ///
    /// Like [`to_html_inline`](Self::to_html_inline) but emits no inline
    /// styles — each span carries just its `HighlightKind::css_class()`
    /// (`kf-root`, `kf-quality`, …), leaving the palette to a stylesheet.
    /// Unhighlighted gaps are HTML-escaped verbatim.
    ///
    /// Span offsets are snapped to char boundaries: the highlighter can
    /// produce char-misaligned offsets around multibyte text (e.g. `♭`), and
    /// this renderer must never panic or drop source text — worst case a
    /// span's styling shifts by a character.
    #[must_use]
    pub fn to_html_classes(source: &str, spans: &[HighlightSpan]) -> String {
        /// Floor `i` to the nearest char boundary at or below it.
        fn snap(s: &str, mut i: usize) -> usize {
            i = i.min(s.len());
            while i > 0 && !s.is_char_boundary(i) {
                i -= 1;
            }
            i
        }

        let mut html = String::with_capacity(source.len() * 2);
        let mut pos = 0;

        for span in spans {
            let start = snap(source, span.span.start).max(pos);
            let end = snap(source, span.span.end()).max(start);

            if start > pos {
                html.push_str(&Self::escape_html(&source[pos..start]));
            }

            if end > start {
                html.push_str(&format!(
                    "<span class=\"{}\">{}</span>",
                    span.kind.css_class(),
                    Self::escape_html(&source[start..end])
                ));
            }

            pos = end;
        }

        if pos < source.len() {
            html.push_str(&Self::escape_html(&source[pos..]));
        }

        html
    }

    /// Convert a style to inline CSS.
    fn style_to_css(style: &Style) -> String {
        let mut css = format!("color:{};", style.color.to_css());

        if style.bold {
            css.push_str("font-weight:bold;");
        }
        if style.italic {
            css.push_str("font-style:italic;");
        }
        if style.underline {
            css.push_str("text-decoration:underline;");
        }

        css
    }

    /// Escape HTML special characters.
    fn escape_html(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    /// Render highlighted spans to ANSI terminal escape codes.
    ///
    /// Uses 256-color mode for broad terminal compatibility.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let spans = Highlighter::highlight_line("Gmaj7");
    /// let ansi = Renderer::to_ansi("Gmaj7", &spans, &Theme::default_dark());
    /// println!("{}", ansi);
    /// ```
    #[must_use]
    pub fn to_ansi(source: &str, spans: &[HighlightSpan], theme: &Theme) -> String {
        let mut output = String::with_capacity(source.len() * 2);
        let mut pos = 0;

        for span in spans {
            // Add unstyled text before this span
            if span.span.start > pos {
                output.push_str(&source[pos..span.span.start]);
            }

            // Add the styled span
            if let Some(text) = span.extract(source) {
                let style = theme.style_for(span.kind);
                output.push_str(&Self::style_to_ansi(style));
                output.push_str(text);
                output.push_str("\x1b[0m"); // Reset
            }

            pos = span.span.end();
        }

        // Add any remaining unstyled text
        if pos < source.len() {
            output.push_str(&source[pos..]);
        }

        output
    }

    /// Convert a style to ANSI escape codes.
    fn style_to_ansi(style: &Style) -> String {
        let mut codes = Vec::new();

        // Color using 256-color mode
        let color_code = style.color.to_ansi_256();
        codes.push(format!("38;5;{color_code}"));

        if style.bold {
            codes.push("1".to_string());
        }
        if style.italic {
            codes.push("3".to_string());
        }
        if style.underline {
            codes.push("4".to_string());
        }

        format!("\x1b[{}m", codes.join(";"))
    }

    /// Convert highlighted spans to styled spans for UI framework rendering.
    ///
    /// Returns a vector of `StyledSpan` objects that contain both the
    /// text and styling information, suitable for direct rendering in
    /// frameworks like Dioxus, Yew, or egui.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let spans = Highlighter::highlight_line("Gmaj7");
    /// let styled = Renderer::to_styled_spans("Gmaj7", &spans, &Theme::default_dark());
    /// for span in styled {
    ///     // Render span.text with span.style
    /// }
    /// ```
    #[must_use]
    pub fn to_styled_spans(
        source: &str,
        spans: &[HighlightSpan],
        theme: &Theme,
    ) -> Vec<StyledSpan> {
        let mut styled = Vec::new();
        let mut pos = 0;

        for span in spans {
            // Add unstyled text before this span
            if span.span.start > pos {
                let text = source[pos..span.span.start].to_string();
                if !text.is_empty() {
                    styled.push(StyledSpan::new(
                        text,
                        HighlightKind::Unknown,
                        Style::color(theme.foreground),
                    ));
                }
            }

            // Add the styled span
            if let Some(text) = span.extract(source) {
                let style = theme.style_for(span.kind).clone();
                styled.push(StyledSpan::new(text.to_string(), span.kind, style));
            }

            pos = span.span.end();
        }

        // Add any remaining unstyled text
        if pos < source.len() {
            let text = source[pos..].to_string();
            if !text.is_empty() {
                styled.push(StyledSpan::new(
                    text,
                    HighlightKind::Unknown,
                    Style::color(theme.foreground),
                ));
            }
        }

        styled
    }

    /// Generate a CSS stylesheet for highlight classes.
    ///
    /// Produces CSS rules for all highlight kind classes,
    /// useful for external stylesheet generation.
    #[must_use]
    pub fn generate_css(theme: &Theme) -> String {
        let mut css = String::new();

        css.push_str(&format!(
            ".keyflow-code {{ background: {}; color: {}; font-family: monospace; }}\n",
            theme.background.to_css(),
            theme.foreground.to_css()
        ));

        // Generate rules for each highlight kind
        let kinds = [
            HighlightKind::Root,
            HighlightKind::ScaleDegree,
            HighlightKind::RomanNumeral,
            HighlightKind::Accidental,
            HighlightKind::Quality,
            HighlightKind::Extension,
            HighlightKind::Modifier,
            HighlightKind::Bass,
            HighlightKind::BassSlash,
            HighlightKind::Duration,
            HighlightKind::SlashRhythm,
            HighlightKind::Rest,
            HighlightKind::Space,
            HighlightKind::Push,
            HighlightKind::Pull,
            HighlightKind::Triplet,
            HighlightKind::Dot,
            HighlightKind::Section,
            HighlightKind::MeasureCount,
            HighlightKind::SectionComment,
            HighlightKind::SectionBracket,
            HighlightKind::MeasureSeparator,
            HighlightKind::Title,
            HighlightKind::Artist,
            HighlightKind::Tempo,
            HighlightKind::TimeSignature,
            HighlightKind::Key,
            HighlightKind::TempoArrow,
            HighlightKind::Command,
            HighlightKind::Dynamic,
            HighlightKind::TextCue,
            HighlightKind::Comment,
            HighlightKind::CommentMarker,
            HighlightKind::MemoryRecall,
            HighlightKind::Repeat,
            HighlightKind::TrackMarker,
            HighlightKind::MelodyBlock,
            HighlightKind::Unknown,
        ];

        for kind in kinds {
            let style = theme.style_for(kind);
            let class = kind.css_class();

            css.push_str(&format!(".{} {{ ", class));
            css.push_str(&format!("color: {}; ", style.color.to_css()));

            if style.bold {
                css.push_str("font-weight: bold; ");
            }
            if style.italic {
                css.push_str("font-style: italic; ");
            }
            if style.underline {
                css.push_str("text-decoration: underline; ");
            }

            css.push_str("}\n");
        }

        css
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlighting::Highlighter;

    #[test]
    fn test_to_html() {
        let source = "Gmaj7";
        let spans = Highlighter::highlight_line(source);
        let html = Renderer::to_html(source, &spans, &Theme::default_dark());

        assert!(html.contains("<pre"));
        assert!(html.contains("</pre>"));
        assert!(html.contains("kf-root"));
    }

    #[test]
    fn test_to_html_inline() {
        let source = "Am";
        let spans = Highlighter::highlight_line(source);
        let html = Renderer::to_html_inline(source, &spans, &Theme::default_dark());

        assert!(!html.contains("<pre"));
        assert!(html.contains("span"));
    }

    #[test]
    fn test_to_ansi() {
        let source = "G7";
        let spans = Highlighter::highlight_line(source);
        let ansi = Renderer::to_ansi(source, &spans, &Theme::default_dark());

        // Should contain ANSI escape codes
        assert!(ansi.contains("\x1b["));
        // Should contain reset code
        assert!(ansi.contains("\x1b[0m"));
    }

    #[test]
    fn test_to_styled_spans() {
        let source = "G_4";
        let spans = Highlighter::highlight_line(source);
        let styled = Renderer::to_styled_spans(source, &spans, &Theme::default_dark());

        assert!(!styled.is_empty());
        // Should have at least root and duration
        assert!(styled.iter().any(|s| s.kind == HighlightKind::Root));
        assert!(styled.iter().any(|s| s.kind == HighlightKind::Duration));
    }

    #[test]
    fn test_generate_css() {
        let css = Renderer::generate_css(&Theme::default_dark());

        assert!(css.contains(".keyflow-code"));
        assert!(css.contains(".kf-root"));
        assert!(css.contains(".kf-quality"));
        assert!(css.contains(".kf-section"));
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(Renderer::escape_html("<test>"), "&lt;test&gt;");
        assert_eq!(Renderer::escape_html("a & b"), "a &amp; b");
        assert_eq!(Renderer::escape_html("\"quote\""), "&quot;quote&quot;");
    }

    #[test]
    fn test_styled_span_creation() {
        let style = Style::bold(super::super::theme::Color::rgb(255, 0, 0));
        let span = StyledSpan::new("test".to_string(), HighlightKind::Root, style);

        assert_eq!(span.text, "test");
        assert_eq!(span.kind, HighlightKind::Root);
        assert!(span.style.bold);
    }
}
