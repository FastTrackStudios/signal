//! The lyrics panel: the song's words, following the song, in two views.
//!
//! - **Audience** — what a room reads: the words alone, as large as the
//!   panel lets them be, centred on a dark field lit by the section's
//!   colour, a slide (or a line, or a section) at a time. A title card
//!   before the first line; nothing through an instrumental.
//! - **Performer** — what the band reads: where the song is (the
//!   section, how far through, what is next and when) over the lyrics as
//!   a teleprompter that runs itself — the line being sung large, the
//!   ones after it following, the sections labelled as they come.
//! - **Confidence Monitor** — what a singer glances at, as ProPresenter's
//!   stage display has it: the slide on screen now, large, over a rule,
//!   and the next slide under it in its section's colour (yellow when
//!   that is too pale to tell from white).
//!
//! Which view (and the Audience's layer) is a small switch in the panel's
//! top-right corner; everything else is the view.
//!
//! The lines are the song's own: the Lyrics track's items, one a line,
//! labelled with its text ([`session::lyrics::stamp_lines`] puts them
//! there from a synced `.lrc` when a song is prepared). Sections and
//! slides are derived from them against the song's section regions
//! ([`session::lyrics`]). The panel runs ahead of the song — words on a
//! screen are needed before they are sung ([`SWITCH_EARLY`],
//! [`LINE_EARLY`]).
//!
//! Inline styles throughout (see the repo's CLAUDE.md on Blitz).

use std::rc::Rc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_util::StreamExt as _;
use session::lyrics::{Layer, LyricSection, Lyrics, SLIDE_LINES, SectionSpan, Slide};

use crate::shell::{ACCENT, DIM, RULE, TEXT};
use crate::studio::StudioSession;

/// How much sooner than the song a section or a slide comes up: words
/// on a screen have to be there before they are sung, so the change is
/// made ahead, the way a projectionist makes it.
const SWITCH_EARLY: f64 = 1.0;

/// How much sooner a line is lit than it is sung — enough to be read
/// into, not so much that it runs ahead of the singer.
const LINE_EARLY: f64 = 0.4;

/// The three ways the panel shows the words.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Audience,
    Performer,
    Confidence,
}

impl View {
    const ALL: [Self; 3] = [Self::Audience, Self::Performer, Self::Confidence];

    const fn name(self) -> &'static str {
        match self {
            Self::Audience => "Audience",
            Self::Performer => "Performer",
            Self::Confidence => "Confidence Monitor",
        }
    }
}

/// What the panel shows — Audience or Performer, and the Audience's
/// layer. A host provides one above the songs so the choice holds from
/// song to song; without one each panel keeps its own.
#[derive(Clone, Copy, PartialEq)]
pub struct LyricsChoice {
    pub view: Signal<View>,
    pub layer: Signal<Layer>,
}

impl LyricsChoice {
    /// The Confidence Monitor, unless `FTS_LYRICS_VIEW` names another
    /// (`audience`, `performer`) — a screen set up for one view opens on it.
    #[must_use]
    pub fn new() -> Self {
        let view = match std::env::var("FTS_LYRICS_VIEW").as_deref() {
            Ok("audience") => View::Audience,
            Ok("performer") => View::Performer,
            _ => View::Confidence,
        };
        Self {
            view: Signal::new(view),
            layer: Signal::new(Layer::Slide),
        }
    }
}

impl Default for LyricsChoice {
    fn default() -> Self {
        Self::new()
    }
}

/// The song's lyrics as the panel shows them: lines, sections (with the
/// colour of the region each came from), slides, and what the song is
/// called.
#[derive(Clone, PartialEq)]
struct Words {
    lyrics: Lyrics,
    sections: Vec<LyricSection>,
    colors: Vec<String>,
    slides: Vec<Slide>,
    title: String,
    artist: Option<String>,
    song: (f64, f64),
}

