use crate::models::cv::LifetimeCV;
use crate::services::embeddings::EmbeddingEngine;
use crate::services::matcher::{tailor_cv_with_scorer, TailorResult};
use crate::services::score::{ScoreMode, Scorer};

/// Hugging Face URLs for the three model files.
///
/// Using Hugging Face's own CDN directly rather than self-hosting via
/// GitHub Releases: HF's CDN is specifically built for exactly this use
/// case (browser-side fetching of model weights — it's the same mechanism
/// libraries like transformers.js/onnxruntime-web rely on), so CORS is a
/// near-non-issue here, unlike GitHub Releases' download redirects. No
/// manual "create a release, attach a binary, update the tag" maintenance
/// step either.
///
/// # Privacy note
/// This does mean the app contacts a third party (huggingface.co) at
/// runtime to fetch these files — worth being upfront about given this
/// project's "nothing leaves your device" positioning. Nothing CV- or
/// JD-related is ever sent to Hugging Face, only a request for public
/// model weights; this is a much narrower exception than the privacy
/// story is actually protecting (your personal data never being
/// transmitted anywhere), but it is a real, honest exception to "fully
/// self-contained, zero third-party contact."
///
/// `#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]`: these are
/// only referenced from the `#[cfg(target_arch = "wasm32")]`-gated
/// `fetch_model_bytes_cached` below, so on a native (non-wasm32) build —
/// e.g. `cargo clippy` running against the host target rather than
/// `wasm32-unknown-unknown` — that whole function doesn't exist and these
/// constants are genuinely, legitimately unused. This tells clippy that's
/// expected rather than a real dead-code bug, without silencing the lint
/// entirely (a real wasm32 build, where these ARE used, still gets full
/// dead-code checking).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const MODEL_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors";
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const CONFIG_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json";
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const TOKENIZER_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json";

/// Name of the browser Cache Storage bucket used to persist the downloaded
/// model across sessions. Bump this (e.g. to "cv-generator-model-v2") if
/// you ever change which files are hosted at the URLs above, so old
/// cached bytes from a previous model version aren't served stale.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const CACHE_NAME: &str = "cv-generator-model-v1";

/// Fetch the three model files, using the browser's persistent Cache
/// Storage API so the ~25-90MB download only ever happens once per
/// browser — not once per page load/session. Subsequent calls (including
/// after closing and reopening the tab, or a page refresh) read straight
/// from disk-backed cache with no network request at all, as long as the
/// cache hasn't been cleared.
///
/// Deliberately a free function taking no `&EmbeddingWorker` — see the
/// doc comment on `EmbeddingWorker::load_model` for why fetching and
/// constructing the engine are kept as two separate steps (holding a
/// Dioxus signal `write()` guard across a multi-second network fetch is a
/// known-risky pattern).
///
/// UNVERIFIED like the rest of this feature — I could not build or run
/// this in my sandbox (no wasm32 target, no browser). The Cache Storage
/// `add()`-then-`match()` pattern used here is a standard, well-documented
/// approach (`add()` fetches AND stores in one step, avoiding the need to
/// manually construct a `Response` from raw bytes, which is a much
/// fiddlier and less certain API surface), but I have not run it.
#[cfg(target_arch = "wasm32")]
pub async fn fetch_model_bytes_cached() -> Result<(Vec<u8>, String, String), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    async fn get_cache() -> Result<web_sys::Cache, String> {
        let window =
            web_sys::window().ok_or_else(|| "no window (not in a browser?)".to_string())?;
        let cache_storage = window
            .caches()
            .map_err(|e| format!("Cache Storage API unavailable: {e:?}"))?;
        let cache_js = JsFuture::from(cache_storage.open(CACHE_NAME))
            .await
            .map_err(|e| format!("cache open failed: {e:?}"))?;
        cache_js
            .dyn_into::<web_sys::Cache>()
            .map_err(|_| "cache open: unexpected result type".to_string())
    }

    async fn fetch_via_cache(
        cache: &web_sys::Cache,
        url: &str,
    ) -> Result<web_sys::Response, String> {
        let existing = JsFuture::from(cache.match_with_str(url))
            .await
            .map_err(|e| format!("cache match failed for {url}: {e:?}"))?;

        if existing.is_undefined() {
            // Not cached yet: `Cache.add()` fetches AND stores the response
            // in one step. If this 404s or hits a CORS error, `add()`
            // itself rejects — surfacing exactly the failure mode
            // described in this module's CORS warning above.
            JsFuture::from(cache.add_with_str(url))
                .await
                .map_err(|e| format!("fetching {url} failed (network or CORS error): {e:?}"))?;
        }

        let response_js = JsFuture::from(cache.match_with_str(url))
            .await
            .map_err(|e| format!("cache match (after add) failed for {url}: {e:?}"))?;
        response_js
            .dyn_into::<web_sys::Response>()
            .map_err(|_| format!("{url}: expected a cached Response, got something else"))
    }

    async fn response_bytes(resp: web_sys::Response) -> Result<Vec<u8>, String> {
        let buf_promise = resp
            .array_buffer()
            .map_err(|e| format!("array_buffer() call failed: {e:?}"))?;
        let buf_js = JsFuture::from(buf_promise)
            .await
            .map_err(|e| format!("array_buffer() await failed: {e:?}"))?;
        let array = js_sys::Uint8Array::new(&buf_js);
        let mut bytes = vec![0u8; array.length() as usize];
        array.copy_to(&mut bytes[..]);
        Ok(bytes)
    }

    async fn response_text(resp: web_sys::Response) -> Result<String, String> {
        let bytes = response_bytes(resp).await?;
        String::from_utf8(bytes).map_err(|e| format!("response body not valid utf-8: {e}"))
    }

    let cache = get_cache().await?;

    let model_resp = fetch_via_cache(&cache, MODEL_URL).await?;
    let model_bytes = response_bytes(model_resp).await?;

    let config_resp = fetch_via_cache(&cache, CONFIG_URL).await?;
    let config_json = response_text(config_resp).await?;

    let tokenizer_resp = fetch_via_cache(&cache, TOKENIZER_URL).await?;
    let tokenizer_json = response_text(tokenizer_resp).await?;

    Ok((model_bytes, config_json, tokenizer_json))
}

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
    /// Currently unused (the wiring test below uses `inject_engine_for_test`
    /// instead, to avoid depending on real candle model loading), but kept
    /// available — useful once real weights are available for a test that
    /// exercises the actual `load_model`/`fetch_and_load_model` async path.
    #[allow(dead_code)]
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
        assert_eq!(result.tailored.skills.len(), 1);
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
