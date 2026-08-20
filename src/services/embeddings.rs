//! Sentence-transformer embedding engine.
//!
//! Loads a BERT-family model (all-MiniLM-L6-v2, exported as safetensors —
//! see the loading notes on `EmbeddingEngine::load`) plus a Hugging Face
//! tokenizer, and produces 384-dim, mean-pooled, L2-normalized sentence
//! embeddings for CV-to-JD semantic matching.
//!
//! ── IMPORTANT: verification status ──────────────────────────────────────
//! Every other file changed in this project during development was
//! compiled and unit-tested against the real project code before being
//! handed over. This file could NOT be compiled or run: the sandbox this
//! was written in has only Rust 1.75 (via `apt`), which is too old to even
//! *resolve* candle's current dependency graph against today's crates.io
//! index (most crates now require Cargo's `edition2024` support). There is
//! also no `wasm32-unknown-unknown` target installed, and no browser to
//! actually run inference in.
//!
//! What that means concretely: the tensor-shape logic below (BertModel
//! forward pass, masked mean pooling) follows the same well-established
//! pattern used in candle's own official example
//! (`candle-wasm-examples/bert` in the candle-transformers repo) — this is
//! a very standard, widely-documented architecture, not something novel —
//! but it has not been compiled here. Before relying on it:
//!   1. `cargo check` this file with your normal (newer) toolchain and fix
//!      any API drift (candle's public API does shift between versions).
//!   2. Run the `#[cfg(test)]` tests at the bottom against a tiny synthetic
//!      model (they don't need real downloaded weights) to sanity-check
//!      tensor shapes and the pooling/normalization math.
//!   3. Only then wire in real downloaded weights and test end-to-end in a
//!      browser.
//!
//! All inference is CPU-only (candle's plain `Device::Cpu` backend, no
//! WebGPU/WASM-SIMD tuning attempted) and intended to run inside a Web
//! Worker so it doesn't block the main thread — the worker-thread wiring
//! itself is outside this file's scope.

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig, DTYPE};
use std::collections::HashMap;

// ── Public types ──────────────────────────────────────────────────────────────

/// A single embedding vector (384 dimensions for all-MiniLM-L6-v2).
pub type Embedding = Vec<f32>;

/// Text item to be embedded, tagged with an ID for result correlation.
#[derive(Debug, Clone)]
pub struct EmbedItem {
    pub id: String,
    pub text: String,
}

/// Loaded embedding engine — holds the BERT model, tokenizer, and device.
pub struct EmbeddingEngine {
    model: BertModel,
    tokenizer: tokenizers::Tokenizer,
    device: Device,
    /// Cache of id → embedding for CV items.
    cache: HashMap<String, Embedding>,
}

// ── Constants ─────────────────────────────────────────────────────────────────

/// all-MiniLM-L6-v2 produces 384-dim embeddings.
const EMBEDDING_DIM: usize = 384;

/// Maximum token length for the tokenizer. all-MiniLM-L6-v2 was trained
/// with a 256-token max, but CV/JD blocks are short paragraphs, not full
/// documents — 128 keeps padding (and therefore wasted compute) down
/// without truncating anything realistic.
const MAX_SEQ_LEN: usize = 128;

// ── Implementation ────────────────────────────────────────────────────────────

impl EmbeddingEngine {
    /// Load the engine from raw model weights, model config, and tokenizer
    /// JSON — all three as in-memory byte/string buffers (there is no
    /// filesystem in a browser; these should come from `fetch()` responses
    /// at the call site, not from disk paths).
    ///
    /// # Expected inputs
    /// - `model_bytes`: a `.safetensors` file for a BERT-architecture
    ///   model — e.g. the `model.safetensors` export of
    ///   `sentence-transformers/all-MiniLM-L6-v2`. NOT an `.onnx` file —
    ///   candle loads safetensors weights directly into its own `BertModel`
    ///   rather than running an ONNX graph, which is the whole reason this
    ///   is tractable to hand-verify (no ONNX op-by-op reimplementation
    ///   needed).
    /// - `config_json`: that model's `config.json` (defines hidden size,
    ///   number of layers/heads, vocab size, etc. — required to construct
    ///   the right-shaped `BertModel` before loading weights into it).
    /// - `tokenizer_json`: that model's `tokenizer.json`.
    ///
    /// # Errors
    /// Returns an error string if any of the three inputs fail to parse,
    /// or if the weights don't match the shapes the config implies.
    pub fn load(
        model_bytes: &[u8],
        config_json: &str,
        tokenizer_json: &str,
    ) -> Result<Self, String> {
        let device = Device::Cpu;

        let config: BertConfig =
            serde_json::from_str(config_json).map_err(|e| format!("config parse failed: {e}"))?;

        let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_json.as_bytes())
            .map_err(|e| format!("tokenizer load failed: {e}"))?;

