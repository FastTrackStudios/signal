//! The Patch List view: source kinds against inputs, grouped by
//! performer.
//!
//! Read-only here (spec #48, `flow.patch-list.plan`): the table is a
//! projection of the album's list resolved against the active studio
//! profile, drawn the way tracking sorts — each performer a header,
//! their rig under it, one row per channel or mic — with the headphone
//! buses at the bottom and every role the room does not have marked
//! rather than dropped. Editing a row, applying the plan and the
//! per-session override are #57.
//!
//! The picture is recorded the way every other view in this window is:
//! one `PaintScene`, explicit geometry, the embedded font, no CSS and
//! no layout engine — so the bench can draw it headlessly and the
//! committed PNG is the fixture.

use anyrender::PaintScene;
use patch_list::profile::Resolved;
use patch_list::{PatchList, StudioProfile};
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use crate::arrangement::Palette;
use crate::text::Font;

/// Whether the room resolved a row's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// The profile has the role.
    Resolved,
    /// It does not — shown, never an error.
    Unresolved,
}

/// One channel or mic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The kind of source (`guitar`, `drums`).
    pub kind: String,
    /// The rig key as the list writes it (`di`, `kick/in`).
    pub key: String,
    /// The input role, or the MIDI device for a MIDI entry.
    pub role: String,
    /// What the room makes of it, for the Input column.
    pub input: String,
    pub mark: Mark,
    /// Whether the session override replaced this entry
    /// (`flow.patch-list.session-override`) — marked, never hidden.
    pub overridden: bool,
    /// Whether the session has no track for this entry
    /// (`flow.patch-list.apply`) — a view state, never acted on.
    pub unused: bool,
}

/// One performer's rows, under their header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerformerRows {
    pub name: String,
    pub rows: Vec<Row>,
}

/// One headphone bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusRow {
    pub name: String,
    /// Who it is for, joined for the column.
    pub audience: String,
    /// The bus role.
    pub role: String,
    /// The output pair, or `unresolved`.
    pub input: String,
    pub mark: Mark,
    /// Whether the session override replaced this bus.
    pub overridden: bool,
}

/// The whole view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// The room the table was resolved against.
    pub studio: String,
    pub performers: Vec<PerformerRows>,
    pub buses: Vec<BusRow>,
    /// How many rows and buses the room could not resolve.
    pub unresolved: usize,
    /// Tracks the session has that no entry names — a view state,
    /// never acted on by apply (`flow.patch-list.apply`).
    pub unpatched: Vec<String>,
    /// When set, the session is stale (`flow.patch-list.apply`): the
    /// album or the active profile has moved on since the last apply.
    /// The banner text is what the view shows.
    pub stale: Option<String>,
}

impl Table {
    /// Build the table from a list and a room.
    ///
    /// A list the room double-books is not a table: validation is the
    /// one refusal, and the caller shows the error instead. Everything
    /// else — a role the room lacks, a bus it has no output for — is a
    /// marked row.
    ///
    /// # Errors
    ///
    /// When two entries name one role the profile does not share.
    // r[impl flow.patch-list.plan]
    // r[impl flow.patch-list.studio-profiles]
    pub fn build(
        studio: &str,
        list: &PatchList,
        profile: &StudioProfile,
    ) -> Result<Self, patch_list::ValidationError> {
        Self::build_layered(studio, &patch_list::layer(list, None), profile)
    }

