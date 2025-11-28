// src/services/aws.rs
use crate::services::settings::{SettingsError, SettingsService};
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sesv2::Client as SesClient;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

#[derive(Debug, Error)]
pub enum AWSError {
    #[error("AWS credentials not configured")]
    NotConfigured,

    #[error("S3 operation failed: {0}")]
    S3Error(String),

    #[error("SES operation failed: {0}")]
    SESError(String),

    #[error("Settings error: {0}")]
    SettingsError(#[from] SettingsError),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AWSConfig {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
    pub s3_bucket_name: String,
    pub cloudfront_domain: Option<String>,
    pub ses_from_email: String,
    pub ses_region: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct S3Object {
    pub key: String,
    pub size: i64,
    pub last_modified: Option<DateTime<Utc>>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
    pub service: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct EmailAttachment {
    pub filename: String,
    pub content: Vec<u8>,
    pub content_type: String,
}

#[derive(Debug)]
pub struct AWSService {
    settings_service: Arc<SettingsService>,
}

impl AWSService {
    pub fn new(settings_service: Arc<SettingsService>) -> Self {
        Self { settings_service }
    }

    /// Get AWS configuration from settings
    pub async fn get_config(&self) -> Result<AWSConfig, AWSError> {
        let keys = [
            "aws_access_key_id",
            "aws_secret_access_key",
            "aws_region",
            "aws_s3_bucket_name",
            "aws_cloudfront_domain",
            "aws_ses_from_email",
            "aws_ses_region",
        ];

        let settings = self.settings_service.get_settings(&keys).await?;

        let access_key_id = settings
            .get("aws_access_key_id")
            .and_then(|v| v.clone())
            .ok_or(AWSError::NotConfigured)?;

        let secret_access_key = settings
            .get("aws_secret_access_key")
            .and_then(|v| v.clone())
            .ok_or(AWSError::NotConfigured)?;

        let region = settings
            .get("aws_region")
            .and_then(|v| v.clone())
            .unwrap_or_else(|| "us-east-1".to_string());

        let s3_bucket_name = settings
            .get("aws_s3_bucket_name")
            .and_then(|v| v.clone())
            .unwrap_or_else(|| "".to_string());

        let cloudfront_domain = settings
            .get("aws_cloudfront_domain")
            .and_then(|v| v.clone());

        let ses_from_email = settings
            .get("aws_ses_from_email")
            .and_then(|v| v.clone())
            .unwrap_or_else(|| "".to_string());

        let ses_region = settings
            .get("aws_ses_region")
            .and_then(|v| v.clone())
            .unwrap_or_else(|| region.clone());

        Ok(AWSConfig {
            access_key_id,
            secret_access_key,
            region,
            s3_bucket_name,
            cloudfront_domain,
            ses_from_email,
            ses_region,
        })
    }

    /// Initialize S3 client with credentials from settings
    async fn get_s3_client(&self) -> Result<(S3Client, String), AWSError> {
        let config = self.get_config().await?;

        if config.s3_bucket_name.is_empty() {
            return Err(AWSError::InvalidConfig(
                "S3 bucket name not configured".to_string(),
            ));
        }

        let credentials = Credentials::new(
            &config.access_key_id,
            &config.secret_access_key,
            None,
            None,
            "settings",
        );

        let region = Region::new(config.region.clone());

        let aws_config = aws_config::defaults(BehaviorVersion::latest())
            .region(region)
            .credentials_provider(credentials)
            .load()
            .await;

        let client = S3Client::new(&aws_config);

        Ok((client, config.s3_bucket_name))
    }

    /// Upload a file to S3
    pub async fn upload_file(
        &self,
        file_data: Vec<u8>,
        file_name: &str,
        content_type: &str,
    ) -> Result<String, AWSError> {
        let (client, bucket) = self.get_s3_client().await?;

        let body = ByteStream::from(Bytes::from(file_data));

        client
            .put_object()
            .bucket(&bucket)
            .key(file_name)
            .body(body)
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, key = %file_name, "Failed to upload file to S3");
                AWSError::S3Error(format!("Upload failed: {}", e))
            })?;

        let url = self.get_file_url(file_name, false).await?;

        info!(key = %file_name, bucket = %bucket, "File uploaded to S3 successfully");
        Ok(url)
    }

