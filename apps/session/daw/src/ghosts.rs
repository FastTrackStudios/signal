//! Everyone else in the session, drawn faintly over the arrangement.
//!
//! Each remote peer is a ghost in their own colour: their time selection
//! as a wash, their edit cursor as a thin line, their play cursor (when
//! transports are independent) as a faint line with no trail, the items
//! they have selected outlined, and their mouse — an arrow with their
//! name — on the beat and the track it is over. Faint on purpose: this is
//! what other people are doing, and it must never be mistaken for, or
//! draw over, what YOU are doing.
//!
//! The collaboration driver publishes the roster here ([`publish`]); the
//! arrangement paints it every frame ([`paint`]), interpolating pointers
//! and extrapolating play cursors to the moment being drawn.

use std::collections::HashMap;
use std::sync::Mutex;

use session::sync::presence::{PeerState, Pointer, Roster};
use vello::kurbo::{Affine, BezPath, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};

use crate::arrangement::{Arrangement, Palette, Viewport};
use crate::text::Font;

struct Published {
    roster: Roster,
    /// The roster as last published, in words, and when it last changed:
    /// the driver publishes every tick whether or not anyone moved.
    said: String,
    changed: web_time::Instant,
    /// Local clock → shared clock, milliseconds.
    clock_offset_ms: f64,
    /// Whose play cursors to draw (independent transports only).
    show_play: bool,
}

static ROSTER: Mutex<Option<Published>> = Mutex::new(None);

/// Publish the roster the arrangement should draw. `None` clears it
/// (the session was left).
pub fn publish(roster: Option<Roster>, clock_offset_ms: f64, show_play: bool) {
    if let Ok(mut slot) = ROSTER.lock() {
        *slot = roster.map(|roster| {
            let said = format!("{roster:?}");
            let changed = match slot.as_ref() {
                Some(was) if was.said == said => was.changed,
                _ => web_time::Instant::now(),
            };
            Published {
                roster,
                said,
                changed,
                clock_offset_ms,
                show_play,
            }
        });
    }
}

/// How long after the roster last changed its pointers may still be
/// gliding: their interpolation delay, and some.
const GLIDING: std::time::Duration = std::time::Duration::from_millis(600);

/// Someone else in the session, and the song they are on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    /// Their song's project here (what the tabs know it by).
    pub project: String,
    /// Their song's name everywhere (`Washed`).
    pub song: String,
    pub name: String,
    pub color: u32,
}

/// Everyone else in the session.
static EVERYONE: Mutex<Vec<Person>> = Mutex::new(Vec::new());

/// Publish who is on which song.
pub fn publish_everyone(everyone: Vec<Person>) {
    if let Ok(mut slot) = EVERYONE.lock()
        && *slot != everyone
    {
        *slot = everyone;
    }
}

/// Everyone else in the session.
#[must_use]
pub fn everyone() -> Vec<Person> {
    EVERYONE.lock().map(|all| all.clone()).unwrap_or_default()
}

/// The people on the song that is `project` here: (name, colour).
#[must_use]
pub fn on_song(project: &str) -> Vec<(String, u32)> {
    EVERYONE
        .lock()
        .map(|all| {
            all.iter()
                .filter(|p| p.project == project)
                .map(|p| (p.name.clone(), p.color))
                .collect()
        })
        .unwrap_or_default()
}

/// Anyone moving: the arrangement keeps repainting while someone's
/// pointer glides or their play cursor runs — and only then. Someone
/// merely present, still, needs no frames.
#[must_use]
pub fn active() -> bool {
    ROSTER.lock().ok().is_some_and(|slot| {
        slot.as_ref().is_some_and(|p| {
            !p.roster.peers.is_empty()
                && (p.changed.elapsed() < GLIDING
                    || (p.show_play
                        && p.roster
                            .peers
                            .values()
                            .any(|peer| peer.play.as_ref().is_some_and(|play| play.playing))))
        })
    })
}

/// What this peer is doing, as the arrangement last saw it — read by the
/// collaboration driver, which decides what to tell the others and when.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Local {
    /// The mouse: over the lanes a time and a track, anywhere else a
    /// place in a named panel ([`local_window_pointer`]).
    pub pointer: Option<Pointer>,
    pub selected_items: Vec<String>,
    pub selected_tracks: Vec<String>,
}

static LOCAL: Mutex<Local> = Mutex::new(Local {
    pointer: None,
    selected_items: Vec::new(),
    selected_tracks: Vec::new(),
});

