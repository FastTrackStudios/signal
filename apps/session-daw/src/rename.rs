//! Renaming a track in place.
//!
//! A control is a value you set; a name is text you EDIT, and the
//! difference is a caret. So this is its own small state machine rather
//! than another control: while it is open the keyboard belongs to it,
//! and nothing else in the window should act on a keypress.
//!
//! # What it deliberately is not
//!
//! Not a text field with selection, word motion, or a clipboard. A
//! track name is a few words typed once, and every one of those
//! features is a place for the editing model to disagree with the
//! system's. When a real text surface exists — the expression editor
//! has one — this should become a use of it rather than a second
//! implementation that got further.

/// An open rename.
#[derive(Clone, Debug, PartialEq)]
pub struct Rename {
    /// Which panel the edit is open in.
    ///
    /// The two have different geometry for a name plate — a row's is
    /// beside its knobs, a strip's is across its bottom — and only one
    /// is on screen at a time. Carried so the paint asks the right
    /// layout rather than the window remembering which view it was in
    /// when the rename opened.
    pub surface: Surface,
    /// Which row or strip is being renamed.
    pub row: usize,
    /// What is being renamed, so the edit survives the rows being
    /// rebuilt underneath it — and so a mark in the ruler can be
    /// renamed by the same machine as a track, which is the whole
    /// reason this is a state machine and not a widget.
    pub what: What,
    text: String,
    /// The caret, as a byte offset into `text`.
    ///
    /// Bytes rather than characters because that is what `String`
    /// indexes by, and kept on a character boundary by only ever moving
    /// it with `char_indices` — a caret in the middle of a multi-byte
    /// character would panic on the next insert.
    caret: usize,
}

/// Which panel a rename is open in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// A row in the arrangement's track panel.
    Arrange,
    /// A strip in the mixer.
    Mixer,
    /// A mark or a band in the ruler's lanes.
    Ruler,
}

/// What a rename is renaming.
///
/// A track is addressed by its guid, which survives anything. A marker
/// or a region is addressed by REAPER's own number, which does not — a
/// renumber reassigns it — so a rename left open across one commits to
/// whatever now holds that number. That is the same risk every REAPER
/// action on a marker carries, and the alternative is refusing to
/// rename the thing the user is looking at.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum What {
    Track(String),
    Marker(u32),
    Region(u32),
}

