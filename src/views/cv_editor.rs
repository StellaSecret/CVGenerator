#![allow(non_snake_case)]
use crate::i18n;
use crate::router::Route;
use cv_generator::models::*;
use cv_generator::services::storage::save_cv;
use dioxus::prelude::*;
use uuid::Uuid;

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

const STEP_KEYS: [&str; 6] = [
    "ed_step_personal",
    "ed_step_experience",
    "ed_step_skills",
    "ed_step_education",
    "ed_step_projects",
    "ed_step_langs",
];

#[derive(Clone, PartialEq, Debug)]
enum Step {
    Personal,
    Experience,
    Skills,
    Education,
    Projects,
    Languages,
    Done,
}
impl Step {
    fn index(&self) -> usize {
        match self {
            Self::Personal => 0,
            Self::Experience => 1,
            Self::Skills => 2,
            Self::Education => 3,
            Self::Projects => 4,
            Self::Languages => 5,
            Self::Done => 6,
        }
    }
    fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Personal,
            1 => Self::Experience,
            2 => Self::Skills,
            3 => Self::Education,
            4 => Self::Projects,
            5 => Self::Languages,
            _ => Self::Done,
        }
    }
    fn next(&self) -> Self {
        Self::from_index(self.index() + 1)
    }
    fn prev(&self) -> Self {
        if self.index() == 0 {
            Self::Personal
        } else {
            Self::from_index(self.index() - 1)
        }
    }
}

#[component]
fn StepButton(label: String, index: usize, current_idx: usize, mut step: Signal<Step>) -> Element {
    let cls = if index == current_idx {
        "step step-active"
    } else if index < current_idx {
        "step step-done"
    } else {
        "step"
    };
    let num = index + 1;
    rsx! {
        div {
            class: cls,
            onclick: move |_| { *step.write() = Step::from_index(index); },
            div { class: "step-num", "{num}" }
            div { class: "step-label", "{label}" }
        }
    }
}

// ── Experience ─────────────────────────────────────────────────────────────────

