use axum::{
    extract::{Multipart, DefaultBodyLimit, State, Path},
    http::{header, StatusCode},
    response::{IntoResponse, Response, Html, Json},
    routing::{get, post, delete},
    Router,
};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{info, error};
use tempfile::TempDir;
use uuid::Uuid;
use xxhash_rust::xxh64::xxh64;
use std::fs;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Serialize)]
struct ValidationResult {
    valid: bool,
    errors: Vec<ValidationError>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct ValidationError {
    line: Option<u32>,
    column: Option<u32>,
    message: String,
    severity: String,
}

#[derive(Serialize)]
struct PackageInfo {
    name: String,
    description: String,
    category: String,
}

#[derive(Serialize)]
struct PackagesResponse {
    count: usize,
    packages: Vec<PackageInfo>,
}

// ============================================================================
// Compilation Cache System (24h TTL)
// Caches compiled PDFs by xxHash64 of input files to avoid re-compilation
// ============================================================================

const CACHE_TTL_SECS: u64 = 24 * 60 * 60; // 24 hours
const CACHE_CLEANUP_INTERVAL_SECS: u64 = 60 * 60; // 1 hour

#[derive(Clone, Serialize, Deserialize)]
struct CacheEntry {
    hash: u64,
    filename: String,
    created_at: u64, // Unix timestamp
    compile_time_ms: u64, // Original compilation time
}

#[derive(Clone)]
struct CompilationCache {
    enabled: bool,
    cache_dir: PathBuf,
    entries: Arc<RwLock<HashMap<u64, CacheEntry>>>,
}

impl CompilationCache {
    fn new(enabled: bool) -> Self {
        let cache_dir = PathBuf::from("/tmp/tachyon-pdf-cache");
        if enabled {
            fs::create_dir_all(&cache_dir).ok();
        }
        Self {
            enabled,
            cache_dir,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Compute xxHash64 of all input data (for cache key)
    fn hash_input(data: &[u8]) -> u64 {
        xxh64(data, 0)
    }

    /// Check if compiled PDF exists in cache and is not expired
    async fn get_pdf(&self, hash: u64) -> Option<(Vec<u8>, u64)> {
        if !self.enabled {
            return None;
        }

        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(&hash) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            if now - entry.created_at < CACHE_TTL_SECS {
                let path = self.cache_dir.join(&entry.filename);
                if let Ok(data) = fs::read(&path) {
                    info!("⚡ Cache HIT! Returning cached PDF (hash {:016x})", hash);
                    return Some((data, entry.compile_time_ms));
                }
            }
        }
        None
    }

    /// Store compiled PDF in cache
    async fn put_pdf(&self, hash: u64, pdf_data: &[u8], compile_time_ms: u64) {
        if !self.enabled {
            return;
        }

        let cache_filename = format!("{:016x}.pdf", hash);
        let path = self.cache_dir.join(&cache_filename);
        
        if fs::write(&path, pdf_data).is_ok() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            let entry = CacheEntry {
                hash,
                filename: cache_filename,
                created_at: now,
                compile_time_ms,
            };
            
            let mut entries = self.entries.write().await;
            entries.insert(hash, entry);
            info!("💾 Cache STORE: PDF cached (hash {:016x}, {}KB)", hash, pdf_data.len() / 1024);
        }
    }

