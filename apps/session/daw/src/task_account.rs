//! Signing this window in to Task, for its library: from another device
//! (`auth_client::device`, RFC 8628) — the window shows a code and a link,
//! its person approves it in a browser where they are signed in, and the
//! session it gets is kept (`auth_client::FileTokenStore`) so the next
//! launch is signed in already.
//!
//! The account server is the one the Task server trusts
//! (`central_auth` in its `/.well-known/task-server.json`); the token it
//! gives is the session every org on that server takes. Which orgs this
//! person is in comes from the same document, read with the token.
//!
//! Blocking, on the engine's runtime, like everything a window's start
//! screen runs on a thread of its own.

use std::path::PathBuf;
use std::time::Duration;

use auth_client::device::{self, DeviceCode, Poll};
use auth_client::{FileTokenStore, StoredSession, TokenStore};

/// The Task server a window signs in to unless told another
/// (`FTS_TASK_SERVER`, as a `wss://` or `https://` base).
pub const SERVER: &str = "https://task.fasttrackstudio.app";

/// The client this window is to the account server — the name its
/// person's session list shows.
const CLIENT_ID: &str = "session-app";

/// A signed-in person on a Task server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// `https://…` — the server, without the per-org path.
    pub server: String,
    pub session: StoredSession,
}

impl Account {
    /// The person, as a name to show and to be known by in a set.
    #[must_use]
    pub fn name(&self) -> String {
        self.session
            .email
            .as_deref()
            .and_then(|e| e.split('@').next())
            .filter(|n| !n.is_empty())
            .unwrap_or("Me")
            .to_owned()
    }

    /// The org `org`'s library, as this person.
    #[must_use]
    pub fn library(&self, org: &str) -> session_library::Library {
        session_library::Library {
            server: self
                .server
                .replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1),
            org: org.to_owned(),
            token: Some(self.session.token.clone()),
        }
    }
}

/// The server a window signs in to: `FTS_TASK_SERVER` or [`SERVER`], as an
/// `https://` base.
#[must_use]
pub fn server() -> String {
    std::env::var("FTS_TASK_SERVER")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| SERVER.to_owned())
        .trim()
        .trim_end_matches('/')
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1)
}

/// Where the session is kept.
fn store() -> Option<FileTokenStore> {
    let dir: PathBuf = dirs::data_dir()?.join("Session");
    Some(FileTokenStore::new(dir.join("task-session.json")))
}

/// The person signed in last time, if the session was kept.
#[must_use]
pub fn signed_in() -> Option<Account> {
    let session = store()?
        .load()
        .inspect_err(|e| tracing::warn!(error = %e, "task: the kept session could not be read"))
        .ok()??;
    Some(Account {
        server: server(),
        session,
    })
}

/// Forget the kept session.
pub fn sign_out() {
    if let Some(Err(e)) = store().map(|s| s.clear()) {
        tracing::warn!(error = %e, "task: the kept session could not be cleared");
    }
}

/// A sign-in started: what to show, and where to poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    /// The account server (`central_auth`).
    pub issuer: String,
    pub code: DeviceCode,
}

impl Started {
    /// The link to open to approve it, the code filled in.
    #[must_use]
    pub fn link(&self) -> String {
        format!("{}{}", self.issuer, self.code.verification_uri_complete)
    }
}

/// Start signing in to `server`: ask its account server for a code.
///
/// # Errors
///
/// The server or its account server could not be reached, or said
/// something this does not understand.
pub fn start(server: &str) -> eyre::Result<Started> {
    let runtime = crate::open::engine_runtime()?;
    runtime.block_on(async {
        let http = reqwest::Client::new();
        let well_known = http
            .get(format!("{server}/.well-known/task-server.json"))
            .send()
            .await?
            .text()
            .await?;
        let issuer = device::issuer_from_well_known(&well_known)
            .ok_or_else(|| eyre::eyre!("{server} names no account server"))?;
        let started = http
            .post(format!("{issuer}{}", device::CODE_PATH))
            .header("content-type", "application/json")
            .body(device::start_body(CLIENT_ID))
            .send()
            .await?
            .text()
            .await?;
        let code = device::parse_start(&started)?;
        tracing::info!(auth.flow = "device", auth.issuer = %issuer, "task: sign-in started");
        Ok(Started { issuer, code })
    })
}

/// Wait for `started` to be approved (or refused, or to run out), polling
/// as the server asks; the session is kept once it is.
///
/// # Errors
///
/// The sign-in was refused or ran out, or the account server stopped
/// answering sensibly.
pub fn finish(server: &str, started: &Started) -> eyre::Result<Account> {
    let runtime = crate::open::engine_runtime()?;
    let session = runtime.block_on(async {
        let http = reqwest::Client::new();
        let mut wait = started.code.interval();
        let deadline = std::time::Instant::now()
            + Duration::from_secs(started.code.expires_in_seconds.max(60));
        loop {
            architect::platform::sleep(wait).await;
            if std::time::Instant::now() > deadline {
                eyre::bail!(device::DeviceError::Expired);
            }
            let answer = http
                .post(format!("{}{}", started.issuer, device::TOKEN_PATH))
                .header("content-type", "application/json")
                .body(device::poll_body(&started.code.device_code, CLIENT_ID))
                .send()
                .await?;
            let ok = answer.status().is_success();
            match device::parse_poll(ok, &answer.text().await?)? {
                Poll::Approved(session) => break Ok::<_, eyre::Report>(session),
                Poll::Pending => {}
                Poll::SlowDown => wait += device::SLOW_DOWN_STEP,
            }
        }
    })?;
    tracing::info!(
        auth.flow = "device",
        auth.outcome = "approved",
        "task: signed in"
    );
    if let Some(Err(e)) = store().map(|s| s.save(&session)) {
        // Not fatal: signed in for now, asked again next launch.
        tracing::warn!(error = %e, "task: the session could not be kept");
    }
    Ok(Account {
        server: server.to_owned(),
        session,
    })
}

/// The orgs on the account's server this person is in (every org the
/// server hosts, when it does not say).
///
/// # Errors
///
/// The server could not be reached, or its answer read.
pub fn orgs(account: &Account) -> eyre::Result<Vec<String>> {
    let runtime = crate::open::engine_runtime()?;
    let doc: serde_json::Value = runtime.block_on(async {
        Ok::<_, eyre::Report>(
            reqwest::Client::new()
                .get(format!("{}/.well-known/task-server.json", account.server))
                .bearer_auth(&account.session.token)
                .send()
                .await?
                .json()
                .await?,
        )
    })?;
    // (slug, whether this person is in it — `None` when the server does
    // not say).
    let orgs: Vec<(String, Option<bool>)> = doc
        .get("orgs")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|org| {
            let slug = org.get("slug")?.as_str()?.to_owned();
            Some((slug, org.get("member").and_then(serde_json::Value::as_bool)))
        })
        .collect();
    let anyone = orgs.iter().all(|(_, member)| member.is_none());
    Ok(orgs
        .into_iter()
        .filter(|(_, member)| anyone || *member == Some(true))
        .map(|(slug, _)| slug)
        .collect())
}
