use crate::i18n;
use crate::router::Route;
use cv_generator::models::LifetimeCV;
use cv_generator::services::matcher::{
    apply_manual_project_selection, apply_manual_skill_selection,
};
use cv_generator::services::renderer::render_tailored_cv;
use cv_generator::services::score::ScoreMode;
use cv_generator::services::worker::{fetch_model_bytes_cached, EmbeddingWorker, WorkerStatus};
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

/// Drill-down navigation for the tailor output column (mirrors the
/// Experience step's `ExpView` in cv_editor): instead of stacking the
/// score, both adjustment panels and the rendered CV on one long page,
/// each section is a separate view reached through clickable cards and a
/// breadcrumb that returns to the hub. The three views:
///  - `Summary`: score banner, keyword clouds, and the section cards.
///  - `Adjust`: the manual project + skills selection panels (shared
///    Apply/Reset/Clear row lives here).
///  - `Preview`: the rendered iframe + Download button (the button needs
///    the iframe actually rendered to call print(), so hiding it via CSS
///    to keep a stray Download button elsewhere would print a blank page).
#[derive(Clone, PartialEq)]
enum TailorView {
    Summary,
    Adjust,
    Preview,
}

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

/// `YYYY-MM-DD` for the "applied on" stamp. Web builds read the real
/// clock; native builds only exist for tests/clippy, where a rough
/// epoch-date estimate is enough.
fn today_date() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let d = js_sys::Date::new_0();
        format!(
            "{:04}-{:02}-{:02}",
            d.get_full_year(),
            d.get_month() + 1,
            d.get_date(),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let days = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0);
        let year = 1970 + days / 365;
        let month = (days % 365) / 30 + 1;
        let day = days % 30 + 1;
        format!("{year:04}-{month:02}-{day:02}")
    }
}

fn localized(t: &cv_generator::models::LocalizedText) -> &str {
    if !t.fr.is_empty() {
        &t.fr
    } else {
        &t.en
    }
}

