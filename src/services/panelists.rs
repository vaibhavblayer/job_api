// src/services/panelists.rs
//! Service for managing interview panelists

use crate::candidates::models::{InterviewPanelMember, Panelist};
use crate::common::ApiError;
use sqlx::SqlitePool;
use tracing::{debug, error, info};

use crate::common::generate_panelist_id;

/// Get all active panelists, ordered by usage count (most used first)
pub async fn get_panelists(pool: &SqlitePool) -> Result<Vec<Panelist>, ApiError> {
    debug!("Fetching all active panelists");

    let panelists = sqlx::query_as::<_, Panelist>(
        r#"
        SELECT * FROM panelists 
        WHERE is_active = 1 
        ORDER BY usage_count DESC, last_used_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching panelists");
        ApiError::DatabaseError(e)
    })?;

    info!(count = panelists.len(), "Fetched panelists successfully");
    Ok(panelists)
}

/// Save or update a panelist (upsert)
pub async fn upsert_panelist(
    pool: &SqlitePool,
    panel_member: &InterviewPanelMember,
) -> Result<(), ApiError> {
    debug!(
        email = %panel_member.email,
        name = ?panel_member.name,
        role = ?panel_member.role,
        "Upserting panelist"
    );

    // Check if panelist exists
    let existing = sqlx::query_scalar::<_, Option<String>>(
        "SELECT id FROM panelists WHERE email = ?"
    )
    .bind(&panel_member.email)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error checking panelist existence");
        ApiError::DatabaseError(e)
    })?;

    if let Some(id) = existing {
        // Update existing panelist
        sqlx::query(
            r#"
            UPDATE panelists 
            SET name = COALESCE(?, name),
                role = COALESCE(?, role),
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(&panel_member.name)
        .bind(&panel_member.role)
        .bind(&id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error updating panelist");
            ApiError::DatabaseError(e)
        })?;

        debug!(id = ?id, "Updated existing panelist");
    } else {
        // Insert new panelist
        let id = generate_panelist_id();
        sqlx::query(
            r#"
            INSERT INTO panelists (id, email, name, role, is_active, usage_count)
            VALUES (?, ?, ?, ?, 1, 0)
            "#,
        )
        .bind(&id)
        .bind(&panel_member.email)
        .bind(&panel_member.name)
        .bind(&panel_member.role)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error inserting panelist");
            ApiError::DatabaseError(e)
        })?;

        info!(id = %id, email = %panel_member.email, "Created new panelist");
    }

    Ok(())
}

/// Increment usage count for a panelist
pub async fn increment_panelist_usage(pool: &SqlitePool, email: &str) -> Result<(), ApiError> {
    debug!(email = %email, "Incrementing panelist usage count");

    sqlx::query(
        r#"
        UPDATE panelists 
        SET usage_count = usage_count + 1,
            last_used_at = datetime('now'),
            updated_at = datetime('now')
        WHERE email = ?
        "#,
    )
    .bind(email)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error incrementing panelist usage");
        ApiError::DatabaseError(e)
    })?;

    Ok(())
}

/// Deactivate a panelist (soft delete)
pub async fn deactivate_panelist(pool: &SqlitePool, panelist_id: &str) -> Result<(), ApiError> {
    debug!(panelist_id = %panelist_id, "Deactivating panelist");

    sqlx::query(
        r#"
        UPDATE panelists 
        SET is_active = 0,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(panelist_id)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error deactivating panelist");
        ApiError::DatabaseError(e)
    })?;

    info!(panelist_id = %panelist_id, "Deactivated panelist");
    Ok(())
}
