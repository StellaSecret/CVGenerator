use crate::models::{LifetimeCV, TailoringSession};
use serde::{Deserialize, Serialize};

// ── Backup payload ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct BackupData {
    pub version: u8,
    pub exported_at: i64,
    pub cv: LifetimeCV,
    /// Application-tracking sessions (the saved tailoring sessions).
    /// `#[serde(default)]` keeps v1 backups (which predate this field)
    /// fully restorable.
    #[serde(default)]
    pub saved_sessions: Vec<TailoringSession>,
}

const BACKUP_VERSION: u8 = 2;
// The Google Drive OAuth scope string lives hard-coded in auth.rs
// ("https://www.googleapis.com/auth/drive.appdata"), so this file needs no
// scope constant of its own.
#[cfg(target_arch = "wasm32")]
const DRIVE_API: &str = "https://www.googleapis.com/drive/v3/files";
#[cfg(target_arch = "wasm32")]
const UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3/files";
#[cfg(target_arch = "wasm32")]
const BACKUP_NAME: &str = "cv_generator_backup.json";

// ── Serialise / deserialise ───────────────────────────────────────────────────

pub fn build_backup(cv: &LifetimeCV, saved_sessions: &[TailoringSession]) -> String {
    let data = BackupData {
        version: BACKUP_VERSION,
        exported_at: now_ms(),
        cv: cv.clone(),
        saved_sessions: saved_sessions.to_vec(),
    };
    // `.expect()` here (rather than propagating a `Result`, as this file's
    // Drive-facing functions below do) is deliberate, not an oversight:
    // `serde_json::to_string_pretty` can only fail on a plain derived-
    // `Serialize` struct tree like `BackupData` if it contains a non-finite
    // float or a non-string map key. The only `f32` reachable from here is
    // `TailoringSession::match_score`/`TailoredCV::match_score`, and both
    // are computed exclusively through `matcher.rs`'s `weight_total > 0.0`-
    // guarded division (see `mean_score`/`mean_skill_score`/`match_score`
    // in matcher/mod.rs), so it can never be NaN — this really is
    // infallible in practice, not just assumed to be.
    serde_json::to_string_pretty(&data).expect("BackupData serialization failed")
}

#[derive(Debug, Clone)]
pub struct RestoredData {
    pub cv: LifetimeCV,
    pub saved_sessions: Vec<TailoringSession>,
}

pub fn restore_from_json(json: &str) -> Result<RestoredData, String> {
    let data: BackupData =
        serde_json::from_str(json).map_err(|e| format!("Invalid backup: {e}"))?;
    let mut cv = data.cv;
    cv.backfill_project_ids();
    Ok(RestoredData {
        cv,
        saved_sessions: data.saved_sessions,
    })
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

/// Triage a Drive HTTP status into the error to surface, or `None` for
/// success. Kept free of `reqwest` types (a wasm-only dependency) so it
/// compiles and is unit-testable on native, where the async `check` wrapper
/// can't run.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn drive_error_from_status(status: u16, body: &str) -> Option<String> {
    if (200..400).contains(&status) {
        return None;
    }
    if status == 401 {
        // Stored token refused: it's dead. Scrub it so the UI drops back
        // to a clean signed-out state and the user can re-auth, instead of
        // hitting the same 401 forever.
        crate::services::auth::clear_token();
        return Some(crate::services::auth::AUTH_EXPIRED_ERR.to_string());
    }
    Some(format!("HTTP {status}: {body}"))
}

#[cfg(target_arch = "wasm32")]
// Takes a `reqwest::Response`, which — unlike `web_sys::Response` in
// worker.rs — has no public constructor for a synthetic instance outside
// an actual `reqwest::Client` request/response round trip, so there's no
// equivalent to worker.rs's `synthetic_response` helper available here
// without either a real network call or a mock HTTP server. Skipped for
// that reason, same as `drive_backup`/`drive_restore` below (its only
// callers), all of which hit the real Google Drive API.
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
#[cfg(target_arch = "wasm32")]
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

#[cfg(not(target_arch = "wasm32"))]
pub async fn drive_backup(
    _cv: &LifetimeCV,
    _saved_sessions: &[TailoringSession],
    _token: &str,
) -> Result<String, String> {
    Err("Drive backup is only available on web".to_string())
}

// ── Drive: restore ────────────────────────────────────────────────────────────

/// Download the backup from Google Drive `appDataFolder` and return the CV.
#[cfg(target_arch = "wasm32")]
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

#[cfg(not(target_arch = "wasm32"))]
pub async fn drive_restore(_token: &str) -> Result<RestoredData, String> {
    Err("Drive restore is only available on web".to_string())
}

// ── Local export (browser download) ──────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
pub fn local_export(cv: &LifetimeCV, saved_sessions: &[TailoringSession]) {
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
                        if let Some(el) = a.dyn_ref::<web_sys::HtmlElement>() {
                            el.click();
                        }
                        let _ = body.remove_child(&a);
                    }
                }
            }
            Url::revoke_object_url(&url).ok();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn local_export(_cv: &LifetimeCV, _saved_sessions: &[TailoringSession]) {}

// ── Time helper ───────────────────────────────────────────────────────────────

