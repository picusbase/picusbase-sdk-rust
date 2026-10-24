use aws_config::BehaviorVersion;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CorsConfiguration, CorsRule};
use aws_sdk_s3::{
    config::{Credentials, Region},
    Client,
};
use picusbase_errors::{ErrorCategory, ErrorLevel, PicusError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize)]
struct BucketConfig {
    max_size: Option<u64>,
}

#[derive(Clone)]
pub struct StorageManager {
    client: Client,
}

impl StorageManager {
    pub async fn new(
        endpoint: String,
        region: String,
        access_key: String,
        secret_key: String,
        _public_url: String,
    ) -> Self {
        let credentials = Credentials::new(access_key, secret_key, None, None, "rustfs");

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .credentials_provider(credentials)
            .region(Region::new(region))
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();

        let client = Client::from_conf(s3_config);

        Self { client }
    }

    pub async fn list_buckets(&self) -> Result<Vec<super::BucketInfo>, PicusError> {
        let resp = self.client.list_buckets().send().await.map_err(|e| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Failed to list buckets: {}", e),
            )
        })?;

        let mut buckets = Vec::new();

        for b in resp.buckets() {
            if let Some(name) = b.name() {
                let name = name.to_string();
                let created_at = b.creation_date().map(|d| d.to_string());

                // Fetch max size
                let max_size = self.get_bucket_max_size(&name).await.unwrap_or(None);

                buckets.push(super::BucketInfo {
                    name,
                    created_at,
                    max_size,
                });
            }
        }

        Ok(buckets)
    }

    pub async fn create_tenant_bucket(
        &self,
        bucket: &str,
        public: bool,
        max_size: Option<u64>,
        _allowed_mimes: Option<Vec<String>>,
    ) -> Result<(), PicusError> {
        // Check if bucket exists
        let exists = match self.client.head_bucket().bucket(bucket).send().await {
            Ok(_) => true,
            Err(_) => false,
        };

        if !exists {
            self.client
                .create_bucket()
                .bucket(bucket)
                .send()
                .await
                .map_err(|e| {
                    println!("DEBUG: Failed to create bucket '{}': {:?}", bucket, e);
                    PicusError::new(
                        "STORAGE",
                        ErrorCategory::NET,
                        ErrorLevel::Error,
                        0,
                        format!("Failed to create bucket: {:?}", e),
                    )
                })?;
        }

        // Set CORS configuration
        let cors_rule = CorsRule::builder()
            .allowed_origins("*")
            .allowed_methods("GET")
            .allowed_methods("PUT")
            .allowed_methods("POST")
            .allowed_methods("DELETE")
            .allowed_methods("HEAD")
            .allowed_headers("*")
            .expose_headers("ETag")
            .max_age_seconds(3000)
            .build(); // Result

        let cors_rule = match cors_rule {
            Ok(r) => r,
            Err(e) => {
                return Err(PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to build CORS rule: {}", e),
                ))
            }
        };

        let cors_config = CorsConfiguration::builder().cors_rules(cors_rule).build();

        let cors_config = match cors_config {
            Ok(c) => c,
            Err(e) => {
                return Err(PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to build CORS config: {}", e),
                ))
            }
        };

        self.client
            .put_bucket_cors()
            .bucket(bucket)
            .cors_configuration(cors_config)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to set bucket CORS: {}", e),
                )
            })?;

        if let Some(size) = max_size {
            self.set_bucket_max_size(bucket, Some(size)).await?;
        }

        if public {
            let policy = format!(
                r#"{{
                "Version": "2012-10-17",
                "Statement": [
                    {{
                        "Sid": "PublicReadGetObject",
                        "Effect": "Allow",
                        "Principal": "*",
                        "Action": "s3:GetObject",
                        "Resource": "arn:aws:s3:::{}/*"
                    }}
                ]
            }}"#,
                bucket
            );

            // Try ACL first (best effort — RustFS returns NotImplemented for canned
            // ACLs and is ignored below; the bucket policy call is what actually
            // grants public read on RustFS)
            let _ = self
                .client
                .put_bucket_acl()
                .bucket(bucket)
                .acl(aws_sdk_s3::types::BucketCannedAcl::PublicRead)
                .send()
                .await;

            // Try Policy (ignore error if unsupported)
            let _ = self
                .client
                .put_bucket_policy()
                .bucket(bucket)
                .policy(policy)
                .send()
                .await;
        }

        Ok(())
    }

    pub async fn get_bucket_cors(&self, bucket: &str) -> Result<Vec<String>, PicusError> {
        let res = self
            .client
            .get_bucket_cors()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to get bucket CORS: {}", e),
                )
            })?;

        let mut rules = Vec::new();
        for rule in res.cors_rules() {
            rules.push(format!("{:?}", rule));
        }
        Ok(rules)
    }

    pub async fn generate_upload_url(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        expires_in_secs: u64,
    ) -> Result<String, PicusError> {
        let expires_in = Duration::from_secs(expires_in_secs);
        let presigning_config = PresigningConfig::expires_in(expires_in).map_err(|e| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Invalid presigning config: {}", e),
            )
        })?;

        let presigned_req = self
            .client
            .put_object()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .presigned(presigning_config)
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to presign upload url: {}", e),
                )
            })?;

        Ok(presigned_req.uri().to_string())
    }

    pub async fn upload_object(
        &self,
        bucket: &str,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(), PicusError> {
        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(aws_sdk_s3::primitives::ByteStream::from(data))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to upload object: {}", e),
                )
            })?;
        Ok(())
    }

    pub async fn create_folder(&self, bucket: &str, path: &str) -> Result<(), PicusError> {
        let key = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{}/", path)
        };

        self.upload_object(bucket, &key, vec![], "application/x-directory")
            .await
    }

    pub async fn generate_download_url(
        &self,
        bucket: &str,
        key: &str,
        expires_in_secs: i32,
    ) -> Result<String, PicusError> {
        let expires_in = Duration::from_secs(expires_in_secs as u64);
        let presigning_config = PresigningConfig::expires_in(expires_in).map_err(|e| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Invalid presigning config: {}", e),
            )
        })?;

        let presigned_req = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to presign download url: {}", e),
                )
            })?;

        Ok(presigned_req.uri().to_string())
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), PicusError> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to delete object: {}", e),
                )
            })?;

        Ok(())
    }

    /// Download an object's bytes from S3 (server-side proxy for browser clients)
    pub async fn download_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<(Vec<u8>, String), PicusError> {
        let resp = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to get object: {}", e),
                )
            })?;

        let content_type = resp
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to read object body: {}", e),
                )
            })?
            .into_bytes()
            .to_vec();

        Ok((bytes, content_type))
    }

    pub async fn copy_object(
        &self,
        source_bucket: &str,
        source_key: &str,
        dest_bucket: &str,
        dest_key: &str,
    ) -> Result<(), PicusError> {
        let copy_source = format!("{}/{}", source_bucket, source_key); // Format: bucket/key

        self.client
            .copy_object()
            .bucket(dest_bucket)
            .copy_source(copy_source)
            .key(dest_key)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to copy object: {}", e),
                )
            })?;

        Ok(())
    }

    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<String>,
    ) -> Result<Vec<String>, PicusError> {
        let mut req = self.client.list_objects_v2().bucket(bucket);
        if let Some(p) = prefix {
            req = req.prefix(p);
        }

        let resp = req.send().await.map_err(|e| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Failed to list objects: {}", e),
            )
        })?;

        let keys = resp
            .contents()
            .iter()
            .filter_map(|obj| obj.key().map(|k| k.to_string()))
            .collect();

        Ok(keys)
    }

    pub async fn get_bucket_max_size(&self, bucket: &str) -> Result<Option<u64>, PicusError> {
        let resp = self
            .client
            .get_object()
            .bucket(bucket)
            .key(".picus-config")
            .send()
            .await;

        match resp {
            Ok(output) => {
                let bytes = output
                    .body
                    .collect()
                    .await
                    .map_err(|e| {
                        PicusError::new(
                            "STORAGE",
                            ErrorCategory::NET,
                            ErrorLevel::Error,
                            0,
                            e.to_string(),
                        )
                    })?
                    .into_bytes();
                let config: BucketConfig =
                    serde_json::from_slice(&bytes).unwrap_or(BucketConfig { max_size: None });
                Ok(config.max_size)
            }
            Err(_) => Ok(None),
        }
    }

    pub async fn set_bucket_max_size(
        &self,
        bucket: &str,
        max_size: Option<u64>,
    ) -> Result<(), PicusError> {
        let config = BucketConfig { max_size };
        let body = serde_json::to_vec(&config).map_err(|e| {
            PicusError::new(
                "STORAGE",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                e.to_string(),
            )
        })?;

        self.client
            .put_object()
            .bucket(bucket)
            .key(".picus-config")
            .body(ByteStream::from(body))
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "STORAGE",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to set bucket config: {:?}", e),
                )
            })?;

        Ok(())
    }
}
