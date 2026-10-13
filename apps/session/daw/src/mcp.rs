//! The mixer, as a recorded scene.
//!
//! The same architecture as the arrangement, turned ninety degrees: a
//! scene recorded once in content space and replayed under a transform,
//! with an index that lets a frame skip the strips it cannot see. There
//! the unit is a row and the scroll is vertical; here it is a strip and
//! the scroll is horizontal. Everything else — the shape layer the
//! controls come from, the culling, the level-of-detail — is shared.
//!
//! # The same session, in the other direction
//!
//! A mixer and a track panel are two views of one list, and the thing
//! that makes them hard to keep honest is that the list is a TREE. The
//! panel says so with indentation, which it has room for because it
//! stacks downward. A strip is 86 wide and cannot be indented without
//! becoming narrower than its own controls.
//!
//! So the nesting goes ABOVE the strips instead, as bands: one row per
//! depth, each band spanning the strips beneath it. A folder reads as a
//! bracket over its children rather than as a gap to their left, and the
//! two views can be compared track for track — which is the whole reason
//! to draw the structure at all.
//!
//! # What is not here yet
//!
//! Live levels. The meter draws its well and whatever level it is
//! handed, and nothing hands it one, so every meter is at rest. That is
//! the same gap the panel has and it closes in the same place.

use anyrender::{PaintScene, Scene};
use daw_proto::Track;
use daw_theme_art::geometry::mcp as g;
use daw_ui::controls::{Collapse, PanAnchor};
use daw_ui::studio::{ProjectRef, RowsRef};
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use crate::arrangement::Palette;
use crate::profile::Counts;
use crate::text::Font;

/// REAPER's wide strip, and the width everything in the geometry was
/// measured against.
///
/// Narrow mode's 54 is a separate layout rather than this one scaled, so
/// it is not modelled by shrinking this. What IS modelled by shrinking
/// is `Track::width`: a strip narrower than this keeps its controls in
/// their measured columns and loses the room to the right of them.
pub const STRIP_W: f64 = 86.0;
const _: () = assert!(
    g::STRIP_W.to_bits() == 86.0_f32.to_bits(),
    "mcp::STRIP_W must match the measured geometry"
);
/// The gap between strips, so two adjacent ones read as two.
pub const STRIP_GAP: f64 = 1.0;

/// The widest a touchscreen's strip is (before its zoom): mute and solo
/// side by side across it, the fader under them — thinner than REAPER's
/// 86, so more of a phone's or a tablet's mixer is on screen at once.
pub const TOUCH_STRIP_W: f64 = 62.0;

/// A name with what its folders already say taken off the front.
///
/// `T1 Trig` sits inside `Tom 1`. The `T1` is not information at that
/// point — the strip is standing on a band of the folder's colour that
/// runs under every one of its children — so on a narrow strip it is
/// two of the four characters you have spent saying something the eye
/// has already been told. Dropped, the strip says `Trig`, which is the
/// whole of what distinguishes it from its neighbour.
///
/// Matching is deliberately loose about how a folder writes its name.
/// `Tom 1` and `T1` are the same thing said long and short, so a name
/// is reduced to its letters and digits, and a folder also answers to
/// its initials with the digits kept: `Tom 1` gives `tom1` and `t1`,
/// and `T1 Trig`'s first word matches the second.
///
/// Returns `None` when nothing can safely come off — including when
/// every word would, because a strip labelled with nothing is worse
/// than one labelled redundantly.
fn shorten(name: &str, ancestors: &[&str]) -> Option<String> {
    fn letters(text: &str) -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }
    /// A folder's short form: the first letter of each word, with any
    /// digits kept — `Tom 1` -> `t1`, `Hi-Hat` -> `hh`.
    ///
    /// Split on anything that is not alphanumeric, not just spaces, so
    /// `Hi-Hat` is two words rather than one. And a word that is only
    /// digits contributes its digits and no initial — otherwise the `1`
    /// of `Tom 1` counts twice and the key comes out `t11`, matching
    /// nothing.
    fn initials(text: &str) -> String {
        let mut out = String::new();
        for word in text.split(|c: char| !c.is_alphanumeric()) {
            if let Some(first) = word.chars().next() {
                if first.is_alphabetic() {
                    out.extend(first.to_lowercase());
                }
            }
            out.extend(word.chars().filter(char::is_ascii_digit));
        }
        out
    }

    let keys: Vec<String> = ancestors
        .iter()
        .flat_map(|a| [letters(a), initials(a)])
        .filter(|k| !k.is_empty())
        .collect();

    // The longest leading run of words that matches a folder, not just
    // the first word: `Tom 1 Trig` has to lose both `Tom` and `1`, and
    // taking one word at a time stops at `Tom`, which matches nothing
    // on its own.
    let mut words: Vec<&str> = name.split_whitespace().collect();
    loop {
        let Some(take) = (1..words.len()).rev().find(|take| {
            let head = letters(&words[..*take].concat());
            !head.is_empty() && keys.iter().any(|k| *k == head)
        }) else {
            break;
        };
        words.drain(..take);
    }
    let short = words.join(" ");
    (short.len() < name.len()).then_some(short)
}

/// How much shorter each level of nesting makes a strip.
///
/// Folder depth reads off the BOTTOM of the mixer: strips share a top
/// edge and their bottoms step up with depth, so a folder's children sit
/// visibly inside it. Brackets across the top said the same thing and
/// were removed — that band is wanted for something else, and a
/// staircase costs no height that the strips were using.
pub const INDENT_STEP: f64 = 12.0;

/// How tall the mixer panel opens by default.
///
/// REAPER's own default, and a *default* is all it is: the panel is
/// draggable and a strip fills whatever height it is given. There is no
/// fixed REAPER strip height to match.
///
/// This was briefly 310, measured off a running REAPER — wrongly. That
/// REAPER had "show multiple rows of tracks" ON, so its mixer had
/// wrapped the session into two rows, and what I measured was the ROW
/// PITCH of a wrapped layout, not the height of a strip. With the
/// option off there is one row and each strip is as tall as the panel.
///
/// The width measured the same way IS right — 86, confirmed twice — and
/// that is the difference worth remembering: a strip's width is a real
/// constant, its height is whatever the dock gives it.
pub const DEFAULT_HEIGHT: f64 = 371.0;

/// What fits in a strip of a given width.
///
/// The width counterpart of [`Collapse`], which does the same job for a
/// strip's height. REAPER needs no such thing because every strip there
/// is one width; ours are not, so the measured columns — the button
/// column at 55, the fader at 30 — stop being reachable long before the
/// strip stops being useful.
///
/// The rule is the same one the track panel follows: shed controls in
/// the order they stop earning their space, and never let one overflow
/// its strip. What a narrow strip keeps is what you look at a send or a
/// trigger for — its level, its mute, and which track it is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Squeeze {
    /// Everything REAPER's strip has, at the columns it measures them
    /// to. The button column sits at 55 and the routing widget after it,
    /// so this is the only width where they are reachable.
    Full,
    /// The head of the strip — its FX pill and its pan — with the meter
    /// beside the fader. Routing goes first, because it is the control
    /// you set once and then read off the arrangement.
    Head,
    /// The fader down the middle, mute and solo stacked over it, and the
    /// name. What you look at a send or a trigger for: its level, its
    /// mute, and which track it is.
    Level,
}

impl Squeeze {
    /// What fits in `width`.
    ///
    /// Thresholds are what each tier NEEDS: the button column plus its
    /// own 21 for `Full`, the meter's 26 and the fader's 22 with their
    /// gaps for `Head`.
    #[must_use]
    pub fn at(width: f64) -> Self {
        if width >= 78.0 {
            Self::Full
        } else if width >= 60.0 {
            Self::Head
        } else {
            Self::Level
        }
    }

    /// The pan knob and the FX pill, which share a threshold: both want
    /// a cell and enough either side to read as placed rather than
    /// wedged.
    #[must_use]
    pub fn head(self) -> bool {
        self <= Self::Head
    }

    /// The meter, which shares the stretch with the fader.
    #[must_use]
    pub fn meter(self) -> bool {
        self <= Self::Head
    }

    /// Routing, and the measured button column it sits in.
    #[must_use]
    pub fn columns(self) -> bool {
        self == Self::Full
    }
}

/// The mixer, recorded.
pub struct Mixer {
    /// The strips, at x = index * [`PITCH`].
    strips: Scene,
    /// Command range per strip, so a frame draws only what it can see.
    index: Vec<std::ops::Range<u32>>,
    /// The left edge of every strip, plus the right edge of the last —
    /// so strip `i` occupies `offsets[i]..offsets[i + 1]`.
    ///
    /// Cumulative because strips are no longer one width. What "which
    /// strip is at this x" costs went from a division to a binary
    /// search, which is the same trade the arrangement made when rows
    /// stopped being one height.
    offsets: Vec<f64>,
    /// How many strips there are.
    pub count: usize,
    /// How deep the nesting goes.
    pub depth: usize,
    /// The height a strip was recorded at.
    pub height: f64,
    /// The y every strip puts its button column at.
    ///
    /// Exposed because the LIVE controls have to land on the same line
    /// the recorded chrome was built around. Recomputing it in the
    /// overlay is how the two ended up disagreeing: the recorded strips
    /// share one line and a per-strip recomputation gives each its own.
    pub buttons_top: f64,
    /// How much of a strip the rack took.
    pub rack_h: f64,
    /// Live-mode strips — see `strip::shape`.
    pub live: bool,
    /// Touchscreen strips — see `strip::Strip::touched`.
    pub touch: bool,
    /// The mode the strips were laid out for — see `Settings::mixing`.
    pub mixing: bool,
    pub recording: bool,
    /// Each strip's own height.
    ///
    /// NOT the mixer's: nesting shortens a strip from the bottom, so a
    /// track three levels deep is shorter than the panel by three
    /// steps. `Collapse` resolves a strip's sections against the height
    /// it is DRAWN at, so handing the overlay the panel height instead
    /// of the strip's gave it different bands from the recorded chrome
    /// — which moved the arm, the buttons and the fader down by
    /// whatever the indent came to.
    heights: Vec<f64>,
    /// Whether each strip's track wants the column layout when it is
    /// focused — decided from its chain, and carried here so every
    /// reading of a strip's geometry (the recording, the live pass,
    /// the hit tests) resolves the same `Strip`.
    columns: Vec<bool>,
    /// Each strip's track name, already fitted to its plate.
    ///
    /// Fitted HERE and drawn in the overlay, which is the split the
    /// rest of this window uses: choosing a label costs a measured
    /// shrink and an ancestor-name search, and that answer only changes
    /// when the strips do — while the INK on it changes with mute, solo
    /// and selection, which change constantly. So the expensive,
    /// constant part is recorded and the cheap, varying part is not.
    labels: Vec<(String, f32)>,
    /// The theme the strips were recorded with, if any — see `skin`. The
    /// live pass and the hit tests read the same one, so a theme that
    /// reloads mid-frame cannot draw one strip two ways.
    pub skin: Option<std::sync::Arc<theme_runtime::Theme>>,
    /// `skin::version()` when recorded: a different one is a stale mixer.
    pub skin_version: u64,
    /// Each strip not recorded yet, and what recording it needs — see
    /// [`Mixer::record`]. `None` once recorded.
    jobs: Vec<Option<Job>>,
    /// The rack, whether it is drawn live, and the settings the strips
    /// were laid out with: what a strip recorded later is recorded with.
    rack: Vec<crate::tone::Which>,
    live_racks: bool,
    settings: crate::settings::Settings,
    /// Which strips the theme drew. One it cannot draw yet — a live or
    /// touch strip, a strip with its rack beside it — is the painters'.
    themed: Vec<bool>,
}

