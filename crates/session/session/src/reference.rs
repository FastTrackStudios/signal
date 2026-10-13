//! A recording played alongside the session, to work against.
//!
//! What Songsterr does with a YouTube video and what an engineer does
//! with a reference mix: the record you are trying to match, running in
//! step with the session so you can hear both against each other. It is
//! also what makes tempo mapping possible for someone with no multitrack
//! at all — you map the bar lines to a video.
//!
//! The reference is not audio the session owns. It is somewhere else,
//! played by something else — a browser's video element, a media player
//! — and all this domain can do is say where that player SHOULD be and
//! what it should be doing. Which is enough, and is the part that is
//! the same whatever is doing the playing.

/// Where the reference comes from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Source {
    /// A YouTube video, by its id — the eleven characters, not a URL.
    ///
    /// The id rather than the URL because a URL carries a playlist, a
    /// start time, a tracking tag and whatever else was on the end of
    /// what somebody pasted, and none of that survives being stored
    /// and reopened as the same video.
    Youtube(String),
    /// Anything a player can open by address.
    Url(String),
}

impl Source {
    /// A YouTube source from whatever somebody pasted.
    ///
    /// Takes the id out of a watch URL, a short link, an embed, or a
    /// bare id typed on its own. Everything else on the end — the
    /// playlist, the start time, the tracking tag a share button adds —
    /// is dropped, because none of it identifies the video and all of
    /// it changes between two people pasting the same one.
    ///
    /// `None` when there is no id in there. Refusing beats storing the
    /// whole URL as an "id" and failing later at the player, where the
    /// error is somebody else's.
    #[must_use]
    pub fn youtube(pasted: &str) -> Option<Self> {
        let text = pasted.trim();
        if text.is_empty() {
            return None;
        }
        // A bare id: eleven characters of YouTube's alphabet.
        if is_id(text) {
            return Some(Self::Youtube(text.to_owned()));
        }
        // Everything else: find the id wherever this shape puts it.
        let after = |marker: &str| {
            text.split_once(marker)
                .map(|(_, rest)| rest.split(['&', '?', '#', '/']).next().unwrap_or(""))
        };
        ["v=", "youtu.be/", "/embed/", "/shorts/", "/live/"]
            .into_iter()
            .filter_map(after)
            .find(|candidate| is_id(candidate))
            .map(|id| Self::Youtube(id.to_owned()))
    }
}

