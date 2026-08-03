use crate::i18n;
use crate::router::Route;
use cv_generator::models::LifetimeCV;
use cv_generator::services::renderer::render_lifetime_cv;
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
fn download_pdf(iframe_id: &str, filename: &str) {
    // Suggested filename for the browser's "Save as PDF" print destination
    // (it uses document.title, sans extension — the .pdf gets added
    // automatically).
    let title = filename.strip_suffix(".pdf").unwrap_or(filename);
    let js = format!(
        r#"(function(){{
        var f = document.getElementById('{iframe_id}');
        if (!f || !f.contentWindow) return;
        try {{
            if (f.contentDocument) {{ f.contentDocument.title = {title:?}; }}
        }} catch (e) {{}}
        f.contentWindow.focus();
        f.contentWindow.print();
    }})();"#
    );
    let _ = js_sys::eval(&js);
}

#[cfg(not(target_arch = "wasm32"))]
fn download_pdf(_iframe_id: &str, _filename: &str) {}

#[component]
pub fn CvPreview() -> Element {
    let cv: Signal<LifetimeCV> = use_context();
    let lang: Signal<i18n::Lang> = use_context();
    let l = *lang.read();
    let cv_ref = cv.read();
    let html = render_lifetime_cv(&cv_ref, l);
    let has_data = !cv_ref.personal.name.is_empty();
    let name = cv_ref.personal.name.clone();

    let t_nav_back = i18n::tr("nav_back", l);
    let t_pv_edit = i18n::tr("pv_edit_cv", l);
    let t_title = i18n::tr("pv_title", l);
    let t_subtitle = i18n::tr("pv_subtitle", l);
    let t_download = i18n::tr("pv_download", l);
    let t_download_hint = i18n::tr("pv_download_hint", l);
    let t_empty = i18n::tr("pv_empty", l);
    let t_fill = i18n::tr("pv_fill_first", l);

    rsx! {
        div { class: "page",
            div { class: "page-back-row",
                Link { to: Route::Home {},     class: "page-back-link", "{t_nav_back}" }
                Link { to: Route::CvEditor {}, class: "page-back-link", "{t_pv_edit}" }
            }
            div { class: "page-header row-between",
                div {
                    h1 { "{t_title}" }
                    p { class: "subtitle", "{t_subtitle}" }
                }
                div { class: "header-actions",
                    if has_data {
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| {
                                let fname = if name.is_empty() {
                                    "cv.pdf".to_string()
                                } else {
                                    format!("{}-cv.pdf", name.to_lowercase().replace(' ', "-"))
                                };
                                download_pdf("cv-preview-frame", &fname);
                            },
                            "{t_download}"
                        }
                    }
                }
            }
            if has_data {
                p { class: "hint", "{t_download_hint}" }
            }

            if !has_data {
                div { class: "empty-state",
                    p { "{t_empty}" }
                    Link { to: Route::CvEditor {}, "{t_fill}" }
                }
            } else {
                iframe {
                    id: "cv-preview-frame",
                    class: "cv-iframe",
                    srcdoc: "{html}",
                }
            }
        }
    }
}
