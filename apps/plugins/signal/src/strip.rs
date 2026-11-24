//! The kit's mixer as strips: `signal.chain/1`, typed.
//!
//! One [`Strip`] per port of the session (a track of the kit), its phases
//! in order, each a run of inserts (the blocks). The same model Studio's
//! mixer view reads from the same JSON (`SIGNAL_SESSION`), kept field for
//! field the contract's, so the two can share one component later; it is
//! the JSON, deserialized, and nothing of either side's.

use serde::Deserialize;

/// `SIGNAL_SESSION`'s answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Session {
    pub session: Option<String>,
    #[serde(default)]
    pub ports: Vec<Strip>,
}

/// One port's chain: a strip.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Strip {
    #[serde(default)]
    pub revision: u64,
    pub port: Option<String>,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub phases: Vec<PhaseStrip>,
    #[serde(default)]
    pub midi: Option<Midi>,
}

impl Strip {
    /// The track's own name: the last part of its path.
    #[must_use]
    pub fn name(&self) -> &str {
        self.port
            .as_deref()
            .and_then(|p| p.rsplit('/').next())
            .unwrap_or("Signal")
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Midi {
    #[serde(default)]
    pub publishes_hits: bool,
}

/// One phase of a strip: its inserts.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PhaseStrip {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub bypass: bool,
    #[serde(default)]
    pub makeup_db: f32,
    #[serde(default)]
    pub blocks: Vec<Insert>,
}

/// One block: an insert on the strip.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Insert {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub bypass: bool,
    #[serde(default)]
    pub status: String,
    pub key: Option<Key>,
    #[serde(default)]
    pub params: Vec<ParamView>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Key {
    pub from: String,
    pub via: String,
}

/// One parameter as a strip shows it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ParamView {
    pub id: String,
    pub clap_id: Option<String>,
    pub name: String,
    pub text: String,
    #[serde(default)]
    pub normalized: f32,
}

impl Session {
    /// From `SIGNAL_SESSION`'s JSON.
    #[must_use]
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_reads_as_strips() {
        let json = r#"{"session":"s","ports":[{"schema":"signal.chain/1","revision":3,"port":"Drums/Kick","mode":"per-track",
            "phases":[{"id":"rescue","name":"Rescue","bypass":false,"makeup_db":0.0,"blocks":[{"id":"rescue.gate","kind":"gate","name":"Gate",
            "bypass":false,"status":"ok","key":{"from":"Drums/Kick/Sum/Trig","via":"midi"},"params":[{"id":"threshold","clap_id":"b001",
            "name":"Threshold","text":"-40.0 dB","normalized":0.43}]}]}]}]}"#;
        let s = Session::parse(json);
        assert_eq!(s.ports[0].name(), "Kick");
        assert_eq!(
            s.ports[0].phases[0].blocks[0]
                .key
                .as_ref()
                .map(|k| k.via.as_str()),
            Some("midi")
        );
        assert_eq!(s.ports[0].phases[0].blocks[0].params[0].text, "-40.0 dB");
    }
}