    /// List files in S3 bucket with optional prefix filtering
    pub async fn list_files(&self, prefix: Option<&str>) -> Result<Vec<S3Object>, AWSError> {
        let (client, bucket) = self.get_s3_client().await?;

        info!(bucket = %bucket, prefix = ?prefix, "Listing S3 objects");

        let mut request = client.list_objects_v2().bucket(&bucket);

        if let Some(p) = prefix {
            request = request.prefix(p);
        }

        let response = request.send().await.map_err(|e| {
            // Extract more detailed error information
            let error_details = format!("{:?}", e);
            error!(
                error = %e,
                error_details = %error_details,
                bucket = %bucket,
                "Failed to list S3 objects"
            );
            AWSError::S3Error(format!("List failed for bucket '{}': {}", bucket, e))
        })?;

        let objects: Vec<S3Object> = response
            .contents()
            .iter()
            .map(|obj| S3Object {
                key: obj.key().unwrap_or("").to_string(),
                size: obj.size().unwrap_or(0),
                last_modified: obj.last_modified().map(|dt| {
                    DateTime::from_timestamp(dt.secs(), dt.subsec_nanos()).unwrap_or_else(Utc::now)
                }),
                content_type: None,
            })
            .collect();

        debug!(count = objects.len(), prefix = ?prefix, "Listed S3 objects");
        Ok(objects)
    }