/// Is this a YouTube id?
///
/// Eleven characters of `A-Za-z0-9_-`. Checked rather than assumed: a
/// URL with a `v=` that holds something else is a URL for a different
/// site, and taking eleven characters off it would produce an id that
/// looks right and plays nothing.
fn is_id(text: &str) -> bool {
    text.len() == 11
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A reference recording, lined up with the session.
#[derive(Clone, PartialEq, Debug)]
pub struct Reference {
    pub source: Source,
    /// The session time that lines up with [`Self::from`].
    ///
    /// Two numbers rather than one offset because both ends are things
    /// a person points at: "bar one of my session" and "the downbeat in
    /// the video". An offset is the arithmetic between them, and asking
    /// someone to do that arithmetic is asking them to get it wrong.
    pub anchor: f64,
    /// Where in the recording that moment is.
    pub from: f64,
    /// How fast the recording runs against the session.
    ///
    /// One unless the reference is at a different tempo and is being
    /// stretched to match. A rate of zero or less is not a rate.
    pub rate: f64,
}

impl Reference {
    /// A reference lined up so its start is the session's start.
    #[must_use]
    pub fn new(source: Source) -> Self {
        Self {
            source,
            anchor: 0.0,
            from: 0.0,
            rate: 1.0,
        }
    }

    /// Where the player should be when the session is at `at`.
    ///
    /// Before the anchor the recording has not started: the answer is
    /// its beginning rather than a negative time, because a player
    /// cannot be at minus four seconds and asking it to be leaves it
    /// wherever it was.
    #[must_use]
    pub fn at(&self, session: f64) -> f64 {
        let rate = if self.rate > 0.0 { self.rate } else { 1.0 };
        (session - self.anchor).mul_add(rate, self.from).max(0.0)
    }

    /// The session time a moment in the recording lines up with.
    ///
    /// The inverse of [`Self::at`], for the other direction: a click on
    /// the video's own scrubber has to land somewhere in the session.
    #[must_use]
    pub fn session_at(&self, reference: f64) -> f64 {
        let rate = if self.rate > 0.0 { self.rate } else { 1.0 };
        (reference - self.from) / rate + self.anchor
    }

    /// Line the recording up from two moments, taking the rate from
    /// the distance between them.
    ///
    /// One point says where the recording starts; two say how fast it
    /// runs. It is the same gesture twice — "that, there, is this,
    /// here" — and it is how a reference recorded a few percent off, or
    /// one somebody re-recorded at a different tempo, is made to hold
    /// for a whole song instead of drifting out by the last chorus.
    ///
    /// Each point is `(session, recording)`. Two points that give no
    /// usable rate — the same moment twice, or one that runs backwards
    /// — keep the rate there was and anchor on the first, which is
    /// exactly [`Self::aligned`] on that point. A rate of zero would
    /// park the recording forever and a negative one would run it
    /// backwards, and neither is a thing anybody meant to ask for.
    #[must_use]
    pub fn lined_up(&self, first: (f64, f64), second: (f64, f64)) -> Self {
        let (session, recording) = (second.0 - first.0, second.1 - first.1);
        let rate = recording / session;
        let usable = session.abs() > f64::EPSILON && rate.is_finite() && rate > 0.0;
        Self {
            rate: if usable { rate } else { self.rate },
            ..self.aligned(first.0, first.1)
        }
    }

    /// Line the recording up so `session` and `reference` are the same
    /// moment, keeping the rate.
    ///
    /// The gesture behind "that hit, there, is this hit, here".
    #[must_use]
    pub fn aligned(&self, session: f64, reference: f64) -> Self {
        Self {
            anchor: session,
            from: reference.max(0.0),
            ..self.clone()
        }
    }
}

/// What the player should be told.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Command {
    /// Move to this time in the recording.
    Seek(f64),
    Play,
    Pause,
    /// It is already doing the right thing.
    Nothing,
}

/// How far out the player may be before it is worth correcting.
///
/// A seek is not free and it is not silent: a video element that is
/// nudged every frame stutters, and a reference that stutters is worse
/// than one that is a few milliseconds late. Eighty is under what a
/// listener hears against a click and far more than a player's own
/// jitter, so it corrects real drift and ignores the rest.
pub const TOLERANCE: f64 = 0.080;

/// What the player is doing now.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Playing {
    pub at: f64,
    pub playing: bool,
}