    /// Build the table from an album with a session override already
    /// layered on top (`flow.patch-list.session-override`): the same
    /// table [`build`] produces when there is no override, plus every
    /// row the override touched marked.
    ///
    /// # Errors
    ///
    /// When two entries name one role the profile does not share.
    // r[impl flow.patch-list.plan]
    // r[impl flow.patch-list.studio-profiles]
    // r[impl flow.patch-list.session-override]
    pub fn build_layered(
        studio: &str,
        layered: &patch_list::Layered,
        profile: &StudioProfile,
    ) -> Result<Self, patch_list::ValidationError> {
        let plan = patch_list::validate(&layered.list, profile)?;
        let mut performers: Vec<PerformerRows> = Vec::new();
        for entry in &plan.entries {
            let overridden = layered.overridden_entries.contains(&(
                entry.lowered.performer.clone(),
                entry.lowered.kind.clone(),
                entry.lowered.key.clone(),
            ));
            let row = Row {
                kind: entry.lowered.kind.clone(),
                key: entry.lowered.key.clone(),
                role: match &entry.lowered.entry {
                    patch_list::Entry::Role(role) => role.clone(),
                    patch_list::Entry::Midi { device, .. } => device.clone(),
                },
                input: describe(&entry.resolved),
                mark: mark(&entry.resolved),
                overridden,
                // Whether the session has a track for this entry is
                // session data `build_layered` has no view of — set
                // afterward via `with_unused`, same as `unpatched`.
                unused: false,
            };
            match performers
                .iter_mut()
                .find(|p| p.name == entry.lowered.performer)
            {
                Some(performer) => performer.rows.push(row),
                None => performers.push(PerformerRows {
                    name: entry.lowered.performer.clone(),
                    rows: vec![row],
                }),
            }
        }
        let buses = plan
            .buses
            .iter()
            .map(|bus| BusRow {
                name: bus.name.clone(),
                audience: bus.bus.audience.join(", "),
                role: bus.bus.output.clone(),
                input: describe(&bus.resolved),
                mark: mark(&bus.resolved),
                overridden: layered.overridden_buses.contains(&bus.name),
            })
            .collect();
        Ok(Self {
            studio: studio.to_owned(),
            performers,
            buses,
            unresolved: plan.unresolved(),
            unpatched: Vec::new(),
            stale: None,
        })
    }

    /// Mark the session stale with a banner message
    /// (`flow.patch-list.apply`) — never auto-applies, only shown.
    #[must_use]
    pub fn with_stale(mut self, banner: Option<String>) -> Self {
        self.stale = banner;
        self
    }

    /// Record the session's own tracks that no entry names — a view
    /// state, never acted on (`flow.patch-list.apply`).
    #[must_use]
    pub fn with_unpatched(mut self, tracks: Vec<String>) -> Self {
        self.unpatched = tracks;
        self
    }

    /// Mark every rig entry with no matching track as unused — a view
    /// state, never acted on (`flow.patch-list.apply`). `keys` are
    /// `(performer, kind, key)` triples, the same path apply's own
    /// `Lowered` entries carry.
    #[must_use]
    pub fn with_unused(mut self, keys: &[(String, String, String)]) -> Self {
        for performer in &mut self.performers {
            for row in &mut performer.rows {
                if keys
                    .iter()
                    .any(|(p, k, key)| *p == performer.name && *k == row.kind && *key == row.key)
                {
                    row.unused = true;
                }
            }
        }
        self
    }

    /// The fixture album in the fixture room — what the render fixture
    /// is a picture of.
    ///
    /// # Errors
    ///
    /// When the committed fixtures stop parsing or stop validating,
    /// which means a broken fixture rather than a broken machine — so
    /// it is returned and named rather than panicked, and the test that
    /// reads it says which file to look at.
    pub fn fixture() -> Result<Self, String> {
        let list = PatchList::from_styx(patch_list::FIXTURE_ALBUM)
            .map_err(|e| format!("the fixture album does not parse: {e}"))?;
        let room = StudioProfile::from_styx(patch_list::FIXTURE_STUDIO)
            .map_err(|e| format!("the fixture room does not parse: {e}"))?;
        Self::build(patch_list::FIXTURE_STUDIO_NAME, &list, &room)
            .map_err(|e| format!("the fixture album is not valid: {e}"))
    }