impl Mixer {
    /// A strip's fitted track name, and the size it fits at.
    ///
    /// Chosen when the mixer was recorded, drawn every frame — see
    /// `labels` for why the two are split.
    #[must_use]
    pub fn label(&self, row: usize) -> Option<(&str, f32)> {
        self.labels
            .get(row)
            .map(|(text, size)| (text.as_str(), *size))
    }

    /// Record the whole mixer, `height` being the height of its REAPER
    /// part.
    ///
    /// The brackets take their share off the top and the strips get the
    /// rest. Depth is measured before anything is recorded, because a
    /// strip resolves its sections against the height it is DRAWN at —
    /// recording at a guessed height and then discovering the nesting
    /// would collapse the wrong sections.
    #[must_use]
    pub fn build(
        palette: &Palette,
        font: &Font,
        project: &ProjectRef,
        rows: &RowsRef,
        height: f64,
        layout: crate::layout::Layout,
        rack: &[crate::tone::Which],
        // Whether the racks will be drawn LIVE over this recording.
        //
        // A rack with a spectrum in it moves, and the live pass draws
        // the whole panel — over an identical one from the recording,
        // which is then pure overdraw. Told in advance, the recording
        // reserves the rack's HEIGHT and draws nothing in it, so the
        // strip's layout is unchanged and the commands are paid for
        // once.
        live_racks: bool,
        settings: crate::settings::Settings,
        tone_settings: &crate::tone::Store,
    ) -> Self {
        let mut mixer = Self::lay_out(
            palette,
            font,
            project,
            rows,
            height,
            layout,
            rack,
            live_racks,
            settings,
            tone_settings,
        );
        mixer.record(0..mixer.count, palette, font, tone_settings, None);
        mixer
    }

    /// [`Mixer::build`] without the recording: every strip laid out, none
    /// recorded until [`Mixer::record`] asks for it. A session of two
    /// hundred strips shows fifteen; recording all of them held the
    /// mixer's opening frame ~150 ms for strips nobody saw.
    #[must_use]
    pub fn lay_out(
        palette: &Palette,
        font: &Font,
        project: &ProjectRef,
        rows: &RowsRef,
        height: f64,
        layout: crate::layout::Layout,
        rack: &[crate::tone::Which],
        live_racks: bool,
        settings: crate::settings::Settings,
        tone_settings: &crate::tone::Store,
    ) -> Self {
        let depth_seen = rows
            .iter()
            .map(|(_, depth)| usize::try_from(*depth).unwrap_or(0))
            .max()
            .map_or(0, |deepest| deepest.saturating_add(1));

        // The panel splits into the REAPER strip and the rack above it.
        //
        // `height` is the WHOLE panel, and the strip takes about a third
        // of it off the bottom — which is what REAPER's own mixer looks
        // like on a 1440p screen, and leaves the other two thirds for
        // the processing.
        //
        // This used to be the other way round: the strip was the height
        // and the rack was a 220-pixel band added on top. That reads as
        // a channel strip with a stripe of graphs stuck to it, when the
        // thing being built is a channel you MIX on — where the
        // processing is most of what you are looking at and the fader
        // is the part you reach for after deciding.
        //
        // A rack nobody is wide enough to draw is height nobody should
        // pay for. Two thirds of the panel reserved for graphs that
        // every strip is too narrow to show is what the Overview preset
        // looked like before this: a wall of nothing over a row of
        // faders pushed to the floor.
        //
        // Asked of the widths the rows CARRY rather than of the widths
        // that come back from `widths` below, because that one borrows
        // between strips to open a selection — and a selection wide
        // enough for a rack is a reason to have one, not a reason to
        // have already decided.
        let any_rack = rows
            .iter()
            .any(|(track, _)| layout.width_of(track.width) >= crate::tone::LEGIBLE);
        let tone = !rack.is_empty() && any_rack;
        // The strip keeps about a third off the bottom and the rack
        // gets the other two — which is what REAPER's own mixer looks
        // like on a 1440p screen, and what this panel was measured
        // against.
        //
        // The rack takes its whole share, whatever is currently in it.
        //
        // Three processors do not fill it, and the empty space below
        // them is deliberate: it is the room for the rest of the chain,
        // which is coming. Sizing the box to today's contents would
        // mean every processor added moved the fader down — and a strip
        // whose controls sit at a different height because of what is
        // plugged into it cannot be scanned across, which is the rule
        // the whole panel is built on.
        //
        // Once the chain outgrows the share it scrolls inside it rather
        // than pushing the strip's controls off the bottom. That is the
        // same rule, arriving from the other side.
        let control = if tone {
            (height * CONTROL_SHARE).max(CONTROL_MIN).min(height)
        } else {
            height
        };
        let rack_h = height - control;

        // One section layout for the whole mixer, resolved against the
        // height LEFT OVER — not against each strip's own.
        //
        // Strips are different heights (folder depth shortens them) and
        // different widths, and resolving sections per strip put the
        // mute of one track at a different y from the mute of the next.
        // A mixer is read by scanning ACROSS it: "which of these is
        // muted" has to be answerable with one horizontal look, and that
        // needs the button column on one line. The fader gives way
        // instead — it is shorter on a shortened strip, which is the
        // cost of the indent rather than a second inconsistency.
        let live = settings.live_strips;
        let touch = settings.touch_strips;
        let shared = crate::strip::shape_for(height - rack_h, live, touch);
        let buttons_top = rack_h
            + crate::strip::fx_section(live, touch)
            + f64::from(shared.pan_band)
            + f64::from(shared.input_band)
            + 4.0;

        let strips = Scene::new();
        let mut index = Vec::with_capacity(rows.len());
        let mut offsets = Vec::with_capacity(rows.len().saturating_add(1));
        // Every strip's width, resolved together — an opened strip
        // borrows from the others rather than adding to the total.
        // A selected strip opens to the focus width — see `widths`.
        // Bounded by what the others can lend, so it reaches it on a
        // wide mixer and falls short gracefully on a crowded one.
        let widths = widths(rows, layout, tone, settings.focus_width(height), touch);

        // The tint of the folder open at each depth, so a strip can
        // draw the colours of everything it sits inside.
        let mut lineage: Vec<Color> = Vec::new();
        let mut lineage_names: Vec<&str> = Vec::new();

        let mut heights = Vec::with_capacity(rows.len());
        let mut columns = Vec::with_capacity(rows.len());
        let mut labels = Vec::with_capacity(rows.len());

        crate::skin::from_env();
        let skin_version = crate::skin::version();
        let skin = crate::skin::current();

        let mut themed_rows = Vec::with_capacity(rows.len());
        let mut jobs: Vec<Option<Job>> = Vec::with_capacity(rows.len());
        let mut x = 0.0_f64;
        for (ordinal, (track, depth)) in rows.iter().enumerate() {
            let depth = usize::try_from(*depth).unwrap_or(0);
            // Everything this track is inside, outermost first. A folder
            // closing simply means the next track is shallower, so the
            // truncate IS the close — no bookkeeping of ends needed.
            lineage.truncate(depth);
            lineage_names.truncate(depth);
            let ancestors = lineage.clone();
            let ancestor_names = lineage_names.clone();
            lineage.push(crate::tcp::folder_band(palette, track));
            lineage_names.push(track.name.as_str());
            let from = u32::try_from(strips.commands.len()).unwrap_or(u32::MAX);
            offsets.push(x);
            let w = widths.get(ordinal).copied().unwrap_or(layout.strip);
            // Nesting shortens the strip from the BOTTOM, so the tops
            // stay level and the bottoms staircase.
            let strip_h = (height - crate::num::coord(depth) * INDENT_STEP).max(1.0);
            heights.push(strip_h);
            let column = tone_settings
                .get(&track.guid)
                .is_some_and(crate::tone::Tone::wants_column);
            columns.push(column);
            labels.push(fit_name(font, track, w, &ancestor_names));
            // A folder with nothing under it in the rows is folded: the
            // rows are what the folder state left, so the next row not
            // being deeper is the fold.
            let collapsed = track.is_folder
                && rows
                    .get(ordinal.saturating_add(1))
                    .is_none_or(|(_, next)| usize::try_from(*next).unwrap_or(0) <= depth);
            let slot = Slot {
                x,
                width: w,
                height: strip_h,
                buttons_top,
                rack_h,
                mixer_h: height,
                column,
                collapsed,
                live,
                touch,
            };
            // A track with no settings yet gets none drawn rather than
            // someone else's — `Store::seed` runs before this, so the
            // miss is a track that arrived between the two. Nor does one
            // whose rack the live pass will draw.
            // The theme draws the stacked desktop strip; the rest is still
            // the painters'.
            let stacked = crate::strip::Strip::laid_out(
                w,
                strip_h,
                height,
                rack_h,
                buttons_top,
                column,
                live,
            )
            .layout()
                == crate::strip::Layout::Stacked;
            // A live or a touch strip is a different strip; a theme draws
            // one only when it says it does (`$strip.live`, `$strip.touch`)
            // — REAPER's layout has neither.
            let says = |theme: &theme_runtime::Theme, token: &str| {
                theme
                    .token(token)
                    .and_then(theme_runtime::Value::truthy)
                    .unwrap_or(false)
            };
            let by_theme = skin.as_ref().is_some_and(|t| {
                stacked && (!live || says(t, "strip.live")) && (!touch || says(t, "strip.touch"))
            });
            themed_rows.push(by_theme);
            let job = Job {
                track: track.clone(),
                slot,
                ancestors,
                depth,
                collapsed,
            };
            jobs.push(Some(job));
            index.push(from..u32::try_from(strips.commands.len()).unwrap_or(u32::MAX));
            x += w + STRIP_GAP;
        }
        offsets.push(x);
        let _ = project;

        Self {
            strips,
            index,
            offsets,
            count: rows.len(),
            depth: depth_seen,
            height,
            buttons_top,
            rack_h,
            live,
            touch,
            heights,
            columns,
            labels,
            skin,
            skin_version,
            themed: themed_rows,
            mixing: settings.mixing,
            recording: settings.recording,
            jobs,
            rack: rack.to_vec(),
            live_racks,
            settings,
        }
    }