/// Whether the mouse is over the lanes, where the arrangement reports it
/// as a time and a track — finer than any panel position.
static OVER_LANES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The mouse moved over the arrangement: a time and a track over the
/// lanes, `None` off them (the window overlay reports it from there).
/// The panel id of the arrangement's track panel: a pointer over it is
/// anchored to a track row there, and the arrangement paints it (a DOM
/// overlay would draw under the arrangement's canvas).
pub const TCP: &str = "tcp";

/// The arrangement's own rectangle, as a panel: a pointer over it (lanes
/// or track panel) is the arrangement's to publish.
pub const ARRANGEMENT: &str = "arrangement";

/// The pointer is over the arrangement's track panel: on `track` (`None`:
/// its header, above the rows), `u` across the panel, `v` down the row.
pub fn local_tcp_pointer(track: Option<String>, u: f64, v: f64) {
    OVER_LANES.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut local) = LOCAL.lock() {
        local.pointer = Some(Pointer::Anchor {
            panel: TCP.into(),
            key: track.unwrap_or_default(),
            u: u.clamp(0.0, 1.0),
            v,
        });
    }
}

pub fn local_pointer(pointer: Option<(f64, Option<String>, f64)>) {
    OVER_LANES.store(pointer.is_some(), std::sync::atomic::Ordering::Relaxed);
    if let Some((at, track, y)) = pointer
        && let Ok(mut local) = LOCAL.lock()
    {
        local.pointer = Some(Pointer::Timeline { at, track, y });
    }
}

// ── panels: where each named part of the window is ──────────────────────

/// A panel's rectangle in the window, logical pixels: x, y, width, height.
pub type PanelRect = (f64, f64, f64, f64);

thread_local! {
    static REGION_NODES: std::cell::RefCell<HashMap<String, std::rc::Rc<dioxus::prelude::MountedData>>> =
        std::cell::RefCell::new(HashMap::new());
    static REGION_RECTS: std::cell::RefCell<HashMap<String, PanelRect>> = std::cell::RefCell::new(HashMap::new());
}

/// A panel mounted: remember its node, measured by [`measure_regions`].
/// For `onmounted` on the panel's outermost element.
pub fn region_mounted(id: &str, node: std::rc::Rc<dioxus::prelude::MountedData>) {
    REGION_NODES.with(|n| n.borrow_mut().insert(id.to_owned(), node));
}

/// Re-measure every panel (they move with the window and the layout).
pub async fn measure_regions() {
    let nodes: Vec<(String, std::rc::Rc<dioxus::prelude::MountedData>)> = REGION_NODES.with(|n| {
        n.borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    });
    for (id, node) in nodes {
        match node.get_client_rect().await {
            Ok(r) if r.size.width > 0.0 && r.size.height > 0.0 => {
                REGION_RECTS.with(|m| {
                    m.borrow_mut()
                        .insert(id, (r.origin.x, r.origin.y, r.size.width, r.size.height))
                });
            }
            // Unmounted, or laid out to nothing: not somewhere to point.
            _ => {
                REGION_RECTS.with(|m| m.borrow_mut().remove(&id));
            }
        }
    }
}

/// A panel that anchors pointers to its content: what is under a point
/// of it, and where that thing is now. Coordinates are logical pixels
/// relative to the panel's own rectangle (see [`region_mounted`], under
/// the same name).
pub trait Anchor {
    /// The content under `(x, y)`: a key and a place on it (`None` over
    /// nothing in particular — the pointer is then placed in the panel).
    /// `size` is the panel's width and height.
    fn anchor(&self, x: f64, y: f64, size: (f64, f64)) -> Option<(String, f64, f64)>;
    /// Where that content is in this view now (`None`: not shown).
    fn place(&self, key: &str, u: f64, v: f64, size: (f64, f64)) -> Option<(f64, f64)>;
}