    /// Remove expired entries (called by cleanup task)
    async fn cleanup_expired(&self) -> usize {
        if !self.enabled {
            return 0;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let mut entries = self.entries.write().await;
        let expired: Vec<u64> = entries
            .iter()
            .filter(|(_, e)| now - e.created_at >= CACHE_TTL_SECS)
            .map(|(k, _)| *k)
            .collect();
        
        let count = expired.len();
        for hash in expired {
            if let Some(entry) = entries.remove(&hash) {
                let path = self.cache_dir.join(&entry.filename);
                fs::remove_file(&path).ok();
            }
        }
        count
    }

    /// Get cache statistics
    async fn stats(&self) -> (usize, u64) {
        let entries = self.entries.read().await;
        let count = entries.len();
        let total_size: u64 = entries
            .values()
            .filter_map(|e| fs::metadata(self.cache_dir.join(&e.filename)).ok())
            .map(|m| m.len())
            .sum();
        (count, total_size)
    }
}

/// Background task to clean up expired cache entries every hour
async fn cache_cleanup_task(cache: CompilationCache) {
    loop {
        tokio::time::sleep(Duration::from_secs(CACHE_CLEANUP_INTERVAL_SECS)).await;
        let removed = cache.cleanup_expired().await;
        if removed > 0 {
            info!("🧹 Cache cleanup: removed {} expired entries", removed);
        }
        let (count, size) = cache.stats().await;
        if count > 0 {
            info!("📊 Cache stats: {} PDFs cached, {:.2} MB total", count, size as f64 / 1024.0 / 1024.0);
        }
    }
}

// ============================================================================
// Webhook System
// ============================================================================

#[derive(Clone, Serialize, Deserialize)]
struct WebhookSubscription {
    id: String,
    url: String,
    events: Vec<String>,
    created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
}

#[derive(Deserialize)]
struct CreateWebhookRequest {
    url: String,
    events: Vec<String>,
    #[serde(default)]
    secret: Option<String>,
}

#[derive(Serialize)]
struct CreateWebhookResponse {
    id: String,
    url: String,
    events: Vec<String>,
    created_at: u64,
}

#[derive(Serialize)]
struct WebhooksListResponse {
    count: usize,
    webhooks: Vec<WebhookSubscription>,
}

#[derive(Clone, Serialize)]
struct WebhookPayload {
    event: String,
    timestamp: u64,
    compile_time_ms: u64,
    files_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pdf_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    cache_status: String,
}

/// Fire webhooks asynchronously (non-blocking)
async fn fire_webhooks(
    webhooks: Arc<RwLock<Vec<WebhookSubscription>>>,
    event: String,
    compile_time_ms: u64,
    files_count: usize,
    pdf_data: Option<Vec<u8>>,
    error_msg: Option<String>,
    cache_status: String,
) {
    let subs = webhooks.read().await;
    let matching: Vec<_> = subs
        .iter()
        .filter(|w| w.events.contains(&event) || w.events.contains(&"*".to_string()))
        .cloned()
        .collect();
    drop(subs);

    if matching.is_empty() {
        return;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let pdf_base64 = pdf_data.map(|d| general_purpose::STANDARD.encode(&d));

    let payload = WebhookPayload {
        event: event.clone(),
        timestamp,
        compile_time_ms,
        files_count,
        pdf_base64,
        error: error_msg,
        cache_status,
    };

    let client = reqwest::Client::new();

    for webhook in matching {
        let client = client.clone();
        let payload = payload.clone();
        let url = webhook.url.clone();
        
        tokio::spawn(async move {
            match client
                .post(&url)
                .header("Content-Type", "application/json")
                .header("X-Tachyon-Event", &payload.event)
                .json(&payload)
                .timeout(Duration::from_secs(10))
                .send()
                .await
            {
                Ok(res) => {
                    info!("🔔 Webhook delivered to {} - Status: {}", url, res.status());
                }
                Err(e) => {
                    error!("⚠️ Webhook delivery failed to {}: {}", url, e);
                }
            }
        });
    }
}

// ============================================================================
// Format Cache System (HMR v2 - Preamble Snapshotting)
// Tracks preamble hashes to detect warm compilations
// ============================================================================

use std::collections::HashSet;

#[derive(Clone)]
struct FormatCache {
    /// Track preambles we've seen (and thus Tectonic has cached)
    seen_preambles: Arc<RwLock<HashSet<u64>>>,
}

impl FormatCache {
    fn new() -> Self {
        Self {
            seen_preambles: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Extract preamble from LaTeX content (everything before \begin{document})
    fn extract_preamble(content: &str) -> Option<&str> {
        content.find("\\begin{document}").map(|pos| &content[..pos])
    }

    /// Hash the preamble to create a unique format identifier
    fn hash_preamble(preamble: &str) -> u64 {
        xxh64(preamble.as_bytes(), 0)
    }

    /// Check if we've seen this preamble before (meaning Tectonic has it cached)
    async fn check_and_mark(&self, preamble_hash: u64) -> bool {
        let mut seen = self.seen_preambles.write().await;
        if seen.contains(&preamble_hash) {
            true // HIT - we've compiled with this preamble before
        } else {
            seen.insert(preamble_hash);
            false // MISS - first time seeing this preamble
        }
    }
}

// App state shared across handlers
#[derive(Clone)]
struct AppState {
    compilation_cache: CompilationCache,
    webhooks: Arc<RwLock<Vec<WebhookSubscription>>>,
    format_cache: FormatCache,
}

// ============================================================================
// Handlers
// ============================================================================

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../public/index.html"))
}

/// GET /packages - List available LaTeX packages
async fn packages_handler() -> Json<PackagesResponse> {
    // Common packages available in Tectonic
    let packages = vec![
        PackageInfo { name: "amsmath".into(), description: "AMS mathematical facilities".into(), category: "math".into() },
        PackageInfo { name: "amssymb".into(), description: "AMS symbols".into(), category: "math".into() },
        PackageInfo { name: "amsthm".into(), description: "AMS theorem environments".into(), category: "math".into() },
        PackageInfo { name: "graphicx".into(), description: "Enhanced graphics support".into(), category: "graphics".into() },
        PackageInfo { name: "tikz".into(), description: "Create graphics programmatically".into(), category: "graphics".into() },
        PackageInfo { name: "pgfplots".into(), description: "Create plots".into(), category: "graphics".into() },
        PackageInfo { name: "hyperref".into(), description: "Hyperlinks and bookmarks".into(), category: "document".into() },
        PackageInfo { name: "geometry".into(), description: "Page layout".into(), category: "document".into() },
        PackageInfo { name: "fancyhdr".into(), description: "Custom headers and footers".into(), category: "document".into() },
        PackageInfo { name: "booktabs".into(), description: "Professional tables".into(), category: "tables".into() },
        PackageInfo { name: "tabularx".into(), description: "Flexible tables".into(), category: "tables".into() },
        PackageInfo { name: "longtable".into(), description: "Multi-page tables".into(), category: "tables".into() },
        PackageInfo { name: "xcolor".into(), description: "Color support".into(), category: "formatting".into() },
        PackageInfo { name: "listings".into(), description: "Source code formatting".into(), category: "formatting".into() },
        PackageInfo { name: "minted".into(), description: "Syntax highlighting (requires pygments)".into(), category: "formatting".into() },
        PackageInfo { name: "algorithm2e".into(), description: "Algorithm typesetting".into(), category: "formatting".into() },
        PackageInfo { name: "biblatex".into(), description: "Bibliography management".into(), category: "bibliography".into() },
        PackageInfo { name: "natbib".into(), description: "Natural citation styles".into(), category: "bibliography".into() },
        PackageInfo { name: "fontspec".into(), description: "Font selection (XeLaTeX/LuaLaTeX)".into(), category: "fonts".into() },
        PackageInfo { name: "unicode-math".into(), description: "Unicode math fonts".into(), category: "fonts".into() },
        PackageInfo { name: "inputenc".into(), description: "Input encoding".into(), category: "encoding".into() },
        PackageInfo { name: "babel".into(), description: "Multilingual support".into(), category: "language".into() },
        PackageInfo { name: "polyglossia".into(), description: "Multilingual (XeLaTeX)".into(), category: "language".into() },
        PackageInfo { name: "csquotes".into(), description: "Context-sensitive quotes".into(), category: "language".into() },
        PackageInfo { name: "siunitx".into(), description: "SI units formatting".into(), category: "science".into() },
        PackageInfo { name: "chemfig".into(), description: "Chemical structures".into(), category: "science".into() },
        PackageInfo { name: "circuitikz".into(), description: "Electrical circuits".into(), category: "science".into() },
        PackageInfo { name: "float".into(), description: "Float placement control".into(), category: "floats".into() },
        PackageInfo { name: "subcaption".into(), description: "Sub-figures and sub-tables".into(), category: "floats".into() },
        PackageInfo { name: "caption".into(), description: "Caption customization".into(), category: "floats".into() },
        PackageInfo { name: "enumitem".into(), description: "List customization".into(), category: "lists".into() },
        PackageInfo { name: "tcolorbox".into(), description: "Colored boxes".into(), category: "boxes".into() },
        PackageInfo { name: "mdframed".into(), description: "Framed environments".into(), category: "boxes".into() },
        PackageInfo { name: "microtype".into(), description: "Micro-typography".into(), category: "typography".into() },
        PackageInfo { name: "setspace".into(), description: "Line spacing".into(), category: "typography".into() },
        PackageInfo { name: "titlesec".into(), description: "Section title formatting".into(), category: "typography".into() },
        PackageInfo { name: "parskip".into(), description: "Paragraph spacing".into(), category: "typography".into() },
    ];
    
    Json(PackagesResponse {
        count: packages.len(),
        packages,
    })
}

// ============================================================================
// Webhook Handlers
// ============================================================================

/// POST /webhooks - Register a new webhook
async fn create_webhook_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateWebhookRequest>,
) -> impl IntoResponse {
    // Validate URL format
    if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Invalid URL. Must start with http:// or https://"
        }))).into_response();
    }

    // Validate events
    let valid_events = ["compile.success", "compile.error", "*"];
    for event in &req.events {
        if !valid_events.contains(&event.as_str()) {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": format!("Invalid event: {}. Valid events: compile.success, compile.error, *", event)
            }))).into_response();
        }
    }

    let webhook = WebhookSubscription {
        id: Uuid::new_v4().to_string(),
        url: req.url.clone(),
        events: req.events.clone(),
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        secret: req.secret,
    };

    let response = CreateWebhookResponse {
        id: webhook.id.clone(),
        url: webhook.url.clone(),
        events: webhook.events.clone(),
        created_at: webhook.created_at,
    };

    state.webhooks.write().await.push(webhook);
    info!("\u{1F514} Webhook registered: {} -> {}", response.id, response.url);

    (StatusCode::CREATED, Json(response)).into_response()
}

