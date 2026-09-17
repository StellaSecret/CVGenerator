//! Wasm-only (browser) half of `worker.rs`: the `#[cfg(target_arch =
//! "wasm32")]` model-fetching code and its `#[wasm_bindgen_test]`s.
//!
//! Kept in its own module (declared from `worker.rs` under
//! `#[cfg(target_arch = "wasm32")]`) so a `cargo mutants ...
//! -C --target=wasm32-unknown-unknown` shard can target THIS file alone:
//! every function it mutates either has a real `#[wasm_bindgen_test]` or a
//! `#[cfg_attr(test, mutants::skip)]`, so no native `#[test]` (which
//! silently doesn't run under the wasm harness) can leak in and report
//! false "missed"s. The rest of `worker.rs` (the `EmbeddingWorker` code)
//! stays in the native shard, where its native tests do run.

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
const MODEL_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors";
const CONFIG_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json";
const TOKENIZER_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json";

/// Name of the browser Cache Storage bucket used to persist the downloaded
/// model across sessions. Bump this (e.g. to "cv-generator-model-v2") if
/// you ever change which files are hosted at the URLs above, so old
/// cached bytes from a previous model version aren't served stale.
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

async fn response_text(resp: web_sys::Response) -> Result<String, String> {
    let bytes = response_bytes(resp).await?;
    String::from_utf8(bytes).map_err(|e| format!("response body not valid utf-8: {e}"))
}

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
