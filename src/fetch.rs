use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct DenoVersionsResponse {
    /// The latest version of the module available.
    pub latest: String,
    /// All of the published versions of the module.
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionMetadataResponse {
    pub upload_options: UploadOptions,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadOptions {
    #[serde(rename = "type")]
    pub upload_options_type: String,
    #[serde(rename = "ref")]
    pub upload_options_ref: String,
    pub repository: String,
}

impl UploadOptions {
    /// Creates a link to where the library can be downloaded as a tarball.
    pub fn tarball_url(&self) -> Option<String> {
        match self.upload_options_type.as_str() {
            "github" => Some(format!(
                "https://api.github.com/repos/{}/tarball/{}",
                self.repository, self.upload_options_ref
            )),
            _ => None,
        }
    }
}

/// Fetches metadata about the versions for the provided module.
pub async fn fetch_versions_for_module(
    client: &Client,
    module_name: &str,
) -> Result<DenoVersionsResponse, FetchError> {
    log::debug!("Fetching versions for module {}.", module_name);
    let response = client
        .get(&format!(
            "https://cdn.deno.land/{}/meta/versions.json",
            module_name
        ))
        .send()
        .await?;

    // Deno returns a non-json content type if the module doesn't exist.
    match response.headers().get("Content-Type").map(|v| v.to_str()) {
        Some(Ok("application/json")) => response.json().await.map_err(FetchError::from),
        _ => Err(FetchError::MetadataNotPresent),
    }
}

/// Fetches the metadata about the specified version for a module.
pub async fn fetch_version_metadata(
    client: &Client,
    module_name: &str,
    version: &str,
) -> Result<VersionMetadataResponse, FetchError> {
    log::debug!("Fetching version {} for module {}.", version, module_name);
    let response = client
        .get(&format!(
            "https://cdn.deno.land/{}/versions/{}/meta/meta.json",
            module_name, version
        ))
        .send()
        .await?;

    // Deno returns a non-json content type if the module doesn't exist.
    match response.headers().get("Content-Type").map(|v| v.to_str()) {
        Some(Ok("application/json")) => response.json().await.map_err(FetchError::from),
        _ => Err(FetchError::MetadataNotPresent),
    }
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("{0}")]
    HTTP(#[from] reqwest::Error),
    #[error("resource has no metadata")]
    MetadataNotPresent,
}
