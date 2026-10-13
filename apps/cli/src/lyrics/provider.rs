//! The provider seam: anything that can answer "which synced lyrics
//! exist for this song?" with a list of candidates.
//!
//! Modelled on multiplatform-lyric-downloader, which tries Spotify →
//! Deezer → LRCLIB → Musixmatch → `YouTube` in order and keeps the first
//! line-timed hit. Only LRCLIB is here today: it needs no account. The
//! others slot in as further implementations of [`LyricsProvider`]:
//!
//! - **Spotify** serves its own line-synced lyrics, but only to a signed-in
//!   user. That provider will take the user's own token from an environment
//!   variable at run time; it is never written into the source, never
//!   passed on the command line (where `ps` would show it), and never put in
//!   a log field or a span — record that one was *presented*, not what it
//!   is.
//! - **Deezer** and **Musixmatch** are the same shape: a user-supplied
//!   token from the environment, the same rule for it.
//!
//! The trait is deliberately synchronous and object-safe, so a list of
//! `Box<dyn LyricsProvider>` can be walked in the user's order.

/// What a provider is asked for.
#[derive(Debug, Clone, Copy)]
pub struct Query<'a> {
    pub title: &'a str,
    /// One artist credit, or `None` to search by title alone.
    pub artist: Option<&'a str>,
}

/// One version of a song a provider knows, with its lyrics if it has them.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Which provider found it ([`LyricsProvider::name`]).
    pub provider: &'static str,
    /// The provider's own id — enough to fetch this exact version again.
    pub id: String,
    pub track: String,
    pub artist: String,
    pub album: Option<String>,
    /// Length of the recording the lyrics were timed against, in seconds.
    pub duration: Option<f64>,
    /// Line-timed LRC text, verbatim from the provider.
    pub synced: Option<String>,
}

impl Candidate {
    /// Whether this carries line-timed lyrics — the only kind worth writing.
    pub fn is_synced(&self) -> bool {
        self.synced.as_deref().is_some_and(|s| !s.trim().is_empty())
    }

    /// A live recording, by its title: "Who Else - Live", "Praise (Live)".
    /// Only ever a tie-breaker — a live version at the right length is the
    /// right version.
    pub fn is_live(&self) -> bool {
        self.track
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word.eq_ignore_ascii_case("live"))
    }
}

/// A source of line-synced lyrics.
pub trait LyricsProvider {
    /// Short, stable name — written into the `.lrc` beside the id.
    fn name(&self) -> &'static str;

    /// Every version the provider holds for this title and artist, synced
    /// or not; the caller filters.
    ///
    /// # Errors
    ///
    /// A transport or decoding failure. "Nothing found" is `Ok(vec![])`.
    fn search(&self, query: &Query<'_>) -> eyre::Result<Vec<Candidate>>;

    /// One version by the provider's own id.
    ///
    /// # Errors
    ///
    /// A transport or decoding failure, or no such id.
    fn get(&self, id: &str) -> eyre::Result<Candidate>;
}
