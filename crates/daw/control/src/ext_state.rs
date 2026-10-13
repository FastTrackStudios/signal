//! ExtState — Persistent Key-Value Storage Client
//!
//! Client-side handle for REAPER's persistent extension state storage.
//! Values are scoped by section (typically your extension name) and key.
//!
//! # Example
//!
//! ```no_run
//! # async fn example(daw: &daw_control::Daw) -> daw_control::Result<()> {
//! let ext = daw.ext_state();
//!
//! // Store a value (persistent across REAPER restarts)
//! ext.set("MyExt", "last_preset", "Clean Guitar", true).await?;
//!
//! // Read it back
//! if let Some(preset) = ext.get("MyExt", "last_preset").await? {
//!     println!("Last preset: {}", preset);
//! }
//!
//! // Check existence
//! if ext.has("MyExt", "last_preset").await? {
//!     println!("Key exists!");
//! }
//!
//! // Delete it
//! ext.delete("MyExt", "last_preset", true).await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::{DawClients, Error};

/// Handle for persistent key-value storage (REAPER's ExtState API).
///
/// Lightweight handle — cheap to clone. Created via `Daw::ext_state()`.
#[derive(Clone)]
pub struct ExtState {
    clients: Arc<DawClients>,
}

impl ExtState {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// Get a value by section and key.
    ///
    /// Returns `None` if the key doesn't exist or is empty.
    pub async fn get(&self, section: &str, key: &str) -> crate::Result<Option<String>> {
        Ok(self
            .clients
            .ext_state
            .get(section.to_string(), key.to_string())
            .await?)
    }

    /// Set a value. If `persist` is true, it survives REAPER restarts.
    pub async fn set(
        &self,
        section: &str,
        key: &str,
        value: &str,
        persist: bool,
    ) -> crate::Result<()> {
        self.clients
            .ext_state
            .set(
                section.to_string(),
                key.to_string(),
                value.to_string(),
                persist,
            )
            .await??;
        Ok(())
    }

    /// Delete a value. If `persist` is true, also removes from persistent storage.
    pub async fn delete(&self, section: &str, key: &str, persist: bool) -> crate::Result<()> {
        self.clients
            .ext_state
            .delete(section.to_string(), key.to_string(), persist)
            .await??;
        Ok(())
    }

    /// Check if a value exists for the given section and key.
    pub async fn has(&self, section: &str, key: &str) -> crate::Result<bool> {
        Ok(self
            .clients
            .ext_state
            .has(section.to_string(), key.to_string())
            .await?)
    }

    /// Project-scoped get: value stored inside the project file itself.
    ///
    /// Returns `None` if the key doesn't exist.
    pub async fn get_project(
        &self,
        project: daw_proto::ProjectContext,
        section: &str,
        key: &str,
    ) -> crate::Result<Option<String>> {
        Ok(self
            .clients
            .ext_state
            .get_project(project, section.to_string(), key.to_string())
            .await?)
    }

    /// Project-scoped set: value lives inside the project file.
    pub async fn set_project(
        &self,
        project: daw_proto::ProjectContext,
        section: &str,
        key: &str,
        value: &str,
    ) -> crate::Result<()> {
        self.clients
            .ext_state
            .set_project(
                project,
                section.to_string(),
                key.to_string(),
                value.to_string(),
            )
            .await??;
        Ok(())
    }

    /// Get a typed value by section and key, automatically deserializing from JSON.
    ///
    /// Returns `None` if the key doesn't exist or is empty.
    /// Returns an error if the value exists but cannot be deserialized.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example(ext: &daw_control::ExtState) -> daw_control::Result<()> {
    /// #[derive(serde::Deserialize)]
    /// struct Config {
    ///     volume: f32,
    ///     preset: String,
    /// }
    ///
    /// if let Some(config) = ext.get_typed::<Config>("MyExt", "config").await? {
    ///     println!("Volume: {}", config.volume);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_typed<T: serde::de::DeserializeOwned>(
        &self,
        section: &str,
        key: &str,
    ) -> crate::Result<Option<T>> {
        let Some(json_str) = self.get(section, key).await? else {
            return Ok(None);
        };

        match serde_json::from_str(&json_str) {
            Ok(value) => Ok(Some(value)),
            Err(e) => Err(Error::Other(format!(
                "Failed to deserialize ExtState value for {}/{}: {}",
                section, key, e
            ))),
        }
    }

    /// Set a typed value, automatically serializing to JSON.
    ///
    /// If `persist` is true, the value survives REAPER restarts.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example(ext: &daw_control::ExtState) -> daw_control::Result<()> {
    /// #[derive(serde::Serialize)]
    /// struct Config {
    ///     volume: f32,
    ///     preset: String,
    /// }
    ///
    /// let config = Config {
    ///     volume: 0.8,
    ///     preset: "Clean".to_string(),
    /// };
    ///
    /// ext.set_typed("MyExt", "config", &config, true).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_typed<T: serde::Serialize>(
        &self,
        section: &str,
        key: &str,
        value: &T,
        persist: bool,
    ) -> crate::Result<()> {
        let json_str = serde_json::to_string(value).map_err(|e| {
            Error::Other(format!(
                "Failed to serialize value for {}/{}: {}",
                section, key, e
            ))
        })?;

        self.set(section, key, &json_str, persist).await
    }
}
