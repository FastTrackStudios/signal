//! The `AccountAuth` RPC surface — [`crate::Account`] handed to a GUI.
//!
//! A thin wrapper rather than putting `#[derive(HasDispatcher)]` on
//! [`Account`] itself: `Account` is shared as `Arc<Account>` everywhere else
//! (the engine's HTTP callback route holds the same `Arc`, per
//! `apps/desktop/src/engine_tone3000.rs::extend_account`, so a sign-in
//! redeemed there is visible here on the next `status()` call — no second
//! copy of the session to drift out of step), and a vox service backend
//! needs to be cheaply `Clone`, which an `Arc` already is.

use std::sync::Arc;

use architect::{HasDispatcher, Layer, Services, layers};
use signal_account_proto::account::{AccountAuth, Service};
use signal_account_proto::{AccountStatus, AuthRequest};

use crate::Account;

/// Mount this on any vox transport (`.router()`) to expose [`Account`] to a
/// GUI. Cheap to clone — one `Arc` underneath, the same one the engine's
/// `/account/callback` route redeems into.
#[derive(Clone, HasDispatcher)]
pub struct AccountBackend {
    account: Arc<Account>,
}

impl AccountBackend {
    #[must_use]
    pub const fn new(account: Arc<Account>) -> Self {
        Self { account }
    }

    /// The composed service router — mount on any vox transport.
    #[must_use]
    pub fn router(&self) -> architect::LayerRouter {
        self.clone().into_router()
    }
}

impl AccountAuth for AccountBackend {
    fn status(&self) -> AccountStatus {
        let s = self.account.status();
        AccountStatus {
            signed_in: s.signed_in,
            email: s.email,
            error: s.error,
        }
    }

    fn begin_sign_in(&self) -> AuthRequest {
        let start = self.account.begin_sign_in();
        AuthRequest {
            authorize_url: start.authorize_url,
            request_id: start.request_id,
        }
    }

    fn sign_out(&self) {
        if let Err(e) = self.account.sign_out() {
            tracing::warn!("account: sign_out failed: {e}");
        }
    }
}

impl Services for AccountBackend {
    fn layers() -> impl Layer<Self> {
        layers![Service]
    }
}
