//! TONE3000 tone library — session and downloads.
//!
//! [TONE3000](https://www.tone3000.com) hosts the large public library of NAM
//! captures and impulse responses. This crate is the engine's half of that
//! integration: it holds the OAuth session, fetches models the user picked,
//! and writes them into the local NAM library where
//! [`signal_nam`]'s scanner and catalog already index them.
//!
//! # Why the engine owns this
//!
//! The rig core is headless and every GUI is a remote, so the session lives
//! here rather than in whichever UI happens to be attached. That is not only
//! our own rule — it is forced twice over:
//!
//! - A plugin editor renders through Blitz, which is a renderer and not a
//!   browser, so it cannot host the authorization page at all.
//! - The browser remote is served over plain HTTP on the LAN, where
//!   `WebCrypto` is unavailable (non-secure origin). PKCE generated in that
//!   page would work on localhost and fail silently on a phone. So PKCE is
//!   generated *here*, and the UI is handed the finished authorize URL.
//!
//! A GUI therefore only ever: asks for an authorize URL, opens it in the
//! system browser, and hands back the callback it receives. Tokens never
//! cross to the client.
//!
//! # What the API terms require of us
//!
//! Downloads are per-user and on request — the catalog may not be bulk
//! fetched, mirrored or cached, so there is deliberately no "sync the
//! library" operation here. Creator and licence metadata travel with every
//! file into [`signal_nam::nam_file::Provenance`], because stripping them is
//! forbidden and because a download outlives the session that made it.

mod session;
mod store;

pub use session::{AuthStart, Session, SessionError};
pub use store::{DownloadOutcome, TokenStore, Tokens};
