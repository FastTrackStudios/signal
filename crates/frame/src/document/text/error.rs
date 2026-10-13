/// A markup error, positioned for a human (or a model) to fix.
///
/// `line` and `col` are 1-based; `col` counts characters, not bytes.
/// `Display` renders `line:col: message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl ParseError {
    pub(crate) fn at(src: &str, offset: usize, message: impl Into<String>) -> Self {
        let (line, col) = line_col(src, offset);
        Self {
            line,
            col,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for ParseError {}

pub(crate) fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(src.len());
    let before = src.get(..offset).unwrap_or(src);
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let col = before[line_start..].chars().count() + 1;
    (line, col)
}
