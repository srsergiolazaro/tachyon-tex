use axum::{
    extract::{Multipart, DefaultBodyLimit},
    http::{header, StatusCode},
    response::{IntoResponse, Response, Html},
    routing::{get, post},
    Router,
};
use std::io::Cursor;
use std::path::PathBuf;
use tower_http::cors::CorsLayer;
use tracing::{info, error};
use tempfile::TempDir;
use std::fs;

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../public/index.html"))
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Check for --warmup flag
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
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(CorsLayer::permissive());

    let addr = "0.0.0.0:8080";
    info!("🚀 Tachyon-Tex (Moonshot Mode) listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn compile_handler(
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut zip_data = Vec::new();

    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() == Some("file") {
            zip_data = field.bytes().await.unwrap().to_vec();
        }
    }

    if zip_data.is_empty() {
        return (StatusCode::BAD_REQUEST, "No ZIP file provided").into_response();
    }

    // Create temp directory for extraction
    let temp_dir = match TempDir::new() {
        Ok(d) => d,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp dir: {}", e)).into_response(),
    };

    let reader = Cursor::new(zip_data);
    let mut archive = match zip::ZipArchive::new(reader) {
        Ok(a) => a,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid ZIP: {}", e)).into_response(),
    };

    let mut main_file_path: Option<PathBuf> = None;

    // Extract all files to temp directory
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let name = file.name().to_string();
        
        if file.is_dir() {
            continue;
        }

        let out_path = temp_dir.path().join(&name);
        
        // Create parent directories if needed
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let mut content = Vec::new();
        std::io::copy(&mut file, &mut content).unwrap();
        fs::write(&out_path, &content).unwrap();

        // Detect main file (prioritize main.tex)
        if name == "main.tex" || (name.ends_with(".tex") && main_file_path.is_none()) {
            main_file_path = Some(out_path);
        }
    }

    let main_tex_path = match main_file_path {
        Some(p) => p,
        None => return (StatusCode::BAD_REQUEST, "No .tex file found in ZIP").into_response(),
    };

    info!("Compiling {:?}...", main_tex_path);
    let start = std::time::Instant::now();

    // Use Tectonic CLI-style compilation with filesystem support
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
                
                // Find the PDF
                let pdf_name = main_tex_path.file_stem().unwrap().to_str().unwrap();
                let pdf_path = temp_dir.path().join(format!("{}.pdf", pdf_name));
                
                match fs::read(&pdf_path) {
                    Ok(pdf_data) => {
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "application/pdf")
                            .header("X-Compile-Time-Ms", duration.as_millis().to_string())
                            .body(axum::body::Body::from(pdf_data))
                            .unwrap()
                    }
                    Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "PDF was not generated").into_response(),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("Compilation failed: {}", stderr);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("LaTeX Error: {}", stderr)).into_response()
            }
        }
        Err(e) => {
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
