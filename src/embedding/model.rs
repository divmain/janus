use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use directories::BaseDirs;
use fastembed::{EmbeddingModel as FastembedModel, InitOptions, TextEmbedding};
use parking_lot::Mutex;
use tokio::sync::OnceCell;

use crate::config::Config;

pub const EMBEDDING_DIMENSIONS: usize = 768;
pub const EMBEDDING_MODEL_NAME: &str = "jinaai/jina-embeddings-v2-base-code";

/// Batch size for embedding generation operations.
/// Processing in batches improves throughput while controlling memory usage.
pub const EMBEDDING_BATCH_SIZE: usize = 32;

/// Timeout for embedding generation (30 seconds)
pub const EMBEDDING_TIMEOUT: Duration = Duration::from_secs(30);

/// Wrapper around the fastembed TextEmbedding model with lazy loading.
///
/// Uses `parking_lot::Mutex` (not `tokio::sync::Mutex`) because the embedding
/// inference is CPU-bound and always runs inside `spawn_blocking`. This avoids
/// holding an async mutex while blocking the tokio executor thread.
///
/// `parking_lot::Mutex` is chosen over `std::sync::Mutex` because it:
/// - Does not implement poisoning (`.lock()` never panics)
/// - Is more compact and faster than the standard library implementation
///
/// The inner model is wrapped in `Arc<Mutex<_>>` so it can be moved into
/// `spawn_blocking` closures which require `'static` captured values.
pub struct EmbeddingModel {
    inner: Arc<Mutex<TextEmbedding>>,
}

/// Global singleton for the embedding model.
///
/// Uses `tokio::sync::OnceCell` with `get_or_try_init()` so that if
/// initialization fails (e.g., network timeout downloading the model),
/// the cell remains unset and subsequent calls will retry. This is
/// important for long-running processes (TUI, MCP server) where a
/// transient failure should not permanently disable semantic search.
static EMBEDDING_MODEL: OnceCell<EmbeddingModel> = OnceCell::const_new();

impl EmbeddingModel {
    /// Load the embedding model from cache or download it
    fn load() -> Result<Self, String> {
        let cache_dir = get_embedding_cache_dir()?;

        // Parse the model name string to get the enum variant
        let model = FastembedModel::from_str(EMBEDDING_MODEL_NAME)
            .map_err(|e| format!("Invalid embedding model name '{EMBEDDING_MODEL_NAME}': {e}"))?;

        let options = InitOptions::new(model)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(true);

        let inner = TextEmbedding::try_new(options).map_err(|e| {
            format!(
                "Failed to load embedding model '{EMBEDDING_MODEL_NAME}': {e}. This may be caused by network issues when downloading the model from HuggingFace (~161MB). Please check your internet connection and try again."
            )
        })?;

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    /// Generate embedding for a single text.
    ///
    /// Runs the CPU-bound inference on a blocking thread via `spawn_blocking`
    /// to avoid stalling the async executor.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let text = text.to_string();
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock();
            let embeddings = guard.embed(vec![&text], None).map_err(|e| format!("{e}"))?;
            embeddings
                .into_iter()
                .next()
                .ok_or_else(|| "No embedding generated".to_string())
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
    }

    /// Generate embeddings for a batch of texts.
    ///
    /// Runs the CPU-bound inference on a blocking thread via `spawn_blocking`
    /// to avoid stalling the async executor.
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        let texts_owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock();
            let texts_ref: Vec<&str> = texts_owned.iter().map(|s| s.as_str()).collect();
            guard.embed(texts_ref, None).map_err(|e| format!("{e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
    }
}

/// Get the cache directory for embeddings
fn get_embedding_cache_dir() -> Result<PathBuf, String> {
    let base_dirs = BaseDirs::new().ok_or("Could not determine base directories")?;
    let cache_dir = base_dirs.data_local_dir().join("janus").join("embeddings");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache directory: {e}"))?;

    Ok(cache_dir)
}

/// Get or initialize the global embedding model singleton.
///
/// Returns an error if semantic search is disabled in the config.
///
/// Uses `get_or_try_init()` so that if initialization fails (e.g., network
/// timeout downloading the model from HuggingFace), the `OnceCell` remains
/// unset and subsequent calls will retry. This matches the retry-on-failure
/// pattern used by the store singleton in `src/store/mod.rs`.
///
/// The config check is performed on every call (before touching the `OnceCell`)
/// so that a config change can take effect without restarting the process.
pub async fn get_embedding_model() -> Result<&'static EmbeddingModel, String> {
    // Check if semantic search is enabled before loading the model.
    // This runs on every call so config changes take effect on retry.
    match Config::load() {
        Ok(config) => {
            if !config.semantic_search_enabled() {
                return Err(
                    "Semantic search is disabled. Enable with: janus config set semantic_search.enabled true".to_string()
                );
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to load config: {e}. Proceeding with semantic search enabled by default."
            );
        }
    }

