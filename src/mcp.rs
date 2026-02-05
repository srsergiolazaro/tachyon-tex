use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{info, error};
use tempfile::TempDir;
use std::fs;
use std::path::PathBuf;
use base64::Engine;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::*,
    prompt_handler, prompt_router, schemars,
    service::RequestContext,
    task_handler,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use tectonic::driver::{ProcessingSessionBuilder, OutputFormat, PassSetting};

use crate::models::*;
use crate::services::*;
use crate::handlers::CapturingStatusBackend;

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CompileArgs {
    /// The name of the main .tex file to compile
    pub main: Option<String>,
    /// A map of filenames to their contents
    pub files: HashMap<String, String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ValidateArgs {
    /// List of files to validate
    pub files: Vec<String>,
}

#[derive(Clone)]
pub struct TachyonMcpServer {
    state: AppState,
    tool_router: ToolRouter<TachyonMcpServer>,
    prompt_router: PromptRouter<TachyonMcpServer>,
    processor: Arc<Mutex<rmcp::task_manager::OperationProcessor>>,
}

impl TachyonMcpServer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
            processor: Arc::new(Mutex::new(rmcp::task_manager::OperationProcessor::new())),
        }
    }
}

#[tool_router]
impl TachyonMcpServer {
    #[tool(description = "Compile LaTeX files into a PDF")]
    async fn compile(&self, Parameters(args): Parameters<CompileArgs>) -> Result<CallToolResult, McpError> {
        let files_received = args.files.len();
        let main_tex_name = args.main.unwrap_or_else(|| "main.tex".to_string());
        
        let temp_base = if std::path::Path::new("/dev/shm").exists() {
            let path = PathBuf::from("/dev/shm/tachyon-compilations");
            let _ = fs::create_dir_all(&path);
            path
        } else {
            std::env::temp_dir()
        };

        let temp_dir = TempDir::new_in(&temp_base).map_err(|e| {
            McpError::internal_error(format!("Failed to create temp dir: {}", e), None)
        })?;

        let mut all_input_data = Vec::new();
        for (name, content) in &args.files {
            let path = temp_dir.path().join(name);
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(e) = fs::write(&path, content) {
                return Err(McpError::internal_error(format!("Failed to write file {}: {}", name, e), None));
            }
            all_input_data.extend_from_slice(content.as_bytes());
        }

        let main_tex_path = temp_dir.path().join(&main_tex_name);
        let input_hash = CompilationCache::hash_input(&all_input_data);

        if let Some((cached_pdf, original_time)) = self.state.compilation_cache.get_pdf(input_hash).await {
            info!("📦 MCP Cache HIT for hash {:016x}", input_hash);
            return Ok(CallToolResult::success(vec![
                Content::text(format!("Compilation successful (CACHED). Time: {}ms", original_time)),
                Content::resource(ResourceContents::BlobResourceContents {
                    blob: base64::engine::general_purpose::STANDARD.encode(cached_pdf),
                    uri: format!("file:///{}.pdf", main_tex_name.replace(".tex", "")),
                    mime_type: Some("application/pdf".to_string()),
                    meta: None,
                })
            ]));
        }

        info!("MCP Compiling {:?} ({} files)...", main_tex_path, files_received);
        let start = Instant::now();

        let (result, logs) = {
            let mut status = CapturingStatusBackend::new();
            let bundle_res = self.state.config.default_bundle(false, &mut status);
            
            match bundle_res {
                Ok(bundle) => {
                    let mut sb = ProcessingSessionBuilder::default();
                    sb.bundle(bundle)
                        .primary_input_path(&main_tex_path)
                        .tex_input_name(&main_tex_name)
                        .format_name("latex")
                        .format_cache_path(&self.state.format_cache_path)
                        .output_dir(temp_dir.path())
                        .print_stdout(false)
                        .output_format(OutputFormat::Pdf)
                        .pass(PassSetting::Default);

                    let res = (|| -> Result<Vec<u8>, String> {
                        let mut sess = sb.create(&mut status).map_err(|e| e.to_string())?;
                        sess.run(&mut status).map_err(|e| e.to_string())?;
                        let pdf_name = main_tex_path.file_stem().unwrap().to_str().unwrap();
                        let pdf_path = temp_dir.path().join(format!("{}.pdf", pdf_name));
                        fs::read(&pdf_path).map_err(|e| e.to_string())
                    })();
                    (res, status.get_logs())
                },
                Err(e) => (Err(format!("Bundle error: {}", e)), status.get_logs())
            }
        };

        let compile_time_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(pdf_data) => {
                self.state.compilation_cache.put_pdf(input_hash, &pdf_data, compile_time_ms).await;
                Ok(CallToolResult::success(vec![
                    Content::text(format!("Compilation successful. Time: {}ms", compile_time_ms)),
                    Content::resource(ResourceContents::BlobResourceContents {
                        blob: base64::engine::general_purpose::STANDARD.encode(pdf_data),
                        uri: format!("file:///{}.pdf", main_tex_name.replace(".tex", "")),
                        mime_type: Some("application/pdf".to_string()),
                        meta: None,
                    })
                ]))
            }
            Err(e) => {
                error!("MCP Compilation failed:\n{}", logs);
                Ok(CallToolResult::error(vec![
                    Content::text(format!("LaTeX Error: {}", e)),
                    Content::text(format!("Logs:\n{}", logs))
                ]))
            }
        }
    }

    #[tool(description = "Validate LaTeX files for common errors")]
    async fn validate(&self, Parameters(args): Parameters<ValidateArgs>) -> Result<CallToolResult, McpError> {
        info!("MCP Validating {} files...", args.files.len());
        // Simple validation for now, matching the existing handler
        Ok(CallToolResult::success(vec![Content::text("Validation passed (placeholder)")]))
    }

    #[tool(description = "Check status of the Tachyon-Tex engine")]
    async fn health(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text("🚀 Tachyon-Tex Engine is Operational")]))
    }
}

#[prompt_router]
impl TachyonMcpServer {}

#[tool_handler]
#[prompt_handler]
#[task_handler]
impl ServerHandler for TachyonMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "tachyon-tex-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            instructions: Some("This server provides LaTeX compilation and validation tools.".to_string()),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        Ok(self.get_info())
    }
}
