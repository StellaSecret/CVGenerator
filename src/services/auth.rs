// ── Google Identity Services (GIS) OAuth2 ─────────────────────────────────────
//
// Uses the Google-hosted `google.accounts.oauth2` JS library to obtain an
// access token. The library handles the full auth flow client-side so no
// `client_secret` is needed.

#[cfg(target_arch = "wasm32")]
const TOKEN_KEY: &str = "cv_generator_google_token";
#[cfg(target_arch = "wasm32")]
const TOKEN_EXPIRY_KEY: &str = "cv_generator_google_token_expiry";

/// How far ahead of the provider's `expires_in` a token is treated as dead.
/// Prevents a token that's about to expire from being used and then refused
/// with a 401 right at the boundary.
const EXPIRY_MARGIN_MS: u64 = 30_000;

/// Error marker returned by drive.rs when the stored token is no longer
/// valid (Google refuses it). sync.rs matches on it and re-prompts.
pub const AUTH_EXPIRED_ERR: &str = "auth_expired";

/// Epoch-ms timestamp at which an access token issued at `now_ms` with
/// `expires_in` seconds of remaining life should be considered expired.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn token_expiry_ms(now_ms: u64, expires_in_secs: u64) -> u64 {
    now_ms
        .saturating_add(expires_in_secs.saturating_mul(1000))
        .saturating_sub(EXPIRY_MARGIN_MS)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

// ── Token listeners (WASM only) ──────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
use std::sync::OnceLock;

#[cfg(target_arch = "wasm32")]
static TOKEN_CLIENT: OnceLock<wasm_bindgen::JsValue> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static TOKEN_LISTENERS: RefCell<Vec<Box<dyn FnMut(&str)>>> = const { RefCell::new(Vec::new()) };
}

#[cfg(target_arch = "wasm32")]
pub fn on_token_received(cb: Box<dyn FnMut(&str)>) {
    TOKEN_LISTENERS.with(|listeners| {
        listeners.borrow_mut().push(cb);
    });
}

/// Test-only accessor so `on_token_received`'s effect (pushing onto the
/// otherwise-private `TOKEN_LISTENERS` thread-local) is actually observable
/// from a test — without this, "replace on_token_received with ()" is a
/// mutation no black-box test could ever catch, since nothing outside this
/// module could tell the difference.
#[cfg(all(test, target_arch = "wasm32"))]
fn token_listener_count() -> usize {
    TOKEN_LISTENERS.with(|listeners| listeners.borrow().len())
}

// ── Token storage ────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
pub fn get_token() -> Option<String> {
    use gloo_storage::{LocalStorage, Storage};
    if let Ok(expiry) = LocalStorage::get::<u64>(TOKEN_EXPIRY_KEY) {
        if now_ms() >= expiry {
            // Stale cached token: drop it so the UI shows signed-out and
            // forces a fresh sign-in instead of trusting an expired token.
            clear_token();
            return None;
        }
    }
    LocalStorage::get(TOKEN_KEY).ok()
}

#[cfg(target_arch = "wasm32")]
pub fn set_token(token: &str) {
    use gloo_storage::{LocalStorage, Storage};
    if token.is_empty() {
        clear_token();
    } else {
        let _ = LocalStorage::set(TOKEN_KEY, token);
    }
}

/// Persist a token together with its expiry, as provided by the OAuth
/// response's `expires_in` (seconds). Without an expiry value the token
/// would otherwise sit in localStorage indefinitely, long past death.
#[cfg(target_arch = "wasm32")]
pub fn set_token_with_expiry(token: &str, expires_in_secs: u64) {
    use gloo_storage::{LocalStorage, Storage};
    set_token(token);
    let _ = LocalStorage::set(TOKEN_EXPIRY_KEY, token_expiry_ms(now_ms(), expires_in_secs));
}

