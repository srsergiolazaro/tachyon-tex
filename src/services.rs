use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use xxhash_rust::xxh64::xxh64;
use std::fs;
use crate::models::WebhookSubscription;

// ============================================================================
// Blob Store (Image Fingerprinting)
// ============================================================================

#[derive(Clone)]
pub struct BlobStore {
    pub cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl BlobStore {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, hash: &str) -> Option<Vec<u8>> {
        let cache = self.cache.read().await;
        cache.get(hash).cloned()
    }

    pub async fn put(&self, hash: String, data: Vec<u8>) {
        let mut cache = self.cache.write().await;
        cache.insert(hash, data);
    }
}

// ============================================================================
// PDF Compilation Cache
// ============================================================================

#[derive(Clone)]
pub struct CacheEntry {
    pub pdf_path: PathBuf,
    pub created_at: u64,
    pub compile_time_ms: u64,
    pub size_bytes: usize,
}

#[derive(Clone)]
pub struct CompilationCache {
    pub enabled: bool,
    pub cache_dir: PathBuf,
    pub entries: Arc<RwLock<HashMap<u64, CacheEntry>>>,
}

impl CompilationCache {
    pub fn new(enabled: bool) -> Self {
        let cache_dir = std::env::temp_dir().join("tachyon_pdf_cache");
        fs::create_dir_all(&cache_dir).ok();

        Self {
            enabled,
            cache_dir,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn hash_input(data: &[u8]) -> u64 {
        xxh64(data, 0)
    }

    pub async fn get_pdf(&self, hash: u64) -> Option<(Vec<u8>, u64)> {
        if !self.enabled { return None; }

        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(&hash) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            if now - entry.created_at < 86400 {
                if let Ok(data) = fs::read(&entry.pdf_path) {
                    return Some((data, entry.compile_time_ms));
                }
            }
        }
        None
    }

    pub async fn put_pdf(&self, hash: u64, pdf_data: &[u8], compile_time_ms: u64) {
        if !self.enabled { return; }

        let filename = format!("{:x}.pdf", hash);
        let path = self.cache_dir.join(filename);

        if fs::write(&path, pdf_data).is_ok() {
            let mut entries = self.entries.write().await;
            entries.insert(hash, CacheEntry {
                pdf_path: path,
                created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                compile_time_ms,
                size_bytes: pdf_data.len(),
            });
        }
    }

    pub async fn cleanup_expired(&self) -> usize {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let mut entries = self.entries.write().await;
        let mut to_remove = Vec::new();

        for (hash, entry) in entries.iter() {
            if now - entry.created_at >= 86400 {
                to_remove.push(*hash);
                fs::remove_file(&entry.pdf_path).ok();
            }
        }

        let count = to_remove.len();
        for hash in to_remove {
            entries.remove(&hash);
        }
        count
    }

    pub async fn stats(&self) -> (usize, usize) {
        let entries = self.entries.read().await;
        let total_size = entries.values().map(|e| e.size_bytes).sum();
        (entries.len(), total_size)
    }
}

// ============================================================================
// HMR v2 Format Cache (Preamble tracking)
// ============================================================================

#[derive(Clone)]
pub struct FormatCache {
    pub seen_preambles: Arc<RwLock<HashSet<u64>>>,
}

impl FormatCache {
    pub fn new() -> Self {
        Self {
            seen_preambles: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn extract_preamble(content: &str) -> Option<&str> {
        content.find("\\begin{document}").map(|pos| &content[..pos])
    }

    pub fn hash_preamble(preamble: &str) -> u64 {
        xxh64(preamble.as_bytes(), 0)
    }

    pub async fn check_and_mark(&self, preamble_hash: u64) -> bool {
        let mut seen = self.seen_preambles.write().await;
        if seen.contains(&preamble_hash) {
            true // HIT
        } else {
            seen.insert(preamble_hash);
            false // MISS
        }
    }
}

// ============================================================================
// Shared State
// ============================================================================

#[derive(Clone)]
pub struct AppState {
    pub compilation_cache: CompilationCache,
    pub webhooks: Arc<RwLock<Vec<WebhookSubscription>>>,
    pub format_cache: FormatCache,
    pub blob_store: BlobStore,
    pub config: Arc<tectonic::config::PersistentConfig>,
    pub format_cache_path: PathBuf,
}
