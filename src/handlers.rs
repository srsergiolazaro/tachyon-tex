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

    while let Ok(Some(field)) = multipart.next_field().await {
        let file_name = field.file_name().unwrap_or("file.tex").to_string();
        if let Ok(data) = field.bytes().await {
            files_received += 1;
            let path = temp_dir.path().join(&file_name);
            if let Some(parent) = path.parent() { fs::create_dir_all(parent).ok(); }
            fs::write(&path, &data).ok();
            all_input_data.extend_from_slice(&data);
            if file_name.ends_with(".tex") {
                main_tex_data = data.to_vec();
                main_tex_path_relative = file_name.clone();
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
    ws.on_upgrade(move |socket| handle_socket(socket, state))
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
                    let full_error = format!("{}\n\nLogs:\n{}", e, logs);
                    let _ = socket.send(Message::Text(serde_json::json!({"type": "compile_error", "error": full_error}).to_string())).await;
                }
            }
        }
    }
}

// ============================================================================
// Status Backend
// ============================================================================

struct CapturingStatusBackend {
    logs: Vec<String>,
}

impl CapturingStatusBackend {
    fn new() -> Self {
        Self { logs: Vec::new() }
    }
    
    fn get_logs(&self) -> String {
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