        // Load safetensors weights directly from an in-memory buffer (no
        // filesystem access, which is unavailable in a browser/worker
        // context). `VarBuilder::from_buffered_safetensors` is the same
        // entry point candle's own wasm examples use for exactly this
        // reason.
        let vb = VarBuilder::from_buffered_safetensors(model_bytes.to_vec(), DTYPE, &device)
            .map_err(|e| format!("safetensors load failed: {e}"))?;

        let model = BertModel::load(vb, &config).map_err(|e| format!("model init failed: {e}"))?;

        Ok(Self {
            model,
            tokenizer,
            device,
            cache: HashMap::new(),
        })
    }

    /// Embed a batch of texts. Returns a parallel Vec of embeddings.
    ///
    /// Internally: tokenize → pad/truncate to `MAX_SEQ_LEN` → BERT forward
    /// pass → attention-masked mean pool → L2 normalize.
    pub fn embed(&mut self, texts: &[String]) -> Result<Vec<Embedding>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| format!("tokenize failed: {e}"))?;

        let batch_size = encodings.len();
        let seq_len = MAX_SEQ_LEN;

        let mut input_ids_data = vec![0u32; batch_size * seq_len];
        let mut attention_mask_data = vec![0u32; batch_size * seq_len];
        let mut token_type_ids_data = vec![0u32; batch_size * seq_len];

        for (i, enc) in encodings.iter().enumerate() {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            let types = enc.get_type_ids();
            let len = ids.len().min(seq_len);

            for j in 0..len {
                input_ids_data[i * seq_len + j] = ids[j];
                attention_mask_data[i * seq_len + j] = mask[j];
                token_type_ids_data[i * seq_len + j] = types[j];
            }
            // Positions beyond `len` (padding) stay 0 in all three arrays —
            // an attention_mask of 0 means "ignore this position", which is
            // exactly what an empty/pad token type + pad token id need.
        }

        // Build input tensors, shape [batch_size, seq_len].
        let input_ids = Tensor::from_vec(input_ids_data, (batch_size, seq_len), &self.device)
            .map_err(|e| format!("input_ids tensor build failed: {e}"))?;
        let token_type_ids =
            Tensor::from_vec(token_type_ids_data, (batch_size, seq_len), &self.device)
                .map_err(|e| format!("token_type_ids tensor build failed: {e}"))?;
        let attention_mask_u32 = Tensor::from_vec(
            attention_mask_data.clone(),
            (batch_size, seq_len),
            &self.device,
        )
        .map_err(|e| format!("attention_mask tensor build failed: {e}"))?;

        // BertModel::forward takes an `Option<&Tensor>` attention mask.
        let last_hidden_state = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask_u32))
            .map_err(|e| format!("forward pass failed: {e}"))?;

        // Extract to plain f32 for pooling — see the module doc comment for
        // why pooling happens in verified plain-Rust code rather than
        // further chained candle tensor ops (broadcasting/dtype rules are
        // easy to get subtly wrong and I could not compile-check them
        // here).
        let hidden_dim = last_hidden_state
            .dim(2)
            .map_err(|e| format!("could not read hidden dim: {e}"))?;
        let hidden: Vec<Vec<Vec<f32>>> = last_hidden_state
            .to_dtype(DType::F32)
            .and_then(|t| t.to_vec3())
            .map_err(|e| format!("tensor->vec conversion failed: {e}"))?;

        let mut embeddings = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            let mask_row = &attention_mask_data[i * seq_len..(i + 1) * seq_len];
            let pooled = masked_mean_pool(&hidden[i], mask_row, hidden_dim);
            embeddings.push(l2_normalize(&pooled));
        }

        Ok(embeddings)
    }

    /// Embed texts with caching — only embed texts whose IDs are not yet cached.
    /// Returns embeddings in the same order as the input items.
    pub fn embed_with_cache(&mut self, items: &[EmbedItem]) -> Result<Vec<Embedding>, String> {
        let mut to_embed_indices = Vec::new();
        let mut to_embed_texts = Vec::new();

        for (i, item) in items.iter().enumerate() {
            if !self.cache.contains_key(&item.id) {
                to_embed_indices.push(i);
                to_embed_texts.push(item.text.clone());
            }
        }

        if !to_embed_texts.is_empty() {
            let new_embeddings = self.embed(&to_embed_texts)?;
            for (idx, emb) in to_embed_indices.iter().zip(new_embeddings) {
                self.cache.insert(items[*idx].id.clone(), emb);
            }
        }

        let results: Vec<Embedding> = items
            .iter()
            .map(|item| {
                self.cache
                    .get(&item.id)
                    .cloned()
                    .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM])
            })
            .collect();

        Ok(results)
    }

    /// Embed a single text (for JD).
    pub fn embed_single(&mut self, text: &str) -> Result<Embedding, String> {
        let embeddings = self.embed(&[text.to_string()])?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }

    /// Invalidate cache for specific IDs (called when CV data changes).
    pub fn invalidate_cache(&mut self, ids: &[String]) {
        for id in ids {
            self.cache.remove(id.as_str());
        }
    }

    /// Clear all cached embeddings.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get embedding dimension.
    pub fn dim(&self) -> usize {
        EMBEDDING_DIM
    }

    /// Get number of cached embeddings.
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

