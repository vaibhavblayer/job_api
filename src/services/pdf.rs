// PDF Generation Service for Offer Letters
use anyhow::Result;
use chrono::Utc;
use printpdf::*;
use sqlx::SqlitePool;
use std::fs::File;
use std::io::BufWriter;
use std::sync::Arc;

use crate::common::generate_raw_id;
use crate::services::aws::AWSService;
use crate::services::settings::SettingsService;

/// Offer letter data structure
#[derive(Debug, Clone)]
pub struct OfferLetterData {
    pub candidate_name: String,
    pub job_title: String,
    pub salary: f64,
    pub start_date: String,
    pub benefits: String,
    pub additional_terms: String,
    pub company_name: String,
    pub content: String,
}

/// PDF generation service
#[derive(Debug, Clone)]
pub struct PDFService {
    pool: SqlitePool,
    settings_service: Arc<SettingsService>,
    aws_service: Arc<AWSService>,
}

impl PDFService {
    pub fn new(
        pool: SqlitePool,
        settings_service: Arc<SettingsService>,
        aws_service: Arc<AWSService>,
    ) -> Self {
        Self {
            pool,
            settings_service,
            aws_service,
        }
    }

    /// Generate offer letter PDF
    pub async fn generate_offer_letter_pdf(
        &self,
        data: OfferLetterData,
        logo_path: Option<&str>,
        signature_path: Option<&str>,
    ) -> Result<String> {
        // Generate PDF synchronously (not across await points)
        let temp_path = self.generate_pdf_sync(&data, logo_path, signature_path)?;

        // Upload to storage (async)
        let pdf_url = self.upload_pdf(&temp_path).await?;

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        Ok(pdf_url)
    }

    /// Generate PDF synchronously (to avoid Send issues with PdfDocument)
    fn generate_pdf_sync(
        &self,
        data: &OfferLetterData,
        _logo_path: Option<&str>,
        _signature_path: Option<&str>,
    ) -> Result<String> {
        // Create PDF document
        let (doc, page1, layer1) = PdfDocument::new(
            "Offer Letter",
            Mm(210.0), // A4 width
            Mm(297.0), // A4 height
            "Layer 1",
        );

        let current_layer = doc.get_page(page1).get_layer(layer1);

        // Define fonts
        let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;
        let font_regular = doc.add_builtin_font(BuiltinFont::Helvetica)?;

        // Define margins and positions
        let left_margin = Mm(20.0);
        let _right_margin = Mm(190.0);
        let top_margin = Mm(277.0);
        let mut current_y = top_margin;

        // Add logo if provided (placeholder for company name if logo path exists)
        if _logo_path.is_some() {
            current_layer.use_text(&data.company_name, 16.0, Mm(85.0), current_y, &font_bold);
            current_y -= Mm(15.0);
        }

        // Add title
        current_y -= Mm(20.0);
        current_layer.use_text("OFFER LETTER", 18.0, left_margin, current_y, &font_bold);

        // Add date
        current_y -= Mm(15.0);
        let date_str = Utc::now().format("%B %d, %Y").to_string();
        current_layer.use_text(&date_str, 11.0, left_margin, current_y, &font_regular);

        // Add candidate name
        current_y -= Mm(15.0);
        current_layer.use_text(
            &format!("Dear {},", data.candidate_name),
            11.0,
            left_margin,
            current_y,
            &font_regular,
        );

        // Add main content
        current_y -= Mm(10.0);
        let content_lines = self.wrap_text(&data.content, 85);
        for line in content_lines {
            current_y -= Mm(5.0);
            if current_y < Mm(30.0) {
                // Need new page
                break;
            }
            current_layer.use_text(&line, 11.0, left_margin, current_y, &font_regular);
        }

        // Add job details section
        current_y -= Mm(15.0);
        current_layer.use_text(
            "Position Details:",
            12.0,
            left_margin,
            current_y,
            &font_bold,
        );

        current_y -= Mm(8.0);
        current_layer.use_text(
            &format!("Position: {}", data.job_title),
            11.0,
            left_margin,
            current_y,
            &font_regular,
        );

        current_y -= Mm(6.0);
        current_layer.use_text(
            &format!("Salary: ${:.2} per year", data.salary),
            11.0,
            left_margin,
            current_y,
            &font_regular,
        );

        current_y -= Mm(6.0);
        current_layer.use_text(
            &format!("Start Date: {}", data.start_date),
            11.0,
            left_margin,
            current_y,
            &font_regular,
        );

        // Add benefits section
        if !data.benefits.is_empty() {
            current_y -= Mm(12.0);
            current_layer.use_text("Benefits:", 12.0, left_margin, current_y, &font_bold);

            current_y -= Mm(8.0);
            let benefits_lines = self.wrap_text(&data.benefits, 85);
            for line in benefits_lines {
                current_y -= Mm(5.0);
                if current_y < Mm(30.0) {
                    break;
                }
                current_layer.use_text(&line, 11.0, left_margin, current_y, &font_regular);
            }
        }

        // Add additional terms
        if !data.additional_terms.is_empty() {
            current_y -= Mm(12.0);
            current_layer.use_text(
                "Additional Terms:",
                12.0,
                left_margin,
                current_y,
                &font_bold,
            );

            current_y -= Mm(8.0);
            let terms_lines = self.wrap_text(&data.additional_terms, 85);
            for line in terms_lines {
                current_y -= Mm(5.0);
                if current_y < Mm(30.0) {
                    break;
                }
                current_layer.use_text(&line, 11.0, left_margin, current_y, &font_regular);
            }
        }

        // Add signature section (placeholder text if signature path exists)
        current_y -= Mm(20.0);
        if _signature_path.is_some() {
            current_layer.use_text("[Signature]", 10.0, left_margin, current_y, &font_regular);
            current_y -= Mm(8.0);
        }

        current_layer.use_text(
            &format!("{}", data.company_name),
            11.0,
            left_margin,
            current_y,
            &font_regular,
        );

        // Generate filename
        let filename = format!(
            "offer_letter_{}_{}.pdf",
            data.candidate_name.replace(" ", "_").to_lowercase(),
            Utc::now().timestamp()
        );

        // Save PDF to temporary file
        let temp_path = format!("/tmp/{}", filename);
        doc.save(&mut BufWriter::new(File::create(&temp_path)?))?;

        Ok(temp_path)
    }