thread_local! {
    static ANCHORS: std::cell::RefCell<HashMap<String, std::rc::Rc<dyn Anchor>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// A panel's content anchoring, under its region name.
pub fn register_anchor(panel: &str, anchor: std::rc::Rc<dyn Anchor>) {
    ANCHORS.with(|a| a.borrow_mut().insert(panel.to_owned(), anchor));
}

fn anchor_for(panel: &str) -> Option<std::rc::Rc<dyn Anchor>> {
    ANCHORS.with(|a| a.borrow().get(panel).cloned())
}

/// Where a named panel is in this window.
#[must_use]
pub fn region_rect(id: &str) -> Option<PanelRect> {
    REGION_RECTS.with(|m| m.borrow().get(id).copied())
}

/// The mouse moved in the window (logical pixels): which panel it is
/// over and where in it, or where in the window. Over the lanes the
/// arrangement's own report wins.
pub fn local_window_pointer(x: f64, y: f64, window: (f64, f64)) {
    // Over the arrangement, the arrangement publishes the pointer (its
    // lanes and its track panel both). Off it, the window does — which is
    // how leaving it for the mixer below lets go of the lane it was on.
    let inside = |(rx, ry, rw, rh): PanelRect| x >= rx && y >= ry && x < rx + rw && y < ry + rh;
    match region_rect(ARRANGEMENT).map(inside) {
        Some(true) => return,
        Some(false) => OVER_LANES.store(false, std::sync::atomic::Ordering::Relaxed),
        // Not measured yet: trust the arrangement's own word.
        None if OVER_LANES.load(std::sync::atomic::Ordering::Relaxed) => return,
        None => {}
    }
    let hit = REGION_RECTS
        .with(|m| {
            m.borrow()
                .iter()
                .filter(|(id, _)| id.as_str() != ARRANGEMENT)
                .filter(|(_, (rx, ry, rw, rh))| x >= *rx && y >= *ry && x < rx + rw && y < ry + rh)
                // The smallest panel under the point is the one it is in.
                .min_by(|a, b| (a.1.2 * a.1.3).total_cmp(&(b.1.2 * b.1.3)))
                .map(|(id, rect)| (id.clone(), *rect))
        })
        .map(|(id, (rx, ry, rw, rh))| {
            // Anchored to the content when the panel can say what is there;
            // else a place in the panel.
            anchor_for(&id)
                .and_then(|a| a.anchor(x - rx, y - ry, (rw, rh)))
                .map_or_else(
                    || Pointer::Region {
                        region: id.clone(),
                        x: (x - rx) / rw,
                        y: (y - ry) / rh,
                    },
                    |(key, u, v)| Pointer::Anchor {
                        panel: id.clone(),
                        key,
                        u,
                        v,
                    },
                )
        });
    let pointer = hit.unwrap_or_else(|| Pointer::Region {
        region: "window".into(),
        x: x / window.0.max(1.0),
        y: y / window.1.max(1.0),
    });
    if let Ok(mut local) = LOCAL.lock() {
        local.pointer = Some(pointer);
    }
}

/// The mouse left the window.
pub fn local_window_left() {
    OVER_LANES.store(false, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut local) = LOCAL.lock() {
        local.pointer = None;
    }
}

/// Everyone else's pointer that is not over the lanes (those the
/// arrangement draws), placed in this window: (x, y, name, colour).
#[must_use]
pub fn window_pointers(window: (f64, f64)) -> Vec<(f64, f64, String, u32)> {
    let Ok(slot) = ROSTER.lock() else {
        return Vec::new();
    };
    let Some(published) = slot.as_ref() else {
        return Vec::new();
    };
    let now = now_ms();
    published
        .roster
        .peers
        .values()
        .filter_map(|peer| {
            let state = peer.state.as_ref()?;
            // Over the lanes or the track panel, the arrangement draws it.
            if drawn_by_arrangement(peer.trail.latest()?) {
                return None;
            }
            let (x, y) = peer.trail.screen_at(now, |p| match p {
                Pointer::Region { region, x, y } => {
                    let (rx, ry, rw, rh) = if region == "window" {
                        (0.0, 0.0, window.0, window.1)
                    } else {
                        // A panel this window is not showing: nowhere.
                        region_rect(region)?
                    };
                    Some((x.mul_add(rw, rx), y.mul_add(rh, ry)))
                }
                Pointer::Anchor { panel, key, u, v } => {
                    let (rx, ry, rw, rh) = region_rect(panel)?;
                    let (px, py) = anchor_for(panel)?.place(key, *u, *v, (rw, rh))?;
                    // Only where this window shows it: a measure on a page
                    // (or a strip scrolled) out of view is not drawn at all
                    // — never over whatever lies past the panel's edge.
                    let inside = (0.0..=rw).contains(&px) && (0.0..=rh).contains(&py);
                    inside.then_some((rx + px, ry + py))
                }
                _ => None,
            })?;
            Some((x, y, state.name.clone(), state.color))
        })
        .collect()
}

/// A pointer the arrangement paints itself: over its lanes, or over its
/// track panel.
fn drawn_by_arrangement(pointer: &Pointer) -> bool {
    match pointer {
        Pointer::Timeline { .. } => true,
        Pointer::Anchor { panel, .. } => panel == TCP,
        _ => false,
    }
}

/// The selection, as drawn this frame.
pub fn local_selection(
    items: &std::collections::HashSet<String>,
    rows: &[(daw_proto::Track, u32)],
) {
    if let Ok(mut local) = LOCAL.lock() {
        let mut items: Vec<String> = items.iter().cloned().collect();
        items.sort();
        let tracks: Vec<String> = rows
            .iter()
            .filter(|(t, _)| t.selected)
            .map(|(t, _)| t.guid.clone())
            .collect();
        if local.selected_items != items {
            local.selected_items = items;
        }
        if local.selected_tracks != tracks {
            local.selected_tracks = tracks;
        }
    }
}

/// The arrangement's track guids and item guids as last drawn (the
/// collaboration puppet points at real ones).
static SHOWN: Mutex<(Vec<String>, Vec<String>, Vec<String>)> =
    Mutex::new((Vec::new(), Vec::new(), Vec::new()));

pub(crate) fn local_shown(rows: &[(daw_proto::Track, u32)], scene: &Arrangement) {
    if let Ok(mut shown) = SHOWN.lock()
        && shown.0.len() != rows.len()
    {
        shown.0 = rows.iter().map(|(t, _)| t.guid.clone()).collect();
        shown.1 = scene.item_boxes().iter().map(|i| i.guid.clone()).collect();
        shown.2 = rows.iter().map(|(t, _)| t.name.clone()).collect();
    }
}

#[must_use]
pub fn local_rows() -> Vec<String> {
    SHOWN.lock().map(|s| s.0.clone()).unwrap_or_default()
}

/// A shown track's name, by guid.
#[must_use]
pub fn local_track_name(guid: &str) -> Option<String> {
    let shown = SHOWN.lock().ok()?;
    let i = shown.0.iter().position(|g| g == guid)?;
    shown.2.get(i).cloned()
}

#[must_use]
pub fn local_item_guids() -> Vec<String> {
    SHOWN.lock().map(|s| s.1.clone()).unwrap_or_default()
}

/// This peer's presence as the arrangement last saw it.
#[must_use]
pub fn local() -> Local {
    LOCAL.lock().map(|l| l.clone()).unwrap_or_default()
}

/// Milliseconds on this machine's clock, for presence timing.
#[must_use]
pub fn now_ms() -> f64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// How strongly a ghost is drawn, against the local cursor's 1.0.
const FAINT: f32 = 0.55;
const NAME_SIZE: f32 = 11.0;

fn color_of(state: &PeerState) -> Color {
    let [r, g, b] = [
        (state.color >> 16) & 0xff,
        (state.color >> 8) & 0xff,
        state.color & 0xff,
    ]
    .map(|c| u8::try_from(c).unwrap_or(0));
    Color::from_rgba8(r, g, b, 0xff)
}

/// Paint every remote peer. `rows` is the arrangement's row → track list,
/// `lanes_at` the lanes' origin (as the selection overlay takes it), and
/// `height` how far down lines reach.
#[allow(clippy::too_many_arguments)] // the arrangement's own paint context
pub fn paint(
    painter: &mut impl anyrender::PaintScene,
    palette: &Palette,
    font: &Font,
    scene: &Arrangement,
    rows: &[(daw_proto::Track, u32)],
    view: Viewport,
    lanes_at: (f64, f64),
    height: f64,
) {
    let Ok(slot) = ROSTER.lock() else { return };
    let Some(published) = slot.as_ref() else {
        return;
    };
    let local_now = now_ms();
    let shared_now = local_now + published.clock_offset_ms;

    let left = scene.tcp.width();
    let x_of = |seconds: f64| seconds.mul_add(view.pps, left - view.scroll_x);
    let row_of: HashMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, (t, _))| (t.guid.as_str(), i))
        .collect();
    let (_, oy) = lanes_at;
    let band = |row: usize| {
        scene
            .row_box(row)
            .map(|(top, h)| (top.mul_add(view.zoom_y, oy), h * view.zoom_y))
    };

    for peer in published.roster.peers.values() {
        let Some(state) = &peer.state else { continue };
        let color = color_of(state);

        // Their time selection, a wash fainter than yours.
        if let Some((start, end)) = state.time_selection {
            let (x0, x1) = (x_of(start).max(left), x_of(end).max(left));
            if x1 > x0 {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color.with_alpha(0.10),
                    None,
                    &Rect::new(x0, 0.0, x1, height),
                );
            }
        }
        // Their selected items, outlined in their colour.
        for item in scene
            .item_boxes()
            .iter()
            .filter(|i| state.selected_items.contains(&i.guid))
        {
            let Some((top, h)) = band(item.row) else {
                continue;
            };
            let r = Rect::new(
                item.x0.mul_add(view.pps, lanes_at.0),
                top + 1.0,
                item.x1.mul_add(view.pps, lanes_at.0),
                top + h - 1.0,
            );
            painter.stroke(
                &Stroke::new(1.5),
                Affine::IDENTITY,
                color.with_alpha(FAINT),
                None,
                &r,
            );
        }
        // Their selected tracks: a stripe down the lanes' left edge.
        for guid in &state.selected_tracks {
            let Some((top, h)) = row_of.get(guid.as_str()).and_then(|r| band(*r)) else {
                continue;
            };
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color.with_alpha(FAINT),
                None,
                &Rect::new(left, top, left + 3.0, top + h),
            );
        }
        // Their edit cursor.
        if let Some(at) = state.edit_cursor {
            let x = x_of(at);
            if x >= left {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color.with_alpha(FAINT),
                    None,
                    &Rect::new(x, 0.0, x + 1.0, height),
                );
            }
        }
        // Their play cursor, when everyone plays on their own.
        if published.show_play
            && let Some(play) = peer.play
        {
            let x = x_of(play.position_at(shared_now));
            if x >= left {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color.with_alpha(if play.playing { 0.45 } else { 0.25 }),
                    None,
                    &Rect::new(x - 0.75, 0.0, x + 0.75, height),
                );
            }
        }
        // Their mouse, with their name: exactly where it is in the row,
        // interpolated on this screen so it glides from row to row.
        if peer.trail.latest().is_some_and(drawn_by_arrangement)
            && let Some((x, y)) = peer.trail.screen_at(local_now, |p| match p {
                // Over the track panel: across it, and down the row.
                Pointer::Anchor { panel, key, u, v } if panel == TCP => {
                    let y = if key.is_empty() {
                        v * crate::ruler::ruler_h()
                    } else {
                        let (top, h) = row_of.get(key.as_str()).and_then(|r| band(*r))?;
                        v.mul_add(h, top)
                    };
                    let lanes_top = if key.is_empty() { 0.0 } else { oy };
                    let x = u * left;
                    ((0.0..=left).contains(&x) && (lanes_top..=height).contains(&y))
                        .then_some((x, y))
                }
                Pointer::Timeline { at, track, y } => {
                    let (x, y) = match track {
                        // Over a track: in its row, if this view shows the
                        // row — not scrolled under the ruler, not past the
                        // lanes' foot (where, in a shorter arrangement, the
                        // mixer or another panel is).
                        Some(guid) => {
                            let (top, h) = row_of.get(guid.as_str()).and_then(|r| band(*r))?;
                            (x_of(*at), y.mul_add(h, top))
                        }
                        None => (x_of(*at), y * crate::ruler::ruler_h()),
                    };
                    let lanes_top = if track.is_some() { oy } else { 0.0 };
                    ((left..=view.width).contains(&x) && (lanes_top..=height).contains(&y))
                        .then_some((x, y))
                }
                _ => None,
            })
        {
            paint_pointer(painter, palette, font, &state.name, color, Point::new(x, y));
        }
    }
}