impl Rename {
    /// Begin, with the existing name selected in the sense that typing
    /// replaces it.
    ///
    /// REAPER's behaviour and every other renamer's: you double-click a
    /// name to CHANGE it, so the common case is typing a new one, and
    /// the rarer case — fixing a typo — is one End key away.
    #[must_use]
    pub fn new(surface: Surface, row: usize, what: What, from: &str) -> Self {
        Self {
            surface,
            row,
            what,
            text: from.to_owned(),
            caret: from.len(),
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn caret(&self) -> usize {
        self.caret
    }

    /// Type a character.
    pub fn insert(&mut self, c: char) {
        // Control characters are not text. They arrive as key events
        // too, and inserting one puts an unprintable glyph in a track
        // name that looks like nothing and breaks the next search.
        if c.is_control() {
            return;
        }
        self.text.insert(self.caret, c);
        self.caret += c.len_utf8();
    }

    /// Backspace: delete the character before the caret.
    pub fn backspace(&mut self) {
        let Some((at, ch)) = self.text[..self.caret].char_indices().next_back() else {
            return;
        };
        self.text.remove(at);
        self.caret = at;
        let _ = ch;
    }

    /// Delete: the character after it.
    pub fn delete(&mut self) {
        if self.caret < self.text.len() {
            self.text.remove(self.caret);
        }
    }

    /// Move the caret one character left.
    pub fn left(&mut self) {
        if let Some((at, _)) = self.text[..self.caret].char_indices().next_back() {
            self.caret = at;
        }
    }

    /// And one right.
    pub fn right(&mut self) {
        if let Some(ch) = self.text[self.caret..].chars().next() {
            self.caret += ch.len_utf8();
        }
    }

    pub fn home(&mut self) {
        self.caret = 0;
    }

    pub fn end(&mut self) {
        self.caret = self.text.len();
    }

    /// The name to commit, or `None` if it should not be.
    ///
    /// An empty name is refused rather than accepted: a track with no
    /// name cannot be found, selected by name, or told apart from its
    /// neighbours, and "I cleared it and pressed enter" is far more
    /// often a mistake than an intention.
    #[must_use]
    pub fn commit(&self) -> Option<&str> {
        let trimmed = self.text.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::{Rename, Surface, What};

    fn open(name: &str) -> Rename {
        Rename::new(Surface::Arrange, 3, What::Track("kick".into()), name)
    }

    #[test]
    fn typing_appends_at_the_caret() {
        let mut r = open("Kick");
        r.insert('!');
        assert_eq!(r.text(), "Kick!");
        r.home();
        r.insert('>');
        assert_eq!(r.text(), ">Kick!");
    }

    #[test]
    fn backspace_and_delete_are_opposite_sides_of_the_caret() {
        let mut r = open("Kick");
        r.backspace();
        assert_eq!(r.text(), "Kic");
        r.home();
        r.delete();
        assert_eq!(r.text(), "ic");
    }

    /// The caret never lands inside a character. Bytes are what a
    /// `String` indexes by, and a caret halfway through a multi-byte
    /// character panics on the next insert — so every motion goes
    /// through `char_indices`.
    #[test]
    fn the_caret_stays_on_character_boundaries() {
        let mut r = open("Käse—Bü");
        for _ in 0..20 {
            r.left();
        }
        assert_eq!(r.caret(), 0);
        for _ in 0..20 {
            r.right();
            // Would panic if the caret were mid-character.
            let _ = &r.text()[..r.caret()];
        }
        assert_eq!(r.caret(), r.text().len());
        r.insert('!');
        assert!(r.text().ends_with('!'));
    }

    /// Backspace on an empty name does nothing rather than panicking.
    #[test]
    fn backspace_at_the_start_is_harmless() {
        let mut r = open("");
        r.backspace();
        r.delete();
        r.left();
        assert_eq!(r.text(), "");
        assert_eq!(r.caret(), 0);
    }

    /// An empty name is refused. A track with no name cannot be found,
    /// selected by name, or told from its neighbours, and clearing the
    /// field is far more often a slip than an intention.
    #[test]
    fn an_empty_name_is_not_committed() {
        let mut r = open("Kick");
        for _ in 0..10 {
            r.backspace();
        }
        assert_eq!(r.commit(), None);
        r.insert(' ');
        assert_eq!(r.commit(), None, "whitespace is not a name either");
        r.insert('K');
        assert_eq!(r.commit(), Some("K"));
    }

    /// Surrounding space is trimmed, because it is invisible and makes
    /// two tracks that look identical sort apart.
    #[test]
    fn a_name_is_trimmed() {
        let mut r = open("  Kick  ");
        assert_eq!(r.commit(), Some("Kick"));
        r.insert('!');
        assert_eq!(r.commit(), Some("Kick  !".trim()));
    }

    /// Control characters arrive as key events and are not text.
    #[test]
    fn control_characters_are_not_typed() {
        let mut r = open("Kick");
        for c in ['\n', '\t', '\u{1b}', '\u{7f}'] {
            r.insert(c);
        }
        assert_eq!(r.text(), "Kick");
    }
}

/// Draw an open rename over the name it replaces.
///
/// Live, over the recorded row, like every other changing thing in this
/// window — a name being typed changes on every keystroke, and
/// re-recording the panel per character would be the most expensive
/// text field ever built.
pub fn paint(
    painter: &mut impl anyrender::PaintScene,
    palette: &crate::arrangement::Palette,
    font: &crate::text::Font,
    rename: &Rename,
    field: vello::kurbo::Rect,
    transform: vello::kurbo::Affine,
) {
    use anyrender::PaintScene as _;
    use vello::kurbo::Rect;
    use vello::peniko::Fill;

    let mut scene = anyrender::Scene::new();
    // A lit field, so it is obvious the keyboard has gone somewhere.
    scene.fill(
        Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
        palette.tcp_field,
        None,
        &field,
    );
    scene.fill(
        Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
        palette.accent,
        None,
        &Rect::new(field.x0, field.y1 - 1.0, field.x1, field.y1),
    );

    const SIZE: f32 = 11.0;
    let baseline = field.y0 + field.height() / 2.0 + f64::from(SIZE) / 3.0;
    // The text scrolls to keep the caret visible: a name longer than
    // its field is normal, and a caret you cannot see is a text field
    // you cannot use.
    let before = font.width(&rename.text()[..rename.caret()], SIZE);
    let room = field.width() - 8.0;
    let shift = (before - room).max(0.0);
    crate::tcp::glyphs(
        &mut scene,
        font,
        palette.text,
        rename.text(),
        field.x0 + 4.0 - shift,
        baseline,
        SIZE,
    );
    let caret = field.x0 + 4.0 + before - shift;
    scene.fill(
        Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
        palette.text,
        None,
        &Rect::new(caret, field.y0 + 3.0, caret + 1.0, field.y1 - 3.0),
    );

    for command in &scene.commands {
        crate::arrangement::submit_command(painter, command, transform);
    }
}
