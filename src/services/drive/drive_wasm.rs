//! Wasm-only (browser) half of `drive.rs`: the real Google Drive API
//! calls (`drive_backup`/`drive_restore`), the browser-download
//! `local_export`, and `local_export`'s `#[wasm_bindgen_test]`.
//!
//! Kept in its own module (declared from `drive.rs` under
//! `#[cfg(target_arch = "wasm32")]`) so a `cargo mutants ...
//! -C --target=wasm32-unknown-unknown` shard can target THIS file alone:
//! every function it mutates either has a real `#[wasm_bindgen_test]` or a
//! `#[cfg_attr(test, mutants::skip)]`, so no native `#[test]` (which
//! silently doesn't run under the wasm harness) can leak in and report
//! false "missed"s. `drive.rs` keeps the serialisation layer
//! (`build_backup`/`restore_from_json`), `drive_error_from_status`,
//! `now_ms`, and the native dead stubs, which the native shard's tests
//! still cover. On wasm, `drive.rs` re-exports this module's `pub` entry
//! points (`pub use drive_wasm::{...}`), so callers keep using
//! `drive::local_export` etc. unchanged.

use super::{build_backup, drive_error_from_status, restore_from_json, RestoredData};
use crate::models::{LifetimeCV, TailoringSession};

// The Google Drive OAuth scope string lives hard-coded in auth.rs
// ("https://www.googleapis.com/auth/drive.appdata"), so this file needs no
// scope constant of its own.
const DRIVE_API: &str = "https://www.googleapis.com/drive/v3/files";
const UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3/files";
const BACKUP_NAME: &str = "cv_generator_backup.json";

// ── HTTP helpers ──────────────────────────────────────────────────────────────

// Takes a `reqwest::Response`, which — unlike `web_sys::Response` in
// worker_wasm.rs — has no public constructor for a synthetic instance
// outside an actual `reqwest::Client` request/response round trip, so
// there's no equivalent to worker_wasm.rs's `synthetic_response` helper
// available here without either a real network call or a mock HTTP server.
// Skipped for that reason, same as `drive_backup`/`drive_restore` below
// (its only callers), all of which hit the real Google Drive API.
#[cfg_attr(test, mutants::skip)]
async fn check(resp: reqwest::Response) -> Result<reqwest::Response, String> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    match drive_error_from_status(status.as_u16(), &body) {
        Some(err) => Err(err),
        // `None` is unreachable for a non-success status; kept for totality.
        None => Err(format!("HTTP {status}: {body}")),
    }
}

// ── Drive: backup ─────────────────────────────────────────────────────────────

/// Upload the current CV to Google Drive `appDataFolder`.
/// Creates the file on first run; patches it on subsequent runs.
/// Returns the Drive file ID on success.
// Real network call to the Google Drive API — see `check`'s comment above
// for why that's not mockable here yet.
#[cfg_attr(test, mutants::skip)]
pub async fn drive_backup(
    cv: &LifetimeCV,
    saved_sessions: &[TailoringSession],
    token: &str,
) -> Result<String, String> {
    let json = build_backup(cv, saved_sessions);
    let bytes = json.into_bytes();
    let client = reqwest::Client::new();

    // Search for an existing backup file
    let search = check(
        client
            .get(DRIVE_API)
            .query(&[
                (
                    "q",
                    &format!(
                        "name='{BACKUP_NAME}' and 'appDataFolder' in parents and trashed=false"
                    ),
                ),
                ("fields", &"files(id)".to_string()),
                ("spaces", &"appDataFolder".to_string()),
            ])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Drive search: {e}"))?,
    )
    .await?;

    let body: serde_json::Value = search.json().await.map_err(|e| format!("json: {e}"))?;
    let file_id = body["files"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|f| f["id"].as_str().map(String::from));

    let fid = match file_id {
        // File exists → just patch the content
        Some(id) => id,
        // First time → create the metadata shell, then upload content
        None => {
            let meta = serde_json::json!({
                "name": BACKUP_NAME,
                "parents": ["appDataFolder"],
                "mimeType": "application/json"
            });
            let resp = check(
                client
                    .post(DRIVE_API)
                    .bearer_auth(token)
                    .header("Content-Type", "application/json")
                    .body(meta.to_string())
                    .send()
                    .await
                    .map_err(|e| format!("Drive create: {e}"))?,
            )
            .await?;
            let created: serde_json::Value = resp.json().await.map_err(|e| format!("json: {e}"))?;
            created["id"]
                .as_str()
                .map(String::from)
                .ok_or_else(|| format!("No file id returned: {created}"))?
        }
    };

    // Upload (or overwrite) the content
    check(
        client
            .patch(format!("{UPLOAD_API}/{fid}?uploadType=media"))
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .body(bytes)
            .send()
            .await
            .map_err(|e| format!("Drive upload: {e}"))?,
    )
    .await?;

    Ok(fid)
}

// ── Drive: restore ────────────────────────────────────────────────────────────

