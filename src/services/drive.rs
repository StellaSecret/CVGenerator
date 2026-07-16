use crate::models::LifetimeCV;
use serde::{Deserialize, Serialize};

// ── Backup payload ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct BackupData {
    pub version: u8,
    pub exported_at: i64,
    pub cv: LifetimeCV,
}

const BACKUP_VERSION: u8 = 1;
// DRIVE_SCOPE is documented for reference but only needed if you build a consent URL here.
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
#[cfg(target_arch = "wasm32")]
const DRIVE_API: &str = "https://www.googleapis.com/drive/v3/files";
#[cfg(target_arch = "wasm32")]
const UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3/files";
#[cfg(target_arch = "wasm32")]
const BACKUP_NAME: &str = "cv_generator_backup.json";

// ── Serialise / deserialise ───────────────────────────────────────────────────

pub fn build_backup(cv: &LifetimeCV) -> String {
    let data = BackupData {
        version: BACKUP_VERSION,
        exported_at: now_ms(),
        cv: cv.clone(),
    };
    serde_json::to_string_pretty(&data).expect("BackupData serialization failed")
}

pub fn restore_from_json(json: &str) -> Result<LifetimeCV, String> {
    let data: BackupData =
        serde_json::from_str(json).map_err(|e| format!("Invalid backup: {e}"))?;
    Ok(data.cv)
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
async fn check(resp: reqwest::Response) -> Result<reqwest::Response, String> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }
    Ok(resp)
}

// ── Drive: backup ─────────────────────────────────────────────────────────────

/// Upload the current CV to Google Drive `appDataFolder`.
/// Creates the file on first run; patches it on subsequent runs.
/// Returns the Drive file ID on success.
#[cfg(target_arch = "wasm32")]
pub async fn drive_backup(cv: &LifetimeCV, token: &str) -> Result<String, String> {
    let json = build_backup(cv);
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
pub async fn drive_backup(_cv: &LifetimeCV, _token: &str) -> Result<String, String> {
    Err("Drive backup is only available on web".to_string())
}

// ── Drive: restore ────────────────────────────────────────────────────────────

/// Download the backup from Google Drive `appDataFolder` and return the CV.
#[cfg(target_arch = "wasm32")]
pub async fn drive_restore(token: &str) -> Result<LifetimeCV, String> {
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
pub async fn drive_restore(_token: &str) -> Result<LifetimeCV, String> {
    Err("Drive restore is only available on web".to_string())
}

// ── Local export (browser download) ──────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
pub fn local_export(cv: &LifetimeCV) {
    use js_sys::Array;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use web_sys::{Blob, Url};

    let json = build_backup(cv);
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
pub fn local_export(_cv: &LifetimeCV) {}

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
    use crate::models::{LifetimeCV, PersonalInfo};

    fn sample_cv() -> LifetimeCV {
        LifetimeCV {
            personal: PersonalInfo {
                name: "Jane Smith".to_string(),
                email: "jane@example.com".to_string(),
                title: "Rust Engineer".to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn backup_roundtrip() {
        let cv = sample_cv();
        let json = build_backup(&cv);
        assert!(json.contains("Jane Smith"));
        assert!(json.contains("\"version\""));

        let restored = restore_from_json(&json).expect("restore failed");
        assert_eq!(restored.personal.name, "Jane Smith");
        assert_eq!(restored.personal.email, "jane@example.com");
    }

    #[test]
    fn backup_has_correct_version() {
        let cv = sample_cv();
        let json = build_backup(&cv);
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
    fn empty_cv_roundtrip() {
        let cv = LifetimeCV::default();
        let json = build_backup(&cv);
        let restored = restore_from_json(&json).unwrap();
        assert!(restored.personal.name.is_empty());
        assert!(restored.experiences.is_empty());
    }

    #[test]
    fn backup_contains_exported_at() {
        let cv = sample_cv();
        let json = build_backup(&cv);
        let data: BackupData = serde_json::from_str(&json).unwrap();
        assert!(data.exported_at >= 0);
    }
}
