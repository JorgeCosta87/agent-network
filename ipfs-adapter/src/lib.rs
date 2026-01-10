use anyhow::{Context, Result};
use reqwest::{Client, multipart};
use serde::Deserialize;
use std::time::Duration;
use bytes::Bytes;

#[derive(Clone)]
pub struct IpfsClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct IpfsAddResponse {
    #[serde(rename = "Name")]
    _name: String,
    #[serde(rename = "Hash")]
    hash: String,
    #[serde(rename = "Size")]
    _size: String,
}

#[derive(Debug, Deserialize)]
struct _IpfsPinResponse {
    #[serde(rename = "Pins")]
    _pins: Vec<String>,
}

impl IpfsClient {
    pub fn new(url: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            base_url: url.trim_end_matches('/').to_string(),
        }
    }

    /// Uploads data to IPFS and pins it locally
    /// Returns the CID (Content Identifier)
    pub async fn add(&self, data: Vec<u8>) -> Result<String> {
        let part = multipart::Part::bytes(data)
            .file_name("data"); // IPFS needs a filename even if dummy

        let form = multipart::Form::new().part("file", part);

        let response = self.client
            .post(format!("{}/api/v0/add?pin=true", self.base_url))
            .multipart(form)
            .send()
            .await
            .context("Failed to connect to IPFS node")?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("IPFS add failed: {}", error_text);
        }

        let result: IpfsAddResponse = response.json().await
            .context("Failed to parse IPFS response")?;

        Ok(result.hash)
    }

    /// Retrieves data from IPFS by CID
    pub async fn cat(&self, cid: &str) -> Result<Bytes> {
        let response = self.client
            .post(format!("{}/api/v0/cat?arg={}", self.base_url, cid))
            .send()
            .await
            .context("Failed to connect to IPFS node")?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("IPFS cat failed: {}", error_text);
        }

        let bytes = response.bytes().await
            .context("Failed to read data from IPFS")?;

        Ok(bytes)
    }

    /// Pins a CID locally to ensure redundancy
    pub async fn pin(&self, cid: &str) -> Result<()> {
        let response = self.client
            .post(format!("{}/api/v0/pin/add?arg={}&recursive=true", self.base_url, cid))
            .send()
            .await
            .context("Failed to connect to IPFS node")?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("IPFS pin failed: {}", error_text);
        }

        Ok(())
    }

    /// Unpins a CID (garbage collection will eventually remove it)
    pub async fn unpin(&self, cid: &str) -> Result<()> {
        let response = self.client
            .post(format!("{}/api/v0/pin/rm?arg={}", self.base_url, cid))
            .send()
            .await
            .context("Failed to connect to IPFS node")?;

        // Ignore error if not pinned
        if !response.status().is_success() {
            return Ok(());
        }

        Ok(())
    }
}
