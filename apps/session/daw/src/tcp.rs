//! The track control panel, drawn into the recorded scene.
//!
//! # Why this is not the DOM row
//!
//! `daw_ui::components::tcp::TrackRow` is the REAPER-matched panel, and
//! it stays the reference: every coordinate here comes from
//! `daw_theme_art::geometry::tcp`, the same measured constants that row
//! positions its controls with. Neither file is allowed to invent a
//! number. When REAPER's layout is re-measured, both move, because both
//! read the same constants.
//!
//! What differs is the RENDERING, and only because it has to. The DOM
//! row is a tree of elements with SVG controls inside it; this is a list
//! of fills and glyph runs recorded once and replayed under a transform.
//! The panel had to come onto the canvas because it could not otherwise
//! stay level with the lanes — native scroll moves DOM content on the
//! compositor while a canvas redraws on the main thread, and the two
//! visibly tore apart while scrolling.
//!
//! # What is drawn, and what is not yet
//!
//! Drawn: the tint, the left column and its rule, the track number, the
//! name field with the record arm on it, the name, volume and pan, the
//! routing and FX buttons, the envelope button, the FX slot, the record
//! input combo, mute and solo in their gutter, phase, and the meter.
//!
//! Not yet: the meter's live level (it draws its well, because at rest
//! REAPER's shows nothing), the fixed-lanes button, and hit testing.
//! Those are the next pieces, and they are listed here rather than left
//! to be discovered as absences.

use anyrender::PaintScene;
use daw_proto::Track;
use daw_theme_art::geometry::tcp as g;
use vello::kurbo::{Affine, Rect, Vec2};
use vello::peniko::{Color, Fill};

use daw_theme_art::paint::tcp as art;

use crate::arrangement::Palette;
use crate::text::Font;

/// How much of a track panel fits in the height it has.
///
/// REAPER stops shrinking tracks well before a session of two thousand
/// fits on a screen, which is the wrong trade when the question is
/// "where is everything". So rows here go as small as
/// [`crate::heights::MIN`], and the panel sheds controls on the way down
/// rather than drawing them on top of each other.
///
/// The thresholds are what each tier actually NEEDS, not taste: `Full`
/// is row one at 6 plus its 24, row two at 34 plus its 20, and the
/// phase button hanging off the bottom.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Density {
    /// Everything REAPER's row has.
    Full,
    /// The whole control row — name field, record arm, volume, pan,
    /// routing, FX, mute and solo — flattened into whatever height the
    /// row has.
    ///
    /// Controls are SQUASHED here, not dropped. A track at fourteen
    /// pixels still has everything a track at seventy has, just flatter,
    /// and every one of them stays in the same column: a panel where
    /// controls appear and vanish as tracks resize cannot be read down,
    /// and hitting a control would depend on how tall its track happens
    /// to be. What row two and the stacked gutter cost is HEIGHT, and
    /// that is the one thing this tier does not have, so mute and solo
    /// lie down beside the row instead of stacking above it.
    Compact,
    /// A coloured band. At this height a glyph would not be legible and
    /// a control would not be hittable, so the row is its tint and its
    /// divider — which is exactly what a session zoomed out to fit is
    /// being read for.
    Bar,
}

/// The colours the ported controls light up in, from the resolved theme.
///
/// One place, so a control cannot pick a different blue from the one
/// beside it — and so that changing the theme changes the controls,
/// which was the whole reason the art stopped carrying its own hex.
#[must_use]
pub fn lit(palette: &Palette) -> art::Lit {
    art::Lit {
        volume: to_theme(palette.accent),
        pan: to_theme(palette.pan),
        rec: to_theme(palette.rec),
        bypass: to_theme(palette.mute),
    }
}

/// Mute and solo, at the one size they are drawn at everywhere.
///
/// The theme's measured 21 by 20 — and that size on every track, which
/// is why `Layout::min` is what it is: the shortest settable row has to
/// hold the controls, rather than the controls shrinking to fit each
/// row. A control that is a different shape on every track cannot be
/// built on — no shared hit target, no drag across a column, no "the
/// mute column" for anything else to address.
pub const BUTTON: (f64, f64) = (21.0, 20.0);
/// Between the two, so they read as two controls.
pub const BUTTON_GAP: f64 = 1.0;

/// The space between the track number and the icon under it, in the rail.
const ICON_GAP: f64 = 4.0;

/// The compact panel's name field at its widest: the knob and the gutter
/// it gave up are the name's.
const NAMES_FIELD_W: f64 = 112.0;
/// And at its narrowest: a short name still gets a field to press.
const NAMES_FIELD_MIN: f64 = 48.0;

