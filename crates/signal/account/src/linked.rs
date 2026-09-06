//! Reading a linked third-party token from the issuer.
//!
//! `GET /oauth2/linked-token?provider=…` answers with a **short-lived
//! access token** for a service the account has linked, and never with the
//! refresh token. The issuer refreshes centrally and keeps the rotation to
//! itself, which is the whole reason this is worth asking for rather than
//! holding a session per machine.

use crate::session::{Account, AccountError};

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
    pub async fn linked_token(&self, provider: &str) -> Result<LinkedToken, AccountError> {
        let token = self.access_token()?;
        let response = self
            .http()
            .get(format!("{}/oauth2/linked-token", self.issuer()))
            .query(&[("provider", provider)])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| AccountError::Issuer(e.to_string()))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AccountConfig;
    use auth_client::{FileTokenStore, StoredSession, TokenStore as _};
    use wiremock::matchers::{bearer_token, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn signed_in(server: &MockServer) -> (Account, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("session.json");
        FileTokenStore::new(session_path.clone())
            .save(&StoredSession::new("fts-access-token"))
            .unwrap();
        let account = Account::new(AccountConfig {
            issuer: server.uri(),
            client_id: "signal".into(),
            redirect_uri: "http://localhost:4040/account/callback".into(),
            session_path,
        });
        (account, dir)
    }

    #[tokio::test]
    async fn a_linked_token_comes_back_with_its_handle() {
        let server = MockServer::start().await;
        let (account, _dir) = signed_in(&server);
        Mock::given(method("GET"))
            .and(path("/oauth2/linked-token"))
            .and(query_param("provider", "tone3000"))
            // The FastTrackStudio session is what authorizes the read.
            .and(bearer_token("fts-access-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "provider": "tone3000",
                "login": "acodywright",
                "account_id": "57af",
                "access_token": "t3k-access-token",
                "scope": null
            })))
            .mount(&server)
            .await;

        let linked = account
            .linked_token("tone3000")
            .await
            .expect("linked token");
        assert_eq!(linked.access_token, "t3k-access-token");
        assert_eq!(linked.login.as_deref(), Some("acodywright"));
    }

    /// "You have not linked it" and "your session may not read it" are
    /// different problems with different fixes, and the message has to say
    /// which — one is a click, the other is signing in again.
    #[tokio::test]
    async fn the_issuers_refusal_is_passed_through_verbatim() {
        let server = MockServer::start().await;
        let (account, _dir) = signed_in(&server);
        Mock::given(method("GET"))
            .and(path("/oauth2/linked-token"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": "not_linked",
                "message": "no such account is linked to this user"
            })))
            .mount(&server)
            .await;

        let err = account.linked_token("tone3000").await.unwrap_err();
        assert!(err.to_string().contains("not_linked"), "{err}");
        assert!(
            err.to_string().contains("no such account is linked"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn asking_without_a_session_does_not_reach_the_network() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let account = Account::new(AccountConfig {
            issuer: server.uri(),
            client_id: "signal".into(),
            redirect_uri: "http://localhost:4040/account/callback".into(),
            session_path: dir.path().join("absent.json"),
        });
        let err = account.linked_token("tone3000").await.unwrap_err();
        assert!(matches!(err, AccountError::NotSignedIn), "{err}");
        assert!(
            server
                .received_requests()
                .await
                .unwrap_or_default()
                .is_empty(),
            "a signed-out engine must not call the issuer"
        );
    }

    #[test]
    fn an_unparseable_error_body_still_says_something() {
        assert_eq!(
            describe(503, "<html>gateway</html>"),
            "the issuer answered 503"
        );
        assert_eq!(
            describe(403, r#"{"error":"insufficient_scope"}"#),
            "insufficient_scope"
        );
    }
}
