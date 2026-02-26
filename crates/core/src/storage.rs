use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing;

// ─── Storage configuration ─────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StorageConfig {
    #[serde(rename = "local")]
    Local { path: String },
    #[serde(rename = "s3")]
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
        prefix: Option<String>,
    },
    #[serde(rename = "webdav")]
    WebDav {
        url: String,
        username: String,
        password: String,
        prefix: Option<String>,
    },
}

// ─── Storage trait ──────────────────────────────

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// List files in a directory (relative path)
    async fn list_files(&self, dir: &str) -> Result<Vec<String>>;

    /// List files recursively in a directory tree (relative path)
    async fn list_files_recursive(&self, dir: &str) -> Result<Vec<String>>;

    /// Read file contents
    async fn read_file(&self, path: &str) -> Result<Vec<u8>>;

    /// Get a local path for file (downloading temporarily if remote)
    async fn get_local_path(&self, path: &str) -> Result<PathBuf>;

    /// Check if a file exists
    async fn exists(&self, path: &str) -> Result<bool>;

    /// Get file size
    async fn file_size(&self, path: &str) -> Result<u64>;

    /// Write file contents
    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()>;

    /// Delete file
    async fn delete_file(&self, path: &str) -> Result<()>;

    /// Storage type name
    fn storage_type(&self) -> &str;
}

// ─── Local storage ──────────────────────────────

pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn list_files(&self, dir: &str) -> Result<Vec<String>> {
        let full_path = self.root.join(dir);
        let mut files = Vec::new();

        if !full_path.exists() {
            return Ok(files);
        }

        let mut entries = tokio::fs::read_dir(&full_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    files.push(format!("{}/{}", dir, name));
                }
            }
        }
        Ok(files)
    }

    async fn list_files_recursive(&self, dir: &str) -> Result<Vec<String>> {
        let full_path = self.root.join(dir);
        let mut files = Vec::new();
        if !full_path.exists() {
            return Ok(files);
        }

        let root = self.root.clone();
        let target = full_path.clone();
        for entry in walkdir::WalkDir::new(target)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(&root) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(files)
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = self.root.join(path);
        Ok(tokio::fs::read(&full_path).await?)
    }

    async fn get_local_path(&self, path: &str) -> Result<PathBuf> {
        Ok(self.root.join(path))
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.root.join(path).exists())
    }

    async fn file_size(&self, path: &str) -> Result<u64> {
        let meta = tokio::fs::metadata(self.root.join(path)).await?;
        Ok(meta.len())
    }

    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        let full_path = self.root.join(path);
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(full_path, data).await?;
        Ok(())
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let full_path = self.root.join(path);
        match tokio::fs::remove_file(full_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn storage_type(&self) -> &str {
        "local"
    }
}

// ─── S3 storage ─────────────────────────────────

pub struct S3Storage {
    bucket: Box<s3::Bucket>,
    prefix: String,
    cache_dir: PathBuf,
}

