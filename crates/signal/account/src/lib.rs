//! The `FastTrackStudio` account, on the engine.
//!
//! One account across Task, Session, Signal, Keyflow and Ignition, served
//! by `auth.fasttrackstudio.app`. This crate is Signal's half: sign in to
//! that issuer, keep the session, and read the third-party tokens it
//! brokers on the account's behalf.
//!
//! # Why the engine holds this too
//!
//! The same reason it holds the TONE3000 session: the authorization page is
//! a web application, a Blitz plugin editor is a renderer and not a browser,
//! and the browser remote is served over plain HTTP on the LAN where
//! `WebCrypto` — and so PKCE — is unavailable. Every GUI is a courier for a
//! URL in one direction and a callback in the other.
//!
//! # What the brokered token buys
//!
//! Signing in to TONE3000 directly (see `signal-tone3000`) works and is
//! kept. But TONE3000 rotates its refresh token on every use, so a session
//! copied between machines invalidates itself, and each machine has to do
//! its own authorization. Linking the account once at
//! `auth.fasttrackstudio.app/account` moves the refreshing to one place; a
//! device then asks the issuer for a short-lived access token and never
//! holds the refresh token at all.
//!
//! So: **sign in to `FastTrackStudio` on a new machine, and the captures are
//! already there.**

mod linked;
mod session;

pub use linked::LinkedToken;
pub use session::{Account, AccountConfig, AccountError, AccountStatus, AuthStart};

/// The provider id TONE3000 is linked under, and the OIDC scope that
/// authorizes reading its token. They are the same word by coincidence of
/// naming, not by rule — the issuer maps one to the other.
pub const TONE3000_PROVIDER: &str = "tone3000";
