//! Input — High-level client wrapper for input interception
//!
//! Provides ergonomic access to the InputService for subscribing to
//! keyboard/mouse events and managing key filters from extension processes.

use crate::DawClients;
use std::sync::Arc;

/// Handle for input interception and key event streaming.
pub struct Input {
    clients: Arc<DawClients>,
}

impl Input {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    // `subscribe` retired with the architect::rpc port — input event
    // streaming lives on a sibling trait.

    /// Upload a key filter configuration.
    ///
    /// The host uses this to decide synchronously which keys to eat.
    pub async fn set_key_filter(&self, filter: daw_proto::KeyFilter) -> crate::Result<()> {
        self.clients.input.set_key_filter(filter).await?;
        Ok(())
    }

    /// Get the current key filter.
    pub async fn get_key_filter(&self) -> crate::Result<daw_proto::KeyFilter> {
        Ok(self.clients.input.get_key_filter().await?)
    }

    /// Enable or disable input interception.
    pub async fn set_enabled(&self, enabled: bool) -> crate::Result<()> {
        self.clients.input.set_enabled(enabled).await?;
        Ok(())
    }

    /// Check if input interception is currently enabled.
    pub async fn is_enabled(&self) -> crate::Result<bool> {
        Ok(self.clients.input.is_enabled().await?)
    }

    /// Execute a DAW action by command name or numeric ID.
    ///
    /// Extensions call this after resolving a keybinding to an action.
    pub async fn execute_action(&self, action_id: &str) -> crate::Result<()> {
        self.clients
            .input
            .execute_action(action_id.to_string())
            .await?;
        Ok(())
    }
}
