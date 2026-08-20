use crate::i18n;
use crate::router::Route;
use cv_generator::models::LifetimeCV;
use cv_generator::services::renderer::render_tailored_cv;
use cv_generator::services::score::ScoreMode;
use cv_generator::services::worker::{fetch_model_bytes_cached, EmbeddingWorker, WorkerStatus};
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
fn download_pdf(iframe_id: &str, filename: &str) {
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

fn score_color(score: u32) -> &'static str {
    if score >= 60 {
        "#16a34a"
    } else if score >= 30 {
        "#d97706"
    } else {
        "#dc2626"
    }
}

fn mode_label(mode: ScoreMode, l: i18n::Lang) -> &'static str {
    match mode {
        ScoreMode::Keyword => match l {
            i18n::Lang::Fr => "Mots-clés",
            _ => "Keywords",
        },
        ScoreMode::Embedding => match l {
            i18n::Lang::Fr => "Embeddings (sémantique)",
            _ => "Embeddings (semantic)",
        },
        ScoreMode::Hybrid => match l {
            i18n::Lang::Fr => "Hybride",
            _ => "Hybrid",
        },
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
    let mut score_mode = use_signal(|| ScoreMode::Keyword);
    let mut worker = use_signal(EmbeddingWorker::new);
    // Explicit model-loading state so Embedding/Hybrid mode can never
    // silently fall back to a broken/deflated result the way it used to
    // (see tailor_with_embeddings' doc comment): the UI now only allows
    // generating in those modes once the model has actually finished
    // loading, and shows the real state (idle / loading / ready / error)
    // rather than a mode description that describes wishful behavior.
    // Reuses the pre-existing `WorkerStatus` type from worker.rs (rather
    // than a separate parallel enum) since its variants already matched
    // exactly what this UI needed.
    let mut model_state = use_signal(|| WorkerStatus::Idle);
    // Raw per-experience/project scores from the last tailor run, purely
    // for inspection — see `ExperienceScoreDebug`'s doc comment in
    // matcher.rs for why this exists (distinguishing "the embedding
    // scoring is noise" from "selection logic is still wrong" requires
    // seeing the actual numbers, not just the final in/out list).
    let mut debug_scores =
        use_signal(Vec::<cv_generator::services::matcher::ExperienceScoreDebug>::new);
    let mut show_debug = use_signal(|| false);

    let has_cv = !cv.read().personal.name.is_empty();
    let jd_empty = jd_text.read().trim().is_empty();
    let is_generated = *generated.read();
    let score = *match_score.read();
    let color = score_color(score);
    let current_mode = *score_mode.read();

    let matched_label =
        i18n::tr("tl_matched", l).replace("{}", &matched_kws.read().len().to_string());
    let missing_label =
        i18n::tr("tl_missing", l).replace("{}", &missing_kws.read().len().to_string());
    let matched_list: Vec<String> = matched_kws.read().iter().take(20).cloned().collect();
    let missing_list: Vec<String> = missing_kws.read().iter().take(15).cloned().collect();
    let has_missing = !missing_kws.read().is_empty();

    // Precomputed, plain-string view of the raw score debug data — kept as
    // a flat Vec of already-formatted rows (rather than formatting inside
    // the rsx! loop below) to match this file's existing convention of
    // building display strings ahead of the render tree.
    struct DebugProjRow {
        line: String,
    }
    struct DebugExpRow {
        line: String,
        opacity: &'static str,
        projects: Vec<DebugProjRow>,
    }
    let debug_rows: Vec<DebugExpRow> = debug_scores
        .read()
        .iter()
        .map(|exp_dbg| {
            let mark = if exp_dbg.selected { "✓" } else { "✗" };
            let line = format!(
                "{mark} {} — {} — score: {:.4}",
                exp_dbg.company, exp_dbg.role, exp_dbg.score
            );
            let opacity = if exp_dbg.selected { "1.0" } else { "0.5" };
            let projects = exp_dbg
                .projects
                .iter()
                .map(|p| {
                    let pmark = if p.selected { "✓" } else { "✗" };
                    DebugProjRow {
                        line: format!("  {pmark} {} — score: {:.4}", p.name, p.score),
                    }
                })
                .collect();
            DebugExpRow {
                line,
                opacity,
                projects,
            }
        })
        .collect();

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
    let t_dl_hint = i18n::tr("pv_download_hint", l);
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

                            div { class: "field",
                                label { class: "label",
                                    match l {
                                        i18n::Lang::Fr => "Mode de correspondance",
                                        _ => "Matching mode",
                                    }
                                }
                                div { class: "mode-toggle",
                                    for mode in [ScoreMode::Keyword, ScoreMode::Embedding, ScoreMode::Hybrid] {
                                        button {
                                            class: if current_mode == mode { "mode-btn active" } else { "mode-btn" },
                                            onclick: move |_| { score_mode.set(mode); },
                                            "{mode_label(mode, l)}"
                                        }
                                    }
                                }
                                p { class: "hint",
                                    match current_mode {
                                        ScoreMode::Keyword => match l {
                                            i18n::Lang::Fr => "Correspondance par mots-clés avec TF-IDF et fuzzy matching",
                                            _ => "Keyword matching with TF-IDF and fuzzy matching",
                                        },
                                        ScoreMode::Embedding => match l {
                                            i18n::Lang::Fr => "Similarité sémantique via un petit modèle (all-MiniLM-L6-v2), en plus des mots-clés",
                                            _ => "Semantic similarity via a small local model (all-MiniLM-L6-v2), on top of keyword matching",
                                        },
                                        ScoreMode::Hybrid => match l {
                                            i18n::Lang::Fr => "Combinaison pondérée de mots-clés (60%) et similarité sémantique (40%)",
                                            _ => "Weighted blend of keywords (60%) and semantic similarity (40%)",
                                        },
                                    }
                                }

                                // Only Embedding/Hybrid need the model. Keyword mode
                                // needs nothing extra and this whole block is hidden.
                                if current_mode != ScoreMode::Keyword {
                                    div { class: "model-load-status",
                                        match &*model_state.read() {
                                            WorkerStatus::Idle => rsx! {
                                                p { class: "hint",
                                                    match l {
                                                        i18n::Lang::Fr => "Ce mode utilise un petit modèle (~25 Mo) intégré à l'application — aucune donnée n'est envoyée à un tiers.",
                                                        _ => "This mode uses a small model (~25MB) bundled with the app — nothing is sent to a third party.",
                                                    }
                                                }
                                                button {
                                                    class: "btn btn-secondary",
                                                    onclick: move |_| {
                                                        model_state.set(WorkerStatus::Loading);
                                                        spawn(async move {
                                                            // Split into two steps deliberately: fetching
                                                            // (~25MB, multi-second) happens with no
                                                            // EmbeddingWorker access at all, so no signal
                                                            // write() guard is held across that long await.
                                                            // Only the brief final construction step below
                                                            // touches `worker`. See fetch_model_bytes_cached's
                                                            // doc comment for why.
                                                            match fetch_model_bytes_cached().await {
                                                                Ok((model_bytes, config_json, tokenizer_json)) => {
                                                                    let load_result = worker
                                                                        .write()
                                                                        .load_model(&model_bytes, &config_json, &tokenizer_json)
                                                                        .await;
                                                                    match load_result {
                                                                        Ok(()) => model_state.set(WorkerStatus::Ready),
                                                                        Err(e) => model_state.set(WorkerStatus::Error(e)),
                                                                    }
                                                                }
                                                                Err(e) => model_state.set(WorkerStatus::Error(e)),
                                                            }
                                                        });
                                                    },
                                                    match l {
                                                        i18n::Lang::Fr => "Charger le modèle",
                                                        _ => "Load model",
                                                    }
                                                }
                                            },
                                            WorkerStatus::Loading => rsx! {
                                                p { class: "hint",
                                                    match l {
                                                        i18n::Lang::Fr => "Téléchargement du modèle en cours…",
                                                        _ => "Downloading model…",
                                                    }
                                                }
                                            },
                                            WorkerStatus::Ready => rsx! {
                                                p { class: "hint hint-success",
                                                    match l {
                                                        i18n::Lang::Fr => "Modèle chargé et prêt.",
                                                        _ => "Model loaded and ready.",
                                                    }
                                                }
                                            },
                                            WorkerStatus::Error(msg) => {
                                                let error_label = match l {
                                                    i18n::Lang::Fr => format!("Échec du chargement du modèle : {msg}"),
                                                    _ => format!("Model failed to load: {msg}"),
                                                };
                                                rsx! {
                                                    p { class: "hint hint-error", "{error_label}" }
                                                    button {
                                                        class: "btn btn-secondary",
                                                        onclick: move |_| {
                                                            model_state.set(WorkerStatus::Loading);
                                                            spawn(async move {
                                                                match fetch_model_bytes_cached().await {
                                                                    Ok((model_bytes, config_json, tokenizer_json)) => {
                                                                        let load_result = worker
                                                                            .write()
                                                                            .load_model(&model_bytes, &config_json, &tokenizer_json)
                                                                            .await;
                                                                        match load_result {
                                                                            Ok(()) => model_state.set(WorkerStatus::Ready),
                                                                            Err(e) => model_state.set(WorkerStatus::Error(e)),
                                                                        }
                                                                    }
                                                                    Err(e) => model_state.set(WorkerStatus::Error(e)),
                                                                }
                                                            });
                                                        },
                                                        match l {
                                                            i18n::Lang::Fr => "Réessayer",
                                                            _ => "Retry",
                                                        }
                                                    }
                                                }
                                            },
                                        }
                                    }
                                }
                            }

                            button {
                                class: "btn btn-primary btn-full",
                                // Previously: this button was only disabled
                                // when the JD text was empty, so selecting
                                // Embedding/Hybrid mode without a loaded
                                // model would silently generate a broken
                                // (Embedding) or deflated (Hybrid) result
                                // with no indication anything was wrong.
                                // Now: those modes also require
                                // WorkerStatus::Ready.
                                disabled: jd_empty || (current_mode != ScoreMode::Keyword && *model_state.read() != WorkerStatus::Ready),
                                onclick: move |_| {
                                    let mode = *score_mode.read();
                                    let jd_emb = if mode != ScoreMode::Keyword {
                                        let jd = jd_text.read().clone();
                                        // model_state gates the button itself now, so
                                        // reaching this point in Embedding/Hybrid mode
                                        // means the model is genuinely ready — this
                                        // is a defensive re-check, not the only guard.
                                        if worker.read().is_ready() {
                                            worker.write().embed_jd(&jd).ok()
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    // Route through EmbeddingWorker::tailor_with_embeddings
                                    // rather than building a fresh Scorer here: a
                                    // freshly-constructed Scorer's `engine` field
                                    // starts `None` and was never connected to
                                    // whatever model `worker` had loaded, so
                                    // Embedding/Hybrid mode silently scored
                                    // everything as 0.0 regardless of jd_emb. This
                                    // method temporarily moves worker's loaded
                                    // engine into the Scorer for the duration of
                                    // scoring, then hands it back.
                                    let result = worker
                                        .write()
                                        .tailor_with_embeddings(&cv.read(), &jd_text.read(), mode, jd_emb.as_deref());
                                    let html   = render_tailored_cv(&result.tailored, &job_title.read(), l);
                                    match_score.set((result.tailored.match_score * 100.0).round() as u32);
                                    matched_kws.set(result.tailored.matched_keywords.clone());
                                    missing_kws.set(result.tailored.missing_keywords.clone());
                                    debug_scores.set(result.debug_scores.clone());
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
                                button {
                                    class: "btn btn-secondary",
                                    onclick: move |_| { let cur = *show_debug.read(); show_debug.set(!cur); },
                                    if *show_debug.read() { "Hide score debug" } else { "Show score debug" }
                                }
                            }

                            if *show_debug.read() {
                                div {
                                    style: "margin: 1rem 0; padding: 1rem; border: 1px solid #444; border-radius: 8px; font-family: monospace; font-size: 0.85rem;",
                                    p { style: "margin-top: 0; font-weight: bold;", "Raw scores (mode: {mode_label(current_mode, l)})" }
                                    for exp_row in debug_rows.iter() {
                                        div {
                                            style: "margin-bottom: 0.5rem; opacity: {exp_row.opacity};",
                                            div { "{exp_row.line}" }
                                            for proj_row in exp_row.projects.iter() {
                                                div {
                                                    style: "margin-left: 1.5rem; opacity: 0.85;",
                                                    "{proj_row.line}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            p { class: "hint", "{t_dl_hint}" }

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