#[component]
fn ExpItem(exp: Experience, index: usize, mut cv: Signal<LifetimeCV>) -> Element {
    let lang: Signal<i18n::Lang> = use_context();
    let mut editing = use_signal(|| false);
    let mut e_company = use_signal(String::new);
    let mut e_role = use_signal(String::new);
    let mut e_location = use_signal(String::new);
    let mut e_start = use_signal(String::new);
    let mut e_end = use_signal(String::new);
    let mut e_projects = use_signal(Vec::<ExperienceProject>::new);

    let t_save = i18n::tr("ed_save_changes", *lang.read());
    let t_cancel = i18n::tr("ed_cancel", *lang.read());
    let t_company = i18n::tr("ed_company", *lang.read());
    let t_role = i18n::tr("ed_role", *lang.read());
    let t_loc = i18n::tr("ed_location", *lang.read());
    let t_start = i18n::tr("ed_start_date", *lang.read());
    let t_end = i18n::tr("ed_end_date", *lang.read());
    let t_projects = i18n::tr("ed_projects", *lang.read());
    let t_project_name = i18n::tr("ed_project_name", *lang.read());
    let t_achieve = i18n::tr("ed_achievements", *lang.read());
    let t_add_bullet = i18n::tr("ed_add_bullet", *lang.read());
    let t_tools = i18n::tr("ed_tools", *lang.read());
    let t_present = i18n::tr("ed_present", *lang.read());
    let t_add_project = i18n::tr("ed_add_project", *lang.read());
    let t_project_ctx = i18n::tr("ed_project_context", *lang.read());

    if *editing.read() {
        rsx! {
            div { class: "inline-form inline-form-compact",
                div { class: "form-row",
                    Field { label: t_company.to_string(), required: true,
                        input { r#type: "text", class: "input",
                            value: e_company.read().clone(),
                            oninput: move |e| { e_company.set(e.value()); },
                        }
                    }
                    Field { label: t_role.to_string(), required: true,
                        input { r#type: "text", class: "input",
                            value: e_role.read().clone(),
                            oninput: move |e| { e_role.set(e.value()); },
                        }
                    }
                }
                div { class: "form-row",
                    Field { label: t_loc.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_location.read().clone(),
                            oninput: move |e| { e_location.set(e.value()); },
                        }
                    }
                    Field { label: t_start.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_start.read().clone(),
                            oninput: move |e| { e_start.set(e.value()); },
                        }
                    }
                    Field { label: t_end.to_string(),
                        input { r#type: "text", class: "input", placeholder: "{t_present}",
                            value: e_end.read().clone(),
                            oninput: move |e| { e_end.set(e.value()); },
                        }
                    }
                }
                div { class: "field",
                    label { class: "label", "{t_projects}" }
                    for pi in 0..e_projects.read().len() {
                        div { class: "project-card",
                            Field { label: t_project_name.to_string(),
                                input { r#type: "text", class: "input",
                                    value: e_projects.read()[pi].name.clone(),
                                    oninput: move |e| { e_projects.write()[pi].name = e.value(); },
                                }
                            }
                            Field { label: t_project_ctx.to_string(),
                                input { r#type: "text", class: "input",
                                    value: e_projects.read()[pi].context.clone(),
                                    oninput: move |e| { e_projects.write()[pi].context = e.value(); },
                                }
                            }
                            div { class: "field",
                                label { class: "label", "{t_achieve}" }
                                for bi in 0..e_projects.read()[pi].bullets.len() {
                                    div { class: "bullet-row",
                                        span { class: "bullet-dot", "•" }
                                        input {
                                            r#type: "text", class: "input",
                                            value: e_projects.read()[pi].bullets[bi].clone(),
                                            oninput: move |e| { e_projects.write()[pi].bullets[bi] = e.value(); },
                                        }
                                        if e_projects.read()[pi].bullets.len() > 1 {
                                            button { class: "btn-icon",
                                                onclick: move |_| { e_projects.write()[pi].bullets.remove(bi); },
                                                "×"
                                            }
                                        }
                                    }
                                }
                                button { class: "btn-text",
                                    onclick: move |_| { e_projects.write()[pi].bullets.push(String::new()); },
                                    "{t_add_bullet}"
                                }
                            }
                            Field { label: t_tools.to_string(),
                                input { r#type: "text", class: "input",
                                    value: e_projects.read()[pi].tools.join(", "),
                                    oninput: move |e| {
                                        let tools: Vec<String> = e.value().split(',')
                                            .map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                                        e_projects.write()[pi].tools = tools;
                                    },
                                }
                            }
                            if e_projects.read().len() > 1 {
                                button { class: "btn-text btn-danger",
                                    onclick: move |_| { e_projects.write().remove(pi); },
                                    "× Remove project"
                                }
                            }
                        }
                    }
                    button { class: "btn-text",
                        onclick: move |_| {
                            e_projects.write().push(ExperienceProject {
                                name: String::new(),
                                context: String::new(),
                                bullets: vec![String::new()],
                                tools: Vec::new(),
                            });
                        },
                        "{t_add_project}"
                    }
                }
                div { class: "form-actions",
                    button { class: "btn btn-primary",
                        onclick: move |_| {
                            let id = cv.read().experiences[index].id.clone();
                            let projects: Vec<ExperienceProject> = e_projects.read().iter().map(|p| {
                                ExperienceProject {
                                    name: p.name.clone(),
                                    context: p.context.clone(),
                                    bullets: p.bullets.iter().filter(|b| !b.is_empty()).cloned().collect(),
                                    tools: p.tools.clone(),
                                }
                            }).filter(|p| !p.name.is_empty() || !p.bullets.is_empty()).collect();
                            cv.write().experiences[index] = Experience {
                                id,
                                company: e_company.read().clone(),
                                role: e_role.read().clone(),
                                location: e_location.read().clone(),
                                start_date: e_start.read().clone(),
                                end_date: e_end.read().clone(),
                                projects,
                            };
                            editing.set(false);
                        },
                        "{t_save}"
                    }
                    button { class: "btn btn-secondary",
                        onclick: move |_| { editing.set(false); },
                        "{t_cancel}"
                    }
                }
            }
        }
    } else {
        let role = exp.role.clone();
        let sub = format!("{} · {} – {}", exp.company, exp.start_date, exp.end_date);
        rsx! {
            div { class: "item-card",
                div { class: "item-card-body",
                    div { class: "item-title", "{role}" }
                    div { class: "item-sub", "{sub}" }
                    for proj in exp.projects.iter() {
                        if !proj.name.is_empty() || !proj.bullets.is_empty() {
                            div { class: "item-project",
                                if !proj.name.is_empty() {
                                    div { class: "item-project-name", "{proj.name}" }
                                }
                                if !proj.context.is_empty() {
                                    div { class: "item-project-context", "{proj.context}" }
                                }
                                if !proj.bullets.is_empty() {
                                    div { class: "item-tags",
                                        for b in proj.bullets.iter().filter(|b| !b.is_empty()) {
                                            span { class: "tag-small", "• {b}" }
                                        }
                                    }
                                }
                                if !proj.tools.is_empty() {
                                    div { class: "item-tags",
                                        for t in &proj.tools { span { class: "tag-small", "{t}" } }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "item-actions",
                    button { class: "btn-icon btn-edit",
                        onclick: move |_| {
                            let item = cv.read().experiences[index].clone();
                            e_company.set(item.company);
                            e_role.set(item.role);
                            e_location.set(item.location);
                            e_start.set(item.start_date);
                            e_end.set(item.end_date);
                            e_projects.set(if item.projects.is_empty() {
                                vec![ExperienceProject { name: String::new(), context: String::new(), bullets: vec![String::new()], tools: Vec::new() }]
                            } else {
                                item.projects
                            });
                            editing.set(true);
                        },
                        "✎"
                    }
                    button { class: "btn-icon btn-danger",
                        onclick: move |_| { cv.write().experiences.remove(index); },
                        "🗑"
                    }
                }
            }
        }
    }
}

// ── Skills ────────────────────────────────────────────────────────────────────

#[component]
fn SkillItem(skill: Skill, index: usize, mut cv: Signal<LifetimeCV>) -> Element {
    let lang: Signal<i18n::Lang> = use_context();
    let mut editing = use_signal(|| false);
    let mut e_name = use_signal(String::new);
    let mut e_category = use_signal(|| SkillCategory::Programming);
    let mut e_level = use_signal(|| SkillLevel::Intermediate);

    let l = *lang.read();
    let t_save = i18n::tr("ed_save_changes", l);
    let t_cancel = i18n::tr("ed_cancel", l);
    let t_sname = i18n::tr("ed_skill_name", l);
    let t_cat = i18n::tr("ed_category", l);
    let t_level = i18n::tr("ed_level", l);

    if *editing.read() {
        rsx! {
            div { class: "inline-form inline-form-compact",
                div { class: "form-row form-row-tight",
                    Field { label: t_sname.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_name.read().clone(),
                            oninput: move |e| { e_name.set(e.value()); },
                        }
                    }
                    Field { label: t_cat.to_string(),
                        select { class: "input select",
                            onchange: move |e| {
                                e_category.set(match e.value().as_str() {
                                    "Framework" => SkillCategory::Framework,
                                    "Tool"      => SkillCategory::Tool,
                                    "Cloud"     => SkillCategory::Cloud,
                                    "Database"  => SkillCategory::Database,
                                    "Soft Skill"=> SkillCategory::Soft,
                                    "Other"     => SkillCategory::Other,
                                    _           => SkillCategory::Programming,
                                });
                            },
                            for cat in SkillCategory::all() {
                                option { value: cat.label(), selected: e_category.read().label() == cat.label(), "{cat.label()}" }
                            }
                        }
                    }
                    Field { label: t_level.to_string(),
                        select { class: "input select",
                            onchange: move |e| {
                                e_level.set(match e.value().as_str() {
                                    "Beginner"     => SkillLevel::Beginner,
                                    "Advanced"     => SkillLevel::Advanced,
                                    "Expert"       => SkillLevel::Expert,
                                    _              => SkillLevel::Intermediate,
                                });
                            },
                            for lvl in SkillLevel::all() {
                                option { value: lvl.label(), selected: e_level.read().label() == lvl.label(), "{lvl.label()}" }
                            }
                        }
                    }
                }
                div { class: "form-actions",
                    button { class: "btn btn-primary",
                        onclick: move |_| {
                            if e_name.read().is_empty() { return; }
                            let id = cv.read().skills[index].id.clone();
                            cv.write().skills[index] = Skill {
                                id,
                                name: e_name.read().clone(),
                                category: e_category.read().clone(),
                                level: e_level.read().clone(),
                            };
                            editing.set(false);
                        },
                        "{t_save}"
                    }
                    button { class: "btn btn-secondary",
                        onclick: move |_| { editing.set(false); },
                        "{t_cancel}"
                    }
                }
            }
        }
    } else {
        let name = skill.name.clone();
        let level = skill.level.label().to_string();
        rsx! {
            div { class: "skill-chip",
                span { "{name}" }
                span { class: "chip-level", "{level}" }
                button { class: "btn-icon-sm btn-edit-sm",
                    onclick: move |_| {
                        let item = cv.read().skills[index].clone();
                        e_name.set(item.name);
                        e_category.set(item.category);
                        e_level.set(item.level);
                        editing.set(true);
                    },
                    "✎"
                }
                button { class: "btn-icon-sm",
                    onclick: move |_| { cv.write().skills.remove(index); },
                    "×"
                }
            }
        }
    }
}

#[component]
fn SkillGroup(cat_label: String, items: Vec<(usize, Skill)>, cv: Signal<LifetimeCV>) -> Element {
    rsx! {
        div { class: "skill-group",
            div { class: "skill-group-label", "{cat_label}" }
            div { class: "skill-chips",
                for (i, skill) in items { SkillItem { skill, index: i, cv } }
            }
        }
    }
}

// ── Education ─────────────────────────────────────────────────────────────────

#[component]
fn EduItem(edu: Education, index: usize, mut cv: Signal<LifetimeCV>) -> Element {
    let lang: Signal<i18n::Lang> = use_context();
    let mut editing = use_signal(|| false);
    let mut e_inst = use_signal(String::new);
    let mut e_degree = use_signal(String::new);
    let mut e_field = use_signal(String::new);
    let mut e_start = use_signal(String::new);
    let mut e_end = use_signal(String::new);

    let l = *lang.read();
    let t_save = i18n::tr("ed_save_changes", l);
    let t_cancel = i18n::tr("ed_cancel", l);
    let t_inst = i18n::tr("ed_institution", l);
    let t_degree = i18n::tr("ed_degree", l);
    let t_field = i18n::tr("ed_field", l);
    let t_start = i18n::tr("ed_start_year", l);
    let t_end = i18n::tr("ed_end_year", l);

    if *editing.read() {
        rsx! {
            div { class: "inline-form inline-form-compact",
                div { class: "form-row",
                    Field { label: t_inst.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_inst.read().clone(),
                            oninput: move |e| { e_inst.set(e.value()); },
                        }
                    }
                    Field { label: t_degree.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_degree.read().clone(),
                            oninput: move |e| { e_degree.set(e.value()); },
                        }
                    }
                }
                div { class: "form-row",
                    Field { label: t_field.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_field.read().clone(),
                            oninput: move |e| { e_field.set(e.value()); },
                        }
                    }
                    Field { label: t_start.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_start.read().clone(),
                            oninput: move |e| { e_start.set(e.value()); },
                        }
                    }
                    Field { label: t_end.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_end.read().clone(),
                            oninput: move |e| { e_end.set(e.value()); },
                        }
                    }
                }
                div { class: "form-actions",
                    button { class: "btn btn-primary",
                        onclick: move |_| {
                            let id = cv.read().education[index].id.clone();
                            let achievements = cv.read().education[index].achievements.clone();
                            cv.write().education[index] = Education {
                                id,
                                institution: e_inst.read().clone(),
                                degree: e_degree.read().clone(),
                                field: e_field.read().clone(),
                                start_year: e_start.read().clone(),
                                end_year: e_end.read().clone(),
                                achievements,
                            };
                            editing.set(false);
                        },
                        "{t_save}"
                    }
                    button { class: "btn btn-secondary",
                        onclick: move |_| { editing.set(false); },
                        "{t_cancel}"
                    }
                }
            }
        }
    } else {
        let title = format!("{} · {}", edu.degree, edu.field);
        let sub = format!(
            "{} · {} – {}",
            edu.institution, edu.start_year, edu.end_year
        );
        rsx! {
            div { class: "item-card",
                div { class: "item-card-body",
                    div { class: "item-title", "{title}" }
                    div { class: "item-sub", "{sub}" }
                }
                div { class: "item-actions",
                    button { class: "btn-icon btn-edit",
                        onclick: move |_| {
                            let item = cv.read().education[index].clone();
                            e_inst.set(item.institution);
                            e_degree.set(item.degree);
                            e_field.set(item.field);
                            e_start.set(item.start_year);
                            e_end.set(item.end_year);
                            editing.set(true);
                        },
                        "✎"
                    }
                    button { class: "btn-icon btn-danger",
                        onclick: move |_| { cv.write().education.remove(index); },
                        "🗑"
                    }
                }
            }
        }
    }
}