    /// Record the strips in `rows` that are not recorded yet — those
    /// [`Mixer::lay_out`] left until they were drawn — left to
    /// right, until `until` (if given) passes. `true` when some in `rows`
    /// are still to do: the host draws again. Cheap when they all are.
    pub fn record(
        &mut self,
        rows: std::ops::Range<usize>,
        palette: &Palette,
        font: &Font,
        tone_settings: &crate::tone::Store,
        until: Option<web_time::Instant>,
    ) -> bool {
        let end = rows.end;
        for row in rows {
            // Out of time: the ones before this are done, so what is left
            // is this one on.
            if until.is_some_and(|at| web_time::Instant::now() >= at) {
                return (row..end).any(|r| self.jobs.get(r).is_some_and(Option::is_some));
            }
            let Some(job) = self.jobs.get_mut(row).and_then(Option::take) else {
                continue;
            };
            let by_theme = self.themed.get(row).copied().unwrap_or(false);
            let ctx = Record {
                palette,
                font,
                skin: self.skin.as_deref().filter(|_| by_theme),
                rack: &self.rack,
                tone: (!self.live_racks).then_some(tone_settings),
                height: self.height,
                rack_h: self.rack_h,
                settings: self.settings,
            };
            let label = self.labels.get(row).map(|(l, size)| (l.as_str(), *size));
            let from = u32::try_from(self.strips.commands.len()).unwrap_or(u32::MAX);
            ctx.strip(&mut self.strips, &job, label);
            let to = u32::try_from(self.strips.commands.len()).unwrap_or(u32::MAX);
            if let Some(span) = self.index.get_mut(row) {
                *span = from..to;
            }
        }
        false
    }

    /// The theme `row` was drawn with, if it was.
    #[must_use]
    pub fn themed(&self, row: usize) -> Option<&std::sync::Arc<theme_runtime::Theme>> {
        self.skin
            .as_ref()
            .filter(|_| self.themed.get(row).copied().unwrap_or(false))
    }

    /// What a theme is told about `row`'s strip beyond its track: one
    /// answer for the recording, the live pass and the hit tests.
    #[must_use]
    pub fn shown<'a>(&'a self, row: usize, font: &'a Font) -> crate::skin::Shown<'a> {
        crate::skin::Shown {
            label: self.label(row),
            depth: self.depth_of(row),
            width: self.strip_box(row).map_or(STRIP_W, |(_, w, _)| w),
            mixer_h: self.height - self.rack_h,
            rack: self.rack_h > 0.0,
            font: Some(font),
            mixing: self.mixing,
            recording: self.recording,
            ..crate::skin::Shown::default()
        }
    }

    /// How many folders `row` sits inside: its strip is shortened by one
    /// step for each.
    #[must_use]
    pub fn depth_of(&self, row: usize) -> usize {
        self.heights.get(row).map_or(0, |h| {
            let steps = ((self.height - h) / INDENT_STEP).round().max(0.0);
            // A count of steps, a handful at most.
            usize::try_from(steps as u64).unwrap_or(0)
        })
    }

    /// One strip's geometry — the `Strip` everything reads.
    #[must_use]
    pub fn strip(&self, row: usize) -> Option<crate::strip::Strip> {
        let (_, width, height) = self.strip_box(row)?;
        Some(
            crate::strip::Strip::laid_out(
                width,
                height,
                self.height,
                self.rack_h,
                self.buttons_top,
                self.columns.get(row).copied().unwrap_or(false),
                self.live,
            )
            .touched(self.touch),
        )
    }

    /// How wide the whole mixer is, in content pixels.
    #[must_use]
    pub fn content_width(&self) -> f64 {
        self.offsets.last().copied().unwrap_or(0.0)
    }

    /// Where a strip is, and how big — for drawing over it.
    ///
    /// The overlay needs this because the strips are RECORDED: a
    /// hovered control cannot be re-recorded without re-recording the
    /// mixer, so it is redrawn live on top, and drawing on top of
    /// something means knowing where it is.
    ///
    /// Returns the strip's left edge in content space, its width, and
    /// its height — the same three numbers `build` handed `strip`.
    #[must_use]
    pub fn strip_box(&self, row: usize) -> Option<(f64, f64, f64)> {
        let left = *self.offsets.get(row)?;
        let right = *self.offsets.get(row.checked_add(1)?)?;
        let width = (right - left - STRIP_GAP).max(0.0);
        Some((left, width, *self.heights.get(row)?))
    }

    /// Which strip is at a content x, if any.
    ///
    /// The binary search `visible` uses, over the same offsets, for the
    /// same reason: the strip you click is the strip you see.
    #[must_use]
    pub fn strip_at(&self, content_x: f64) -> Option<usize> {
        if content_x < 0.0 || self.count == 0 {
            return None;
        }
        let strip = self
            .offsets
            .partition_point(|&x| x <= content_x)
            .checked_sub(1)?;
        (strip < self.count).then_some(strip)
    }

    /// The strips that intersect a viewport `width` wide, scrolled to
    /// `scroll_x`.
    ///
    /// A binary search over [`Mixer::offsets`], because strips are no
    /// longer one width. One strip of bleed either side, so a strip half
    /// off the edge still paints its visible half.
    #[must_use]
    pub fn visible(&self, scroll_x: f64, width: f64) -> std::ops::Range<usize> {
        if self.count == 0 {
            return 0..0;
        }
        // `partition_point` gives the first strip whose LEFT edge is
        // past the boundary; the one before it is the one the edge falls
        // inside.
        let first = self
            .offsets
            .partition_point(|&x| x <= scroll_x)
            .saturating_sub(1);
        let last = self.offsets.partition_point(|&x| x < scroll_x + width);
        first.min(self.count)..last.min(self.count)
    }

    /// Replay the visible strips under `transform`.
    pub fn replay(
        &self,
        painter: &mut impl PaintScene,
        scroll_x: f64,
        width: f64,
        transform: Affine,
    ) -> Counts {
        let mut counts = Counts::default();
        for i in self.visible(scroll_x, width) {
            let Some(span) = self.index.get(i) else {
                continue;
            };
            let start = usize::try_from(span.start).unwrap_or(usize::MAX);
            let end = usize::try_from(span.end).unwrap_or(usize::MAX);
            let Some(cmds) = self
                .strips
                .commands
                .get(start..end.min(self.strips.commands.len()))
            else {
                continue;
            };
            for cmd in cmds {
                counts.replayed = counts.replayed.saturating_add(1);
                if crate::arrangement::submit_command(painter, cmd, transform) {
                    counts.submitted = counts.submitted.saturating_add(1);
                }
            }
        }
        counts
    }
}

/// One channel strip drawn by a theme: the rack and the folder steps
/// as `strip` draws them, the REAPER part below the rack the theme's.
fn themed(
    scene: &mut Scene,
    theme: &theme_runtime::Theme,
    palette: &Palette,
    font: &Font,
    track: &Track,
    slot: Slot,
    (rack, tone): (&[crate::tone::Which], Option<&crate::tone::Tone>),
    (ancestors, shown): (&[Color], crate::skin::Shown<'_>),
) {
    let geometry = crate::strip::Strip::laid_out(
        slot.width,
        slot.height,
        slot.mixer_h,
        slot.rack_h,
        slot.buttons_top,
        slot.column,
        slot.live,
    )
    .touched(slot.touch);
    // Over the rack, what `strip` draws there: the ground, the edge, the
    // selection rule (the theme draws its own when there is no rack) —
    // the theme's `mcp.rackback`, when it has one.
    let fields = crate::skin::fields(palette, track, shown);
    if slot.rack_h > 0.0 && theme.has_role(crate::skin::RACKBACK) {
        theme.record(
            crate::skin::RACKBACK,
            (slot.width, slot.rack_h),
            &fields,
            &mut crate::skin::canvas::Host { scene, font },
            Affine::translate((slot.x, 0.0)),
        );
    } else if slot.rack_h > 0.0 {
        fill(
            scene,
            strip_ground(palette, track),
            Rect::new(slot.x, 0.0, slot.x + slot.width, slot.rack_h),
        );
        if let Some(edge) = strip_edge(palette, track) {
            fill(
                scene,
                edge,
                Rect::new(slot.x, 0.0, slot.x + STRIP_EDGE, slot.rack_h),
            );
        }
        if track.selected {
            fill(
                scene,
                palette.accent,
                Rect::new(slot.x, 0.0, slot.x + slot.width, SELECTED_RULE),
            );
        }
    }
    record_rack(
        scene,
        palette,
        font,
        track,
        geometry.rack_rect().map(|r| (r, slot.x)),
        rack,
        tone,
    );
    // The folders this strip sits inside, as steps under it — see `strip`.
    let full = slot.height + crate::num::coord(ancestors.len()) * INDENT_STEP;
    for (level, tint) in ancestors.iter().enumerate() {
        let top = full - crate::num::coord(level + 1) * INDENT_STEP;
        if theme.has_role(crate::skin::STEP) {
            theme.record(
                crate::skin::STEP,
                (slot.width, INDENT_STEP),
                &crate::skin::step_fields(*tint, level + 1),
                &mut crate::skin::canvas::Host { scene, font },
                Affine::translate((slot.x, top)),
            );
        } else {
            fill(
                scene,
                *tint,
                Rect::new(slot.x, top, slot.x + slot.width, top + INDENT_STEP),
            );
        }
    }
    theme.record(
        crate::skin::STRIP,
        (slot.width, slot.height - slot.rack_h),
        &fields,
        &mut crate::skin::canvas::Host { scene, font },
        Affine::translate((slot.x, slot.rack_h)),
    );
}

/// One channel strip, at `x`.
///
/// The section heights are [`Collapse`]'s, which resolves them against
/// REAPER's own thresholds — the same model the DOM mixer uses, so the
/// two collapse at the same heights rather than at two sets of numbers
/// that happen to agree today.
/// One strip, laid out and not yet recorded.
struct Job {
    track: Track,
    slot: Slot,
    ancestors: Vec<Color>,
    depth: usize,
    collapsed: bool,
}

/// What recording a strip takes beyond the strip itself.
struct Record<'a> {
    palette: &'a Palette,
    font: &'a Font,
    /// The theme, when it draws this strip.
    skin: Option<&'a theme_runtime::Theme>,
    rack: &'a [crate::tone::Which],
    /// The racks' settings, when the recording draws the racks (rather
    /// than the live pass).
    tone: Option<&'a crate::tone::Store>,
    height: f64,
    rack_h: f64,
    settings: crate::settings::Settings,
}

