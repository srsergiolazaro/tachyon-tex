use std::path::Path;
use std::fs;
use tectonic::driver::{ProcessingSessionBuilder, OutputFormat, PassSetting};
use tectonic::status::{StatusBackend, MessageKind};

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

pub struct Compiler;

impl Compiler {
    /// Compiles a single file and returns the PDF bytes and build logs.
    ///
    /// # Arguments
    /// * `main_tex_path` - Path to the main .tex file
    /// * `output_dir` - Directory where output files will be written
    /// * `format_cache_path` - Path to the tectonic format cache
    /// * `config_ptr` - Tectonic persistent config
    pub fn compile_file(
        main_tex_path: &Path,
        output_dir: &Path,
        format_cache_path: &Path,
        config: &tectonic::config::PersistentConfig,
    ) -> (Result<Vec<u8>, String>, String) {
        let (mut res, mut logs) = Self::internal_compile(main_tex_path, output_dir, format_cache_path, config);

        if res.is_err() {
            if let Ok(content) = fs::read_to_string(main_tex_path) {
                // Moonshot #1: Self-Healing Logic
                if let Some(fixed_content) = crate::healer::SelfHealer::attempt_heal(&content, &logs) {
                    tracing::info!("🚑 Self-Healing triggered for {:?}", main_tex_path);
                    let _ = fs::write(main_tex_path, fixed_content);
                    
                    logs.push_str("\n\n--- [Tachyon Self-Healing 🚑] ---\nErrors detected. Applying automated fixes and retrying...\n");
                    
                    let (retry_res, retry_logs) = Self::internal_compile(main_tex_path, output_dir, format_cache_path, config);
                    logs.push_str(&retry_logs);
                    res = retry_res;
                    
                    if res.is_ok() {
                        logs.push_str("\n[Self-Healing] ✅ FIXED! Compilation succeeded after auto-patching.\n");
                    }
                }
            }
        }
        (res, logs)
    }

    fn internal_compile(
        main_tex_path: &Path,
        output_dir: &Path,
        format_cache_path: &Path,
        config: &tectonic::config::PersistentConfig,
    ) -> (Result<Vec<u8>, String>, String) {
        let mut status = CapturingStatusBackend::new();
        let bundle_res = config.default_bundle(false, &mut status);
        
        match bundle_res {
            Ok(bundle) => {
                let mut sb = ProcessingSessionBuilder::default();
                let tex_input_name = main_tex_path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                    
                sb.bundle(bundle)
                    .primary_input_path(main_tex_path)
                    .tex_input_name(&tex_input_name)
                    .format_name("latex")
                    .format_cache_path(format_cache_path)
                    .output_dir(output_dir)
                    .print_stdout(false)
                    .output_format(OutputFormat::Pdf)
                    .pass(PassSetting::Default);

                let res = (|| -> Result<Vec<u8>, String> {
                    let mut sess = sb.create(&mut status).map_err(|e| e.to_string())?;
                    sess.run(&mut status).map_err(|e| e.to_string())?;
                    
                    let pdf_name = main_tex_path.file_stem()
                        .ok_or("Invalid filename")?
                        .to_str()
                        .ok_or("Invalid UTF-8 filename")?;
                        
                    let pdf_path = output_dir.join(format!("{}.pdf", pdf_name));
                    fs::read(&pdf_path).map_err(|e| e.to_string())
                })();
                
                (res, status.get_logs())
            },
            Err(e) => (Err(format!("Bundle error: {}", e)), status.get_logs())
        }
    }
}
