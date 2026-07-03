//! Embedding provider abstraction and vector utilities.
//!
//! The core is LLM-free: an `Embedder` is opt-in. `HashEmbedder` is a
//! deterministic, dependency-free embedder for tests/dev — it is NOT
//! semantically meaningful (it hashes tokens into buckets). A real provider
//! (Ollama) arrives in Fase 1b.

/// A text→vector embedder. Implementations are provided by the app layer;
/// `Database` never holds one.
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
    fn dimensions(&self) -> usize;
    fn model(&self) -> &str;
}

/// Deterministic hashing embedder (bag-of-words into `dim` buckets, L2-normalized).
/// For tests/dev only — not semantic.
pub struct HashEmbedder {
    dim: usize,
}

impl HashEmbedder {
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

/// Embedder backed by a local Ollama server's `/api/embeddings` endpoint.
/// Blocking HTTP via `ureq`; used only when explicitly configured.
pub struct OllamaEmbedder {
    url: String,
    model: String,
}

impl OllamaEmbedder {
    pub fn new(url: String, model: String) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            model,
        }
    }
}

impl Embedder for OllamaEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let endpoint = format!("{}/api/embeddings", self.url);
        let body = serde_json::json!({ "model": self.model, "prompt": text }).to_string();
        let resp = ureq::post(&endpoint)
            .set("Content-Type", "application/json")
            .send_string(&body)
            .map_err(|e| format!("ollama request failed: {e}"))?;
        let text = resp
            .into_string()
            .map_err(|e| format!("ollama read failed: {e}"))?;
        parse_embedding_response(&text)
    }

    fn dimensions(&self) -> usize {
        0 // dynamic (depends on the Ollama model); not used in the retrieval path
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// Parse Ollama's `{"embedding": [..]}` response body into a vector.
pub(crate) fn parse_embedding_response(body: &str) -> Result<Vec<f32>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("bad json: {e}"))?;
    let arr = v
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or("response missing 'embedding' array")?;
    let out: Vec<f32> = arr
        .iter()
        .map(|x| x.as_f64().unwrap_or(0.0) as f32)
        .collect();
    if out.is_empty() {
        return Err("empty embedding".to_string());
    }
    Ok(out)
}

/// Serialize a vector to a little-endian f32 BLOB.
pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

/// Parse a little-endian f32 BLOB back to a vector.
pub fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Cosine similarity in [-1, 1]; 0.0 for length mismatch or a zero vector.
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