/// The compact panel's name field as the rows shown need it
/// ([`fit_names`]): as wide as the longest name, never wider than
/// [`NAMES_FIELD_W`]. The window's, as the panel's shape is — the
/// arrangement, its chrome and its hit test read the same width.
static NAMES_FIT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(NAMES_FIELD_W.to_bits());

fn names_field_w() -> f64 {
    f64::from_bits(NAMES_FIT.load(std::sync::atomic::Ordering::Relaxed))
}

/// Fit the compact panel's name field to `rows`: the longest name at the
/// size its row draws it, its indent, and the field's own margins.
/// `true` when the width changed.
pub fn fit_names(font: &Font, rows: &[(Track, u32)], layout: crate::layout::Layout) -> bool {
    let tcp = Tcp::COMPACT;
    let widest = rows
        .iter()
        .map(|(track, depth)| {
            let size = tcp.name_size(layout.height_of(track.height));
            let indent = (f64::from(*depth) * tcp.indent()).min(tcp.max_indent());
            font.width(&track.name, size) + indent + 10.0
        })
        .fold(0.0_f64, f64::max);
    let fit = widest.ceil().clamp(NAMES_FIELD_MIN, NAMES_FIELD_W);
    let was = names_field_w();
    NAMES_FIT.store(fit.to_bits(), std::sync::atomic::Ordering::Relaxed);
    (was - fit).abs() > 0.5
}

/// Below this tall, volume and pan stop being knobs.
///
/// A knob says its value with the angle of a ring, and an angle needs a
/// circle big enough to have angles in it — at fourteen pixels the ring
/// is three pixels of arc. A fader and a line keep saying it at any
/// height, which matters most exactly here: this is the height tracks
/// sit at once a session is collapsed enough to see all of it.
pub const KNOB_LEGIBLE: f64 = 20.0;

/// Below this many pixels ON SCREEN, a row is drawn as a band.
///
/// The same threshold [`Density::at`] uses, named separately because the
/// replay applies it to the row's height AFTER the zoom while the
/// recording applies it before — see `Arrangement::panel_bar`.
pub const BAND_BELOW: f64 = 11.0;

impl Density {
    /// What fits in `height`.
    #[must_use]
    pub fn at(height: f64) -> Self {
        if height >= 58.0 {
            Self::Full
        } else if height >= BAND_BELOW {
            // Below this a squashed control is a smear and a name is
            // not legible at any size, so there is nothing left to draw
            // but the band.
            Self::Compact
        } else {
            Self::Bar
        }
    }
}

/// The panel's shape: how wide it is, what it shows, and how far it
/// indents.
///
/// Two of them. **Full** is the measured REAPER panel — the name with the
/// record arm on it, volume, pan, routing, the FX pill. **Compact** is the
/// track's name and colour, as Logic's iPad shows its tracks: what a row
/// is scanned for when there is not room for more, a touchscreen's or a
/// docked panel's. A row tall enough for a second line gets its arm, mute
/// and solo on it; the rest is the full panel, which a swipe opens.
///
/// A value rather than a constant because both panels are drawn from the
/// same code: the recorded row, the live controls over it, and the hit
/// test all read this, so they cannot disagree about where anything is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tcp {
    pub compact: bool,
    /// Recording: the record arm is shown only then — in any other mode
    /// it is a button for a job you are not doing.
    pub recording: bool,
    /// Mixing: every item drawn in its track's colour. In the other modes
    /// an item's own colour shows — which take it is — and in a mix that
    /// is noise: what reads there is which track it is.
    pub mixing: bool,
}

impl Tcp {
    pub const FULL: Self = Self {
        compact: false,
        recording: true,
        mixing: false,
    };
    pub const COMPACT: Self = Self {
        compact: true,
        recording: true,
        mixing: false,
    };

    /// The panel's width.
    #[must_use]
    pub fn width(self) -> f64 {
        if self.compact {
            // The rail, the name, and a hair of margin: nothing else.
            f64::from(g::NAME_FIELD_X) + names_field_w() + 6.0
        } else {
            f64::from(g::ROW_W)
        }
    }

    /// Where the gutter starts — the tinted part's width. The compact
    /// panel has no gutter: it is all tint.
    #[must_use]
    pub fn tint_w(self) -> f64 {
        if self.compact {
            self.width()
        } else {
            f64::from(g::TINT_W)
        }
    }

    /// The name field: where it starts, and how wide.
    #[must_use]
    pub fn name_field(self) -> (f64, f64) {
        let x = f64::from(g::NAME_FIELD_X);
        let w = if self.compact {
            names_field_w()
        } else {
            f64::from(g::NAME_FIELD_W)
        };
        (x, w)
    }