fn now_ms() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as i64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LifetimeCV, LocalizedText, PersonalInfo};

    fn sample_cv() -> LifetimeCV {
        LifetimeCV {
            personal: PersonalInfo {
                name: "Jane Smith".to_string(),
                email: "jane@example.com".to_string(),
                title: LocalizedText::same("Rust Engineer"),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Minimal, dependency-free executor for the native `drive_*` stubs in
    /// this module during tests (they return an error immediately on first
    /// poll — no real async I/O — so a no-op waker is sufficient). Mirrors
    /// the same helper in worker.rs.
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

    #[test]
    fn backup_roundtrip() {
        let cv = sample_cv();
        let json = build_backup(&cv, &[]);
        assert!(json.contains("Jane Smith"));
        assert!(json.contains("\"version\""));

        let restored = restore_from_json(&json).expect("restore failed");
        assert_eq!(restored.cv.personal.name, "Jane Smith");
        assert_eq!(restored.cv.personal.email, "jane@example.com");
    }

    #[test]
    fn backup_roundtrips_saved_sessions() {
        let cv = sample_cv();
        let session = TailoringSession {
            id: "s1".to_string(),
            name: "Acme".to_string(),
            job_title: "Platform Engineer".to_string(),
            match_score: 0.87,
            date_applied: "2026-09-11".to_string(),
            status: crate::models::ApplicationStatus::Interviewing,
            ..Default::default()
        };
        let json = build_backup(&cv, &[session]);
        let restored = restore_from_json(&json).expect("restore failed");
        assert_eq!(restored.saved_sessions.len(), 1);
        assert_eq!(restored.saved_sessions[0].name, "Acme");
        assert_eq!(
            restored.saved_sessions[0].status,
            crate::models::ApplicationStatus::Interviewing
        );
        assert_eq!(restored.saved_sessions[0].match_score, 0.87);
        assert_eq!(restored.saved_sessions[0].date_applied, "2026-09-11");
    }

    #[test]
    fn backup_has_correct_version() {
        let cv = sample_cv();
        let json = build_backup(&cv, &[]);
        let data: BackupData = serde_json::from_str(&json).unwrap();
        assert_eq!(data.version, BACKUP_VERSION);
    }

    #[test]
    fn restore_rejects_invalid_json() {
        let err = restore_from_json("not json at all").unwrap_err();
        assert!(err.contains("Invalid backup"));
    }

    #[test]
    fn restore_rejects_missing_fields() {
        let err = restore_from_json(r#"{"version":1}"#).unwrap_err();
        assert!(err.contains("Invalid backup"));
    }

    #[test]
    fn v1_backup_without_sessions_still_restores() {
        // Build a real v2 file, then strip the sessions field and set
        // version=1 to reproduce exactly what a pre-tracking backup holds.
        let cv = sample_cv();
        let v2: serde_json::Value = serde_json::from_str(&build_backup(
            &cv,
            &[crate::models::TailoringSession {
                id: "s1".to_string(),
                name: "Acme".to_string(),
                ..Default::default()
            }],
        ))
        .unwrap();
        let mut v1 = v2.as_object().unwrap().clone();
        v1.remove("saved_sessions");
        v1.insert("version".to_string(), serde_json::json!(1));
        let json = serde_json::to_string(&serde_json::Value::Object(v1)).unwrap();

        let restored = restore_from_json(&json).expect("v1 backup must restore");
        assert_eq!(restored.cv.personal.name, "Jane Smith");
        assert!(restored.saved_sessions.is_empty());
    }

    #[test]
    fn empty_cv_roundtrip() {
        let cv = LifetimeCV::default();
        let json = build_backup(&cv, &[]);
        let restored = restore_from_json(&json).unwrap();
        assert!(restored.cv.personal.name.is_empty());
        assert!(restored.cv.experiences.is_empty());
    }

    #[test]
    fn backup_contains_exported_at() {
        let cv = sample_cv();
        let json = build_backup(&cv, &[]);
        let data: BackupData = serde_json::from_str(&json).unwrap();
        assert!(data.exported_at >= 0);
    }

    #[test]
    fn drive_backup_unavailable_on_native() {
        let res = block_on(drive_backup(&sample_cv(), &[], "fake-token"));
        assert!(
            res.is_err(),
            "native stub must report Drive backup as web-only, got {res:?}"
        );
    }

    #[test]
    fn drive_restore_unavailable_on_native() {
        let res = block_on(drive_restore("fake-token"));
        assert!(
            res.is_err(),
            "native stub must report Drive restore as web-only, got {res:?}"
        );
    }

    #[test]
    fn now_ms_is_ms_since_epoch() {
        let now = now_ms();
        assert!(
            now > 60_000,
            "now_ms must return real epoch milliseconds, got {now}"
        );
    }

    #[test]
    fn drive_error_marks_401_as_expired() {
        // The mutated `status == 401` → `status != 401` branch would answer
        // a generic "HTTP 401" message instead of the expiry marker.
        assert_eq!(
            drive_error_from_status(401, "Token expired"),
            Some(crate::services::auth::AUTH_EXPIRED_ERR.to_string())
        );
    }

    #[test]
    fn drive_error_accepts_success_statuses() {
        assert_eq!(drive_error_from_status(200, ""), None, "200 must pass");
        assert_eq!(drive_error_from_status(299, "x"), None, "299 must pass");
    }

    #[test]
    fn drive_error_reports_other_statuses() {
        let err = drive_error_from_status(404, "gone").expect("404 must error");
        assert!(err.contains("404"), "got {err}");
        assert!(err.contains("gone"), "got {err}");
    }
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
    fn local_export_runs_without_panicking() {
        // Deliberately a smoke test, not a full behavioral one: verifying
        // the download actually happened would mean intercepting
        // `Blob`/`URL.createObjectURL`/the anchor's `.click()`, and a
        // headless-Chrome `.click()` on a `download`-attributed anchor may
        // or may not be observable depending on the test runner's download
        // handling — not something to depend on here. This still catches
        // gross breakage (e.g. a panic from a bad `.expect()` on
        // `web_sys::window()`), just not the "replace local_export with
        // ()" mutant specifically.
        let cv = LifetimeCV::default();
        local_export(&cv, &[]);
    }
}