/// What to tell the player, given where the session is.
///
/// One command at a time, deliberately. A seek and a play issued
/// together race each other in every player worth the name — the seek
/// lands, playback resumes from where it WAS, and the correction
/// undoes itself. Correct the position first and start on the next
/// call, by which time the seek has landed.
#[must_use]
pub fn follow(reference: &Reference, session: f64, rolling: bool, player: Playing) -> Command {
    let want = reference.at(session);
    let drifted = (player.at - want).abs() > TOLERANCE;

    // Stopping comes first and unconditionally. A reference still
    // playing after the session stopped is the one failure a listener
    // cannot miss.
    if !rolling {
        if player.playing {
            return Command::Pause;
        }
        // Parked: follow the session's cursor so scrubbing the session
        // scrubs the video, which is most of what a reference is for
        // when you are not playing.
        return if drifted {
            Command::Seek(want)
        } else {
            Command::Nothing
        };
    }
    if drifted {
        return Command::Seek(want);
    }
    if player.playing {
        Command::Nothing
    } else {
        Command::Play
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Playing, Reference, Source, TOLERANCE, follow};

    fn video() -> Reference {
        Reference::new(Source::Youtube("dQw4w9WgXcQ".into()))
    }

    /// Whatever somebody pasted becomes the same id.
    ///
    /// The point of storing the id rather than the URL: two people
    /// sharing the same video send each other quite different text.
    #[test]
    fn every_shape_of_youtube_link_is_the_same_video() {
        let want = Some(Source::Youtube("dQw4w9WgXcQ".into()));
        for pasted in [
            "dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            // What a share button actually gives you.
            "https://youtu.be/dQw4w9WgXcQ?si=abc123&t=42",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL9&index=2",
            "  https://www.youtube.com/watch?v=dQw4w9WgXcQ  ",
        ] {
            assert_eq!(Source::youtube(pasted), want, "failed on {pasted}");
        }
    }

    /// Something that is not a video is refused, not guessed at.
    ///
    /// Storing the whole URL as an "id" would move the failure to the
    /// player, where it is somebody else's error message.
    #[test]
    fn a_link_with_no_video_in_it_is_refused() {
        for pasted in [
            "",
            "   ",
            "https://www.youtube.com/",
            "https://vimeo.com/123456",
            // A v= holding something that is not an id.
            "https://example.com/watch?v=hello",
            // Ten characters, not eleven.
            "dQw4w9WgXc",
        ] {
            assert_eq!(Source::youtube(pasted), None, "accepted {pasted:?}");
        }
    }

    /// The two-point alignment: a moment here is a moment there.
    #[test]
    fn aligning_two_moments_lines_the_rest_up() {
        // Bar nine of the session is twelve seconds into the video.
        let r = video().aligned(16.0, 12.0);
        assert!((r.at(16.0) - 12.0).abs() < 1e-9, "the anchor itself");
        assert!((r.at(18.0) - 14.0).abs() < 1e-9, "two seconds later, both");
        assert!((r.session_at(14.0) - 18.0).abs() < 1e-9, "and back again");
    }

    /// Before the anchor the recording has not started.
    ///
    /// A negative time is not a place a player can be, and asking for
    /// one leaves it wherever it was — which reads as the sync being
    /// broken rather than as the video not having begun.
    #[test]
    fn before_the_anchor_the_video_is_at_its_start() {
        let r = video().aligned(20.0, 5.0);
        assert!((r.at(0.0) - 0.0).abs() < 1e-9, "not minus fifteen");
        assert!((r.at(19.0) - 4.0).abs() < 1e-9, "one second before, though");
    }

    /// A rate stretches the reference against the session.
    #[test]
    fn a_rate_stretches_the_reference() {
        let r = Reference {
            rate: 2.0,
            ..video()
        };
        assert!((r.at(10.0) - 20.0).abs() < 1e-9);
        assert!((r.session_at(20.0) - 10.0).abs() < 1e-9, "and inverts");
        // A nonsense rate is one, not a division by zero.
        let mad = Reference {
            rate: 0.0,
            ..video()
        };
        assert!((mad.at(10.0) - 10.0).abs() < 1e-9);
    }

    /// Rolling and in step: leave it alone.
    ///
    /// The common case by far, and the one that must produce no
    /// command at all — a player nudged every frame stutters.
    #[test]
    fn in_step_is_left_alone() {
        let r = video();
        let player = Playing {
            at: 10.0 + TOLERANCE / 2.0,
            playing: true,
        };
        assert_eq!(follow(&r, 10.0, true, player), Command::Nothing);
    }

    /// Drift past the tolerance is corrected.
    #[test]
    fn real_drift_is_corrected() {
        let r = video();
        let player = Playing {
            at: 10.0 + TOLERANCE * 2.0,
            playing: true,
        };
        assert_eq!(follow(&r, 10.0, true, player), Command::Seek(10.0));
    }

    /// The seek comes before the play, never together.
    ///
    /// Issued together they race: the seek lands, playback resumes
    /// from where it was, and the correction undoes itself.
    #[test]
    fn a_seek_and_a_play_are_not_issued_together() {
        let r = video();
        // Stopped and far out: correct the position first.
        let lost = Playing {
            at: 0.0,
            playing: false,
        };
        assert_eq!(follow(&r, 10.0, true, lost), Command::Seek(10.0));
        // Once it is in the right place, start it.
        let found = Playing {
            at: 10.0,
            playing: false,
        };
        assert_eq!(follow(&r, 10.0, true, found), Command::Play);
    }

    /// A reference still playing after the session stopped is the one
    /// failure a listener cannot miss.
    #[test]
    fn stopping_the_session_stops_the_reference_first() {
        let r = video();
        let rolling = Playing {
            at: 999.0,
            playing: true,
        };
        // Badly out of position AND playing: still pause first.
        assert_eq!(follow(&r, 10.0, false, rolling), Command::Pause);
    }

    /// Parked, the video follows the session's cursor.
    ///
    /// Scrubbing the session scrubs the video, which is most of what a
    /// reference is for when you are not playing.
    #[test]
    fn parked_the_video_follows_the_cursor() {
        let r = video();
        let parked = Playing {
            at: 2.0,
            playing: false,
        };
        assert_eq!(follow(&r, 30.0, false, parked), Command::Seek(30.0));
        let arrived = Playing {
            at: 30.0,
            playing: false,
        };
        assert_eq!(follow(&r, 30.0, false, arrived), Command::Nothing);
    }
}