    /// Where the name's own text starts: after the record arm where there
    /// is one, a hair inside the field where there is not.
    #[must_use]
    pub fn name_x(self) -> f64 {
        let (x, _) = self.name_field();
        if self.compact { x + 2.0 } else { 58.0 }
    }

    /// The volume knob's centre — the field's right end, either way. The
    /// compact panel has no knob; the name runs to the field's end.
    #[must_use]
    pub fn volume_x(self) -> f64 {
        let (x, w) = self.name_field();
        x + w
    }

    /// Whether this panel shows a control at all.
    #[must_use]
    pub fn shows(self, control: crate::row::Control) -> bool {
        use crate::row::Control;
        // The compact panel is the track's name and nothing else, however
        // tall the row: mute, solo and the arm are the inspector's (down
        // the window's left) and the expanded panel's, a swipe away.
        !self.compact || matches!(control, Control::Folder | Control::Name)
    }

    /// How far a folder's children are indented per level, and at most.
    #[must_use]
    pub fn indent(self) -> f64 {
        if self.compact { 6.0 } else { INDENT }
    }

    #[must_use]
    pub fn max_indent(self) -> f64 {
        if self.compact { 24.0 } else { MAX_INDENT }
    }

    /// The name's type size for a field of `height`.
    #[must_use]
    pub fn name_size(self, height: f64) -> f32 {
        let size = name_size(height);
        if self.compact { size - 1.0 } else { size }
    }
}

/// How far a folder's children are indented per level.
///
/// REAPER indents the row's CONTENT, not the row, so the tint still
/// reaches the panel's edge and only the field and the number move.
pub const INDENT: f64 = 10.0;
/// Past this, an indent would push the name field into the volume knob.
/// Deep templates nest further than the panel is wide, and a clamp is
/// what stops a section eight folders down from drawing on top of its
/// own controls.
pub const MAX_INDENT: f64 = 60.0;

/// The height of a row's control band: the name, knobs and buttons.
pub const BAND_H: f64 = 24.0;

/// Where a row's control band sits: `(top, height)`.
///
/// The same size on every row, whatever its height, 6px down from the
/// top the way a full row has always had it. Only a row too short for
/// that shrinks the band, centred with a pixel clear of each divider.
/// Zooming the rows taller makes the lanes taller; it never stretches the
/// name or moves it off the line the mute and solo sit on. The painting
/// ([`draw_row`]) and the hit test ([`crate::row::Row`]) both ask here,
/// so they cannot disagree.
#[must_use]
pub fn band(y: f64, h: f64, density: Density) -> (f64, f64) {
    if density == Density::Full {
        return (y + f64::from(g::ROW_ONE), BAND_H);
    }
    let band_h = BAND_H.min((h - 2.0).max(1.0));
    let top = ((h - band_h) / 2.0).clamp(1.0, f64::from(g::ROW_ONE).max(1.0));
    (y + top, band_h)
}