// ── Projects ──────────────────────────────────────────────────────────────────

#[component]
fn ProjItem(proj: Project, index: usize, mut cv: Signal<LifetimeCV>) -> Element {
    let lang: Signal<i18n::Lang> = use_context();
    let mut editing = use_signal(|| false);
    let mut e_name = use_signal(String::new);
    let mut e_desc = use_signal(String::new);
    let mut e_url = use_signal(String::new);
    let mut e_tools = use_signal(String::new);

    let l = *lang.read();
    let t_save = i18n::tr("ed_save_changes", l);
    let t_cancel = i18n::tr("ed_cancel", l);
    let t_pname = i18n::tr("ed_proj_name", l);
    let t_desc = i18n::tr("ed_description", l);
    let t_tools = i18n::tr("ed_tools", l);

    if *editing.read() {
        rsx! {
            div { class: "inline-form inline-form-compact",
                div { class: "form-row",
                    Field { label: t_pname.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_name.read().clone(),
                            oninput: move |e| { e_name.set(e.value()); },
                        }
                    }
                    Field { label: "URL",
                        input { r#type: "url", class: "input",
                            value: e_url.read().clone(),
                            oninput: move |e| { e_url.set(e.value()); },
                        }
                    }
                }
                Field { label: t_desc.to_string(),
                    textarea { class: "input textarea", rows: "2",
                        value: e_desc.read().clone(),
                        oninput: move |e| { e_desc.set(e.value()); },
                    }
                }
                Field { label: t_tools.to_string(),
                    input { r#type: "text", class: "input",
                        value: e_tools.read().clone(),
                        oninput: move |e| { e_tools.set(e.value()); },
                    }
                }
                div { class: "form-actions",
                    button { class: "btn btn-primary",
                        onclick: move |_| {
                            let tools: Vec<String> = e_tools.read().split(',')
                                .map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                            let id = cv.read().projects[index].id.clone();
                            let bullets = cv.read().projects[index].bullets.clone();
                            cv.write().projects[index] = Project {
                                id,
                                name: e_name.read().clone(),
                                description: e_desc.read().clone(),
                                url: e_url.read().clone(),
                                tools,
                                bullets,
                            };
                            editing.set(false);
                        },
                        "{t_save}"
                    }
                    button { class: "btn btn-secondary",
                        onclick: move |_| { editing.set(false); },
                        "{t_cancel}"
                    }
                }
            }
        }
    } else {
        let name = proj.name.clone();
        let desc = proj.description.clone();
        rsx! {
            div { class: "item-card",
                div { class: "item-card-body",
                    div { class: "item-title", "{name}" }
                    div { class: "item-sub", "{desc}" }
                }
                div { class: "item-actions",
                    button { class: "btn-icon btn-edit",
                        onclick: move |_| {
                            let item = cv.read().projects[index].clone();
                            e_name.set(item.name);
                            e_desc.set(item.description);
                            e_url.set(item.url);
                            e_tools.set(item.tools.join(", "));
                            editing.set(true);
                        },
                        "✎"
                    }
                    button { class: "btn-icon btn-danger",
                        onclick: move |_| { cv.write().projects.remove(index); },
                        "🗑"
                    }
                }
            }
        }
    }
}