// ─── Keeping it with the project ────────────────────────────────────

/// Where a project's reference is stored.
///
/// Project ext state rather than a file beside it: the reference is
/// part of the session, and a session that arrives without the video it
/// was written against is a session you cannot check.
pub const SECTION: &str = "fasttrackstudio.reference";

/// The one key, holding the whole thing.
///
/// One key rather than four, because the four are only meaningful
/// together — a project that had somehow saved an anchor and lost its
/// source would line up nothing at all, and silently.
pub const KEY: &str = "reference";

impl Reference {
    /// How it is written into ext state.
    ///
    /// `kind|id|anchor|from|rate`, pipe-separated, because a URL can
    /// hold almost anything else and a reader that split on a comma
    /// would break on the first video with one in its address.
    #[must_use]
    pub fn stored(&self) -> String {
        let (kind, id) = match &self.source {
            Source::Youtube(id) => ("youtube", id.as_str()),
            Source::Url(url) => ("url", url.as_str()),
        };
        format!("{kind}|{id}|{}|{}|{}", self.anchor, self.from, self.rate)
    }

    /// Read one back.
    ///
    /// `None` for anything that is not one, rather than a default. A
    /// reference that quietly became "the start of some video" would
    /// line the session up against silence and look like it worked.
    #[must_use]
    pub fn from_stored(text: &str) -> Option<Self> {
        let mut parts = text.split('|');
        let kind = parts.next()?;
        let id = parts.next()?;
        let source = match kind {
            "youtube" => Source::Youtube(id.to_owned()),
            "url" => Source::Url(id.to_owned()),
            _ => return None,
        };
        if id.is_empty() {
            return None;
        }
        let mut number = || parts.next().and_then(|n| n.parse::<f64>().ok());
        let (anchor, from, rate) = (number()?, number()?, number()?);
        if !(anchor.is_finite() && from.is_finite() && rate.is_finite()) {
            return None;
        }
        Some(Self {
            source,
            anchor,
            from,
            rate: if rate > 0.0 { rate } else { 1.0 },
        })
    }
}

#[cfg(test)]
mod storage_tests {
    use super::{Reference, Source};

    /// A reference survives being written and read.
    #[test]
    fn a_reference_round_trips() {
        let original = Reference {
            source: Source::Youtube("dQw4w9WgXcQ".into()),
            anchor: 16.5,
            from: 12.25,
            rate: 1.0,
        };
        let back = Reference::from_stored(&original.stored()).expect("readable");
        assert_eq!(back, original);
    }