/// Auto-persists the working state (JD text, job title, score mode,
/// manual project selection) into the single "current session" storage
/// slot — see `TailoringSession`'s doc comment for why this is kept
/// separate from the named saved-sessions list. Called on every
/// meaningful change (JD/job-title edits, checkbox toggles, mode
/// switches, and the Générer/Apply/Reset/Clear actions) rather than
/// debounced: a textarea-sized write to localStorage is cheap enough
/// that the simplicity of "just always save" wins over adding a debounce
/// mechanism for a save that's already sub-millisecond.
fn persist_current_session(
    job_title: &str,
    jd_text: &str,
    score_mode: ScoreMode,
    checked_project_ids: &std::collections::HashSet<String>,
    checked_skill_ids: &std::collections::HashSet<String>,
) {
    let session = cv_generator::models::TailoringSession {
        id: "current".to_string(),
        name: String::new(),
        job_title: job_title.to_string(),
        jd_text: jd_text.to_string(),
        score_mode,
        checked_project_ids: checked_project_ids.iter().cloned().collect(),
        checked_skill_ids: checked_skill_ids.iter().cloned().collect(),
        updated_at_ms: 0,
        match_score: 0.0,
        date_applied: String::new(),
        status: Default::default(),
    };
    cv_generator::services::storage::save_current_session(&session);
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
    // Manual skill-selection override — the exact same mechanism as the
    // project override above, one level down: `checked_skill_ids` starts as
    // whatever `select_tailored_skills` kept from the last run, then the
    // person can tick/untick individual skills (including re-including one
    // of the non-JD/non-Expert skills the algorithm dropped entirely, since
    // this also reads from `cv`, the full CV). Applied together with the
    // project selection by the single shared "Apply selection" button.
    let mut checked_skill_ids = use_signal(HashSet::<String>::new);
    // The algorithm's skill selection from the last run, frozen at
    // "Générer" time — the skill counterpart of `last_algo_project_ids`
    // (same marker logic, same regenerate-preserves-deviations rule).
    let mut last_algo_skill_ids = use_signal(HashSet::<String>::new);
    // Status filter for the saved-sessions list (`None` = show all). This
    // is the application-tracking entry point: each saved session carries
    // a match score, an "applied on" date and a status, so the list can be
    // filtered by where the person is in each hiring process.
    let mut status_filter = use_signal(|| None::<cv_generator::models::ApplicationStatus>);
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
    // Which drill-down section of the output column is shown (see
    // `TailorView`'s doc comment). Always starts on the Summary hub; each
    // "Générer" run resets back to it so the new score is what the person
    // lands on first.
    let mut view = use_signal(|| TailorView::Summary);

    // Named, explicitly-saved sessions (Item #3) — distinct from the
    // always-on auto-saved "current session" above; see
    // TailoringSession's doc comment for why. Loaded once on mount in
    // the same use_hook as the current-session restore below.
    let mut saved_sessions = use_signal(Vec::<cv_generator::models::TailoringSession>::new);
    let mut new_session_name = use_signal(String::new);

    // Restore the auto-saved "current session" on mount (Item #2) — a
    // reload or accidental navigation-away no longer loses an
    // in-progress JD paste or manual selection. `checked_project_ids` (and
    // its skill counterpart) is restored directly (not merged through the
    // usual algorithm-vs-manual logic, since there's no fresh algorithm run
    // to merge against yet); `last_algo_*` is seeded to the SAME restored
    // set, which means the next "Générer" run treats everything restored as
    // already wanted and only ever ADDS new algorithm picks on top of it,
    // never silently drops something the person had kept before reloading.
    use_hook(|| {
        if let Some(session) = cv_generator::services::storage::load_current_session() {
            job_title.set(session.job_title);
            jd_text.set(session.jd_text);
            score_mode.set(session.score_mode);
            let restored: HashSet<String> = session.checked_project_ids.into_iter().collect();
            checked_project_ids.set(restored.clone());
            last_algo_project_ids.set(restored);
            let restored_skills: HashSet<String> = session.checked_skill_ids.into_iter().collect();
            checked_skill_ids.set(restored_skills.clone());
            last_algo_skill_ids.set(restored_skills);
        }
        saved_sessions.set(cv_generator::services::storage::load_sessions_list());
    });

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
    let t_saved_sessions = i18n::tr("tl_saved_sessions", l);
    let t_save_as = i18n::tr("tl_save_as", l);
    let t_save_as_placeholder = i18n::tr("tl_save_as_placeholder", l);
    let t_save = i18n::tr("tl_save", l);
    let t_load = i18n::tr("tl_load", l);
    let t_delete = i18n::tr("tl_delete", l);
    let t_no_saved_sessions = i18n::tr("tl_no_saved_sessions", l);
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
    let t_filter_all = i18n::tr("tl_filter_all", l);
    let t_applied_on = i18n::tr("tl_applied_on", l);
    let t_status_applied = i18n::tr("tl_status_applied", l);
    let t_status_interviewing = i18n::tr("tl_status_interviewing", l);
    let t_status_offer = i18n::tr("tl_status_offer", l);
    let t_status_rejected = i18n::tr("tl_status_rejected", l);

    // Drill-down navigation labels (see `TailorView`).
    let t_nav_sections = i18n::tr("tl_nav_sections", l);
    let t_nav_summary = i18n::tr("tl_nav_summary", l);
    let t_nav_adjust = i18n::tr("tl_nav_adjust", l);
    let t_nav_preview = i18n::tr("tl_nav_preview", l);
    let t_section_projects = i18n::tr("tl_section_projects", l);
    let t_section_skills = i18n::tr("tl_section_skills", l);
    let t_section_preview = i18n::tr("tl_section_preview", l);
    let t_section_preview_sub = i18n::tr("tl_section_preview_sub", l);

    // Saved sessions to render, narrowed by the status filter (`None` =
    // show all). Computed once here so the rsx! loop below stays a plain
    // read-only iteration.
    let shown_sessions: Vec<cv_generator::models::TailoringSession> = {
        let all = saved_sessions.read();
        match *status_filter.read() {
            Some(st) => all.iter().filter(|s| s.status == st).cloned().collect(),
            None => all.clone(),
        }
    };
    // Display value for the filter `<select>`: the serialized status, or
    // "all" when no filter is active.
    let filter_select_value: &str = match &*status_filter.read() {
        Some(s) => s.as_str(),
        None => "all",
    };

    // Total number of projects across the whole CV — the denominator for the
    // "N of M projects selected" count in the manual-selection panel.
    let total_projects: usize = cv.read().experiences.iter().map(|e| e.projects.len()).sum();
    let selected_count = checked_project_ids.read().len();
    let t_n_selected = i18n::tr("tl_n_selected", l)
        .replacen("{}", &selected_count.to_string(), 1)
        .replacen("{}", &total_projects.to_string(), 1);

    // Skill counterpart of the two counts above.
    let total_skills = cv.read().skills.len();
    let selected_skills_count = checked_skill_ids.read().len();
    let t_n_skills_selected = i18n::tr("tl_n_skills_selected", l)
        .replacen("{}", &selected_skills_count.to_string(), 1)
        .replacen("{}", &total_skills.to_string(), 1);
    let t_adjust_skills = i18n::tr("tl_adjust_skills", l);

    // Skills grouped by category for the manual-selection panel, along with
    // each skill's current checked state (live override) and the algorithm's
    // last decision — the same data the project panel derives per project.
    // Materialized up front (category, then flat entries) so the render tree
    // only does a plain `for` with no conditional emission per category.
    type SkillGroup = (String, Vec<(String, String, bool, bool)>);
    let skill_groups: Vec<SkillGroup> = {
        let checked = checked_skill_ids.read();
        let algo = last_algo_skill_ids.read();
        cv_generator::models::SkillCategory::all()
            .into_iter()
            .map(|cat| {
                let entries: Vec<(String, String, bool, bool)> = cv
                    .read()
                    .skills
                    .iter()
                    .filter(|s| s.category == cat)
                    .map(|s| {
                        let sid = s.id.clone();
                        let is_checked = checked.contains(&sid);
                        let algo_selected = algo.contains(&sid);
                        (sid, s.name.clone(), is_checked, algo_selected)
                    })
                    .collect();
                let label = match l {
                    i18n::Lang::Fr => cat.label_fr().to_string(),
                    _ => cat.label().to_string(),
                };
                (label, entries)
            })
            .filter(|(_, entries)| !entries.is_empty())
            .collect()
    };

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
                            div { class: "saved-sessions-panel",
                                p { class: "saved-sessions-title", "{t_saved_sessions}" }
                                if saved_sessions.read().is_empty() {
                                    p { class: "hint", "{t_no_saved_sessions}" }
                                } else {
                                    div { class: "saved-sessions-filter",
                                        select {
                                            class: "input",
                                            value: filter_select_value,
                                            onchange: move |e| {
                                                let v = e.value();
                                                status_filter.set(if v == "all" {
                                                    None
                                                } else {
                                                    Some(cv_generator::models::ApplicationStatus::from_key(&v))
                                                });
                                            },
                                            option { value: "all", "{t_filter_all}" }
                                            option { value: "applied", "{t_status_applied}" }
                                            option { value: "interviewing", "{t_status_interviewing}" }
                                            option { value: "offer", "{t_status_offer}" }
                                            option { value: "rejected", "{t_status_rejected}" }
                                        }
                                    }
                                    for session in shown_sessions {
                                        {
                                            let session_name = session.name.clone();
                                            let session_for_load = session.clone();
                                            let session_id = session.id.clone();
                                            let session_id_for_delete = session.id.clone();
                                            let session_date = session.date_applied.clone();
                                            let session_score = session.match_score;
                                            let session_score_pct =
                                                format!("{:.0}%", session_score * 100.0);
                                            let session_status_str = session.status.as_str();
                                            rsx! {
                                                div { class: "saved-session-row",
                                                    span { class: "saved-session-name", "{session_name}" }
                                                    if session_score > 0.0 {
                                                        span {
                                                            class: "saved-session-score",
                                                            style: "color: {score_color((session_score * 100.0) as u32)}",
                                                            "{session_score_pct}"
                                                        }
                                                    }
                                                    if !session_date.is_empty() {
                                                        span { class: "saved-session-date", "{t_applied_on} {session_date}" }
                                                    }
                                                    select {
                                                        class: "input saved-session-status",
                                                        value: session_status_str,
                                                        onchange: move |e| {
                                                            let sid = session_id.clone();
                                                            let new_status =
                                                                cv_generator::models::ApplicationStatus::from_key(&e.value());
                                                            if let Some(s) = saved_sessions.write().iter_mut().find(|s| s.id == sid) {
                                                                s.status = new_status;
                                                            }
                                                            cv_generator::services::storage::save_sessions_list(
                                                                &saved_sessions.read(),
                                                            );
                                                        },
                                                        option { value: "applied", "{t_status_applied}" }
                                                        option { value: "interviewing", "{t_status_interviewing}" }
                                                        option { value: "offer", "{t_status_offer}" }
                                                        option { value: "rejected", "{t_status_rejected}" }
                                                    }
                                                    button {
                                                        class: "btn-text",
                                                        onclick: move |_| {
                                                            let session = session_for_load.clone();
                                                            job_title.set(session.job_title.clone());
                                                            jd_text.set(session.jd_text.clone());
                                                            score_mode.set(session.score_mode);
                                                            let restored: HashSet<String> =
                                                                session.checked_project_ids.iter().cloned().collect();
                                                            checked_project_ids.set(restored.clone());
                                                            last_algo_project_ids.set(restored);
                                                            let restored_skills: HashSet<String> =
                                                                session.checked_skill_ids.iter().cloned().collect();
                                                            checked_skill_ids.set(restored_skills.clone());
                                                            last_algo_skill_ids.set(restored_skills);
                                                            // Loading a saved session also makes it
                                                            // the new "current session" going
                                                            // forward, so continuing to edit from
                                                            // here keeps auto-persisting correctly.
                                                            persist_current_session(
                                                                &job_title.read(),
                                                                &jd_text.read(),
                                                                *score_mode.read(),
                                                                &checked_project_ids.read(),
                                                                &checked_skill_ids.read(),
                                                            );
                                                        },
                                                        "{t_load}"
                                                    }
                                                    button {
                                                        class: "btn-text btn-text-danger",
                                                        onclick: move |_| {
                                                            let session_id = session_id_for_delete.clone();
                                                            saved_sessions.write().retain(|s| s.id != session_id);
                                                            cv_generator::services::storage::save_sessions_list(
                                                                &saved_sessions.read(),
                                                            );
                                                        },
                                                        "{t_delete}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "saved-session-save-row",
                                    label { class: "sr-only", r#for: "save-session-name-input", "{t_save_as}" }
                                    input {
                                        id: "save-session-name-input",
                                        r#type: "text", class: "input",
                                        placeholder: "{t_save_as_placeholder}",
                                        value: new_session_name.read().clone(),
                                        oninput: move |e| { new_session_name.set(e.value()); },
                                    }
                                    button {
                                        class: "btn btn-secondary",
                                        disabled: new_session_name.read().trim().is_empty(),
                                        onclick: move |_| {
                                            let name = new_session_name.read().trim().to_string();
                                            if name.is_empty() {
                                                return;
                                            }
                                            let session = cv_generator::models::TailoringSession {
                                                id: uuid::Uuid::new_v4().to_string(),
                                                name,
                                                job_title: job_title.read().clone(),
                                                jd_text: jd_text.read().clone(),
                                                score_mode: *score_mode.read(),
                                                checked_project_ids: checked_project_ids
                                                    .read()
                                                    .iter()
                                                    .cloned()
                                                    .collect(),
                                                checked_skill_ids: checked_skill_ids
                                                    .read()
                                                    .iter()
                                                    .cloned()
                                                    .collect(),
                                                updated_at_ms: 0,
                                                match_score: *match_score.read() as f32 / 100.0,
                                                date_applied: today_date(),
                                                status: Default::default(),
                                            };
                                            saved_sessions.write().push(session);
                                            cv_generator::services::storage::save_sessions_list(
                                                &saved_sessions.read(),
                                            );
                                            new_session_name.set(String::new());
                                        },
                                        "{t_save}"
                                    }
                                }
                            }
                            div { class: "field",
                                label { class: "label", "{t_jt_lbl}" }
                                input {
                                    r#type: "text", class: "input",
                                    placeholder: "Senior Rust Engineer at Acme",
                                    value: job_title.read().clone(),
                                    oninput: move |e| {
                                        job_title.set(e.value());
                                        persist_current_session(
                                            &job_title.read(),
                                            &jd_text.read(),
                                            *score_mode.read(),
                                            &checked_project_ids.read(),
                                            &checked_skill_ids.read(),
                                        );
                                    },
                                }
                            }
                            div { class: "field",
                                label { class: "label", "{t_jd_lbl}" }
                                textarea {
                                    class: "input textarea jd-textarea", rows: "18",
                                    placeholder: "Paste the complete job posting here…",
                                    value: jd_text.read().clone(),
                                    oninput: move |e| {
                                        jd_text.set(e.value());
                                        persist_current_session(
                                            &job_title.read(),
                                            &jd_text.read(),
                                            *score_mode.read(),
                                            &checked_project_ids.read(),
                                            &checked_skill_ids.read(),
                                        );
                                    },
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
                                            onclick: move |_| {
                                                score_mode.set(mode);
                                                persist_current_session(
                                                    &job_title.read(),
                                                    &jd_text.read(),
                                                    mode,
                                                    &checked_project_ids.read(),
                                                    &checked_skill_ids.read(),
                                                );
                                            },
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
                                    // Skill counterpart of the merge above: the algorithm's
                                    // kept skills come straight from the tailored result.
                                    let new_algo_skills: HashSet<String> = result
                                        .tailored
                                        .skills
                                        .iter()
                                        .map(|s| s.id.clone())
                                        .collect();
                                    {
                                        let prev_checked = checked_skill_ids.read().clone();
                                        let prev_algo = last_algo_skill_ids.read();
                                        let user_removed: HashSet<String> = prev_algo
                                            .difference(&prev_checked)
                                            .cloned()
                                            .collect();
                                        let mut merged = prev_checked;
                                        for id in &new_algo_skills {
                                            if !user_removed.contains(id) {
                                                merged.insert(id.clone());
                                            }
                                        }
                                        checked_skill_ids.set(merged);
                                    }
                                    last_algo_skill_ids.set(new_algo_skills);
                                    last_tailored.set(Some(result.tailored.clone()));
                                    result_html.set(html);
                                    generated.set(true);
                                    view.set(TailorView::Summary);
                                    persist_current_session(
                                        &job_title.read(),
                                        &jd_text.read(),
                                        mode,
                                        &checked_project_ids.read(),
                                        &checked_skill_ids.read(),
                                    );
                                },
                                "{t_gen}"
                            }
                        }
                    }

                    div { class: "tailor-output",
                        if is_generated {
                        if *view.read() == TailorView::Summary {
                            div { class: "drill",
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
                                    class: "btn btn-secondary",
                                    onclick: move |_| { let cur = *show_debug.read(); show_debug.set(!cur); },
                                    if *show_debug.read() { "Hide score debug" } else { "Show score debug" }
                                }
                            }

                            p { class: "manual-selection-title", "{t_nav_sections}" }
                            div { class: "item-list",
                                div { class: "item-card",
                                    div { class: "item-card-body clickable",
                                        onclick: move |_| view.set(TailorView::Adjust),
                                        div { class: "item-title", "{t_section_projects}" }
                                        div { class: "item-sub", "{t_n_selected}" }
                                    }
                                }
                                div { class: "item-card",
                                    div { class: "item-card-body clickable",
                                        onclick: move |_| view.set(TailorView::Adjust),
                                        div { class: "item-title", "{t_section_skills}" }
                                        div { class: "item-sub", "{t_n_skills_selected}" }
                                    }
                                }
                                div { class: "item-card",
                                    div { class: "item-card-body clickable",
                                        onclick: move |_| view.set(TailorView::Preview),
                                        div { class: "item-title", "{t_section_preview}" }
                                        div { class: "item-sub", "{t_section_preview_sub}" }
                                    }
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
                            }
                        }

                        if *view.read() == TailorView::Adjust {
                            div { class: "drill",
                                div { class: "breadcrumb",
                                    button { class: "btn-text breadcrumb-crumb",
                                        onclick: move |_| view.set(TailorView::Summary),
                                        "{t_nav_summary}"
                                    }
                                    span { class: "breadcrumb-sep", "›" }
                                    span { class: "breadcrumb-current", "{t_nav_adjust}" }
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
                                                                persist_current_session(
                                                                    &job_title.read(),
                                                                    &jd_text.read(),
                                                                    *score_mode.read(),
                                                                    &checked_project_ids.read(),
                                                                    &checked_skill_ids.read(),
                                                                );
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
                                div { class: "manual-selection-panel",
                                    p { class: "manual-selection-title", "{t_adjust_skills}" }
                                    p { class: "manual-selection-count", "{t_n_skills_selected}" }
                                    // Same marker/count semantics as the project panel above,
                                    // one level down: the algorithm's picks come from the
                                    // `select_tailored_skills` filter of the last run
                                    // (`last_algo_skill_ids`), and the person can tick back in
                                    // a non-JD/non-Expert skill the filter dropped entirely.
                                    for (cat_label, entries) in &skill_groups {
                                        div { class: "manual-selection-exp",
                                            div { class: "manual-selection-exp-header", "{cat_label}" }
                                            for (sid, sname, is_checked, algo_selected) in entries {
                                                {
                                                    let sid = sid.clone();
                                                    let sname = sname.clone();
                                                    let marker = if *is_checked && *algo_selected {
                                                        "auto"
                                                    } else if *is_checked {
                                                        "added"
                                                    } else if *algo_selected {
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
                                                                checked: *is_checked,
                                                                onchange: move |e| {
                                                                    if e.checked() {
                                                                        checked_skill_ids.write().insert(sid.clone());
                                                                    } else {
                                                                        checked_skill_ids.write().remove(&sid);
                                                                    }
                                                                    persist_current_session(
                                                                        &job_title.read(),
                                                                        &jd_text.read(),
                                                                        *score_mode.read(),
                                                                        &checked_project_ids.read(),
                                                                        &checked_skill_ids.read(),
                                                                    );
                                                                },
                                                            }
                                                            span { "{sname}" }
                                                            span { class: "manual-selection-marker {marker_css}",
                                                                "{marker_label}"
                                                            }
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
                                            *checked_skill_ids.write() =
                                                last_algo_skill_ids.read().clone();
                                            apply_confirmed.set(false);
                                            persist_current_session(
                                                &job_title.read(),
                                                &jd_text.read(),
                                                *score_mode.read(),
                                                &checked_project_ids.read(),
                                                &checked_skill_ids.read(),
                                            );
                                        },
                                        "{t_reset_algo}"
                                    }
                                    button {
                                        class: "btn btn-secondary",
                                        onclick: move |_| {
                                            checked_project_ids.write().clear();
                                            checked_skill_ids.write().clear();
                                            apply_confirmed.set(false);
                                            persist_current_session(
                                                &job_title.read(),
                                                &jd_text.read(),
                                                *score_mode.read(),
                                                &checked_project_ids.read(),
                                                &checked_skill_ids.read(),
                                            );
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
                                                tailored.skills = apply_manual_skill_selection(
                                                    &cv.read(),
                                                    &checked_skill_ids.read(),
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

                            }
                        }

                        if *view.read() == TailorView::Preview {
                            div { class: "drill",
                                div { class: "breadcrumb",
                                    button { class: "btn-text breadcrumb-crumb",
                                        onclick: move |_| view.set(TailorView::Summary),
                                        "{t_nav_summary}"
                                    }
                                    span { class: "breadcrumb-sep", "›" }
                                    span { class: "breadcrumb-current", "{t_nav_preview}" }
                                }

                                div { class: "output-actions",
                                    button {
                                        class: "btn btn-primary",
                                        onclick: move |_| { download_pdf("cv-tailor-frame", "tailored-cv.pdf"); },
                                        "{t_dl}"
                                    }
                                }

                            p { class: "hint", "{t_dl_hint}" }

                            iframe {
                                id: "cv-tailor-frame",
                                class: "cv-iframe cv-iframe-tall",
                                srcdoc: result_html.read().clone(),
                            }
                            }
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