impl Words {
    fn of(session: &StudioSession) -> Self {
        let project = &session.project;
        let lyrics = project
            .tracks
            .iter()
            .find(|t| session::lyrics::is_lyrics_track(&t.name))
            .map(|track| {
                Lyrics::from_items(project.lane(&track.guid).iter().filter_map(|item| {
                    Some((
                        item.position.as_seconds(),
                        item.length.as_seconds(),
                        project.title(item)?,
                    ))
                }))
            })
            .unwrap_or_default();
        let mut regions: Vec<_> = project
            .sections
            .iter()
            .filter(|r| r.lane == crate::ruler::SECTIONS_ROW as u32)
            .collect();
        regions.sort_by(|a, b| a.start.total_cmp(&b.start));
        let spans: Vec<SectionSpan> = regions
            .iter()
            .map(|r| SectionSpan {
                name: r.name.clone(),
                start: r.start,
                end: r.end,
            })
            .collect();
        let sections = lyrics.sections(&spans);
        let slides = lyrics.slides(&sections, SLIDE_LINES);
        let meta = session.chart.as_ref().map(|c| &c.metadata);
        let song = (
            spans.first().map_or(0.0, |s| s.start),
            spans.last().map_or(0.0, |s| s.end),
        );
        Self {
            lyrics,
            sections,
            colors: regions
                .iter()
                .map(|r| r.color.clone().unwrap_or_else(|| ACCENT.to_owned()))
                .collect(),
            slides,
            title: meta.and_then(|m| m.title.clone()).unwrap_or_default(),
            artist: meta.and_then(|m| m.artist.clone()),
            song,
        }
    }

    /// When a section comes up on screen: its downbeat, or its first
    /// sung line when that is a pickup ahead of it.
    fn shows_from(&self, section: &LyricSection) -> f64 {
        self.lyrics
            .lines
            .get(section.lines.start)
            .filter(|_| !section.lines.is_empty())
            .map_or(section.start, |first| first.start.min(section.start))
    }

    /// The section shown at `at` — up [`SWITCH_EARLY`] before it comes,
    /// or before its pickup does — or the first before any.
    fn section_at(&self, at: f64) -> Option<usize> {
        self.sections
            .iter()
            .rposition(|s| self.shows_from(s) <= at + SWITCH_EARLY)
            .or_else(|| (!self.sections.is_empty()).then_some(0))
    }

    /// The slide showing at `at`: up [`SWITCH_EARLY`] before it is sung,
    /// and held through a gap after it until the section changes.
    fn slide_at(&self, at: f64) -> Option<usize> {
        self.slides
            .iter()
            .rposition(|s| s.start <= at + SWITCH_EARLY)
    }

    /// The line lit at `at`, [`LINE_EARLY`] ahead of the singer.
    fn line_lit(&self, at: f64) -> Option<usize> {
        self.lyrics.line_at(at + LINE_EARLY)
    }

    fn color(&self, section: usize) -> String {
        self.colors
            .get(section)
            .cloned()
            .unwrap_or_else(|| ACCENT.to_owned())
    }
}

impl Words {
    /// The slide on screen at `at`, as a room sees it: up early, held
    /// through a breath, gone in an instrumental; `None` before the words.
    fn slide_on_screen(&self, at: f64) -> Option<usize> {
        let section = self.section_at(at);
        self.slide_at(at)
            .filter(|&i| Some(self.slides[i].section) == section)
    }

    /// The slide after `at`'s: the next to go up.
    fn slide_next(&self, at: f64) -> Option<usize> {
        let next = self.slide_at(at).map_or(0, |i| i + 1);
        (next < self.slides.len()).then_some(next)
    }

    /// A slide's lines' texts.
    fn texts(&self, slide: Option<usize>) -> Vec<String> {
        slide.map_or_else(Vec::new, |i| {
            self.slides[i]
                .lines
                .clone()
                .map(|k| self.lyrics.lines[k].text.clone())
                .collect()
        })
    }
}

/// The largest font that sets `lines` in a `w` × `h` box: the height
/// shared by the lines, the width by the longest of them (a character is
/// about half an em of this face).
fn fit(lines: &[String], w: f64, h: f64, max: f64) -> f64 {
    let longest = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(8) as f64;
    let count = lines.len().max(1) as f64;
    ((h * 0.82) / (count * 1.18))
        .min((w * 0.9) / (longest * 0.52))
        .clamp(14.0, max)
}

/// The Confidence Monitor's yellow: ProPresenter's for the next slide,
/// and what a section's colour gives way to when it is too pale to tell
/// from the white slide above it.
const NEXT_YELLOW: &str = "#f5c542";

