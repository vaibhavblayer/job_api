// src/common/migrations.rs
//! Database migration and schema management

use sqlx::SqlitePool;
use std::env;
use tracing::{info, warn};

/// Run all database migrations
/// 
/// Per design requirements: "Drop and recreate tables as needed (no legacy preservation)"
/// This ensures clean schema without migration conflicts
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Only drop tables if RESET_DB environment variable is set to "true"
    // This prevents data loss on server restarts
    let should_reset_db = env::var("RESET_DB").unwrap_or_else(|_| "false".to_string()) == "true";

    if should_reset_db {
        warn!("⚠️  RESET_DB=true - Dropping all tables and recreating schema...");
        drop_all_tables(pool).await?;
        info!("✅ Dropped old tables");
    } else {
        info!("ℹ️  Skipping table drop (RESET_DB not set). Tables will be created if they don't exist.");
    }

    create_core_tables(pool).await?;
    create_company_tables(pool).await?;
    create_job_tables(pool).await?;
    create_application_tables(pool).await?;
    create_interview_tables(pool).await?;
    create_messaging_tables(pool).await?;
    create_system_tables(pool).await?;
    create_indexes(pool).await?;
    
    // Initialize default settings from environment variables
    init_default_settings(pool).await?;
    
    // Sync current_stage with status for existing applications
    sync_application_stages(pool).await?;

    info!("✅ Database migration completed successfully!");
    info!("📊 Created all tables with performance indexes");

    Ok(())
}

/// Initialize default system settings from environment variables
/// Only sets values if they don't already exist in the database
async fn init_default_settings(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // AWS settings to initialize from environment
    let aws_settings = vec![
        ("aws_access_key_id", "AWS_ACCESS_KEY_ID"),
        ("aws_secret_access_key", "AWS_SECRET_ACCESS_KEY"),
        ("aws_region", "AWS_REGION"),
        ("aws_s3_bucket_name", "AWS_S3_BUCKET_NAME"),
        ("aws_cloudfront_domain", "AWS_CLOUDFRONT_DOMAIN"),
        ("aws_ses_from_email", "AWS_SES_FROM_EMAIL"),
        ("aws_ses_region", "AWS_SES_REGION"),
    ];

    // Other settings to initialize
    let other_settings = vec![
        ("openai_api_key", "OPENAI_API_KEY"),
        ("openai_model", "OPENAI_MODEL"),
        ("timezone", "TIMEZONE"),
        ("storage_type", "STORAGE_TYPE"),
    ];

    let all_settings: Vec<_> = aws_settings.into_iter().chain(other_settings).collect();

    for (db_key, env_key) in all_settings {
        if let Ok(value) = env::var(env_key) {
            if !value.is_empty() {
                // Check if setting already exists
                let existing: Option<(String,)> = sqlx::query_as(
                    "SELECT value FROM system_settings WHERE key = ?"
                )
                .bind(db_key)
                .fetch_optional(pool)
                .await?;

                if existing.is_none() {
                    // Insert new setting
                    sqlx::query(
                        r#"
                        INSERT INTO system_settings (key, value, encrypted, updated_at, updated_by)
                        VALUES (?, ?, 0, datetime('now'), 'system')
                        "#
                    )
                    .bind(db_key)
                    .bind(&value)
                    .execute(pool)
                    .await?;
                    
                    info!(key = %db_key, "Initialized setting from environment variable");
                }
            }
        }
    }

    // Set default storage_type if not set
    let storage_type: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM system_settings WHERE key = 'storage_type'"
    )
    .fetch_optional(pool)
    .await?;

    if storage_type.is_none() {
        // Check if AWS is configured, use s3-cloudfront, otherwise local
        let aws_key: Option<(String,)> = sqlx::query_as(
            "SELECT value FROM system_settings WHERE key = 'aws_access_key_id'"
        )
        .fetch_optional(pool)
        .await?;

        let default_storage = if aws_key.is_some() { "s3-cloudfront" } else { "local" };
        
        sqlx::query(
            r#"
            INSERT INTO system_settings (key, value, encrypted, updated_at, updated_by)
            VALUES ('storage_type', ?, 0, datetime('now'), 'system')
            "#
        )
        .bind(default_storage)
        .execute(pool)
        .await?;
        
        info!(storage_type = %default_storage, "Set default storage type");
    }

    Ok(())
}