impl Record<'_> {
    /// Record `job`'s strip into `scene`, its label `label`.
    fn strip(&self, scene: &mut Scene, job: &Job, label: Option<(&str, f32)>) {
        // A track with no settings yet gets none drawn rather than
        // someone else's — `Store::seed` runs before this, so the miss
        // is a track that arrived between the two. Nor does one whose
        // rack the live pass will draw.
        let tone = self.tone.and_then(|store| store.get(&job.track.guid));
        match self.skin {
            Some(theme) => {
                let shown = crate::skin::Shown {
                    label,
                    depth: job.depth,
                    collapsed: job.collapsed,
                    width: job.slot.width,
                    mixer_h: self.height - self.rack_h,
                    rack: self.rack_h > 0.0,
                    font: Some(self.font),
                    mixing: self.settings.mixing,
                    recording: self.settings.recording,
                    ..crate::skin::Shown::default()
                };
                themed(
                    scene,
                    theme,
                    self.palette,
                    self.font,
                    &job.track,
                    job.slot,
                    (self.rack, tone),
                    (&job.ancestors, shown),
                );
            }
            None => strip(
                scene,
                self.palette,
                self.font,
                &job.track,
                job.slot,
                self.rack,
                tone,
                &job.ancestors,
            ),
        }
    }
}

/// Where one strip sits, and the mixer's shared button line.
#[derive(Clone, Copy)]
struct Slot {
    x: f64,
    width: f64,
    height: f64,
    /// The y every strip puts its button column at — see `Mixer::build`.
    buttons_top: f64,
    /// How much of the top is the Tone rack's; zero when it is off.
    rack_h: f64,
    /// The mixer's own height, which the top sections resolve against.
    mixer_h: f64,
    /// Whether the track wants the column layout when focused.
    column: bool,
    /// Whether the track is a folder whose rows are folded away.
    collapsed: bool,
    /// A live-mode strip — see `strip::shape`.
    live: bool,
    /// A touchscreen's strip: no FX row — see `strip::fx_section`.
    touch: bool,
}

/// How much of the panel the REAPER strip keeps, with the rack on.
///
/// Two fifths, off the bottom. The rack gets the rest to lay its
/// panels out in, and leaves whatever it does not need — that space
/// belongs to the processors the other phases bring, and holding it
/// open is what keeps the fader still when the phase changes. It was
/// a third; the rack scrolls, the fader does not, and a tall window
/// was spending its height on the part that had somewhere else to go.
const CONTROL_SHARE: f64 = 0.40;

/// The least the strip may be squeezed to.
///
/// Below this the fader has no travel and the buttons crowd, and a
/// channel strip you cannot mix on is not improved by the graphs above
/// it. A short panel gives the rack whatever is left over instead.
const CONTROL_MIN: f64 = 240.0;

/// A control on a strip.
///
/// What a click can land on, as opposed to what it can land IN — the
/// strip is `hit::Target::Track`, and this narrows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    Fx,
    Pan,
    RecArm,
    Mute,
    Solo,
    Routing,
    /// The fader, including its cap. Dragged, not clicked.
    Volume,
    /// The name plate. Double-clicked to rename.
    Name,
    /// Input monitoring, under the record arm.
    ///
    /// Its own control rather than part of the arm: what a track
    /// records and whether you hear it while it does are two decisions,
    /// and REAPER gives them two buttons for that reason.
    Monitor,
    /// The clip latch at the top of the meter — clicked to clear it.
    ///
    /// Its rect is always here, because geometry is worked out without
    /// knowing what the signal did. Whether it is actually a target is
    /// the window's call: with nothing clipped there is nothing to
    /// clear, and the band falls through to the fader underneath it so
    /// the strip behaves exactly as it would without a latch.
    Clip,
    /// The fold at the foot of a folder's strip — clicked to collapse
    /// the folder to one strip, or open it again. Only a target on a
    /// folder; the rect is there on every strip because geometry is
    /// worked out without knowing what the track is.
    Folder,
}

impl Control {
    /// Whether this control is set by DRAGGING rather than clicking.
    ///
    /// The distinction the gesture layer needs: a press on a knob is
    /// the start of a drag and must not act until the pointer moves or
    /// is released, where a press on a mute is a click waiting to
    /// happen.
    #[must_use]
    pub const fn is_continuous(self) -> bool {
        matches!(self, Self::Volume | Self::Pan)
    }
}

/// Which control is at a point inside a strip.
///
/// `x` and `y` are relative to the STRIP, not the window — the caller
/// has already worked out which strip, and passing window coordinates
/// here would mean this function needed the scroll too.
///
/// Laid out from the same `Columns` and the same `Collapse` the strip
/// was drawn from, so a control is hit where it is drawn. That is the
/// property worth having: these are two readings of one layout, not two
/// layouts that have to be kept in step.
#[must_use]
pub fn control_at(mixer: &Mixer, row: usize, x: f64, y: f64) -> Option<Control> {
    mixer.strip(row)?.control_at(x, y)
}

/// Which control of `row`'s strip is at a content point (`x` from the
/// strip's left, `y` from the mixer's top), `track` being what it shows.
///
/// A strip a theme drew is hit where the theme drew it (below the rack,
/// which is still the painters'); the rest by the strip's own geometry.
/// One answer for every view that shows a strip — the mixer and the FX.
#[must_use]
pub fn control_in(
    mixer: &Mixer,
    row: usize,
    (palette, font): (&Palette, &Font),
    track: &Track,
    (x, y): (f64, f64),
) -> Option<Control> {
    let (_, width, height) = mixer.strip_box(row)?;
    match mixer.themed(row) {
        Some(theme) if y >= mixer.rack_h => {
            let fields = crate::skin::fields(palette, track, mixer.shown(row, font));
            theme
                .hit(
                    crate::skin::STRIP,
                    (width, height - mixer.rack_h),
                    &fields,
                    vello::kurbo::Point::new(x, y - mixer.rack_h),
                )
                .and_then(|name| crate::skin::control(&name))
        }
        _ => control_at(mixer, row, x, y),
    }
}

/// How far a strip may be lent down.
///
/// Not to the absolute floor: to the bottom of the TIER it is already
/// in, whichever tier that is. A strip drawing a full rack lends only
/// down to the width that still draws one; a strip drawing curves lends
/// only down to the width that still draws those.
///
/// Without this, opening a strip switched off every OTHER strip's rack.
/// Each lender gave up a handful of pixels, which was enough to carry a
/// strip across a threshold, so one click on a mic blanked the
/// processing on the whole kit. Lending has to be invisible, and a
/// control disappearing is the least invisible thing a layout can do.
///
/// Asking the tier for its own bound rather than naming the thresholds
/// here is not tidiness: the first version of this guarded `LEGIBLE`
/// only, and a strip one tier down walked straight through `SHAPE`
/// instead.
fn lending_floor(width: f64, layout: crate::layout::Layout) -> f64 {
    crate::tone::Rack::at(width)
        .floor()
        .unwrap_or(layout.strip_min)
        .max(layout.strip_min)
}

/// Every strip's width, with the selected ones opened.
///
/// The mics of a piece — a kick's In and Out, a snare's Top and Bottom
/// — are stored narrow, because the tone processing lives on the sum of
/// them and spending a rack's width on each mic would push the kit off
/// the screen. But they are not tracks you never touch: balancing the
/// In against the Out is the mixing move at that level, and sometimes
/// the move is to EQ one of them. So selection is the zoom.
///
/// # An opened strip BORROWS its width
///
/// The extra width does not come from nowhere — it is taken off the
/// other strips, in proportion to how much each has to spare. **The
/// mixer's total width is unchanged by opening a strip.**
///
/// That is the property worth having. The alternative is to let the
/// mixer grow and reserve enough screen for the worst case, which
/// means laying the session out smaller than it needs to be all the
/// time to pay for a zoom you use occasionally — and it still breaks
/// the moment the session is one track bigger than the reserve
/// assumed. Borrowing costs the other strips about four pixels each
/// across a drum kit, which is invisible, and it cannot overflow
/// because there is nothing to overflow WITH.
///
/// Each strip lends in proportion to its headroom above the floor, so a
/// strip already at its minimum lends nothing and no strip is pushed
/// below what its controls need. If the others cannot cover the whole
/// ask, the opened strip gets what there was: the rack then opens to
/// whatever tier that width supports, which is the same graceful path
/// every other width takes.
fn widths(
    rows: &RowsRef,
    layout: crate::layout::Layout,
    tone: bool,
    // `want` is what an opened strip is trying to reach: the FOCUS
    // width, not the working one. A strip you have selected is the one
    // you are working IN, and the rack's editing tier — the plugin's
    // own grid, labels and nodes — starts well above the width a glance
    // needs. Opening to the working width made a selected strip a
    // slightly larger glance.
    want: f64,
    // A touchscreen's strips: no wider than [`TOUCH_STRIP_W`].
    touch: bool,
) -> Vec<f64> {
    let mut widths: Vec<f64> = rows
        .iter()
        .map(|(track, _)| {
            let width = layout.width_of(track.width);
            if touch {
                width.min(TOUCH_STRIP_W)
            } else {
                width
            }
        })
        .collect();
    if !tone {
        return widths;
    }

    let open: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, (track, _))| track.selected)
        .map(|(i, _)| i)
        .collect();
    if open.is_empty() {
        return widths;
    }

    // What the opened strips want, and what the rest can lend.
    let asked: f64 = open
        .iter()
        .filter_map(|i| widths.get(*i))
        .map(|w| (want - w).max(0.0))
        .sum();
    if asked <= 0.0 {
        return widths;
    }
    let headroom: Vec<f64> = widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if open.contains(&i) {
                0.0
            } else {
                (w - lending_floor(*w, layout)).max(0.0)
            }
        })
        .collect();
    let lent: f64 = headroom.iter().sum();
    let taken = asked.min(lent);
    if taken <= 0.0 {
        return widths;
    }

    // Proportional to headroom, so nobody is pushed under their floor:
    // a strip lends at most `taken/lent` of what it had spare, and
    // `taken <= lent`.
    for (i, width) in widths.iter_mut().enumerate() {
        if let Some(spare) = headroom.get(i) {
            *width -= taken * spare / lent;
        }
    }
    // The openers split what was actually raised, in proportion to what
    // each asked for — so two selected strips both open part way rather
    // than the first taking everything and the second getting nothing.
    //
    // Their own widths are untouched by the loop above (their headroom
    // is zero), so each ask is still the one `asked` was summed from.
    for i in &open {
        let Some(width) = widths.get_mut(*i) else {
            continue;
        };
        let ask = (want - *width).max(0.0);
        *width += taken * ask / asked;
    }
    widths
}

