//! What the window says when it will not do a thing.
//!
//! An edit that cannot honestly be made carries a sentence saying why —
//! [`crate::arrange_edit::Effect::Refused`]. Until this existed the
//! sentence went to the log, which is to say nowhere: on the golden
//! session every row is a folded folder, so every item drag was refused
//! and the window looked like one whose item editing did not work. The
//! logic was right and invisible, which from the hand's point of view
//! is the same as being wrong.
//!
//! It is drawn ON the row it is about rather than in a corner. A
//! refusal is always about a particular row — "the takes under THIS row
//! do not line up" — and a corner toast makes the reader carry the
//! message back to the row themselves, which on a session tall enough
//! to need scrolling is a hunt.
//!
//! And it goes away. A refusal is a reply to a gesture, not a state the
//! session is in; something that stayed would have to be dismissed, and
//! a dismissal is another thing to do with a mouse that is already
//! mid-edit.

use vello::kurbo::{Affine, Rect, RoundedRect};
use vello::peniko::Fill;

use crate::arrangement::Palette;
use crate::text::Font;

/// How long the notice reads at full strength before it starts to go.
const HOLD: f64 = 3.2;

/// And how long it takes to go, once it starts.
const FADE: f64 = 0.8;

/// The type size the message is set at.
const SIZE: f32 = 12.0;

/// The gap between baselines, for a message that wraps.
const LEADING: f64 = 15.0;

/// Padding inside the notice's box.
const PAD: f64 = 8.0;

/// A refused edit, saying why, on the row it happened on.
#[derive(Clone, Debug)]
pub struct Notice {
    /// The sentence the refusal carried. Shown as written: it is the
    /// domain's own explanation, and a generic "edit failed" in its
    /// place would throw away the only part worth reading.
    pub why: &'static str,
    /// The folded row it is about, by its folder track guid, when the
    /// refusal knew one.
    pub row: Option<String>,
    /// When it arrived, which is what makes it fade.
    pub since: web_time::Instant,
}

impl Notice {
    /// A notice, now.
    #[must_use]
    pub fn new(why: &'static str, row: Option<String>) -> Self {
        Self {
            why,
            row,
            since: web_time::Instant::now(),
        }
    }

    /// How strongly to draw it, or `None` once it has had its time.
    ///
    /// Separate from painting so the widget can ask whether to keep
    /// requesting frames without drawing one to find out.
    #[must_use]
    pub fn alpha(&self) -> Option<f64> {
        let age = self.since.elapsed().as_secs_f64();
        if age <= HOLD {
            return Some(1.0);
        }
        let gone = (age - HOLD) / FADE;
        (gone < 1.0).then(|| 1.0 - gone)
    }

    /// Whether it is still worth a frame.
    #[must_use]
    pub fn alive(&self) -> bool {
        self.alpha().is_some()
    }
}

/// Break `text` into lines no wider than `room`, on word boundaries.
///
/// Long enough messages are the norm here rather than the exception —
/// the ragged-row refusal is a whole sentence with a suggestion in it,
/// because a refusal that does not say what to do instead is only half
/// an answer. So wrapping is the normal path and not a safety net.
fn wrap(font: &Font, text: &str, size: f32, room: f64) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && font.width(&candidate, size) > room {
            lines.push(std::mem::take(&mut line));
            line = word.to_owned();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The height a notice needs for `why` at a given width.
///
/// Asked before the box is drawn, because the box is sized to the text
/// rather than the text clipped to a box — a refusal with its last
/// clause cut off is a refusal that does not say what to do instead.
#[must_use]
pub fn height(font: &Font, why: &str, room: f64) -> f64 {
    let lines = wrap(font, why, SIZE, room - PAD * 2.0).len().max(1);
    crate::num::coord(lines).mul_add(LEADING, PAD * 2.0)
}

/// Where a notice goes, in the transform the panel passes use.
///
/// Anchored to the left of the lanes rather than to the item that
/// was refused: the item is wherever the drag left the pointer,
/// which on a long take can be off the side of the window, and the
/// row is the thing the message is actually about.
///
/// Clamped into the visible lanes, so scrolling during the fade
/// moves the notice with its row until the row is about to leave
/// and then holds it — a reply that slid off the screen before it
/// was read would be the original bug again, more slowly.
pub fn area(
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(daw_proto::Track, u32)],
    notice: &Notice,
    view: crate::arrangement::Viewport,
) -> Rect {
    const INSET: f64 = 8.0;
    const WIDEST: f64 = 360.0;

    let room = (view.width - view.panel_w - INSET * 2.0).clamp(80.0, WIDEST);
    let tall = height(font, notice.why, room);
    // The row's top in the same space the panel passes draw in —
    // `below` has already been folded into the transform, so this is
    // content and not screen.
    let top = notice
        .row
        .as_ref()
        .and_then(|guid| rows.iter().position(|(track, _)| &track.guid == guid))
        .and_then(|row| scene.row_band(row, view))
        .map_or(view.scroll_y, |(band_top, _)| band_top);
    // The window in content units the lanes are showing, less the
    // notice's own height so it cannot hang off the bottom. The floor
    // is not politeness: `clamp` panics when its bounds cross, and a
    // window shorter than the notice it is showing would cross them.
    let first = view.scroll_y + INSET;
    let last = (view.scroll_y + view.height - crate::ruler::ruler_h() - tall - INSET).max(first);
    let top = (top + 2.0).clamp(first, last);
    let x0 = view.panel_w + INSET;
    Rect::new(x0, top, x0 + room, top + tall)
}

