use axum::{
    extract::{State, Multipart, ws::{WebSocket, Message}},
    response::{IntoResponse, Response},
    Json,
    http::{StatusCode, header},
};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{info, error};
use tempfile::TempDir;
use base64::{Engine as _, engine::general_purpose};
use xxhash_rust::xxh64::xxh64;
use regex::Regex;

use tectonic::driver::{ProcessingSessionBuilder, OutputFormat, PassSetting};
use tectonic::status::{StatusBackend, MessageKind, NoopStatusBackend};

use crate::models::*;
use crate::services::*;

// ============================================================================
// Handlers
// ============================================================================

pub async fn health_handler() -> &'static str {
    "🚀 Tachyon-Tex Engine is Operational"
}

pub async fn validate_handler(Json(payload): Json<ValidationRequest>) -> Json<ValidationResult> {
    info!("Validating {} files...", payload.files.len());
    Json(ValidationResult {
        valid: true,
        errors: vec![],
    })
}

pub async fn compile_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let mut files_received = 0;
    let mut main_tex_data = Vec::new();
    let mut all_input_data = Vec::new();
    let mut main_tex_path_relative = String::from("main.tex");

    let temp_base = if std::path::Path::new("/dev/shm").exists() {
        let path = PathBuf::from("/dev/shm/tachyon-compilations");
        fs::create_dir_all(&path).ok();
        path
    } else {
        std::env::temp_dir()
    };

    let temp_dir = match TempDir::new_in(&temp_base) {
        Ok(d) => d,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp dir: {}", e)).into_response(),
    };

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                error!("Multipart error: {}", e);
                return (StatusCode::BAD_REQUEST, format!("Multipart error: {}", e)).into_response();
            }
        };

        let file_name = field.file_name().unwrap_or("file.tex").to_string();
        
        match field.bytes().await {
            Ok(data) => {
                files_received += 1;
                let path = temp_dir.path().join(&file_name);
                if let Some(parent) = path.parent() { 
                    if let Err(e) = fs::create_dir_all(parent) {
                        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create directory: {}", e)).into_response();
                    }
                }
                if let Err(e) = fs::write(&path, &data) {
                    return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write file {}: {}", file_name, e)).into_response();
                }
                all_input_data.extend_from_slice(&data);
                if file_name.ends_with(".tex") {
                    main_tex_data = data.to_vec();
                    main_tex_path_relative = file_name.clone();
                }
            },
            Err(e) => {
                error!("Failed to read chunks for file {}: {}", file_name, e);
                return (StatusCode::BAD_REQUEST, format!("Failed to read file {}: {}", file_name, e)).into_response();
            }
        }
    }

    let main_tex_path = temp_dir.path().join(&main_tex_path_relative);
    let input_hash = CompilationCache::hash_input(&all_input_data);

    if let Some((cached_pdf, original_time)) = state.compilation_cache.get_pdf(input_hash).await {
        info!("📦 Cache HIT for hash {:016x}", input_hash);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/pdf")
            .header("X-Compile-Time-Ms", original_time.to_string())
            .header("X-Cache", "HIT")
            .header("X-Files-Received", files_received.to_string())
            .body(axum::body::Body::from(cached_pdf))
            .unwrap();
    }

    let hmr_status;
    let preamble_hash;
    if let Ok(content) = String::from_utf8(main_tex_data) {
        if let Some(preamble) = FormatCache::extract_preamble(&content) {
            preamble_hash = FormatCache::hash_preamble(preamble);
            hmr_status = if state.format_cache.check_and_mark(preamble_hash).await { "HIT" } else { "MISS" };
        } else {
            hmr_status = "NONE"; preamble_hash = 0;
        }
    } else {
        hmr_status = "ERROR"; preamble_hash = 0;
    }

    info!("Compiling {:?} ({} files, HMR: {})...", main_tex_path, files_received, hmr_status);
    let start = Instant::now();

    let (result, logs) = {
        let mut status = CapturingStatusBackend::new();
        let bundle_res = state.config.default_bundle(false, &mut status);
        
        match bundle_res {
            Ok(bundle) => {
                let mut sb = ProcessingSessionBuilder::default();
                sb.bundle(bundle)
                    .primary_input_path(&main_tex_path)
                    .tex_input_name(&main_tex_path.file_name().unwrap_or_default().to_string_lossy())
                    .format_name("latex")
                    .format_cache_path(&state.format_cache_path)
                    .output_dir(temp_dir.path())
                    .print_stdout(false)
                    .output_format(OutputFormat::Pdf)
                    .pass(PassSetting::Default);

                let res = (|| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
                    let mut sess = sb.create(&mut status).map_err(|e| e.to_string())?;
                    sess.run(&mut status).map_err(|e| e.to_string())?;
                    let pdf_name = main_tex_path.file_stem().expect("stem").to_str().unwrap();
                    let pdf_path = temp_dir.path().join(format!("{}.pdf", pdf_name));
                    Ok(fs::read(&pdf_path)?)
                })().map_err(|e| e.to_string());
                
                (res, status.get_logs())
            },
            Err(e) => (Err(format!("Bundle error: {}", e)), status.get_logs())
        }
    };

    let compile_time_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(pdf_data) => {
            state.compilation_cache.put_pdf(input_hash, &pdf_data, compile_time_ms).await;
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/pdf")
                .header("X-Compile-Time-Ms", compile_time_ms.to_string())
                .header("X-Cache", "MISS")
                .header("X-HMR", hmr_status)
                .header("X-Files-Received", files_received.to_string())
                .body(axum::body::Body::from(pdf_data))
                .unwrap()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("LaTeX Error: {}\n\nLogs:\n{}", e, logs)).into_response()
    }
}

