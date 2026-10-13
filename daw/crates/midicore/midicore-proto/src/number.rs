//! Constrained numeric newtypes — the vocabulary that makes illegal MIDI
//! values unrepresentable.
//!
//! Following `helgoboss-midi`, the raw widths (`U4`/`U7`/`U14`) and the
//! *semantic* roles (`KeyNumber`, `Velocity`, `ControllerNumber`, …) are
//! distinct types, so a note number can't be passed where a controller value is
//! expected. Each offers `new` (clamping, `const`) and `try_new` (fallible).

use facet::Facet;

/// Generates a 7-bit (`0..=127`) newtype with clamping/fallible constructors,
/// a compact `Debug`, and `Facet` for the wire.
macro_rules! u7_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
        pub struct $name(u8);

        impl $name {
            /// Smallest value (`0`).
            pub const MIN: $name = $name(0);
            /// Largest value (`127`).
            pub const MAX: $name = $name(127);

            /// Construct, clamping into `0..=127`.
            pub const fn new(raw: u8) -> Self {
                Self(if raw > 127 { 127 } else { raw })
            }
            /// Construct, returning `None` if `raw > 127` (status bit set).
            pub const fn try_new(raw: u8) -> Option<Self> {
                if raw < 128 { Some(Self(raw)) } else { None }
            }
            /// The raw `0..=127` value.
            pub const fn get(self) -> u8 {
                self.0
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }

        impl From<$name> for u8 {
            fn from(v: $name) -> u8 { v.0 }
        }
    };
}

u7_newtype!(
    /// A raw 7-bit value, `0..=127` — the base width of every MIDI data byte.
    U7
);
u7_newtype!(
    /// A note/key number, `0..=127` (60 = middle C).
    KeyNumber
);
u7_newtype!(
    /// A note velocity, `0..=127` (0 on a note-on means note-off).
    Velocity
);
u7_newtype!(
    /// A control-change controller number, `0..=127`.
    ControllerNumber
);
u7_newtype!(
    /// A control-change value, `0..=127`.
    ControllerValue
);
u7_newtype!(
    /// A program (patch) number, `0..=127`.
    ProgramNumber
);
u7_newtype!(
    /// A pressure / aftertouch amount, `0..=127`.
    Pressure
);

/// A 4-bit value, `0..=15` — the width of the channel nibble.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct U4(u8);

impl U4 {
    pub const MIN: U4 = U4(0);
    pub const MAX: U4 = U4(15);
    pub const fn new(raw: u8) -> Self {
        Self(if raw > 15 { 15 } else { raw })
    }
    pub const fn try_new(raw: u8) -> Option<Self> {
        if raw < 16 {
            Some(Self(raw))
        } else {
            None
        }
    }
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl core::fmt::Debug for U4 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "U4({})", self.0)
    }
}

/// A 14-bit value, `0..=16383` — pitch-bend and 14-bit CC resolution.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct U14(u16);

impl U14 {
    pub const MIN: U14 = U14(0);
    pub const MAX: U14 = U14(16383);
    pub const CENTER: U14 = U14(8192);

    pub const fn new(raw: u16) -> Self {
        Self(if raw > 16383 { 16383 } else { raw })
    }
    pub const fn try_new(raw: u16) -> Option<Self> {
        if raw < 16384 {
            Some(Self(raw))
        } else {
            None
        }
    }
    pub const fn get(self) -> u16 {
        self.0
    }
    /// The `(lsb, msb)` 7-bit halves as they appear on the wire.
    pub const fn to_halves(self) -> (U7, U7) {
        (U7(self.0 as u8 & 0x7f), U7((self.0 >> 7) as u8 & 0x7f))
    }
    /// Reassemble from the wire `(lsb, msb)` pair.
    pub const fn from_halves(lsb: U7, msb: U7) -> Self {
        Self(((msb.0 as u16) << 7) | lsb.0 as u16)
    }
}

impl core::fmt::Debug for U14 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "U14({})", self.0)
    }
}

/// A MIDI channel, `0..=15` (displayed to humans as `1..=16`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct Channel(u8);

impl Channel {
    pub const fn new(raw: u8) -> Self {
        Self(if raw > 15 { 15 } else { raw })
    }
    pub const fn try_new(raw: u8) -> Option<Self> {
        if raw < 16 {
            Some(Self(raw))
        } else {
            None
        }
    }
    /// The raw `0..=15` value used on the wire.
    pub const fn index(self) -> u8 {
        self.0
    }
    /// The human-facing `1..=16` channel number.
    pub const fn number(self) -> u8 {
        self.0 + 1
    }
}

impl core::fmt::Debug for Channel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Channel({})", self.number())
    }
}

/// A pitch-bend position — a 14-bit value centered at 8192.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct PitchBend(U14);

impl PitchBend {
    pub const CENTER: PitchBend = PitchBend(U14::CENTER);

    pub const fn new(raw: u16) -> Self {
        Self(U14::new(raw))
    }
    pub const fn from_u14(v: U14) -> Self {
        Self(v)
    }
    pub const fn get(self) -> u16 {
        self.0.get()
    }
    /// Signed offset from center, `-8192..=8191`.
    pub const fn offset(self) -> i16 {
        self.0.get() as i16 - 8192
    }
    pub const fn to_halves(self) -> (U7, U7) {
        self.0.to_halves()
    }
    pub const fn from_halves(lsb: U7, msb: U7) -> Self {
        Self(U14::from_halves(lsb, msb))
    }
}

impl core::fmt::Debug for PitchBend {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PitchBend({:+})", self.offset())
    }
}
