//! A small HTTP client for downloads with progress (Node.js builds, npm metadata and
//! tarballs). The system proxy settings (`HTTPS_PROXY`, …) are honoured.

use std::time::Duration;

use serde::de::DeserializeOwned;

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Http {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Http { client })
    }

    /// A client that never uses a proxy (tests talk to a server on localhost).
    pub fn direct() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Http { client })
    }

    async fn get(&self, url: &str) -> Result<reqwest::Response, String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("cannot reach {url}: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("{url}: HTTP {}", response.status()));
        }
        Ok(response)
    }

    pub async fn text(&self, url: &str) -> Result<String, String> {
        self.get(url)
            .await?
            .text()
            .await
            .map_err(|e| format!("{url}: {e}"))
    }

    pub async fn json<T: DeserializeOwned>(&self, url: &str) -> Result<T, String> {
        let text = self.text(url).await?;
        serde_json::from_str(&text).map_err(|e| format!("{url}: unexpected response: {e}"))
    }

    /// Download `url` into memory, reporting `(received, total)` as chunks arrive.
    pub async fn bytes(
        &self,
        url: &str,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>, String> {
        let mut response = self.get(url).await?;
        let total = response.content_length();
        let mut data = Vec::with_capacity(total.unwrap_or(0).min(256 << 20) as usize);
        progress(0, total);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("download of {url} failed: {e}"))?
        {
            data.extend_from_slice(&chunk);
            progress(data.len() as u64, total);
        }
        Ok(data)
    }
}
