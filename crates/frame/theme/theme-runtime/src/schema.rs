//! What a host can bind.

/// How often a field's value changes, which decides when what reads it is
/// worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Changes when an instance is re-recorded (a track's name).
    Static,
    /// Changes every frame (a meter): read in the live pass.
    Live,
    /// Shapes the layout like the box does (the height a whole mixer's
    /// sections are resolved against): read when laying out, and each
    /// value it takes is a layout of its own.
    Layout,
}

/// The fields a host offers a theme, in slot order.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    fields: Vec<(String, Kind)>,
}

impl Schema {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A field whose value changes only when the instance is re-recorded
    /// (a track's name, colour, mute).
    #[must_use]
    pub fn field(mut self, name: &str) -> Self {
        self.fields.push((name.to_owned(), Kind::Static));
        self
    }

    /// A field that changes every frame (a meter, a fader in a drag):
    /// whatever reads it is painted in the live pass.
    #[must_use]
    pub fn live(mut self, name: &str) -> Self {
        self.fields.push((name.to_owned(), Kind::Live));
        self
    }

    /// A field the layout depends on, as it depends on the box: bindings
    /// that read only the box and layout fields are solved once per
    /// distinct value, with the layout. A mixer's own height, which every
    /// strip resolves its sections against whatever its own height is.
    #[must_use]
    pub fn layout(mut self, name: &str) -> Self {
        self.fields.push((name.to_owned(), Kind::Layout));
        self
    }

    /// The slot `name` is read from.
    #[must_use]
    pub fn slot(&self, name: &str) -> Option<u32> {
        self.fields.iter().position(|(n, _)| n == name).and_then(|i| u32::try_from(i).ok())
    }

    pub(crate) fn kind(&self, slot: u32) -> Option<Kind> {
        usize::try_from(slot).ok().and_then(|i| self.fields.get(i)).map(|(_, k)| *k)
    }

    #[must_use]
    pub fn is_live(&self, slot: u32) -> bool {
        self.kind(slot) == Some(Kind::Live)
    }

    #[must_use]
    pub fn is_layout(&self, slot: u32) -> bool {
        self.kind(slot) == Some(Kind::Layout)
    }

    /// The layout fields' slots.
    pub(crate) fn layout_slots(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.fields.len()).filter_map(|i| u32::try_from(i).ok()).filter(|s| self.is_layout(*s))
    }

    /// Field names, in slot order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.fields.iter().map(|(n, _)| n.as_str())
    }
}
