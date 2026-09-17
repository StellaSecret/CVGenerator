use crate::models::cv::LifetimeCV;
use crate::services::embeddings::EmbeddingEngine;
use crate::services::matcher::{tailor_cv_with_scorer, TailorResult};
use crate::services::score::{ScoreMode, Scorer};

// The `#[cfg(target_arch = "wasm32")]` model-fetching half of this file
// (Cache Storage access + `fetch_model_bytes_cached`'s wasm implementation)
// lives in `worker_wasm.rs`, declared here under the same cfg so it exists
// only in the browser (wasm32) build. It's kept in its own module — rather
// than inline like before — so the wasm mutation-testing shard in
// `.github/workflows/mutants.yml` can target a file whose every function is
// either wasm-testable or `mutants::skip`-attributed (native `#[test]`s
// don't execute under the wasm harness, so a mixed file would report mass
// false misses). See also the wasm_bindgen_test docs in `worker_wasm.rs`.
#[cfg(target_arch = "wasm32")]
mod worker_wasm;

#[cfg(target_arch = "wasm32")]
pub use worker_wasm::fetch_model_bytes_cached;

#[cfg(not(target_arch = "wasm32"))]
pub async fn fetch_model_bytes_cached() -> Result<(Vec<u8>, String, String), String> {
    Err("Model fetching is only implemented for the browser (wasm32) build.".to_string())
}

pub struct EmbeddingWorker {
    engine: Option<EmbeddingEngine>,
}

impl Default for EmbeddingWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, PartialEq)]
pub enum WorkerStatus {
    Idle,
    Loading,
    Ready,
    Error(String),
}

impl EmbeddingWorker {
    pub fn new() -> Self {
        EmbeddingWorker { engine: None }
    }

    pub fn engine(&self) -> Option<&EmbeddingEngine> {
        self.engine.as_ref()
    }

    pub fn engine_mut(&mut self) -> Option<&mut EmbeddingEngine> {
        self.engine.as_mut()
    }

    pub fn is_ready(&self) -> bool {
        self.engine.is_some()
    }

    /// Test-only: inject an already-constructed engine directly, bypassing
    /// `load_model`'s real weight/config/tokenizer parsing. Used so worker
    /// wiring tests don't need to also exercise real candle model loading
    /// (that's covered separately by embeddings.rs's own tests against a
    /// tiny synthetic model).
    #[cfg(test)]
    pub(crate) fn inject_engine_for_test(&mut self, engine: EmbeddingEngine) {
        self.engine = Some(engine);
    }

    /// Fetching the bundled model's bytes (via `asset!()`, which must live
    /// in the binary crate — see `views/tailor.rs::fetch_bundled_model_bytes`)
    /// and constructing the engine (`load_model`, below) are deliberately
    /// two separate steps rather than one combined method: fetching is a
    /// multi-second, ~25MB network operation, and holding a Dioxus signal
    /// `write()` guard on this worker across that whole `.await` is a
    /// known-risky pattern in reactive frameworks. Callers should fetch
    /// bytes with no `EmbeddingWorker` access at all, then call
    /// `load_model` (below) only for the brief, synchronous-under-the-hood
    /// final construction step.
    pub async fn load_model(
        &mut self,
        model_bytes: &[u8],
        config_json: &str,
        tokenizer_json: &str,
    ) -> Result<(), String> {
        let engine = EmbeddingEngine::load(model_bytes, config_json, tokenizer_json)?;
        self.engine = Some(engine);
        Ok(())
    }