/// Draw one row at `y`, in panel content space.
///
/// Everything is positioned relative to `y`, so a caller only has to
/// know the row pitch — which is the single fact keeping the panel level
/// with the lanes beside it.
pub fn draw_row(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    depth: i32,
    y: f64,
    h: f64,
    ancestors: &[Color],
    tcp: Tcp,
) {
    let indent = (f64::from(depth.max(0)) * tcp.indent()).min(tcp.max_indent());
    let tint = row_tint(palette, track);
    let density = Density::at(h);
    let rail = f64::from(g::COLUMN_RULE_X);

    // ── The row's ground ──
    //
    // The tint runs the full width and the meter section is painted over
    // its right end, which is how REAPER's `meterRight` reads: one row,
    // with a gutter at the end of it, not two panels side by side.
    rect(scene, tint, 0.0, y, tcp.width(), y + h);
    rect(
        scene,
        palette.tcp_gutter,
        tcp.tint_w(),
        y,
        tcp.width(),
        y + h,
    );

    if density == Density::Bar {
        // Five rectangles a row is nothing at 70px and everything at
        // two: a session zoomed to fit is two thousand rows at once, and
        // culling cannot help when all of them are visible. The column
        // strip, its rule and the panel's right edge are the three that
        // stop carrying information at this height — a one-pixel rule on
        // a two-pixel row is not a rule — so the band keeps the two that
        // say which track this is and how it is coloured.
        return;
    }

    // The left column, and the one-pixel rule closing it. The whole
    // rail moves with the indent rather than staying put while the
    // content slides out from under it: the rail IS the row's left
    // edge, and a nested track whose edge did not move read as a track
    // with a wide gutter rather than as a child.
    rect(scene, palette.tcp_column, 0.0, y, indent + rail, y + h);

    // ── The folders this row sits inside ──
    //
    // The indent is one [`INDENT`] of empty column per level, which said
    // only "this one is deeper". Each of those steps is a folder, so
    // each carries that folder's colour, and because every child of a
    // folder paints it at the same x the steps join top to bottom into
    // one unbroken vertical line — the folder's own left edge running
    // down past everything inside it.
    //
    // The same move the mixer makes along its bottom, turned ninety
    // degrees, which is the whole relationship between the two views.
    //
    // A folder paints its OWN colour in the first step of its rail too,
    // so the line starts on the folder's row rather than on its first
    // child. That is what makes it read as the folder reaching down
    // rather than as a mark its children happen to share.
    folder_steps(scene, ancestors, y, h);
    // The rail carries the row's OWN colour, at the same strength.
    //
    // It used to be flat `tcp_column`, which is nearly black — so the
    // coloured indent ran up to the rail and then stopped dead, and the
    // row read as "some colour, then a black gap, then the track". The
    // gap is where the folder icon and the track number live, which is
    // to say it is the part of the left edge you actually look at.
    //
    // Colouring it means the whole left edge is the track and its
    // lineage, unbroken from the panel's edge to the row's content, and
    // a folder's stripe starts on the folder's own row — its rail — and
    // continues down every child as their ancestor band.
    rect(
        scene,
        folder_band(palette, track),
        indent,
        y,
        indent + rail,
        y + h,
    );

    rect(
        scene,
        palette.tcp_rule,
        indent + rail,
        y,
        indent + rail + 1.0,
        y + h,
    );
    // The panel's own right edge — the boundary with the arrange view,
    // and the last thing missing from the row's silhouette.
    rect(
        scene,
        palette.tcp_rule,
        tcp.width() - 2.0,
        y,
        tcp.width() - 1.0,
        y + h,
    );

    // The left rail: the folder mark at its top, the track number under
    // it. REAPER puts the mark at the BOTTOM of this column; at the top
    // it lines up with the controls beside it, which is what makes a
    // folder readable while scanning a collapsed session rather than
    // something you find by looking down.
    if track.is_folder {
        // In the rail, which has already moved with the indent.
        let mark_scale = (h / 18.0).clamp(0.4, 1.0);
        crate::art::scaled(
            scene,
            &art::folder_mark(to_theme(palette.text_dim)),
            font,
            indent + 9.0_f64.mul_add(-mark_scale, rail) / 2.0,
            2.0_f64.mul_add(mark_scale, y),
            mark_scale,
        );
    }

    // The track number and its icon, as `rail` places them.
    let at = rail_layout(font, track, indent, y, h);
    if let Some(number) = &at.number {
        glyphs(
            scene,
            font,
            ink_on(folder_band(palette, track)),
            &number.text,
            number.x,
            number.baseline,
            number.size,
        );
    }
    if let Some(icon) = at.icon {
        // In the number's ink: on the rail's own colour, a lifted track
        // colour would all but vanish.
        crate::track_icon::paint(
            scene,
            crate::track_icon::Icon::of(&track.name, track.is_folder),
            icon,
            ink_on(folder_band(palette, track)),
        );
    }

    match density {
        Density::Full => {
            let (band_top, band_h) = band(y, h, density);
            row_one(scene, palette, font, track, indent, band_top, band_h, tcp);
            // Recording's row — the input FX and the input — which the
            // compact panel leaves out with the arm.
            if !tcp.compact {
                row_two(scene, palette, font, track, y);
                automation_button(scene, palette, y, h, tcp);
            }
        }
        // The same band as a full row, never taller: a taller row is a
        // taller lane beside the same controls, not a stretched name.
        Density::Compact => {
            let (band_top, band_h) = band(y, h, density);
            row_one(scene, palette, font, track, indent, band_top, band_h, tcp);
        }
        Density::Bar => {}
    }
}

/// The folders a row sits inside, one [`INDENT`] of its colour each down
/// the left edge — see `draw_row`. Shared with a theme's `%steps`.
pub fn folder_steps(scene: &mut anyrender::Scene, ancestors: &[Color], y: f64, h: f64) {
    for (level, tint) in ancestors.iter().enumerate() {
        let left = crate::num::coord(level) * INDENT;
        if left >= MAX_INDENT {
            break;
        }
        rect(
            scene,
            *tint,
            left,
            y,
            (left + INDENT).min(MAX_INDENT),
            y + h,
        );
    }
}

/// A row's track number, as fitted into its rail.
#[derive(Clone, Debug, PartialEq)]
pub struct Number {
    pub text: String,
    pub size: f32,
    /// The glyphs' left edge and baseline.
    pub x: f64,
    pub baseline: f64,
}

