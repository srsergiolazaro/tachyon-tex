use axum::{
    extract::{Multipart, DefaultBodyLimit},
    http::{header, StatusCode},
    response::{IntoResponse, Response, Html, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use tower_http::cors::CorsLayer;
use tracing::{info, error};
use tempfile::TempDir;
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

/// POST /validate - Validate LaTeX syntax without compiling
async fn validate_handler(mut multipart: Multipart) -> impl IntoResponse {
    let mut tex_content = String::new();
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let name = field.name().unwrap_or("").to_string();
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
    let mut bracket_count = 0i32;
    for (line_num, line) in lines.iter().enumerate() {
        // Skip comments
        let content = line.split('%').next().unwrap_or("");
        for ch in content.chars() {
            match ch {
                '{' => brace_count += 1,
                '}' => brace_count -= 1,
                '[' => bracket_count += 1,
                ']' => bracket_count -= 1,
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
async fn compile_handler(mut multipart: Multipart) -> impl IntoResponse {
    let temp_dir = match TempDir::new() {
        Ok(d) => d,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp dir: {}", e)).into_response(),
    };

    let mut has_zip = false;
    let mut files_received = 0;

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let filename = field.file_name().unwrap_or("").to_string();
        let data = field.bytes().await.unwrap_or_default().to_vec();
        
        if data.is_empty() {
            continue;
        }

        // Check if it's a ZIP file
        if filename.ends_with(".zip") || (data.len() > 4 && &data[0..4] == b"PK\x03\x04") {
            has_zip = true;
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

    info!("Compiling {:?} ({} files received)...", main_tex_path, files_received);
    let start = std::time::Instant::now();

    // Use Tectonic CLI
    let result = std::process::Command::new("tectonic")
        .arg("-X")
        .arg("compile")
        .arg(&main_tex_path)
        .arg("--outdir")
        .arg(temp_dir.path())
        .output();

    let duration = start.elapsed();

    match result {
        Ok(output) => {
            if output.status.success() {
                info!("Compiled in {:?}", duration);
                
                let pdf_name = main_tex_path.file_stem().unwrap().to_str().unwrap();
                let pdf_path = temp_dir.path().join(format!("{}.pdf", pdf_name));
                
                match fs::read(&pdf_path) {
                    Ok(pdf_data) => {
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "application/pdf")
                            .header("X-Compile-Time-Ms", duration.as_millis().to_string())
                            .header("X-Files-Received", files_received.to_string())
                            .body(axum::body::Body::from(pdf_data))
                            .unwrap()
                    }
                    Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "PDF was not generated").into_response(),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                error!("Compilation failed: {} {}", stderr, stdout);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("LaTeX Error:\n{}\n{}", stderr, stdout)).into_response()
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
                    info!("Compiled in {:?}", duration);
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/pdf")
                        .header("X-Compile-Time-Ms", duration.as_millis().to_string())
                        .body(axum::body::Body::from(pdf_data))
                        .unwrap()
                }
                Err(e) => {
                    error!("Compilation failed: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("LaTeX Error: {}", e)).into_response()
                }
            }
        }
    }
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

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/compile", post(compile_handler))
        .route("/validate", post(validate_handler))
        .route("/packages", get(packages_handler))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(CorsLayer::permissive());

    let addr = "0.0.0.0:8080";
    info!("🚀 Tachyon-Tex listening on {}", addr);
    info!("   Endpoints: POST /compile, POST /validate, GET /packages");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