fn paint_pointer(
    painter: &mut impl anyrender::PaintScene,
    palette: &Palette,
    font: &Font,
    name: &str,
    color: Color,
    at: Point,
) {
    // An arrow pointing up-left, its tip on the spot.
    let mut arrow = BezPath::new();
    arrow.move_to(at);
    arrow.line_to((at.x, at.y + 14.0));
    arrow.line_to((at.x + 4.0, at.y + 10.5));
    arrow.line_to((at.x + 9.5, at.y + 11.0));
    arrow.close_path();
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color.with_alpha(0.9),
        None,
        &arrow,
    );
    painter.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        palette.text.with_alpha(0.6),
        None,
        &arrow,
    );

    if name.is_empty() {
        return;
    }
    let width = font.width(name, NAME_SIZE);
    let tag = Rect::new(at.x + 10.0, at.y + 12.0, at.x + 18.0 + width, at.y + 27.0);
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color.with_alpha(0.85),
        None,
        &RoundedRect::from_rect(tag, 3.0),
    );
    let mut text = anyrender::Scene::new();
    crate::tcp::glyphs(
        &mut text,
        font,
        Color::from_rgba8(0x10, 0x10, 0x14, 0xff),
        name,
        tag.x0 + 4.0,
        tag.y1 - 4.0,
        NAME_SIZE,
    );
    for command in &text.commands {
        crate::arrangement::submit_command(painter, command, Affine::IDENTITY);
    }
}
