//! [LRCLIB](https://lrclib.net) — the open, community lyrics database.
//! No account, no token: `GET /api/search` for candidates, `GET
//! /api/get/{id}` for one by id.

use std::time::Duration;

use serde::Deserialize;

use super::provider::{Candidate, LyricsProvider, Query};

const BASE: &str = "https://lrclib.net/api";

/// LRCLIB asks every client to name itself, its version and its home.
const USER_AGENT: &str = concat!(
    "FastTrackStudio-Session/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/FastTrackStudios/session)"
);

pub struct Lrclib {
    agent: ureq::Agent,
}

impl Lrclib {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build();
        Self { agent }
    }

    fn fetch(request: ureq::Request) -> eyre::Result<String> {
        match request.call() {
            Ok(response) => Ok(response.into_string()?),
            Err(ureq::Error::Status(code, _)) => Err(eyre::eyre!("lrclib answered HTTP {code}")),
            Err(e) => Err(eyre::eyre!("reaching lrclib: {e}")),
        }
    }
}

/// One record as LRCLIB returns it, from both endpoints.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    id: u64,
    track_name: Option<String>,
    artist_name: Option<String>,
    album_name: Option<String>,
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    synced_lyrics: Option<String>,
}

impl From<Record> for Candidate {
    fn from(r: Record) -> Self {
        Self {
            provider: "lrclib",
            id: r.id.to_string(),
            track: r.track_name.unwrap_or_default(),
            artist: r.artist_name.unwrap_or_default(),
            album: r.album_name.filter(|a| !a.trim().is_empty()),
            duration: r.duration.filter(|d| d.is_finite() && *d > 0.0),
            // An instrumental carries no words, whatever the field says.
            synced: r.synced_lyrics.filter(|_| !r.instrumental),
        }
    }
}

/// A `/api/search` response body.
pub fn parse_search(body: &str) -> eyre::Result<Vec<Candidate>> {
    let records: Vec<Record> = serde_json::from_str(body)?;
    Ok(records.into_iter().map(Candidate::from).collect())
}

/// A `/api/get/{id}` response body.
pub fn parse_get(body: &str) -> eyre::Result<Candidate> {
    let record: Record = serde_json::from_str(body)?;
    Ok(record.into())
}

impl LyricsProvider for Lrclib {
    fn name(&self) -> &'static str {
        "lrclib"
    }

    fn search(&self, query: &Query<'_>) -> eyre::Result<Vec<Candidate>> {
        let mut request = self
            .agent
            .get(&format!("{BASE}/search"))
            .query("track_name", query.title);
        if let Some(artist) = query.artist {
            request = request.query("artist_name", artist);
        }
        parse_search(&Self::fetch(request)?)
    }

    fn get(&self, id: &str) -> eyre::Result<Candidate> {
        let id: u64 = id
            .trim()
            .parse()
            .map_err(|_| eyre::eyre!("an LRCLIB id is a number, not {id:?}"))?;
        parse_get(&Self::fetch(self.agent.get(&format!("{BASE}/get/{id}")))?)
    }
}