    /// The fixture album, with a session override on Cody's DI and a
    /// stale banner — what the second render fixture is a picture of
    /// (#57: override-marked and stale states, #48's fixture
    /// amendment).
    ///
    /// # Errors
    ///
    /// Same as [`Self::fixture`], plus when the override text itself
    /// does not parse — which would mean this function's own literal
    /// is wrong.
    pub fn fixture_overridden_stale() -> Result<Self, String> {
        let list = PatchList::from_styx(patch_list::FIXTURE_ALBUM)
            .map_err(|e| format!("the fixture album does not parse: {e}"))?;
        let room = StudioProfile::from_styx(patch_list::FIXTURE_STUDIO)
            .map_err(|e| format!("the fixture room does not parse: {e}"))?;
        let over = PatchList::from_styx("performers {\n    cody {guitar {di \"DI 9\"}}\n}")
            .map_err(|e| format!("the override literal does not parse: {e}"))?;
        let layered = patch_list::layer(&list, Some(&over));
        let table = Self::build_layered(patch_list::FIXTURE_STUDIO_NAME, &layered, &room)
            .map_err(|e| format!("the fixture album is not valid: {e}"))?;
        Ok(table
            .with_stale(Some(
                "the album's patch list has changed since this session applied it".to_owned(),
            ))
            .with_unpatched(vec!["Extra Guitar (unpatched)".to_owned()])
            .with_unused(&[(
                "producer".to_owned(),
                "talkback".to_owned(),
                "mic".to_owned(),
            )]))
    }
}

/// What the Patch List view has to show.
///
/// A double-booked role is the one thing that refuses a list — and it
/// is precisely the thing the engineer has to see before a take (spec
/// #48, story 24), so it is a STATE of this view rather than an absent
/// view: hiding the only panel that could explain the refusal would
/// invert what the refusal is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Panel {
    /// The album's plan, resolved.
    Plan(Box<Table>),
    /// There is a list and it cannot be shown, with why.
    Problem(String),
    /// This project is not part of an album.
    Absent,
}

impl Panel {
    /// The panel for a project: the album file found by walking up from
    /// its directory, resolved against the machine's active studio
    /// profile.
    ///
    /// A machine with no profile is not a problem: the table builds
    /// against an empty profile and every role shows unresolved, which
    /// is exactly what a list written in another room should look like
    /// here.
    // r[impl flow.patch-list.project-level]
    // r[impl flow.patch-list.studio-profiles]
    #[must_use]
    pub fn for_project(project: &std::path::Path) -> Self {
        let Some(album) = project.parent().and_then(patch_list::find_album) else {
            return Self::Absent;
        };
        let text = match std::fs::read_to_string(&album) {
            Ok(text) => text,
            Err(error) => return Self::problem("the album's patch list did not read", &error),
        };
        let list = match PatchList::from_styx(&text) {
            Ok(list) => list,
            Err(error) => return Self::problem("the album's patch list did not parse", &error),
        };
        let (studio, profile) = patch_list::Studios::in_config_dir()
            .and_then(|studios| studios.active())
            .unwrap_or_else(|| ("none".to_owned(), StudioProfile::default()));
        match Table::build(&studio, &list, &profile) {
            Ok(table) => Self::Plan(Box::new(table)),
            Err(error) => Self::problem("the album's patch list is not valid", &error),
        }
    }

    /// One `warn` line — the refusal is alertable — and the same words
    /// in the view.
    fn problem(what: &'static str, error: &dyn std::fmt::Display) -> Self {
        tracing::warn!(patch.problem = what, error = %error, "patch list");
        Self::Problem(format!("{what}: {error}"))
    }

    /// The table, when there is one.
    #[must_use]
    pub const fn table(&self) -> Option<&Table> {
        match self {
            Self::Plan(table) => Some(table),
            Self::Problem(_) | Self::Absent => None,
        }
    }
}