/// What the rail holds under the folder mark: the number, and under it
/// the track's icon where the row has the height for both.
#[derive(Clone, Debug, PartialEq)]
pub struct Rail {
    pub number: Option<Number>,
    pub icon: Option<Rect>,
}

/// Where the rail's number and icon go on a row at `y`, `h` tall, its rail
/// at `indent` — for the painters and for a theme alike, since only the
/// host measures text.
///
/// The number gets the space the folder mark leaves, and gives way
/// entirely when the mark has taken the rail — the mark is the fact worth
/// keeping when only one of the two fits. Sized and centred to sit INSIDE
/// the rail: a two-digit number left-aligned at eleven points ran past
/// the rule closing it. The icon goes under it only where the row has the
/// height for both, the two centred together.
#[must_use]
pub fn rail_layout(font: &Font, track: &Track, indent: f64, y: f64, h: f64) -> Rail {
    let rail = f64::from(g::COLUMN_RULE_X);
    let mark_h = if track.is_folder {
        (7.0 + 2.0) * (h / 18.0).clamp(0.4, 1.0)
    } else {
        0.0
    };
    let icon = (rail - 6.0).min(16.0);
    let with_icon = h - mark_h >= 11.0 + ICON_GAP + icon + 6.0;
    let group = if with_icon {
        11.0 + ICON_GAP + icon
    } else {
        0.0
    };
    if h - mark_h < 11.0 {
        return Rail {
            number: None,
            icon: None,
        };
    }
    let number = track.index.saturating_add(1).to_string();
    let (text, size) = font.fit(&number, 11.0, 6.0, rail - 3.0);
    let width = font.width(&text, size);
    let middle = y + mark_h + (h - mark_h - group) / 2.0;
    let baseline = if with_icon {
        middle + 9.0
    } else {
        middle + f64::from(size) / 3.0
    };
    let top = middle + 11.0 + ICON_GAP;
    let left = indent + (rail - icon) / 2.0;
    Rail {
        number: Some(Number {
            x: indent + (rail - width) / 2.0,
            text,
            size,
            baseline,
        }),
        icon: with_icon.then(|| Rect::new(left, top, left + icon, top + icon)),
    }
}

// Mute and solo are ONE SIZE on every track, drawn in the live pass —
// see `overlay::panel_controls`. The size is fixed rather than scaled
// to the row because a control that is a different size on every track
// cannot be built on: there is no shared hit target, and nothing else
// can address "the mute column" when the mute column is a different
// shape in every row.
//
// REAPER stacks them, which needs 45 of height; a collapsed track has
// sixteen. Turned a quarter turn they keep their width at any height,
// and the gutter is 47 wide, which is exactly two of them. That costs
// the meter its well in the TCP — the mixer's meter is the one with a
// level behind it.

/// The control row: the name field and everything on it, flattened to
/// whatever height the row has.
///
/// `band` is the vertical space the controls get, and every control in
/// it is drawn at ONE SIZE on every track — centred in the band rather
/// than scaled to it.
///
/// Two earlier versions scaled: first squashed, then uniformly. Both
/// looked reasonable and both were wrong for the same reason. A control
/// that is a different size on every track cannot be built on — there is
/// no shared hit target, no dragging a value across a column of tracks,
/// and nothing else can address "the pan column" because the pan column
/// is a different shape in every row. Consistency across tracks is worth
/// more than filling a tall track's band.
///
/// That is why the shortest settable row is the one that has to hold the
/// controls at their authored size, rather than the controls having to
/// fit whatever a row happens to be — see `Layout::min`.
///
/// The name field is the exception: it is a box, not a control, so it
/// takes the whole band. A field that stayed one height would leave the
/// name floating in a gap on a tall track.
fn row_one(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    indent: f64,
    y: f64,
    band: f64,
    tcp: Tcp,
) {
    /// The height row one is authored at — the name field's.
    const AUTHORED: f64 = 24.0;

    // The name sits on the row's own colour at every height: a dark
    // field behind it was a box the row read fine without. The band is
    // the name's; the controls take their own size, centred in it.
    let field_h = band;
    let control_top = y + (band - AUTHORED) / 2.0;
    let field_top = y;

    // The record arm, on the field's left end. Lit when armed, which is
    // the one control on this row that has to be readable at a glance
    // from across a room — and gone once the row is too short for that
    // to be true, because a five-pixel ring is neither readable nor
    // hittable and the volume and pan indicators are what a collapsed
    // row is being read for.
    // The record arm is live — lit when armed, and that changes on a
    // click. See `overlay::panel_controls`.

    // The name, between the arm and the knob, cut short rather than
    // wrapped or shrunk — REAPER truncates here too. The type shrinks
    // with the row rather than being squashed with it: a flattened glyph
    // is unreadable where a smaller one is merely small.
    let name_x = tcp.name_x() + indent;
    let name_w = (tcp.volume_x() - tcp.name_x() - indent).max(0.0);
    let ink = if track.selected {
        palette.text
    } else {
        palette.text_dim
    };
    let size = tcp.name_size(field_h);
    glyphs(
        scene,
        font,
        ink,
        &font.elide(&track.name, size, name_w),
        name_x,
        f64::from(size).mul_add(0.35, field_top + field_h / 2.0),
        size,
    );

    // Everything to the right of the name — volume, pan, routing, the
    // FX pill and polarity — is drawn in the live pass. Every one of
    // them shows a value that changes while the window is open, so a
    // recorded one is a control that was right when the project opened.
    // What is recorded here is the row's ground, its rail, its colours
    // and its name.
    let _ = (control_top, AUTHORED, band);
}

