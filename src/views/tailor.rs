use crate::i18n;
use crate::router::Route;
use cv_generator::models::LifetimeCV;
use cv_generator::services::matcher::apply_manual_project_selection;
use cv_generator::services::renderer::render_tailored_cv;
use cv_generator::services::score::ScoreMode;
use cv_generator::services::worker::{fetch_model_bytes_cached, EmbeddingWorker, WorkerStatus};
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

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

fn localized(t: &cv_generator::models::LocalizedText) -> &str {
    if !t.fr.is_empty() {
        &t.fr
    } else {
        &t.en
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

    // Manual project-selection override. `checked_project_ids` starts as
    // whatever the automatic pass selected (initialized right after each
    // "Générer" run), then the person can tick/untick individual projects
    // — including re-including one the algorithm excluded entirely, since
    // this reads from `cv` (the full CV), not the already-filtered result.
    // Applied via an explicit "Apply selection" button rather than
    // live-updating on every checkbox click, matching how the rest of
    // this view already works (one explicit "Générer" action, not
    // continuous re-render on every keystroke/change).
    let mut checked_project_ids = use_signal(HashSet::<String>::new);
    // The algorithm's project selection from the last run, frozen at
    // "Générer" time. Together with the live `checked_project_ids` it lets
    // us tell apart "the algorithm picked this" from "the person changed
    // it", and lets a later regeneration preserve the person's manual
    // deviations instead of discarding them (Fix #2).
    let mut last_algo_project_ids = use_signal(HashSet::<String>::new);
    // The last full tailoring result (frozen at "Générer" time). Manual
    // selection only ever overrides `.experiences` on a clone of this —
    // `matched_keywords`/`missing_keywords`/`match_score` are deliberately
    // NOT recomputed from the manually-adjusted experience list, since
    // they're already defined (see matcher.rs) as being based on the
    // CV's full text regardless of what got selected into the tailored
    // output; recomputing them here would make this view inconsistent
    // with what "Générer" itself reports.
    let mut last_tailored = use_signal(|| Option::<cv_generator::models::TailoredCV>::None);
    // Set to true by "Apply selection" so the panel can show a brief
    // confirmation that the manual selection was applied (Item #3).
    let mut apply_confirmed = use_signal(|| false);

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

    // Project-id → algorithm-selected lookup for the manual selection
    // checklist. The checklist itself iterates `cv.experiences` in CV
    // (chronological) order so its order always matches the final rendered
    // document — not the score-sorted order `debug_scores` is stored in
    // (Fix #1) — and this lookup supplies the per-project diff state (Fix #3).
    let mut proj_selected: HashMap<String, bool> = HashMap::new();
    for exp_dbg in debug_scores.read().iter() {
        for p in &exp_dbg.projects {
            proj_selected.insert(p.id.clone(), p.selected);
        }
    }

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
    let t_adjust_selection = i18n::tr("tl_adjust_selection", l);
    let t_apply_selection = i18n::tr("tl_apply_selection", l);
    let t_reset_algo = i18n::tr("tl_reset_algo", l);
    let t_clear_all = i18n::tr("tl_clear_all", l);
    let t_applied = i18n::tr("tl_applied", l);
    let t_score_note = i18n::tr("tl_score_note", l);
    let t_ph = i18n::tr("tl_placeholder", l);

    // Total number of projects across the whole CV — the denominator for the
    // "N of M projects selected" count in the manual-selection panel.
    let total_projects: usize = cv.read().experiences.iter().map(|e| e.projects.len()).sum();
    let selected_count = checked_project_ids.read().len();
    let t_n_selected = i18n::tr("tl_n_selected", l)
        .replacen("{}", &selected_count.to_string(), 1)
        .replacen("{}", &total_projects.to_string(), 1);

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
                                    // Seed the manual override. Instead of blindly replacing
                                    // the checklist with the new algorithm selection (which
                                    // would silently discard the person's manual tweaks on
                                    // every regeneration), merge: preserve the previous
                                    // manual checked set, then fold in the new algorithm's
                                    // picks for anything the person hadn't explicitly removed.
                                    let new_algo: HashSet<String> = result
                                        .debug_scores
                                        .iter()
                                        .flat_map(|e| e.projects.iter())
                                        .filter(|p| p.selected)
                                        .map(|p| p.id.clone())
                                        .collect();
                                    {
                                        let prev_checked = checked_project_ids.read().clone();
                                        let prev_algo = last_algo_project_ids.read();
                                        // ids the person checked that the algorithm hadn't
                                        // picked — keep them (they're deliberate additions)
                                        // and ids the person unchecked that the algorithm
                                        // had picked — don't re-add them on regeneration.
                                        let user_removed: HashSet<String> = prev_algo
                                            .difference(&prev_checked)
                                            .cloned()
                                            .collect();
                                        let mut merged = prev_checked;
                                        for id in &new_algo {
                                            if !user_removed.contains(id) {
                                                merged.insert(id.clone());
                                            }
                                        }
                                        checked_project_ids.set(merged);
                                    }
                                    last_algo_project_ids.set(new_algo);
                                    last_tailored.set(Some(result.tailored.clone()));
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

                            // Manual project selection: lets the person tick/untick
                            // individual projects in or out of the final result,
                            // overriding the automatic scoring. The checklist is
                            // built from `cv.experiences` (not `debug_scores`) so its
                            // order always matches the final rendered document, which
                            // reads chronologically — the score-ranked order would make
                            // the checklist a misleading representation of the output
                            // (Fix #1). An experience's presence is derived, not its own
                            // checkbox: it only appears if at least one of its projects
                            // is checked. Each project shows a small marker telling the
                            // person whether it's an automatic pick, one they added by
                            // hand, or one they removed (Fix #3).
                            // Deliberately always visible once a result exists (not
                            // hidden behind a toggle like the debug panel above) since
                            // this is a real feature, not developer-facing debug info.
                            div { class: "manual-selection-panel",
                                p { class: "manual-selection-title", "{t_adjust_selection}" }
                                p { class: "manual-selection-count", "{t_n_selected}" }
                                p { class: "hint", "{t_score_note}" }
                                for exp in cv.read().experiences.iter() {
                                    div { class: "manual-selection-exp",
                                        div { class: "manual-selection-exp-header",
                                            "{exp.company} — {localized(&exp.role)}"
                                        }
                                        for proj in exp.projects.iter() {
                                            {
                                                let pid = proj.id.clone();
                                                let is_checked = checked_project_ids.read().contains(&pid);
                                                let proj_name = localized(&proj.name).to_string();
                                                // Algorithm decision for this project (score,
                                                // whether the scorer selected it). Falls back to
                                                // "not selected" when the project has no debug
                                                // entry (defensive; every stored project has one).
                                                let algo_selected = *proj_selected
                                                    .get(&pid)
                                                    .unwrap_or(&false);
                                                let marker = if is_checked && algo_selected {
                                                    "auto"
                                                } else if is_checked {
                                                    "added"
                                                } else if algo_selected {
                                                    "removed"
                                                } else {
                                                    "excluded"
                                                };
                                                let marker_css = match marker {
                                                    "added" => "hand-added",
                                                    "removed" => "hand-removed",
                                                    "excluded" => "not-selected",
                                                    _ => "automatic",
                                                };
                                                let marker_label = match marker {
                                                    "added" => i18n::tr("tl_marker_added", l),
                                                    "removed" => i18n::tr("tl_marker_removed", l),
                                                    "excluded" => i18n::tr("tl_marker_excluded", l),
                                                    _ => i18n::tr("tl_marker_auto", l),
                                                };
                                                rsx! {
                                                    label {
                                                        class: "manual-selection-project manual-selection-project-{marker}",
                                                        input {
                                                            r#type: "checkbox",
                                                            checked: is_checked,
                                                            onchange: move |e| {
                                                                if e.checked() {
                                                                    checked_project_ids.write().insert(pid.clone());
                                                                } else {
                                                                    checked_project_ids.write().remove(&pid);
                                                                }
                                                            },
                                                        }
                                                        span { "{proj_name}" }
                                                        span { class: "manual-selection-marker {marker_css}",
                                                            "{marker_label}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "manual-selection-actions",
                                    button {
                                        class: "btn btn-secondary",
                                        onclick: move |_| {
                                            *checked_project_ids.write() =
                                                last_algo_project_ids.read().clone();
                                            apply_confirmed.set(false);
                                        },
                                        "{t_reset_algo}"
                                    }
                                    button {
                                        class: "btn btn-secondary",
                                        onclick: move |_| {
                                            checked_project_ids.write().clear();
                                            apply_confirmed.set(false);
                                        },
                                        "{t_clear_all}"
                                    }
                                    button {
                                        class: "btn btn-primary",
                                        onclick: move |_| {
                                            if let Some(base) = last_tailored.read().clone() {
                                                let mut tailored = base;
                                                tailored.experiences = apply_manual_project_selection(
                                                    &cv.read(),
                                                    &checked_project_ids.read(),
                                                );
                                                let html = render_tailored_cv(&tailored, &job_title.read(), l);
                                                result_html.set(html);
                                            }
                                            apply_confirmed.set(true);
                                        },
                                        "{t_apply_selection}"
                                    }
                                }
                                if *apply_confirmed.read() {
                                    p { class: "hint hint-success manual-selection-applied", "{t_applied}" }
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
