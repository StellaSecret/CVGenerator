// ── Google Identity Services (GIS) OAuth2 ─────────────────────────────────────
//
// Uses the Google-hosted `google.accounts.oauth2` JS library to obtain an
// access token. The library handles the full auth flow client-side so no
// `client_secret` is needed.
//
// The REAL (wasm-only) OAuth flow, localStorage token persistence, and the
// GIS script handling live in `auth_wasm.rs` (declared under
// `#[cfg(target_arch = "wasm32")]`) — kept in its own module so the wasm
// mutation-testing shard in `.github/workflows/mutants.yml` can target a
// file whose every function is either wasm-testable or
// `#[cfg_attr(test, mutants::skip)]`-attributed (native `#[test]`s don't
// execute under the wasm harness, so a mixed file would report mass false
// misses). This file keeps the native dead stubs plus the platform-neutral
// helpers (`now_ms`, `super::token_expiry_ms`, `mask_token`).

#[cfg(target_arch = "wasm32")]
mod auth_wasm;

#[cfg(target_arch = "wasm32")]
pub use auth_wasm::{
    clear_token, get_token, init, on_token_received, set_token, set_token_with_expiry, start_oauth,
};

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

// ── Token storage ────────────────────────────────────────────────────────────
//
// Android gets a real, file-backed implementation here (see the module doc
// comment on `android_auth` for the full flow): Google disallows running
// its OAuth consent screen inside an embedded WebView, so sign-in happens
// natively in Kotlin via Google Play Services, which writes the resulting
// token straight into this same file. Desktop targets (not wasm32, not
// Android) keep the original no-op stubs below — no desktop OAuth flow is
// implemented (or asked for) here.

#[cfg(target_os = "android")]
pub fn get_token() -> Option<String> {
    let path = token_path()?;
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
}
#[cfg(target_os = "android")]
pub fn set_token(token: &str) {
    if let Some(path) = token_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, token);
    }
}
#[cfg(target_os = "android")]
pub fn set_token_with_expiry(token: &str, _expires_in_secs: u64) {
    // Play Services' GoogleAuthUtil.getToken() (see GoogleDriveHelper.kt)
    // doesn't hand back an expires_in the way the web GIS flow does, so
    // there's no expiry to track here — a 401 from Drive (AUTH_EXPIRED_ERR
    // in drive.rs) is what actually signals a dead token on this platform.
    set_token(token);
}
#[cfg(target_os = "android")]
pub fn clear_token() {
    if let Some(path) = token_path() {
        let _ = std::fs::remove_file(&path);
    }
}
#[cfg(target_os = "android")]
fn token_path() -> Option<std::path::PathBuf> {
    crate::services::android_auth::get_files_dir().map(|p| p.join(".cv_drive_token"))
}

#[cfg(target_os = "android")]
pub fn init() {
    // Nothing to do here: Kotlin's `GoogleDriveHelper.super::init()` calls
    // `nativeInit()` (android_auth.rs) itself, at app startup, independent
    // of this function — unlike wasm32's `super::init()`, which has to inject the
    // GIS `<script>` tag before anything else can happen.
}

#[cfg(target_os = "android")]
pub fn start_oauth(_client_id: &str, _redirect_uri: &str) {
    crate::services::android_auth::start_sign_in();
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn get_token() -> Option<String> {
    None
}
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn set_token(_token: &str) {}
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn set_token_with_expiry(_token: &str, _expires_in_secs: u64) {}
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn clear_token() {}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn init() {}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
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

mod tests {
    #[test]
    fn mask_token_long() {
        assert_eq!(super::mask_token("abcdefghijklmnop"), "abcd…mnop");
    }

    #[test]
    fn mask_token_short() {
        assert_eq!(super::mask_token("abc"), "••••");
    }

    #[test]
    fn mask_token_len_eight_is_fully_masked() {
        // Masking only kicks in for lengths strictly greater than 8 — an
        // 8-char token must not be partially revealed.
        assert_eq!(super::mask_token("abcdefgh"), "••••");
    }

    #[test]
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn clear_token_native_stubs() {
        super::set_token("test");
        assert!(super::get_token().is_none());
        super::clear_token();
    }

    #[test]
    fn token_expiry_ms_subtracts_margin() {
        assert_eq!(
            super::token_expiry_ms(1_000, 3600),
            1_000 + 3_600_000 - super::EXPIRY_MARGIN_MS
        );
    }

    #[test]
    fn token_expiry_ms_saturates_to_zero() {
        assert_eq!(super::token_expiry_ms(0, 0), 0);
        assert_eq!(super::token_expiry_ms(10_000, 0), 0);
    }

    #[test]
    fn now_ms_is_epoch_millis() {
        // Any real epoch-ms clock is many orders of magnitude above 2, so
        // both the "replace with 0" and "replace with 1" mutations fail.
        let now = super::now_ms();
        assert!(now > 2, "now_ms must return epoch milliseconds, got {now}");
    }

    #[test]
    fn mask_token_shows_first_and_last_four_chars_when_long_enough() {}

    #[test]
    fn mask_token_hides_a_short_token_entirely() {
        // len() == 8 must NOT clear the `> 8` bound — pins that boundary
        // specifically, not just "short vs long" in general.
        assert_eq!(super::mask_token("abcdefgh"), "••••");
        assert_eq!(super::mask_token(""), "••••");
    }

    // `get_token`'s native stub (`#[cfg(not(target_arch = "wasm32"))] pub
    // fn get_token() -> Option<String> { None }`) always returns `None` —
    // native builds have no localStorage to back it with. Gated the same
    // way the stub itself is, since the wasm32 build's real `get_token`
    // (backed by real localStorage) can legitimately return `Some(...)`
    // after `set_token`, so this assertion would be simply wrong there.
    #[test]
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn get_token_native_stub_always_returns_none() {
        assert_eq!(super::get_token(), None);
    }
}
#[cfg(all(test, target_os = "android"))]
mod android_tests {
    use super::*;

    #[test]
    fn token_path_and_token_roundtrip() {
        let p = super::token_path();
        assert!(p.is_some());
        let _p = p.unwrap();
        super::set_token("tok");
        assert_eq!(super::get_token(), Some("tok".to_string()));
        super::set_token_with_expiry("tok2", 60);
        assert_eq!(super::get_token(), Some("tok2".to_string()));
        super::clear_token();
        assert_eq!(super::get_token(), None);
    }

    #[cfg(all(test, not(target_arch = "wasm32"), not(target_os = "android")))]
    mod native_stub_tests {

        #[test]
        fn stubs_return_defaults() {
            assert_eq!(super::get_token(), None);
            super::set_token("x");
            super::set_token_with_expiry("y", 10);
            super::clear_token();
            super::init();
            super::start_oauth("id", "uri");
        }
    }
}