/// Download the backup from Google Drive `appDataFolder` and return the CV.
// Real network call to the Google Drive API — see `check`'s comment above.
#[cfg_attr(test, mutants::skip)]
pub async fn drive_restore(token: &str) -> Result<RestoredData, String> {
    let client = reqwest::Client::new();

    let search = check(
        client
            .get(DRIVE_API)
            .query(&[
                (
                    "q",
                    &format!(
                        "name='{BACKUP_NAME}' and 'appDataFolder' in parents and trashed=false"
                    ),
                ),
                ("fields", &"files(id)".to_string()),
                ("spaces", &"appDataFolder".to_string()),
            ])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Drive search: {e}"))?,
    )
    .await?;

    let body: serde_json::Value = search.json().await.map_err(|e| format!("json: {e}"))?;
    let file_id = body["files"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|f| f["id"].as_str().map(String::from))
        .ok_or_else(|| "No backup found in Drive".to_string())?;

    let resp = check(
        client
            .get(format!("{DRIVE_API}/{file_id}?alt=media"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Drive download: {e}"))?,
    )
    .await?;

    let text = resp.text().await.map_err(|e| format!("Drive read: {e}"))?;
    restore_from_json(&text)
}

// ── Local export (browser download) ──────────────────────────────────────────

// Test-only observation point. Headless browsers give no reliable way to
// assert that a `download`-attributed anchor's `.click()` actually
// triggered a download, but the wasm `#[wasm_bindgen_test]` below must
// still be able to tell a real `local_export` run apart from
// cargo-mutants' "return true"/"return false"/inlined-`()` replacement
// variants — flipping this on the genuine path (right where the anchor is
// clicked, see `local_export`) gives the test a hard observable it can
// reset and assert across runs. `mutants::skip` keeps cargo-mutants from
// mutating the flag's own initializer into a spurious, unkillable miss.
#[cfg(test)]
#[cfg_attr(test, mutants::skip)]
pub(crate) static DID_RUN_DOWNLOAD_FLOW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Exports the CV backup as a browser download (Blob → object URL →
/// temporary `download`-attributed anchor appended to the body and
/// clicked, then removed; the object URL is revoked afterwards). Returns
/// `true` iff that full flow ran — i.e. the backup was serialized, a blob
/// URL was created, and an anchor was actually clicked — so the browser
/// test (and callers, if they care) can tell a completed download apart
/// from a silent no-op.
pub fn local_export(cv: &LifetimeCV, saved_sessions: &[TailoringSession]) -> bool {
    use js_sys::Array;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use web_sys::{Blob, Url};

    let json = build_backup(cv, saved_sessions);
    let arr = Array::new();
    arr.push(&JsValue::from_str(&json));

    if let Ok(blob) = Blob::new_with_str_sequence(&arr) {
        if let Ok(url) = Url::create_object_url_with_blob(&blob) {
            let window = web_sys::window().expect("no window");
            if let Some(doc) = window.document() {
                if let Ok(a) = doc.create_element("a") {
                    let _ = a.set_attribute("href", &url);
                    let _ = a.set_attribute("download", "cv_generator_backup.json");
                    if let Some(body) = doc.body() {
                        let _ = body.append_child(&a);
                        let mut clicked = false;
                        if let Some(el) = a.dyn_ref::<web_sys::HtmlElement>() {
                            el.click();
                            clicked = true;
                        }
                        let _ = body.remove_child(&a);
                        Url::revoke_object_url(&url).ok();
                        #[cfg(test)]
                        DID_RUN_DOWNLOAD_FLOW.store(true, std::sync::atomic::Ordering::SeqCst);
                        return clicked;
                    }
                }
            }
            Url::revoke_object_url(&url).ok();
        }
    }
    false
}

// ── WASM-only tests ──────────────────────────────────────────────────────────
//
// `local_export` above is `#[cfg(target_arch = "wasm32")]` and purely
// DOM-based (Blob + object URL + a temporary anchor's `.click()`) — no
// network involved, unlike `check`/`drive_backup`/`drive_restore` (see
// their `#[cfg_attr(test, mutants::skip)]` comments), so it's reachable
// here without a mock server.
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
    fn local_export_runs_the_full_download_flow() {
        // `local_export` now returns whether the whole flow "claimed to"
        // complete, and independently sets `DID_RUN_DOWNLOAD_FLOW` only on
        // the genuine path (after the anchor's `.click()`) — the two
        // together pin the end-to-end flow in a way the old smoke test
        // (and a plain `-> bool`) could not: cargo-mutants' "return true"
        // variant returns true without ever running the body, so the flag
        // stays unset and this test fails it, and "return false" and any
        // inner-statement deletion fail on the return/flag assertions too.
        // It still avoids asserting on the actual browser download
        // (headless Chromium's handling of a clicked `download` anchor is
        // not something to depend on), but pins every step up to and
        // including the click.
        let cv = LifetimeCV::default();
        DID_RUN_DOWNLOAD_FLOW.store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(
            local_export(&cv, &[]),
            "expected local_export to create a blob URL, append a download \
             anchor, and click it in the browser"
        );
        assert!(
            DID_RUN_DOWNLOAD_FLOW.load(std::sync::atomic::Ordering::SeqCst),
            "expected the download flow to actually reach the anchor click"
        );
    }
}
