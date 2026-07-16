use crate::i18n;
use crate::router::Route;
use cv_generator::models::LifetimeCV;
use cv_generator::services::matcher::tailor_cv;
use cv_generator::services::renderer::render_tailored_cv;
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
fn download_pdf(iframe_id: &str, filename: &str) {
    let js = format!(
        r#"(function(){{
        var f = document.getElementById('{iframe_id}');
        if (!f || !f.contentDocument || !f.contentDocument.body) {{
            if (f && f.contentWindow) f.contentWindow.print();
            return;
        }}
        if (typeof html2pdf === 'undefined') {{
            if (f && f.contentWindow) f.contentWindow.print();
            return;
        }}
        var el = f.contentDocument.body.cloneNode(true);
        html2pdf().set({{
            margin: [10, 10, 10, 10],
            filename: '{filename}',
            image: {{ type: 'jpeg', quality: 0.98 }},
            html2canvas: {{ scale: 2, useCORS: true, allowTaint: true }},
            jsPDF: {{ unit: 'mm', format: 'a4', orientation: 'portrait' }}
        }}).from(el).save();
    }})();"#
    );
    let _ = js_sys::eval(&js);
}

#[cfg(not(target_arch = "wasm32"))]
fn download_pdf(_iframe_id: &str, _filename: &str) {}

fn score_color(score: u32) -> &'static str {
    if score >= 60 {
        "#16a34a"
    } else if score >= 30 {
        "#d97706"
    } else {
        "#dc2626"
    }
}

#[component]
pub fn Tailor() -> Element {
    let cv: Signal<LifetimeCV> = use_context();
    let lang: Signal<i18n::Lang> = use_context();
    let l = *lang.read();

    let mut jd_text = use_signal(String::new);
    let mut job_title = use_signal(String::new);
    let mut result_html = use_signal(String::new);
    let mut match_score = use_signal(|| 0u32);
    let mut matched_kws = use_signal(Vec::<String>::new);
    let mut missing_kws = use_signal(Vec::<String>::new);
    let mut generated = use_signal(|| false);

    let has_cv = !cv.read().personal.name.is_empty();
    let jd_empty = jd_text.read().trim().is_empty();
    let is_generated = *generated.read();
    let score = *match_score.read();
    let color = score_color(score);

    let matched_label =
        i18n::tr("tl_matched", l).replace("{}", &matched_kws.read().len().to_string());
    let missing_label =
        i18n::tr("tl_missing", l).replace("{}", &missing_kws.read().len().to_string());
    let matched_list: Vec<String> = matched_kws.read().iter().take(20).cloned().collect();
    let missing_list: Vec<String> = missing_kws.read().iter().take(15).cloned().collect();
    let has_missing = !missing_kws.read().is_empty();

    let t_nav = i18n::tr("nav_back", l);
    let t_full = i18n::tr("tl_full_cv", l);
    let t_title = i18n::tr("tl_title", l);
    let t_sub = i18n::tr("tl_subtitle", l);
    let t_empty = i18n::tr("tl_empty", l);
    let t_build = i18n::tr("tl_build_cv", l);
    let t_jt_lbl = i18n::tr("tl_job_title", l);
    let t_jd_lbl = i18n::tr("tl_jd_label", l);
    let t_gen = i18n::tr("tl_generate", l);
    let t_match = i18n::tr("tl_match", l);
    let t_dl = i18n::tr("tl_download", l);
    let t_ph = i18n::tr("tl_placeholder", l);

    rsx! {
        div { class: "page",
            div { class: "page-back-row",
                Link { to: Route::Home {},     class: "page-back-link", "{t_nav}" }
                Link { to: Route::CvPreview {}, class: "page-back-link", "{t_full}" }
            }
            div { class: "page-header",
                h1 { "{t_title}" }
                p { class: "subtitle", "{t_sub}" }
            }

            if !has_cv {
                div { class: "empty-state",
                    p { "{t_empty}" }
                    Link { to: Route::CvEditor {}, "{t_build}" }
                }
            } else {
                div { class: "tailor-layout",
                    div { class: "tailor-input",
                        div { class: "form-section",
                            div { class: "field",
                                label { class: "label", "{t_jt_lbl}" }
                                input {
                                    r#type: "text", class: "input",
                                    placeholder: "Senior Rust Engineer at Acme",
                                    value: job_title.read().clone(),
                                    oninput: move |e| { job_title.set(e.value()); },
                                }
                            }
                            div { class: "field",
                                label { class: "label", "{t_jd_lbl}" }
                                textarea {
                                    class: "input textarea jd-textarea", rows: "18",
                                    placeholder: "Paste the complete job posting here…",
                                    value: jd_text.read().clone(),
                                    oninput: move |e| { jd_text.set(e.value()); },
                                }
                            }
                            button {
                                class: "btn btn-primary btn-full",
                                disabled: jd_empty,
                                onclick: move |_| {
                                    let result = tailor_cv(&cv.read(), &jd_text.read());
                                    let html   = render_tailored_cv(&result.tailored, &job_title.read(), l);
                                    match_score.set((result.tailored.match_score * 100.0).round() as u32);
                                    matched_kws.set(result.tailored.matched_keywords.clone());
                                    missing_kws.set(result.tailored.missing_keywords.clone());
                                    result_html.set(html);
                                    generated.set(true);
                                },
                                "{t_gen}"
                            }
                        }
                    }

                    div { class: "tailor-output",
                        if is_generated {
                            div { class: "score-banner",
                                div { class: "score-left",
                                    div {
                                        class: "score-circle",
                                        style: "border-color: {color}",
                                        span { class: "score-number", style: "color: {color}", "{score}%" }
                                        span { class: "score-label", "{t_match}" }
                                    }
                                }
                                div { class: "score-right",
                                    div { class: "kw-section",
                                        div { class: "kw-label kw-ok", "{matched_label}" }
                                        div { class: "kw-cloud",
                                            for kw in matched_list {
                                                span { class: "tag tag-matched", "{kw}" }
                                            }
                                        }
                                    }
                                    if has_missing {
                                        div { class: "kw-section",
                                            div { class: "kw-label kw-miss", "{missing_label}" }
                                            div { class: "kw-cloud",
                                                for kw in missing_list {
                                                    span { class: "tag tag-missing", "{kw}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "output-actions",
                                button {
                                    class: "btn btn-primary",
                                    onclick: move |_| { download_pdf("cv-tailor-frame", "tailored-cv.pdf"); },
                                    "{t_dl}"
                                }
                            }

                            iframe {
                                id: "cv-tailor-frame",
                                class: "cv-iframe cv-iframe-tall",
                                srcdoc: result_html.read().clone(),
                            }
                        } else {
                            div { class: "output-placeholder",
                                div { class: "placeholder-icon", "📄" }
                                p { "{t_ph}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
