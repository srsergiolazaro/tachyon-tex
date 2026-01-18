use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use xxhash_rust::xxh64::xxh64;
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

// Moonshot #1: In-memory cache - store PDF bytes directly, no fs::read on HIT
pub struct CacheEntry {
    pub pdf_data: Vec<u8>,
    pub created_at: u64,
    pub last_accessed: AtomicU64,  // Moonshot #4: LRU tracking
    pub compile_time_ms: u64,
    pub size_bytes: usize,
}

impl Clone for CacheEntry {
    fn clone(&self) -> Self {
        Self {
            pdf_data: self.pdf_data.clone(),
            created_at: self.created_at,
            last_accessed: AtomicU64::new(self.last_accessed.load(Ordering::Relaxed)),
            compile_time_ms: self.compile_time_ms,
            size_bytes: self.size_bytes,
        }
    }
}

#[derive(Clone)]
pub struct CompilationCache {
    pub enabled: bool,
    pub max_cache_mb: usize,  // Moonshot #4: Memory limit for LRU
    pub entries: Arc<RwLock<HashMap<u64, CacheEntry>>>,
}

impl CompilationCache {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            max_cache_mb: 512,  // 512MB default limit
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn hash_input(data: &[u8]) -> u64 {
        xxh64(data, 0)
    }

    // Moonshot #1: Direct memory access - no fs::read, 10-50x faster
    // Moonshot #4: LRU with 7-day TTL based on last access
    pub async fn get_pdf(&self, hash: u64) -> Option<(Vec<u8>, u64)> {
        if !self.enabled { return None; }

        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(&hash) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            // Update last_accessed on every HIT for LRU
            entry.last_accessed.store(now, Ordering::Relaxed);
            // Return directly from memory - no fs::read!
            return Some((entry.pdf_data.clone(), entry.compile_time_ms));
        }
        None
    }

    // Moonshot #1: Store PDF bytes directly in memory
    pub async fn put_pdf(&self, hash: u64, pdf_data: &[u8], compile_time_ms: u64) {
        if !self.enabled { return; }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let mut entries = self.entries.write().await;
        
        // Check memory limit and evict LRU if needed
        let current_size: usize = entries.values().map(|e| e.size_bytes).sum();
        if current_size + pdf_data.len() > self.max_cache_mb * 1024 * 1024 {
            // Evict least recently accessed entry
            if let Some((&lru_hash, _)) = entries.iter()
                .min_by_key(|(_, e)| e.last_accessed.load(Ordering::Relaxed)) {
                entries.remove(&lru_hash);
            }
        }
        
        entries.insert(hash, CacheEntry {
            pdf_data: pdf_data.to_vec(),
            created_at: now,
            last_accessed: AtomicU64::new(now),
            compile_time_ms,
            size_bytes: pdf_data.len(),
        });
    }

    // Moonshot #4: LRU cleanup - only evict if not accessed in 7 days
    pub async fn cleanup_expired(&self) -> usize {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let mut entries = self.entries.write().await;
        let mut to_remove = Vec::new();

        for (hash, entry) in entries.iter() {
            // 7 days = 604800 seconds, based on last_accessed not created_at
            if now - entry.last_accessed.load(Ordering::Relaxed) >= 604800 {
                to_remove.push(*hash);
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