/// The song as a strip of its sections, each as long as it lasts and in
/// its colour: the one the song is in lit, those gone faint.
#[component]
fn SectionStrip(words: Words, at: f64, height: f64) -> Element {
    let current = words.section_at(at);
    let span = (words.song.1 - words.song.0).max(1e-6);
    let parts: Vec<(f64, String, f64)> = words
        .sections
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let opacity = match current {
                Some(i) if k == i => 1.0,
                Some(i) if k < i => 0.25,
                _ => 0.5,
            };
            ((s.end - s.start) / span * 100.0, words.color(k), opacity)
        })
        .collect();
    rsx! {
        div {
            style: "display:flex; gap:2px; height:{height}px;",
            for (k, (width, fill, opacity)) in parts.into_iter().enumerate() {
                div { key: "{k}", style: "flex:none; width:{width}%; height:{height}px; border-radius:1px; background:{fill}; opacity:{opacity};" }
            }
        }
    }
}

/// A section's name as a rail spells it: the chart's abbreviation
/// written out, its number and part kept — `VS 2B` is "Verse 2B".
fn spelled(name: &str) -> String {
    let (kind, rest) = name.trim().split_once(' ').unwrap_or((name.trim(), ""));
    let full = match kind.to_ascii_uppercase().as_str() {
        "VS" | "V" => "Verse",
        "CH" | "C" => "Chorus",
        "PRE" | "PRE-CH" | "PC" => "Pre-Chorus",
        "POST" | "POST-CH" => "Post-Chorus",
        "BR" | "B" => "Bridge",
        "IN" | "INTRO" => "Intro",
        "OUT" | "OUTRO" => "Outro",
        "INT" => "Interlude",
        "INST" => "Instrumental",
        "TURN" | "TA" => "Turnaround",
        "TAG" => "Tag",
        "REFRAIN" | "REF" => "Refrain",
        "BD" | "BREAKDOWN" => "Breakdown",
        "COUNT" => "Count",
        "END" | "ENDING" => "Ending",
        _ => return name.trim().to_owned(),
    };
    if rest.is_empty() {
        full.to_owned()
    } else {
        format!("{full} {rest}")
    }
}

/// The Confidence Monitor's left edge: the slide's section as a strip of
/// its colour with its name printed down it.
#[component]
fn SectionRail(color: String, name: String) -> Element {
    rsx! {
        div {
            style: "flex:none; width:22px; height:100%; display:flex; align-items:center; justify-content:center; \
                    background:{color};",
            div {
                style: "flex:none; white-space:nowrap; transform:rotate(90deg); font-size:11px; font-weight:800; \
                        letter-spacing:1.5px; text-transform:uppercase; color:#0b0c0e;",
                "{name}"
            }
        }
    }
}

/// A section's colour as the next slide's text: itself, unless it is
/// whitish (bright and barely tinted) or not a `#rrggbb` — then yellow.
fn next_color(color: &str) -> String {
    let channel = |at: usize| {
        color
            .get(at..at + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
    };
    let rgb = (color.len() == 7 && color.starts_with('#'))
        .then(|| Some((channel(1)?, channel(3)?, channel(5)?)))
        .flatten();
    let Some((r, g, b)) = rgb else {
        return NEXT_YELLOW.to_owned();
    };
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let saturation = if hi > 0.0 { (hi - lo) / hi } else { 0.0 };
    if luminance > 0.72 && saturation < 0.3 {
        NEXT_YELLOW.to_owned()
    } else {
        color.to_owned()
    }
}

/// A section colour at `alpha` (0–255), for washes: `#rrggbb` gains an
/// alpha byte; anything else is used as it is.
fn tint(color: &str, alpha: u8) -> String {
    if color.len() == 7 && color.starts_with('#') {
        format!("{color}{alpha:02x}")
    } else {
        color.to_owned()
    }
}

/// A tick every `every`, for as long as it is listened to: from a thread of
/// its own natively (the window's executor keeps no timers), from the
/// browser's timer on a page — which has no threads, so a spawned one never
/// ticked, and the panel never learnt its size.
fn ticks(every: Duration) -> futures_channel::mpsc::UnboundedReceiver<()> {
    let (tick, ticks) = futures_channel::mpsc::unbounded::<()>();
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        while tick.unbounded_send(()).is_ok() {
            std::thread::sleep(every);
        }
    });
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let ms = u32::try_from(every.as_millis()).unwrap_or(u32::MAX);
        while tick.unbounded_send(()).is_ok() {
            gloo_timers::future::TimeoutFuture::new(ms).await;
        }
    });
    ticks
}