    /// A URL with punctuation in it survives too.
    ///
    /// Pipe-separated rather than comma-separated for exactly this: a
    /// video address can hold almost anything, and a reader that split
    /// on a comma would break on the first one that had one.
    #[test]
    fn a_url_with_punctuation_survives() {
        let original = Reference {
            source: Source::Url("https://example.com/a,b?c=1&d=2".into()),
            anchor: 0.0,
            from: 0.0,
            rate: 1.0,
        };
        assert_eq!(
            Reference::from_stored(&original.stored()).as_ref(),
            Some(&original)
        );
    }

    /// Nonsense reads as nothing, not as a default.
    ///
    /// A reference that quietly became "the start of some video" would
    /// line the session up against silence and look like it worked.
    #[test]
    fn nonsense_is_not_a_reference() {
        for text in [
            "",
            "youtube",
            "youtube|",
            "youtube|abc",
            "youtube|abc|1|2",
            "youtube|abc|x|y|z",
            "mystery|abc|0|0|1",
            "youtube|abc|0|0|NaN",
        ] {
            assert!(Reference::from_stored(text).is_none(), "accepted {text:?}");
        }
    }

    /// Two points give a rate, and the rate is what makes the second
    /// point land where it was put — not just the first.
    #[test]
    fn two_moments_give_the_recording_a_rate() {
        let r = Reference::new(Source::Youtube("dQw4w9WgXcQ".into()))
            .lined_up((0.0, 10.0), (100.0, 210.0));
        assert!((r.rate - 2.0).abs() < 1e-9, "rate was {}", r.rate);
        assert!((r.at(0.0) - 10.0).abs() < 1e-9);
        assert!((r.at(100.0) - 210.0).abs() < 1e-9);
        // And halfway through the session is halfway through the span.
        assert!((r.at(50.0) - 110.0).abs() < 1e-9);
    }

    /// The same moment marked twice is one point, not a division by
    /// zero: it anchors and keeps the rate it had.
    #[test]
    fn a_second_point_on_top_of_the_first_is_just_the_first() {
        let original = Reference::new(Source::Youtube("dQw4w9WgXcQ".into()));
        let r = original.lined_up((4.0, 30.0), (4.0, 30.0));
        assert!((r.rate - original.rate).abs() < 1e-9);
        assert_eq!(r, original.aligned(4.0, 30.0));
    }

    /// A second point EARLIER in the recording than the first would run
    /// it backwards. It is refused the same way.
    #[test]
    fn a_backwards_pair_keeps_the_rate_it_had() {
        let r = Reference::new(Source::Youtube("dQw4w9WgXcQ".into()))
            .lined_up((10.0, 60.0), (20.0, 30.0));
        assert!((r.rate - 1.0).abs() < 1e-9, "rate was {}", r.rate);
        assert!((r.at(10.0) - 60.0).abs() < 1e-9);
    }

    /// A stored rate of zero reads as one rather than dividing by it.
    #[test]
    fn a_stored_rate_of_zero_is_one() {
        let r = Reference::from_stored("youtube|abc12345678|0|0|0").expect("readable");
        assert!((r.rate - 1.0).abs() < 1e-9);
    }
}

/// The project's reference, if it has one.
///
/// Anything unreadable comes back as `None` — the same answer as "no
/// reference". A project written by a later version, or hand-edited,
/// should open with no reference rather than one pointing somewhere
/// nobody chose.
#[must_use]
pub fn read<E: daw::service::ExtState>(
    ext: &E,
    project: daw::service::ProjectContext,
) -> Option<Reference> {
    ext.get_project(project, SECTION, KEY)
        .as_deref()
        .and_then(Reference::from_stored)
}

/// Keep this reference with the project: one write.
///
/// # Errors
///
/// Whatever the backend's `set_project` returns — a project that no
/// longer resolves, most likely.
pub fn write<E: daw::service::ExtState>(
    ext: &E,
    project: daw::service::ProjectContext,
    reference: &Reference,
) -> daw_proto::DawResult<()> {
    ext.set_project(project, SECTION, KEY, &reference.stored())
}

