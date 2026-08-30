use serde::{Deserialize, Deserializer, Serialize};

// ── Localized text ────────────────────────────────────────────────────────────

/// A piece of free-text content stored in both English and French so the same
/// CV can be generated in either language. `get()` returns exactly the
/// requested language — empty if that language hasn't been filled in yet —
/// so the two languages never leak into each other. Use
/// `LifetimeCV::seed_missing_translations` to explicitly copy text across as
/// a translation starting point.
///
/// Deserialization accepts two shapes for backward compatibility with backups
/// created before this field existed: the old plain `"some text"` string
/// (which becomes the same in both languages until edited) and the new
/// `{"en": "...", "fr": "..."}` object.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct LocalizedText {
    pub en: String,
    pub fr: String,
}

impl LocalizedText {
    /// Returns the text for the requested language only — empty string if that
    /// language hasn't been filled in. Deliberately does NOT fall back to the
    /// other language: falling back silently made the two languages look like
    /// a single shared field. If you want to reuse text as a translation
    /// starting point, do it explicitly via `LifetimeCV::seed_missing_translations`.
    pub fn get(&self, lang: crate::i18n_core::Lang) -> &str {
        use crate::i18n_core::Lang;
        match lang {
            Lang::En => &self.en,
            Lang::Fr => &self.fr,
        }
    }

    pub fn set(&mut self, lang: crate::i18n_core::Lang, value: String) {
        use crate::i18n_core::Lang;
        match lang {
            Lang::En => self.en = value,
            Lang::Fr => self.fr = value,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.en.is_empty() && self.fr.is_empty()
    }

    /// If `to` is empty and `from` has content, copies `from`'s text into
    /// `to`. Does nothing if `to` already has content — this never overwrites
    /// an existing translation. Used by `LifetimeCV::seed_missing_translations`.
    pub fn seed_missing(&mut self, from: crate::i18n_core::Lang, to: crate::i18n_core::Lang) {
        if self.get(to).is_empty() && !self.get(from).is_empty() {
            let text = self.get(from).to_string();
            self.set(to, text);
        }
    }

    /// Convenience for migrating/testing: sets both languages to the same text.
    pub fn same(s: impl Into<String>) -> Self {
        let s = s.into();
        Self {
            en: s.clone(),
            fr: s,
        }
    }
}

impl<'de> Deserialize<'de> for LocalizedText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Legacy(String),
            Full {
                #[serde(default)]
                en: String,
                #[serde(default)]
                fr: String,
            },
        }
        Ok(match Repr::deserialize(deserializer)? {
            Repr::Legacy(s) => LocalizedText::same(s),
            Repr::Full { en, fr } => LocalizedText { en, fr },
        })
    }
}

// ── Personal ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PersonalInfo {
    pub name: String,
    pub title: LocalizedText, // "Senior Rust Engineer"
    pub email: String,
    pub phone: String,
    pub location: String,
    pub linkedin: String,
    pub github: String,
    pub website: String,
    pub summary: LocalizedText, // 2-3 sentence bio
}

// ── Experience ────────────────────────────────────────────────────────────────