/// How wide the track-coloured rule up a strip's left edge is.
const STRIP_EDGE: f64 = 1.0;

/// How thick the selected strip's top rule is.
const SELECTED_RULE: f64 = 2.0;

fn strip(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    slot: Slot,
    rack: &[crate::tone::Which],
    tone: Option<&crate::tone::Tone>,
    ancestors: &[Color],
) {
    let Slot {
        x,
        width: w,
        height: h,
        buttons_top,
        rack_h,
        ..
    } = slot;
    // `Collapse` is written in the f32 the theme's geometry is, and a
    // strip height is a few hundred pixels — exact either way.
    // Two resolutions, and the difference matters.
    //
    // Nesting shortens a strip from the BOTTOM, so everything anchored
    // to the top — the rack, the coloured band, the pan knob, the arm —
    // is the same on every strip and resolves against the mixer's own
    // height. Only what hangs below is shorter, which is the fader, and
    // that is the cost of the indent rather than a second inconsistency.
    //
    // Resolving the bands per strip instead put the arm of a track
    // three levels deep several pixels above the arm beside it, and a
    // control that moves because of something about ITS track cannot be
    // scanned across tracks.
    let shape = crate::strip::shape_for(slot.mixer_h - rack_h, slot.live, slot.touch);
    let own = crate::strip::shape_for(h - rack_h, slot.live, slot.touch);
    let geometry = crate::strip::Strip::laid_out(
        w,
        h,
        slot.mixer_h,
        rack_h,
        buttons_top,
        slot.column,
        slot.live,
    )
    .touched(slot.touch);
    // The strip's own chrome — band, sections, plate — is the whole
    // strip when the rack is stacked over it, and the left column when
    // the rack stands beside it. See `strip::Layout`.
    let chrome_w = geometry.chrome_width();

    // The strip's ground, and the track's colour as a band across it.
    fill(
        scene,
        strip_ground(palette, track),
        Rect::new(x, 0.0, x + w, h),
    );
    // And the track's colour as a rule up the strip's whole left edge,
    // top to bottom. The rack is tall and the coloured band is a
    // strip's height down; between two racks there was nothing to say
    // where one track ends and the next begins. One pixel, always
    // there, in the colour the strip is read by.
    // The track's colour itself by default, not the band's muted tint
    // of it: one pixel has to carry the colour on its own. The tint,
    // or nothing, when the mixer is set that way — see `layout::Wash`.
    if let Some(edge) = wash(crate::layout::strip_edge(), palette, track) {
        fill(scene, edge, Rect::new(x, 0.0, x + STRIP_EDGE, h));
    }

    // ── The folders this strip sits inside ──
    //
    // Nesting shortens a strip from the bottom, which leaves a
    // staircase of empty space under it — one step per level. Those
    // steps are exactly the shape of the tree, so they carry the
    // colours of the tree: the step immediately under a strip is its
    // parent, the step under that its grandparent, and the bottom step
    // is the outermost folder.
    //
    // Every child of a folder puts that folder's colour at the same y,
    // so the steps join up across the children into one unbroken band —
    // the folder's strip CONTINUING along the bottom of everything it
    // contains, which is how you see what a track is part of without
    // counting indents back to the left.
    //
    // Bare, the staircase said only "this one is deeper than that one".
    // The strip was shortened by one step per level, so the mixer's
    // full height is this strip's plus the steps under it. `ancestors`
    // has exactly one entry per level, which is what makes that true.
    let full = h + crate::num::coord(ancestors.len()) * INDENT_STEP;
    for (level, tint) in ancestors.iter().enumerate() {
        // Level 0 is the OUTERMOST folder, so it sits on the floor; the
        // last is the immediate parent and sits right under the strip.
        //
        // Which is not just an ordering preference — it is what makes
        // the band continuous. A folder's own strip ends one step above
        // its children's, so the band carrying its colour has to run
        // from the child's bottom down to its own, and that is
        // `level + 1` steps off the floor.
        let from_floor = crate::num::coord(level + 1);
        let top = full - from_floor * INDENT_STEP;
        fill(scene, *tint, Rect::new(x, top, x + w, top + INDENT_STEP));
    }

    // The selected strip says so.
    //
    // Without it, a mic that has opened to a working width is just a
    // strip that is inexplicably wider than the one beside it — the
    // layout would look broken rather than focused. A rule along the
    // top edge rather than a tint over the whole strip: the strip's
    // colour is the TRACK's, and overlaying selection on it would make
    // two tracks of the same colour read as different ones.
    if track.selected {
        fill(
            scene,
            palette.accent,
            Rect::new(x, 0.0, x + w, SELECTED_RULE),
        );
    }

    let fx_section = crate::strip::fx_section(slot.live, slot.touch) + rack_h;
    let pan_band = f64::from(shape.pan_band);
    let input_band = f64::from(shape.input_band);
    let stretch_h = f64::from(own.stretch);

    // ── The FX section ──
    //
    // Below the rack, at the top of the REAPER strip — not at the top of
    // the whole panel. It is the button that OPENS the chain, so it
    // belongs with the controls rather than floating above the graphs
    // it would open: sitting on top of the embedded FX it read as a
    // label for them, which is the one thing it is not.
    //
    // The pill itself is NOT recorded: whether a track has a chain
    // changes while the window is open (`FxCountChanged`), and a pill
    // baked in at Empty was a button that told the truth once. It is
    // drawn in the live pass — see `overlay::controls` — which is also
    // where it can answer the pointer.

    // ── The Tone rack ──
    //
    // The whole top of the panel, down to where the REAPER strip
    // starts. It used to begin one FX-section below that, leaving a
    // band of empty strip above it — room reserved for the FX pill back
    // when the pill sat up here.
    record_rack(
        scene,
        palette,
        font,
        track,
        geometry.rack_rect().map(|r| (r, x)),
        rack,
        tone,
    );

    let band_top = fx_section;
    tinted_band(
        scene,
        palette,
        font,
        track,
        Slot {
            x,
            width: chrome_w,
            height: h,
            buttons_top,
            rack_h,
            mixer_h: slot.mixer_h,
            column: slot.column,
            collapsed: slot.collapsed,
            live: slot.live,
            touch: slot.touch,
        },
        (band_top, pan_band, input_band),
        &shape,
    );

    // The stretch section records nothing — see the note above
    // `bottom`. Its height is still resolved, because the bands below
    // it are placed against it.
    let _ = stretch_h;

    bottom(scene, palette, font, track, x, chrome_w, h, slot.collapsed);
}

/// The Tone rack over a strip, clipped to its box: `rack_box` in the
/// strip's own space, with the strip's left edge.
fn record_rack(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    rack_box: Option<(Rect, f64)>,
    rack: &[crate::tone::Which],
    tone: Option<&crate::tone::Tone>,
) {
    if let (Some((rack_box, x)), Some(tone)) = (rack_box, tone) {
        // Clipped to the box, like the live rack is: the chain is
        // longer than the box and would otherwise run down over the
        // strip's own controls. A whole strip is replayed as one span,
        // so the layer's push and pop are never split by the culling.
        scene.push_clip_layer(
            Affine::IDENTITY,
            &Rect::new(rack_box.x0 + x, rack_box.y0, rack_box.x1 + x, rack_box.y1),
        );
        crate::tone::record(
            scene,
            palette,
            font,
            tone,
            // The track's own chain: a return draws its one instance,
            // a channel the eleven.
            tone.panels(rack),
            // From `Strip::rack_rect`, translated by the strip's left
            // edge — so the box the rack is drawn in is the box a grip
            // is measured against.
            crate::tone::Panel::of(rack_box, x),
            // The recorded rack is at rest — Rescue shut, the rest
            // open, see `Folded::rest` — and a fold beyond that is
            // live: a strip whose rack the overlay draws is the one
            // that answers to it. See `overlay::Racks::folded`.
            crate::tone::Folded::rest(),
            rack_ground(palette, track),
        );
        scene.pop_layer();
    }
}

/// What the rack paints under its chain's end: the track's colour when
/// the mixer is set to fill the rack that way, else nothing.
///
/// One answer for the recording and the live pass, so a rack that
/// starts moving does not change colour under its chain.
#[must_use]
pub fn rack_ground(palette: &Palette, track: &Track) -> Option<Color> {
    wash(crate::layout::rack_fill(), palette, track)
}

/// The strip's ground: the panel's grey, or the track's wash when the
/// mixer is set to paint the strip in it — the FX section, the space
/// beside the fader, all of it, so the strip is one surface with its
/// band and its rack.
#[must_use]
pub fn strip_ground(palette: &Palette, track: &Track) -> Color {
    wash(crate::layout::strip_fill(), palette, track).unwrap_or(palette.tcp_tint)
}