// ── Languages ─────────────────────────────────────────────────────────────────

#[component]
fn LangItem(lang_item: Language, index: usize, mut cv: Signal<LifetimeCV>) -> Element {
    let lang: Signal<i18n::Lang> = use_context();
    let mut editing = use_signal(|| false);
    let mut e_name = use_signal(String::new);
    let mut e_level = use_signal(|| LanguageLevel::Conversational);

    let l = *lang.read();
    let t_save = i18n::tr("ed_save_changes", l);
    let t_cancel = i18n::tr("ed_cancel", l);
    let t_lang_label = i18n::tr("ed_language", l);
    let t_level = i18n::tr("ed_level", l);

    if *editing.read() {
        rsx! {
            div { class: "inline-form inline-form-compact",
                div { class: "form-row form-row-tight",
                    Field { label: t_lang_label.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_name.read().clone(),
                            oninput: move |e| { e_name.set(e.value()); },
                        }
                    }
                    Field { label: t_level.to_string(),
                        select { class: "input select",
                            onchange: move |e| {
                                e_level.set(match e.value().as_str() {
                                    "Native / Bilingual" => LanguageLevel::Native,
                                    "Professional"       => LanguageLevel::Professional,
                                    _                    => LanguageLevel::Conversational,
                                });
                            },
                            for lvl in LanguageLevel::all() {
                                option { value: lvl.label(), selected: e_level.read().label() == lvl.label(), "{lvl.label()}" }
                            }
                        }
                    }
                }
                div { class: "form-actions",
                    button { class: "btn btn-primary",
                        onclick: move |_| {
                            if e_name.read().is_empty() { return; }
                            let id = cv.read().languages[index].id.clone();
                            cv.write().languages[index] = Language {
                                id,
                                name: e_name.read().clone(),
                                level: e_level.read().clone(),
                            };
                            editing.set(false);
                        },
                        "{t_save}"
                    }
                    button { class: "btn btn-secondary",
                        onclick: move |_| { editing.set(false); },
                        "{t_cancel}"
                    }
                }
            }
        }
    } else {
        let text = format!("{} · {}", lang_item.name, lang_item.level.label());
        rsx! {
            div { class: "item-card",
                div { class: "item-card-body", div { class: "item-title", "{text}" } }
                div { class: "item-actions",
                    button { class: "btn-icon btn-edit",
                        onclick: move |_| {
                            let item = cv.read().languages[index].clone();
                            e_name.set(item.name);
                            e_level.set(item.level);
                            editing.set(true);
                        },
                        "✎"
                    }
                    button { class: "btn-icon btn-danger",
                        onclick: move |_| { cv.write().languages.remove(index); },
                        "🗑"
                    }
                }
            }
        }
    }
}

// ── Certifications ────────────────────────────────────────────────────────────

#[component]
fn CertItem(cert: Certification, index: usize, mut cv: Signal<LifetimeCV>) -> Element {
    let lang: Signal<i18n::Lang> = use_context();
    let mut editing = use_signal(|| false);
    let mut e_name = use_signal(String::new);
    let mut e_issuer = use_signal(String::new);
    let mut e_date = use_signal(String::new);
    let mut e_url = use_signal(String::new);

    let l = *lang.read();
    let t_save = i18n::tr("ed_save_changes", l);
    let t_cancel = i18n::tr("ed_cancel", l);
    let t_cname = i18n::tr("ed_cert_name", l);
    let t_issuer = i18n::tr("ed_issuer", l);
    let t_date = i18n::tr("ed_date", l);

    if *editing.read() {
        rsx! {
            div { class: "inline-form inline-form-compact",
                div { class: "form-row",
                    Field { label: t_cname.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_name.read().clone(),
                            oninput: move |e| { e_name.set(e.value()); },
                        }
                    }
                    Field { label: t_issuer.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_issuer.read().clone(),
                            oninput: move |e| { e_issuer.set(e.value()); },
                        }
                    }
                }
                div { class: "form-row",
                    Field { label: t_date.to_string(),
                        input { r#type: "text", class: "input",
                            value: e_date.read().clone(),
                            oninput: move |e| { e_date.set(e.value()); },
                        }
                    }
                    Field { label: "URL",
                        input { r#type: "url", class: "input",
                            value: e_url.read().clone(),
                            oninput: move |e| { e_url.set(e.value()); },
                        }
                    }
                }
                div { class: "form-actions",
                    button { class: "btn btn-primary",
                        onclick: move |_| {
                            if e_name.read().is_empty() { return; }
                            let id = cv.read().certifications[index].id.clone();
                            cv.write().certifications[index] = Certification {
                                id,
                                name: e_name.read().clone(),
                                issuer: e_issuer.read().clone(),
                                date: e_date.read().clone(),
                                url: e_url.read().clone(),
                            };
                            editing.set(false);
                        },
                        "{t_save}"
                    }
                    button { class: "btn btn-secondary",
                        onclick: move |_| { editing.set(false); },
                        "{t_cancel}"
                    }
                }
            }
        }
    } else {
        let name = cert.name.clone();
        let sub = format!("{} · {}", cert.issuer, cert.date);
        rsx! {
            div { class: "item-card",
                div { class: "item-card-body",
                    div { class: "item-title", "{name}" }
                    div { class: "item-sub", "{sub}" }
                }
                div { class: "item-actions",
                    button { class: "btn-icon btn-edit",
                        onclick: move |_| {
                            let item = cv.read().certifications[index].clone();
                            e_name.set(item.name);
                            e_issuer.set(item.issuer);
                            e_date.set(item.date);
                            e_url.set(item.url);
                            editing.set(true);
                        },
                        "✎"
                    }
                    button { class: "btn-icon btn-danger",
                        onclick: move |_| { cv.write().certifications.remove(index); },
                        "🗑"
                    }
                }
            }
        }
    }
}

// ── Root editor ───────────────────────────────────────────────────────────────