/// Draw the notice into `area`, at `alpha`.
///
/// `area` is where it goes in the widget's own coordinates — the caller
/// works that out from the row, because only the window knows which
/// rows are on screen and how tall they came out.
pub fn paint(
    painter: &mut impl anyrender::PaintScene,
    palette: &Palette,
    font: &Font,
    why: &str,
    area: Rect,
    alpha: f64,
    transform: Affine,
) {
    use anyrender::PaintScene as _;

    let mut scene = anyrender::Scene::new();
    let alpha = crate::num::narrow(alpha.clamp(0.0, 1.0));

    // A dark plate so the message reads over whatever the lane was
    // drawing, and the warn amber down its leading edge so it is
    // recognisable as a refusal before it is read as words.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.tcp_field.with_alpha(0.94 * alpha),
        None,
        &RoundedRect::from_rect(area, 3.0),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.meter_warn.with_alpha(alpha),
        None,
        &Rect::new(area.x0, area.y0, area.x0 + 2.0, area.y1),
    );

    let room = area.width() - PAD * 2.0;
    for (n, line) in wrap(font, why, SIZE, room).iter().enumerate() {
        let baseline = crate::num::coord(n + 1).mul_add(LEADING, area.y0 + PAD - 3.0);
        crate::tcp::glyphs(
            &mut scene,
            font,
            palette.text.with_alpha(alpha),
            line,
            area.x0 + PAD,
            baseline,
            SIZE,
        );
    }

    for command in &scene.commands {
        crate::arrangement::submit_command(painter, command, transform);
    }
}

#[cfg(test)]
mod tests {
    use super::{HOLD, Notice, wrap};

    #[test]
    fn a_fresh_notice_is_at_full_strength() {
        let notice = Notice::new("no", None);
        assert_eq!(notice.alpha(), Some(1.0));
        assert!(notice.alive());
    }

    #[test]
    fn one_that_has_had_its_time_is_gone() {
        let mut notice = Notice::new("no", None);
        notice.since -= std::time::Duration::from_secs_f64(HOLD + 1.0);
        assert_eq!(notice.alpha(), None);
        assert!(!notice.alive());
    }

    #[test]
    fn it_fades_rather_than_vanishing() {
        let mut notice = Notice::new("no", None);
        notice.since -= std::time::Duration::from_secs_f64(HOLD + 0.4);
        let alpha = notice.alpha().expect("still within the fade");
        assert!(alpha > 0.0 && alpha < 1.0, "half-faded, got {alpha}");
    }

    #[test]
    fn wrapping_breaks_on_words_and_keeps_all_of_them() {
        let font = crate::text::Font::embedded().expect("the embedded font");
        let why = "the takes under this row do not line up here, so there is no one \
                   edit to make — open the folder and work on the track that differs";
        let lines = wrap(&font, why, 12.0, 180.0);
        assert!(lines.len() > 1, "a sentence this long has to wrap");
        let back: Vec<&str> = lines.iter().flat_map(|l| l.split(' ')).collect();
        let sent: Vec<&str> = why.split_whitespace().collect();
        assert_eq!(back, sent, "wrapping must not lose or reorder a word");
    }
}