/// The rule up a strip's left edge in the track's colour, when the mixer
/// is set to draw one — see `layout::strip_edge`.
#[must_use]
pub fn strip_edge(palette: &Palette, track: &Track) -> Option<Color> {
    wash(crate::layout::strip_edge(), palette, track)
}

/// A wash setting as a colour for this track: nothing, the band's
/// tint, or the track's own colour.
fn wash(wash: crate::layout::Wash, palette: &Palette, track: &Track) -> Option<Color> {
    match wash {
        crate::layout::Wash::Off => None,
        crate::layout::Wash::Tint => Some(crate::tcp::row_tint(palette, track)),
        crate::layout::Wash::Full => Some(crate::tcp::track_color(palette, track)),
    }
}

/// The coloured band: pan, the record input, and the arm hanging off its
/// bottom edge.
fn tinted_band(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    slot: Slot,
    bands: (f64, f64, f64),
    shape: &Collapse,
) {
    let Slot { x, width: w, .. } = slot;
    let (band_top, pan_band, input_band) = bands;
    let squeeze = Squeeze::at(w);
    fill(
        scene,
        crate::tcp::row_tint(palette, track),
        Rect::new(x, band_top, x + w, band_top + pan_band + input_band),
    );
    // Pan moves into the input area when its own section is gone, which
    // is `Collapse`'s call rather than a height comparison here.
    let pan_top = match shape.pan {
        PanAnchor::PanSection => band_top + 2.0,
        PanAnchor::InputArea => band_top + pan_band + 2.0,
    };
    // The pan knob is NOT recorded — it is drawn live, from the value
    // the track has now. See `overlay::controls`.

    // The record input, on armed tracks only.
    //
    // It is what a track RECORDS FROM, so a track that is not recording
    // has nothing to say with it — the same rule the track panel
    // follows. Drawn unconditionally it was an empty sunken rectangle on
    // every strip in the session: a black box with no label, which reads
    // as a hole rather than as a field nobody has filled in.
    if squeeze.head() && shape.show_record_input && track.armed {
        let field_x = x + f64::from(g::RECINPUT_X);
        let field_w = f64::from(g::RECINPUT_X)
            .mul_add(-2.0, w)
            .min(f64::from(g::RECINPUT_W));
        let top = band_top + pan_band + 2.0;
        let height = f64::from(g::INPUT_FIELD_H);
        fill(
            scene,
            palette.tcp_combo,
            Rect::new(field_x, top, field_x + field_w, top + height),
        );
        crate::tcp::glyphs(
            scene,
            font,
            palette.text_dim,
            &font.elide(&crate::tcp::record_input(track), 10.0, field_w - 14.0),
            field_x + 4.0,
            top + height / 2.0 + 3.5,
            10.0,
        );
        // The field's caret, so it reads as something you can open.
        crate::tcp::caret(
            scene,
            palette.text_faint,
            field_x + field_w - 9.0,
            top + height / 2.0 - 2.0,
        );
    }

    // The record arm is not recorded either: it is lit or not, and that
    // changes on a click. `overlay::controls` draws it, against the
    // same shared button line this strip was built around.
    let _ = pan_top;
}

/// Where a strip's columns fall, for a strip of this width.
///
/// REAPER has one strip width, so its geometry is a list of fixed
/// offsets. Ours are not one width — a piece is wider than a mic, and a
/// selected strip is wider again — and fixed offsets meant every extra
/// pixel piled up as dead space on the right: the fader stayed a third
/// of the way across, the arm sat where an 86-wide strip puts it, and
/// the meter never grew.
///
/// So the offsets become rules, chosen so that **a strip of exactly
/// [`STRIP_W`] reproduces REAPER's own layout**. That is the constraint
/// that keeps this honest: the measured numbers are still the numbers,
/// they are just expressed as where-they-come-from rather than as
/// where-they-landed.
///
/// | | rule | at 86 | REAPER |
/// |---|---|---|---|
/// | button column | right-aligned, REAPER's margin | 55 | 55 |
/// | fader | centred | 32 | 31 |
/// | scale | left, fixed width | 2 | ~4 |
/// | meter | fills what is left between them | ~0 | not drawn |
#[derive(Clone, Copy, Debug)]
pub struct Columns {
    pub scale_x: f64,
    pub scale_w: f64,
    pub fader_x: f64,
    pub fader_w: f64,
    /// The left edge of the button column.
    pub column_x: f64,
    /// Its centre — what the record arm hangs off.
    pub column_axis: f64,
}

/// The fader's own width. Fixed: a fader is a fader, and a wider strip
/// wants a longer scale, not a fatter handle.
///
/// Wider than the 22 it was, because the column is the METER now — the
/// groove is lit by the signal, the cap rides over it as glass, and the
/// meter is wider than the cap so each channel shows down its own side
/// of it. The cap itself is still 22 (`paint::cap_w`); what grew is the
/// scale it is read against, not the handle.
///
/// There is no second column to size against. That is what lets a narrow strip have both: they were never
/// two things that needed two columns, and REAPER drew no meter at all
/// on an 86-wide strip because there was nowhere left to put one.
const FADER_W: f64 = 30.0;

/// How much room the scale's numbers need. Fixed, because `-54-` is
/// `-54-` at any strip width.
const SCALE_W: f64 = 24.0;

impl Columns {
    pub fn at(x: f64, w: f64) -> Self {
        // Right-aligned by REAPER's own right margin, so the buttons
        // keep their distance from the edge instead of their distance
        // from the left.
        let margin = f64::from(g::STRIP_W - g::COLUMN);
        let column_x = if Squeeze::at(w).columns() {
            (x + w - margin).max(x)
        } else {
            x + (w - f64::from(g::BUTTON_W)).max(0.0) / 2.0
        };
        let fader_x = x + (w - FADER_W) / 2.0;
        // Hard against the meter, because the numbers and the column
        // are one instrument: a tick runs from each number to the
        // meter's edge, and a gap the width of the strip's margin
        // would make that tick a bridge over nothing.
        let scale_x = (fader_x - SCALE_W).max(x + 1.0);
        Self {
            scale_x,
            scale_w: SCALE_W,
            fader_x,
            fader_w: FADER_W,
            column_x,
            column_axis: column_x + f64::from(g::BUTTON_W) / 2.0,
        }
    }
}

/// The label a strip shows for its track, and the size it fits at.
///
/// Three ways to make a name fit, in order of what they cost.
///
/// Print it, shrink it, then drop what the folders already said — and
/// only cut as a last resort. Dropping comes AFTER shrinking because
/// the full name at nine points tells you more than half of it at
/// eleven; it comes before cutting because `Trig` is a word and `T1 …`
/// is not.
fn fit_name(font: &Font, track: &Track, w: f64, ancestor_names: &[&str]) -> (String, f32) {
    let room = w - 8.0;
    let (mut label, mut size) = font.fit(&track.name, 11.0, 7.0, room);
    if label.contains('…')
        && let Some(short) = shorten(&track.name, ancestor_names)
    {
        let (short_label, short_size) = font.fit(&short, 11.0, 7.0, room);
        if !short_label.contains('…') {
            label = short_label;
            size = short_size;
        }
    }
    (label, size)
}

// Nothing in the stretch section is recorded any more.
//
// It held the dB scale and an empty meter well. The well went when the
// fader's groove became the meter, and the scale went live with it —
// its numbers light as the signal passes them, which a recording
// cannot do. The fader, the cap, the button column and the routing
// were already live for the same reason: every one of them is a value
// that changes while the window is open, and a recorded value is one
// that was right once. See `overlay::draw_strip_controls`.

/// The track number under a strip, on the track's own colour.
///
/// The NAME is not here. It is drawn live — see `Mixer::label` —
/// because its ink answers to mute, solo and selection, and it has no
/// plate behind it: a name is text ON the strip, the way a name written
/// on tape is, and a box drawn permanently behind it was only ever a
/// box. A field belongs there while it is being TYPED in, and
/// `rename::paint` draws its own.
fn bottom(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    x: f64,
    w: f64,
    h: f64,
    collapsed: bool,
) {
    // The number sits on the track's own colour, in a band exactly one
    // indent step tall.
    //
    // That height is the whole point and not a detail. A folder's strip
    // ends one step above its children's, and its children each draw a
    // step of its colour under themselves — so if the folder's own
    // colour band is a different height, its line sits at a different
    // level from the line continuing across everything it contains, and
    // the two read as unrelated marks that happen to share a hue.
    //
    // At one step they are the same line. A red Drum Kit puts red under
    // its number, and that red runs unbroken along the floor beneath
    // every track inside it.
    let number = track.index.saturating_add(1).to_string();
    let number_h = INDENT_STEP;
    let number_top = h - number_h;
    if number_h < h {
        fill(
            scene,
            crate::tcp::folder_band(palette, track),
            Rect::new(x, number_top, x + w, h),
        );
    }
    crate::tcp::glyphs(
        scene,
        font,
        // Against a colour now rather than the panel, so the faintest
        // ink in the palette no longer reads — a number you cannot make
        // out is the same as no number.
        palette.text_dim,
        &number,
        // Centred on the strip, under the name that is also centred on
        // it. Pinned left they read as two different alignments for one
        // track's identity — and on a wide strip the number ended up in
        // a corner with the name over the middle of it.
        x + (w - font.width(&number, NUMBER_SIZE)) / 2.0,
        // Centred in the band rather than sat near its top: the band is
        // one indent step tall and the number was riding high in it,
        // which reads as the number belonging to the name above rather
        // than to the colour it is printed on.
        number_top + (number_h + f64::from(NUMBER_SIZE) * 0.72) / 2.0,
        NUMBER_SIZE,
    );
    // A folder's fold, at the left of the band: a chevron pointing
    // down into its open rows, or right at the rows it is holding
    // shut. See `Control::Folder`.
    if track.is_folder {
        let cx = x + f64::from(FOLD_W) / 2.0;
        let cy = number_top + number_h / 2.0;
        let arm = 3.0;
        let mut path = vello::kurbo::BezPath::new();
        if collapsed {
            path.move_to((cx - arm * 0.6, cy - arm));
            path.line_to((cx + arm * 0.6, cy));
            path.line_to((cx - arm * 0.6, cy + arm));
        } else {
            path.move_to((cx - arm, cy - arm * 0.6));
            path.line_to((cx, cy + arm * 0.6));
            path.line_to((cx + arm, cy - arm * 0.6));
        }
        path.close_path();
        scene.fill(
            Fill::NonZero,
            vello::kurbo::Affine::IDENTITY,
            palette.text_dim,
            None,
            &path,
        );
    }
}