impl S3Storage {
    pub fn new(
        bucket_name: &str,
        region: &str,
        endpoint: Option<&str>,
        access_key: &str,
        secret_key: &str,
        prefix: Option<&str>,
        cache_dir: impl Into<PathBuf>,
    ) -> Result<Self> {
        let region = if let Some(ep) = endpoint {
            s3::Region::Custom {
                region: region.to_string(),
                endpoint: ep.to_string(),
            }
        } else {
            region.parse().unwrap_or(s3::Region::UsEast1)
        };

        let credentials =
            s3::creds::Credentials::new(Some(access_key), Some(secret_key), None, None, None)?;

        let bucket = s3::Bucket::new(bucket_name, region, credentials)?;
        let bucket = if endpoint.is_some() {
            // Custom endpoints (MinIO/Ceph, etc.) typically require path-style addressing.
            bucket.with_path_style()
        } else {
            bucket
        };

        let cache_dir = cache_dir.into();
        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            bucket,
            prefix: prefix.unwrap_or("").to_string(),
            cache_dir,
        })
    }

    fn full_key(&self, path: &str) -> String {
        if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.prefix.trim_end_matches('/'), path)
        }
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    async fn list_files(&self, dir: &str) -> Result<Vec<String>> {
        let key = self.full_key(dir);
        let prefix = if key.is_empty() {
            String::new()
        } else {
            format!("{}/", key.trim_matches('/'))
        };
        let results = self.bucket.list(prefix, Some("/".to_string())).await?;

        let mut files = Vec::new();
        for result in results {
            for obj in result.contents {
                let rel = if self.prefix.is_empty() {
                    obj.key.clone()
                } else {
                    obj.key
                        .strip_prefix(&format!("{}/", self.prefix))
                        .unwrap_or(&obj.key)
                        .to_string()
                };
                files.push(rel);
            }
        }
        Ok(files)
    }

    async fn list_files_recursive(&self, dir: &str) -> Result<Vec<String>> {
        let key = self.full_key(dir);
        let prefix = if key.is_empty() {
            String::new()
        } else {
            format!("{}/", key.trim_matches('/'))
        };
        let results = self.bucket.list(prefix, None).await?;
        let mut files = Vec::new();
        for result in results {
            for obj in result.contents {
                let rel = if self.prefix.is_empty() {
                    obj.key.clone()
                } else {
                    obj.key
                        .strip_prefix(&format!("{}/", self.prefix))
                        .unwrap_or(&obj.key)
                        .to_string()
                };
                files.push(rel);
            }
        }
        Ok(files)
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let key = self.full_key(path);
        let response = self.bucket.get_object(&key).await?;
        Ok(response.to_vec())
    }

    async fn get_local_path(&self, path: &str) -> Result<PathBuf> {
        // Download to cache if not exists
        let cache_path = self.cache_dir.join(path);
        if !cache_path.exists() {
            if let Some(parent) = cache_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let data = self.read_file(path).await?;
            tokio::fs::write(&cache_path, &data).await?;
            tracing::debug!("Cached S3 file: {} -> {:?}", path, cache_path);
        }
        Ok(cache_path)
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let key = self.full_key(path);
        let results = self.bucket.list(key.clone(), None).await?;
        for result in results {
            for obj in result.contents {
                if obj.key == key {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    async fn file_size(&self, path: &str) -> Result<u64> {
        let key = self.full_key(path);
        let (head, _) = self.bucket.head_object(&key).await?;
        Ok(head.content_length.unwrap_or(0) as u64)
    }

    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        let key = self.full_key(path);
        let res = self.bucket.put_object(&key, data).await?;
        if !(200..300).contains(&res.status_code()) {
            anyhow::bail!("S3 PUT failed: {}", res.status_code());
        }
        Ok(())
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let key = self.full_key(path);
        let res = self.bucket.delete_object(&key).await?;
        let code = res.status_code();
        if !(200..300).contains(&code) && code != 404 {
            anyhow::bail!("S3 DELETE failed: {}", code);
        }
        Ok(())
    }

    fn storage_type(&self) -> &str {
        "s3"
    }
}

// ─── WebDAV storage ─────────────────────────────

pub struct WebDavStorage {
    base_url: String,
    client: reqwest::Client,
    username: String,
    password: String,
    prefix: String,
    cache_dir: PathBuf,
}

impl WebDavStorage {
    pub fn new(
        url: &str,
        username: &str,
        password: &str,
        prefix: Option<&str>,
        cache_dir: impl Into<PathBuf>,
    ) -> Result<Self> {
        let client = reqwest::Client::builder().build()?;

        let cache_dir = cache_dir.into();
        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            base_url: url.trim_end_matches('/').to_string(),
            client,
            username: username.to_string(),
            password: password.to_string(),
            prefix: prefix.unwrap_or("").to_string(),
            cache_dir,
        })
    }

    fn full_url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        if self.prefix.is_empty() {
            format!("{}/{}", self.base_url, path)
        } else {
            format!(
                "{}/{}/{}",
                self.base_url,
                self.prefix.trim_end_matches('/'),
                path
            )
        }
    }

    async fn ensure_parent_dirs(&self, path: &str) -> Result<()> {
        let parent = std::path::Path::new(path).parent();
        let Some(parent) = parent else {
            return Ok(());
        };
        if parent.as_os_str().is_empty() {
            return Ok(());
        }

        let mut current = String::new();
        for part in parent.iter() {
            let segment = part.to_string_lossy();
            if segment.is_empty() {
                continue;
            }
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(&segment);
            let url = self.full_url(&current);
            let response = self
                .client
                .request(reqwest::Method::from_bytes(b"MKCOL")?, &url)
                .basic_auth(&self.username, Some(&self.password))
                .send()
                .await?;
            let status = response.status();
            if !(status.is_success()
                || status == reqwest::StatusCode::METHOD_NOT_ALLOWED
                || status == reqwest::StatusCode::CONFLICT)
            {
                anyhow::bail!("WebDAV MKCOL failed ({}): {}", current, status);
            }
        }
        Ok(())
    }

    fn extract_hrefs(body: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cursor = 0usize;
        while let Some(start_rel) = body[cursor..]
            .find("<D:href>")
            .or_else(|| body[cursor..].find("<d:href>"))
        {
            let start = cursor + start_rel + 8;
            let rest = &body[start..];
            if let Some(end_rel) = rest.find("</D:href>").or_else(|| rest.find("</d:href>")) {
                out.push(rest[..end_rel].to_string());
                cursor = start + end_rel + 9;
            } else {
                break;
            }
        }
        out
    }

    fn rel_from_href(&self, href: &str) -> Option<String> {
        let raw = href.trim();
        let path = if let Ok(url) = reqwest::Url::parse(raw) {
            url.path().to_string()
        } else {
            raw.to_string()
        };
        let mut rel = path.trim_start_matches('/').to_string();
        if !self.prefix.is_empty() {
            let prefix = self.prefix.trim_matches('/');
            if rel == prefix {
                return Some(String::new());
            }
            let full_prefix = format!("{}/", prefix);
            rel = rel.strip_prefix(&full_prefix)?.to_string();
        }
        Some(rel.trim_matches('/').to_string())
    }

    async fn propfind_entries_depth1(&self, dir: &str) -> Result<Vec<(String, bool)>> {
        let url = self.full_url(dir);
        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND")?, &url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body(r#"<?xml version="1.0" encoding="utf-8"?><propfind xmlns="DAV:"><prop><displayname/></prop></propfind>"#)
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!("WebDAV PROPFIND failed: {}", response.status());
        }

        let body = response.text().await?;
        let mut out = Vec::new();
        for href in Self::extract_hrefs(&body) {
            let is_dir = href.trim_end().ends_with('/');
            if let Some(rel) = self.rel_from_href(&href) {
                out.push((rel, is_dir));
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl StorageBackend for WebDavStorage {
    async fn list_files(&self, dir: &str) -> Result<Vec<String>> {
        let dir_norm = dir.trim_matches('/').to_string();
        let mut files = Vec::new();
        for (rel, is_dir) in self.propfind_entries_depth1(dir).await? {
            if rel.is_empty() || rel == dir_norm || is_dir {
                continue;
            }
            files.push(rel);
        }
        Ok(files)
    }

    async fn list_files_recursive(&self, dir: &str) -> Result<Vec<String>> {
        use std::collections::{HashSet, VecDeque};

        let mut queue = VecDeque::new();
        let root = dir.trim_matches('/').to_string();
        queue.push_back(root.clone());

        let mut seen_dirs = HashSet::new();
        if !root.is_empty() {
            seen_dirs.insert(root);
        }
        let mut files = Vec::new();

        while let Some(current) = queue.pop_front() {
            for (rel, is_dir) in self.propfind_entries_depth1(&current).await? {
                if rel.is_empty() || rel == current {
                    continue;
                }
                if is_dir {
                    if seen_dirs.insert(rel.clone()) {
                        queue.push_back(rel);
                    }
                } else {
                    files.push(rel);
                }
            }
        }

        Ok(files)
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let url = self.full_url(path);
        let response = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!("WebDAV GET failed: {}", response.status());
        }
        Ok(response.bytes().await?.to_vec())
    }

    async fn get_local_path(&self, path: &str) -> Result<PathBuf> {
        let cache_path = self.cache_dir.join(path);
        if !cache_path.exists() {
            if let Some(parent) = cache_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let data = self.read_file(path).await?;
            tokio::fs::write(&cache_path, &data).await?;
            tracing::debug!("Cached WebDAV file: {} -> {:?}", path, cache_path);
        }
        Ok(cache_path)
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let url = self.full_url(path);
        let response = self
            .client
            .head(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        Ok(response.status().is_success())
    }

    async fn file_size(&self, path: &str) -> Result<u64> {
        let url = self.full_url(path);
        let response = self
            .client
            .head(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        let len = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Ok(len)
    }

    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        self.ensure_parent_dirs(path).await?;
        let url = self.full_url(path);
        let response = self
            .client
            .put(&url)
            .basic_auth(&self.username, Some(&self.password))
            .body(data.to_vec())
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!("WebDAV PUT failed: {}", response.status());
        }
        Ok(())
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let url = self.full_url(path);
        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("WebDAV DELETE failed: {}", status);
        }
        Ok(())
    }

    fn storage_type(&self) -> &str {
        "webdav"
    }
}

// ─── Factory ────────────────────────────────────

pub fn create_storage(config: &StorageConfig, cache_dir: &Path) -> Result<Box<dyn StorageBackend>> {
    match config {
        StorageConfig::Local { path } => {
            tracing::info!("Using local storage at: {}", path);
            Ok(Box::new(LocalStorage::new(path)))
        }
        StorageConfig::S3 {
            bucket,
            region,
            endpoint,
            access_key,
            secret_key,
            prefix,
        } => {
            tracing::info!("Using S3 storage: bucket={}, region={}", bucket, region);
            Ok(Box::new(S3Storage::new(
                bucket,
                region,
                endpoint.as_deref(),
                access_key,
                secret_key,
                prefix.as_deref(),
                cache_dir.join("s3_cache"),
            )?))
        }
        StorageConfig::WebDav {
            url,
            username,
            password,
            prefix,
        } => {
            tracing::info!("Using WebDAV storage: {}", url);
            Ok(Box::new(WebDavStorage::new(
                url,
                username,
                password,
                prefix.as_deref(),
                cache_dir.join("webdav_cache"),
            )?))
        }
    }
}