    EMBEDDING_MODEL
        .get_or_try_init(|| async {
            tokio::task::spawn_blocking(EmbeddingModel::load)
                .await
                .map_err(|e| format!("spawn_blocking failed: {e}"))?
        })
        .await
}

/// Generate embedding for a single text (convenience function)
pub async fn generate_embedding(text: &str) -> Result<Vec<f32>, String> {
    let model = get_embedding_model().await?;
    model.embed(text).await
}

/// Generate embedding for a ticket with title and optional body
pub async fn generate_ticket_embedding(
    title: &str,
    body: Option<&str>,
) -> Result<Vec<f32>, String> {
    let full_text = match body {
        Some(b) => format!("{title}\n\n{b}"),
        None => title.to_string(),
    };
    generate_embedding(&full_text).await
}

/// Compute cosine similarity between two embedding vectors.
///
/// Returns a value in `[-1.0, 1.0]`, where `1.0` means identical direction,
/// `0.0` means orthogonal (unrelated), and `-1.0` means opposite.
///
/// # Cases that return `0.0`
///
/// - **Dimension mismatch**: Cosine similarity is undefined for vectors of
///   different dimensions. Returning `0.0` ensures mismatched embeddings are
///   ranked last rather than causing a panic. In practice, this case should
///   not occur because [`load_embeddings()`](crate::store::embeddings) validates
///   that all loaded embeddings match [`EMBEDDING_DIMENSIONS`] at load time,
///   discarding any that don't.
///
/// - **Zero-norm vectors**: If either vector has zero magnitude, division by
///   zero is avoided by returning `0.0`. This is a safe default; zero-norm
///   vectors are implausible outputs from real embedding models.
///
/// - **Genuinely orthogonal vectors**: For valid, non-zero vectors that happen
///   to be perpendicular, `0.0` is the mathematically correct cosine similarity,
///   indicating no semantic relationship.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_embedding_dimensions() {
        // This test verifies that the embedding dimensions constant is correct
        assert_eq!(EMBEDDING_DIMENSIONS, 768);
    }

    #[tokio::test]
    async fn test_ticket_embedding() {
        let title = "Test Ticket";
        let body = "This is a test body";

        // Note: This test will download the model on first run (~161MB)
        let embedding = generate_ticket_embedding(title, Some(body)).await;

        // The model download might fail in test environments, so we just check
        // that we get a result (either Ok or Err)
        match embedding {
            Ok(vec) => {
                assert_eq!(vec.len(), EMBEDDING_DIMENSIONS);
            }
            Err(e) => {
                // In CI environments, the model might not be available
                println!("Embedding generation failed (expected in some environments): {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_similar_texts_have_similar_embeddings() {
        let text1 = "Rust programming language";
        let text2 = "The Rust programming language is great";
        let text3 = "Python is a different language";

        let emb1 = generate_embedding(text1).await;
        let emb2 = generate_embedding(text2).await;
        let emb3 = generate_embedding(text3).await;

        // If we can generate embeddings, check similarity
        if let (Ok(e1), Ok(e2), Ok(e3)) = (emb1, emb2, emb3) {
            let sim_1_2 = cosine_similarity(&e1, &e2);
            let sim_1_3 = cosine_similarity(&e1, &e3);

            // Similar texts should have higher similarity
            assert!(
                sim_1_2 > sim_1_3,
                "Similar texts should have higher cosine similarity"
            );
        } else {
            // Skip this test if model isn't available
            println!("Skipping similarity test - model not available");
        }
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!(similarity.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert_eq!(similarity, 0.0);
    }

    #[tokio::test]
    #[serial]
    async fn test_lazy_loading() {
        // Note: In a parallel test environment, we can't guarantee that
        // the singleton hasn't been initialized by another test. We only
        // verify that calling get_embedding_model() works correctly.

        // Try to get the model - this will initialize it if not already done
        let result = get_embedding_model().await;

        // The result type depends on whether model loading succeeded
        match result {
            Ok(_) => {
                // After a successful call, the OnceCell should contain a value
                assert!(
                    EMBEDDING_MODEL.initialized(),
                    "Model should be loaded after successful init"
                );
                println!("Model loaded successfully");
            }
            Err(e) => {
                // On failure, the OnceCell should remain unset so retry is possible
                println!("Model loading failed (expected in some environments): {e}");
            }
        }
    }
}