#[component]
pub fn CvEditor() -> Element {
    let cv: Signal<LifetimeCV> = use_context();
    let lang: Signal<i18n::Lang> = use_context();
    let l = *lang.read();
    let mut step = use_signal(|| Step::Personal);

    let current_idx = step.read().index();
    let show_nav = *step.read() != Step::Done;
    let show_back = *step.read() != Step::Personal && *step.read() != Step::Done;
    let is_last = *step.read() == Step::Languages;

    let t_back = i18n::tr("ed_back", l);
    let t_title = i18n::tr("ed_title", l);
    let t_sub = i18n::tr("ed_subtitle", l);
    let t_save_f = i18n::tr("ed_save_finish", l);
    let t_save_c = i18n::tr("ed_save_cont", l);
    let step_labels: Vec<String> = (0..6)
        .map(|i| i18n::tr(STEP_KEYS[i], l).to_string())
        .collect();

    rsx! {
        div { class: "page",
            div { class: "page-back-row",
                Link { to: Route::Home {}, class: "page-back-link", "{t_back}" }
            }
            div { class: "page-header",
                h1 { "{t_title}" }
                p { class: "subtitle", "{t_sub}" }
            }

            div { class: "steps",
                for i in 0usize..6 {
                    StepButton {
                        label: step_labels[i].clone(),
                        index: i,
                        current_idx,
                        step,
                    }
                }
            }

            div { class: "editor-body",
                if *step.read() == Step::Personal   { StepPersonal   { cv, lang } }
                if *step.read() == Step::Experience { StepExperience { cv, lang } }
                if *step.read() == Step::Skills     { StepSkills     { cv, lang } }
                if *step.read() == Step::Education  { StepEducation  { cv, lang } }
                if *step.read() == Step::Projects   { StepProjects   { cv, lang } }
                if *step.read() == Step::Languages  { StepLanguages  { cv, lang } }
                if *step.read() == Step::Done       { StepDone { lang } }
            }

            if show_nav {
                div { class: "form-nav",
                    if show_back {
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| {
                                let prev = step.read().prev();
                                *step.write() = prev;
                            },
                            "{t_back}"
                        }
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            save_cv(&cv.read());
                            let next = step.read().next();
                            *step.write() = next;
                        },
                        if is_last { "{t_save_f}" } else { "{t_save_c}" }
                    }
                }
            }
        }
    }
}

// ── Step: Personal ────────────────────────────────────────────────────────────

#[component]
fn StepPersonal(cv: Signal<LifetimeCV>, lang: Signal<i18n::Lang>) -> Element {
    let l = *lang.read();
    let t_title = i18n::tr("ed_personal_title", l);
    let t_name = i18n::tr("ed_fullname", l);
    let t_pro = i18n::tr("ed_pro_title", l);
    let t_email = i18n::tr("ed_email", l);
    let t_phone = i18n::tr("ed_phone", l);
    let t_loc = i18n::tr("ed_location", l);
    let t_li = i18n::tr("ed_linkedin", l);
    let t_gh = i18n::tr("ed_github", l);
    let t_web = i18n::tr("ed_website", l);
    let t_summary = i18n::tr("ed_summary", l);
    let t_hint = i18n::tr("ed_summary_hint", l);

    rsx! {
        div { class: "form-section",
            h2 { "{t_title}" }
            div { class: "form-row",
                Field { label: t_name.to_string(), required: true,
                    input { r#type: "text", class: "input", placeholder: "Jane Smith",
                        value: cv.read().personal.name.clone(),
                        oninput: move |e| { cv.write().personal.name = e.value(); },
                    }
                }
                Field { label: t_pro.to_string(),
                    input { r#type: "text", class: "input", placeholder: "Senior Rust Engineer",
                        value: cv.read().personal.title.clone(),
                        oninput: move |e| { cv.write().personal.title = e.value(); },
                    }
                }
            }
            div { class: "form-row",
                Field { label: t_email.to_string(), required: true,
                    input { r#type: "email", class: "input", placeholder: "jane@example.com",
                        value: cv.read().personal.email.clone(),
                        oninput: move |e| { cv.write().personal.email = e.value(); },
                    }
                }
                Field { label: t_phone.to_string(),
                    input { r#type: "tel", class: "input", placeholder: "+33 6 00 00 00 00",
                        value: cv.read().personal.phone.clone(),
                        oninput: move |e| { cv.write().personal.phone = e.value(); },
                    }
                }
            }
            div { class: "form-row",
                Field { label: t_loc.to_string(),
                    input { r#type: "text", class: "input", placeholder: "Paris, France",
                        value: cv.read().personal.location.clone(),
                        oninput: move |e| { cv.write().personal.location = e.value(); },
                    }
                }
                Field { label: t_li.to_string(),
                    input { r#type: "url", class: "input", placeholder: "https://linkedin.com/in/…",
                        value: cv.read().personal.linkedin.clone(),
                        oninput: move |e| { cv.write().personal.linkedin = e.value(); },
                    }
                }
            }
            div { class: "form-row",
                Field { label: t_gh.to_string(),
                    input { r#type: "url", class: "input", placeholder: "https://github.com/…",
                        value: cv.read().personal.github.clone(),
                        oninput: move |e| { cv.write().personal.github = e.value(); },
                    }
                }
                Field { label: t_web.to_string(),
                    input { r#type: "url", class: "input", placeholder: "https://…",
                        value: cv.read().personal.website.clone(),
                        oninput: move |e| { cv.write().personal.website = e.value(); },
                    }
                }
            }
            Field { label: t_summary.to_string(),
                textarea { class: "input textarea", rows: "4",
                    placeholder: "{t_hint}",
                    value: cv.read().personal.summary.clone(),
                    oninput: move |e| { cv.write().personal.summary = e.value(); },
                }
            }
        }
    }
}

// ── Step: Experience ──────────────────────────────────────────────────────────

