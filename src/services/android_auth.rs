//! JNI bridge to the native Kotlin `GoogleDriveHelper` object in
//! `android/overlay/app/src/main/java/com/stellasecret/cvgenerator/GoogleDriveHelper.kt`.
//!
//! Google disallows running its OAuth consent screen inside an embedded
//! WebView — which is how this app renders everywhere else on Android — so
//! `auth::start_oauth()`'s usual "open a URL" approach doesn't work here.
//! Instead, on Android, sign-in goes entirely through Google Play Services'
//! native Sign-In SDK, driven from Kotlin, with this module as the glue:
//!
//! 1. Kotlin's `GoogleDriveHelper.init()` calls `nativeInit()` below, once,
//!    at app startup, handing us the JVM handle and the app's private
//!    files directory (where the token gets written).
//! 2. `auth::start_oauth()` calls `start_sign_in()` below, which calls back
//!    into `GoogleDriveHelper.startSignIn()` over JNI to launch the actual
//!    native sign-in UI (a system account picker, not a WebView).
//! 3. Once Kotlin has a token, it writes it straight to a file in that
//!    same files directory (`auth::get_token()`/`set_token()` read and
//!    write that same file — see `auth.rs`'s `token_path()`) and calls
//!    `nativeOnTokenSaved()` below, which just flips `TOKEN_SAVED` so the
//!    UI's poll loop (in `views::sync`) notices and re-reads the token
//!    file, rather than trying to marshal the token string itself across
//!    the JNI boundary.
//!
//! Ported from the equivalent module in the PeopleModeler project, which
//! has this working in production on both web and Android; adjusted only
//! for CVGenerator's package name and its lib/bin crate split (this lives
//! in the lib crate so it's compiled into the same Android `.so` as the
//! rest of `services`, and its public items are reachable from the bin
//! crate's `views::sync` as `cv_generator::services::android_auth::...`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static JVM: OnceLock<jni::JavaVM> = OnceLock::new();
static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Set to true by the JNI callback when the token file is written. Public
/// (not `pub(crate)`) because `views::sync`, in the bin crate, polls it
/// directly to know when to re-read the token file — see that module's
/// `#[cfg(target_os = "android")]` block.
pub static TOKEN_SAVED: AtomicBool = AtomicBool::new(false);

/// Returns the app's internal files directory, used for token storage.
pub(crate) fn get_files_dir() -> Option<&'static std::path::Path> {
    FILES_DIR.get().map(|p| p.as_path())
}

/// Called from Kotlin `GoogleDriveHelper.nativeInit()` to store the JVM
/// reference and files dir.
#[no_mangle]
pub extern "system" fn Java_com_stellasecret_cvgenerator_GoogleDriveHelper_nativeInit(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    files_dir: jni::objects::JString,
) {
    if let Ok(jvm) = env.get_java_vm() {
        JVM.set(jvm).ok();
        eprintln!("[android_auth] JVM stored");
    } else {
        eprintln!("[android_auth] FAILED to get JVM from env");
    }
    if let Ok(path) = env.get_string(&files_dir) {
        let path_str: String = path.into();
        FILES_DIR.set(PathBuf::from(path_str.clone())).ok();
        eprintln!("[android_auth] filesDir: {path_str}");
    } else {
        eprintln!("[android_auth] FAILED to get filesDir string");
    }
}

/// Called from Kotlin `GoogleDriveHelper.nativeOnTokenSaved()` after the
/// token file is written. Sets a global flag so the UI can pick up the
/// change on its next poll.
#[no_mangle]
pub extern "system" fn Java_com_stellasecret_cvgenerator_GoogleDriveHelper_nativeOnTokenSaved(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) {
    eprintln!("[android_auth] token saved callback received");
    TOKEN_SAVED.store(true, Ordering::Release);
}

/// Called from `auth::start_oauth()` to trigger native Google Sign-In.
pub fn start_sign_in() {
    eprintln!("[android_auth] start_sign_in called");
    match JVM.get() {
        Some(jvm) => {
            eprintln!("[android_auth] JVM found, attaching thread");
            match jvm.attach_current_thread() {
                Ok(mut env) => {
                    eprintln!(
                        "[android_auth] thread attached, calling GoogleDriveHelper.startSignIn"
                    );
                    match env.call_static_method(
                        "com/stellasecret/cvgenerator/GoogleDriveHelper",
                        "startSignIn",
                        "()V",
                        &[],
                    ) {
                        Ok(_) => eprintln!("[android_auth] startSignIn JNI call succeeded"),
                        Err(e) => {
                            // Clear pending Java exception so next JNI call doesn't crash
                            let _ = env.exception_clear();
                            eprintln!("[android_auth] startSignIn JNI call FAILED: {e:?}");
                        }
                    }
                }
                Err(e) => eprintln!("[android_auth] FAILED to attach thread: {e:?}"),
            }
        }
        None => eprintln!("[android_auth] JVM not initialized — was nativeInit() called?"),
    }
}