// Volume and pan are live: their whole job is to show a value, and a
// recorded knob is one frozen at whatever the project opened with. They
// are drawn per frame in `overlay::panel_controls`, in whichever form
// the row's height calls for — a knob while there is a circle big
// enough to read an angle off, a bar below that. Both forms sit in the
// same two columns, so a row can change which it shows without anything
// moving.

/// The name's type size for a field of `height`.
///
/// Shrinks with the row and stops: past the small end it is unreadable,
/// past the large end it is bigger than REAPER sets it.
#[must_use]
fn name_size(height: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "a type size in points, which every text API takes as f32"
    )]
    let size = (height * 0.48).clamp(6.5, 11.5) as f32;
    size
}

/// Envelope, and — only while armed — the input FX slot and the record
/// input combo.
fn row_two(scene: &mut anyrender::Scene, palette: &Palette, font: &Font, track: &Track, y: f64) {
    let two = y + f64::from(g::ROW_TWO);
    let fh = f64::from(g::FIELD_H);
    // The automation button is hidden for now — it comes back with the
    // envelope model behind it, and an automation control that cannot
    // report or change a mode is a button that lies about what it does.
    // The input FX slot and the record-input combo belong to RECORDING,
    // so they appear when the track is armed and not before. On a
    // 2,000-track orchestral template almost nothing is armed, and
    // drawing an input selector reading "None" on every row filled the
    // panel with the one thing none of those tracks are doing.
    if track.armed {
        rect(scene, palette.tcp_field, 56.0, two, 90.0, two + fh);
        text_centered(
            scene,
            font,
            palette.text_faint,
            "FX",
            (56.0, 90.0),
            two + 14.0,
            10.0,
        );
        rect(scene, palette.tcp_combo, 91.0, two, 286.0, two + fh);
        text_centered(
            scene,
            font,
            palette.text_dim,
            &font.elide(&record_input(track), 11.0, 180.0),
            (91.0, 286.0),
            two + 14.0,
            11.0,
        );
        // The combo's caret — the shared triangle, not a glyph.
        caret(scene, palette.text_faint, 91.0 + 181.0, two + 8.0);
    }
}

/// The automation button, where a row has room for it: a field with an
/// envelope's line drawn in it.
pub(crate) fn automation_button(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    y: f64,
    h: f64,
    tcp: Tcp,
) {
    let Some(r) = crate::row::automation_button(y, h, tcp) else {
        return;
    };
    rect(scene, palette.tcp_field, r.x0, r.y0, r.x1, r.y1);
    let mut line = vello::kurbo::BezPath::new();
    let (x0, x1) = (r.x0 + 4.0, r.x1 - 4.0);
    let (lo, hi) = (r.y1 - 6.0, r.y0 + 6.0);
    line.move_to((x0, lo));
    line.line_to(((x1 - x0).mul_add(0.35, x0), hi));
    line.line_to(((x1 - x0).mul_add(0.65, x0), hi));
    line.line_to((x1, lo - 3.0));
    scene.stroke(
        &vello::kurbo::Stroke::new(1.4),
        Affine::IDENTITY,
        palette.text_dim,
        None,
        &line,
    );
}

/// A track's own colour, or the palette's neutral when it has none.
///
/// Shared with the lanes so an item and its row cannot disagree about
/// what colour the track is.
#[must_use]
pub fn track_color(palette: &Palette, track: &Track) -> Color {
    track.color.map_or(palette.text_faint, |rgb| {
        Color::from_rgba8(
            u8::try_from((rgb >> 16) & 0xff).unwrap_or(0),
            u8::try_from((rgb >> 8) & 0xff).unwrap_or(0),
            u8::try_from(rgb & 0xff).unwrap_or(0),
            0xff,
        )
    })
}

