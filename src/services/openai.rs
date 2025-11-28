// src/services/openai.rs
use crate::services::settings::SettingsService;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum OpenAIError {
    #[error("API key not configured")]
    NotConfigured,

    #[error("API request failed: {0}")]
    RequestFailed(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Settings error: {0}")]
    SettingsError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
    pub models: ModelConfig,
    pub reasoning_effort: ReasoningEffortConfig,
}

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub resume_scanning: String,
    pub email_generation: String,
    pub message_responses: String,
    pub job_description_generation: String,
    pub image_generation: String,
}

#[derive(Debug, Clone)]
pub struct ReasoningEffortConfig {
    pub resume_scanning: String,
    pub email_generation: String,
    pub message_responses: String,
    pub job_description_generation: String,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            resume_scanning: "gpt-5-mini".to_string(),
            email_generation: "gpt-5-mini".to_string(),
            message_responses: "gpt-5-mini".to_string(),
            job_description_generation: "gpt-5".to_string(),
            image_generation: "gpt-image-1".to_string(),
        }
    }
}

impl Default for ReasoningEffortConfig {
    fn default() -> Self {
        Self {
            resume_scanning: "medium".to_string(),
            email_generation: "low".to_string(),
            message_responses: "low".to_string(),
            job_description_generation: "medium".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TextGenerationPurpose {
    ResumeScanning,
    EmailGeneration,
    MessageResponses,
    JobDescriptionGeneration,
    JobDescription,
    EmailComposition,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageSize {
    LinkedIn,  // 1200x627
    Instagram, // 1080x1080
    Facebook,  // 1200x630
    Custom { width: u32, height: u32 },
}

impl ImageSize {
    pub fn to_dimensions(&self) -> (u32, u32) {
        match self {
            ImageSize::LinkedIn => (1200, 627),
            ImageSize::Instagram => (1080, 1080),
            ImageSize::Facebook => (1200, 630),
            ImageSize::Custom { width, height } => (*width, *height),
        }
    }

    pub fn to_dalle_size(&self) -> String {
        match self {
            ImageSize::LinkedIn | ImageSize::Facebook => "1792x1024".to_string(),
            ImageSize::Instagram => "1024x1024".to_string(),
            ImageSize::Custom { width, height } => {
                // DALL-E 3 supports: 1024x1024, 1792x1024, 1024x1792
                if width == height {
                    "1024x1024".to_string()
                } else if width > height {
                    "1792x1024".to_string()
                } else {
                    "1024x1792".to_string()
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SocialPlatform {
    InstagramSquare, // 1080x1080
    InstagramStory,  // 1080x1920
    LinkedIn,        // 1200x627
    Twitter,         // 1200x675
    Facebook,        // 1200x630
}

impl SocialPlatform {
    pub fn to_dimensions(&self) -> (u32, u32) {
        match self {
            SocialPlatform::InstagramSquare => (1080, 1080),
            SocialPlatform::InstagramStory => (1080, 1920),
            SocialPlatform::LinkedIn => (1200, 627),
            SocialPlatform::Twitter => (1200, 675),
            SocialPlatform::Facebook => (1200, 630),
        }
    }

    pub fn to_dalle_size(&self) -> String {
        match self {
            SocialPlatform::InstagramSquare => "1024x1024".to_string(),
            SocialPlatform::InstagramStory => "1024x1792".to_string(),
            SocialPlatform::LinkedIn | SocialPlatform::Twitter | SocialPlatform::Facebook => {
                "1792x1024".to_string()
            }
        }
    }

    pub fn platform_name(&self) -> &str {
        match self {
            SocialPlatform::InstagramSquare => "Instagram Square Post",
            SocialPlatform::InstagramStory => "Instagram Story",
            SocialPlatform::LinkedIn => "LinkedIn Post",
            SocialPlatform::Twitter => "Twitter Post",
            SocialPlatform::Facebook => "Facebook Post",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ImageStyle {
    Professional,
    Modern,
    Creative,
    Minimalist,
    Vibrant,
}

impl ImageStyle {
    pub fn to_prompt_modifier(&self) -> &str {
        match self {
            ImageStyle::Professional => "professional, clean, corporate style",
            ImageStyle::Modern => "modern, sleek, contemporary design",
            ImageStyle::Creative => "creative, artistic, unique design",
            ImageStyle::Minimalist => "minimalist, simple, elegant design",
            ImageStyle::Vibrant => "vibrant, colorful, energetic design",
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    messages: Option<Vec<ChatMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    id: String,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    output: Vec<OutputItem>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OutputItem {
    #[serde(default)]
    content: Vec<ContentItem>,
}

#[derive(Debug, Deserialize)]
struct ContentItem {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ImageGenerationRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImageGenerationResponse {
    created: u64,
    data: Vec<ImageData>,
}

#[derive(Debug, Deserialize)]
struct ImageData {
    url: Option<String>,
    b64_json: Option<String>,
    revised_prompt: Option<String>,
}

#[derive(Debug)]
pub struct OpenAIService {
    settings_service: Arc<SettingsService>,
    client: Client,
}

impl OpenAIService {
    pub fn new(settings_service: Arc<SettingsService>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(180)) // Increased to 3 minutes for AI generation
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            settings_service,
            client,
        }
    }

    /// Get OpenAI configuration from settings
    pub async fn get_config(&self) -> Result<OpenAIConfig, OpenAIError> {
        // Get API key
        let api_key = self
            .settings_service
            .get_setting("openai_api_key")
            .await
            .map_err(|e| OpenAIError::SettingsError(e.to_string()))?
            .ok_or(OpenAIError::NotConfigured)?;

        // Get base URL (with default)
        let base_url = self
            .settings_service
            .get_setting("openai_base_url")
            .await
            .map_err(|e| OpenAIError::SettingsError(e.to_string()))?
            .unwrap_or_else(|| "https://api.openai.com".to_string());

        // Get model configurations (with defaults)
        let models = ModelConfig {
            resume_scanning: self
                .get_model_setting("openai_model_resume_scanning", "gpt-5-mini")
                .await?,
            email_generation: self
                .get_model_setting("openai_model_email_generation", "gpt-5-mini")
                .await?,
            message_responses: self
                .get_model_setting("openai_model_message_responses", "gpt-5-mini")
                .await?,
            job_description_generation: self
                .get_model_setting("openai_model_job_description", "gpt-5")
                .await?,
            image_generation: self
                .get_model_setting("openai_model_image_generation", "gpt-image-1")
                .await?,
        };

        // Get reasoning effort configurations (with defaults)
        let reasoning_effort = ReasoningEffortConfig {
            resume_scanning: self
                .get_reasoning_effort_setting("openai_reasoning_effort_resume_scanning", "medium")
                .await?,
            email_generation: self
                .get_reasoning_effort_setting("openai_reasoning_effort_email_generation", "low")
                .await?,
            message_responses: self
                .get_reasoning_effort_setting("openai_reasoning_effort_message_responses", "low")
                .await?,
            job_description_generation: self
                .get_reasoning_effort_setting("openai_reasoning_effort_job_description", "medium")
                .await?,
        };

        Ok(OpenAIConfig {
            api_key,
            base_url,
            models,
            reasoning_effort,
        })
    }

    async fn get_model_setting(&self, key: &str, default: &str) -> Result<String, OpenAIError> {
        Ok(self
            .settings_service
            .get_setting(key)
            .await
            .map_err(|e| OpenAIError::SettingsError(e.to_string()))?
            .unwrap_or_else(|| default.to_string()))
    }

    async fn get_reasoning_effort_setting(
        &self,
        key: &str,
        default: &str,
    ) -> Result<String, OpenAIError> {
        Ok(self
            .settings_service
            .get_setting(key)
            .await
            .map_err(|e| OpenAIError::SettingsError(e.to_string()))?
            .unwrap_or_else(|| default.to_string()))
    }

    /// Generate text using OpenAI API
    pub async fn generate_text(
        &self,
        purpose: TextGenerationPurpose,
        prompt: &str,
        context: Option<serde_json::Value>,
    ) -> Result<String, OpenAIError> {
        let config = self.get_config().await?;

        // Select model and reasoning effort based on purpose
        let (model, reasoning_effort) = match purpose {
            TextGenerationPurpose::ResumeScanning => (
                &config.models.resume_scanning,
                &config.reasoning_effort.resume_scanning,
            ),
            TextGenerationPurpose::EmailGeneration | TextGenerationPurpose::EmailComposition => (
                &config.models.email_generation,
                &config.reasoning_effort.email_generation,
            ),
            TextGenerationPurpose::MessageResponses => (
                &config.models.message_responses,
                &config.reasoning_effort.message_responses,
            ),
            TextGenerationPurpose::JobDescriptionGeneration
            | TextGenerationPurpose::JobDescription => (
                &config.models.job_description_generation,
                &config.reasoning_effort.job_description_generation,
            ),
        };

        // Build messages
        let mut messages = vec![ChatMessage {
            role: "system".to_string(),
            content: self.get_system_prompt(purpose),
        }];

        // Add context if provided
        if let Some(ctx) = context {
            let context_str = serde_json::to_string_pretty(&ctx)
                .map_err(|e| OpenAIError::SerializationError(e.to_string()))?;
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: format!("Context:\n{}\n\nTask:\n{}", context_str, prompt),
            });
        } else {
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            });
        }

        // Build request - GPT-5 uses different format (input + reasoning) vs GPT-4 (messages)
        let is_gpt5 =
            model.starts_with("gpt-5") || model.starts_with("o1") || model.starts_with("o3");

        let request = if is_gpt5 {
            // GPT-5 format (Responses API)
            let content: Vec<serde_json::Value> = messages
                .iter()
                .map(|msg| {
                    serde_json::json!({
                        "type": "input_text",
                        "text": msg.content
                    })
                })
                .collect();

            ChatCompletionRequest {
                model: model.clone(),
                messages: None,
                input: Some(vec![serde_json::json!({
                    "role": "user",
                    "content": content
                })]),
                temperature: None,
                max_tokens: None,
                max_output_tokens: Some(4000),
                reasoning: Some(serde_json::json!({"effort": reasoning_effort})),
                text: Some(serde_json::json!({"format": {"type": "text"}})),
            }
        } else {
            // GPT-4 format (Chat Completions API)
            ChatCompletionRequest {
                model: model.clone(),
                messages: Some(messages),
                input: None,
                temperature: Some(0.7),
                max_tokens: Some(2000),
                max_output_tokens: None,
                reasoning: None,
                text: None,
            }
        };

        debug!(
            purpose = ?purpose,
            model = %model,
            reasoning_effort = %reasoning_effort,
            "Sending OpenAI text generation request"
        );

        // Make API request with retry logic
        let response = self.make_request_with_retry(&config, request).await?;

        // Extract generated text - handle both GPT-4 (choices) and GPT-5 (output) formats
        let generated_text = if !response.output.is_empty() {
            // GPT-5 format - try multiple extraction paths
            let mut text_found: Option<String> = None;

            for output in &response.output {
                // Try content array
                if let Some(content_items) = output.content.first() {
                    if let Some(txt) = &content_items.text {
                        text_found = Some(txt.clone());
                        break;
                    }
                }
            }

            text_found.ok_or_else(|| {
                error!(
                    "Failed to extract text from GPT-5 response, output items: {}",
                    response.output.len()
                );
                OpenAIError::InvalidResponse("No text in output".to_string())
            })?
        } else {
            // GPT-4 format
            response
                .choices
                .first()
                .ok_or_else(|| OpenAIError::InvalidResponse("No choices in response".to_string()))?
                .message
                .content
                .clone()
        };

        if let Some(usage) = response.usage {
            info!(
                purpose = ?purpose,
                model = %model,
                tokens_used = usage.total_tokens,
                "OpenAI text generation completed"
            );
        }

        Ok(generated_text)
    }

    /// Make API request with retry logic
    async fn make_request_with_retry(
        &self,
        config: &OpenAIConfig,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, OpenAIError> {
        let max_retries = 3;
        let mut last_error = None;

        for attempt in 1..=max_retries {
            match self.make_request(config, &request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    warn!(
                        attempt = attempt,
                        max_retries = max_retries,
                        error = %e,
                        "OpenAI API request failed, retrying..."
                    );
                    last_error = Some(e);

                    // Exponential backoff
                    if attempt < max_retries {
                        let delay = std::time::Duration::from_millis(1000 * 2_u64.pow(attempt - 1));
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| OpenAIError::RequestFailed("Unknown error".to_string())))
    }

    /// Make a single API request
    async fn make_request(
        &self,
        config: &OpenAIConfig,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, OpenAIError> {
        // Use /v1/responses for GPT-5 models, /v1/chat/completions for others
        let endpoint = if request.model.starts_with("gpt-5")
            || request.model.starts_with("o1")
            || request.model.starts_with("o3")
        {
            "v1/responses"
        } else {
            "v1/chat/completions"
        };
        let url = format!("{}/{}", config.base_url.trim_end_matches('/'), endpoint);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| OpenAIError::RequestFailed(e.to_string()))?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(OpenAIError::RateLimitExceeded);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "OpenAI API request failed");
            return Err(OpenAIError::RequestFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        response
            .json::<ChatCompletionResponse>()
            .await
            .map_err(|e| OpenAIError::InvalidResponse(e.to_string()))
    }

    /// Get system prompt based on purpose
    fn get_system_prompt(&self, purpose: TextGenerationPurpose) -> String {
        match purpose {
            TextGenerationPurpose::ResumeScanning => {
                "You are an expert resume analyzer. Extract key information from resumes including skills, experience, education, and provide a quality score. Be thorough and accurate.".to_string()
            }
            TextGenerationPurpose::EmailGeneration | TextGenerationPurpose::EmailComposition => {
                "You are a professional email writer. Generate clear, professional, and personalized emails based on the provided context and template. Maintain a friendly yet professional tone.".to_string()
            }
            TextGenerationPurpose::MessageResponses => {
                "You are a helpful assistant responding to candidate inquiries. Provide clear, accurate, and empathetic responses. Be professional yet approachable.".to_string()
            }
            TextGenerationPurpose::JobDescriptionGeneration | TextGenerationPurpose::JobDescription => {
                "You are an expert job description writer. Create compelling, clear, and comprehensive job descriptions that attract qualified candidates. Include all necessary details while maintaining readability.".to_string()
            }
        }
    }

    /// Generate image using DALL-E API
    pub async fn generate_image(
        &self,
        prompt: &str,
        size: ImageSize,
        style: ImageStyle,
    ) -> Result<String, OpenAIError> {
        let config = self.get_config().await?;

        // Enhance prompt with style modifier
        let enhanced_prompt = format!("{}, {}", prompt, style.to_prompt_modifier());

        // Build request - note: gpt-image-1 doesn't support size, quality, or style parameters
        // It generates images based on the prompt alone
        let request = ImageGenerationRequest {
            model: config.models.image_generation.clone(),
            prompt: enhanced_prompt,
            n: None,               // Optional, defaults to 1
            size: None,            // Not supported by gpt-image-1
            quality: None,         // Not supported by gpt-image-1
            style: None,           // Not supported by gpt-image-1
            response_format: None, // Can be "url" or "b64_json", defaults to "url"
        };

        debug!(
            model = %config.models.image_generation,
            size = ?size,
            style = ?style,
            "Sending OpenAI image generation request"
        );

        // Make API request
        let url = format!(
            "{}/v1/images/generations",
            config.base_url.trim_end_matches('/')
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| OpenAIError::RequestFailed(e.to_string()))?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(OpenAIError::RateLimitExceeded);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "OpenAI image generation failed");
            return Err(OpenAIError::RequestFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let image_response = response
            .json::<ImageGenerationResponse>()
            .await
            .map_err(|e| OpenAIError::InvalidResponse(e.to_string()))?;

        // Extract image URL or base64 data
        let image_data = image_response
            .data
            .first()
            .ok_or_else(|| OpenAIError::InvalidResponse("No image data in response".to_string()))?;

        let result = if let Some(url) = &image_data.url {
            url.clone()
        } else if let Some(b64) = &image_data.b64_json {
            // Return base64 data with data URI prefix
            format!("data:image/png;base64,{}", b64)
        } else {
            return Err(OpenAIError::InvalidResponse(
                "No image URL or base64 data in response".to_string(),
            ));
        };

        info!(
            model = %config.models.image_generation,
            size = ?size,
            style = ?style,
            "OpenAI image generation completed"
        );

        Ok(result)
    }

    /// Generate social media post image for a job
    pub async fn generate_social_media_post(
        &self,
        job_title: &str,
        company_name: &str,
        location: Option<&str>,
        salary_range: Option<&str>,
        platform: SocialPlatform,
        style: ImageStyle,
    ) -> Result<String, OpenAIError> {
        // Build a detailed prompt for the social media post
        let mut prompt_parts = vec![
            format!(
                "Create a professional social media post image for {} advertising a job opening",
                platform.platform_name()
            ),
            format!("Job Title: {}", job_title),
            format!("Company: {}", company_name),
        ];

        if let Some(loc) = location {
            prompt_parts.push(format!("Location: {}", loc));
        }

        if let Some(salary) = salary_range {
            prompt_parts.push(format!("Salary: {}", salary));
        }

        prompt_parts.push("Include the job title prominently displayed".to_string());
        prompt_parts.push("Include the company name".to_string());
        prompt_parts.push("Use professional typography and layout".to_string());
        prompt_parts.push("Make it eye-catching and suitable for recruitment".to_string());
        prompt_parts.push("Include visual elements that represent the job or industry".to_string());

        // Add platform-specific guidance
        match platform {
            SocialPlatform::InstagramSquare => {
                prompt_parts.push("Square format optimized for Instagram feed".to_string());
                prompt_parts.push("Use bold, readable text that works on mobile".to_string());
            }
            SocialPlatform::InstagramStory => {
                prompt_parts.push("Vertical format optimized for Instagram Stories".to_string());
                prompt_parts.push("Use large text that's readable on mobile screens".to_string());
            }
            SocialPlatform::LinkedIn => {
                prompt_parts.push("Professional format for LinkedIn".to_string());
                prompt_parts.push("Corporate and business-appropriate design".to_string());
            }
            SocialPlatform::Twitter => {
                prompt_parts.push("Horizontal format for Twitter".to_string());
                prompt_parts.push("Concise and attention-grabbing design".to_string());
            }
            SocialPlatform::Facebook => {
                prompt_parts.push("Horizontal format for Facebook".to_string());
                prompt_parts.push("Engaging and shareable design".to_string());
            }
        }

        let full_prompt = prompt_parts.join(". ");

        info!(
            job_title = %job_title,
            company = %company_name,
            platform = ?platform,
            style = ?style,
            "Generating social media post image"
        );

        // Generate the image using the existing generate_image method
        // Note: We're using ImageSize::Custom to pass the platform dimensions
        let (width, height) = platform.to_dimensions();
        self.generate_image(&full_prompt, ImageSize::Custom { width, height }, style)
            .await
    }

    /// Test OpenAI connection
    pub async fn test_connection(&self) -> Result<String, OpenAIError> {
        let config = self.get_config().await?;

        let request = ChatCompletionRequest {
            model: "gpt-5-mini".to_string(),
            messages: None,
            input: Some(vec![serde_json::json!({
                "role": "user",
                "content": [{"type": "input_text", "text": "Say 'Connection successful' if you can read this."}]
            })]),
            temperature: None,
            max_tokens: None,
            max_output_tokens: Some(10),
            reasoning: Some(serde_json::json!({"effort": "minimal"})),
            text: Some(serde_json::json!({"format": {"type": "text"}})),
        };

        let response = self.make_request(&config, &request).await?;

        Ok(response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_else(|| "Connection successful".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model_config() {
        let config = ModelConfig::default();
        assert_eq!(config.resume_scanning, "gpt-5-mini");
        assert_eq!(config.email_generation, "gpt-5-mini");
        assert_eq!(config.message_responses, "gpt-5-mini");
        assert_eq!(config.job_description_generation, "gpt-5");
        assert_eq!(config.image_generation, "gpt-image-1");
    }

    #[test]
    fn test_default_reasoning_effort_config() {
        let config = ReasoningEffortConfig::default();
        assert_eq!(config.resume_scanning, "medium");
        assert_eq!(config.email_generation, "low");
        assert_eq!(config.message_responses, "low");
        assert_eq!(config.job_description_generation, "medium");
    }
}