    pub fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        match &mut self.engine {
            Some(engine) => engine.embed(texts),
            None => Err("Embedding engine not loaded".to_string()),
        }
    }

    pub fn embed_jd(&mut self, jd_text: &str) -> Result<Vec<f32>, String> {
        let embeddings = self.embed_texts(&[jd_text.to_string()])?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "No embedding returned".to_string())
    }

    /// Tailor a CV using `mode`, with the worker's own loaded embedding
    /// engine (if any) actually wired into the `Scorer` doing the scoring.
    ///
    /// This used to be a free function (`tailor_with_embeddings`) that built
    /// a brand-new `Scorer` with its own, separate `engine: None` field —
    /// meaning the engine this `EmbeddingWorker` loaded (via `load_model`)
    /// was never passed to the scorer that actually needed it. In
    /// `ScoreMode::Embedding` / `ScoreMode::Hybrid`, every per-text
    /// embedding lookup silently fell back to a `None` engine and scored
    /// `0.0`, no matter what `jd_embedding` was — the JD embedding was
    /// computed correctly, but never had anything to compare against.
    ///
    /// The fix: temporarily move `self.engine` into the `Scorer` for the
    /// duration of scoring (`Option::take`), then move it back out
    /// afterwards, so this worker keeps ownership (and its warm cache)
    /// between calls.
    pub fn tailor_with_embeddings(
        &mut self,
        cv: &LifetimeCV,
        jd_text: &str,
        mode: ScoreMode,
        jd_embedding: Option<&[f32]>,
    ) -> TailorResult {
        let mut scorer = Scorer::new(mode);
        scorer.engine = self.engine.take();
        let result = tailor_cv_with_scorer(cv, jd_text, &mut scorer, jd_embedding);
        self.engine = scorer.engine.take();
        result
    }
}