/// How wide the fold target at the foot of a folder's strip is.
pub const FOLD_W: u32 = 14;

/// How big the track number under a strip is drawn.
const NUMBER_SIZE: f32 = 9.0;

/// A strip height, for the f32 the theme's geometry is written in.
pub const fn f64_to_f32(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "a pixel height; f32 holds it exactly and the theme's API takes one"
    )]
    let narrowed = value as f32;
    narrowed
}

fn fill(scene: &mut Scene, color: Color, rect: Rect) {
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
}

#[cfg(test)]
mod selection_tests {
    /// A mixer laid out whole and recorded as it is drawn records each
    /// strip exactly as one built eagerly — and nothing until asked; a
    /// past deadline records none and says some are left.
    #[test]
    fn a_lazily_recorded_mixer_draws_what_an_eager_one_does() {
        use super::{Font, Mixer, Palette};
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = Font::embedded().expect("the embedded font");
        let tracks: Vec<Track> = (0..40)
            .map(|i| Track {
                guid: format!("t{i}"),
                name: format!("Track {i}"),
                ..Track::default()
            })
            .collect();
        let rows = RowsRef(std::sync::Arc::new(
            tracks.iter().cloned().map(|t| (t, 0)).collect(),
        ));
        let project =
            daw_ui::studio::ProjectRef(std::sync::Arc::new(daw_ui::studio::Project::default()));
        let store = crate::tone::Store::default();
        let build = |lazy: bool| {
            let args = (
                &palette,
                &font,
                &project,
                &rows,
                600.0,
                crate::layout::Layout::default(),
            );
            if lazy {
                Mixer::lay_out(
                    args.0,
                    args.1,
                    args.2,
                    args.3,
                    args.4,
                    args.5,
                    &[],
                    false,
                    crate::settings::Settings::default(),
                    &store,
                )
            } else {
                Mixer::build(
                    args.0,
                    args.1,
                    args.2,
                    args.3,
                    args.4,
                    args.5,
                    &[],
                    false,
                    crate::settings::Settings::default(),
                    &store,
                )
            }
        };
        let eager = build(false);
        let mut lazy = build(true);
        let spans = |m: &Mixer| {
            m.index
                .iter()
                .map(ExactSizeIterator::len)
                .collect::<Vec<_>>()
        };
        assert!(lazy.strips.commands.is_empty(), "nothing recorded at build");
        assert_eq!(
            lazy.content_width().to_bits(),
            eager.content_width().to_bits(),
            "laid out whole"
        );
        let late = web_time::Instant::now();
        assert!(
            lazy.record(0..5, &palette, &font, &store, Some(late)),
            "a spent budget leaves them"
        );
        let shown = lazy.visible(0.0, 600.0);
        assert!(!lazy.record(shown.clone(), &palette, &font, &store, None));
        for row in 0..40 {
            let want = if shown.contains(&row) {
                spans(&eager)[row]
            } else {
                0
            };
            assert_eq!(spans(&lazy)[row], want, "strip {row}");
        }
        assert!(!lazy.record(0..40, &palette, &font, &store, None));
        assert_eq!(
            spans(&lazy),
            spans(&eager),
            "every strip as the eager build drew it"
        );
    }

    use super::widths;
    use crate::layout::Layout;
    use daw_proto::Track;
    use daw_ui::studio::RowsRef;

    /// `stored` widths, with the strip at `select` selected.
    fn rows(stored: &[u32], select: Option<usize>) -> RowsRef {
        RowsRef(std::sync::Arc::new(
            stored
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    (
                        Track {
                            guid: format!("t{i}"),
                            width: Some(*w),
                            selected: select == Some(i),
                            ..Track::default()
                        },
                        0,
                    )
                })
                .collect(),
        ))
    }

    fn total(widths: &[f64]) -> f64 {
        widths.iter().sum()
    }

    /// The claim the whole design rests on: opening a strip does not
    /// make the mixer wider. If the kit fitted the screen before the
    /// click, it fits after it.
    #[test]
    fn opening_a_strip_does_not_widen_the_mixer() {
        let stored = [60, 195, 86, 86, 30, 30, 195, 195, 60, 86];
        let layout = Layout::default();
        let shut = widths(&rows(&stored, None), layout, true, WANT, false);
        let open = widths(&rows(&stored, Some(2)), layout, true, WANT, false);

        assert!(
            (total(&shut) - total(&open)).abs() < 1e-9,
            "the total moved: {} -> {}",
            total(&shut),
            total(&open)
        );
    }

    /// What an opened strip is asking for in these tests: the working
    /// width, which the fixture's neighbours can actually cover. A real
    /// selection asks for the FOCUS width and gets as much of it as the
    /// others can lend — see `the_opened_strip_takes_what_it_can_get`.
    const WANT: f64 = crate::tone::WORKING;

    /// And the strip you opened actually opened.
    #[test]
    fn the_opened_strip_reaches_the_width_it_asked_for() {
        let stored = [60, 195, 86, 86, 30, 30, 195, 195, 60, 86];
        let open = widths(
            &rows(&stored, Some(2)),
            Layout::default(),
            true,
            WANT,
            false,
        );
        assert!(
            (open[2] - WANT).abs() < 1e-9,
            "wanted {WANT}, got {}",
            open[2]
        );
    }

    /// An ask the others cannot cover is not a failure: the strip takes
    /// what was raised and the rack opens to whatever tier that width
    /// supports. A focus width on a crowded mixer is exactly this case,
    /// and the mixer must still not get wider.
    #[test]
    fn the_opened_strip_takes_what_it_can_get() {
        let stored = [60, 60, 60, 60, 60, 60];
        let layout = Layout::default();
        let shut = widths(&rows(&stored, None), layout, true, 900.0, false);
        let open = widths(&rows(&stored, Some(2)), layout, true, 900.0, false);
        assert!(
            (total(&shut) - total(&open)).abs() < 1e-9,
            "the total moved"
        );
        assert!(open[2] > shut[2], "it opened as far as it could");
        assert!(open[2] < 900.0, "and not further than there was");
        for (i, width) in open.iter().enumerate() {
            assert!(
                *width >= layout.strip_min - 1e-9,
                "strip {i} fell under the floor"
            );
        }
    }

    /// Everyone else gives up a little, and nobody is pushed under the
    /// floor their controls need.
    #[test]
    fn the_others_lend_from_their_headroom() {
        let stored = [60, 195, 86, 86, 30, 30, 195, 195, 60, 86];
        let layout = Layout::default();
        let open = widths(&rows(&stored, Some(2)), layout, true, WANT, false);

        for (i, width) in open.iter().enumerate() {
            assert!(
                *width >= layout.strip_min - 1e-9,
                "strip {i} fell to {width}, under the floor of {}",
                layout.strip_min
            );
        }
        // A strip already AT the floor has nothing to lend and keeps
        // every pixel: lending is proportional to headroom.
        assert!(
            (open[4] - 30.0).abs() < 1e-9,
            "a floor strip lent: {}",
            open[4]
        );
        assert!(open[1] < 195.0, "a wide strip should have lent");
    }

    /// A piece is stored at the width selection opens to, so selecting
    /// one is a no-op — nothing moves under you when you click a track
    /// that is already open.
    #[test]
    fn selecting_an_already_open_strip_changes_nothing() {
        let stored = [60, 195, 86, 86, 30];
        let layout = Layout::default();
        let shut = widths(&rows(&stored, None), layout, true, WANT, false);
        let open = widths(&rows(&stored, Some(1)), layout, true, WANT, false);
        assert_eq!(shut, open);
    }

    /// When the others cannot cover the ask, the opened strip takes
    /// what there was rather than overdrawing — the rack then opens to
    /// whatever tier that width supports.
    #[test]
    fn an_ask_larger_than_the_headroom_is_capped() {
        // Three strips at the floor have nothing to lend.
        let stored = [30, 30, 30];
        let layout = Layout::default();
        let shut = widths(&rows(&stored, None), layout, true, WANT, false);
        let open = widths(&rows(&stored, Some(0)), layout, true, WANT, false);
        assert!((total(&shut) - total(&open)).abs() < 1e-9);
        assert!(
            (open[0] - 30.0).abs() < 1e-9,
            "nothing could be lent, so nothing should have moved: {:?}",
            open
        );
    }

    /// Two selected strips both open part way rather than the first
    /// taking everything.
    #[test]
    fn two_open_strips_share_what_is_raised() {
        let stored = [30, 30, 86, 86];
        let layout = Layout::default();
        let open = widths(&rows(&stored, Some(0)), layout, true, WANT, false);
        let both = {
            let rows = RowsRef(std::sync::Arc::new(
                stored
                    .iter()
                    .enumerate()
                    .map(|(i, w)| {
                        (
                            Track {
                                guid: format!("t{i}"),
                                width: Some(*w),
                                selected: i < 2,
                                ..Track::default()
                            },
                            0,
                        )
                    })
                    .collect(),
            ));
            widths(&rows, layout, true, WANT, false)
        };
        assert!((total(&open) - total(&both)).abs() < 1e-9);
        assert!(
            both[0] > 30.0 && both[1] > 30.0,
            "both should have opened: {both:?}"
        );
        assert!(
            both[0] < open[0],
            "sharing means each gets less than one alone would"
        );
    }

    /// Opening one strip must not switch off anyone else's rack.
    ///
    /// The bug this guards: every lender gave up a few pixels, which
    /// was enough to carry a strip across `LEGIBLE`, so one click on a
    /// mic blanked the processing on the whole kit. Lending has to be
    /// invisible, and a control vanishing is the least invisible thing
    /// a layout can do.
    #[test]
    fn lending_never_costs_a_strip_its_rack() {
        // A kit's worth of pieces sitting just above the threshold,
        // which is where the bug bit.
        let piece = crate::tone::LEGIBLE + 4.0;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions,
            reason = "a strip width in pixels, built for a fixture"
        )]
        let piece = piece as u32;
        let stored = [piece, piece, piece, piece, piece, piece, 86, 86, 30, 30];
        let layout = Layout::default();

        let shut = widths(&rows(&stored, None), layout, true, WANT, false);
        let open = widths(&rows(&stored, Some(6)), layout, true, WANT, false);

        for (i, (before, after)) in shut.iter().zip(&open).enumerate() {
            if i == 6 {
                continue;
            }
            assert_eq!(
                crate::tone::Rack::at(*before),
                crate::tone::Rack::at(*after),
                "strip {i} changed rack tier when another opened: {before} -> {after}"
            );
        }
    }

    /// Outside the Tone sub-mode selection is not a zoom at all.
    #[test]
    fn selection_only_opens_in_tone() {
        let stored = [60, 86, 86];
        let layout = Layout::default();
        let plain = widths(&rows(&stored, Some(1)), layout, false, WANT, false);
        assert_eq!(plain, vec![60.0, 86.0, 86.0]);
    }

    /// What selection opens to is legible — the two constants are set
    /// independently and nothing else would catch them crossing.
    #[test]
    fn an_opened_strip_is_legible() {
        assert!(crate::tone::WORKING >= crate::tone::LEGIBLE);
        assert_eq!(
            crate::tone::Rack::at(crate::tone::WORKING),
            crate::tone::Rack::Full
        );
    }
}