/// How a resolution reads in the Input column.
fn describe(resolved: &Resolved) -> String {
    match resolved {
        // One-based for a human: the profile counts channels from zero
        // because the daw's `RecordInput` does, and nobody patching a
        // room counts that way.
        Resolved::Audio { channel } => format!("ch {}", channel.saturating_add(1)),
        Resolved::Midi { device, channel } => channel.as_ref().map_or_else(
            || format!("MIDI {device} all ch"),
            |channel| format!("MIDI {device} ch {channel}"),
        ),
        Resolved::Pair { left, right } => {
            format!("out {}/{}", left.saturating_add(1), right.saturating_add(1))
        }
        Resolved::Unresolved => "unresolved".to_owned(),
    }
}

const fn mark(resolved: &Resolved) -> Mark {
    match resolved {
        Resolved::Unresolved => Mark::Unresolved,
        _ => Mark::Resolved,
    }
}

/// The row height.
const ROW_H: f64 = 20.0;
/// A performer header's height.
const HEADER_H: f64 = 28.0;
/// The gutter down each side.
const PAD: f64 = 16.0;
/// The stale banner's height.
const STALE_BANNER_H: f64 = 22.0;
/// The width of an overridden row's left-edge marker.
const OVERRIDE_MARKER_W: f64 = 4.0;
/// Where each column starts, from the left edge of the table.
const COL_KIND: f64 = 24.0;
const COL_KEY: f64 = 132.0;
const COL_ROLE: f64 = 340.0;
const COL_INPUT: f64 = 560.0;
/// The type sizes.
const TITLE_SIZE: f32 = 17.0;
const HEADER_SIZE: f32 = 14.0;
const ROW_SIZE: f32 = 12.5;

/// Draw whichever state the view is in: the plan, why there is no plan,
/// or a line saying this project has no album file.
// r[impl flow.patch-list.plan]
pub fn paint_panel(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    panel: &Panel,
    origin: (f64, f64),
    width: f64,
) {
    let (x0, y0) = origin;
    match panel {
        Panel::Plan(table) => paint(painter, palette, font, table, origin, width),
        // A refusal is drawn in the same colour an unresolved row is,
        // because it is the same kind of thing to look at.
        Panel::Problem(why) => crate::tcp::glyphs(
            painter,
            font,
            palette.rec,
            why,
            x0 + PAD,
            y0 + PAD + f64::from(ROW_SIZE),
            ROW_SIZE,
        ),
        Panel::Absent => crate::tcp::glyphs(
            painter,
            font,
            palette.text_dim,
            "No patch list for this album — add patch-list.styx beside its sessions",
            x0 + PAD,
            y0 + PAD + f64::from(ROW_SIZE),
            ROW_SIZE,
        ),
    }
}

/// Draw the table into a scene, top-left at `origin`, `width` wide.
// r[impl flow.patch-list.plan]
pub fn paint(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    table: &Table,
    origin: (f64, f64),
    width: f64,
) {
    let (x0, _) = origin;
    let mut y = heading(painter, palette, font, table, origin, width);
    y = columns(
        painter,
        palette,
        font,
        ["Source", "Mic", "Role", "Channel"],
        x0,
        y,
        width,
    );
    y = performers(painter, palette, font, table, (x0, y), width);
    y += 12.0;
    y = columns(
        painter,
        palette,
        font,
        ["Headphones", "For", "Role", "Output"],
        x0,
        y,
        width,
    );
    y = buses(painter, palette, font, table, (x0, y), width);
    unpatched(painter, palette, font, table, (x0, y), width);
}

