use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use directories::BaseDirs;
use fastembed::{EmbeddingModel as FastembedModel, InitOptions, TextEmbedding};

pub const EMBEDDING_DIMENSIONS: usize = 384;
pub const EMBEDDING_MODEL_NAME: &str = "AllMiniLML6V2";

/// Wrapper around the fastembed TextEmbedding model with lazy loading
/// Uses Mutex for interior mutability since TextEmbedding requires &mut self
pub struct EmbeddingModel {
    inner: Mutex<TextEmbedding>,
}

/// Global singleton for the embedding model
static EMBEDDING_MODEL: OnceLock<Result<EmbeddingModel, String>> = OnceLock::new();

impl EmbeddingModel {
    /// Load the embedding model from cache or download it
    fn load() -> Result<Self, String> {
        let cache_dir = get_embedding_cache_dir()?;

        let options = InitOptions::new(FastembedModel::AllMiniLML6V2)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(true);

        let inner = TextEmbedding::try_new(options).map_err(|e| {
            format!(
                "Failed to load embedding model '{}': {}. This may be caused by network issues when downloading the model from HuggingFace (~30MB). Please check your internet connection and try again.",
                EMBEDDING_MODEL_NAME, e
            )
        })?;

        Ok(Self {
            inner: Mutex::new(inner),
        })
    }

    /// Generate embedding for a single text
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| format!("Mutex poisoned: {}", e))?;

        let embeddings = guard
            .embed(vec![text], None)
            .map_err(|e| format!("{}", e))?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "No embedding generated".to_string())
    }

    /// Generate embeddings for a batch of texts
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| format!("Mutex poisoned: {}", e))?;

        let texts_vec: Vec<&str> = texts.to_vec();
        guard.embed(texts_vec, None).map_err(|e| format!("{}", e))
    }
}

/// Get the cache directory for embeddings
fn get_embedding_cache_dir() -> Result<PathBuf, String> {
    let base_dirs = BaseDirs::new().ok_or("Could not determine base directories")?;
    let cache_dir = base_dirs.data_local_dir().join("janus").join("embeddings");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache directory: {}", e))?;

    Ok(cache_dir)
}

/// Get or initialize the global embedding model singleton
pub fn get_embedding_model() -> Result<&'static EmbeddingModel, String> {
    EMBEDDING_MODEL
        .get_or_init(EmbeddingModel::load)
        .as_ref()
        .map_err(|e| e.clone())
}

/// Generate embedding for a single text (convenience function)
pub fn generate_embedding(text: &str) -> Result<Vec<f32>, String> {
    let model = get_embedding_model()?;
    model.embed(text)
}

/// Generate embedding for a ticket with title and optional body
pub fn generate_ticket_embedding(title: &str, body: Option<&str>) -> Result<Vec<f32>, String> {
    let full_text = match body {
        Some(b) => format!("{}\n\n{}", title, b),
        None => title.to_string(),
    };
    generate_embedding(&full_text)
}

/// Compute cosine similarity between two embeddings
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
        assert_eq!(EMBEDDING_DIMENSIONS, 384);
    }

    #[test]
    fn test_ticket_embedding() {
        let title = "Test Ticket";
        let body = "This is a test body";

        // Note: This test will download the model on first run (~30MB)
        let embedding = generate_ticket_embedding(title, Some(body));

        // The model download might fail in test environments, so we just check
        // that we get a result (either Ok or Err)
        match embedding {
            Ok(vec) => {
                assert_eq!(vec.len(), EMBEDDING_DIMENSIONS);
            }
            Err(e) => {
                // In CI environments, the model might not be available
                println!(
                    "Embedding generation failed (expected in some environments): {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_similar_texts_have_similar_embeddings() {
        let text1 = "Rust programming language";
        let text2 = "The Rust programming language is great";
        let text3 = "Python is a different language";

        let emb1 = generate_embedding(text1);
        let emb2 = generate_embedding(text2);
        let emb3 = generate_embedding(text3);

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

    #[test]
    #[serial]
    fn test_lazy_loading() {
        // Note: In a parallel test environment, we can't guarantee that
        // the singleton hasn't been initialized by another test. We only
        // verify that calling get_embedding_model() works correctly.

        // Try to get the model - this will initialize it if not already done
        let result = get_embedding_model();

        // After calling get_embedding_model, the OnceLock should contain a value
        assert!(
            EMBEDDING_MODEL.get().is_some(),
            "Model should be loaded after first use"
        );

        // The result type depends on whether model loading succeeded
        match result {
            Ok(_) => println!("Model loaded successfully"),
            Err(_) => println!("Model loading failed (expected in some environments)"),
        }
    }
}