/// The panel's size in pixels, read back from the layout and kept up to
/// date — the Audience's words are sized to it.
///
/// It starts at the size the lyrics last had (a view switched back to
/// takes the same place), and is read the moment the panel is mounted:
/// the words are drawn at their size from the first frame, not small and
/// then grown a tick later.
fn use_size() -> (Signal<(f64, f64)>, Signal<Option<Rc<MountedData>>>) {
    thread_local! {
        static LAST: std::cell::Cell<(f64, f64)> = const { std::cell::Cell::new((0.0, 0.0)) };
    }
    // Where it starts: the size it last had, or — never shown yet — the
    // window less the bars, which is where the Lyrics view is. Never
    // nothing: words drawn late are worse than words a hair off.
    let mut size = use_signal(|| {
        let last = LAST.with(std::cell::Cell::get);
        if last.0 > 0.0 && last.1 > 0.0 {
            return last;
        }
        try_consume_context::<crate::shell::WindowSize>().map_or((0.0, 0.0), |window| {
            let (w, h) = *window.0.peek();
            (
                w,
                (h - crate::shell::BAR_H - crate::shell::BOTTOM_H).max(0.0),
            )
        })
    });
    let node = use_signal(|| None::<Rc<MountedData>>);
    use_effect(move || {
        let (w, h) = size();
        if w > 0.0 && h > 0.0 {
            LAST.with(|last| last.set((w, h)));
        }
    });
    // Mounted: measured now, not at the next tick.
    use_effect(move || {
        if let Some(node) = node() {
            spawn(async move {
                if let Ok(rect) = node.get_client_rect().await {
                    let now = (rect.size.width, rect.size.height);
                    if *size.peek() != now {
                        size.set(now);
                    }
                }
            });
        }
    });
    use_future(move || async move {
        // Read on a tick: a panel resizes when a divider or the window
        // moves, and nothing tells it.
        let mut ticks = ticks(Duration::from_millis(400));
        while ticks.next().await.is_some() {
            let Some(node) = node.peek().clone() else {
                continue;
            };
            if let Ok(rect) = node.get_client_rect().await {
                let now = (rect.size.width, rect.size.height);
                if *size.peek() != now {
                    size.set(now);
                }
            }
        }
    });
    (size, node)
}

