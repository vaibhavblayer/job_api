use serde::{Deserialize, Serialize};
use sqlx::FromRow;

fn deserialize_bool_from_int<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: i64 = Deserialize::deserialize(deserializer)?;
    Ok(value)
}

fn serialize_bool_to_bool<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_bool(*value != 0)
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Company {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub industry: Option<String>,
    pub company_size: Option<String>, // e.g., "1-10", "11-50", "51-200", "201-500", "501-1000", "1000+"
    pub founded_year: Option<i32>,
    pub headquarters: Option<String>, // JSON: {"city": "San Francisco", "state": "CA", "country": "USA"}
    pub operating_locations: Option<String>, // JSON array: [{"city": "NYC", "state": "NY", "country": "USA"}, ...]
    pub culture: Option<String>, // JSON: {"values": ["Innovation", "Collaboration"], "work_environment": "Hybrid", "perks": [...]}
    pub benefits: Option<String>, // JSON array: ["Health Insurance", "401k", "Remote Work", ...]
    pub default_logo_url: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CompanyAsset {
    pub id: String,
    pub company_id: String,
    pub asset_type: String, // 'logo' or 'image'
    pub url: String,
    pub filename: String,
    pub file_size: i64,
    pub mime_type: String,
    #[serde(deserialize_with = "deserialize_bool_from_int")]
    #[serde(serialize_with = "serialize_bool_to_bool")]
    pub is_default: i64,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCompanyRequest {
    pub name: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub industry: Option<String>,
    pub company_size: Option<String>,
    pub founded_year: Option<i32>,
    pub headquarters: Option<serde_json::Value>, // Will be stored as JSON string
    pub operating_locations: Option<Vec<serde_json::Value>>, // Will be stored as JSON array string
    pub culture: Option<serde_json::Value>,      // Will be stored as JSON string
    pub benefits: Option<Vec<String>>,           // Will be stored as JSON array string
    pub default_logo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCompanyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
    pub industry: Option<String>,
    pub company_size: Option<String>,
    pub founded_year: Option<i32>,
    pub headquarters: Option<serde_json::Value>,
    pub operating_locations: Option<Vec<serde_json::Value>>,
    pub culture: Option<serde_json::Value>,
    pub benefits: Option<Vec<String>>,
    pub default_logo_url: Option<String>,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Deserialize)]
pub struct SaveUrlAsAssetRequest {
    pub url: String,
    pub asset_type: String,
    pub is_default: Option<bool>,
}
