use crate::i18n_core::{self, Lang};
use crate::models::{LifetimeCV, SkillCategory, TailoredCV};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn tag(href: &str, label: &str) -> String {
    format!(r#"<span class="tag">{}</span>"#, esc(label)).replace(
        "tag",
        if href.is_empty() {
            "tag"
        } else {
            "tag tag-link"
        },
    )
}

// ── Shared CSS ────────────────────────────────────────────────────────────────

const CV_CSS: &str = r#"
:root { color-scheme: light; }
.cv-doc, .cv-doc * { margin: 0; padding: 0; box-sizing: border-box; }
.cv-doc {
  font-family: 'Segoe UI', Helvetica, Arial, sans-serif;
  color: #1a1a2e;
  max-width: 860px;
  margin: 0 auto;
  padding: 48px 40px;
  font-size: 14px;
  line-height: 1.5;
  background: #fff;
}
.cv-doc .toolbar {
  display: flex;
  gap: 10px;
  margin-bottom: 24px;
  padding-bottom: 16px;
  border-bottom: 1px solid #e8eaf0;
}
.cv-doc .btn {
  padding: 8px 18px;
  border-radius: 6px;
  border: none;
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
}
.cv-doc .btn-primary { background: #2563eb; color: #fff; }
.cv-doc .btn-secondary { background: #f1f5f9; color: #334155; border: 1px solid #e2e8f0; }

/* ── Header ── */
.cv-doc .header { margin-bottom: 28px; }
.cv-doc .name { font-size: 2rem; font-weight: 700; color: #0f172a; letter-spacing: -0.5px; }
.cv-doc .job-title { font-size: 1.05rem; color: #475569; margin-top: 4px; }
.cv-doc .contact {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 20px;
  margin-top: 10px;
  font-size: 0.8rem;
  color: #64748b;
}
.cv-doc .contact a { color: #2563eb; text-decoration: none; }
.cv-doc .summary {
  margin-top: 14px;
  color: #334155;
  font-size: 0.9rem;
  line-height: 1.7;
  max-width: 700px;
}

/* ── Section ── */
.cv-doc .section { margin-bottom: 26px; }
.cv-doc .section-title {
  font-size: 0.68rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  color: #2563eb;
  border-bottom: 1.5px solid #e2e8f0;
  padding-bottom: 5px;
  margin-bottom: 14px;
}

/* ── Experience ── */
.cv-doc .exp-item { margin-bottom: 18px; }
.cv-doc .exp-header { display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; gap: 4px; }
.cv-doc .exp-company { font-weight: 700; color: #0f172a; }
.cv-doc .exp-role { font-style: italic; color: #475569; font-size: 0.9rem; margin-top: 1px; }
.cv-doc .exp-dates { font-size: 0.8rem; color: #94a3b8; white-space: nowrap; }
.cv-doc .exp-location { font-size: 0.8rem; color: #94a3b8; }
.cv-doc .exp-bullets { margin: 8px 0 0 18px; }
.cv-doc .exp-bullets li { color: #334155; margin-bottom: 3px; font-size: 0.88rem; }
.cv-doc .exp-tools { margin-top: 7px; display: flex; flex-wrap: wrap; gap: 5px; }

/* ── Skills ── */
.cv-doc .skills-block { margin-bottom: 8px; }
.cv-doc .skills-category { font-weight: 600; font-size: 0.82rem; color: #475569; display: inline; }
.cv-doc .skills-list { display: inline; font-size: 0.88rem; color: #334155; }

/* ── Projects ── */
.cv-doc .proj-item { margin-bottom: 16px; }
.cv-doc .proj-header { display: flex; justify-content: space-between; align-items: baseline; }
.cv-doc .proj-name { font-weight: 700; color: #0f172a; }
.cv-doc .proj-url { font-size: 0.8rem; color: #2563eb; text-decoration: none; }
.cv-doc .proj-desc { font-size: 0.88rem; color: #475569; margin-top: 3px; }
.cv-doc .proj-bullets { margin: 6px 0 0 18px; }
.cv-doc .proj-bullets li { color: #334155; margin-bottom: 3px; font-size: 0.88rem; }
.cv-doc .proj-tools { margin-top: 6px; display: flex; flex-wrap: wrap; gap: 5px; }

/* ── Education ── */
.cv-doc .edu-item { margin-bottom: 12px; }
.cv-doc .edu-header { display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; }
.cv-doc .edu-inst { font-weight: 700; color: #0f172a; }
.cv-doc .edu-degree { color: #475569; font-size: 0.88rem; margin-top: 2px; }
.cv-doc .edu-dates { font-size: 0.8rem; color: #94a3b8; }
.cv-doc .edu-achievements { margin: 6px 0 0 18px; }
.cv-doc .edu-achievements li { color: #334155; font-size: 0.88rem; margin-bottom: 2px; }

/* ── Languages & Certs ── */
.cv-doc .lang-list { display: flex; flex-wrap: wrap; gap: 12px; }
.cv-doc .lang-item { font-size: 0.88rem; }
.cv-doc .lang-name { font-weight: 600; color: #0f172a; }
.cv-doc .lang-level { color: #64748b; font-size: 0.8rem; }

/* ── Tags ── */
.cv-doc .tag {
  background: #eff6ff;
  color: #2563eb;
  padding: 2px 8px;
  border-radius: 10px;
  font-size: 0.78rem;
  font-weight: 500;
  white-space: nowrap;
}

/* ── Gap analysis (tailored only) ── */
.cv-doc .gap-banner {
  background: #fafafa;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  padding: 14px 18px;
  margin-bottom: 24px;
  font-size: 0.85rem;
}
.cv-doc .gap-score { font-weight: 700; font-size: 1.1rem; color: #2563eb; }
.cv-doc .gap-section { margin-top: 8px; }
.cv-doc .gap-label { font-weight: 600; color: #475569; margin-bottom: 4px; }
.cv-doc .kw-matched { color: #16a34a; background: #f0fdf4; }
.cv-doc .kw-missing { color: #dc2626; background: #fef2f2; }

@media print {
  .cv-doc .toolbar { display: none; }
  .cv-doc { padding: 20px; }
  @page { margin: 1.5cm; }
}

/* ── Keep items intact across page breaks (print & PDF) ── */
.cv-doc .header,
.cv-doc .exp-item,
.cv-doc .proj-item,
.cv-doc .edu-item,
.cv-doc .skills-block,
.cv-doc .gap-banner,
.cv-doc .gap-section {
  break-inside: avoid;
  page-break-inside: avoid;
}
.cv-doc .section-title {
  break-after: avoid;
  page-break-after: avoid;
}
.cv-doc .section-title + * {
  break-before: avoid;
  page-break-before: avoid;
}
"#;

// ── Shared HTML scaffolding ───────────────────────────────────────────────────

fn html_wrap(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8"/>
  <meta name="viewport" content="width=device-width,initial-scale=1"/>
  <meta name="color-scheme" content="light"/>
  <title>{title}</title>
  <style>{css}</style>
</head>
<body>
<div class="cv-doc">
{body}
</div>
</body>
</html>"#,
        title = esc(title),
        css = CV_CSS,
        body = body,
    )
}

// ── Section builders ──────────────────────────────────────────────────────────

// ── Rich text renderer ───────────────────────────────────────────────────────
// Converts plain text (as pasted from LinkedIn) into HTML:
//   - Blank lines → paragraph breaks
//   - Single newlines → <br>
//   - **text** → <strong>text</strong>
//   - HTML special chars are escaped first
fn render_rich_text(text: &str) -> String {
    // First escape HTML
    let escaped = esc(text);
    // Convert **bold** markers
    let bolded = apply_bold(&escaped);
    // Split into paragraphs on blank lines, then preserve single newlines
    bolded
        .split("\n\n")
        .map(|para| {
            let lines = para
                .split('\n')
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("<br>");
            format!("<p>{lines}</p>")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_bold(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consume second *
                          // Collect until next **
            let mut inner = String::new();
            loop {
                match chars.next() {
                    Some('*') if chars.peek() == Some(&'*') => {
                        chars.next();
                        break;
                    }
                    Some(ch) => inner.push(ch),
                    None => {
                        result.push_str("**");
                        result.push_str(&inner);
                        break;
                    }
                }
            }
            result.push_str(&format!("<strong>{inner}</strong>"));
        } else {
            result.push(c);
        }
    }
    result
}

fn render_header(p: &crate::models::PersonalInfo) -> String {
    let mut contacts = vec![];
    if !p.email.is_empty() {
        contacts.push(format!(
            r#"<a href="mailto:{}">{}</a>"#,
            esc(&p.email),
            esc(&p.email)
        ));
    }
    if !p.phone.is_empty() {
        contacts.push(esc(&p.phone));
    }
    if !p.location.is_empty() {
        contacts.push(esc(&p.location));
    }
    if !p.linkedin.is_empty() {
        contacts.push(format!(
            r#"<a href="{}">{}</a>"#,
            esc(&p.linkedin),
            "LinkedIn"
        ));
    }
    if !p.github.is_empty() {
        contacts.push(format!(r#"<a href="{}">{}</a>"#, esc(&p.github), "GitHub"));
    }
    if !p.website.is_empty() {
        contacts.push(format!(
            r#"<a href="{}">{}</a>"#,
            esc(&p.website),
            esc(&p.website)
        ));
    }

    let contact_html = if contacts.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="contact">{}</div>"#, contacts.join(""))
    };

    let summary_html = if p.summary.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="summary">{}</div>"#,
            render_rich_text(&p.summary)
        )
    };

    format!(
        r#"<div class="header">
  <div class="name">{name}</div>
  <div class="job-title">{title}</div>
  {contact}
  {summary}
</div>"#,
        name = esc(&p.name),
        title = esc(&p.title),
        contact = contact_html,
        summary = summary_html,
    )
}

fn render_experience(experiences: &[crate::models::Experience], lang: Lang) -> String {
    if experiences.is_empty() {
        return String::new();
    }
    let mut items = String::new();
    for exp in experiences {
        let bullets: String = exp
            .bullets
            .iter()
            .filter(|b| !b.is_empty())
            .map(|b| format!("<li>{}</li>", esc(b)))
            .collect();
        let bullets_html = if bullets.is_empty() {
            String::new()
        } else {
            format!(r#"<ul class="exp-bullets">{}</ul>"#, bullets)
        };
        let tools_html: String = exp
            .tools
            .iter()
            .filter(|t| !t.is_empty())
            .map(|t| tag("", t))
            .collect::<Vec<_>>()
            .join(" ");
        let tools_div = if tools_html.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="exp-tools">{}</div>"#, tools_html)
        };
        let location_html = if exp.location.is_empty() {
            String::new()
        } else {
            format!(
                r#" · <span class="exp-location">{}</span>"#,
                esc(&exp.location)
            )
        };
        items.push_str(&format!(
            r#"<div class="exp-item">
  <div class="exp-header">
    <span class="exp-company">{company}{location}</span>
    <span class="exp-dates">{start} – {end}</span>
  </div>
  <div class="exp-role">{role}</div>
  {bullets}
  {tools}
</div>"#,
            company = esc(&exp.company),
            location = location_html,
            start = esc(&exp.start_date),
            end = esc(&exp.end_date),
            role = esc(&exp.role),
            bullets = bullets_html,
            tools = tools_div,
        ));
    }
    format!(
        r#"<div class="section"><div class="section-title">{}</div>{}</div>"#,
        i18n_core::tr("rs_experience", lang),
        items
    )
}

fn render_skills(skills: &[crate::models::Skill], lang: Lang) -> String {
    if skills.is_empty() {
        return String::new();
    }

    // Group by category
    let categories = SkillCategory::all();
    let mut blocks = String::new();
    for cat in &categories {
        let cat_skills: Vec<&crate::models::Skill> =
            skills.iter().filter(|s| &s.category == cat).collect();
        if cat_skills.is_empty() {
            continue;
        }
        let names: Vec<String> = cat_skills.iter().map(|s| esc(&s.name)).collect();
        blocks.push_str(&format!(
            r#"<div class="skills-block">
  <span class="skills-category">{cat}: </span>
  <span class="skills-list">{list}</span>
</div>"#,
            cat = cat.label(),
            list = names.join(", "),
        ));
    }
    format!(
        r#"<div class="section"><div class="section-title">{}</div>{}</div>"#,
        i18n_core::tr("rs_skills", lang),
        blocks
    )
}

fn render_projects(projects: &[crate::models::Project], lang: Lang) -> String {
    if projects.is_empty() {
        return String::new();
    }
    let mut items = String::new();
    for proj in projects {
        let bullets: String = proj
            .bullets
            .iter()
            .filter(|b| !b.is_empty())
            .map(|b| format!("<li>{}</li>", esc(b)))
            .collect();
        let bullets_html = if bullets.is_empty() {
            String::new()
        } else {
            format!(r#"<ul class="proj-bullets">{}</ul>"#, bullets)
        };
        let tools_html: String = proj
            .tools
            .iter()
            .filter(|t| !t.is_empty())
            .map(|t| tag("", t))
            .collect::<Vec<_>>()
            .join(" ");
        let tools_div = if tools_html.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="proj-tools">{}</div>"#, tools_html)
        };
        let url_html = if proj.url.is_empty() {
            String::new()
        } else {
            format!(
                r#"<a href="{}" class="proj-url">{}</a>"#,
                esc(&proj.url),
                esc(&proj.url)
            )
        };
        items.push_str(&format!(
            r#"<div class="proj-item">
  <div class="proj-header">
    <span class="proj-name">{name}</span>
    {url}
  </div>
  <div class="proj-desc">{desc}</div>
  {bullets}
  {tools}
</div>"#,
            name = esc(&proj.name),
            url = url_html,
            desc = esc(&proj.description),
            bullets = bullets_html,
            tools = tools_div,
        ));
    }
    format!(
        r#"<div class="section"><div class="section-title">{}</div>{}</div>"#,
        i18n_core::tr("rs_projects", lang),
        items
    )
}

fn render_education(education: &[crate::models::Education], lang: Lang) -> String {
    if education.is_empty() {
        return String::new();
    }
    let mut items = String::new();
    for edu in education {
        let achievements: String = edu
            .achievements
            .iter()
            .filter(|a| !a.is_empty())
            .map(|a| format!("<li>{}</li>", esc(a)))
            .collect();
        let ach_html = if achievements.is_empty() {
            String::new()
        } else {
            format!(r#"<ul class="edu-achievements">{}</ul>"#, achievements)
        };
        items.push_str(&format!(
            r#"<div class="edu-item">
  <div class="edu-header">
    <span class="edu-inst">{inst}</span>
    <span class="edu-dates">{start} – {end}</span>
  </div>
  <div class="edu-degree">{degree} · {field}</div>
  {achievements}
</div>"#,
            inst = esc(&edu.institution),
            start = esc(&edu.start_year),
            end = esc(&edu.end_year),
            degree = esc(&edu.degree),
            field = esc(&edu.field),
            achievements = ach_html,
        ));
    }
    format!(
        r#"<div class="section"><div class="section-title">{}</div>{}</div>"#,
        i18n_core::tr("rs_education", lang),
        items
    )
}

fn render_languages(languages: &[crate::models::Language], lang: Lang) -> String {
    if languages.is_empty() {
        return String::new();
    }
    let items: String = languages.iter().map(|l| {
        format!(
            r#"<div class="lang-item"><span class="lang-name">{name}</span> <span class="lang-level">({level})</span></div>"#,
            name  = esc(&l.name),
            level = l.level.label(),
        )
    }).collect();
    format!(
        r#"<div class="section"><div class="section-title">{}</div><div class="lang-list">{}</div></div>"#,
        i18n_core::tr("rs_languages", lang),
        items
    )
}

fn render_certifications(certs: &[crate::models::Certification], lang: Lang) -> String {
    if certs.is_empty() {
        return String::new();
    }
    let items: String = certs.iter().map(|c| {
        let name = if c.url.is_empty() {
            esc(&c.name)
        } else {
            format!(r#"<a href="{}" class="proj-url">{}</a>"#, esc(&c.url), esc(&c.name))
        };
        format!(
            r#"<div class="lang-item"><span class="lang-name">{name}</span> <span class="lang-level">· {issuer}, {date}</span></div>"#,
            name   = name,
            issuer = esc(&c.issuer),
            date   = esc(&c.date),
        )
    }).collect();
    format!(
        r#"<div class="section"><div class="section-title">{}</div><div class="lang-list">{}</div></div>"#,
        i18n_core::tr("rs_certifications", lang),
        items
    )
}

// ── Public renderers ──────────────────────────────────────────────────────────

/// Render the complete lifetime CV — every item, nothing filtered.
pub fn render_lifetime_cv(cv: &LifetimeCV, lang: Lang) -> String {
    let body = format!(
        "{header}{exp}{skills}{projects}{edu}{lang}{certs}",
        header = render_header(&cv.personal),
        exp = render_experience(&cv.experiences, lang),
        skills = render_skills(&cv.skills, lang),
        projects = render_projects(&cv.projects, lang),
        edu = render_education(&cv.education, lang),
        lang = render_languages(&cv.languages, lang),
        certs = render_certifications(&cv.certifications, lang),
    );
    let title = if cv.personal.name.is_empty() {
        "My CV".to_string()
    } else {
        format!("{} — CV", cv.personal.name)
    };
    html_wrap(&title, &body)
}

/// Render a tailored CV with a gap-analysis banner at the top.
// keep pub(crate) score_color visible to tests
pub(crate) fn score_color_for(score: f32) -> &'static str {
    if score >= 0.6 {
        "#16a34a"
    } else if score >= 0.3 {
        "#d97706"
    } else {
        "#dc2626"
    }
}

/// Render a tailored CV with a gap-analysis banner at the top.
pub fn render_tailored_cv(cv: &TailoredCV, job_title: &str, lang: Lang) -> String {
    let score_pct = (cv.match_score * 100.0).round() as u32;
    let score_color = score_color_for(cv.match_score);

    let matched_tags: String = cv
        .matched_keywords
        .iter()
        .map(|k| format!(r#"<span class="tag kw-matched">{}</span>"#, esc(k)))
        .collect::<Vec<_>>()
        .join(" ");

    let missing_tags: String = cv
        .missing_keywords
        .iter()
        .take(15)
        .map(|k| format!(r#"<span class="tag kw-missing">{}</span>"#, esc(k)))
        .collect::<Vec<_>>()
        .join(" ");

    let gap_banner = format!(
        r#"<div class="gap-banner">
  <div>Match score: <span class="gap-score" style="color:{color}">{score}%</span>
  {for_role}</div>
  {matched}
  {missing}
</div>"#,
        color = score_color,
        score = score_pct,
        for_role = if job_title.is_empty() {
            String::new()
        } else {
            format!(" · <strong>{}</strong>", esc(job_title))
        },
        matched = if matched_tags.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="gap-section"><div class="gap-label">✓ Matched keywords</div>{}</div>"#,
                matched_tags
            )
        },
        missing = if missing_tags.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="gap-section"><div class="gap-label">✗ Gap keywords — consider adding these to your CV</div>{}</div>"#,
                missing_tags
            )
        },
    );

    let body = format!(
        "{banner}{header}{exp}{skills}{projects}{edu}{lang}{certs}",
        banner = gap_banner,
        header = render_header(&cv.personal),
        exp = render_experience(&cv.experiences, lang),
        skills = render_skills(&cv.skills, lang),
        projects = render_projects(&cv.projects, lang),
        edu = render_education(&cv.education, lang),
        lang = render_languages(&cv.languages, lang),
        certs = render_certifications(&cv.certifications, lang),
    );

    let title = if cv.personal.name.is_empty() {
        "Tailored CV".to_string()
    } else {
        format!("{} — Tailored CV", cv.personal.name)
    };
    html_wrap(&title, &body)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    fn minimal_cv() -> LifetimeCV {
        LifetimeCV {
            personal: PersonalInfo {
                name: "Jane Smith".to_string(),
                email: "jane@example.com".to_string(),
                title: "Rust Engineer".to_string(),
                location: "Paris, France".to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn full_cv() -> LifetimeCV {
        let mut cv = minimal_cv();
        cv.experiences.push(Experience {
            id: "e1".to_string(),
            company: "Acme Corp".to_string(),
            role: "Software Engineer".to_string(),
            start_date: "Jan 2021".to_string(),
            end_date: "Present".to_string(),
            bullets: vec!["Built distributed systems".to_string()],
            tools: vec!["Rust".to_string(), "PostgreSQL".to_string()],
            ..Default::default()
        });
        cv.skills.push(Skill {
            id: "s1".to_string(),
            name: "Rust".to_string(),
            category: SkillCategory::Programming,
            level: SkillLevel::Expert,
        });
        cv.skills.push(Skill {
            id: "s2".to_string(),
            name: "PostgreSQL".to_string(),
            category: SkillCategory::Database,
            level: SkillLevel::Advanced,
        });
        cv.education.push(Education {
            id: "edu1".to_string(),
            institution: "MIT".to_string(),
            degree: "MSc".to_string(),
            field: "Computer Science".to_string(),
            start_year: "2017".to_string(),
            end_year: "2019".to_string(),
            achievements: vec![],
        });
        cv.projects.push(Project {
            id: "p1".to_string(),
            name: "CV Generator".to_string(),
            description: "A cool Rust project".to_string(),
            url: "https://github.com/me/cv-gen".to_string(),
            tools: vec!["Rust".to_string()],
            bullets: vec!["Implemented keyword matching".to_string()],
        });
        cv
    }

    // ── HTML structure ────────────────────────────────────────────────────────

    #[test]
    fn empty_cv_does_not_panic() {
        let html = render_lifetime_cv(&LifetimeCV::default(), Lang::En);
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn output_is_complete_html_document() {
        let html = render_lifetime_cv(&minimal_cv(), Lang::En);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<html"));
        assert!(html.contains("</html>"));
        assert!(html.contains("<style>"));
    }

    #[test]
    fn page_title_includes_name() {
        let html = render_lifetime_cv(&minimal_cv(), Lang::En);
        assert!(html.contains("<title>Jane Smith"));
    }

    // ── Personal ──────────────────────────────────────────────────────────────

    #[test]
    fn personal_name_in_output() {
        assert!(render_lifetime_cv(&minimal_cv(), Lang::En).contains("Jane Smith"));
    }

    #[test]
    fn personal_email_is_mailto_link() {
        let html = render_lifetime_cv(&minimal_cv(), Lang::En);
        assert!(html.contains("mailto:jane@example.com"));
    }

    #[test]
    fn personal_location_in_output() {
        assert!(render_lifetime_cv(&minimal_cv(), Lang::En).contains("Paris, France"));
    }

    // ── Experience ────────────────────────────────────────────────────────────

    #[test]
    fn experience_company_and_role_in_output() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("Acme Corp"));
        assert!(html.contains("Software Engineer"));
    }

    #[test]
    fn experience_bullets_in_output() {
        assert!(render_lifetime_cv(&full_cv(), Lang::En).contains("Built distributed systems"));
    }

    #[test]
    fn experience_tools_rendered_as_tags() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("PostgreSQL"));
        assert!(html.contains(r#"class="tag""#));
    }

    #[test]
    fn no_experience_section_when_empty() {
        let html = render_lifetime_cv(&minimal_cv(), Lang::En);
        assert!(!html.contains(">Experience<"));
    }

    // ── Skills ────────────────────────────────────────────────────────────────

    #[test]
    fn skills_show_category_labels() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("Programming"));
        assert!(html.contains("Database"));
    }

    // ── Education ────────────────────────────────────────────────────────────

    #[test]
    fn education_fields_in_output() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("MIT"));
        assert!(html.contains("MSc"));
        assert!(html.contains("Computer Science"));
    }

    // ── Projects ─────────────────────────────────────────────────────────────

    #[test]
    fn project_name_url_description_in_output() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("CV Generator"));
        assert!(html.contains("https://github.com/me/cv-gen"));
        assert!(html.contains("A cool Rust project"));
        assert!(html.contains("Implemented keyword matching"));
    }

    // ── HTML escaping ─────────────────────────────────────────────────────────

    #[test]
    fn xss_in_name_is_escaped() {
        let mut cv = minimal_cv();
        cv.personal.name = "<script>alert('xss')</script>".to_string();
        let html = render_lifetime_cv(&cv, Lang::En);
        assert!(
            !html.contains("<script>alert"),
            "Raw <script> must not appear"
        );
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn ampersand_in_company_is_escaped() {
        let mut cv = minimal_cv();
        cv.experiences.push(Experience {
            id: "e1".to_string(),
            company: "Smith & Jones Ltd".to_string(),
            ..Default::default()
        });
        let html = render_lifetime_cv(&cv, Lang::En);
        assert!(html.contains("Smith &amp; Jones Ltd"));
        assert!(!html.contains("Smith & Jones"));
    }

    // ── Tailored CV ───────────────────────────────────────────────────────────

    #[test]
    fn tailored_shows_score_as_percentage() {
        let cv = TailoredCV {
            match_score: 0.75,
            ..Default::default()
        };
        assert!(render_tailored_cv(&cv, "", Lang::En).contains("75%"));
    }

    #[test]
    fn tailored_shows_job_title() {
        let cv = TailoredCV {
            match_score: 0.5,
            ..Default::default()
        };
        assert!(render_tailored_cv(&cv, "Senior Rust Engineer", Lang::En)
            .contains("Senior Rust Engineer"));
    }

    #[test]
    fn tailored_matched_keywords_use_green_class() {
        let cv = TailoredCV {
            matched_keywords: vec!["rust".to_string()],
            ..Default::default()
        };
        let html = render_tailored_cv(&cv, "", Lang::En);
        assert!(html.contains("rust"));
        assert!(html.contains("kw-matched"));
    }

    #[test]
    fn tailored_missing_keywords_use_red_class() {
        let cv = TailoredCV {
            missing_keywords: vec!["golang".to_string()],
            ..Default::default()
        };
        let html = render_tailored_cv(&cv, "", Lang::En);
        assert!(html.contains("golang"));
        assert!(html.contains("kw-missing"));
    }

    #[test]
    fn tailored_missing_keywords_capped_at_fifteen() {
        let cv = TailoredCV {
            missing_keywords: (0..30).map(|i| format!("keyword{i}")).collect(),
            ..Default::default()
        };
        let html = render_tailored_cv(&cv, "", Lang::En);
        assert!(html.contains("keyword14"), "14th keyword should appear");
        assert!(!html.contains("keyword15"), "15th+ should be truncated");
    }

    // ── score_color_for ───────────────────────────────────────────────────────

    #[test]
    fn score_color_green_high() {
        assert_eq!(score_color_for(1.00), "#16a34a");
    }
    #[test]
    fn score_color_green_exact() {
        assert_eq!(score_color_for(0.60), "#16a34a");
    }
    #[test]
    fn score_color_amber_mid() {
        assert_eq!(score_color_for(0.50), "#d97706");
    }
    #[test]
    fn score_color_amber_exact() {
        assert_eq!(score_color_for(0.30), "#d97706");
    }
    #[test]
    fn score_color_red_low() {
        assert_eq!(score_color_for(0.00), "#dc2626");
    }
    #[test]
    fn score_color_red_just_under_amber() {
        assert_eq!(score_color_for(0.29), "#dc2626");
    }
}
