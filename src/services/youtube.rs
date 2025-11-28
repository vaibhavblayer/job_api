// src/services/youtube.rs
//! YouTube Data API v3 integration for video management

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::services::settings::SettingsService;

#[derive(Debug, Clone)]
pub struct YouTubeService {
    client: Client,
    settings_service: Arc<SettingsService>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YouTubeVideo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub thumbnail_url: String,
    pub published_at: String,
    pub duration: String,
    pub view_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct YouTubeApiResponse {
    items: Option<Vec<YouTubeVideoItem>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YouTubeVideoItem {
    id: VideoId,
    snippet: VideoSnippet,
    #[serde(rename = "contentDetails")]
    content_details: Option<ContentDetails>,
    statistics: Option<Statistics>,
}

// Playlist items response structure
#[derive(Debug, Deserialize)]
struct PlaylistItemsResponse {
    items: Option<Vec<PlaylistItem>>,
}

#[derive(Debug, Deserialize)]
struct PlaylistItem {
    snippet: PlaylistItemSnippet,
    #[serde(rename = "contentDetails")]
    content_details: Option<PlaylistItemContentDetails>,
}

#[derive(Debug, Deserialize)]
struct PlaylistItemSnippet {
    #[serde(rename = "resourceId")]
    resource_id: ResourceId,
}

#[derive(Debug, Deserialize)]
struct ResourceId {
    #[serde(rename = "videoId")]
    video_id: String,
}

#[derive(Debug, Deserialize)]
struct PlaylistItemContentDetails {
    #[serde(rename = "videoId")]
    video_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum VideoId {
    String(String),
    Object { #[serde(rename = "videoId")] video_id: String },
}

#[derive(Debug, Deserialize)]
struct VideoSnippet {
    title: String,
    description: String,
    #[serde(rename = "publishedAt")]
    published_at: String,
    thumbnails: Thumbnails,
}

#[derive(Debug, Deserialize)]
struct Thumbnails {
    #[serde(rename = "medium")]
    medium: Option<Thumbnail>,
    #[serde(rename = "high")]
    high: Option<Thumbnail>,
    #[serde(rename = "default")]
    default: Option<Thumbnail>,
}

#[derive(Debug, Deserialize)]
struct Thumbnail {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ContentDetails {
    duration: String,
}

#[derive(Debug, Deserialize)]
struct Statistics {
    #[serde(rename = "viewCount")]
    view_count: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum YouTubeError {
    #[error("YouTube API error: {0}")]
    ApiError(String),
    #[error("Not configured: {0}")]
    NotConfigured(String),
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

impl YouTubeService {
    pub fn new(settings_service: Arc<SettingsService>) -> Self {
        Self {
            client: Client::new(),
            settings_service,
        }
    }

    /// Get user's uploaded videos from their YouTube channel
    pub async fn get_user_videos(
        &self,
        access_token: &str,
        max_results: u32,
    ) -> Result<Vec<YouTubeVideo>, YouTubeError> {
        debug!("Fetching user's YouTube videos");

        // First, get the user's uploads playlist ID
        let channel_response = self
            .client
            .get("https://www.googleapis.com/youtube/v3/channels")
            .query(&[("part", "contentDetails"), ("mine", "true")])
            .bearer_auth(access_token)
            .send()
            .await?;

        if !channel_response.status().is_success() {
            let error_text = channel_response.text().await.unwrap_or_default();
            error!("YouTube API error: {}", error_text);
            return Err(YouTubeError::ApiError(error_text));
        }

        let channel_data: serde_json::Value = channel_response.json().await?;
        let uploads_playlist_id = channel_data["items"][0]["contentDetails"]["relatedPlaylists"]
            ["uploads"]
            .as_str()
            .ok_or_else(|| {
                YouTubeError::InvalidResponse("Could not find uploads playlist".to_string())
            })?;

        debug!("Found uploads playlist: {}", uploads_playlist_id);

        // Get videos from uploads playlist
        let videos_response = self
            .client
            .get("https://www.googleapis.com/youtube/v3/playlistItems")
            .query(&[
                ("part", "snippet,contentDetails"),
                ("playlistId", uploads_playlist_id),
                ("maxResults", &max_results.to_string()),
            ])
            .bearer_auth(access_token)
            .send()
            .await?;

        if !videos_response.status().is_success() {
            let error_text = videos_response.text().await.unwrap_or_default();
            error!("YouTube API error: {}", error_text);
            return Err(YouTubeError::ApiError(error_text));
        }

        let playlist_response: PlaylistItemsResponse = videos_response.json().await?;

        // Get video IDs from playlist items
        let video_ids: Vec<String> = playlist_response
            .items
            .unwrap_or_default()
            .iter()
            .map(|item| item.snippet.resource_id.video_id.clone())
            .collect();

        if video_ids.is_empty() {
            return Ok(Vec::new());
        }

        debug!("Found {} video IDs from playlist", video_ids.len());

        // Fetch detailed video information
        let details_response = self
            .client
            .get("https://www.googleapis.com/youtube/v3/videos")
            .query(&[
                ("part", "snippet,contentDetails,statistics"),
                ("id", &video_ids.join(",")),
            ])
            .bearer_auth(access_token)
            .send()
            .await?;

        let details_data: YouTubeApiResponse = if details_response.status().is_success() {
            details_response.json().await?
        } else {
            let error_text = details_response.text().await.unwrap_or_default();
            warn!("Failed to fetch video details: {}", error_text);
            return Err(YouTubeError::ApiError(format!("Failed to fetch video details: {}", error_text)));
        };

        let videos: Vec<YouTubeVideo> = details_data
            .items
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                let video_id = match item.id {
                    VideoId::String(id) => id,
                    VideoId::Object { video_id } => video_id,
                };

                let thumbnail_url = item
                    .snippet
                    .thumbnails
                    .high
                    .or(item.snippet.thumbnails.medium)
                    .or(item.snippet.thumbnails.default)
                    .map(|t| t.url)
                    .unwrap_or_else(|| {
                        format!("https://img.youtube.com/vi/{}/mqdefault.jpg", video_id)
                    });

                let duration = item
                    .content_details
                    .as_ref()
                    .map(|cd| parse_youtube_duration(&cd.duration))
                    .unwrap_or_else(|| "Unknown".to_string());

                let view_count = item
                    .statistics
                    .as_ref()
                    .and_then(|s| s.view_count.as_ref())
                    .and_then(|v| v.parse().ok());

                YouTubeVideo {
                    id: video_id,
                    title: item.snippet.title,
                    description: item.snippet.description,
                    thumbnail_url,
                    published_at: item.snippet.published_at,
                    duration,
                    view_count,
                }
            })
            .collect();

        info!("Fetched {} YouTube videos", videos.len());
        Ok(videos)
    }

    /// Get details for a specific YouTube video
    pub async fn get_video_details(
        &self,
        video_id: &str,
        access_token: &str,
    ) -> Result<YouTubeVideo, YouTubeError> {
        debug!("Fetching YouTube video details for: {}", video_id);

        let response = self
            .client
            .get("https://www.googleapis.com/youtube/v3/videos")
            .query(&[
                ("part", "snippet,contentDetails,statistics"),
                ("id", video_id),
            ])
            .bearer_auth(access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!("YouTube API error: {}", error_text);
            return Err(YouTubeError::ApiError(error_text));
        }

        let api_response: YouTubeApiResponse = response.json().await?;

        let item = api_response
            .items
            .unwrap_or_default()
            .into_iter()
            .next()
            .ok_or_else(|| YouTubeError::InvalidResponse("Video not found".to_string()))?;

        let video_id = match item.id {
            VideoId::String(id) => id,
            VideoId::Object { video_id } => video_id,
        };

        let thumbnail_url = item
            .snippet
            .thumbnails
            .high
            .or(item.snippet.thumbnails.medium)
            .or(item.snippet.thumbnails.default)
            .map(|t| t.url)
            .unwrap_or_else(|| format!("https://img.youtube.com/vi/{}/mqdefault.jpg", video_id));

        let duration = item
            .content_details
            .as_ref()
            .map(|cd| parse_youtube_duration(&cd.duration))
            .unwrap_or_else(|| "Unknown".to_string());

        let view_count = item
            .statistics
            .as_ref()
            .and_then(|s| s.view_count.as_ref())
            .and_then(|v| v.parse().ok());

        Ok(YouTubeVideo {
            id: video_id,
            title: item.snippet.title,
            description: item.snippet.description,
            thumbnail_url,
            published_at: item.snippet.published_at,
            duration,
            view_count,
        })
    }
}

/// Parse YouTube duration format (PT1H2M10S) to readable format
fn parse_youtube_duration(duration: &str) -> String {
    let duration = duration.trim_start_matches("PT");
    
    let hours = duration
        .split('H')
        .next()
        .and_then(|h| h.parse::<u32>().ok())
        .unwrap_or(0);
    
    let minutes = duration
        .split('H')
        .last()
        .and_then(|rest| rest.split('M').next())
        .and_then(|m| m.parse::<u32>().ok())
        .unwrap_or(0);
    
    let seconds = duration
        .split('M')
        .last()
        .and_then(|rest| rest.trim_end_matches('S').parse::<u32>().ok())
        .unwrap_or(0);

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_youtube_duration() {
        assert_eq!(parse_youtube_duration("PT1H2M10S"), "1:02:10");
        assert_eq!(parse_youtube_duration("PT5M30S"), "5:30");
        assert_eq!(parse_youtube_duration("PT45S"), "0:45");
        assert_eq!(parse_youtube_duration("PT1H0M0S"), "1:00:00");
    }
}