async fn drop_all_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Drop tables in reverse dependency order
    let tables = vec![
        "job_social_images",
        "offer_letters",
        "interview_interviewers",
        "interviews",
        "stage_history",
        "video_submissions",
        "videos",
        "application_status_history",
        "applications",
        "job_status_history",
        "job_views",
        "jobs",
        "job_templates",
        "company_assets",
        "companies",
        "message_attachments",
        "conversation_messages",
        "resume_assets",
        "events",
        "resumes",
        "testimonials",
        "education",
        "experiences",
        "profiles",
        "admin_users",
        "system_settings",
        "ai_usage_logs",
        "email_history",
        "users",
    ];

    for table in tables {
        let _ = sqlx::query(&format!("DROP TABLE IF EXISTS {}", table))
            .execute(pool)
            .await;
    }

    Ok(())
}

async fn create_core_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Users table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,
            name TEXT,
            avatar TEXT,
            avatar_filename TEXT,
            avatar_updated_at TEXT,
            provider TEXT,
            provider_id TEXT,
            created_at TEXT DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Profiles table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS profiles (
            user_id TEXT PRIMARY KEY,
            first_name TEXT,
            last_name TEXT,
            phone TEXT,
            location TEXT,
            bio TEXT,
            website TEXT,
            linkedin_url TEXT,
            github_url TEXT,
            skills TEXT,
            resume_status TEXT DEFAULT 'pending',
            last_resume_id TEXT,
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Experiences table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS experiences (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            company TEXT NOT NULL,
            title TEXT NOT NULL,
            start_date TEXT NOT NULL,
            end_date TEXT,
            description TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Education table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS education (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            institution TEXT NOT NULL,
            degree TEXT NOT NULL,
            field_of_study TEXT,
            start_date TEXT NOT NULL,
            end_date TEXT,
            description TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Testimonials table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS testimonials (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            content TEXT NOT NULL,
            rating INTEGER DEFAULT 5,
            position TEXT,
            company TEXT,
            featured BOOLEAN DEFAULT 0,
            approved BOOLEAN DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_company_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Companies table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS companies (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT,
            website TEXT,
            industry TEXT,
            company_size TEXT,
            founded_year INTEGER,
            headquarters TEXT,
            operating_locations TEXT,
            culture TEXT,
            benefits TEXT,
            default_logo_url TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Company assets table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS company_assets (
            id TEXT PRIMARY KEY,
            company_id TEXT NOT NULL,
            asset_type TEXT NOT NULL CHECK (asset_type IN ('logo', 'image')),
            url TEXT NOT NULL,
            filename TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            mime_type TEXT NOT NULL,
            is_default INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(company_id) REFERENCES companies(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_job_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Job templates table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS job_templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            company_id TEXT,
            template_type TEXT NOT NULL CHECK (template_type IN ('system', 'custom', 'ai')),
            job_data TEXT NOT NULL,
            ai_context TEXT,
            created_by TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(company_id) REFERENCES companies(id) ON DELETE CASCADE,
            FOREIGN KEY(created_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Add ai_context column to existing job_templates table if it doesn't exist
    let _ = sqlx::query("ALTER TABLE job_templates ADD COLUMN ai_context TEXT")
        .execute(pool)
        .await;

    // Migration: Update CHECK constraint to include 'ai' template type
    // SQLite doesn't support ALTER TABLE to modify constraints, so we need to recreate the table
    migrate_job_templates_check_constraint(pool).await?;

    // Jobs table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS jobs (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT,
            location TEXT,
            company TEXT,
            company_id TEXT,
            company_logo_url TEXT,
            job_image_url TEXT,
            salary_min INTEGER,
            salary_max INTEGER,
            job_type TEXT,
            experience_level TEXT,
            requirements TEXT,
            benefits TEXT,
            status TEXT DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'archived', 'closed')),
            is_featured INTEGER DEFAULT 0,
            educational_qualifications TEXT,
            template_id TEXT,
            draft_data TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            published_at TEXT,
            FOREIGN KEY(company_id) REFERENCES companies(id),
            FOREIGN KEY(template_id) REFERENCES job_templates(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Job status history table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS job_status_history (
            id TEXT PRIMARY KEY,
            job_id TEXT NOT NULL,
            old_status TEXT,
            new_status TEXT NOT NULL,
            changed_by TEXT NOT NULL,
            notes TEXT,
            changed_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE,
            FOREIGN KEY(changed_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Job views table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS job_views (
            id TEXT PRIMARY KEY,
            job_id TEXT NOT NULL,
            user_id TEXT,
            ip_address TEXT,
            user_agent TEXT,
            viewed_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(job_id) REFERENCES jobs(id),
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Job social images table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS job_social_images (
            id TEXT PRIMARY KEY,
            job_id TEXT NOT NULL,
            platform TEXT NOT NULL,
            image_url TEXT NOT NULL,
            prompt TEXT,
            style TEXT,
            created_by TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE,
            FOREIGN KEY(created_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Migration to update job_templates CHECK constraint to include 'ai' template type
/// SQLite doesn't support modifying CHECK constraints, so we recreate the table
async fn migrate_job_templates_check_constraint(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Check if the constraint already includes 'ai' by trying to insert a test row
    // If it fails with a constraint error, we need to migrate
    let needs_migration = sqlx::query(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='job_templates'"
    )
    .fetch_optional(pool)
    .await?
    .map(|row: sqlx::sqlite::SqliteRow| {
        use sqlx::Row;
        let sql: String = row.get("sql");
        // Check if the CHECK constraint already includes 'ai'
        !sql.contains("'ai'")
    })
    .unwrap_or(false);

    if !needs_migration {
        return Ok(());
    }

    tracing::info!("Migrating job_templates table to support 'ai' template type...");

    // Step 1: Create new table with updated CHECK constraint
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS job_templates_new (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            company_id TEXT,
            template_type TEXT NOT NULL CHECK (template_type IN ('system', 'custom', 'ai')),
            job_data TEXT NOT NULL,
            ai_context TEXT,
            created_by TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(company_id) REFERENCES companies(id) ON DELETE CASCADE,
            FOREIGN KEY(created_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Step 2: Copy data from old table to new table
    sqlx::query(
        r#"
        INSERT INTO job_templates_new (id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at)
        SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
        FROM job_templates
        "#,
    )
    .execute(pool)
    .await?;

    // Step 3: Drop old table
    sqlx::query("DROP TABLE job_templates")
        .execute(pool)
        .await?;

    // Step 4: Rename new table to original name
    sqlx::query("ALTER TABLE job_templates_new RENAME TO job_templates")
        .execute(pool)
        .await?;

    // Step 5: Recreate indexes
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_job_templates_type ON job_templates(template_type)")
        .execute(pool)
        .await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_job_templates_created_by ON job_templates(created_by)")
        .execute(pool)
        .await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_job_templates_company ON job_templates(company_id)")
        .execute(pool)
        .await;

    tracing::info!("Successfully migrated job_templates table to support 'ai' template type");

    Ok(())
}

async fn create_application_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Resumes table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS resumes (
            id TEXT PRIMARY KEY,
            user_id TEXT,
            filename TEXT,
            status TEXT DEFAULT 'submitted',
            score REAL,
            parsed_json TEXT,
            submitted_at TEXT DEFAULT (datetime('now')),
            deleted_at TEXT,
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Add deleted_at column to existing resumes table if it doesn't exist
    let _ = sqlx::query("ALTER TABLE resumes ADD COLUMN deleted_at TEXT")
        .execute(pool)
        .await;

    // Add label column to existing resumes table if it doesn't exist
    let _ = sqlx::query("ALTER TABLE resumes ADD COLUMN label TEXT")
        .execute(pool)
        .await;

    // Resume events table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            resume_id TEXT,
            actor TEXT,
            action TEXT,
            note TEXT,
            at TEXT DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Resume assets table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS resume_assets (
            id TEXT PRIMARY KEY,
            resume_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            path TEXT NOT NULL,
            page INTEGER,
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(resume_id) REFERENCES resumes(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Applications table
    // Status values: submitted, reviewed, shortlisted, interviewed, offered, hired, rejected, withdrawn
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS applications (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            job_id TEXT NOT NULL,
            resume_id TEXT,
            status TEXT DEFAULT 'submitted' CHECK (status IN (
                'submitted', 'reviewed', 'shortlisted', 'interviewed', 
                'offered', 'hired', 'rejected', 'withdrawn'
            )),
            current_stage TEXT DEFAULT 'Applied' CHECK (current_stage IN (
                'Applied', 'Resume Review', 'Shortlisted', 'Interview Scheduled',
                'Interview Completed', 'Offer Extended', 'Hired', 'Rejected'
            )),
            cover_letter TEXT,
            applied_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            UNIQUE(user_id, job_id),
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE,
            FOREIGN KEY(resume_id) REFERENCES resumes(id) ON DELETE SET NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Application status history table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS application_status_history (
            id TEXT PRIMARY KEY,
            application_id TEXT NOT NULL,
            status TEXT NOT NULL,
            changed_by TEXT NOT NULL,
            notes TEXT,
            changed_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(application_id) REFERENCES applications(id),
            FOREIGN KEY(changed_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Stage history table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS stage_history (
            id TEXT PRIMARY KEY,
            application_id TEXT NOT NULL,
            stage TEXT NOT NULL,
            changed_by TEXT NOT NULL,
            changed_by_name TEXT,
            notes TEXT,
            changed_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(application_id) REFERENCES applications(id) ON DELETE CASCADE,
            FOREIGN KEY(changed_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Video submissions table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS video_submissions (
            id TEXT PRIMARY KEY,
            application_id TEXT NOT NULL,
            s3_url TEXT NOT NULL,
            filename TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            duration_seconds INTEGER NOT NULL,
            mime_type TEXT NOT NULL,
            uploaded_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(application_id) REFERENCES applications(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Videos table (for user-uploaded videos not tied to applications)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS videos (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            s3_url TEXT,
            filename TEXT,
            file_size INTEGER,
            duration_seconds INTEGER NOT NULL,
            mime_type TEXT,
            uploaded_at TEXT DEFAULT (datetime('now')),
            video_source TEXT DEFAULT 'upload',
            youtube_video_id TEXT,
            youtube_thumbnail_url TEXT,
            youtube_title TEXT,
            youtube_description TEXT,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Add YouTube columns to existing videos table if they don't exist
    let _ = sqlx::query("ALTER TABLE videos ADD COLUMN video_source TEXT DEFAULT 'upload'")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE videos ADD COLUMN youtube_video_id TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE videos ADD COLUMN youtube_thumbnail_url TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE videos ADD COLUMN youtube_title TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE videos ADD COLUMN youtube_description TEXT")
        .execute(pool)
        .await;

    Ok(())
}

async fn create_interview_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Interviews table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS interviews (
            id TEXT PRIMARY KEY,
            application_id TEXT NOT NULL,
            candidate_id TEXT NOT NULL,
            job_id TEXT,
            scheduled_date TEXT NOT NULL,
            duration_minutes INTEGER NOT NULL,
            interview_type TEXT NOT NULL,
            google_meet_link TEXT,
            google_calendar_event_id TEXT,
            panel_members TEXT NOT NULL,
            notes TEXT,
            status TEXT DEFAULT 'scheduled',
            created_by TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(application_id) REFERENCES applications(id) ON DELETE CASCADE,
            FOREIGN KEY(candidate_id) REFERENCES users(id),
            FOREIGN KEY(job_id) REFERENCES jobs(id),
            FOREIGN KEY(created_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create panelists table for storing frequently used interview panelists
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS panelists (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            name TEXT,
            role TEXT,
            department TEXT,
            is_active INTEGER DEFAULT 1,
            usage_count INTEGER DEFAULT 0,
            last_used_at TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create index on email for faster lookups
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_panelists_email ON panelists(email)")
        .execute(pool)
        .await?;

    // Interview interviewers junction table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS interview_interviewers (
            interview_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            PRIMARY KEY (interview_id, user_id),
            FOREIGN KEY(interview_id) REFERENCES interviews(id) ON DELETE CASCADE,
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Offer letters table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS offer_letters (
            id TEXT PRIMARY KEY,
            candidate_id TEXT NOT NULL,
            job_id TEXT,
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
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(candidate_id) REFERENCES users(id),
            FOREIGN KEY(job_id) REFERENCES jobs(id),
            FOREIGN KEY(created_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Email history table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS email_history (
            id TEXT PRIMARY KEY,
            application_id TEXT NOT NULL,
            candidate_id TEXT NOT NULL,
            job_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            content TEXT NOT NULL,
            cc TEXT,
            sent_by TEXT NOT NULL,
            sent_at TEXT DEFAULT (datetime('now')),
            email_type TEXT,
            FOREIGN KEY(application_id) REFERENCES applications(id) ON DELETE CASCADE,
            FOREIGN KEY(candidate_id) REFERENCES users(id),
            FOREIGN KEY(job_id) REFERENCES jobs(id),
            FOREIGN KEY(sent_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_messaging_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Conversation messages table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS conversation_messages (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            message TEXT NOT NULL,
            is_read INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Message attachments table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS message_attachments (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            filename TEXT NOT NULL,
            original_filename TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            mime_type TEXT NOT NULL,
            file_path TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(message_id) REFERENCES conversation_messages(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_system_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // System settings table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS system_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            encrypted INTEGER DEFAULT 0,
            description TEXT,
            updated_at TEXT DEFAULT (datetime('now')),
            updated_by TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Saved jobs table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS saved_jobs (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            job_id TEXT NOT NULL,
            saved_at TEXT DEFAULT (datetime('now')),
            UNIQUE(user_id, job_id),
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await?;

    // User OAuth tokens table (for storing per-user OAuth tokens like YouTube)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_oauth_tokens (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            provider TEXT NOT NULL,
            access_token TEXT NOT NULL,
            refresh_token TEXT,
            token_expires_at TEXT,
            scopes TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
            UNIQUE(user_id, provider)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create index for user OAuth tokens
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_oauth_tokens_user_provider ON user_oauth_tokens(user_id, provider)")
        .execute(pool)
        .await?;

    // Admin users table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS admin_users (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL UNIQUE,
            role TEXT NOT NULL DEFAULT 'admin',
            permissions TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            created_by TEXT,
            FOREIGN KEY(user_id) REFERENCES users(id),
            FOREIGN KEY(created_by) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // AI usage logs table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS ai_usage_logs (
            id TEXT PRIMARY KEY,
            user_id TEXT,
            action TEXT NOT NULL,
            model TEXT NOT NULL,
            purpose TEXT,
            tokens_used INTEGER,
            cost_estimate REAL,
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY(user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_indexes(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let indexes = vec![
        // User and profile indexes
        "CREATE INDEX IF NOT EXISTS idx_experiences_user_id ON experiences(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_education_user_id ON education(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_testimonials_user ON testimonials(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_testimonials_featured ON testimonials(featured, approved)",
        
        // Company indexes
        "CREATE INDEX IF NOT EXISTS idx_companies_name ON companies(name)",
        "CREATE INDEX IF NOT EXISTS idx_company_assets_company_id ON company_assets(company_id)",
        "CREATE INDEX IF NOT EXISTS idx_company_assets_type ON company_assets(company_id, asset_type)",
        "CREATE INDEX IF NOT EXISTS idx_company_assets_default ON company_assets(company_id, is_default)",
        
        // Job template indexes
        "CREATE INDEX IF NOT EXISTS idx_job_templates_type ON job_templates(template_type)",
        "CREATE INDEX IF NOT EXISTS idx_job_templates_created_by ON job_templates(created_by)",
        "CREATE INDEX IF NOT EXISTS idx_job_templates_company ON job_templates(company_id)",
        
        // Job indexes
        "CREATE INDEX IF NOT EXISTS idx_jobs_company_id ON jobs(company_id)",
        "CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status)",
        "CREATE INDEX IF NOT EXISTS idx_jobs_featured ON jobs(is_featured, status)",
        "CREATE INDEX IF NOT EXISTS idx_jobs_template_id ON jobs(template_id)",
        "CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_job_status_history_job_id ON job_status_history(job_id)",
        "CREATE INDEX IF NOT EXISTS idx_job_status_history_changed_at ON job_status_history(job_id, changed_at)",
        "CREATE INDEX IF NOT EXISTS idx_job_views_job_date ON job_views(job_id, viewed_at)",
        "CREATE INDEX IF NOT EXISTS idx_job_social_images_job_id ON job_social_images(job_id)",
        
        // Application indexes
        "CREATE INDEX IF NOT EXISTS idx_applications_user_job ON applications(user_id, job_id)",
        "CREATE INDEX IF NOT EXISTS idx_applications_status ON applications(status)",
        "CREATE INDEX IF NOT EXISTS idx_applications_current_stage ON applications(current_stage)",
        "CREATE INDEX IF NOT EXISTS idx_applications_job_stage ON applications(job_id, current_stage)",
        "CREATE INDEX IF NOT EXISTS idx_stage_history_application_id ON stage_history(application_id)",
        "CREATE INDEX IF NOT EXISTS idx_stage_history_changed_at ON stage_history(application_id, changed_at)",
        "CREATE INDEX IF NOT EXISTS idx_stage_history_stage ON stage_history(stage)",
        "CREATE INDEX IF NOT EXISTS idx_video_submissions_application_id ON video_submissions(application_id)",
        
        // Interview indexes
        "CREATE INDEX IF NOT EXISTS idx_interviews_application_id ON interviews(application_id)",
        "CREATE INDEX IF NOT EXISTS idx_interviews_candidate_id ON interviews(candidate_id)",
        "CREATE INDEX IF NOT EXISTS idx_interviews_job_id ON interviews(job_id)",
        "CREATE INDEX IF NOT EXISTS idx_interviews_scheduled_date ON interviews(scheduled_date)",
        "CREATE INDEX IF NOT EXISTS idx_interviews_status ON interviews(status)",
        "CREATE INDEX IF NOT EXISTS idx_offer_letters_candidate_id ON offer_letters(candidate_id)",
        "CREATE INDEX IF NOT EXISTS idx_email_history_application_id ON email_history(application_id)",
        "CREATE INDEX IF NOT EXISTS idx_email_history_candidate_id ON email_history(candidate_id)",
        "CREATE INDEX IF NOT EXISTS idx_email_history_job_id ON email_history(job_id)",
        
        // Message indexes
        "CREATE INDEX IF NOT EXISTS idx_messages_user_created ON conversation_messages(user_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_message_attachments_message ON message_attachments(message_id)",
        
        // System indexes
        "CREATE INDEX IF NOT EXISTS idx_ai_usage_logs_user_id ON ai_usage_logs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_ai_usage_logs_created_at ON ai_usage_logs(created_at)",
        
        // Saved jobs indexes
        "CREATE INDEX IF NOT EXISTS idx_saved_jobs_user_id ON saved_jobs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_saved_jobs_job_id ON saved_jobs(job_id)",
        "CREATE INDEX IF NOT EXISTS idx_saved_jobs_user_job ON saved_jobs(user_id, job_id)",
    ];

    for index_sql in indexes {
        sqlx::query(index_sql).execute(pool).await?;
    }

    Ok(())
}

/// Sync current_stage field with status for existing applications
/// This ensures the Applications by Stage analytics shows correct data
async fn sync_application_stages(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Update current_stage based on status for all applications
    let updates = vec![
        ("submitted", "Applied"),
        ("reviewed", "Resume Review"),
        ("shortlisted", "Shortlisted"),
        ("interviewed", "Interview Completed"),
        ("offered", "Offer Extended"),
        ("hired", "Hired"),
        ("rejected", "Rejected"),
        ("withdrawn", "Applied"),
    ];

    for (status, stage) in updates {
        let result = sqlx::query(
            "UPDATE applications SET current_stage = ? WHERE status = ? AND current_stage != ?"
        )
        .bind(stage)
        .bind(status)
        .bind(stage)
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            info!(
                status = %status,
                stage = %stage,
                count = result.rows_affected(),
                "Synced application stages"
            );
        }
    }

    Ok(())
}
