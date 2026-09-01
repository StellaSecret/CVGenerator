use crate::models::LifetimeCV;

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