/// The colour a folder writes along the bottom of its children.
///
/// The track's own colour, not `row_tint`'s. That one mixes a few per
/// cent of the colour into the panel's grey — right for a strip body,
/// where the colour is a hint behind controls you are reading — and
/// hopeless for a twelve-pixel band whose ENTIRE job is to be
/// identifiable at a glance across half a screen.
///
/// Still short of the raw colour: pulled toward the panel so a row of
/// bands reads as part of the mixer rather than as a stripe of paint
/// across the bottom of it.
pub fn folder_band(palette: &Palette, track: &Track) -> Color {
    /// How far toward the track's own colour the band goes.
    const STRENGTH: f32 = 0.62;
    if track.color.is_none() {
        return palette.tcp_gutter;
    }
    let raw = track_color(palette, track);
    let [br, bg, bb, _] = raw.components;
    let [ar, ag, ab, aa] = palette.tcp_tint.components;
    Color::new([
        (br - ar).mul_add(STRENGTH, ar),
        (bg - ag).mul_add(STRENGTH, ag),
        (bb - ab).mul_add(STRENGTH, ab),
        aa,
    ])
}

/// Ink for a number sitting on `background`.
///
/// Black on anything but a very dark band, which is what was asked for
/// and what the drum template wants: the bands are saturated mid-tones
/// and black on them reads as a number stamped on a colour, where a
/// light ink reads as a second label floating over it.
///
/// Worth writing down, because it is a real trade and not an oversight:
/// **strict WCAG contrast prefers the light ink on three of these four
/// bands.** Linearising the channels properly puts the kick, the toms
/// and the kit itself near 0.07 relative luminance, where black is
/// about 2.4:1 and white about 8.6:1; only the snare's olive is a
/// coin-flip at 4.4 against 4.7.
///
/// What is measured here is therefore sRGB-space brightness — the
/// channels weighted but NOT linearised — which tracks how bright a
/// colour looks better than it tracks how legible it is. That is the
/// right measure for this particular decision and the wrong one for an
/// accessibility claim, so no such claim is made. If these numbers ever
/// need to meet a contrast standard, this function is where that starts
/// and the answer will be the light ink.
///
/// The weights are still the luminance ones rather than a flat average:
/// green carries most of what the eye reads as brightness and blue
/// almost none, so a saturated blue and a saturated yellow average the
/// same and look nothing alike.
///
/// The threshold only has to catch bands dark enough that black would
/// vanish into them — a near-black track colour is the case it exists
/// for, not the mid-tones.
const INK_FLOOR: f32 = 0.179;

/// A track's icon's ink: its own colour, lifted a third of the way to white
/// so it reads bold on the dark field — or the panel's dim ink, for a track
/// with no colour of its own.
#[must_use]
pub fn icon_ink(palette: &Palette, track: &Track) -> Color {
    if track.color.is_none() {
        return palette.text_dim;
    }
    mix(
        track_color(palette, track),
        Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
        0.35,
    )
}

pub fn ink_on(background: Color) -> Color {
    let [r, g, b, _] = background.components;
    let luminance = 0.2126_f32.mul_add(r, 0.7152_f32.mul_add(g, 0.0722 * b));
    if luminance > INK_FLOOR {
        Color::from_rgba8(0, 0, 0, 0xff)
    } else {
        Color::from_rgba8(0xe8, 0xe8, 0xea, 0xff)
    }
}

/// The row's background, tinted toward the track's own colour.
///
/// Shared with the band the replay substitutes at small zooms, so the
/// two cannot disagree about what colour a track is.
///
/// REAPER tints the whole row rather than showing a colour chip, which
/// is what makes a session readable by section at a glance. The strength
/// is the theme's, not a number chosen here.
/// A selected row is the same colour, LIT — the tint mixed further
/// toward the track's own colour, not washed toward white.
///
/// Toward its own colour and not toward a highlight, because the colour
/// is what identifies the track: a selection that greyed or blued it
/// would take away the one thing the tint is for. Lighter and more
/// saturated reads as "this one", and still reads as the same track.
const SELECTED_LIFT: f32 = 0.45;

#[must_use]
pub fn row_tint(palette: &Palette, track: &Track) -> Color {
    let Some(_) = track.color else {
        // A track with no colour of its own still has to be able to
        // show that it is selected, and it has no colour to lift, so
        // it lifts toward the panel's own ink instead.
        return if track.selected {
            mix(palette.tcp_tint, palette.text_faint, SELECTED_LIFT)
        } else {
            palette.tcp_tint
        };
    };
    let strength = if track.selected {
        // Past 1 would be past the track's own colour and into a
        // colour it is not, so the lift is what is left of the way
        // there rather than a constant added on.
        palette
            .track_tint
            .mul_add(1.0 - SELECTED_LIFT, SELECTED_LIFT)
    } else {
        palette.track_tint
    };
    mix(palette.tcp_tint, track_color(palette, track), strength)
}

