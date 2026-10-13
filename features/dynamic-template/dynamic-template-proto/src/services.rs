//! Service definitions for dynamic template organization
//!
//! Defines the `DynamicTemplateService` trait for organizing items into
//! track hierarchies.

use crate::types::{
    ExtractMetadataRequest, ExtractMetadataResponse, GroupInfo, OrganizeRequest, OrganizeResponse,
};
use vox::service;

/// Service for dynamic template organization
///
/// This service provides methods for organizing audio items (e.g., audio files,
/// media items) into hierarchical track structures based on pattern matching
/// against known instrument groups.
///
/// # Example
///
/// ```ignore
/// use dynamic_template_proto::{DynamicTemplateServiceClient, OrganizeRequest};
///
/// let client = DynamicTemplateServiceClient::new(handle);
/// let response = client.organize(OrganizeRequest::new(vec![
///     "Kick In.wav".to_string(),
///     "Snare Top.wav".to_string(),
/// ])).await?;
///
/// println!("Created {} tracks", response.tracks_created);
/// ```
#[service]
pub trait DynamicTemplateService {
    /// Organize items into a track hierarchy
    ///
    /// Takes a list of item names and organizes them into a hierarchical
    /// track structure based on pattern matching.
    ///
    /// # Arguments
    ///
    /// * `request` - The organize request containing items and options
    ///
    /// # Returns
    ///
    /// An `OrganizeResponse` containing the organized hierarchy and metadata
    async fn organize(&self, request: OrganizeRequest) -> OrganizeResponse;

    /// Get available group names
    ///
    /// Returns a list of all supported instrument groups (e.g., Drums, Bass,
    /// Guitars, Keys, Vocals, etc.)
    async fn list_groups(&self) -> Vec<GroupInfo>;

    /// Extract metadata from a single item name
    ///
    /// Parses an item name and extracts any embedded metadata such as tempo,
    /// song name, equipment names, Pro Tools markers, etc.
    ///
    /// # Arguments
    ///
    /// * `request` - The request containing the item name
    ///
    /// # Returns
    ///
    /// An `ExtractMetadataResponse` with the extracted metadata and cleaned name
    async fn extract_metadata(&self, request: ExtractMetadataRequest) -> ExtractMetadataResponse;
}