#[component]
fn StepExperience(cv: Signal<LifetimeCV>, lang: Signal<i18n::Lang>) -> Element {
    let l = *lang.read();
    let mut show_form = use_signal(|| cv.read().experiences.is_empty());
    let mut new_company = use_signal(String::new);
    let mut new_role = use_signal(String::new);
    let mut new_loc = use_signal(String::new);
    let mut new_start = use_signal(String::new);
    let t_present_s = i18n::tr("ed_present", l);
    let mut new_end = use_signal(|| t_present_s.to_string());
    let mut new_projects = use_signal(|| vec![ExperienceProject { name: String::new(), context: String::new(), bullets: vec![String::new()], tools: Vec::new() }]);

    let experiences: Vec<Experience> = cv.read().experiences.clone();
    let adding = *show_form.read();

    let t_title = i18n::tr("ed_exp_title", l);
    let t_hint = i18n::tr("ed_exp_hint", l);
    let t_add_exp = i18n::tr("ed_add_exp", l);
    let t_new_pos = i18n::tr("ed_new_position", l);
    let t_company = i18n::tr("ed_company", l);
    let t_role = i18n::tr("ed_role", l);
    let t_location = i18n::tr("ed_location", l);
    let t_start = i18n::tr("ed_start_date", l);
    let t_end = i18n::tr("ed_end_date", l);
    let t_present = i18n::tr("ed_present", l);
    let t_projects = i18n::tr("ed_projects", l);
    let t_project_name = i18n::tr("ed_project_name", l);
    let t_achieve = i18n::tr("ed_achievements", l);
    let t_add_bullet = i18n::tr("ed_add_bullet", l);
    let t_tools = i18n::tr("ed_tools", l);
    let t_add_pos = i18n::tr("ed_add_position", l);
    let t_add_project = i18n::tr("ed_add_project", l);
    let t_project_ctx = i18n::tr("ed_project_context", l);
    let t_cancel = i18n::tr("ed_cancel", l);

    rsx! {
        div { class: "form-section",
            h2 { "{t_title}" }
            p { class: "hint", "{t_hint}" }

            div { class: "item-list",
                for (i, exp) in experiences.into_iter().enumerate() {
                    ExpItem { exp, index: i, cv }
                }
            }

            if !adding {
                button {
                    class: "btn btn-outline",
                    onclick: move |_| { show_form.set(true); },
                    "{t_add_exp}"
                }
            } else {
                div { class: "inline-form",
                    h3 { "{t_new_pos}" }
                    div { class: "form-row",
                        Field { label: t_company.to_string(), required: true,
                            input { r#type: "text", class: "input", placeholder: "Acme Corp",
                                value: new_company.read().clone(),
                                oninput: move |e| { new_company.set(e.value()); },
                            }
                        }
                        Field { label: t_role.to_string(), required: true,
                            input { r#type: "text", class: "input", placeholder: "Software Engineer",
                                value: new_role.read().clone(),
                                oninput: move |e| { new_role.set(e.value()); },
                            }
                        }
                    }
                    div { class: "form-row",
                        Field { label: t_location.to_string(),
                            input { r#type: "text", class: "input", placeholder: "Paris, France",
                                value: new_loc.read().clone(),
                                oninput: move |e| { new_loc.set(e.value()); },
                            }
                        }
                        Field { label: t_start.to_string(),
                            input { r#type: "text", class: "input", placeholder: "Jan 2021",
                                value: new_start.read().clone(),
                                oninput: move |e| { new_start.set(e.value()); },
                            }
                        }
                        Field { label: t_end.to_string(),
                            input { r#type: "text", class: "input", placeholder: "{t_present}",
                                value: new_end.read().clone(),
                                oninput: move |e| { new_end.set(e.value()); },
                            }
                        }
                    }
                    div { class: "field",
                        label { class: "label", "{t_projects}" }
                        for pi in 0..new_projects.read().len() {
                            div { class: "project-card",
                                Field { label: t_project_name.to_string(),
                                    input { r#type: "text", class: "input", placeholder: "API Platform",
                                        value: new_projects.read()[pi].name.clone(),
                                        oninput: move |e| { new_projects.write()[pi].name = e.value(); },
                                    }
                                }
                                Field { label: t_project_ctx.to_string(),
                                    input { r#type: "text", class: "input",
                                        value: new_projects.read()[pi].context.clone(),
                                        oninput: move |e| { new_projects.write()[pi].context = e.value(); },
                                    }
                                }
                                div { class: "field",
                                    label { class: "label", "{t_achieve}" }
                                    for bi in 0..new_projects.read()[pi].bullets.len() {
                                        div { class: "bullet-row",
                                            span { class: "bullet-dot", "•" }
                                            input {
                                                r#type: "text", class: "input",
                                                placeholder: "Reduced API latency by 40%",
                                                value: new_projects.read()[pi].bullets[bi].clone(),
                                                oninput: move |e| { new_projects.write()[pi].bullets[bi] = e.value(); },
                                            }
                                            if new_projects.read()[pi].bullets.len() > 1 {
                                                button {
                                                    class: "btn-icon",
                                                    onclick: move |_| { new_projects.write()[pi].bullets.remove(bi); },
                                                    "×"
                                                }
                                            }
                                        }
                                    }
                                    button {
                                        class: "btn-text",
                                        onclick: move |_| { new_projects.write()[pi].bullets.push(String::new()); },
                                        "{t_add_bullet}"
                                    }
                                }
                                Field { label: t_tools.to_string(),
                                    input { r#type: "text", class: "input", placeholder: "Rust, PostgreSQL, Kubernetes",
                                        value: new_projects.read()[pi].tools.join(", "),
                                        oninput: move |e| {
                                            let tools: Vec<String> = e.value().split(',')
                                                .map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                                            new_projects.write()[pi].tools = tools;
                                        },
                                    }
                                }
                                if new_projects.read().len() > 1 {
                                    button { class: "btn-text btn-danger",
                                        onclick: move |_| { new_projects.write().remove(pi); },
                                        "× Remove project"
                                    }
                                }
                            }
                        }
                        button {
                            class: "btn-text",
                            onclick: move |_| {
                                new_projects.write().push(ExperienceProject {
                                    name: String::new(),
                                    context: String::new(),
                                    bullets: vec![String::new()],
                                    tools: Vec::new(),
                                });
                            },
                            "{t_add_project}"
                        }
                    }
                    div { class: "form-actions",
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| {
                                if new_company.read().is_empty() || new_role.read().is_empty() { return; }
                                let projects: Vec<ExperienceProject> = new_projects.read().iter().map(|p| {
                                    ExperienceProject {
                                        name: p.name.clone(),
                                        context: p.context.clone(),
                                        bullets: p.bullets.iter().filter(|b| !b.is_empty()).cloned().collect(),
                                        tools: p.tools.clone(),
                                    }
                                }).filter(|p| !p.name.is_empty() || !p.bullets.is_empty()).collect();
                                cv.write().experiences.push(Experience {
                                    id: new_id(), company: new_company.read().clone(),
                                    role: new_role.read().clone(), location: new_loc.read().clone(),
                                    start_date: new_start.read().clone(), end_date: new_end.read().clone(),
                                    projects,
                                });
                                new_company.set(String::new()); new_role.set(String::new());
                                new_loc.set(String::new());     new_start.set(String::new());
                                new_end.set(t_present.to_string());
                                new_projects.set(vec![ExperienceProject { name: String::new(), context: String::new(), bullets: vec![String::new()], tools: Vec::new() }]);
                                show_form.set(false);
                            },
                            "{t_add_pos}"
                        }
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| { show_form.set(false); },
                            "{t_cancel}"
                        }
                    }
                }
            }
        }
    }
}

// ── Step: Skills ──────────────────────────────────────────────────────────────

