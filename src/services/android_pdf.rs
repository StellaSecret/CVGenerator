//! JNI bridge to the native Kotlin `PdfExporter` object in
//! `android/overlay/app/src/main/java/com/stellasecret/cvgenerator/PdfExporter.kt`.
//!
//! Android's WebView implements no `window.print()`, so the web path's
//! browser print-to-PDF cannot run there. Instead, `views::cv_preview` and
//! `views::tailor` hand the already-rendered CV HTML to [`export_pdf`],
//! which asks Kotlin to load that HTML into a dedicated off-screen WebView
//! and open the system print dialog — its "Save as PDF" destination
//! produces the same real-text-layer PDF the browser produces on the web.
//!
//! Mirrors `android_auth.rs`'s JVM wiring: the JVM handle comes from
//! `android_auth::jvm()`, populated once at startup by
//! `GoogleDriveHelper.nativeInit()`.

use jni::objects::{JObject, JValue};

/// Asks the Kotlin `PdfExporter` to print the given CV HTML. `filename`
/// suggests the printed document's name (the "Save as PDF" destination
/// uses it, minus any `.pdf` suffix, as the saved file name).
pub fn export_pdf(html: &str, filename: &str) {
    let Some(jvm) = super::android_auth::jvm() else {
        eprintln!("[android_pdf] JVM not initialized — was nativeInit() called?");
        return;
    };
    let Ok(mut env) = jvm.attach_current_thread() else {
        eprintln!("[android_pdf] FAILED to attach thread");
        return;
    };
    let html_obj = match env.new_string(html).map(JObject::from) {
        Ok(obj) => obj,
        Err(e) => {
            eprintln!("[android_pdf] failed to allocate html string: {e:?}");
            return;
        }
    };
    let name_obj = match env.new_string(filename).map(JObject::from) {
        Ok(obj) => obj,
        Err(e) => {
            eprintln!("[android_pdf] failed to allocate filename string: {e:?}");
            return;
        }
    };
    if let Err(e) = env.call_static_method(
        "com/stellasecret/cvgenerator/PdfExporter",
        "exportPdf",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&html_obj), JValue::Object(&name_obj)],
    ) {
        // Clear the pending Java exception so the next JNI call doesn't crash.
        let _ = env.exception_clear();
        eprintln!("[android_pdf] exportPdf JNI call FAILED: {e:?}");
    } else {
        eprintln!("[android_pdf] exportPdf JNI call succeeded");
    }
}
