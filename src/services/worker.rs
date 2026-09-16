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
/// # Bundle weight vs. download weight
/// This fetch is fully on-demand: nothing is downloaded until the user
/// chooses an Embedding/Hybrid score mode and clicks "Load model" (the
/// UI block is hidden entirely in Keyword mode — see views/tailor.rs).
/// The `candle-*` and `tokenizers` crate code IS however statically
/// linked into the wasm binary for every build (they're plain, ungated
/// dependencies in Cargo.toml), adding a few MB to the bundle. So on a
/// modest connection the *initial* app load includes that crate code, but
/// the dominant cost — the ~25-90MB model download — only happens on
/// explicit opt-in, and then persists in Cache Storage across sessions.
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
async fn get_cache() -> Result<web_sys::Cache, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let window = web_sys::window().ok_or_else(|| "no window (not in a browser?)".to_string())?;
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

#[cfg(target_arch = "wasm32")]
// Needs a real network fetch (or CORS-permitting mock server) to
// `MODEL_URL`/`CONFIG_URL`/`TOKENIZER_URL` to exercise meaningfully — the
// browser Cache API calls around it (`cache.match_with_str`,
// `cache.add_with_str`) aren't mockable from `wasm_bindgen_test` without
// also controlling what `fetch()` resolves to. `response_bytes`/
// `response_text`/`get_cache` below don't have this problem (see their
// tests) and are NOT skipped.
#[cfg_attr(test, mutants::skip)]
async fn fetch_via_cache(cache: &web_sys::Cache, url: &str) -> Result<web_sys::Response, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
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

#[cfg(target_arch = "wasm32")]
async fn response_bytes(resp: web_sys::Response) -> Result<Vec<u8>, String> {
    use wasm_bindgen_futures::JsFuture;
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

#[cfg(target_arch = "wasm32")]
async fn response_text(resp: web_sys::Response) -> Result<String, String> {
    let bytes = response_bytes(resp).await?;
    String::from_utf8(bytes).map_err(|e| format!("response body not valid utf-8: {e}"))
}

#[cfg(target_arch = "wasm32")]
// Thin orchestration over `fetch_via_cache` (see its skip comment above) —
// skipped for the same reason: no path to exercise the real network fetch
// in CI without a mock server this project doesn't have yet. The parts
// split out below (`get_cache`, `response_bytes`, `response_text`) don't
// have this problem and are NOT skipped — see their own tests.
#[cfg_attr(test, mutants::skip)]
pub async fn fetch_model_bytes_cached() -> Result<(Vec<u8>, String, String), String> {
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

// ── WASM-only tests ──────────────────────────────────────────────────────────
//
// `get_cache`/`response_bytes`/`response_text` above are `#[cfg(target_arch
// = "wasm32")]` — invisible to a native `cargo test --lib`, and previously
// only reachable by `fetch_model_bytes_cached`'s untested real-network path
// (see its `#[cfg_attr(test, mutants::skip)]` comment). None of these three
// need a real network fetch to exercise for real, though:
//   - `get_cache` only opens the browser's local CacheStorage (a same-
//     origin, no-network API) — genuinely callable here.
//   - `response_bytes`/`response_text` take a `web_sys::Response` as INPUT,
//     which can be constructed synthetically from an in-memory byte string
//     via `Response::new_with_opt_u8_array` — no fetch involved either.
// Requires `wasm-pack test --headless --chrome` (or firefox) to run — see
// the `wasm-*` shards in `.github/workflows/mutants.yml` and the `wasm`
// entry in `ci.yml`. `wasm_bindgen_test_configure!(run_in_browser)` is
// required because these touch real browser APIs (CacheStorage), not just
// pure wasm32 arithmetic that could run in Node/no browser at all.
#[cfg(all(test, target_arch = "wasm32"))]
// cargo-mutants only auto-skips functions carrying an attribute
// whose last path segment is literally `test` (`#[test]`,
// `#[tokio::test]`, ...) or an enclosing `#[cfg(test)]` it detects
// directly on that item — `#[wasm_bindgen_test]`'s path doesn't
// match that check, and the `cfg(test)` on this module wasn't
// enough either in practice, so without this every helper and
// test function below got "mutated" to `()` and reported as a
// missed mutant (trivially: a test that asserts nothing passes).
#[cfg_attr(test, mutants::skip)]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    fn synthetic_response(body: &[u8]) -> web_sys::Response {
        // Matches the pattern wasm-bindgen's own web-sys test suite uses
        // (crates/web-sys/tests/wasm/response.rs) — `new_with_opt_u8_array`
        // takes an `Option<&mut [u8]>`, not a `js_sys::Uint8Array`.
        let mut owned = body.to_vec();
        web_sys::Response::new_with_opt_u8_array(Some(&mut owned))
            .expect("constructing a synthetic Response from an in-memory byte array must not fail")
    }

    #[wasm_bindgen_test]
    async fn response_bytes_roundtrips_the_response_body() {
        let resp = synthetic_response(b"hello wasm");
        let bytes = response_bytes(resp)
            .await
            .expect("response_bytes must succeed on a well-formed synthetic Response");
        assert_eq!(bytes, b"hello wasm".to_vec());
    }

    #[wasm_bindgen_test]
    async fn response_bytes_handles_an_empty_body() {
        let resp = synthetic_response(&[]);
        let bytes = response_bytes(resp)
            .await
            .expect("response_bytes must succeed on an empty body");
        assert!(
            bytes.is_empty(),
            "expected an empty byte vec, got {bytes:?}"
        );
    }

    #[wasm_bindgen_test]
    async fn response_text_decodes_valid_utf8() {
        let resp = synthetic_response("{\"hello\":\"wasm\"}".as_bytes());
        let text = response_text(resp)
            .await
            .expect("response_text must succeed on valid UTF-8");
        assert_eq!(text, "{\"hello\":\"wasm\"}");
    }

    #[wasm_bindgen_test]
    async fn response_text_rejects_invalid_utf8() {
        // A lone continuation byte (0x80) is never valid at the start of a
        // UTF-8 sequence.
        let resp = synthetic_response(&[0xFF, 0xFE, 0x80]);
        let result = response_text(resp).await;
        assert!(
            result.is_err(),
            "expected an error decoding invalid UTF-8, got {result:?}"
        );
    }

    #[wasm_bindgen_test]
    async fn get_cache_opens_the_named_cache_storage_entry() {
        let cache = get_cache().await.expect(
            "get_cache must succeed in a real browser test environment (CacheStorage is a \
             same-origin, no-network browser API — should always be available here)",
        );
        // Round-trip a value through the real Cache API to confirm we got a
        // genuinely usable `Cache` handle back, not just any JsValue that
        // happened to `dyn_into` without erroring.
        let key = "https://example.invalid/cv-generator-worker-test-key";
        let resp = synthetic_response(b"cache smoke test");
        let put_promise = cache.put_with_str(key, &resp);
        wasm_bindgen_futures::JsFuture::from(put_promise)
            .await
            .expect("cache.put_with_str must succeed on a real Cache handle");
        let matched = wasm_bindgen_futures::JsFuture::from(cache.match_with_str(key))
            .await
            .expect("cache.match_with_str must succeed");
        assert!(
            !matched.is_undefined(),
            "expected the just-`put` entry to be found by match_with_str"
        );
        // Clean up so repeated local test runs don't accumulate stale
        // entries in the browser's real CacheStorage for this origin.
        let _ = wasm_bindgen_futures::JsFuture::from(cache.delete_with_str(key)).await;
    }
}
