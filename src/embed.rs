//! Embedding provider abstraction and vector utilities.
//!
//! The core is LLM-free: an `Embedder` is opt-in. `HashEmbedder` is a
//! deterministic, dependency-free embedder for tests/dev — it is NOT
//! semantically meaningful (it hashes tokens into buckets). A real provider
//! (Ollama) arrives in Fase 1b.

/// A text→vector embedder. Implementations are provided by the app layer;
/// `Database` never holds one.
#[allow(dead_code)] // TODO(fase-1b): remove once wired into the server/CLI
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
    fn dimensions(&self) -> usize;
    fn model(&self) -> &str;
}

/// Deterministic hashing embedder (bag-of-words into `dim` buckets, L2-normalized).
/// For tests/dev only — not semantic.
#[allow(dead_code)] // TODO(fase-1b): remove once wired into the server/CLI
pub struct HashEmbedder {
    dim: usize,
}

impl HashEmbedder {
    #[allow(dead_code)] // TODO(fase-1b): remove once wired into the server/CLI
    pub fn new(dim: usize) -> Self {
        Self { dim: dim.max(1) }
    }
}

impl Embedder for HashEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let mut v = vec![0f32; self.dim];
        for token in text.split_whitespace() {
            let mut h: u64 = 1469598103934665603; // FNV-1a offset
            for b in token.to_lowercase().bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(1099511628211);
            }
            let idx = (h % self.dim as u64) as usize;
            let sign = if (h >> 63) & 1 == 1 { -1.0 } else { 1.0 };
            v[idx] += sign;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        Ok(v)
    }

    fn dimensions(&self) -> usize {
        self.dim
    }

    fn model(&self) -> &str {
        "hash-v1"
    }
}

/// Serialize a vector to a little-endian f32 BLOB.
#[allow(dead_code)] // TODO(fase-1a): remove once used in Task 3/4
pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

/// Parse a little-endian f32 BLOB back to a vector.
#[allow(dead_code)] // TODO(fase-1a): remove once used in Task 3/4
pub fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Cosine similarity in [-1, 1]; 0.0 for length mismatch or a zero vector.
#[allow(dead_code)] // TODO(fase-1a): remove once used in Task 3/4
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}