#[cfg(target_arch = "wasm32")]
pub fn clear_token() {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::delete(TOKEN_KEY);
    LocalStorage::delete(TOKEN_EXPIRY_KEY);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get_token() -> Option<String> {
    None
}
#[cfg(not(target_arch = "wasm32"))]
pub fn set_token(_token: &str) {}
#[cfg(not(target_arch = "wasm32"))]
pub fn set_token_with_expiry(_token: &str, _expires_in_secs: u64) {}
#[cfg(not(target_arch = "wasm32"))]
pub fn clear_token() {}

// ── OAuth flow (WASM — GIS library) ──────────────────────────────────────────

/// Initialise the Google Identity Services library by injecting its script tag.
/// Call once on app startup.
#[cfg(target_arch = "wasm32")]
// Injects a <script src="https://accounts.google.com/gsi/client"> tag and
// relies on that real, network-loaded Google script defining `window.
// google.accounts.oauth2` — not something a headless CI browser with no
// network access to accounts.google.com (or a same-origin mock of Google's
// exact global JS API shape) can exercise for real. `get_gis_oauth2`'s own
// "not present" path IS tested below, since that doesn't need the script.
#[cfg_attr(test, mutants::skip)]
pub fn init() {
    use wasm_bindgen::JsCast;
    let doc = web_sys::window()
        .expect("no window in WASM")
        .document()
        .expect("no document in WASM");

    if get_gis_oauth2().is_ok() {
        return;
    }

    if let Ok(script) = doc.create_element("script") {
        if let Ok(s) = script.dyn_into::<web_sys::HtmlScriptElement>() {
            s.set_src("https://accounts.google.com/gsi/client");
            s.set_defer(true);
            if let Some(body) = doc.body() {
                let _ = body.append_child(&s);
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn get_gis_oauth2() -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    let google = js_sys::Reflect::get(&js_sys::global(), &"google".into())?;
    let accounts = js_sys::Reflect::get(&google, &"accounts".into())?;
    js_sys::Reflect::get(&accounts, &"oauth2".into())
}

/// Trigger the Google sign-in flow using the GIS Token Client.
/// Calls `on_token_received` listeners with the access token on success.
#[cfg(target_arch = "wasm32")]
// Same reason as `init` above — this drives the real GIS `initTokenClient`/
// `requestAccessToken` flow, which needs the real Google script loaded and
// (for `requestAccessToken`) real user OAuth consent. Not mockable without
// either network access to Google in CI or a hand-rolled JS shim that
// reproduces GIS's exact callback-invocation semantics closely enough to
// trust the result — more risk than value for what it'd actually prove.
#[cfg_attr(test, mutants::skip)]
pub fn start_oauth(client_id: &str, _redirect_uri: &str) {
    use wasm_bindgen::JsCast;

    if let Some(tc) = TOKEN_CLIENT.get() {
        if let Some(f) = js_sys::Reflect::get(tc, &"requestAccessToken".into())
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
        {
            let _ = f.call0(tc);
        }
        return;
    }

    if get_gis_oauth2().is_err() {
        return;
    }

    use wasm_bindgen::prelude::Closure;

    let cb = Closure::wrap(Box::new(move |resp: wasm_bindgen::JsValue| {
        if let Some(token) = js_sys::Reflect::get(&resp, &"access_token".into())
            .ok()
            .and_then(|t| t.as_string())
        {
            let expires_in = js_sys::Reflect::get(&resp, &"expires_in".into())
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(3600.0) as u64;
            set_token_with_expiry(&token, expires_in);
            TOKEN_LISTENERS.with(|listeners| {
                for cb in listeners.borrow_mut().iter_mut() {
                    cb(&token);
                }
            });
        }
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);

    let config = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&config, &"client_id".into(), &client_id.into());
    let _ = js_sys::Reflect::set(
        &config,
        &"scope".into(),
        &"https://www.googleapis.com/auth/drive.appdata".into(),
    );
    let _ = js_sys::Reflect::set(&config, &"callback".into(), cb.as_ref());

    if let Ok(oauth2) = get_gis_oauth2() {
        if let Some(f) = js_sys::Reflect::get(&oauth2, &"initTokenClient".into())
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
        {
            if let Ok(tc) = f.call1(&oauth2, &config) {
                cb.forget();
                TOKEN_CLIENT.set(tc.clone()).ok();
                if let Some(f) = js_sys::Reflect::get(&tc, &"requestAccessToken".into())
                    .ok()
                    .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
                {
                    let _ = f.call0(&tc);
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn init() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn start_oauth(_client_id: &str, _redirect_uri: &str) {}

// ── UI helper ─────────────────────────────────────────────────────────────────

pub fn mask_token(t: &str) -> String {
    if t.len() > 8 {
        format!("{}…{}", &t[..4], &t[t.len() - 4..])
    } else {
        "••••".to_string()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_token_long() {
        assert_eq!(mask_token("abcdefghijklmnop"), "abcd…mnop");
    }

    #[test]
    fn mask_token_short() {
        assert_eq!(mask_token("abc"), "••••");
    }

    #[test]
    fn mask_token_len_eight_is_fully_masked() {
        // Masking only kicks in for lengths strictly greater than 8 — an
        // 8-char token must not be partially revealed.
        assert_eq!(mask_token("abcdefgh"), "••••");
    }

    #[test]
    fn get_set_clear_token_native_stubs() {
        set_token("test");
        assert!(get_token().is_none());
        clear_token();
    }

    #[test]
    fn token_expiry_ms_subtracts_margin() {
        assert_eq!(
            token_expiry_ms(1_000, 3600),
            1_000 + 3_600_000 - EXPIRY_MARGIN_MS
        );
    }

    #[test]
    fn token_expiry_ms_saturates_to_zero() {
        assert_eq!(token_expiry_ms(0, 0), 0);
        assert_eq!(token_expiry_ms(10_000, 0), 0);
    }

    #[test]
    fn now_ms_is_epoch_millis() {
        // Any real epoch-ms clock is many orders of magnitude above 2, so
        // both the "replace with 0" and "replace with 1" mutations fail.
        let now = now_ms();
        assert!(now > 2, "now_ms must return epoch milliseconds, got {now}");
    }
}

// ── WASM-only tests ──────────────────────────────────────────────────────────
//
// `get_token`/`set_token`/`set_token_with_expiry`/`clear_token` above are
// `#[cfg(target_arch = "wasm32")]` and read/write real `localStorage` — not
// reachable from a native `cargo test --lib` at all (the native builds
// just above are inert stubs, already covered by
// `get_set_clear_token_native_stubs`). None of these need network or a
// real OAuth flow, just a real browser's `localStorage`, so they're
// directly testable here. Requires `wasm-pack test --headless --chrome`
// (see the `wasm-*` shards in `.github/workflows/mutants.yml`).
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
    use gloo_storage::{LocalStorage, Storage};
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    // Every test clears both keys first: wasm-bindgen-test runs all tests
    // in one shared browser page/localStorage, so state can otherwise leak
    // between tests run in the same session.
    fn reset() {
        clear_token();
    }

    #[wasm_bindgen_test]
    fn set_then_get_roundtrips_the_token() {
        reset();
        set_token("abc123");
        assert_eq!(get_token().as_deref(), Some("abc123"));
        reset();
    }

    #[wasm_bindgen_test]
    fn set_empty_string_clears_instead_of_storing() {
        reset();
        set_token("abc123");
        set_token("");
        assert_eq!(
            get_token(),
            None,
            "set_token(\"\") must clear, not store an empty string"
        );
    }

    #[wasm_bindgen_test]
    fn clear_token_removes_both_keys() {
        reset();
        set_token("abc123");
        clear_token();
        assert_eq!(get_token(), None);
        assert!(
            LocalStorage::get::<String>(TOKEN_KEY).is_err(),
            "clear_token must remove the raw storage entry, not just make \
             get_token() report None for some other reason"
        );
        assert!(
            LocalStorage::get::<u64>(TOKEN_EXPIRY_KEY).is_err(),
            "clear_token must also remove the expiry entry"
        );
    }

    #[wasm_bindgen_test]
    fn set_token_with_expiry_is_readable_before_it_expires() {
        reset();
        // A full hour of margin comfortably exceeds this test's runtime.
        set_token_with_expiry("abc123", 3600);
        assert_eq!(get_token().as_deref(), Some("abc123"));
        reset();
    }

    #[wasm_bindgen_test]
    fn set_token_with_expiry_zero_seconds_is_already_expired() {
        reset();
        // `token_expiry_ms(now, 0)` saturates to `now.saturating_sub(margin)`
        // — at or before `now_ms()`'s next read, so `get_token` must treat
        // it as already expired and drop it, not return it once.
        set_token_with_expiry("abc123", 0);
        assert_eq!(
            get_token(),
            None,
            "a token with 0 seconds of validity must read back as expired"
        );
        // And `get_token`'s own expiry check must have cleared it, not just
        // reported None while leaving stale data behind.
        assert!(LocalStorage::get::<String>(TOKEN_KEY).is_err());
        reset();
    }

    #[wasm_bindgen_test]
    fn on_token_received_registers_a_listener() {
        let before = token_listener_count();
        on_token_received(Box::new(|_token: &str| {}));
        assert_eq!(
            token_listener_count(),
            before + 1,
            "on_token_received must push onto TOKEN_LISTENERS"
        );
    }

    #[wasm_bindgen_test]
    fn get_gis_oauth2_errors_when_the_google_script_is_not_loaded() {
        // No network access to accounts.google.com in this test
        // environment, so `window.google` is genuinely absent here — this
        // exercises the real "not loaded yet" path `init()` itself checks
        // before injecting the script tag, without needing the script.
        assert!(
            get_gis_oauth2().is_err(),
            "expected an error with no `google` global present on this page"
        );
    }
}