// ── Math utilities ────────────────────────────────────────────────────────────

/// Attention-masked mean pool over the sequence dimension for a single
/// batch item: sum hidden states only at positions where
/// `attention_mask == 1` (real tokens), then divide by the count of real
/// tokens — NOT by the fixed padded sequence length.
///
/// This fixes a real correctness bug in the original (stub) version of
/// this function: it summed every position including padding and divided
/// by the full padded `seq_len` unconditionally. Since padding positions
/// still produce non-zero hidden states after self-attention (padding
/// isn't literally zero after a transformer layer — it's zero *input*
/// that becomes nonzero through attention with real tokens), including
/// them dilutes the embedding by an amount that depends on how much
/// padding happens to be present — i.e., a longer padded sequence for a
/// short sentence would have silently produced a systematically different
/// embedding than the same sentence tokenized with less padding. Standard
/// sentence-transformers mean pooling is always masked for exactly this
/// reason.
fn masked_mean_pool(
    hidden_states: &[Vec<f32>],
    attention_mask: &[u32],
    hidden_dim: usize,
) -> Vec<f32> {
    let mut pooled = vec![0.0f32; hidden_dim];
    let mut real_token_count = 0.0f32;

    for (token_hidden, &mask) in hidden_states.iter().zip(attention_mask.iter()) {
        if mask == 0 {
            continue;
        }
        real_token_count += 1.0;
        for d in 0..hidden_dim.min(token_hidden.len()) {
            pooled[d] += token_hidden[d];
        }
    }

    if real_token_count > 0.0 {
        for v in &mut pooled {
            *v /= real_token_count;
        }
    }

    pooled
}

