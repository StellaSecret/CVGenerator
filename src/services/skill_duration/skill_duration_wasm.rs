//! Wasm-only (browser) half of `skill_duration.rs`: the single
//! `js_sys::Date`-backed clock read used by `current_year_month`, split out
//! of that function's `#[cfg(target_arch = "wasm32")]` branch, plus its
//! `#[wasm_bindgen_test]`.
//!
//! Kept in its own module (declared from `skill_duration.rs` under
//! `#[cfg(target_arch = "wasm32")]`) so a `cargo mutants ...
//! -C --target=wasm32-unknown-unknown` shard can target THIS file alone:
//! every function it mutates either has a real `#[wasm_bindgen_test]` or a
//! `#[cfg_attr(test, mutants::skip)]`, so no native `#[test]` (which
//! silently doesn't run under the wasm harness) can leak in and report
//! false "missed"s. Note that `skill_duration.rs`'s native
//! `current_year_month` branch, and the whole pure date-math layer
//! (`parse_month_year`, `merge_intervals`, `format_years`, ...), stay in
//! `skill_duration.rs` for the native shard.

use super::YearMonth;

/// Current (year, month) computed from the browser's real clock — `month`
/// is 1-12, calendar convention (NOT the 0-indexed convention JS
/// `Date.getMonth()` uses; converted below). This is the one real call to
/// an actual clock, isolated here the same way `drive.rs`'s `now_ms()`
/// isolates its own clock access.
pub(super) fn current_year_month_wasm() -> YearMonth {
    let d = js_sys::Date::new_0();
    (d.get_full_year() as i32, d.get_month() as u32 + 1)
}

// ── WASM-only tests ──────────────────────────────────────────────────────────
//
// `current_year_month` isn't itself `#[cfg(target_arch = "wasm32")]`-gated
// (only its internal branch is), but its wasm32 branch — the one real code
// path this project's `js_sys::Date` clock access actually runs through in
// production — is still invisible to native `cargo test --lib`. Directly
// mirrors `js_sys::Date::new_0()` here to derive an independently-computed
// expected value, the same way the native
// `current_year_month_native_divides_total_seconds_by_a_day_not_modulo`
// test in `skill_duration.rs` mirrors `SystemTime::now()`.
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

    #[wasm_bindgen_test]
    fn current_year_month_wasm_matches_js_date_now() {
        let expected = js_sys::Date::new_0();
        let (year, month) = current_year_month_wasm();
        assert_eq!(year, expected.get_full_year() as i32);
        // JS `Date.getMonth()` is 0-indexed; the conversion to the 1-12
        // calendar convention this codebase uses everywhere else (see
        // e.g. `YearMonth`'s own doc comment) is `+ 1` — pins that
        // conversion specifically, distinct from the native side's
        // `/`-vs-`%` day-count mutant.
        assert_eq!(month, expected.get_month() as u32 + 1);
    }
}
