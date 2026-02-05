use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(untagged)]
pub enum WsFileContent {
    /// Plain text content (for .tex, .sty, .cls, .bib files)
    Raw(String),
    /// Explicit base64-encoded binary content (for images, fonts, etc.)
    Binary { base64: String },
    /// Reference to previously uploaded blob by hash
    HashRef { #[serde(rename = "type")] content_type: String, value: String },
}

#[derive(Deserialize)]
pub struct WsProject {
    pub main: Option<String>,
    pub files: HashMap<String, WsFileContent>,
}

#[derive(Deserialize, Debug)]
pub struct CompilationRequest {
    pub main_tex: String,
    pub webhook_url: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ValidationRequest {
    pub files: Vec<String>,
}

#[derive(Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationMessage>,
}

#[derive(Serialize)]
pub struct ValidationMessage {
    pub file: String,
    pub line: u32,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebhookSubscription {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
}

#[derive(Serialize)]
pub struct WebhookPayload {
    pub event: String,
    pub timestamp: u64,
    pub project_id: Option<String>,
    pub success: bool,
    pub compile_time_ms: u64,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct CompilationResponse {
    pub success: bool,
    pub compile_time_ms: u64,
    pub cache_hit: bool,
    pub pdf_base64: Option<String>,
    pub error: Option<String>,
}
