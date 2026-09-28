//! Android-only native half of `drive.rs`: the same Google Drive
//! `appDataFolder` backup/restore calls as `drive_wasm.rs`, over `reqwest`
//! (rustls TLS — see Cargo.toml's android dependency block), so Google
//! sync works on Android exactly as it does on web. The access token
//! itself comes from the native Google Sign-In flow (see
//! `android_auth.rs`). Desktop targets keep the original "only available
//! on web" stubs in `drive.rs`; `local_export` (a browser download) has no
//! Android equivalent here and stays a stub returning `false`.
//!
//! Kept as a separate file from `drive_wasm.rs` (rather than widening its
//! cfg) so the wasm mutation-testing shard stays scoped to wasm-only code.

use super::{build_backup, drive_error_from_status, restore_from_json, RestoredData};
use crate::models::{LifetimeCV, TailoringSession};

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
