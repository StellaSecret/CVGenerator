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
// helpers (`now_ms`, `token_expiry_ms`, `mask_token`).

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

    #[test]
    fn mask_token_shows_first_and_last_four_chars_when_long_enough() {
        assert_eq!(mask_token("abcdefghij"), "abcd…ghij");
    }

    #[test]
    fn mask_token_hides_a_short_token_entirely() {
        // len() == 8 must NOT clear the `> 8` bound — pins that boundary
        // specifically, not just "short vs long" in general.
        assert_eq!(mask_token("abcdefgh"), "••••");
        assert_eq!(mask_token(""), "••••");
    }

    // `get_token`'s native stub (`#[cfg(not(target_arch = "wasm32"))] pub
    // fn get_token() -> Option<String> { None }`) always returns `None` —
    // native builds have no localStorage to back it with. Gated the same
    // way the stub itself is, since the wasm32 build's real `get_token`
    // (backed by real localStorage) can legitimately return `Some(...)`
    // after `set_token`, so this assertion would be simply wrong there.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn get_token_native_stub_always_returns_none() {
        assert_eq!(get_token(), None);
    }
}