pub fn tailor_keyword_only(cv: &LifetimeCV, jd_text: &str) -> TailorResult {
    let mut scorer = Scorer::new(ScoreMode::Keyword);
    tailor_cv_with_scorer(cv, jd_text, &mut scorer, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    /// Minimal, dependency-free executor for polling the small, non-blocking
    /// futures in this module during tests (no real async I/O happens here
    /// — `load_model` has no `.await` points of its own — so a trivial
    /// no-op waker is sufficient; no need to pull in `pollster`/`futures`
    /// just for this).
    ///
    /// Used by `load_model_returns_err_and_leaves_worker_unready_on_bad_input`
    /// below to drive the real (non-mocked) `load_model` async path without
    /// pulling in `pollster`/`futures` just for one test.
    fn block_on<F: std::future::Future>(mut fut: F) -> F::Output {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw_waker) };
        let mut cx = Context::from_waker(&waker);
        // Safety: `fut` is a local, not moved after this point.
        let mut fut = unsafe { std::pin::Pin::new_unchecked(&mut fut) };
        loop {
            if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
                return out;
            }
        }
    }

    fn test_cv() -> LifetimeCV {
        LifetimeCV {
            personal: PersonalInfo {
                name: "Test".to_string(),
                ..Default::default()
            },
            skills: vec![Skill {
                id: "s1".into(),
                name: "Rust".into(),
                category: SkillCategory::Programming,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn keyword_tailor_works() {
        let cv = test_cv();
        let result = tailor_keyword_only(&cv, "Rust programming language");
        assert_eq!(result.tailored.skills.len(), 1);
        assert!(result.tailored.match_score > 0.0);
    }

    #[test]
    fn tailor_with_embeddings_falls_back_without_engine() {
        let cv = test_cv();
        let mut worker = EmbeddingWorker::new();
        let result = worker.tailor_with_embeddings(
            &cv,
            "Rust programming language",
            ScoreMode::Embedding,
            None,
        );
        // Without an engine an Embedding scorer scores everything 0, so the
        // only surviving skills are Expert/Mastery ones — and test_cv's
        // single 'Rust' is Intermediate. Production never runs this path
        // (the UI gates Embedding/Hybrid behind a loaded model); this just
        // guards against panic and NaN, and pins the drop-behaviour.
        assert_eq!(result.tailored.skills.len(), 0);
    }

    #[test]
    fn tailor_with_embeddings_keeps_engine_after_call() {
        // Regression test for the engine-ownership bug: after scoring, the
        // worker must still own its engine (so `is_ready()` stays true and
        // the warm cache persists across repeated tailoring calls) rather
        // than losing it inside the temporary `Scorer`.
        //
        // Uses `inject_engine_for_test` + the shared tiny synthetic model
        // from embeddings.rs rather than routing through `load_model`'s
        // real safetensors/config parsing, since this test is about the
        // Scorer<->EmbeddingWorker wiring, not about candle model loading
        // (which embeddings.rs's own tests cover separately).
        let cv = test_cv();
        let mut worker = EmbeddingWorker::new();
        worker.inject_engine_for_test(crate::services::embeddings::tiny_test_engine());
        assert!(worker.is_ready());

        let _ = worker.tailor_with_embeddings(
            &cv,
            "Rust programming language",
            ScoreMode::Embedding,
            Some(&[1.0; 8]), // matches tiny_test_engine's hidden_size (8), not the real model's 384
        );

        assert!(
            worker.is_ready(),
            "worker must still own its engine after tailor_with_embeddings"
        );
    }

    // `load_model` itself has no `#[cfg(target_arch = "wasm32")]` gate — only
    // `fetch_model_bytes_cached` (which supplies its bytes) does — so, unlike
    // that function, it's fully reachable from a native `cargo test`. Every
    // other test above deliberately bypasses it via `inject_engine_for_test`
    // to avoid exercising real candle model loading, which left it as the
    // one mutant `cargo-mutants` could reach but nothing actually called:
    // stubbing its whole body to `Ok(())` compiled and passed the rest of
    // the suite. Malformed `config_json` fails fast in
    // `EmbeddingEngine::load`'s first parsing step (before touching
    // `model_bytes`/`tokenizer_json` at all — see embeddings.rs), so this
    // needs no real model weights to genuinely exercise the function and
    // distinguish it from an unconditional `Ok(())`.
    #[test]
    fn load_model_returns_err_and_leaves_worker_unready_on_bad_input() {
        let mut worker = EmbeddingWorker::new();
        let result = block_on(worker.load_model(&[], "not valid json", "not valid json"));
        assert!(
            result.is_err(),
            "malformed config must fail to load, got {result:?}"
        );
        assert!(
            !worker.is_ready(),
            "a failed load must not leave the worker holding an engine"
        );
    }

    #[test]
    fn fetch_model_bytes_cached_unavailable_on_native() {
        let res = block_on(fetch_model_bytes_cached());
        assert!(
            res.is_err(),
            "native stub must report model fetching as web-only, got {res:?}"
        );
    }

    #[test]
    fn worker_starts_without_engine() {
        let mut worker = EmbeddingWorker::new();
        assert!(!worker.is_ready());
        assert!(worker.engine().is_none());
        assert!(worker.engine_mut().is_none());
    }

    #[test]
    fn worker_exposes_engine_after_injection() {
        let mut worker = EmbeddingWorker::new();
        worker.inject_engine_for_test(crate::services::embeddings::tiny_test_engine());
        assert!(worker.is_ready());
        assert!(worker.engine().is_some());
        assert!(worker.engine_mut().is_some());
    }

    #[test]
    fn embed_texts_forwards_engine_output() {
        let mut engine = crate::services::embeddings::tiny_test_engine();
        let expected = engine.embed(&["hello world".to_string()]).unwrap();
        let mut worker = EmbeddingWorker::new();
        worker.inject_engine_for_test(crate::services::embeddings::tiny_test_engine());
        let got = worker.embed_texts(&["hello world".to_string()]).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn embed_jd_forwards_engine_output() {
        let mut engine = crate::services::embeddings::tiny_test_engine();
        let expected = engine
            .embed(&["job description".to_string()])
            .unwrap()
            .remove(0);
        let mut worker = EmbeddingWorker::new();
        worker.inject_engine_for_test(crate::services::embeddings::tiny_test_engine());
        let got = worker.embed_jd("job description").unwrap();
        assert_eq!(got, expected);
    }
}
