use crate::i18n;
use crate::router::Route;
use cv_generator::models::LifetimeCV;
use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    let cv: Signal<LifetimeCV> = use_context();
    let lang: Signal<i18n::Lang> = use_context();
    let l = *lang.read();
    let cv_ref = cv.read();

    let exp_count = cv_ref.experiences.len();
    let skill_count = cv_ref.skills.len();
    let has_personal = !cv_ref.personal.name.is_empty();
    let display_name = if has_personal {
        cv_ref.personal.name.clone()
    } else {
        i18n::tr("home_not_filled", l).to_string()
    };

    let t_title = i18n::tr("home_title", l);
    let t_subtitle = i18n::tr("home_subtitle", l);
    let t_personal = i18n::tr("home_personal", l);
    let t_experience = i18n::tr("home_experience", l);
    let t_skills = i18n::tr("home_skills", l);
    let t_positions = i18n::tr("home_positions", l);
    let t_skills_st = i18n::tr("home_skills_stored", l);
    let t_s1t = i18n::tr("home_step1_title", l);
    let t_s1d = i18n::tr("home_step1_desc", l);
    let t_s1b = i18n::tr("home_step1_btn", l);
    let t_s2t = i18n::tr("home_step2_title", l);
    let t_s2d = i18n::tr("home_step2_desc", l);
    let t_s2b = i18n::tr("home_step2_btn", l);
    let t_s3t = i18n::tr("home_step3_title", l);
    let t_s3d = i18n::tr("home_step3_desc", l);
    let t_s3b = i18n::tr("home_step3_btn", l);
    let t_s4t = i18n::tr("home_step4_title", l);
    let t_s4d = i18n::tr("home_step4_desc", l);
    let t_s4b = i18n::tr("home_step4_btn", l);

    rsx! {
        div { class: "page",
            div { class: "page-header",
                h1 { "{t_title}" }
                p { class: "subtitle", "{t_subtitle}" }
            }

            div { class: "cards",
                div { class: "card",
                    div { class: "card-icon", "👤" }
                    div { class: "card-body",
                        h3 { "{t_personal}" }
                        p { "{display_name}" }
                    }
                    div { class: "card-status",
                        if has_personal {
                            span { class: "badge badge-ok", "✓" }
                        } else {
                            span { class: "badge badge-warn", "!" }
                        }
                    }
                }
                div { class: "card",
                    div { class: "card-icon", "💼" }
                    div { class: "card-body",
                        h3 { "{t_experience}" }
                        p { "{exp_count} {t_positions}" }
                    }
                    div { class: "card-status",
                        if exp_count > 0 {
                            span { class: "badge badge-ok", "✓" }
                        } else {
                            span { class: "badge badge-warn", "0" }
                        }
                    }
                }
                div { class: "card",
                    div { class: "card-icon", "🛠️" }
                    div { class: "card-body",
                        h3 { "{t_skills}" }
                        p { "{skill_count} {t_skills_st}" }
                    }
                    div { class: "card-status",
                        if skill_count > 0 {
                            span { class: "badge badge-ok", "✓" }
                        } else {
                            span { class: "badge badge-warn", "0" }
                        }
                    }
                }
            }

            div { class: "action-grid",
                div { class: "action-card",
                    h2 { "{t_s1t}" }
                    p { "{t_s1d}" }
                    Link { to: Route::CvEditor {},
                        button { class: "btn btn-primary btn-lg", "{t_s1b}" }
                    }
                }
                div { class: "action-card",
                    h2 { "{t_s2t}" }
                    p { "{t_s2d}" }
                    Link { to: Route::CvPreview {},
                        button {
                            class: if has_personal && exp_count > 0 {
                                "btn btn-secondary btn-lg"
                            } else {
                                "btn btn-secondary btn-lg btn-disabled"
                            },
                            "{t_s2b}"
                        }
                    }
                }
                div { class: "action-card action-card-highlight",
                    h2 { "{t_s3t}" }
                    p { "{t_s3d}" }
                    Link { to: Route::Tailor {},
                        button {
                            class: if has_personal {
                                "btn btn-primary btn-lg"
                            } else {
                                "btn btn-secondary btn-lg btn-disabled"
                            },
                            "{t_s3b}"
                        }
                    }
                }
                div { class: "action-card",
                    h2 { "{t_s4t}" }
                    p { "{t_s4d}" }
                    Link { to: Route::Sync {},
                        button { class: "btn btn-secondary btn-lg", "{t_s4b}" }
                    }
                }
            }
        }
    }
}
