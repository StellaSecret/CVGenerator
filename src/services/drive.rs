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
//
// The real (wasm-only) Google Drive API calls (`drive_backup`/
// `drive_restore`), the browser-download `local_export`, and
// `local_export`'s `#[wasm_bindgen_test]`s live in `drive_wasm.rs`
// (declared under `#[cfg(target_arch = "wasm32")]`) — kept in its own
// module so the wasm mutation-testing shard in
// `.github/workflows/mutants.yml` can target a file whose every function
// is either wasm-testable or `#[cfg_attr(test, mutants::skip)]`-attributed
// (native `#[test]`s don't execute under the wasm harness, so a mixed
// file would report mass false misses). On wasm, the names are re-exported
// below, so this module's public API is unchanged.

#[cfg(target_arch = "wasm32")]
mod drive_wasm;

#[cfg(target_arch = "wasm32")]
pub use drive_wasm::{drive_backup, drive_restore, local_export};

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

// ── Drive: backup ─────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub async fn drive_backup(
    _cv: &LifetimeCV,
    _saved_sessions: &[TailoringSession],
    _token: &str,
) -> Result<String, String> {
    Err("Drive backup is only available on web".to_string())
}

// ── Drive: restore ────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub async fn drive_restore(_token: &str) -> Result<RestoredData, String> {
    Err("Drive restore is only available on web".to_string())
}

// ── Local export (browser download) ──────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub fn local_export(_cv: &LifetimeCV, _saved_sessions: &[TailoringSession]) -> bool {
    false
}

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
    fn local_export_unavailable_on_native() {
        assert!(
            !local_export(&sample_cv(), &[]),
            "native stub must report local export as web-only"
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