pub async fn ws_route_handler(
    ws: axum::extract::ws::WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws
        .max_frame_size(128 * 1024 * 1024)
        .max_message_size(128 * 1024 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

pub async fn handle_socket(mut socket: WebSocket, state: AppState) {
    info!("\u{1F50C} WebSocket connection established");
    
    while let Some(msg_res) = socket.recv().await {
        let msg = match msg_res {
            Ok(Message::Text(t)) => t,
            _ => continue,
        };

        if let Ok(project) = serde_json::from_str::<WsProject>(&msg) {
            info!("\u{1F4D1} Live Project Compile: {} files", project.files.len());
            
            let temp_base = if std::path::Path::new("/dev/shm").exists() {
                let path = PathBuf::from("/dev/shm/tachyon-compilations");
                fs::create_dir_all(&path).ok();
                path
            } else {
                std::env::temp_dir()
            };

            let temp_dir = match TempDir::new_in(&temp_base) {
                Ok(d) => d,
                Err(e) => {
                    let _ = socket.send(Message::Text(serde_json::json!({"type": "compile_error", "error": e.to_string()}).to_string())).await;
                    continue;
                }
            };

            let mut uploaded_hashes = std::collections::HashMap::new();

            for (name, content) in &project.files {
                let path = temp_dir.path().join(name);
                if let Some(parent) = path.parent() { fs::create_dir_all(parent).ok(); }
                match content {
                    WsFileContent::Raw(data) => {
                        if name.ends_with(".tex") || name.ends_with(".sty") || name.ends_with(".cls") {
                            let _ = fs::write(&path, data);
                        } else if let Ok(binary) = general_purpose::STANDARD.decode(data) {
                            let hash = xxh64(&binary, 0);
                            let hash_hex = format!("{:x}", hash);
                            state.blob_store.put(hash_hex.clone(), binary.clone()).await;
                            uploaded_hashes.insert(name.clone(), hash_hex); // Store hash to return to client
                            let _ = fs::write(&path, binary);
                        } else { let _ = fs::write(&path, data); }
                    },
                    WsFileContent::HashRef { value, .. } => {
                        if let Some(binary) = state.blob_store.get(value).await { let _ = fs::write(&path, binary); }
                    }
                }
            }

            let main_tex = project.main.clone().unwrap_or_else(|| "main.tex".to_string());
            let main_path = temp_dir.path().join(&main_tex);
            let start = Instant::now();

            let (result, logs) = {
                let mut status = CapturingStatusBackend::new();
                let bundle_res = state.config.default_bundle(false, &mut status);
                
                match bundle_res {
                    Ok(bundle) => {
                        let mut sb = ProcessingSessionBuilder::default();
                        sb.bundle(bundle)
                            .primary_input_path(&main_path)
                            .tex_input_name(&main_tex)
                            .format_name("latex")
                            .format_cache_path(&state.format_cache_path)
                            .output_dir(temp_dir.path())
                            .print_stdout(false)
                            .output_format(OutputFormat::Pdf)
                            .pass(PassSetting::Default);

                        let res = (|| -> Result<Vec<u8>, String> {
                            let mut sess = sb.create(&mut status).map_err(|e| e.to_string())?;
                            sess.run(&mut status).map_err(|e| e.to_string())?;
                            let pdf_name = main_path.file_stem().unwrap().to_str().unwrap();
                            let pdf_path = temp_dir.path().join(format!("{}.pdf", pdf_name));
                            fs::read(&pdf_path).map_err(|e| e.to_string())
                        })();
                        (res, status.get_logs())
                    },
                    Err(e) => (Err(format!("Bundle error: {}", e)), status.get_logs())
                }
            };

            match result {
                Ok(pdf_data) => {
                    let duration = start.elapsed().as_millis() as u64;
                    let _ = socket.send(Message::Text(serde_json::json!({
                        "type": "compile_success",
                        "compile_time_ms": duration,
                        "pdf": general_purpose::STANDARD.encode(&pdf_data),
                        "blobs": uploaded_hashes
                    }).to_string())).await;
                }
                Err(e) => {
                    error!("Compilation failed logs:\n{}", logs); // Log raw output for debugging
                    let parsed = parse_log_errors(&logs);
                    let response = serde_json::json!({
                        "type": "compile_error",
                        "error": e.to_string(),
                        "logs": logs,
                        "details": parsed
                    });
                    let _ = socket.send(Message::Text(response.to_string())).await;
                }
            }
        }
    }
}

// ============================================================================
// Status Backend
// ============================================================================

pub struct CapturingStatusBackend {
    logs: Vec<String>,
}

impl CapturingStatusBackend {
    pub fn new() -> Self {
        Self { logs: Vec::new() }
    }
    
    pub fn get_logs(&self) -> String {
        self.logs.join("\n")
    }
}

impl StatusBackend for CapturingStatusBackend {
    fn report(&mut self, kind: MessageKind, args: std::fmt::Arguments<'_>, err: Option<&anyhow::Error>) {
        let prefix = match kind {
            MessageKind::Note => "Note",
            MessageKind::Warning => "Warning",
            MessageKind::Error => "Error",
        };
        self.logs.push(format!("[{}] {}", prefix, args));
        if let Some(e) = err {
            self.logs.push(format!("Caused by: {}", e));
        }
    }

    fn dump_error_logs(&mut self, output: &[u8]) {
        if let Ok(s) = std::str::from_utf8(output) {
            self.logs.push(s.to_string());
        }
    }
}

fn parse_log_errors(log: &str) -> Vec<serde_json::Value> {
    let mut errors = Vec::new();
    // Match structure: [Error] filename.tex:9: Message...
    let direct_regex = Regex::new(r"^\[Error\] ([^:]+):(\d+): (.*)").unwrap();
    
    // Match standard TeX errors "! ..." AND Tectonic "error: ..."
    let error_regex = Regex::new(r"^(?:!|error:)(.*)").unwrap();
    let line_regex = Regex::new(r"^l\.(\d+)(.*)").unwrap();
    let file_regex = Regex::new(r"\(([^)\n]+\.(?:tex|sty|cls))").unwrap();
    
    let lines: Vec<&str> = log.lines().collect();
    
    for (i, line) in lines.iter().enumerate() {
        // 1. Try Direct Pattern (Best Quality)
        if let Some(caps) = direct_regex.captures(line) {
            let file = caps.get(1).unwrap().as_str().trim().to_string();
            let line_num: u32 = caps.get(2).unwrap().as_str().parse().unwrap_or(0);
            let message = caps.get(3).unwrap().as_str().trim().to_string();

            let mut error_obj = serde_json::Map::new();
            error_obj.insert("file".to_string(), serde_json::Value::String(file));
            error_obj.insert("line".to_string(), serde_json::Value::Number(serde_json::Number::from(line_num)));
            error_obj.insert("message".to_string(), serde_json::Value::String(message));
            
            errors.push(serde_json::Value::Object(error_obj));
            continue;
        }

        // 2. Try Standard TeX Pattern (Fallback)
        if let Some(caps) = error_regex.captures(line) {
            let message = caps.get(1).unwrap().as_str().trim().to_string();
            // Ignore generic "halted" messages which aren't specific errors
            if message.contains("halted on potentially-recoverable error") { continue; }

            let mut error_obj = serde_json::Map::new();
            error_obj.insert("message".to_string(), serde_json::Value::String(message));
            
            // Look ahead for line number (heuristic: next 10 lines)
            for j in i+1..std::cmp::min(i + 10, lines.len()) {
                if let Some(l_caps) = line_regex.captures(lines[j]) {
                    if let Ok(line_num) = l_caps.get(1).unwrap().as_str().parse::<u32>() {
                         error_obj.insert("line".to_string(), serde_json::Value::Number(serde_json::Number::from(line_num)));
                         let context = l_caps.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
                         error_obj.insert("context".to_string(), serde_json::Value::String(context));
                    }
                    break;
                }
            }
            
            // Look backwards for filename (heuristic: find last file opening pattern)
            let mut found_file = "unknown".to_string();
            for j in (0..i).rev() {
                if let Some(f_caps) = file_regex.captures(lines[j]) {
                    let mut possible_file = f_caps.get(1).unwrap().as_str().to_string();
                     if let Some(idx) = possible_file.find(' ') {
                        possible_file = possible_file[..idx].to_string();
                    }
                    found_file = possible_file;
                    break;
                }
            }
            error_obj.insert("file".to_string(), serde_json::Value::String(found_file));
            
            errors.push(serde_json::Value::Object(error_obj));
        }
    }
    
    errors
}