/// Blend `b` into `a` by `t`.
pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.components;
    let [br, bg, bb, _] = b.components;
    Color::new([
        (br - ar).mul_add(t, ar),
        (bg - ag).mul_add(t, ag),
        (bb - ab).mul_add(t, ab),
        aa,
    ])
}

/// Where the volume control fills to, from a gain.
///
/// This used to be `volume / 2.0` — linear in GAIN, which put unity at
/// the middle and made the position mean nothing: half the travel was
/// the top 6 dB and the bottom half covered everything from −6 dB to
/// silence. A fader you cannot read a level off is a handle.
///
/// It now goes through the measured taper — see
/// `daw_theme_art::paint::tcp::gain_norm` — which is linear in dB, so
/// the dB scale beside it labels the positions it actually uses.
///
/// **Known limit**: that taper tops out at unity, because the labels it
/// was fitted from run −6 to −54 and extrapolating the fit lands 0 dB
/// exactly on the groove's top pixel. REAPER's boost region was not
/// visible in the capture, so a track above unity currently pegs at the
/// top rather than showing how far above. That wants a second
/// measurement with a boosted track rather than a guess at `+12`.
#[must_use]
pub fn volume_fraction(volume: f64) -> f64 {
    daw_theme_art::paint::tcp::gain_norm(volume)
}

/// Mute and solo take their lit colour from the resolved theme, so a
/// REAPER theme's own mute red reaches the canvas rather than the art
/// crate's default.
#[must_use]
pub fn mute_lit(palette: &Palette) -> daw_theme::Color {
    to_theme(palette.mute)
}

#[must_use]
pub fn solo_lit(palette: &Palette) -> daw_theme::Color {
    to_theme(palette.solo)
}

#[must_use]
pub fn to_theme(color: Color) -> daw_theme::Color {
    let [red, green, blue, alpha] = color.to_rgba8().to_u8_array();
    daw_theme::Color {
        r: red,
        g: green,
        b: blue,
        a: alpha,
    }
}

/// What a track records from, as the panel prints it.
///
/// `daw_ui`'s own formatter rather than a `Debug` of the enum: this is a
/// label a person reads off a strip, and "Audio { channel: 0 }" is a
/// dump of a data structure. One formatter, so the mixer and the panel
/// name an input the same way.
#[must_use]
pub fn record_input(track: &Track) -> String {
    daw_ui::controls::record_input_name(track)
}

fn rect(scene: &mut anyrender::Scene, color: Color, x0: f64, y0: f64, x1: f64, y1: f64) {
    scene_fill(scene, color, &Rect::new(x0, y0, x1, y1));
}

fn scene_fill(scene: &mut anyrender::Scene, color: Color, shape: &impl vello::kurbo::Shape) {
    use anyrender::PaintScene as _;
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, shape);
}

/// A knob: its body, and a dot showing where it points.
///
/// A dot rather than a rotated pointer line, because the recorded scene
/// The combo's disclosure triangle.
pub fn caret(scene: &mut anyrender::Scene, color: Color, x: f64, y: f64) {
    use vello::kurbo::BezPath;
    let mut path = BezPath::new();
    path.move_to((x, y));
    path.line_to((x + 7.0, y));
    path.line_to((x + 3.5, y + 4.0));
    path.close_path();
    scene_fill(scene, color, &path);
}

/// A run of text with its baseline at `baseline`.
///
/// Public because `crate::art` draws the labels inside ported control
/// drawings, and those have to come out in the same font as the track
/// names beside them.
pub fn glyphs(
    scene: &mut impl PaintScene,
    font: &Font,
    color: Color,
    body: &str,
    x: f64,
    baseline: f64,
    size: f32,
) {
    if body.is_empty() {
        return;
    }
    let glyphs = font.layout(body, size);
    if glyphs.is_empty() {
        return;
    }
    scene.draw_glyphs(
        font.data(),
        size,
        true,
        &[],
        Vec2::ZERO,
        Fill::NonZero,
        color,
        1.0,
        Affine::translate((x, baseline)),
        None,
        glyphs.into_iter(),
    );
}

/// The same, centred between `left` and `right`.
fn text_centered(
    scene: &mut impl PaintScene,
    font: &Font,
    color: Color,
    body: &str,
    span: (f64, f64),
    baseline: f64,
    size: f32,
) {
    let (left, right) = span;
    let w = font.width(body, size);
    glyphs(
        scene,
        font,
        color,
        body,
        left + (right - left - w) / 2.0,
        baseline,
        size,
    );
}