/// Drop it.
///
/// A separate call rather than writing an empty value, because an empty
/// value is a stored reference that reads as `None` — and the next
/// person to look at the `.RPP` cannot tell the two apart.
///
/// # Errors
///
/// Whatever the backend's `delete_project` returns.
pub fn forget<E: daw::service::ExtState>(
    ext: &E,
    project: daw::service::ProjectContext,
) -> daw_proto::DawResult<()> {
    ext.delete_project(project, SECTION, KEY)
}

#[cfg(test)]
mod stored_in_the_project {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use daw::service::{ExtState, ProjectContext};
    use daw_proto::DawResult;

    use super::{Reference, Source, forget, read, write};

    /// Project-scoped keys only, which is all a reference ever touches.
    #[derive(Default)]
    struct Fake {
        project: Mutex<HashMap<(String, String), String>>,
    }

    impl ExtState for Fake {
        fn get(&self, _section: &str, _key: &str) -> Option<String> {
            None
        }
        fn set(&self, _section: &str, _key: &str, _value: &str, _persist: bool) -> DawResult<()> {
            Ok(())
        }
        fn delete(&self, _section: &str, _key: &str, _persist: bool) -> DawResult<()> {
            Ok(())
        }
        fn has(&self, _section: &str, _key: &str) -> bool {
            false
        }
        fn get_project(
            &self,
            _project: ProjectContext,
            section: &str,
            key: &str,
        ) -> Option<String> {
            self.project
                .lock()
                .expect("lock")
                .get(&(section.to_owned(), key.to_owned()))
                .cloned()
        }
        fn set_project(
            &self,
            _project: ProjectContext,
            section: &str,
            key: &str,
            value: &str,
        ) -> DawResult<()> {
            self.project
                .lock()
                .expect("lock")
                .insert((section.to_owned(), key.to_owned()), value.to_owned());
            Ok(())
        }
        fn delete_project(
            &self,
            _project: ProjectContext,
            section: &str,
            key: &str,
        ) -> DawResult<()> {
            self.project
                .lock()
                .expect("lock")
                .remove(&(section.to_owned(), key.to_owned()));
            Ok(())
        }
        fn has_project(&self, _project: ProjectContext, section: &str, key: &str) -> bool {
            self.project
                .lock()
                .expect("lock")
                .contains_key(&(section.to_owned(), key.to_owned()))
        }
    }

    fn lined_up() -> Reference {
        Reference::new(Source::Youtube("dQw4w9WgXcQ".into())).aligned(4.0, 30.0)
    }

    /// A project nobody has put a reference on has none.
    #[test]
    fn an_untouched_project_has_no_reference() {
        assert_eq!(read(&Fake::default(), ProjectContext::Current), None);
    }

    /// The alignment survives the round trip, which is the whole point:
    /// a reference that came back pointing at the start of the video
    /// would have to be lined up again every time the session opened.
    #[test]
    fn what_goes_in_comes_back_lined_up() {
        let ext = Fake::default();
        let original = lined_up();
        write(&ext, ProjectContext::Current, &original).expect("written");
        assert_eq!(read(&ext, ProjectContext::Current), Some(original));
    }

    /// Nonsense in the project reads as no reference, not as a
    /// reference to nowhere.
    #[test]
    fn something_unreadable_is_no_reference() {
        let ext = Fake::default();
        ext.set_project(ProjectContext::Current, super::SECTION, super::KEY, "junk")
            .expect("written");
        assert_eq!(read(&ext, ProjectContext::Current), None);
    }

    /// Forgetting removes the key rather than blanking it, so the file
    /// says "no reference" and not "a reference that reads as none".
    #[test]
    fn forgetting_removes_the_key() {
        let ext = Fake::default();
        write(&ext, ProjectContext::Current, &lined_up()).expect("written");
        forget(&ext, ProjectContext::Current).expect("forgotten");
        assert!(!ext.has_project(ProjectContext::Current, super::SECTION, super::KEY));
        assert_eq!(read(&ext, ProjectContext::Current), None);
    }
}