/// Backward-compatible deserializer for `ExperienceProject::context`: accepts
/// either a single `LocalizedText` (the shape it used to be — collapsed into
/// a one-element list, or an empty list if it was empty) or the current
/// `Vec<LocalizedText>`. See the field's own doc comment for why this exists.
fn deserialize_context<'de, D>(deserializer: D) -> Result<Vec<LocalizedText>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Legacy(LocalizedText),
        List(Vec<LocalizedText>),
    }
    Ok(match Repr::deserialize(deserializer)? {
        Repr::Legacy(lt) if lt.is_empty() => Vec::new(),
        Repr::Legacy(lt) => vec![lt],
        Repr::List(list) => list,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExperienceProject {
    pub name: LocalizedText,
    // A list, not one paragraph, matching `bullets`/`Education::achievements`
    // below — the same shape as everything else in this struct that's
    // free-form user content. `pdf_import::flush_project` already
    // accumulates a "Situation: ..." / "Tasks: ..." intro as separate
    // entries internally (a context-label line like "Tasks:" always
    // starts a fresh entry — see `commit_pending_line`), so keeping that
    // as a list instead of joining it into one string is a natural fit,
    // not a workaround.
    //
    // Deserialization accepts both shapes for backward compatibility with
    // CVs saved before this became a list: the old single `LocalizedText`
    // (wrapped into a one-element list, or dropped entirely if it was
    // empty) and the new array. Without this, loading a pre-existing saved
    // CV would fail to deserialize `ExperienceProject` at all — and
    // `storage::load_cv` swallows that error into `None`, which reads to
    // the user as their whole saved CV silently vanishing, not just this
    // one field. See `LocalizedText`'s own `Deserialize` impl above for the
    // same pattern applied to its earlier plain-string → `{en, fr}` migration.
    #[serde(deserialize_with = "deserialize_context", default)]
    pub context: Vec<LocalizedText>,
    pub bullets: Vec<LocalizedText>,
    // References into `cv.skills` (by `Skill.id`), NOT free-text tool
    // names. Deliberately strict: the editor only lets you pick from
    // skills that already exist in `cv.skills` (reusing the same
    // `skill_ids` pattern `Experience` already uses), rather than
    // free-typed text that drifts in spelling/casing from the CV's
    // canonical skill names (which is exactly what caused several
    // keyword-matching bugs earlier — "K8s" vs "Kubernetes", accents,
    // etc.) and required a same-experience "pooled_tools" fallback in
    // `matcher.rs` for projects that never got a matching tool typed in.
    //
    // No backward-compat migration from the old free-text `tools` field
    // is attempted here: `#[serde(default)]` means a CV saved before this
    // change simply loads with an empty `skill_ids` for every project
    // (the old `tools` array is an unrecognized field to serde, and is
    // silently ignored on load rather than erroring) — the person re-tags
    // projects with the new picker rather than the app trying to guess
    // which old free-text string matches which of their canonical skills.
    #[serde(default)]
    pub skill_ids: Vec<String>,
    // Optional period for this sub-project (e.g. "February 2025" /
    // "February 2026"), distinct from the parent Experience's own dates.
    // Left empty when the source CV doesn't give the project its own
    // date range.
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Experience {
    pub id: String,
    pub company: String,
    pub role: LocalizedText,
    pub location: String,
    pub start_date: String,               // "Jan 2021"
    pub end_date: String,                 // "Present" or "Mar 2024"
    pub projects: Vec<ExperienceProject>, // sub-projects within this role
    #[serde(default)]
    pub skill_ids: Vec<String>, // references to skills defined in cv.skills
}

// ── Skills ────────────────────────────────────────────────────────────────────

// Deliberately 6 categories, not the original 9: `Tool` and `Other` were
// vague catch-alls with no real boundary against the more specific
// categories (almost anything could be "a tool"), and `Framework`/`Soft`
// didn't earn their own bucket often enough to justify the ambiguity they
// added elsewhere. `Middleware` is new — app servers and message brokers
// (JBoss, WebLogic, CICS, Solace) were previously forced into
// `CloudInfrastructure` alongside the actual compute/OS/cloud layer they
// run on top of, which is a different concern.
//
// Rule of thumb for the two categories people most often confuse:
// `Middleware` is software that runs your application/business logic
// *on top of* a platform (app servers, message brokers, ESBs).
// `PlatformsInfrastructure` is the compute/OS/virtualization/cloud/
// container layer everything else sits on (Mainframe, VMware, Linux,
// Kubernetes, Docker, AWS/GCP/etc).
//
// `#[serde(alias = ...)]` lets a CV saved under the old 9-category scheme
// load directly under the new one without a migration pass or any risk of
// a deserialize error on an old file: the removed categories fold into
// their closest surviving neighbor (`Framework`/`Soft` → `Programming`,
// `Tool`/`Other` → `AutomationDevOps`), and the renamed one
// (`CloudInfrastructure` → `PlatformsInfrastructure`) keeps working under
// its old name too. This is a best-effort approximation for the merged
// categories, not a claim that e.g. every old `Tool` entry truly belonged
// in DevOps — worth a skim of your skills list after upgrading.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillCategory {
    #[default]
    #[serde(alias = "Framework", alias = "Soft")]
    Programming,
    #[serde(alias = "CloudInfrastructure")]
    PlatformsInfrastructure,
    #[serde(alias = "Tool", alias = "Other")]
    AutomationDevOps,
    Monitoring,
    Middleware,
    Database,
}

impl SkillCategory {
    pub fn label(&self) -> &str {
        match self {
            Self::Programming => "Programming",
            Self::PlatformsInfrastructure => "Platforms & Infrastructure",
            Self::AutomationDevOps => "Automation & DevOps",
            Self::Monitoring => "Monitoring",
            Self::Middleware => "Middleware",
            Self::Database => "Database",
        }
    }
    pub fn label_fr(&self) -> &str {
        match self {
            Self::Programming => "Programmation",
            Self::PlatformsInfrastructure => "Plateformes & Infrastructure",
            Self::AutomationDevOps => "Automatisation & DevOps",
            Self::Monitoring => "Supervision",
            Self::Middleware => "Middleware",
            Self::Database => "Bases de données",
        }
    }
    pub fn all() -> Vec<Self> {
        vec![
            Self::Programming,
            Self::PlatformsInfrastructure,
            Self::AutomationDevOps,
            Self::Monitoring,
            Self::Middleware,
            Self::Database,
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillLevel {
    Beginner,
    #[default]
    Intermediate,
    Advanced,
    Expert,
    Mastery,
}

impl SkillLevel {
    pub fn label(&self) -> &str {
        match self {
            Self::Beginner => "Beginner",
            Self::Intermediate => "Intermediate",
            Self::Advanced => "Advanced",
            Self::Expert => "Expert",
            Self::Mastery => "Mastery",
        }
    }
    pub fn label_fr(&self) -> &str {
        match self {
            Self::Beginner => "Débutant",
            Self::Intermediate => "Intermédiaire",
            Self::Advanced => "Avancé",
            Self::Expert => "Expert",
            Self::Mastery => "Maîtrise",
        }
    }
    pub fn all() -> Vec<Self> {
        vec![
            Self::Beginner,
            Self::Intermediate,
            Self::Advanced,
            Self::Expert,
            Self::Mastery,
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub category: SkillCategory,
    pub level: SkillLevel,
}

// ── Education ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Education {
    pub id: String,
    pub institution: String,
    pub degree: LocalizedText, // "MSc", "BEng", "Bootcamp"
    pub field: LocalizedText,  // "Computer Science"
    pub start_year: String,
    pub end_year: String, // "Present" or year
    pub achievements: Vec<LocalizedText>,
}

// ── Projects ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: LocalizedText,
    pub url: String,
    pub tools: Vec<String>,
    pub bullets: Vec<LocalizedText>,
}

// ── Languages ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LanguageLevel {
    #[default]
    Conversational,
    Professional,
    Native,
}

impl LanguageLevel {
    pub fn label(&self) -> &str {
        match self {
            Self::Conversational => "Conversational",
            Self::Professional => "Professional",
            Self::Native => "Native / Bilingual",
        }
    }
    pub fn label_fr(&self) -> &str {
        match self {
            Self::Conversational => "Conversationnel",
            Self::Professional => "Professionnel",
            Self::Native => "Bilingue",
        }
    }
    pub fn all() -> Vec<Self> {
        vec![Self::Conversational, Self::Professional, Self::Native]
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Language {
    pub id: String,
    pub name: String,
    pub level: LanguageLevel,
}

// ── Certification ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Certification {
    pub id: String,
    pub name: String,
    pub issuer: String,
    pub date: String,
    pub url: String,
}

// ── Root CV ───────────────────────────────────────────────────────────────────

/// The "lifetime CV" — stores everything the user has ever done.
/// This is the single source of truth; tailored CVs are computed from it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LifetimeCV {
    pub personal: PersonalInfo,
    pub experiences: Vec<Experience>,
    pub skills: Vec<Skill>,
    pub education: Vec<Education>,
    pub projects: Vec<Project>,
    pub languages: Vec<Language>,
    pub certifications: Vec<Certification>,
}

impl LifetimeCV {
    /// All text in the CV concatenated — used by the matcher for gap analysis.
    /// Localized fields contribute both languages, so keyword matching works
    /// whichever language the job description happens to be in.
    pub fn all_text(&self) -> String {
        let mut parts = vec![
            self.personal.summary.en.clone(),
            self.personal.summary.fr.clone(),
            self.personal.title.en.clone(),
            self.personal.title.fr.clone(),
        ];
        for exp in &self.experiences {
            parts.push(exp.role.en.clone());
            parts.push(exp.role.fr.clone());
            parts.push(exp.company.clone());
            for proj in &exp.projects {
                parts.push(proj.name.en.clone());
                parts.push(proj.name.fr.clone());
                parts.extend(proj.context.iter().map(|c| c.en.clone()));
                parts.extend(proj.context.iter().map(|c| c.fr.clone()));
                parts.extend(proj.bullets.iter().map(|b| b.en.clone()));
                parts.extend(proj.bullets.iter().map(|b| b.fr.clone()));
                parts.extend(proj.skill_ids.iter().filter_map(|id| {
                    self.skills
                        .iter()
                        .find(|s| &s.id == id)
                        .map(|s| s.name.clone())
                }));
            }
        }
        for skill in &self.skills {
            parts.push(skill.name.clone());
        }
        for proj in &self.projects {
            parts.push(proj.name.clone());
            parts.push(proj.description.en.clone());
            parts.push(proj.description.fr.clone());
            parts.extend(proj.tools.clone());
            parts.extend(proj.bullets.iter().map(|b| b.en.clone()));
            parts.extend(proj.bullets.iter().map(|b| b.fr.clone()));
        }
        parts.join(" ")
    }

    /// Explicit, opt-in action: for every localized field that's empty in
    /// `to`, copy over whatever text exists in `from` as a starting point to
    /// translate from. Never touches a field that already has content in
    /// `to` — this is purely additive, so it's safe to run repeatedly (e.g.
    /// after adding a new experience) without clobbering translations you've
    /// already written.
    pub fn seed_missing_translations(
        &mut self,
        from: crate::i18n_core::Lang,
        to: crate::i18n_core::Lang,
    ) {
        self.personal.title.seed_missing(from, to);
        self.personal.summary.seed_missing(from, to);
        for exp in &mut self.experiences {
            exp.role.seed_missing(from, to);
            for proj in &mut exp.projects {
                proj.name.seed_missing(from, to);
                for c in &mut proj.context {
                    c.seed_missing(from, to);
                }
                for bullet in &mut proj.bullets {
                    bullet.seed_missing(from, to);
                }
            }
        }
        for edu in &mut self.education {
            edu.degree.seed_missing(from, to);
            edu.field.seed_missing(from, to);
            for a in &mut edu.achievements {
                a.seed_missing(from, to);
            }
        }
        for proj in &mut self.projects {
            proj.description.seed_missing(from, to);
            for bullet in &mut proj.bullets {
                bullet.seed_missing(from, to);
            }
        }
    }

    /// Apply a parsed PDF import. Merges non-empty fields: overwrites empty
    /// fields, appends to lists (skills, experiences, etc.) only if the
    /// current list is empty (to avoid duplicates on re-import).
    pub fn apply_import(&mut self, imported: LifetimeCV) {
        // Personal: overwrite if current is empty
        if self.personal.name.is_empty() {
            self.personal.name = imported.personal.name;
        }
        if self.personal.email.is_empty() {
            self.personal.email = imported.personal.email;
        }
        if self.personal.phone.is_empty() {
            self.personal.phone = imported.personal.phone;
        }
        if self.personal.location.is_empty() {
            self.personal.location = imported.personal.location;
        }
        if self.personal.linkedin.is_empty() {
            self.personal.linkedin = imported.personal.linkedin;
        }
        if self.personal.github.is_empty() {
            self.personal.github = imported.personal.github;
        }
        if self.personal.website.is_empty() {
            self.personal.website = imported.personal.website;
        }
        if self.personal.title.en.is_empty() {
            self.personal.title.en = imported.personal.title.en;
        }
        if self.personal.title.fr.is_empty() {
            self.personal.title.fr = imported.personal.title.fr;
        }
        if self.personal.summary.en.is_empty() {
            self.personal.summary.en = imported.personal.summary.en;
        }
        if self.personal.summary.fr.is_empty() {
            self.personal.summary.fr = imported.personal.summary.fr;
        }
        // Lists: only import if current is empty
        if self.experiences.is_empty() {
            self.experiences = imported.experiences;
        }
        if self.skills.is_empty() {
            self.skills = imported.skills;
        }
        if self.education.is_empty() {
            self.education = imported.education;
        }
        if self.projects.is_empty() {
            self.projects = imported.projects;
        }
        if self.languages.is_empty() {
            self.languages = imported.languages;
        }
        if self.certifications.is_empty() {
            self.certifications = imported.certifications;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n_core::Lang;

    // ── LocalizedText ─────────────────────────────────────────────────────────

    #[test]
    fn get_does_not_fall_back_across_languages() {
        let text = LocalizedText {
            en: "Hello".to_string(),
            fr: String::new(),
        };
        assert_eq!(text.get(Lang::En), "Hello");
        assert_eq!(text.get(Lang::Fr), ""); // must stay empty, not fall back to English
    }

    #[test]
    fn seed_missing_only_fills_empty_target() {
        let mut text = LocalizedText {
            en: "Hello".to_string(),
            fr: String::new(),
        };
        text.seed_missing(Lang::En, Lang::Fr);
        assert_eq!(text.fr, "Hello");

        // Running it again after a translation exists must not clobber it.
        text.fr = "Bonjour".to_string();
        text.seed_missing(Lang::En, Lang::Fr);
        assert_eq!(text.fr, "Bonjour");
    }

    #[test]
    fn backfill_project_ids_backfills_empty_but_keeps_existing() {
        let cv = LifetimeCV {
            personal: PersonalInfo {
                summary: LocalizedText::same("An engineer"),
                ..Default::default()
            },
            experiences: vec![Experience {
                role: LocalizedText::same("Engineer"),
                projects: vec![
                    ExperienceProject {
                        id: String::new(), // legacy CV: id missing
                        name: LocalizedText::same("App"),
                        ..Default::default()
                    },
                    ExperienceProject {
                        id: "existing-id".to_string(),
                        name: LocalizedText::same("Site"),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut cv = cv;
        cv.backfill_project_ids();
        let id0 = &cv.experiences[0].projects[0].id;
        assert!(!id0.is_empty(), "empty id should get a fresh backfill");
        assert_eq!(
            cv.experiences[0].projects[1].id, "existing-id",
            "an existing id must not be replaced"
        );
        let before = cv.experiences[0].projects[0].id.clone();
        cv.backfill_project_ids();
        assert_eq!(&cv.experiences[0].projects[0].id, &before);
    }

    #[test]
    fn seed_missing_translations_fills_cv_without_overwriting() {
        let mut cv = LifetimeCV {
            personal: PersonalInfo {
                summary: LocalizedText {
                    en: "An engineer".to_string(),
                    fr: String::new(),
                },
                ..Default::default()
            },
            experiences: vec![Experience {
                role: LocalizedText {
                    en: "Engineer".to_string(),
                    fr: "Ingénieur".to_string(),
                },
                projects: vec![ExperienceProject {
                    context: vec![LocalizedText {
                        en: "Context".to_string(),
                        fr: String::new(),
                    }],
                    bullets: vec![LocalizedText {
                        en: "Did a thing".to_string(),
                        fr: String::new(),
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        cv.seed_missing_translations(Lang::En, Lang::Fr);
        assert_eq!(cv.personal.summary.fr, "An engineer");
        assert_eq!(cv.experiences[0].role.fr, "Ingénieur"); // untouched, already had content
        assert_eq!(cv.experiences[0].projects[0].context[0].fr, "Context");
        assert_eq!(cv.experiences[0].projects[0].bullets[0].fr, "Did a thing");
    }

    // ── LifetimeCV::default ───────────────────────────────────────────────────

    #[test]
    fn default_cv_is_fully_empty() {
        let cv = LifetimeCV::default();
        assert!(cv.personal.name.is_empty());
        assert!(cv.personal.email.is_empty());
        assert!(cv.experiences.is_empty());
        assert!(cv.skills.is_empty());
        assert!(cv.education.is_empty());
        assert!(cv.projects.is_empty());
        assert!(cv.languages.is_empty());
        assert!(cv.certifications.is_empty());
    }

    // ── LifetimeCV::all_text ──────────────────────────────────────────────────

    #[test]
    fn all_text_empty_cv_returns_whitespace() {
        let text = LifetimeCV::default().all_text();
        assert!(
            text.trim().is_empty(),
            "Empty CV should produce empty all_text"
        );
    }

    #[test]
    fn all_text_includes_personal_summary_and_title() {
        let cv = LifetimeCV {
            personal: PersonalInfo {
                summary: LocalizedText::same("Experienced developer"),
                title: LocalizedText::same("Rust Engineer"),
                ..Default::default()
            },
            ..Default::default()
        };
        let text = cv.all_text();
        assert!(text.contains("Experienced developer"));
        assert!(text.contains("Rust Engineer"));
    }

    #[test]
    fn all_text_includes_experience_fields() {
        let cv = LifetimeCV {
            skills: vec![
                Skill {
                    id: "s-rust".to_string(),
                    name: "Rust".to_string(),
                    ..Default::default()
                },
                Skill {
                    id: "s-grpc".to_string(),
                    name: "gRPC".to_string(),
                    ..Default::default()
                },
            ],
            experiences: vec![Experience {
                id: "1".to_string(),
                role: LocalizedText::same("Software Engineer"),
                company: "Acme".to_string(),
                projects: vec![ExperienceProject {
                    name: LocalizedText::same("API Platform"),
                    context: vec![LocalizedText::same("Legacy monolith needed decomposition")],
                    bullets: vec![LocalizedText::same("Built APIs")],
                    skill_ids: vec!["s-rust".to_string(), "s-grpc".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = cv.all_text();
        assert!(text.contains("Software Engineer"));
        assert!(text.contains("Acme"));
        assert!(text.contains("API Platform"));
        assert!(text.contains("Legacy monolith"));
        assert!(text.contains("Built APIs"));
        assert!(text.contains("Rust"));
        assert!(text.contains("gRPC"));
    }

    #[test]
    fn all_text_includes_skill_names() {
        let cv = LifetimeCV {
            skills: vec![
                Skill {
                    id: "1".to_string(),
                    name: "PostgreSQL".to_string(),
                    category: SkillCategory::Database,
                    level: SkillLevel::Expert,
                },
                Skill {
                    id: "2".to_string(),
                    name: "Tokio".to_string(),
                    category: SkillCategory::Programming,
                    level: SkillLevel::Advanced,
                },
            ],
            ..Default::default()
        };
        let text = cv.all_text();
        assert!(text.contains("PostgreSQL"));
        assert!(text.contains("Tokio"));
    }

    #[test]
    fn all_text_includes_project_fields() {
        let cv = LifetimeCV {
            projects: vec![Project {
                id: "1".to_string(),
                name: "MyProject".to_string(),
                description: LocalizedText::same("A Dioxus app"),
                tools: vec!["Dioxus".to_string()],
                bullets: vec![LocalizedText::same("Implemented routing")],
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = cv.all_text();
        assert!(text.contains("MyProject"));
        assert!(text.contains("A Dioxus app"));
        assert!(text.contains("Dioxus"));
        assert!(text.contains("Implemented routing"));
    }

    #[test]
    fn all_text_multiple_experiences_all_included() {
        let cv = LifetimeCV {
            skills: vec![
                Skill {
                    id: "s-python".to_string(),
                    name: "Python".to_string(),
                    ..Default::default()
                },
                Skill {
                    id: "s-go".to_string(),
                    name: "Go".to_string(),
                    ..Default::default()
                },
            ],
            experiences: vec![
                Experience {
                    id: "1".to_string(),
                    company: "AlphaCo".to_string(),
                    projects: vec![ExperienceProject {
                        skill_ids: vec!["s-python".to_string()],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                Experience {
                    id: "2".to_string(),
                    company: "BetaCo".to_string(),
                    projects: vec![ExperienceProject {
                        skill_ids: vec!["s-go".to_string()],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let text = cv.all_text();
        assert!(text.contains("AlphaCo"));
        assert!(text.contains("BetaCo"));
        assert!(text.contains("Python"));
        assert!(text.contains("Go"));
    }

    // ── SkillCategory ─────────────────────────────────────────────────────────

    #[test]
    fn skill_category_labels_are_correct() {
        assert_eq!(SkillCategory::Programming.label(), "Programming");
        assert_eq!(
            SkillCategory::PlatformsInfrastructure.label(),
            "Platforms & Infrastructure"
        );
        assert_eq!(
            SkillCategory::AutomationDevOps.label(),
            "Automation & DevOps"
        );
        assert_eq!(SkillCategory::Monitoring.label(), "Monitoring");
        assert_eq!(SkillCategory::Middleware.label(), "Middleware");
        assert_eq!(SkillCategory::Database.label(), "Database");
    }

    #[test]
    fn skill_category_french_labels_are_correct() {
        assert_eq!(SkillCategory::Programming.label_fr(), "Programmation");
        assert_eq!(
            SkillCategory::PlatformsInfrastructure.label_fr(),
            "Plateformes & Infrastructure"
        );
        assert_eq!(
            SkillCategory::AutomationDevOps.label_fr(),
            "Automatisation & DevOps"
        );
        assert_eq!(SkillCategory::Monitoring.label_fr(), "Supervision");
        assert_eq!(SkillCategory::Middleware.label_fr(), "Middleware");
        assert_eq!(SkillCategory::Database.label_fr(), "Bases de données");
    }

    #[test]
    fn skill_category_all_covers_every_variant() {
        let all = SkillCategory::all();
        assert_eq!(
            all.len(),
            6,
            "SkillCategory::all() should return all 6 variants"
        );
        assert!(all.contains(&SkillCategory::Programming));
        assert!(all.contains(&SkillCategory::PlatformsInfrastructure));
        assert!(all.contains(&SkillCategory::AutomationDevOps));
        assert!(all.contains(&SkillCategory::Monitoring));
        assert!(all.contains(&SkillCategory::Middleware));
        assert!(all.contains(&SkillCategory::Database));
    }

    // Regression coverage for the 9→6 category migration: a CV saved under
    // the old scheme must still deserialize correctly under the new one,
    // via `#[serde(alias = ...)]` on the surviving variants — see
    // SkillCategory's doc comment for the full mapping and rationale.
    #[test]
    fn skill_category_old_names_deserialize_via_alias() {
        let cases = [
            (r#""Programming""#, SkillCategory::Programming),
            (r#""Framework""#, SkillCategory::Programming),
            (r#""Soft""#, SkillCategory::Programming),
            (
                r#""CloudInfrastructure""#,
                SkillCategory::PlatformsInfrastructure,
            ),
            (r#""Tool""#, SkillCategory::AutomationDevOps),
            (r#""Other""#, SkillCategory::AutomationDevOps),
            (r#""AutomationDevOps""#, SkillCategory::AutomationDevOps),
            (r#""Monitoring""#, SkillCategory::Monitoring),
            (r#""Database""#, SkillCategory::Database),
        ];
        for (json, expected) in cases {
            let got: SkillCategory = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("failed to deserialize {json}: {e}"));
            assert_eq!(got, expected, "deserializing {json}");
        }
    }

    // ── SkillLevel ────────────────────────────────────────────────────────────

    #[test]
    fn skill_level_labels_are_correct() {
        assert_eq!(SkillLevel::Beginner.label(), "Beginner");
        assert_eq!(SkillLevel::Intermediate.label(), "Intermediate");
        assert_eq!(SkillLevel::Advanced.label(), "Advanced");
        assert_eq!(SkillLevel::Expert.label(), "Expert");
        assert_eq!(SkillLevel::Mastery.label(), "Mastery");
    }

    #[test]
    fn skill_level_french_labels_are_correct() {
        assert_eq!(SkillLevel::Beginner.label_fr(), "Débutant");
        assert_eq!(SkillLevel::Intermediate.label_fr(), "Intermédiaire");
        assert_eq!(SkillLevel::Advanced.label_fr(), "Avancé");
        assert_eq!(SkillLevel::Expert.label_fr(), "Expert");
        assert_eq!(SkillLevel::Mastery.label_fr(), "Maîtrise");
    }

    #[test]
    fn skill_level_all_covers_every_variant() {
        let all = SkillLevel::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&SkillLevel::Expert));
        assert!(all.contains(&SkillLevel::Beginner));
        assert!(all.contains(&SkillLevel::Mastery));
    }

    // ── LanguageLevel ─────────────────────────────────────────────────────────

    #[test]
    fn language_level_labels_are_correct() {
        assert_eq!(LanguageLevel::Conversational.label(), "Conversational");
        assert_eq!(LanguageLevel::Professional.label(), "Professional");
        assert_eq!(LanguageLevel::Native.label(), "Native / Bilingual");
    }

    #[test]
    fn language_level_all_covers_every_variant() {
        let all = LanguageLevel::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&LanguageLevel::Native));
    }

    // ── Serde round-trip ──────────────────────────────────────────────────────

    #[test]
    fn lifetime_cv_serialises_and_deserialises() {
        let cv = LifetimeCV {
            personal: PersonalInfo {
                name: "Jane Smith".to_string(),
                email: "jane@example.com".to_string(),
                ..Default::default()
            },
            experiences: vec![Experience {
                id: "e1".to_string(),
                company: "Acme".to_string(),
                projects: vec![ExperienceProject {
                    skill_ids: vec!["s1".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            skills: vec![Skill {
                id: "s1".to_string(),
                name: "Rust".to_string(),
                category: SkillCategory::Programming,
                level: SkillLevel::Expert,
            }],
            ..Default::default()
        };

        let json = serde_json::to_string(&cv).expect("Serialisation failed");
        let restored: LifetimeCV = serde_json::from_str(&json).expect("Deserialisation failed");

        assert_eq!(restored.personal.name, cv.personal.name);
        assert_eq!(restored.personal.email, cv.personal.email);
        assert_eq!(restored.experiences.len(), 1);
        assert_eq!(restored.experiences[0].company, "Acme");
        assert_eq!(restored.skills[0].name, "Rust");
    }

    #[test]
    fn empty_cv_round_trips_cleanly() {
        let cv = LifetimeCV::default();
        let json = serde_json::to_string(&cv).unwrap();
        let back: LifetimeCV = serde_json::from_str(&json).unwrap();
        assert_eq!(cv, back);
    }

    // ── ExperienceProject::context backward compatibility ──────────────────────
    //
    // `context` used to be a single `LocalizedText`, not a `Vec`. These pin
    // down that a CV saved under the old shape still loads correctly under
    // the new one, rather than failing to deserialize (which `storage::
    // load_cv` would silently turn into "your saved CV is gone").

    #[test]
    fn context_deserializes_from_legacy_single_object() {
        let json = r#"{"name":{"en":"","fr":""},"context":{"en":"Old single context","fr":"Ancien contexte"},"bullets":[],"tools":[],"start_date":"","end_date":""}"#;
        let proj: ExperienceProject =
            serde_json::from_str(json).expect("legacy context object should deserialize");
        assert_eq!(proj.context.len(), 1);
        assert_eq!(proj.context[0].en, "Old single context");
        assert_eq!(proj.context[0].fr, "Ancien contexte");
    }

    #[test]
    fn context_deserializes_from_legacy_empty_object_as_empty_list() {
        let json = r#"{"name":{"en":"","fr":""},"context":{"en":"","fr":""},"bullets":[],"tools":[],"start_date":"","end_date":""}"#;
        let proj: ExperienceProject =
            serde_json::from_str(json).expect("legacy empty context should deserialize");
        assert!(proj.context.is_empty());
    }

    #[test]
    fn context_deserializes_from_current_list_shape() {
        let json = r#"{"name":{"en":"","fr":""},"context":[{"en":"First","fr":""},{"en":"Second","fr":""}],"bullets":[],"tools":[],"start_date":"","end_date":""}"#;
        let proj: ExperienceProject =
            serde_json::from_str(json).expect("current context array should deserialize");
        assert_eq!(proj.context.len(), 2);
        assert_eq!(proj.context[0].en, "First");
        assert_eq!(proj.context[1].en, "Second");
    }

    #[test]
    fn context_deserializes_from_legacy_plain_string() {
        // Belt-and-suspenders: LocalizedText itself also accepts a bare
        // string (an even older shape, from before LocalizedText existed).
        // Confirms that compatibility layer keeps working once nested a
        // level deeper under the new Vec.
        let json = r#"{"name":{"en":"","fr":""},"context":"Very old plain string","bullets":[],"tools":[],"start_date":"","end_date":""}"#;
        let proj: ExperienceProject =
            serde_json::from_str(json).expect("legacy plain-string context should deserialize");
        assert_eq!(proj.context.len(), 1);
        assert_eq!(proj.context[0].en, "Very old plain string");
        assert_eq!(proj.context[0].fr, "Very old plain string");
    }

    #[test]
    fn context_missing_entirely_defaults_to_empty_list() {
        // An even older ExperienceProject shape from before `context`
        // existed at all shouldn't fail to load either.
        let json =
            r#"{"name":{"en":"","fr":""},"bullets":[],"tools":[],"start_date":"","end_date":""}"#;
        let proj: ExperienceProject =
            serde_json::from_str(json).expect("missing context field should default");
        assert!(proj.context.is_empty());
    }

    #[test]
    fn is_empty_requires_both_languages_empty() {
        // is_empty is true only when BOTH en and fr are empty. A text with
        // content in one language but not the other is NOT empty (an
        // `||` mutation here would wrongly report it as empty).
        assert!(LocalizedText {
            en: String::new(),
            fr: String::new()
        }
        .is_empty());
        assert!(!LocalizedText {
            en: "x".into(),
            fr: String::new()
        }
        .is_empty());
        assert!(!LocalizedText {
            en: String::new(),
            fr: "x".into()
        }
        .is_empty());
        assert!(!LocalizedText {
            en: "x".into(),
            fr: "y".into()
        }
        .is_empty());
    }

    #[test]
    fn all_text_resolves_skill_ids_to_names_by_exact_id() {
        // all_text must append a skill's name only for the skill whose id
        // EXACTLY matches the project's skill_ids entry. If the id-comparison
        // were inverted (==→!=), a project referencing "s2" would pull in the
        // wrong skill's name ("Rust" here) and never "OnlyHere".
        let cv = LifetimeCV {
            skills: vec![
                Skill {
                    id: "s1".to_string(),
                    name: "Rust".to_string(),
                    ..Default::default()
                },
                Skill {
                    id: "s2".to_string(),
                    name: "OnlyHere".to_string(),
                    ..Default::default()
                },
            ],
            experiences: vec![Experience {
                projects: vec![ExperienceProject {
                    skill_ids: vec!["s2".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = cv.all_text();
        assert!(text.contains("OnlyHere"), "got: {text:?}");
        // "Rust" is the name of the NON-referenced skill. It legitimately
        // appears once (in the skills listing). If the id-comparison were
        // inverted (==→!=), the `find` would wrongly resolve "s2" to the
        // first skill whose id != "s2" (i.e. "Rust"), pulling "Rust" in a
        // second time. Counting occurrences therefore kills that mutant.
        assert_eq!(text.matches("Rust").count(), 1, "got: {text:?}");
    }

    #[test]
    fn apply_import_fills_empty_personal_fields_and_keeps_filled_ones() {
        let mut cv = LifetimeCV {
            personal: PersonalInfo {
                name: "Keep Me".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let imported = LifetimeCV {
            personal: PersonalInfo {
                name: "Imported Name".to_string(),
                email: "x@y.z".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        cv.apply_import(imported.clone());
        assert_eq!(cv.personal.name, "Keep Me");
        assert_eq!(cv.personal.email, "x@y.z");
    }

    #[test]
    fn apply_import_imports_lists_only_when_currently_empty() {
        let mut cv = LifetimeCV {
            skills: vec![Skill {
                id: "s1".to_string(),
                name: "Existing".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let imported = LifetimeCV {
            skills: vec![Skill {
                id: "s2".to_string(),
                name: "Imported".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        cv.apply_import(imported);
        assert_eq!(cv.skills.len(), 1, "filled list must not be overwritten");
        assert_eq!(cv.skills[0].name, "Existing");
    }
}

// ── Tailored CV (output of JD matching) ──────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct TailoredCV {
    pub personal: PersonalInfo,
    pub experiences: Vec<Experience>, // filtered + sorted by relevance
    pub skills: Vec<Skill>,           // filtered + sorted by relevance
    pub education: Vec<Education>,    // always included
    pub projects: Vec<Project>,       // filtered + sorted by relevance
    pub languages: Vec<Language>,     // always included
    pub certifications: Vec<Certification>,
    pub matched_keywords: Vec<String>, // keywords found in your CV
    pub missing_keywords: Vec<String>, // keywords in JD but NOT in your CV
    pub match_score: f32,              // 0.0 – 1.0
}