/// GET /webhooks - List all registered webhooks
async fn list_webhooks_handler(
    State(state): State<AppState>,
) -> Json<WebhooksListResponse> {
    let webhooks = state.webhooks.read().await;
    Json(WebhooksListResponse {
        count: webhooks.len(),
        webhooks: webhooks.clone(),
    })
}

/// DELETE /webhooks/:id - Remove a webhook
async fn delete_webhook_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut webhooks = state.webhooks.write().await;
    let original_len = webhooks.len();
    webhooks.retain(|w| w.id != id);
    
    if webhooks.len() < original_len {
        info!("\u{1F5D1}\u{FE0F} Webhook deleted: {}", id);
        (StatusCode::OK, Json(serde_json::json!({"deleted": true, "id": id})))
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Webhook not found"})))
    }
}

/// POST /validate - Validate LaTeX syntax without compiling
async fn validate_handler(mut multipart: Multipart) -> impl IntoResponse {
    let mut tex_content = String::new();
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let _name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().unwrap_or("").to_string();
        let data = field.bytes().await.unwrap_or_default().to_vec();
        
        if filename.ends_with(".tex") && tex_content.is_empty() {
            tex_content = String::from_utf8_lossy(&data).to_string();
        }
        if !filename.is_empty() {
            files.insert(filename, data);
        }
    }

    if tex_content.is_empty() {
        return Json(ValidationResult {
            valid: false,
            errors: vec![ValidationError {
                line: None,
                column: None,
                message: "No .tex file provided".into(),
                severity: "error".into(),
            }],
            warnings: vec![],
        });
    }

    // Perform syntax validation
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let lines: Vec<&str> = tex_content.lines().collect();

    // Check for basic structure
    let has_documentclass = tex_content.contains("\\documentclass");
    let has_begin_doc = tex_content.contains("\\begin{document}");
    let has_end_doc = tex_content.contains("\\end{document}");

    if !has_documentclass {
        errors.push(ValidationError {
            line: Some(1),
            column: None,
            message: "Missing \\documentclass declaration".into(),
            severity: "error".into(),
        });
    }

    if !has_begin_doc {
        errors.push(ValidationError {
            line: None,
            column: None,
            message: "Missing \\begin{document}".into(),
            severity: "error".into(),
        });
    }

    if !has_end_doc {
        errors.push(ValidationError {
            line: Some(lines.len() as u32),
            column: None,
            message: "Missing \\end{document}".into(),
            severity: "error".into(),
        });
    }

    // Check for unbalanced braces
    let mut brace_count = 0i32;
    for (line_num, line) in lines.iter().enumerate() {
        // Skip comments
        let content = line.split('%').next().unwrap_or("");
        for ch in content.chars() {
            match ch {
                '{' => brace_count += 1,
                '}' => brace_count -= 1,
                _ => {}
            }
        }
        if brace_count < 0 {
            errors.push(ValidationError {
                line: Some((line_num + 1) as u32),
                column: None,
                message: "Unmatched closing brace '}'".into(),
                severity: "error".into(),
            });
            brace_count = 0;
        }
    }

    if brace_count > 0 {
        warnings.push(format!("{} unclosed brace(s) '{{' in document", brace_count));
    }

    // Check for common issues
    for (line_num, line) in lines.iter().enumerate() {
        // Check for $$ (should use \[ \] instead)
        if line.contains("$$") {
            warnings.push(format!(
                "Line {}: Consider using \\[ \\] instead of $$ for display math",
                line_num + 1
            ));
        }
        
        // Check for \it, \bf (deprecated)
        if line.contains("\\it ") || line.contains("\\it}") {
            warnings.push(format!(
                "Line {}: \\it is deprecated, use \\textit{{}} instead",
                line_num + 1
            ));
        }
        if line.contains("\\bf ") || line.contains("\\bf}") {
            warnings.push(format!(
                "Line {}: \\bf is deprecated, use \\textbf{{}} instead",
                line_num + 1
            ));
        }
    }

    // Check for unbalanced environments
    let env_regex = regex::Regex::new(r"\\(begin|end)\{(\w+)\}").unwrap();
    let mut env_stack: Vec<(String, usize)> = Vec::new();
    
    for (line_num, line) in lines.iter().enumerate() {
        for cap in env_regex.captures_iter(line) {
            let cmd = &cap[1];
            let env_name = &cap[2];
            
            if cmd == "begin" {
                env_stack.push((env_name.to_string(), line_num + 1));
            } else if cmd == "end" {
                if let Some((last_env, _)) = env_stack.pop() {
                    if last_env != env_name {
                        errors.push(ValidationError {
                            line: Some((line_num + 1) as u32),
                            column: None,
                            message: format!(
                                "Environment mismatch: expected \\end{{{}}}, found \\end{{{}}}",
                                last_env, env_name
                            ),
                            severity: "error".into(),
                        });
                    }
                } else {
                    errors.push(ValidationError {
                        line: Some((line_num + 1) as u32),
                        column: None,
                        message: format!("\\end{{{}}} without matching \\begin", env_name),
                        severity: "error".into(),
                    });
                }
            }
        }
    }

    for (env_name, line_num) in env_stack {
        if env_name != "document" || has_end_doc {
            errors.push(ValidationError {
                line: Some(line_num as u32),
                column: None,
                message: format!("Unclosed environment: {}", env_name),
                severity: "error".into(),
            });
        }
    }

    Json(ValidationResult {
        valid: errors.is_empty(),
        errors,
        warnings,
    })
}