/// The lyrics panel.
#[component]
pub fn LyricsPanel() -> Element {
    let session: StudioSession = use_context();
    let words = use_hook(|| Words::of(&session));
    let reading = crate::progress::use_reading();
    let own = use_hook(LyricsChoice::new);
    let LyricsChoice {
        mut view,
        mut layer,
    } = try_use_context::<LyricsChoice>().unwrap_or(own);
    let (size, mut node) = use_size();
    let mut open = use_signal(|| false);
    let (mut awake, mut moved) = use_controls(open);
    let at = reading().at;
    let has = words.lyrics.layers();
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; overflow:hidden; \
                    background:#07080b; color:{TEXT}; font-family:system-ui, sans-serif;",
            onmounted: move |e| node.set(Some(e.data())),
            onmousemove: move |_| {
                moved.set(web_time::Instant::now());
                if !*awake.peek() {
                    awake.set(true);
                }
            },
            if words.lyrics.lines.is_empty() {
                div {
                    style: "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; \
                            align-items:center; justify-content:center; padding:24px; text-align:center; \
                            color:{DIM}; font-size:13px; line-height:1.6;",
                    "No lyrics for this song yet — put a synced .lrc beside its chart \
                     (session lyrics fetch) and prepare the song again."
                }
            } else if size().0 <= 0.0 || size().1 <= 0.0 {
                // Not measured yet: nothing, rather than words sized to
                // nothing (a frame, at most — see `use_size`).
            } else {
                match view() {
                    View::Audience => rsx! { Audience { words: words.clone(), layer: layer(), at, size: size() } },
                    View::Performer => rsx! { Performer { words: words.clone(), at, size: size() } },
                    View::Confidence => rsx! { Confidence { words: words.clone(), at, size: size() } },
                }
            }
            // Which view: a dropdown in the corner, there only while the
            // mouse is moving over the panel (or its menu is open) — a
            // screen someone reads from carries no controls the rest of
            // the time.
            if awake() || open() {
                div {
                    style: "position:absolute; top:8px; right:8px; display:flex; flex-direction:column; \
                            align-items:flex-end;",
                    button {
                        style: "height:24px; padding:0 10px; border-radius:6px; border:1px solid {RULE}; \
                                background:rgba(12,13,17,0.86); color:#d5d8de; font-size:11px; font-weight:700; \
                                cursor:pointer; white-space:nowrap;",
                        onclick: move |_| open.toggle(),
                        "{view().name()}  \u{25BE}"
                    }
                    if open() {
                        div {
                            style: "margin-top:4px; min-width:170px; padding:4px; border-radius:8px; \
                                    background:#121318; border:1px solid {RULE}; \
                                    box-shadow:0 8px 24px rgba(0,0,0,0.55);",
                            for each in View::ALL {
                                MenuRow {
                                    label: each.name().to_owned(),
                                    on: view() == each,
                                    available: true,
                                    pick: move |()| {
                                        view.set(each);
                                        open.set(false);
                                    },
                                }
                            }
                            if view() == View::Audience {
                                div { style: "height:1px; margin:4px 2px; background:{RULE};" }
                                div {
                                    style: "padding:2px 8px 4px 8px; font-size:9px; font-weight:800; letter-spacing:1.2px; \
                                            text-transform:uppercase; color:#6b707a;",
                                    "Show"
                                }
                                for each in [Layer::Section, Layer::Slide, Layer::Line] {
                                    MenuRow {
                                        label: each.name().to_owned(),
                                        on: layer() == each,
                                        available: has.contains(&each),
                                        pick: move |()| {
                                            layer.set(each);
                                            open.set(false);
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// How long the view's dropdown stays after the mouse stops moving.
const CONTROLS_LINGER: Duration = Duration::from_millis(2500);

/// Whether the panel's controls are up: woken by a mouse moving over it,
/// asleep again [`CONTROLS_LINGER`] after it stops (never while `open`).
fn use_controls(open: Signal<bool>) -> (Signal<bool>, Signal<web_time::Instant>) {
    let mut awake = use_signal(|| false);
    let moved = use_signal(web_time::Instant::now);
    use_future(move || async move {
        let mut ticks = ticks(Duration::from_millis(250));
        while ticks.next().await.is_some() {
            if *awake.peek() && !*open.peek() && moved.peek().elapsed() > CONTROLS_LINGER {
                awake.set(false);
            }
        }
    });
    (awake, moved)
}

/// One row of the view's dropdown.
#[component]
fn MenuRow(label: String, on: bool, available: bool, pick: EventHandler<()>) -> Element {
    let (bg, fg) = match (on, available) {
        (true, _) => ("#1f2a3a", "#ffffff"),
        (false, true) => ("transparent", "#c7cad1"),
        (false, false) => ("transparent", "#4b4f57"),
    };
    let mark = if on { "\u{2713}" } else { "" };
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:8px; padding:6px 8px; border-radius:5px; \
                    background:{bg}; color:{fg}; font-size:12px; font-weight:600; cursor:pointer;",
            onclick: move |_| {
                if available {
                    pick.call(());
                }
            },
            div { style: "width:12px; color:{ACCENT}; font-weight:800;", "{mark}" }
            "{label}"
        }
    }
}

/// The room's view: the words and nothing else, as large as the panel
/// allows, centred, on a dark field lit by the section's colour.
#[component]
fn Audience(words: Words, layer: Layer, at: f64, size: (f64, f64)) -> Element {
    let lines = &words.lyrics.lines;
    let section = words.section_at(at);
    let color = section.map_or_else(|| ACCENT.to_owned(), |i| words.color(i));
    let before_words = lines
        .first()
        .is_some_and(|first| at + SWITCH_EARLY < first.start);
    let shown: Vec<usize> = if before_words {
        Vec::new()
    } else {
        match layer {
            Layer::Section => section
                .map(|i| words.sections[i].lines.clone().collect())
                .unwrap_or_default(),
            Layer::Line => words
                .lyrics
                .line_at(at + SWITCH_EARLY)
                .into_iter()
                .collect(),
            // A slide holds through a breath, but not into the next
            // section's instrumental: then the screen goes dark.
            _ => words
                .slide_at(at)
                .filter(|&i| Some(words.slides[i].section) == section)
                .map(|i| words.slides[i].lines.clone().collect())
                .unwrap_or_default(),
        }
    };
    let (w, h) = (size.0.max(1.0), size.1.max(1.0));
    // As large as fits: the height shared by the lines, the width by the
    // longest of them (a character is about half an em of this face).
    let longest = shown
        .iter()
        .map(|&k| lines[k].text.chars().count())
        .max()
        .unwrap_or(1)
        .max(8) as f64;
    let count = shown.len().max(1) as f64;
    let font = ((h * 0.78) / (count * 1.2))
        .min((w * 0.86) / (longest * 0.52))
        .clamp(18.0, 140.0);
    let glow = tint(&color, 0x38);
    let title_size = (font * 0.9).min(72.0);
    let artist_gap = (font * 0.25).min(18.0);
    let artist_size = (font * 0.32).clamp(12.0, 26.0);
    let line_gap = font * 0.12;
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; \
                    flex-direction:column; align-items:center; justify-content:center; \
                    padding:0 6%; text-align:center; \
                    background:radial-gradient(ellipse at 50% 55%, {glow} 0%, #0c0e16 55%, #040507 100%);",
            if before_words {
                div {
                    style: "font-size:{title_size}px; font-weight:800; letter-spacing:-1px; \
                            line-height:1.1; color:#ffffff;",
                    "{words.title}"
                }
                if let Some(artist) = &words.artist {
                    div {
                        style: "margin-top:{artist_gap}px; font-size:{artist_size}px; \
                                font-weight:500; letter-spacing:2px; text-transform:uppercase; color:#9aa3b5;",
                        "{artist}"
                    }
                }
            }
            for k in shown {
                div {
                    key: "{k}",
                    style: "font-size:{font}px; line-height:1.14; font-weight:800; letter-spacing:-0.02em; \
                            color:#ffffff; margin:0 0 {line_gap}px 0;",
                    "{lines[k].text}"
                }
            }
        }
    }
}

/// One row of the Performer's teleprompter.
#[derive(Clone, Copy, PartialEq)]
enum Row {
    /// A section's label, where it begins.
    Heading(usize),
    /// A section with nothing sung in it.
    Instrumental(usize),
    Line(usize),
}

/// The band's view: where the song is, over the lyrics as a teleprompter
/// that runs itself.
#[component]
fn Performer(words: Words, at: f64, size: (f64, f64)) -> Element {
    let lines = &words.lyrics.lines;
    let Some(i) = words.section_at(at) else {
        return rsx! {};
    };
    let section = &words.sections[i];
    let color = words.color(i);
    let through =
        ((at - section.start) / (section.end - section.start).max(1e-6)).clamp(0.0, 1.0) * 100.0;
    let lit = words.line_lit(at);
    // The line the prompter is on: the one lit, or else the next to come.
    let focus = lit.or_else(|| lines.iter().position(|l| l.start > at + LINE_EARLY));
    // The whole song as one list: each section's label, then its lines.
    let rows: Vec<Row> = words
        .sections
        .iter()
        .enumerate()
        .flat_map(|(k, s)| {
            let body: Vec<Row> = if s.lines.is_empty() {
                vec![Row::Instrumental(k)]
            } else {
                s.lines.clone().map(Row::Line).collect()
            };
            std::iter::once(Row::Heading(k)).chain(body)
        })
        .collect();
    // Scrolled to the focus: one row of what has gone above it, and the
    // section's label kept when the focus is its first line.
    let at_row = focus
        .and_then(|f| rows.iter().position(|r| *r == Row::Line(f)))
        .or_else(|| rows.iter().position(|r| *r == Row::Heading(i)))
        .unwrap_or(0);
    let from = at_row.saturating_sub(
        if matches!(rows.get(at_row.wrapping_sub(1)), Some(Row::Heading(_))) {
            2
        } else {
            1
        },
    );
    let w = size.0.max(1.0);
    let big = (w / 19.0).clamp(20.0, 40.0);
    let mid = (big * 0.66).max(15.0);
    let small = (big * 0.52).max(13.0);
    let next = words.sections.get(i + 1).map(|s| {
        (
            s.name.clone(),
            (words.shows_from(s) - at).max(0.0),
            words.color(i + 1),
        )
    });
    let strip_words = words.clone();
    let row = |r: Row, n: usize| -> Element {
        match r {
            Row::Heading(k) => {
                let c = words.color(k);
                let name = &words.sections[k].name;
                let above = if n == 0 { 2 } else { 12 };
                rsx! {
                    div {
                        key: "h{k}",
                        style: "display:flex; align-items:center; gap:8px; margin:{above}px 0 6px 0;",
                        div { style: "width:3px; height:12px; border-radius:1px; background:{c};" }
                        div { style: "font-size:11px; font-weight:800; letter-spacing:1.5px; text-transform:uppercase; color:{c};", "{name}" }
                        div { style: "flex:1; height:1px; background:#1d2027;" }
                    }
                }
            }
            Row::Instrumental(k) => rsx! {
                div { key: "i{k}", style: "font-size:{small}px; color:#5a5f69; font-style:italic; margin:0 0 6px 11px;", "Instrumental" }
            },
            Row::Line(k) => {
                let style = match focus {
                    Some(f) if k == f => format!(
                        "font-size:{big}px; line-height:1.16; font-weight:800; color:{}; margin:2px 0 8px 0; \
                         padding-left:8px; border-left:3px solid {};",
                        if lit == Some(k) { "#ffffff" } else { "#cfd3da" },
                        words.color(
                            words
                                .sections
                                .iter()
                                .position(|s| s.lines.contains(&k))
                                .unwrap_or(i)
                        ),
                    ),
                    Some(f) if k == f + 1 => format!(
                        "font-size:{mid}px; line-height:1.22; font-weight:700; color:#a9afba; margin:0 0 7px 0; padding-left:11px;"
                    ),
                    Some(f) if k < f => format!(
                        "font-size:{small}px; line-height:1.28; font-weight:500; color:#474c55; margin:0 0 6px 0; padding-left:11px;"
                    ),
                    _ => format!(
                        "font-size:{small}px; line-height:1.28; font-weight:600; color:#767c87; margin:0 0 6px 0; padding-left:11px;"
                    ),
                };
                rsx! { div { key: "l{k}", style: style, "{lines[k].text}" } }
            }
        }
    };
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; flex-direction:column; \
                    background:#07080b;",
            // Where the song is — the section, how far through, and what
            // is next and when — in one line under the switch's corner.
            div {
                style: "flex:none; display:flex; align-items:center; gap:10px; height:36px; padding:0 16px; \
                        margin-right:270px;",
                div {
                    style: "flex:none; padding:2px 9px; border-radius:5px; background:{color}; color:#0b0c0e; \
                            font-size:12px; font-weight:800; letter-spacing:1px; text-transform:uppercase;",
                    "{section.name}"
                }
                div {
                    style: "flex:1; min-width:30px; height:3px; background:#1b1d23; border-radius:2px;",
                    div { style: "height:3px; width:{through}%; background:{color}; border-radius:2px;" }
                }
                if let Some((name, due, c)) = next {
                    div {
                        style: "flex:none; font-size:11px; font-weight:700; color:#8b9099; white-space:nowrap;",
                        span { style: "color:{c}; letter-spacing:1px; text-transform:uppercase;", "{name}" }
                        " in {due:.0}s"
                    }
                }
            }
            // The song as a strip of its sections.
            div {
                style: "flex:none; margin:0 16px;",
                SectionStrip { words: strip_words, at, height: 3.0 }
            }
            // The prompter: from just above the line being sung, on.
            div {
                style: "flex:1; min-height:0; overflow:hidden; padding:12px 16px 0 16px;",
                for (n, r) in rows[from..].iter().copied().take(40).enumerate() {
                    {row(r, n)}
                }
            }
        }
    }
}

/// The singer's glance, as ProPresenter's stage display has it: the
/// slide on screen now over a rule, the next slide under it — in its
/// section's colour, so a change of section is seen coming, and named on
/// the rule when it is one.
#[component]
fn Confidence(words: Words, at: f64, size: (f64, f64)) -> Element {
    let now = words.texts(words.slide_on_screen(at));
    let next_slide = words.slide_next(at);
    let next = words.texts(next_slide);
    // Each half's section, for its rail: the slide's own, or — with no
    // slide up (an instrumental, the count) — the section the song is in.
    let now_section = words
        .slide_on_screen(at)
        .map(|n| words.slides[n].section)
        .or_else(|| words.section_at(at));
    let now_rail = now_section.map(|s| (words.color(s), spelled(&words.sections[s].name)));
    let next_rail = next_slide.map(|n| {
        let s = words.slides[n].section;
        (words.color(s), spelled(&words.sections[s].name))
    });
    let next_color = next_slide.map_or_else(
        || NEXT_YELLOW.to_owned(),
        |n| next_color(&words.color(words.slides[n].section)),
    );
    // The next slide's section, when it is a new one: said on the rule.
    let next_section = next_slide
        .map(|n| words.slides[n].section)
        .filter(|s| Some(*s) != words.section_at(at))
        .map(|s| words.sections[s].name.clone());
    let (w, h) = (size.0.max(1.0), size.1.max(1.0));
    let half = (h - 12.0) / 2.0;
    let now_font = fit(&now, w * 0.92, half, 110.0);
    let next_font = fit(&next, w * 0.92, half, 90.0);
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; flex-direction:column; \
                    background:#000000;",
            div {
                style: "flex:1; min-height:0; display:flex;",
                if let Some((color, name)) = now_rail {
                    SectionRail { color, name }
                }
                div {
                    style: "flex:1; min-width:0; display:flex; flex-direction:column; align-items:center; \
                            justify-content:center; padding:0 4%; text-align:center;",
                    for (k, text) in now.iter().enumerate() {
                        div {
                            key: "{k}",
                            style: "font-size:{now_font}px; line-height:1.12; font-weight:800; color:#ffffff;",
                            "{text}"
                        }
                    }
                }
            }
            div {
                style: "flex:none; position:relative; height:2px; background:#3a3d44;",
                if let Some(name) = next_section {
                    div {
                        style: "position:absolute; left:32px; top:-8px; padding:0 6px; background:#000000; \
                                font-size:11px; font-weight:800; letter-spacing:1.5px; text-transform:uppercase; \
                                color:{next_color};",
                        "{name}"
                    }
                }
            }
            div {
                style: "flex:1; min-height:0; display:flex;",
                if let Some((color, name)) = next_rail {
                    SectionRail { color, name }
                }
                div {
                    style: "flex:1; min-width:0; display:flex; flex-direction:column; align-items:center; \
                            justify-content:center; padding:0 4%; text-align:center;",
                    for (k, text) in next.iter().enumerate() {
                        div {
                            key: "{k}",
                            style: "font-size:{next_font}px; line-height:1.12; font-weight:800; color:{next_color};",
                            "{text}"
                        }
                    }
                }
            }
            // Where the song is, along the very bottom.
            div {
                style: "flex:none; padding:0 6px 6px 6px;",
                SectionStrip { words: words.clone(), at, height: 4.0 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The next slide wears its section's colour unless that is whitish.
    #[test]
    fn a_pale_section_colour_gives_way_to_yellow() {
        assert_eq!(next_color("#3aa0ff"), "#3aa0ff");
        assert_eq!(next_color("#e84a5f"), "#e84a5f");
        assert_eq!(next_color("#f2f2f2"), NEXT_YELLOW);
        assert_eq!(next_color("#dde3ea"), NEXT_YELLOW);
        assert_eq!(next_color("rgb(1,2,3)"), NEXT_YELLOW);
    }

    /// A rail spells the section out, keeping its number and part.
    #[test]
    fn a_rail_spells_the_section() {
        assert_eq!(spelled("VS 2B"), "Verse 2B");
        assert_eq!(spelled("CH 1"), "Chorus 1");
        assert_eq!(spelled("PRE-CH"), "Pre-Chorus");
        assert_eq!(spelled("IN"), "Intro");
        assert_eq!(spelled("Mystery 3"), "Mystery 3");
    }
}
