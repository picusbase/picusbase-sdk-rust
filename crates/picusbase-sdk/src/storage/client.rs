use crate::PicusClient;
use picusbase_errors::{ErrorCategory, ErrorLevel, PicusError};
use serde_json::json;

pub struct StorageClient<'a> {
    pub(crate) client: &'a PicusClient,
    pub(crate) project_ref: Option<&'a str>,
}

impl<'a> StorageClient<'a> {
    pub fn new(client: &'a PicusClient) -> Self {
        Self {
            client,
            project_ref: None,
        }
    }

    pub fn with_project(mut self, project_ref: &'a str) -> Self {
        self.project_ref = Some(project_ref);
        self
    }

    pub fn from(&self, bucket_id: &str) -> StorageFileApi<'a> {
        StorageFileApi {
            client: self.client,
            project_ref: self.project_ref,
            bucket_id: bucket_id.to_string(),
        }
    }

    pub async fn create_bucket(
        &self,
        name: &str,
        public: bool,
        max_size: Option<u64>,
        allowed_mimes: Option<Vec<String>>,
    ) -> Result<(), PicusError> {
        let body = json!({
            "name": name,
            "public": public,
            "max_size": max_size,
            "allowed_mimes": allowed_mimes
        });
        self.client
            .post_storage("/api/v1/storage/bucket", &body, self.project_ref)
            .await?;
        Ok(())
    }

    pub async fn update_bucket_config(
        &self,
        bucket: &str,
        max_size: Option<u64>,
    ) -> Result<(), PicusError> {
        let body = json!({
            "bucket": bucket,
            "max_size": max_size
        });
        self.client
            .post_storage("/api/v1/storage/bucket/config", &body, self.project_ref)
            .await?;
        Ok(())
    }

    pub async fn get_bucket_config(&self, bucket: &str) -> Result<Option<u64>, PicusError> {
        let body = json!({
            "bucket": bucket
        });
        let res = self
            .client
            .post_storage("/api/v1/storage/bucket/get-config", &body, self.project_ref)
            .await?;

        if let Some(max_size) = res.get("max_size") {
            if max_size.is_null() {
                return Ok(None);
            }
            return Ok(max_size.as_u64());
        }

        Ok(None)
    }

    pub async fn list_buckets(&self) -> Result<Vec<super::BucketInfo>, PicusError> {
        let res = self
            .client
            .post_storage("/api/v1/storage/buckets", &json!({}), self.project_ref)
            .await?;

        let buckets_value = res.get("buckets").ok_or_else(|| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                "Missing buckets field".to_string(),
            )
        })?;

        // Try to parse as Vec<BucketInfo>
        if let Ok(buckets) = serde_json::from_value::<Vec<super::BucketInfo>>(buckets_value.clone())
        {
            return Ok(buckets);
        }

        // Fallback: Try to parse as Vec<String> (legacy format)
        if let Ok(names) = serde_json::from_value::<Vec<String>>(buckets_value.clone()) {
            return Ok(names
                .into_iter()
                .map(|name| super::BucketInfo {
                    name,
                    created_at: None,
                    max_size: None,
                })
                .collect());
        }

        Err(PicusError::new(
            "SDK",
            ErrorCategory::DATA,
            ErrorLevel::Error,
            0,
            format!("Deserialization failed - Value: {:?}", buckets_value),
        ))
    }
}

pub struct StorageFileApi<'a> {
    client: &'a PicusClient,
    project_ref: Option<&'a str>,
    bucket_id: String,
}