/// POST /compile - Compile LaTeX to PDF (supports ZIP or multiple files)
/// Now with PDF caching: if the same input is compiled twice, returns cached result
async fn compile_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Collect all input data for hashing
    let mut all_input_data: Vec<u8> = Vec::new();
    let mut files_data: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let filename = field.file_name().unwrap_or("").to_string();
        let data = field.bytes().await.unwrap_or_default().to_vec();
        
        if data.is_empty() {
            continue;
        }
        
        // Add to hash input: filename + data
        all_input_data.extend(filename.as_bytes());
        all_input_data.extend(&data);
        files_data.push((filename, data));
    }

    if files_data.is_empty() {
        return (StatusCode::BAD_REQUEST, "No files provided. Send a ZIP or multiple files via multipart/form-data").into_response();
    }

    // Calculate hash of all input data
    let input_hash = CompilationCache::hash_input(&all_input_data);

    // Check cache first
    if let Some((cached_pdf, original_compile_time)) = state.compilation_cache.get_pdf(input_hash).await {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/pdf")
            .header("X-Compile-Time-Ms", "0")
            .header("X-Original-Compile-Time-Ms", original_compile_time.to_string())
            .header("X-Cache", "HIT")
            .header("X-Files-Received", files_data.len().to_string())
            .body(axum::body::Body::from(cached_pdf))
            .unwrap();
    }

    // Cache miss - need to compile
    let temp_dir = match TempDir::new() {
        Ok(d) => d,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp dir: {}", e)).into_response(),
    };

    let mut files_received = 0;

    for (filename, data) in files_data {
        // Check if it's a ZIP file
        if filename.ends_with(".zip") || (data.len() > 4 && &data[0..4] == b"PK\x03\x04") {
            let reader = Cursor::new(data);
            let mut archive = match zip::ZipArchive::new(reader) {
                Ok(a) => a,
                Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid ZIP: {}", e)).into_response(),
            };

            for i in 0..archive.len() {
                let mut file = archive.by_index(i).unwrap();
                let name = file.name().to_string();
                
                if file.is_dir() {
                    continue;
                }

                let out_path = temp_dir.path().join(&name);
                
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).ok();
                }

                let mut content = Vec::new();
                std::io::copy(&mut file, &mut content).unwrap();
                fs::write(&out_path, &content).unwrap();
                files_received += 1;
            }
        } else if !filename.is_empty() {
            // Regular file upload
            let out_path = temp_dir.path().join(&filename);
            
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            
            fs::write(&out_path, &data).unwrap();
            files_received += 1;
        }
    }

    if files_received == 0 {
        return (StatusCode::BAD_REQUEST, "No files provided. Send a ZIP or multiple files via multipart/form-data").into_response();
    }

    // Robust main file detection
    let mut main_file_path: Option<PathBuf> = None;
    let mut tex_files = Vec::new();

    fn find_tex_files(dir: &std::path::Path, tex_files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    find_tex_files(&path, tex_files);
                } else if path.extension().and_then(|s| s.to_str()) == Some("tex") {
                    tex_files.push(path);
                }
            }
        }
    }
    find_tex_files(temp_dir.path(), &mut tex_files);

    // Heuristic 1: Look for main.tex exactly
    for path in &tex_files {
        if path.file_name().and_then(|s| s.to_str()) == Some("main.tex") {
            main_file_path = Some(path.clone());
            break;
        }
    }

    // Heuristic 2: Look for \begin{document}
    if main_file_path.is_none() {
        for path in &tex_files {
            if let Ok(content) = fs::read_to_string(path) {
                if content.contains("\\begin{document}") {
                    main_file_path = Some(path.clone());
                    break;
                }
            }
        }
    }

    // Heuristic 3: Use the first .tex file
    if main_file_path.is_none() {
        main_file_path = tex_files.first().cloned();
    }

    let main_tex_path = match main_file_path {
        Some(p) => p,
        None => return (StatusCode::BAD_REQUEST, "No .tex file found").into_response(),
    };

    // HMR v2: Detect preamble and check format cache
    let mut hmr_status = "NONE";
    let mut preamble_hash: u64 = 0;
    
    if let Ok(tex_content) = fs::read_to_string(&main_tex_path) {
        if let Some(preamble) = FormatCache::extract_preamble(&tex_content) {
            preamble_hash = FormatCache::hash_preamble(preamble);
            let is_warm = state.format_cache.check_and_mark(preamble_hash).await;
            if is_warm {
                hmr_status = "HIT";
                info!("⚡ HMR HIT: Reusing cached format {:016x}", preamble_hash);
            } else {
                hmr_status = "MISS";
                info!("🔥 HMR MISS: First compile with preamble {:016x}", preamble_hash);
            }
        }
    }

    info!("Compiling {:?} ({} files received, HMR: {})...", main_tex_path, files_received, hmr_status);
    let start = std::time::Instant::now();

    // Use Tectonic CLI (it has internal format caching)
    let result = std::process::Command::new("tectonic")
        .arg("-X")
        .arg("compile")
        .arg(&main_tex_path)
        .arg("--outdir")
        .arg(temp_dir.path())
        .output();

    let duration = start.elapsed();
    let compile_time_ms = duration.as_millis() as u64;

    let (response, webhook_data): (Response<axum::body::Body>, Option<(bool, Option<Vec<u8>>, Option<String>, String)>) = match result {
        Ok(output) => {
            if output.status.success() {
                info!("Compiled in {:?} (HMR: {})", duration, hmr_status);
                
                let pdf_name = main_tex_path.file_stem().expect("Failed to get file stem").to_str().unwrap();
                let pdf_path = temp_dir.path().join(format!("{}.pdf", pdf_name));
                
                match fs::read(&pdf_path) {
                    Ok(pdf_data) => {
                        // Store in cache for future requests
                        state.compilation_cache.put_pdf(input_hash, &pdf_data, compile_time_ms).await;
                        
                        let response = Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "application/pdf")
                            .header("X-Compile-Time-Ms", compile_time_ms.to_string())
                            .header("X-Cache", "MISS")
                            .header("X-HMR", hmr_status)
                            .header("X-Preamble-Hash", format!("{:016x}", preamble_hash))
                            .header("X-Files-Received", files_received.to_string())
                            .body(axum::body::Body::from(pdf_data.clone()))
                            .unwrap();
                        
                        (response, Some((true, Some(pdf_data), None, "MISS".to_string())))
                    }
                    Err(_) => (
                        (StatusCode::INTERNAL_SERVER_ERROR, "PDF was not generated").into_response(),
                        Some((false, None, Some("PDF was not generated".to_string()), "MISS".to_string()))
                    ),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let error_msg = format!("LaTeX Error:\n{}\n{}", stderr, stdout);
                error!("Compilation failed: {} {}", stderr, stdout);
                (
                    (StatusCode::INTERNAL_SERVER_ERROR, error_msg.clone()).into_response(),
                    Some((false, None, Some(error_msg), "MISS".to_string()))
                )
            }
        }
        Err(_) => {
            // Fallback to latex_to_pdf for simple documents
            info!("Tectonic CLI not available, falling back to latex_to_pdf");
            let tex_content = match fs::read_to_string(&main_tex_path) {
                Ok(c) => c,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read tex: {}", e)).into_response(),
            };
            
            match tectonic::latex_to_pdf(&tex_content) {
                Ok(pdf_data) => {
                    let duration = start.elapsed();
                    let compile_time_ms = duration.as_millis() as u64;
                    info!("Compiled in {:?}", duration);
                    
                    // Store in cache
                    state.compilation_cache.put_pdf(input_hash, &pdf_data, compile_time_ms).await;
                    
                    let response = Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/pdf")
                        .header("X-Compile-Time-Ms", compile_time_ms.to_string())
                        .header("X-Cache", "MISS")
                        .body(axum::body::Body::from(pdf_data.clone()))
                        .unwrap();
                    
                    (response, Some((true, Some(pdf_data), None, "MISS".to_string())))
                }
                Err(e) => {
                    let error_msg = format!("LaTeX Error: {}", e);
                    error!("Compilation failed: {}", e);
                    (
                        (StatusCode::INTERNAL_SERVER_ERROR, error_msg.clone()).into_response(),
                        Some((false, None, Some(error_msg), "MISS".to_string()))
                    )
                }
            }
        }
    };

    // Explicitly drop the temp_dir to ensure it's deleted before sending the response
    let path = temp_dir.path().to_path_buf();
    drop(temp_dir);
    info!("\u{1F9F9} Cleaned up temporary directory: {:?}", path);

    // Clean up temp dir explicitly is already done by drop(temp_dir)


    // Fire webhooks asynchronously (non-blocking)
    if let Some((success, pdf_data, error_msg, cache_status)) = webhook_data {
        let event = if success { "compile.success".to_string() } else { "compile.error".to_string() };
        let webhooks = state.webhooks.clone();
        tokio::spawn(async move {
            fire_webhooks(
                webhooks,
                event,
                compile_time_ms,
                files_received,
                pdf_data,
                error_msg,
                cache_status,
            ).await;
        });
    }

    response
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let is_warmup = args.iter().any(|arg| arg == "--warmup");

    if is_warmup {
        info!("🔥 Moonshot Warmup: Pre-caching LaTeX packages...");
        let warmup_tex = include_str!("../warmup.tex");
        
        match tectonic::latex_to_pdf(warmup_tex) {
            Ok(_) => info!("✅ Warmup complete. Packages cached."),
            Err(e) => error!("Warmup failed: {}", e),
        }
        return;
    }

    // Initialize PDF compilation cache based on environment variable
    let cache_enabled = std::env::var("PDF_CACHE_ENABLED")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false);
    
    let compilation_cache = CompilationCache::new(cache_enabled);
    
    if cache_enabled {
        info!("📦 PDF cache ENABLED (TTL: 24h, cleanup: every 1h)");
        // Spawn background cleanup task
        let cache_clone = compilation_cache.clone();
        tokio::spawn(async move {
            cache_cleanup_task(cache_clone).await;
        });
    } else {
        info!("📦 PDF cache DISABLED (set PDF_CACHE_ENABLED=true to enable)");
    }

    // Initialize webhooks storage
    let webhooks: Arc<RwLock<Vec<WebhookSubscription>>> = Arc::new(RwLock::new(Vec::new()));

    // Initialize Format Cache for HMR v2
    let format_cache = FormatCache::new();
    info!("⚡ Format Cache initialized (in-memory preamble tracking)");

    let state = AppState { 
        compilation_cache,
        webhooks,
        format_cache,
    };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/compile", post(compile_handler))
        .route("/validate", post(validate_handler))
        .route("/packages", get(packages_handler))
        .route("/webhooks", post(create_webhook_handler))
        .route("/webhooks", get(list_webhooks_handler))
        .route("/webhooks/:id", delete(delete_webhook_handler))
        .with_state(state)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(CorsLayer::permissive());

    let addr = "0.0.0.0:8080";
    info!("🚀 Tachyon-Tex listening on {}", addr);
    info!("   Endpoints: POST /compile, POST /validate, GET /packages");
    info!("   Webhooks:  POST /webhooks, GET /webhooks, DELETE /webhooks/:id");
    info!("   HMR v2:    Preamble format caching enabled");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