#[cfg(test)]
mod column_tests {
    use super::{Columns, STRIP_W};
    use daw_theme_art::geometry::mcp as g;

    /// The constraint the whole model rests on: expressing the measured
    /// offsets as RULES must not change what they evaluate to at the
    /// width they were measured at. If this drifts, every number in
    /// `geometry::mcp` has quietly stopped describing what is drawn.
    #[test]
    fn an_86_wide_strip_is_still_reapers_layout() {
        let c = Columns::at(0.0, STRIP_W);
        assert!(
            (c.column_x - f64::from(g::COLUMN)).abs() < 0.01,
            "button column drifted: {} vs REAPER's {}",
            c.column_x,
            g::COLUMN
        );
        assert!(
            (c.column_axis - f64::from(g::COLUMN_AXIS)).abs() < 0.01,
            "column axis drifted: {} vs {}",
            c.column_axis,
            g::COLUMN_AXIS
        );
        let arm = c.column_axis - f64::from(g::ARM_CELL_W) * 0.486;
        assert!(
            (arm - f64::from(g::ARM_LEFT)).abs() < 0.01,
            "record arm drifted: {arm} vs REAPER's {}",
            g::ARM_LEFT
        );
        // REAPER's groove sits at 31; centring puts the cap's cell at
        // 32. Measured against the CAP rather than the column, because
        // the column grew to carry the meter either side of it — the
        // handle is what has to land where REAPER's handle lands.
        let cap = c.fader_x + (c.fader_w - daw_theme_art::paint::tcp::cap_w(c.fader_w)) / 2.0;
        assert!((cap - 32.0).abs() < 0.5, "the cap sits at {cap}");
        // REAPER draws no meter at this width because the scale and
        // the fader have taken it all. We draw one anyway, because it
        // is the fader's own groove and costs no width at all — which
        // is the whole point of the change.
        assert!(c.fader_w > 0.0);
    }

    /// The whole point of the change: width goes somewhere useful.
    #[test]
    fn a_wider_strip_spends_the_width() {
        let narrow = Columns::at(0.0, STRIP_W);
        let wide = Columns::at(0.0, 195.0);

        assert!(
            wide.fader_x > narrow.fader_x,
            "the fader should move to the middle, not stay left"
        );
        assert!(
            (wide.fader_x + wide.fader_w / 2.0 - 195.0 / 2.0).abs() < 0.01,
            "the fader should be centred"
        );
        assert!(
            wide.column_axis > narrow.column_axis,
            "the buttons and the arm should travel right with the edge"
        );
    }

    /// The right margin is REAPER's, at every width — that is what
    /// "right-aligned" has to mean for the column to look placed rather
    /// than pushed.
    #[test]
    fn the_right_margin_is_constant() {
        let margin = f64::from(STRIP_W - f64::from(g::COLUMN));
        for w in [STRIP_W, 96.0, 130.0, 195.0, 300.0] {
            let c = Columns::at(0.0, w);
            assert!(
                (w - c.column_x - margin).abs() < 0.01,
                "margin at width {w}: {}",
                w - c.column_x
            );
        }
    }

    /// The meter is the fader, at every width — so a very wide strip
    /// gets a longer scale rather than a fatter handle, and a narrow
    /// one still gets a meter where it used to get none.
    #[test]
    fn the_meter_is_the_fader_at_every_width() {
        for w in [56.0, STRIP_W, 96.0, 195.0, 600.0] {
            let c = Columns::at(0.0, w);
            assert!(
                (c.fader_w - super::FADER_W).abs() < f64::EPSILON,
                "the fader fattened at {w}"
            );
            let groove = daw_theme_art::paint::tcp::groove_w(c.fader_w);
            assert!(
                groove >= 4.0 && groove < c.fader_w,
                "groove at {w}: {groove}"
            );
        }
    }

    /// Nothing escapes the strip it belongs to.
    #[test]
    fn every_column_stays_inside_the_strip() {
        for w in [30.0, 56.0, STRIP_W, 96.0, 130.0, 195.0] {
            let c = Columns::at(10.0, w);
            let right = 10.0 + w;
            for (name, edge) in [
                ("scale", c.scale_x + c.scale_w),
                ("fader", c.fader_x + c.fader_w),
                ("column", c.column_x + f64::from(g::BUTTON_W)),
            ] {
                assert!(
                    edge <= right + 0.01,
                    "{name} runs past the strip at width {w}: {edge} > {right}"
                );
            }
            assert!(c.scale_x >= 10.0 && c.fader_x >= 10.0 && c.column_x >= 10.0);
        }
    }
}

#[cfg(test)]
mod folder_band_tests {
    use super::INDENT_STEP;

    /// A folder's own colour band and the band its children draw for it
    /// must occupy the same pixels, or the "line" is two lines.
    ///
    /// The folder's band is the last [`INDENT_STEP`] of ITS strip; the
    /// child draws the folder's step `level + 1` off the floor. This is
    /// the arithmetic that makes those the same range — get it wrong by
    /// the difference between the band height and the indent step and
    /// the stripes look almost right, which is worse than obviously
    /// wrong because nobody reports it.
    #[test]
    fn a_folders_band_lines_up_with_its_childrens() {
        const FULL: f64 = 460.0;
        for folder_depth in 0..4_usize {
            // The folder's own strip, and its number band at the foot.
            let folder_h = FULL - folder_depth as f64 * INDENT_STEP;
            let folder_band = (folder_h - INDENT_STEP, folder_h);

            // A child one level deeper: the folder is the LAST of its
            // ancestors, so `level` is the folder's own depth.
            let child_depth = folder_depth + 1;
            let child_h = FULL - child_depth as f64 * INDENT_STEP;
            let ancestors = child_depth; // one entry per level above it
            let full_from_child = child_h + ancestors as f64 * INDENT_STEP;
            let from_floor = (folder_depth + 1) as f64;
            let top = full_from_child - from_floor * INDENT_STEP;
            let child_draws = (top, top + INDENT_STEP);

            assert!(
                (folder_band.0 - child_draws.0).abs() < 1e-9
                    && (folder_band.1 - child_draws.1).abs() < 1e-9,
                "depth {folder_depth}: folder's band {folder_band:?} but children draw it at {child_draws:?}"
            );
        }
    }

    /// And a grandchild puts it in the same place as a child does — the
    /// line is continuous across every level below the folder, not just
    /// the one immediately under it.
    #[test]
    fn the_line_survives_deeper_nesting() {
        const FULL: f64 = 460.0;
        let folder_depth = 1_usize;
        let expected = FULL - (folder_depth + 1) as f64 * INDENT_STEP;
        for descendant_depth in (folder_depth + 1)..5 {
            let h = FULL - descendant_depth as f64 * INDENT_STEP;
            let full = h + descendant_depth as f64 * INDENT_STEP;
            let top = full - (folder_depth + 1) as f64 * INDENT_STEP;
            assert!(
                (top - expected).abs() < 1e-9,
                "a descendant at depth {descendant_depth} drew the folder's line at {top}, not {expected}"
            );
        }
    }
}

#[cfg(test)]
mod shorten_tests {
    use super::shorten;

    /// The case that prompted it: a trigger inside the tom it triggers.
    #[test]
    fn a_folder_does_not_need_saying_twice() {
        assert_eq!(
            shorten("T1 Trig", &["Drum Kit", "Toms", "Tom 1"]).as_deref(),
            Some("Trig")
        );
        assert_eq!(
            shorten("T4 Trig", &["Drum Kit", "Toms", "Tom 4"]).as_deref(),
            Some("Trig")
        );
    }

    /// Long and short forms of a folder's name are the same folder —
    /// `Tom 1` has to answer to `T1` or the rule never fires, since the
    /// template writes the folder long and the tracks short.
    #[test]
    fn a_folder_answers_to_its_initials() {
        assert_eq!(shorten("HH Bleed", &["Hi-Hat"]).as_deref(), Some("Bleed"));
        assert_eq!(shorten("Tom 1 Trig", &["Tom 1"]).as_deref(), Some("Trig"));
    }

    /// A name that says nothing its folders said keeps every word.
    #[test]
    fn an_unrelated_name_is_left_alone() {
        assert_eq!(shorten("Stereo L", &["Drum Kit", "Rooms"]), None);
        assert_eq!(shorten("Bottom", &["Snare", "Sum"]), None);
        assert_eq!(shorten("Trig", &["Tom 1"]), None);
    }

    /// And a strip is never left with no label at all — the last word
    /// stays even when the folder said it.
    #[test]
    fn it_never_strips_the_whole_name() {
        assert_eq!(shorten("T1", &["Tom 1"]), None);
        assert_eq!(shorten("Tom 1", &["Tom 1"]), None);
        for short in [shorten("Sum", &["Kick"]), shorten("Kick", &["Kick"])] {
            assert!(short.as_deref().is_none_or(|s| !s.is_empty()));
        }
    }

    /// It only ever gets shorter — a caller that swaps in the result
    /// must never end up with MORE to fit than it started with.
    #[test]
    fn the_result_is_always_shorter() {
        for (name, ancestors) in [
            ("T1 Trig", &["Tom 1"][..]),
            ("Stereo L", &["Rooms"][..]),
            ("Kick In", &["Kick"][..]),
            ("", &["Kick"][..]),
        ] {
            if let Some(short) = shorten(name, ancestors) {
                assert!(short.len() < name.len(), "{name:?} -> {short:?}");
            }
        }
    }
}
