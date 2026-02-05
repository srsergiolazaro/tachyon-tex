use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use tower_http::cors::CorsLayer;
use tower_http::compression::CompressionLayer;  // Moonshot #3: Zstd compression
use tower_http::services::ServeDir;
use std::time::Duration;

mod models;
mod services;
mod handlers;
mod mcp;
pub mod compiler;
pub mod healer;

use crate::models::*;
use crate::services::*;
use crate::handlers::*;

const CACHE_CLEANUP_INTERVAL_SECS: u64 = 3600; // 1 hour

use clap::{Parser, Subcommand};
use crate::compiler::Compiler;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tachyon-tex")]
#[command(about = "Tachyon-Tex High-Performance LaTeX Engine")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Run in warmup mode (exit after caching resources)
    #[arg(long, global = true)]
    warmup: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the backend server (default)
    Serve,
    /// Compile a LaTeX file directly
    Compile {
        /// Input file path
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    // 1. Initialize Logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();

    if cli.warmup {
        info!("🌌 Tachyon-Tex starting in WARMUP mode...");
    } else {
        info!("🌌 Tachyon-Tex Orbital Engine initializing...");
    }

    // Initialize Tectonic Config once
    let config = tectonic::config::PersistentConfig::open(false).expect("Failed to open Tectonic config");
    let format_cache_path = config.format_cache_path().expect("Failed to get format cache path");
    info!("🏗️ Tectonic Engine Configured (FormatCache: {})", format_cache_path.display());

    if cli.warmup {
        info!("✨ Warmup complete. Tectonic resources cached. Exiting.");
        return;
    }

    match cli.command.unwrap_or(Commands::Serve) {
        Commands::Serve => {
             run_server(config, format_cache_path).await;
        }
        Commands::Compile { file } => {
            info!("📄 Compiling file: {:?}", file);
            let output_dir = std::env::current_dir().unwrap();
            let (result, logs) = Compiler::compile_file(
                &file,
                &output_dir,
                &format_cache_path,
                &config
            );
            
            match result {
                Ok(_) => info!("✅ Compilation successful!"),
                Err(e) => {
                    tracing::error!("❌ Compilation failed: {}", e);
                    println!("{}", logs);
                    std::process::exit(1);
                }
            }
        }
    }
}

async fn run_server(config: tectonic::config::PersistentConfig, format_cache_path: PathBuf) {
     // 2. Initialize State and Services
    let pdf_cache_enabled = std::env::var("PDF_CACHE_ENABLED").unwrap_or_else(|_| "true".to_string()) == "true";
    let compilation_cache = CompilationCache::new(pdf_cache_enabled);
    let webhooks = Arc::new(RwLock::new(Vec::<WebhookSubscription>::new()));
    let format_cache = FormatCache::new();
    let blob_store = BlobStore::new();

    let state = AppState { 
        compilation_cache: compilation_cache.clone(),
        webhooks: webhooks.clone(),
        format_cache,
        blob_store,
        config: Arc::new(config),
        format_cache_path,
    };

    // 3. Background Tasks
    tokio::spawn(cache_cleanup_task(compilation_cache));

    // 4. MCP Setup
    let ct = tokio_util::sync::CancellationToken::new();
    let mcp_state = state.clone();
    let mcp_service = rmcp::transport::streamable_http_server::StreamableHttpService::new(
        move || Ok(crate::mcp::TachyonMcpServer::new(mcp_state.clone())),
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default().into(),
        rmcp::transport::streamable_http_server::StreamableHttpServerConfig {
            cancellation_token: ct.child_token(),
            ..Default::default()
        },
    );

    // 5. Build API Router - Moonshot #3: Add compression for 70% smaller responses
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/compile", post(compile_handler))
        .route("/validate", post(validate_handler))
        .route("/ws", get(ws_route_handler))
        .nest_service("/mcp", mcp_service)
        .fallback_service(ServeDir::new("public"))  // Serve static files from /public
        .layer(CompressionLayer::new())  // Moonshot #3: ~70% smaller responses
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024)) // 100MB limit
        .with_state(state);

    // 5. Start Server
    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("🚀 Tachyon-Tex Server listening on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}

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
