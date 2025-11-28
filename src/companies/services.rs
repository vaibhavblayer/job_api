use super::models::{Company, CompanyAsset, CreateCompanyRequest, UpdateCompanyRequest};
use crate::common::{generate_asset_id, generate_company_id, ApiError, Validator};
use sqlx::SqlitePool;
use tracing::info;

pub struct CompaniesService {
    db: SqlitePool,
}

impl CompaniesService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    // ============================================================================
    // Company CRUD Operations
    // ============================================================================

    /// Get all companies
    pub async fn get_all_companies(&self) -> Result<Vec<Company>, ApiError> {
        let companies = sqlx::query_as::<_, Company>(
            r#"
            SELECT id, name, description, website, industry, company_size, founded_year,
                   headquarters, operating_locations, culture, benefits, default_logo_url,
                   created_at, updated_at
            FROM companies
            ORDER BY name ASC
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(companies)
    }

    /// Get company by ID
    pub async fn get_company_by_id(&self, company_id: &str) -> Result<Company, ApiError> {
        let company = sqlx::query_as::<_, Company>(
            r#"
            SELECT id, name, description, website, industry, company_size, founded_year,
                   headquarters, operating_locations, culture, benefits, default_logo_url,
                   created_at, updated_at
            FROM companies
            WHERE id = ?
            "#,
        )
        .bind(company_id)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Company not found".to_string()))?;

        Ok(company)
    }

    /// Create a new company
    pub async fn create_company(&self, request: CreateCompanyRequest) -> Result<Company, ApiError> {
        // Validate request
        let validation_result = request.validate(&request);
        if !validation_result.is_valid {
            return Err(ApiError::from(validation_result));
        }

        let company_id = generate_company_id();
        let now = chrono::Utc::now().to_rfc3339();

        // Convert JSON fields to strings
        let headquarters_json = request
            .headquarters
            .as_ref()
            .map(|h| serde_json::to_string(h).unwrap_or_else(|_| "{}".to_string()));

        let operating_locations_json = request
            .operating_locations
            .as_ref()
            .map(|locs| serde_json::to_string(locs).unwrap_or_else(|_| "[]".to_string()));

        let culture_json = request
            .culture
            .as_ref()
            .map(|c| serde_json::to_string(c).unwrap_or_else(|_| "{}".to_string()));

        let benefits_json = request
            .benefits
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_else(|_| "[]".to_string()));

        sqlx::query(
            r#"
            INSERT INTO companies (
                id, name, description, website, industry, company_size, founded_year,
                headquarters, operating_locations, culture, benefits, default_logo_url,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&company_id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&request.website)
        .bind(&request.industry)
        .bind(&request.company_size)
        .bind(&request.founded_year)
        .bind(headquarters_json.as_deref())
        .bind(operating_locations_json.as_deref())
        .bind(culture_json.as_deref())
        .bind(benefits_json.as_deref())
        .bind(&request.default_logo_url)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                ApiError::ValidationError("Company name already exists".to_string())
            } else {
                ApiError::DatabaseError(e)
            }
        })?;

        info!("Created company: {} ({})", request.name, company_id);

        self.get_company_by_id(&company_id).await
    }

    /// Update an existing company
    pub async fn update_company(
        &self,
        company_id: &str,
        request: UpdateCompanyRequest,
    ) -> Result<Company, ApiError> {
        // Check if company exists
        self.get_company_by_id(company_id).await?;

        let now = chrono::Utc::now().to_rfc3339();

        // Build dynamic update query
        let mut updates = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(name) = &request.name {
            if name.trim().is_empty() {
                return Err(ApiError::ValidationError(
                    "Company name cannot be empty".to_string(),
                ));
            }
            updates.push("name = ?");
            params.push(name.clone());
        }

        if let Some(description) = &request.description {
            updates.push("description = ?");
            params.push(description.clone());
        }

        if let Some(website) = &request.website {
            updates.push("website = ?");
            params.push(website.clone());
        }

        if let Some(industry) = &request.industry {
            updates.push("industry = ?");
            params.push(industry.clone());
        }

        if let Some(company_size) = &request.company_size {
            updates.push("company_size = ?");
            params.push(company_size.clone());
        }

        if let Some(founded_year) = &request.founded_year {
            updates.push("founded_year = ?");
            params.push(founded_year.to_string());
        }

        if let Some(headquarters) = &request.headquarters {
            updates.push("headquarters = ?");
            params.push(serde_json::to_string(headquarters).unwrap_or_else(|_| "{}".to_string()));
        }

        if let Some(operating_locations) = &request.operating_locations {
            updates.push("operating_locations = ?");
            params.push(
                serde_json::to_string(operating_locations).unwrap_or_else(|_| "[]".to_string()),
            );
        }

        if let Some(culture) = &request.culture {
            updates.push("culture = ?");
            params.push(serde_json::to_string(culture).unwrap_or_else(|_| "{}".to_string()));
        }

        if let Some(benefits) = &request.benefits {
            updates.push("benefits = ?");
            params.push(serde_json::to_string(benefits).unwrap_or_else(|_| "[]".to_string()));
        }

        if let Some(default_logo_url) = &request.default_logo_url {
            updates.push("default_logo_url = ?");
            params.push(default_logo_url.clone());
        }

        if updates.is_empty() {
            return self.get_company_by_id(company_id).await;
        }

        updates.push("updated_at = ?");
        params.push(now.clone());
        params.push(company_id.to_string());

        let query = format!("UPDATE companies SET {} WHERE id = ?", updates.join(", "));

        let mut query_builder = sqlx::query(&query);
        for param in params {
            query_builder = query_builder.bind(param);
        }

        query_builder.execute(&self.db).await.map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                ApiError::ValidationError("Company name already exists".to_string())
            } else {
                ApiError::DatabaseError(e)
            }
        })?;

        info!("Updated company: {}", company_id);

        self.get_company_by_id(company_id).await
    }

    /// Delete a company
    pub async fn delete_company(&self, company_id: &str) -> Result<(), ApiError> {
        // Check if company exists
        self.get_company_by_id(company_id).await?;

        // Delete company (CASCADE will delete associated assets)
        let result = sqlx::query("DELETE FROM companies WHERE id = ?")
            .bind(company_id)
            .execute(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;

        if result.rows_affected() == 0 {
            return Err(ApiError::BadRequest("Company not found".to_string()));
        }

        info!("Deleted company: {}", company_id);

        Ok(())
    }

    // ============================================================================
    // Company Asset Management
    // ============================================================================

    /// Get all assets for a company
    pub async fn get_company_assets(
        &self,
        company_id: &str,
    ) -> Result<Vec<CompanyAsset>, ApiError> {
        // Check if company exists
        self.get_company_by_id(company_id).await?;

        let assets = sqlx::query_as::<_, CompanyAsset>(
            r#"
            SELECT id, company_id, asset_type, url, filename, file_size, mime_type, is_default, created_at
            FROM company_assets
            WHERE company_id = ?
            ORDER BY is_default DESC, created_at DESC
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(assets)
    }

    /// Create a new company asset
    pub async fn create_company_asset(
        &self,
        company_id: &str,
        asset_type: &str,
        url: String,
        filename: String,
        file_size: i64,
        mime_type: String,
        is_default: bool,
    ) -> Result<CompanyAsset, ApiError> {
        // Check if company exists
        self.get_company_by_id(company_id).await?;

        // Validate asset_type
        if asset_type != "logo" && asset_type != "image" {
            return Err(ApiError::ValidationError(
                "Asset type must be 'logo' or 'image'".to_string(),
            ));
        }

        let asset_id = generate_asset_id();
        let now = chrono::Utc::now().to_rfc3339();
        let is_default_int = if is_default { 1 } else { 0 };

        // If this is set as default, unset other defaults of the same type
        if is_default {
            sqlx::query(
                r#"
                UPDATE company_assets
                SET is_default = 0
                WHERE company_id = ? AND asset_type = ?
                "#,
            )
            .bind(company_id)
            .bind(asset_type)
            .execute(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;
        }

        sqlx::query(
            r#"
            INSERT INTO company_assets (id, company_id, asset_type, url, filename, file_size, mime_type, is_default, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&asset_id)
        .bind(company_id)
        .bind(asset_type)
        .bind(&url)
        .bind(&filename)
        .bind(file_size)
        .bind(&mime_type)
        .bind(is_default_int)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // If this is a logo and set as default, update the company's default_logo_url
        if asset_type == "logo" && is_default {
            sqlx::query(
                r#"
                UPDATE companies
                SET default_logo_url = ?
                WHERE id = ?
                "#,
            )
            .bind(&url)
            .bind(company_id)
            .execute(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;

            info!(
                "Updated company {} default_logo_url to {}",
                company_id, url
            );
        }

        info!(
            "Created company asset: {} for company {}",
            asset_id, company_id
        );

        self.get_company_asset_by_id(&asset_id).await
    }

    /// Get a specific company asset by ID
    pub async fn get_company_asset_by_id(&self, asset_id: &str) -> Result<CompanyAsset, ApiError> {
        let asset = sqlx::query_as::<_, CompanyAsset>(
            r#"
            SELECT id, company_id, asset_type, url, filename, file_size, mime_type, is_default, created_at
            FROM company_assets
            WHERE id = ?
            "#,
        )
        .bind(asset_id)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Asset not found".to_string()))?;

        Ok(asset)
    }

    /// Delete a company asset
    pub async fn delete_company_asset(
        &self,
        company_id: &str,
        asset_id: &str,
    ) -> Result<(), ApiError> {
        // Check if asset exists and belongs to the company
        let asset = self.get_company_asset_by_id(asset_id).await?;

        if asset.company_id != company_id {
            return Err(ApiError::Forbidden(
                "Asset does not belong to this company".to_string(),
            ));
        }

        let result = sqlx::query("DELETE FROM company_assets WHERE id = ?")
            .bind(asset_id)
            .execute(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;

        if result.rows_affected() == 0 {
            return Err(ApiError::BadRequest("Asset not found".to_string()));
        }

        info!("Deleted company asset: {}", asset_id);

        Ok(())
    }

    /// Set an asset as the default for its type
    pub async fn set_default_asset(
        &self,
        company_id: &str,
        asset_id: &str,
    ) -> Result<CompanyAsset, ApiError> {
        // Check if asset exists and belongs to the company
        let asset = self.get_company_asset_by_id(asset_id).await?;

        if asset.company_id != company_id {
            return Err(ApiError::Forbidden(
                "Asset does not belong to this company".to_string(),
            ));
        }

        // Unset other defaults of the same type
        sqlx::query(
            r#"
            UPDATE company_assets
            SET is_default = 0
            WHERE company_id = ? AND asset_type = ?
            "#,
        )
        .bind(company_id)
        .bind(&asset.asset_type)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // Set this asset as default
        sqlx::query(
            r#"
            UPDATE company_assets
            SET is_default = 1
            WHERE id = ?
            "#,
        )
        .bind(asset_id)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // If this is a logo, update the company's default_logo_url
        if asset.asset_type == "logo" {
            sqlx::query(
                r#"
                UPDATE companies
                SET default_logo_url = ?
                WHERE id = ?
                "#,
            )
            .bind(&asset.url)
            .bind(company_id)
            .execute(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;

            info!(
                "Updated company {} default_logo_url to {}",
                company_id, asset.url
            );
        }

        info!(
            "Set asset {} as default for company {}",
            asset_id, company_id
        );

        self.get_company_asset_by_id(asset_id).await
    }
}