/// The title, the room's summary, and — when the session is stale — the
/// banner (`flow.patch-list.apply`: never auto-applies, only shown).
/// Returns the y under everything drawn.
fn heading(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    table: &Table,
    origin: (f64, f64),
    width: f64,
) -> f64 {
    let (x0, y0) = origin;
    let y = y0 + PAD;
    let baseline = y + f64::from(TITLE_SIZE);
    crate::tcp::glyphs(
        painter,
        font,
        palette.text,
        "Patch List",
        x0 + PAD,
        baseline,
        TITLE_SIZE,
    );
    // The count is the whole point of the summary, so it is coloured
    // like the rows it counts: nothing unresolved reads as chrome, one
    // unresolved role reads as the thing to look at.
    let (summary, color) = if table.unresolved == 0 {
        (
            format!("{} — every role resolved", table.studio),
            palette.text_dim,
        )
    } else {
        (
            format!("{} — {} unresolved", table.studio, table.unresolved),
            palette.rec,
        )
    };
    crate::tcp::glyphs(
        painter,
        font,
        color,
        &summary,
        x0 + PAD + 120.0,
        baseline,
        ROW_SIZE,
    );
    let mut y = y + f64::from(TITLE_SIZE) + 12.0;
    if let Some(banner) = &table.stale {
        fill(
            painter,
            palette.rec,
            Rect::new(x0 + PAD, y, x0 + width - PAD, y + STALE_BANNER_H),
        );
        crate::tcp::glyphs(
            painter,
            font,
            palette.surface,
            &format!("Stale — {banner}"),
            x0 + PAD + 8.0,
            y + STALE_BANNER_H - 8.0,
            ROW_SIZE,
        );
        y += STALE_BANNER_H + 8.0;
    }
    y
}

/// Every performer's header and rig, returning the y under them.
fn performers(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    table: &Table,
    origin: (f64, f64),
    width: f64,
) -> f64 {
    let (x0, mut y) = origin;
    // One running count across the whole table, so the banding does not
    // restart at each performer and make two adjacent rigs look like
    // one block.
    let mut striped = 0_usize;
    for performer in &table.performers {
        fill(
            painter,
            palette.tcp_column,
            Rect::new(x0 + PAD, y, x0 + width - PAD, y + HEADER_H),
        );
        crate::tcp::glyphs(
            painter,
            font,
            palette.text,
            &performer.name,
            x0 + PAD + 8.0,
            y + HEADER_H - 9.0,
            HEADER_SIZE,
        );
        y += HEADER_H;
        for row in &performer.rows {
            line(
                painter,
                palette,
                font,
                &Line {
                    cells: [&row.kind, &row.key, &row.role, &row.input],
                    mark: row.mark,
                    striped: striped.is_multiple_of(2),
                    overridden: row.overridden,
                    unused: row.unused,
                },
                (x0, y),
                width,
            );
            striped = striped.wrapping_add(1);
            y += ROW_H;
        }
    }
    y
}

/// The headphone buses, returning the y under them.
fn buses(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    table: &Table,
    origin: (f64, f64),
    width: f64,
) -> f64 {
    let (x0, mut y) = origin;
    for (index, bus) in table.buses.iter().enumerate() {
        line(
            painter,
            palette,
            font,
            &Line {
                cells: [&bus.name, &bus.audience, &bus.role, &bus.input],
                mark: bus.mark,
                striped: index.is_multiple_of(2),
                overridden: bus.overridden,
                // A headphone bus is never a track's input, so apply
                // never marks one unused — only rig entries are.
                unused: false,
            },
            (x0, y),
            width,
        );
        y += ROW_H;
    }
    y
}

/// The session's unpatched tracks — a view state, never acted on by
/// apply (`flow.patch-list.apply`). Nothing is drawn when there are
/// none.
fn unpatched(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    table: &Table,
    origin: (f64, f64),
    width: f64,
) {
    if table.unpatched.is_empty() {
        return;
    }
    let (x0, mut y) = origin;
    y += 12.0;
    y = columns(
        painter,
        palette,
        font,
        ["Unpatched", "", "", ""],
        x0,
        y,
        width,
    );
    for name in &table.unpatched {
        crate::tcp::glyphs(
            painter,
            font,
            palette.text_dim,
            name,
            x0 + PAD + COL_KIND,
            y + ROW_H - 6.0,
            ROW_SIZE,
        );
        y += ROW_H;
    }
}