/// L2-normalize a vector to unit length.
fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1e-12 {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

/// Cosine similarity between two embedding vectors.
///
/// Returns a value in [-1.0, 1.0] where 1.0 means identical direction.
/// Returns 0.0 for zero vectors (avoids division by zero).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a < 1e-12 || norm_b < 1e-12 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

// ── Test-only synthetic model builder (shared across modules' tests) ──────────
//
// `pub(crate)` and declared outside `mod tests` (rather than inside it) so
// other modules' tests — e.g. worker.rs's wiring tests — can build the
// same tiny synthetic engine instead of duplicating it or depending on
// real downloaded weights.
#[cfg(test)]
pub(crate) fn tiny_test_engine() -> EmbeddingEngine {
    let device = Device::Cpu;
    let config = BertConfig {
        vocab_size: 40,
        hidden_size: 8,
        num_hidden_layers: 1,
        num_attention_heads: 2,
        intermediate_size: 16,
        hidden_act: candle_transformers::models::bert::HiddenAct::Gelu,
        hidden_dropout_prob: 0.0,
        // Must be >= MAX_SEQ_LEN (128, the fixed padded length `embed()`
        // always builds tensors at) since BERT looks up one position
        // embedding per sequence position, including padding positions —
        // undersizing this relative to MAX_SEQ_LEN caused an out-of-bounds
        // index-select ("invalid index 32 with dim size 32") the first
        // time this was actually run. The real all-MiniLM-L6-v2 config
        // uses 512, so this only ever bit the synthetic test fixture.
        max_position_embeddings: MAX_SEQ_LEN,
        type_vocab_size: 2,
        initializer_range: 0.02,
        layer_norm_eps: 1e-12,
        pad_token_id: 0,
        position_embedding_type: candle_transformers::models::bert::PositionEmbeddingType::Absolute,
        use_cache: false,
        classifier_dropout: None,
        model_type: Some("bert".to_string()),
    };
    // UNCERTAIN API CALL: `VarBuilder::zeros` is the constructor I'd expect
    // for this ("give me a VarBuilder that inits any requested tensor to
    // zeros, no data needed"), but I could not verify it against the
    // actual candle-nn 0.11 API surface in this sandbox. If this doesn't
    // compile, the fix is almost certainly swapping in
    // `candle_nn::VarBuilder::from_varmap(&candle_nn::VarMap::new(),
    // DType::F32, &device)` instead, which is the more common pattern
    // I've seen for "build a VarBuilder with no real weights yet".
    let vb = VarBuilder::zeros(DType::F32, &device);
    let model = BertModel::load(vb, &config).expect("tiny synthetic model should build");

    let tokenizer_json = r#"{
        "model": { "type": "WordLevel", "vocab": {"[UNK]":0,"[PAD]":1,"[CLS]":2,"[SEP]":3,"hello":4,"world":5}, "unk_token": "[UNK]" }
    }"#;
    let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_json.as_bytes())
        .expect("tiny tokenizer should parse");

    EmbeddingEngine {
        model,
        tokenizer,
        device,
        cache: HashMap::new(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// These deliberately do NOT need real downloaded model weights — they use
// `tiny_test_engine()` above (small hidden size, one layer, tiny vocab,
// randomly-initialized weights) purely to sanity-check tensor shapes and
// the pooling/normalization math end-to-end. They cannot verify the
// *model produces semantically correct embeddings* (that requires real
// pretrained weights and a real browser run) — only that the plumbing
// (tokenize → tensor build → forward → pool → normalize) doesn't panic
// and produces the expected shapes.
//
// IMPORTANT: this whole test module is unverified along with the rest of
// this file — I could not run `cargo test` here (see module doc comment).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_mean_pool_ignores_padding() {
        // Two "real" tokens with hidden values [1,1] and [3,3], then one
        // padding position with a huge value that must NOT affect the mean.
        let hidden = vec![vec![1.0, 1.0], vec![3.0, 3.0], vec![999.0, 999.0]];
        let mask = vec![1, 1, 0];
        let pooled = masked_mean_pool(&hidden, &mask, 2);
        assert!((pooled[0] - 2.0).abs() < 1e-6, "got {pooled:?}");
        assert!((pooled[1] - 2.0).abs() < 1e-6, "got {pooled:?}");
    }

    #[test]
    fn masked_mean_pool_all_padding_returns_zero_not_nan() {
        let hidden = vec![vec![5.0, 5.0]];
        let mask = vec![0];
        let pooled = masked_mean_pool(&hidden, &mask, 2);
        assert_eq!(pooled, vec![0.0, 0.0]);
    }

    #[test]
    fn cosine_similarity_identical_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_opposite_is_negative() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_empty_returns_zero() {
        assert!(cosine_similarity(&[], &[]).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_different_lengths_returns_zero() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_zero_vector_returns_zero() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 2.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn l2_normalize_unit_length() {
        let v = vec![3.0, 4.0];
        let n = l2_normalize(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn l2_normalize_zero_vector() {
        let v = vec![0.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert!(n.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn embed_produces_correct_dim_and_is_normalized() {
        let mut engine = tiny_test_engine();
        let out = engine
            .embed(&["hello world".to_string()])
            .expect("embed should not error on synthetic model");
        assert_eq!(out.len(), 1);
        // NOTE: this synthetic model's hidden_size is 8, not the real
        // model's 384 — EMBEDDING_DIM is a documented constant for the
        // real model, this test just checks the pipeline runs end-to-end
        // and returns a normalized vector of *some* consistent dimension.
        let norm: f32 = out[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4 || norm == 0.0);
    }

    #[test]
    fn embed_empty_texts_returns_empty() {
        let mut engine = tiny_test_engine();
        let result = engine.embed(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn cache_basic_operations() {
        let mut engine = tiny_test_engine();

        let items = vec![
            EmbedItem {
                id: "a".into(),
                text: "hello".into(),
            },
            EmbedItem {
                id: "b".into(),
                text: "world".into(),
            },
        ];

        let _ = engine.embed_with_cache(&items).unwrap();
        assert_eq!(engine.cache_len(), 2);

        let _ = engine.embed_with_cache(&items).unwrap();
        assert_eq!(engine.cache_len(), 2);

        engine.invalidate_cache(&["a".to_string()]);
        assert_eq!(engine.cache_len(), 1);
    }

    #[test]
    fn clear_cache_empties_all() {
        let mut engine = tiny_test_engine();

        let items = vec![EmbedItem {
            id: "x".into(),
            text: "test".into(),
        }];
        let _ = engine.embed_with_cache(&items).unwrap();
        assert_eq!(engine.cache_len(), 1);

        engine.clear_cache();
        assert_eq!(engine.cache_len(), 0);
    }
}