impl<'a> StorageFileApi<'a> {
    /// Upload a file to storage (WASM)
    #[cfg(target_arch = "wasm32")]
    pub async fn upload(&self, path: &str, file: web_sys::File) -> Result<String, PicusError> {
        // Use Proxy Upload via Storage Service to avoid CORS issues
        let form_data = web_sys::FormData::new().map_err(|e| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("Failed to create FormData: {:?}", e),
            )
        })?;

        form_data
            .append_with_str("bucket", &self.bucket_id)
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::DATA,
                    ErrorLevel::Error,
                    0,
                    format!("{:?}", e),
                )
            })?;
        form_data.append_with_str("path", path).map_err(|e| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("{:?}", e),
            )
        })?;
        form_data.append_with_blob("file", &file).map_err(|e| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("{:?}", e),
            )
        })?;

        self.client
            .upload_storage("/api/v1/storage/upload", form_data, self.project_ref)
            .await?;

        Ok(path.to_string())
    }

    /// Upload a file to storage (Native)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn upload(
        &self,
        path: &str,
        file: Vec<u8>,
        mime: &str,
    ) -> Result<String, PicusError> {
        let part = reqwest::multipart::Part::bytes(file)
            .file_name(path.to_string())
            .mime_str(mime)
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::DATA,
                    ErrorLevel::Error,
                    0,
                    e.to_string(),
                )
            })?;

        let form = reqwest::multipart::Form::new()
            .text("bucket", self.bucket_id.clone())
            .text("path", path.to_string())
            .part("file", part);

        self.client
            .post_storage_multipart("/api/v1/storage/upload", form, self.project_ref)
            .await?;
        Ok(path.to_string())
    }

    pub async fn create_folder(&self, path: &str) -> Result<(), PicusError> {
        let body = json!({
            "bucket": self.bucket_id,
            "path": path
        });
        self.client
            .post_storage("/api/v1/storage/folder", &body, self.project_ref)
            .await?;
        Ok(())
    }

    pub async fn move_(&self, from_path: &str, to_path: &str) -> Result<(), PicusError> {
        let body = json!({
            "bucket": self.bucket_id,
            "from_path": from_path,
            "to_path": to_path
        });

        self.client
            .post_storage("/api/v1/storage/move", &body, self.project_ref)
            .await?;
        Ok(())
    }

    pub async fn copy(&self, from_path: &str, to_path: &str) -> Result<(), PicusError> {
        let body = json!({
            "bucket": self.bucket_id,
            "from_path": from_path,
            "to_path": to_path
        });

        self.client
            .post_storage("/api/v1/storage/copy", &body, self.project_ref)
            .await?;
        Ok(())
    }

    pub async fn remove(&self, paths: Vec<&str>) -> Result<(), PicusError> {
        let body = json!({
            "bucket": self.bucket_id,
            "paths": paths
        });

        self.client
            .post_storage("/api/v1/storage/delete", &body, self.project_ref)
            .await?;
        Ok(())
    }

    pub fn get_public_url(&self, path: &str) -> String {
        format!("https://{}.s3.picus.local/{}", self.bucket_id, path)
    }

    pub async fn create_signed_url(
        &self,
        path: &str,
        expires_in: i32,
    ) -> Result<String, PicusError> {
        let body = json!({
            "bucket": self.bucket_id,
            "path": path,
            "expires_in": expires_in
        });

        let res = self
            .client
            .post_storage("/api/v1/storage/download-url", &body, self.project_ref)
            .await?;
        let url = res.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                "No download URL returned".to_string(),
            )
        })?;

        Ok(url.to_string())
    }

    /// Download a file's bytes through the storage proxy (avoids CORS/internal hostname issues)
    pub async fn download(&self, path: &str) -> Result<Vec<u8>, PicusError> {
        let storage_url = self.client.storage_url.as_deref().ok_or_else(|| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                "Storage URL not configured".to_string(),
            )
        })?;

        let url = format!(
            "{}/api/v1/storage/download",
            storage_url.trim_end_matches('/')
        );

        let http = reqwest::Client::new();
        let mut req = http.post(&url).json(&json!({
            "bucket": self.bucket_id,
            "path": path
        }));

        if let Some(token) = self.client.token() {
            req = req.bearer_auth(token);
        }
        if let Some(pr) = self.project_ref {
            req = req.header("x-project-ref", pr);
        }

        let resp = req.send().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Download request failed: {}", e),
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "STORAGE",
                ErrorCategory::NET,
                ErrorLevel::Error,
                status.as_u16(),
                format!("Download failed: {} - {}", status, text),
            ));
        }

        let bytes = resp.bytes().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Failed to read download response: {}", e),
            )
        })?;

        Ok(bytes.to_vec())
    }

    pub async fn list(&self, prefix: Option<String>) -> Result<Vec<String>, PicusError> {
        let body = json!({
            "bucket": self.bucket_id,
            "prefix": prefix
        });

        let res = self
            .client
            .post_storage("/api/v1/storage/list", &body, self.project_ref)
            .await?;
        let keys = res
            .get("keys")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::DATA,
                    ErrorLevel::Error,
                    0,
                    "No keys returned".to_string(),
                )
            })?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        Ok(keys)
    }
}