    /// Upload PDF to storage (async)
    async fn upload_pdf(&self, temp_path: &str) -> Result<String> {
        let filename = std::path::Path::new(temp_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("offer_letter.pdf");

        // Upload to storage
        let storage_type = self
            .settings_service
            .get_setting("storage_type")
            .await?
            .unwrap_or_else(|| "local".to_string());

        let pdf_url = if storage_type.starts_with("s3") {
            // Upload to S3
            let file_data = std::fs::read(temp_path)?;
            let s3_key = format!("offer-letters/{}", filename);

            self.aws_service
                .upload_file(file_data, &s3_key, "application/pdf")
                .await?
        } else {
            // Store locally
            let local_dir = "uploads/offer-letters";
            std::fs::create_dir_all(local_dir)?;
            let local_path = format!("{}/{}", local_dir, filename);
            std::fs::copy(temp_path, &local_path)?;
            format!("/uploads/offer-letters/{}", filename)
        };

        Ok(pdf_url)
    }

    /// Wrap text to fit within specified character width
    fn wrap_text(&self, text: &str, max_chars: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current_line = String::new();

        for word in text.split_whitespace() {
            if current_line.len() + word.len() + 1 > max_chars {
                if !current_line.is_empty() {
                    lines.push(current_line.clone());
                    current_line.clear();
                }
            }

            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        lines
    }

    /// Store offer letter record in database
    pub async fn store_offer_letter_record(
        &self,
        candidate_id: &str,
        job_id: &str,
        data: &OfferLetterData,
        pdf_url: &str,
        logo_url: Option<&str>,
        signature_url: Option<&str>,
        created_by: &str,
    ) -> Result<String> {
        let id = generate_raw_id(8);

        sqlx::query(
            r#"
            INSERT INTO offer_letters (
                id, candidate_id, job_id, job_title, salary, start_date,
                benefits, additional_terms, content, pdf_url, logo_url,
                signature_url, created_by, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(candidate_id)
        .bind(job_id)
        .bind(&data.job_title)
        .bind(data.salary)
        .bind(&data.start_date)
        .bind(&data.benefits)
        .bind(&data.additional_terms)
        .bind(&data.content)
        .bind(pdf_url)
        .bind(logo_url)
        .bind(signature_url)
        .bind(created_by)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Get offer letter by ID
    pub async fn get_offer_letter(&self, id: &str) -> Result<Option<OfferLetterRecord>> {
        let record = sqlx::query_as::<_, OfferLetterRecord>(
            r#"
            SELECT id, candidate_id, job_id, job_title, salary, start_date,
                   benefits, additional_terms, content, pdf_url, logo_url,
                   signature_url, sent_at, created_by, created_at
            FROM offer_letters
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(record)
    }

    /// Mark offer letter as sent
    pub async fn mark_as_sent(&self, id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE offer_letters
            SET sent_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

/// Offer letter database record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OfferLetterRecord {
    pub id: String,
    pub candidate_id: String,
    pub job_id: String,
    pub job_title: String,
    pub salary: f64,
    pub start_date: String,
    pub benefits: String,
    pub additional_terms: String,
    pub content: String,
    pub pdf_url: String,
    pub logo_url: Option<String>,
    pub signature_url: Option<String>,
    pub sent_at: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create offer_letters table
        sqlx::query(
            r#"
            CREATE TABLE offer_letters (
                id TEXT PRIMARY KEY,
                candidate_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                job_title TEXT NOT NULL,
                salary REAL,
                start_date TEXT,
                benefits TEXT,
                additional_terms TEXT,
                content TEXT NOT NULL,
                pdf_url TEXT,
                logo_url TEXT,
                signature_url TEXT,
                sent_at TEXT,
                created_by TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_wrap_text() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool.clone()));
        let aws_service = Arc::new(AWSService::new(settings_service.clone()));
        let pdf_service = PDFService::new(pool, settings_service, aws_service);

        let text = "This is a long line of text that should be wrapped into multiple lines based on the maximum character width specified.";
        let lines = pdf_service.wrap_text(text, 30);

        assert!(lines.len() > 1);
        for line in &lines {
            assert!(line.len() <= 35); // Allow some flexibility
        }
    }

    #[tokio::test]
    async fn test_store_offer_letter_record() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool.clone()));
        let aws_service = Arc::new(AWSService::new(settings_service.clone()));
        let pdf_service = PDFService::new(pool, settings_service, aws_service);

        let data = OfferLetterData {
            candidate_name: "John Doe".to_string(),
            job_title: "Software Engineer".to_string(),
            salary: 100000.0,
            start_date: "2024-01-15".to_string(),
            benefits: "Health insurance, 401k".to_string(),
            additional_terms: "Remote work available".to_string(),
            company_name: "Tech Corp".to_string(),
            content: "We are pleased to offer you the position...".to_string(),
        };

        let id = pdf_service
            .store_offer_letter_record(
                "candidate-123",
                "job-456",
                &data,
                "/path/to/offer.pdf",
                Some("/path/to/logo.png"),
                Some("/path/to/signature.png"),
                "admin-789",
            )
            .await
            .unwrap();

        assert!(!id.is_empty());

        // Verify record was stored
        let record = pdf_service.get_offer_letter(&id).await.unwrap();
        assert!(record.is_some());

        let record = record.unwrap();
        assert_eq!(record.candidate_id, "candidate-123");
        assert_eq!(record.job_title, "Software Engineer");
        assert_eq!(record.salary, 100000.0);
    }

    #[tokio::test]
    async fn test_mark_as_sent() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool.clone()));
        let aws_service = Arc::new(AWSService::new(settings_service.clone()));
        let pdf_service = PDFService::new(pool, settings_service, aws_service);

        let data = OfferLetterData {
            candidate_name: "Jane Smith".to_string(),
            job_title: "Product Manager".to_string(),
            salary: 120000.0,
            start_date: "2024-02-01".to_string(),
            benefits: "Full benefits package".to_string(),
            additional_terms: "".to_string(),
            company_name: "Startup Inc".to_string(),
            content: "Congratulations on your offer...".to_string(),
        };

        let id = pdf_service
            .store_offer_letter_record(
                "candidate-456",
                "job-789",
                &data,
                "/path/to/offer2.pdf",
                None,
                None,
                "admin-123",
            )
            .await
            .unwrap();

        // Mark as sent
        pdf_service.mark_as_sent(&id).await.unwrap();

        // Verify sent_at is set
        let record = pdf_service.get_offer_letter(&id).await.unwrap().unwrap();
        assert!(record.sent_at.is_some());
    }
}