/// One row of the table, whichever half it is in.
struct Line<'a> {
    /// The four columns, left to right.
    cells: [&'a str; 4],
    /// Whether the room resolved it — the last column's colour.
    mark: Mark,
    /// Which band this row takes.
    striped: bool,
    /// Whether the session override replaced this row
    /// (`flow.patch-list.session-override`) — a marker on the left
    /// edge, not a different band, so the resolved/unresolved colour
    /// still reads.
    overridden: bool,
    /// Whether the session has no track for this row
    /// (`flow.patch-list.apply`) — a marker on the right edge, the
    /// override marker's mirror.
    unused: bool,
}

/// Draw one row: its band, then its four cells.
fn line(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    row: &Line<'_>,
    origin: (f64, f64),
    width: f64,
) {
    let (x0, y) = origin;
    let band = if row.striped {
        palette.row_a
    } else {
        palette.row_b
    };
    fill(
        painter,
        band,
        Rect::new(x0 + PAD, y, x0 + width - PAD, y + ROW_H),
    );
    if row.overridden {
        // A marker on the row's own left edge — not a different band,
        // so the resolved/unresolved colour in the last column still
        // reads for what it is.
        fill(
            painter,
            palette.accent,
            Rect::new(x0 + PAD, y, x0 + PAD + OVERRIDE_MARKER_W, y + ROW_H),
        );
    }
    if row.unused {
        // The override marker's mirror, on the right edge: the session
        // has no track for this entry — a view state, never acted on
        // by apply (`flow.patch-list.apply`).
        fill(
            painter,
            palette.text_faint,
            Rect::new(
                x0 + width - PAD - OVERRIDE_MARKER_W,
                y,
                x0 + width - PAD,
                y + ROW_H,
            ),
        );
    }
    let baseline = y + ROW_H - 6.0;
    let last = match row.mark {
        Mark::Resolved => palette.text,
        Mark::Unresolved => palette.rec,
    };
    // The first column names the thing, the middle two qualify it, the
    // last is the answer — so the middle two are dimmer than either end.
    let colors = [palette.text_dim, palette.text, palette.text_dim, last];
    for ((body, color), x) in row
        .cells
        .iter()
        .zip(colors)
        .zip([COL_KIND, COL_KEY, COL_ROLE, COL_INPUT])
    {
        cell(painter, font, color, body, x0 + x, baseline);
    }
}

/// A column header strip, returning the y under it.
fn columns(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    names: [&str; 4],
    x0: f64,
    y: f64,
    width: f64,
) -> f64 {
    let baseline = y + ROW_H - 6.0;
    for (name, x) in names
        .into_iter()
        .zip([COL_KIND, COL_KEY, COL_ROLE, COL_INPUT])
    {
        cell(painter, font, palette.text_faint, name, x0 + x, baseline);
    }
    fill(
        painter,
        palette.tcp_rule,
        Rect::new(x0 + PAD, y + ROW_H, x0 + width - PAD, y + ROW_H + 1.0),
    );
    y + ROW_H + 1.0
}

fn cell(
    painter: &mut impl PaintScene,
    font: &Font,
    color: Color,
    body: &str,
    x: f64,
    baseline: f64,
) {
    crate::tcp::glyphs(painter, font, color, body, x, baseline, ROW_SIZE);
}

fn fill(painter: &mut impl PaintScene, color: Color, rect: Rect) {
    painter.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
}

/// Render the view headlessly into RGBA8 bytes.
///
/// `None` when this box has no usable GPU — the picture is a fixture,
/// and a software fallback would render a different one.
#[must_use]
pub fn shot(table: &Table, size: (u32, u32)) -> Option<Vec<u8>> {
    use anyrender::ImageRenderer;

    let (width, height) = size;
    let theme = daw_ui::theming::Theme::dark();
    let palette = Palette::from_theme(&theme);
    let font = Font::embedded().ok()?;
    let mut renderer = anyrender_vello::VelloImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    renderer.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.surface,
                None,
                &Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
            paint(
                painter,
                &palette,
                &font,
                table,
                (0.0, 0.0),
                f64::from(width),
            );
        },
        &mut buffer,
    );
    Some(buffer)
}
