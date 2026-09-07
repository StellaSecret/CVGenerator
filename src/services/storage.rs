use crate::models::{LifetimeCV, TailoringSession};

// ── Web (WASM) ────────────────────────────────────────────────────────────────
// CV_KEY is scoped to this block so it doesn't trigger dead_code on native.

#[cfg(target_arch = "wasm32")]
const CV_KEY: &str = "cv_generator_lifetime_cv";

#[cfg(target_arch = "wasm32")]
pub fn save_cv(cv: &LifetimeCV) {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::set(CV_KEY, cv).expect("Failed to persist CV to localStorage");
}

#[cfg(target_arch = "wasm32")]
pub fn load_cv() -> Option<LifetimeCV> {
    use gloo_storage::{LocalStorage, Storage};
    let mut cv: LifetimeCV = LocalStorage::get(CV_KEY).ok()?;
    cv.backfill_project_ids();
    Some(cv)
}

#[cfg(target_arch = "wasm32")]
pub fn clear_cv() {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::delete(CV_KEY);
}

// ── Tailoring sessions (web) ───────────────────────────────────────────────────
// Two separate keys, not one: the "current session" auto-saves continuously
// as the person types/ticks checkboxes (so a reload never loses in-progress
// work), while "saved sessions" is a named list only touched by an explicit
// Save/Delete action — see TailoringSession's doc comment for why these
// are kept apart rather than folding the current session into the list.

#[cfg(target_arch = "wasm32")]
const CURRENT_SESSION_KEY: &str = "cv_generator_current_session";
#[cfg(target_arch = "wasm32")]
const SAVED_SESSIONS_KEY: &str = "cv_generator_saved_sessions";

#[cfg(target_arch = "wasm32")]
pub fn save_current_session(session: &TailoringSession) {
    use gloo_storage::{LocalStorage, Storage};
    let _ = LocalStorage::set(CURRENT_SESSION_KEY, session);
}

#[cfg(target_arch = "wasm32")]
pub fn load_current_session() -> Option<TailoringSession> {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::get(CURRENT_SESSION_KEY).ok()
}

#[cfg(target_arch = "wasm32")]
pub fn save_sessions_list(sessions: &[TailoringSession]) {
    use gloo_storage::{LocalStorage, Storage};
    let _ = LocalStorage::set(SAVED_SESSIONS_KEY, sessions);
}

#[cfg(target_arch = "wasm32")]
pub fn load_sessions_list() -> Vec<TailoringSession> {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::get(SAVED_SESSIONS_KEY).unwrap_or_default()
}

// ── Mobile / Desktop (non-WASM) ───────────────────────────────────────────────
// Swapped in when building with --platform android or desktop.
// Uses a JSON file in the app data directory.

#[cfg(not(target_arch = "wasm32"))]
fn data_path() -> std::path::PathBuf {
    // Dioxus mobile exposes dirs via dioxus_desktop::tao / platform APIs;
    // for now we fall back to the current directory so the project compiles
    // everywhere. In production, swap this for the platform data dir.
    std::env::current_dir()
        .unwrap_or_default()
        .join("cv_data.json")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_cv(cv: &LifetimeCV) {
    let json = serde_json::to_string_pretty(cv).expect("serialisation failed");
    std::fs::write(data_path(), json).expect("failed to write CV file");
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_cv() -> Option<LifetimeCV> {
    let json = std::fs::read_to_string(data_path()).ok()?;
    let mut cv: LifetimeCV = serde_json::from_str(&json).ok()?;
    cv.backfill_project_ids();
    Some(cv)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_cv() {
    let _ = std::fs::remove_file(data_path());
}

// ── Tailoring sessions (native) ────────────────────────────────────────────────
// Same rationale as the wasm versions above — see TailoringSession's doc
// comment and the wasm `save_current_session`/`save_sessions_list` comment.

#[cfg(not(target_arch = "wasm32"))]
fn current_session_path() -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("cv_current_session.json")
}

#[cfg(not(target_arch = "wasm32"))]
fn saved_sessions_path() -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("cv_saved_sessions.json")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_current_session(session: &TailoringSession) {
    if let Ok(json) = serde_json::to_string_pretty(session) {
        let _ = std::fs::write(current_session_path(), json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_current_session() -> Option<TailoringSession> {
    let json = std::fs::read_to_string(current_session_path()).ok()?;
    serde_json::from_str(&json).ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_sessions_list(sessions: &[TailoringSession]) {
    if let Ok(json) = serde_json::to_string_pretty(sessions) {
        let _ = std::fs::write(saved_sessions_path(), json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_sessions_list() -> Vec<TailoringSession> {
    let Ok(json) = std::fs::read_to_string(saved_sessions_path()) else {
        return Vec::new();
    };
    serde_json::from_str(&json).unwrap_or_default()
}