    /// Delete a single file from S3
    pub async fn delete_file(&self, key: &str) -> Result<(), AWSError> {
        let (client, bucket) = self.get_s3_client().await?;

        client
            .delete_object()
            .bucket(&bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, key = %key, "Failed to delete S3 object");
                AWSError::S3Error(format!("Delete failed: {}", e))
            })?;

        info!(key = %key, "File deleted from S3 successfully");
        Ok(())
    }

    /// Delete multiple files from S3
    pub async fn delete_files(&self, keys: Vec<String>) -> Result<(), AWSError> {
        for key in keys {
            self.delete_file(&key).await?;
        }
        Ok(())
    }

    /// Get file URL (with optional CloudFront support)
    pub async fn get_file_url(&self, key: &str, use_cloudfront: bool) -> Result<String, AWSError> {
        let config = self.get_config().await?;

        if use_cloudfront {
            if let Some(cloudfront_domain) = &config.cloudfront_domain {
                return Ok(format!("https://{}/{}", cloudfront_domain, key));
            }
        }

        // Standard S3 URL
        let url = format!(
            "https://{}.s3.{}.amazonaws.com/{}",
            config.s3_bucket_name, config.region, key
        );

        Ok(url)
    }

    /// Test S3 connection
    pub async fn test_s3_connection(&self) -> Result<TestResult, AWSError> {
        match self.get_s3_client().await {
            Ok((client, bucket)) => {
                // Try to list objects (with max 1 result) to verify access
                match client
                    .list_objects_v2()
                    .bucket(&bucket)
                    .max_keys(1)
                    .send()
                    .await
                {
                    Ok(_) => {
                        info!("S3 connection test successful");
                        Ok(TestResult {
                            success: true,
                            message: format!("Successfully connected to S3 bucket: {}", bucket),
                            service: "aws_s3".to_string(),
                        })
                    }
                    Err(e) => {
                        warn!(error = %e, "S3 connection test failed");
                        Ok(TestResult {
                            success: false,
                            message: format!("Failed to access S3 bucket: {}", e),
                            service: "aws_s3".to_string(),
                        })
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "S3 client initialization failed");
                Ok(TestResult {
                    success: false,
                    message: format!("Failed to initialize S3 client: {}", e),
                    service: "aws_s3".to_string(),
                })
            }
        }
    }

    /// Initialize SES client with credentials from settings
    async fn get_ses_client(&self) -> Result<SesClient, AWSError> {
        let config = self.get_config().await?;

        if config.ses_from_email.is_empty() {
            return Err(AWSError::InvalidConfig(
                "SES from email not configured".to_string(),
            ));
        }

        let credentials = Credentials::new(
            &config.access_key_id,
            &config.secret_access_key,
            None,
            None,
            "settings",
        );

        let region = Region::new(config.ses_region.clone());

        let aws_config = aws_config::defaults(BehaviorVersion::latest())
            .region(region)
            .credentials_provider(credentials)
            .load()
            .await;

        let client = SesClient::new(&aws_config);

        Ok(client)
    }

    /// Send email via SES
    pub async fn send_email(
        &self,
        to: Vec<String>,
        subject: &str,
        body: &str,
        attachments: Option<Vec<EmailAttachment>>,
    ) -> Result<(), AWSError> {
        let client = self.get_ses_client().await?;
        let config = self.get_config().await?;

        use aws_sdk_sesv2::types::{Body as SesBody, Content, Destination, EmailContent, Message};

        // Build destination
        let destination = Destination::builder()
            .set_to_addresses(Some(to.clone()))
            .build();

        // Build message content
        let subject_content = Content::builder()
            .data(subject)
            .charset("UTF-8")
            .build()
            .map_err(|e| AWSError::SESError(format!("Failed to build subject: {}", e)))?;

        let body_content = Content::builder()
            .data(body)
            .charset("UTF-8")
            .build()
            .map_err(|e| AWSError::SESError(format!("Failed to build body: {}", e)))?;

        let ses_body = SesBody::builder().html(body_content).build();

        let message = Message::builder()
            .subject(subject_content)
            .body(ses_body)
            .build();

        let email_content = EmailContent::builder().simple(message).build();

        // Send email
        let result = client
            .send_email()
            .from_email_address(&config.ses_from_email)
            .destination(destination)
            .content(email_content)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, to = ?to, "Failed to send email via SES");
                AWSError::SESError(format!("Send failed: {}", e))
            })?;

        info!(
            to = ?to,
            message_id = ?result.message_id(),
            "Email sent successfully via SES"
        );

        // Note: Attachments with SESv2 require using raw email format
        // For now, we'll log a warning if attachments are provided
        if let Some(attachments) = attachments {
            if !attachments.is_empty() {
                warn!(
                    count = attachments.len(),
                    "Email attachments provided but not yet implemented with SESv2 simple format"
                );
            }
        }

        Ok(())
    }

    /// Test SES connection
    pub async fn test_ses_connection(&self) -> Result<TestResult, AWSError> {
        match self.get_ses_client().await {
            Ok(client) => {
                // Try to get account details to verify access
                match client.get_account().send().await {
                    Ok(_) => {
                        info!("SES connection test successful");
                        Ok(TestResult {
                            success: true,
                            message: "Successfully connected to AWS SES".to_string(),
                            service: "aws_ses".to_string(),
                        })
                    }
                    Err(e) => {
                        warn!(error = %e, "SES connection test failed");
                        Ok(TestResult {
                            success: false,
                            message: format!("Failed to access SES: {}", e),
                            service: "aws_ses".to_string(),
                        })
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "SES client initialization failed");
                Ok(TestResult {
                    success: false,
                    message: format!("Failed to initialize SES client: {}", e),
                    service: "aws_ses".to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::SettingsService;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE system_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                encrypted INTEGER DEFAULT 0,
                description TEXT,
                updated_at TEXT DEFAULT (datetime('now')),
                updated_by TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_get_config_not_configured() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool));
        let aws_service = AWSService::new(settings_service);

        let result = aws_service.get_config().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AWSError::NotConfigured));
    }

    #[tokio::test]
    async fn test_get_config_with_settings() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool));

        // Set AWS credentials
        settings_service
            .set_setting("aws_access_key_id", "test_key_id", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("aws_secret_access_key", "test_secret", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("aws_region", "us-west-2", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("aws_s3_bucket_name", "test-bucket", false, Some("admin"))
            .await
            .unwrap();

        let aws_service = AWSService::new(settings_service);
        let config = aws_service.get_config().await.unwrap();

        assert_eq!(config.access_key_id, "test_key_id");
        assert_eq!(config.secret_access_key, "test_secret");
        assert_eq!(config.region, "us-west-2");
        assert_eq!(config.s3_bucket_name, "test-bucket");
    }

    #[tokio::test]
    async fn test_get_file_url_standard() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool));

        settings_service
            .set_setting("aws_access_key_id", "test_key", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("aws_secret_access_key", "test_secret", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("aws_region", "us-east-1", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("aws_s3_bucket_name", "my-bucket", false, Some("admin"))
            .await
            .unwrap();

        let aws_service = AWSService::new(settings_service);
        let url = aws_service
            .get_file_url("test/file.pdf", false)
            .await
            .unwrap();

        assert_eq!(
            url,
            "https://my-bucket.s3.us-east-1.amazonaws.com/test/file.pdf"
        );
    }

    #[tokio::test]
    async fn test_get_file_url_cloudfront() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool));

        settings_service
            .set_setting("aws_access_key_id", "test_key", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("aws_secret_access_key", "test_secret", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("aws_s3_bucket_name", "my-bucket", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting(
                "aws_cloudfront_domain",
                "d123456.cloudfront.net",
                false,
                Some("admin"),
            )
            .await
            .unwrap();

        let aws_service = AWSService::new(settings_service);
        let url = aws_service
            .get_file_url("test/file.pdf", true)
            .await
            .unwrap();

        assert_eq!(url, "https://d123456.cloudfront.net/test/file.pdf");
    }
}
