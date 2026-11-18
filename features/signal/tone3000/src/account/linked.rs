//! Reading a linked third-party token from the issuer.
//!
//! `GET /oauth2/linked-token?provider=…` answers with a **short-lived
//! access token** for a service the account has linked, and never with the
//! refresh token. The issuer refreshes centrally and keeps the rotation to
//! itself, which is the whole reason this is worth asking for rather than
//! holding a session per machine.

use crate::account::session::{Account, AccountError};

/// A third-party token the issuer holds on the account's behalf.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct LinkedToken {
    /// The provider id, echoed back.
    #[serde(default)]
    pub provider: String,
    /// The handle at that provider, when the issuer could read it live.
    /// `None` does not mean the token is bad — the lookup is best-effort.
    #[serde(default)]
    pub login: Option<String>,
    /// The provider's stable id for the account.
    #[serde(default)]
    pub account_id: String,
    /// The credential. Short-lived; ask again rather than storing it.
    pub access_token: String,
    #[serde(default)]
    pub scope: Option<String>,
}

impl Account {
    /// Ask the issuer for the account's linked token at `provider`.
    ///
    /// Not cached here on purpose. The token is short-lived and the issuer
    /// is the only thing that knows when it was refreshed; a copy kept on
    /// this side would be a second expiry to reason about, and the call is
    /// one request against a service the engine is already talking to.
    ///
    /// # Errors
    ///
    /// - [`AccountError::NotSignedIn`] — no `FastTrackStudio` session.
    /// - [`AccountError::Issuer`] — the account has not linked that
    ///   provider (`not_linked`), the session lacks the provider's scope
    ///   (`insufficient_scope`), or the request failed.
    async fn linked_request(
        &self,
        provider: &str,
        token: &str,
    ) -> Result<reqwest::Response, AccountError> {
        self.http()
            .get(format!("{}/oauth2/linked-token", self.issuer()))
            .query(&[("provider", provider)])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| AccountError::Issuer(e.to_string()))
    }

    pub async fn linked_token(&self, provider: &str) -> Result<LinkedToken, AccountError> {
        let mut token = self.fresh_access_token(false).await?;
        let mut response = self.linked_request(provider, &token).await?;
        // Refused as expired or unknown: refresh once and ask again, rather
        // than make the person sign in because a clock drifted.
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            token = self.fresh_access_token(true).await?;
            response = self.linked_request(provider, &token).await?;
        }

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| AccountError::Issuer(e.to_string()))?;
        if !status.is_success() {
            // The issuer's own words: "not_linked" and "insufficient_scope"
            // call for completely different actions by the person reading
            // them — link the account, versus sign in again for the scope.
            return Err(AccountError::Issuer(describe(status.as_u16(), &text)));
        }
        serde_json::from_str(&text)
            .map_err(|e| AccountError::Issuer(format!("unreadable linked-token response: {e}")))
    }
}

/// Turn the issuer's JSON error into one line worth showing.
fn describe(status: u16, body: &str) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let field = |key: &str| {
        parsed
            .as_ref()
            .and_then(|v| v.get(key))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
    };
    match (field("error"), field("message")) {
        (Some(error), Some(message)) => format!("{error}: {message}"),
        (Some(error), None) => error,
        (None, Some(message)) => message,
        (None, None) => format!("the issuer answered {status}"),
    }
}