#[component]
fn StepSkills(cv: Signal<LifetimeCV>, lang: Signal<i18n::Lang>) -> Element {
    let l = *lang.read();
    let mut new_name = use_signal(String::new);
    let mut new_category = use_signal(|| SkillCategory::Programming);
    let mut new_level = use_signal(|| SkillLevel::Intermediate);

    let skills: Vec<Skill> = cv.read().skills.clone();
    let groups: Vec<(SkillCategory, Vec<(usize, Skill)>)> = SkillCategory::all()
        .into_iter()
        .map(|cat| {
            let items: Vec<(usize, Skill)> = skills
                .iter()
                .enumerate()
                .filter(|(_, s)| s.category == cat)
                .map(|(i, s)| (i, s.clone()))
                .collect();
            (cat, items)
        })
        .filter(|(_, items)| !items.is_empty())
        .collect();

    let t_title = i18n::tr("ed_skills_title", l);
    let t_hint = i18n::tr("ed_skills_hint", l);
    let t_sname = i18n::tr("ed_skill_name", l);
    let t_cat = i18n::tr("ed_category", l);
    let t_level = i18n::tr("ed_level", l);
    let t_add = i18n::tr("ed_add", l);

    rsx! {
        div { class: "form-section",
            h2 { "{t_title}" }
            p { class: "hint", "{t_hint}" }

            div { class: "item-list",
                for (cat, items) in groups {
                    SkillGroup { cat_label: cat.label().to_string(), items, cv }
                }
            }

            div { class: "inline-form inline-form-compact",
                div { class: "form-row form-row-tight",
                    Field { label: t_sname.to_string(),
                        input { r#type: "text", class: "input", placeholder: "Rust",
                            value: new_name.read().clone(),
                            oninput: move |e| { new_name.set(e.value()); },
                        }
                    }
                    Field { label: t_cat.to_string(),
                        select { class: "input select",
                            onchange: move |e| {
                                new_category.set(match e.value().as_str() {
                                    "Framework" => SkillCategory::Framework,
                                    "Tool"      => SkillCategory::Tool,
                                    "Cloud"     => SkillCategory::Cloud,
                                    "Database"  => SkillCategory::Database,
                                    "Soft Skill"=> SkillCategory::Soft,
                                    "Other"     => SkillCategory::Other,
                                    _           => SkillCategory::Programming,
                                });
                            },
                            for cat in SkillCategory::all() {
                                option { value: cat.label(), "{cat.label()}" }
                            }
                        }
                    }
                    Field { label: t_level.to_string(),
                        select { class: "input select",
                            onchange: move |e| {
                                new_level.set(match e.value().as_str() {
                                    "Beginner"     => SkillLevel::Beginner,
                                    "Advanced"     => SkillLevel::Advanced,
                                    "Expert"       => SkillLevel::Expert,
                                    _              => SkillLevel::Intermediate,
                                });
                            },
                            for lvl in SkillLevel::all() {
                                option { value: lvl.label(), "{lvl.label()}" }
                            }
                        }
                    }
                    div { class: "field field-btn",
                        label { class: "label", " " }
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| {
                                if new_name.read().is_empty() { return; }
                                cv.write().skills.push(Skill {
                                    id: new_id(), name: new_name.read().clone(),
                                    category: new_category.read().clone(),
                                    level: new_level.read().clone(),
                                });
                                new_name.set(String::new());
                            },
                            "{t_add}"
                        }
                    }
                }
            }
        }
    }
}

// ── Step: Education ───────────────────────────────────────────────────────────

#[component]
fn StepEducation(cv: Signal<LifetimeCV>, lang: Signal<i18n::Lang>) -> Element {
    let l = *lang.read();
    let mut new_inst = use_signal(String::new);
    let mut new_degree = use_signal(String::new);
    let mut new_field = use_signal(String::new);
    let mut new_start = use_signal(String::new);
    let mut new_end = use_signal(String::new);

    let education: Vec<Education> = cv.read().education.clone();

    let t_title = i18n::tr("ed_edu_title", l);
    let t_inst = i18n::tr("ed_institution", l);
    let t_degree = i18n::tr("ed_degree", l);
    let t_field = i18n::tr("ed_field", l);
    let t_start = i18n::tr("ed_start_year", l);
    let t_end = i18n::tr("ed_end_year", l);
    let t_add = i18n::tr("ed_add_edu", l);

    rsx! {
        div { class: "form-section",
            h2 { "{t_title}" }
            div { class: "item-list",
                for (i, edu) in education.into_iter().enumerate() {
                    EduItem { edu, index: i, cv }
                }
            }
            div { class: "inline-form",
                div { class: "form-row",
                    Field { label: t_inst.to_string(),
                        input { r#type: "text", class: "input", placeholder: "MIT",
                            value: new_inst.read().clone(),
                            oninput: move |e| { new_inst.set(e.value()); },
                        }
                    }
                    Field { label: t_degree.to_string(),
                        input { r#type: "text", class: "input", placeholder: "MSc / BEng",
                            value: new_degree.read().clone(),
                            oninput: move |e| { new_degree.set(e.value()); },
                        }
                    }
                }
                div { class: "form-row",
                    Field { label: t_field.to_string(),
                        input { r#type: "text", class: "input", placeholder: "Computer Science",
                            value: new_field.read().clone(),
                            oninput: move |e| { new_field.set(e.value()); },
                        }
                    }
                    Field { label: t_start.to_string(),
                        input { r#type: "text", class: "input", placeholder: "2019",
                            value: new_start.read().clone(),
                            oninput: move |e| { new_start.set(e.value()); },
                        }
                    }
                    Field { label: t_end.to_string(),
                        input { r#type: "text", class: "input", placeholder: "2021",
                            value: new_end.read().clone(),
                            oninput: move |e| { new_end.set(e.value()); },
                        }
                    }
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        if new_inst.read().is_empty() { return; }
                        cv.write().education.push(Education {
                            id: new_id(), institution: new_inst.read().clone(),
                            degree: new_degree.read().clone(), field: new_field.read().clone(),
                            start_year: new_start.read().clone(), end_year: new_end.read().clone(),
                            achievements: vec![],
                        });
                        new_inst.set(String::new()); new_degree.set(String::new());
                        new_field.set(String::new()); new_start.set(String::new());
                        new_end.set(String::new());
                    },
                    "{t_add}"
                }
            }
        }
    }
}

// ── Step: Projects ────────────────────────────────────────────────────────────

#[component]
fn StepProjects(cv: Signal<LifetimeCV>, lang: Signal<i18n::Lang>) -> Element {
    let l = *lang.read();
    let mut new_name = use_signal(String::new);
    let mut new_desc = use_signal(String::new);
    let mut new_url = use_signal(String::new);
    let mut new_tools = use_signal(String::new);

    let projects: Vec<Project> = cv.read().projects.clone();

    let t_title = i18n::tr("ed_proj_title", l);
    let t_hint = i18n::tr("ed_proj_hint", l);
    let t_pname = i18n::tr("ed_proj_name", l);
    let t_desc = i18n::tr("ed_description", l);
    let t_tools = i18n::tr("ed_tools", l);
    let t_add = i18n::tr("ed_add_proj", l);

    rsx! {
        div { class: "form-section",
            h2 { "{t_title}" }
            p { class: "hint", "{t_hint}" }
            div { class: "item-list",
                for (i, proj) in projects.into_iter().enumerate() {
                    ProjItem { proj, index: i, cv }
                }
            }
            div { class: "inline-form",
                div { class: "form-row",
                    Field { label: t_pname.to_string(),
                        input { r#type: "text", class: "input", placeholder: "CV Generator",
                            value: new_name.read().clone(),
                            oninput: move |e| { new_name.set(e.value()); },
                        }
                    }
                    Field { label: "URL",
                        input { r#type: "url", class: "input", placeholder: "https://github.com/…",
                            value: new_url.read().clone(),
                            oninput: move |e| { new_url.set(e.value()); },
                        }
                    }
                }
                Field { label: t_desc.to_string(),
                    textarea { class: "input textarea", rows: "2",
                        placeholder: "One-sentence description of what it does and why you built it.",
                        value: new_desc.read().clone(),
                        oninput: move |e| { new_desc.set(e.value()); },
                    }
                }
                Field { label: t_tools.to_string(),
                    input { r#type: "text", class: "input", placeholder: "Rust, Dioxus, SQLite",
                        value: new_tools.read().clone(),
                        oninput: move |e| { new_tools.set(e.value()); },
                    }
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        if new_name.read().is_empty() { return; }
                        let tools = new_tools.read().split(',')
                            .map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                        cv.write().projects.push(Project {
                            id: new_id(), name: new_name.read().clone(),
                            description: new_desc.read().clone(), url: new_url.read().clone(),
                            tools, bullets: vec![],
                        });
                        new_name.set(String::new()); new_desc.set(String::new());
                        new_url.set(String::new());  new_tools.set(String::new());
                    },
                    "{t_add}"
                }
            }
        }
    }
}

// ── Step: Languages & Certs ───────────────────────────────────────────────────

#[component]
fn StepLanguages(cv: Signal<LifetimeCV>, lang: Signal<i18n::Lang>) -> Element {
    let l = *lang.read();
    let mut new_lang = use_signal(String::new);
    let mut new_lang_level = use_signal(|| LanguageLevel::Professional);
    let mut new_cert = use_signal(String::new);
    let mut new_issuer = use_signal(String::new);
    let mut new_cert_date = use_signal(String::new);
    let mut new_cert_url = use_signal(String::new);

    let languages: Vec<Language> = cv.read().languages.clone();
    let certifications: Vec<Certification> = cv.read().certifications.clone();

    let t_title = i18n::tr("ed_langs_title", l);
    let t_langs = i18n::tr("ed_languages", l);
    let t_lang = i18n::tr("ed_language", l);
    let t_level = i18n::tr("ed_level", l);
    let t_add = i18n::tr("ed_add", l);
    let t_certs = i18n::tr("ed_certifications", l);
    let t_cname = i18n::tr("ed_cert_name", l);
    let t_issuer = i18n::tr("ed_issuer", l);
    let t_date = i18n::tr("ed_date", l);
    let t_addcert = i18n::tr("ed_add_cert", l);

    rsx! {
        div { class: "form-section",
            h2 { "{t_title}" }

            div { class: "subsection",
                h3 { "{t_langs}" }
                div { class: "item-list",
                    for (i, lang_item) in languages.into_iter().enumerate() {
                        LangItem { lang_item, index: i, cv }
                    }
                }
                div { class: "form-row form-row-tight",
                    Field { label: t_lang.to_string(),
                        input { r#type: "text", class: "input", placeholder: "French",
                            value: new_lang.read().clone(),
                            oninput: move |e| { new_lang.set(e.value()); },
                        }
                    }
                    Field { label: t_level.to_string(),
                        select { class: "input select",
                            onchange: move |e| {
                                new_lang_level.set(match e.value().as_str() {
                                    "Native / Bilingual" => LanguageLevel::Native,
                                    "Professional"       => LanguageLevel::Professional,
                                    _                    => LanguageLevel::Conversational,
                                });
                            },
                            for lvl in LanguageLevel::all() {
                                option { value: lvl.label(), "{lvl.label()}" }
                            }
                        }
                    }
                    div { class: "field field-btn",
                        label { class: "label", " " }
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| {
                                if new_lang.read().is_empty() { return; }
                                cv.write().languages.push(Language {
                                    id: new_id(), name: new_lang.read().clone(),
                                    level: new_lang_level.read().clone(),
                                });
                                new_lang.set(String::new());
                            },
                            "{t_add}"
                        }
                    }
                }
            }

            div { class: "subsection",
                h3 { "{t_certs}" }
                div { class: "item-list",
                    for (i, cert) in certifications.into_iter().enumerate() {
                        CertItem { cert, index: i, cv }
                    }
                }
                div { class: "form-row",
                    Field { label: t_cname.to_string(),
                        input { r#type: "text", class: "input", placeholder: "AWS Solutions Architect",
                            value: new_cert.read().clone(),
                            oninput: move |e| { new_cert.set(e.value()); },
                        }
                    }
                    Field { label: t_issuer.to_string(),
                        input { r#type: "text", class: "input", placeholder: "Amazon Web Services",
                            value: new_issuer.read().clone(),
                            oninput: move |e| { new_issuer.set(e.value()); },
                        }
                    }
                }
                div { class: "form-row",
                    Field { label: t_date.to_string(),
                        input { r#type: "text", class: "input", placeholder: "Jun 2024",
                            value: new_cert_date.read().clone(),
                            oninput: move |e| { new_cert_date.set(e.value()); },
                        }
                    }
                    Field { label: "URL",
                        input { r#type: "url", class: "input", placeholder: "https://…",
                            value: new_cert_url.read().clone(),
                            oninput: move |e| { new_cert_url.set(e.value()); },
                        }
                    }
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        if new_cert.read().is_empty() { return; }
                        cv.write().certifications.push(Certification {
                            id: new_id(), name: new_cert.read().clone(),
                            issuer: new_issuer.read().clone(), date: new_cert_date.read().clone(),
                            url: new_cert_url.read().clone(),
                        });
                        new_cert.set(String::new());     new_issuer.set(String::new());
                        new_cert_date.set(String::new()); new_cert_url.set(String::new());
                    },
                    "{t_addcert}"
                }
            }
        }
    }
}

// ── Step: Done ────────────────────────────────────────────────────────────────

#[component]
fn StepDone(lang: Signal<i18n::Lang>) -> Element {
    let l = *lang.read();
    let nav = use_navigator();
    let t_done = i18n::tr("ed_done_title", l);
    let t_desc = i18n::tr("ed_done_desc", l);
    let t_preview = i18n::tr("ed_done_preview", l);
    let t_tailor = i18n::tr("ed_done_tailor", l);

    rsx! {
        div { class: "form-section done-screen",
            div { class: "done-icon", "✅" }
            h2 { "{t_done}" }
            p { "{t_desc}" }
            div { class: "done-actions",
                button {
                    class: "btn btn-secondary btn-lg",
                    onclick: move |_| { nav.push(crate::router::Route::CvPreview {}); },
                    "{t_preview}"
                }
                button {
                    class: "btn btn-primary btn-lg",
                    onclick: move |_| { nav.push(crate::router::Route::Tailor {}); },
                    "{t_tailor}"
                }
            }
        }
    }
}

// ── Shared Field wrapper ──────────────────────────────────────────────────────

#[component]
fn Field(label: String, required: Option<bool>, children: Element) -> Element {
    let req = required.unwrap_or(false);
    rsx! {
        div { class: "field",
            label { class: "label",
                "{label}"
                if req { span { class: "required", " *" } }
            }
            {children}
        }
    }
}
