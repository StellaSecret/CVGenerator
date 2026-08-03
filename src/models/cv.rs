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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExperienceProject {
    pub name: LocalizedText,
    pub context: LocalizedText,
    pub bullets: Vec<LocalizedText>,
    pub tools: Vec<String>,
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
}

// ── Skills ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillCategory {
    #[default]
    Programming,
    Framework,
    Tool,
    Cloud,
    Database,
    Soft,
    Other,
}

impl SkillCategory {
    pub fn label(&self) -> &str {
        match self {
            Self::Programming => "Programming",
            Self::Framework => "Framework",
            Self::Tool => "Tool",
            Self::Cloud => "Cloud",
            Self::Database => "Database",
            Self::Soft => "Soft Skill",
            Self::Other => "Other Skills",
        }
    }
    pub fn all() -> Vec<Self> {
        vec![
            Self::Programming,
            Self::Framework,
            Self::Tool,
            Self::Cloud,
            Self::Database,
            Self::Soft,
            Self::Other,
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
}

impl SkillLevel {
    pub fn label(&self) -> &str {
        match self {
            Self::Beginner => "Beginner",
            Self::Intermediate => "Intermediate",
            Self::Advanced => "Advanced",
            Self::Expert => "Expert",
        }
    }
    pub fn all() -> Vec<Self> {
        vec![
            Self::Beginner,
            Self::Intermediate,
            Self::Advanced,
            Self::Expert,
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
                parts.push(proj.context.en.clone());
                parts.push(proj.context.fr.clone());
                parts.extend(proj.bullets.iter().map(|b| b.en.clone()));
                parts.extend(proj.bullets.iter().map(|b| b.fr.clone()));
                parts.extend(proj.tools.clone());
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
                proj.context.seed_missing(from, to);
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
                    context: LocalizedText {
                        en: "Context".to_string(),
                        fr: String::new(),
                    },
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
        assert_eq!(cv.experiences[0].projects[0].context.fr, "Context");
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
            experiences: vec![Experience {
                id: "1".to_string(),
                role: LocalizedText::same("Software Engineer"),
                company: "Acme".to_string(),
                projects: vec![ExperienceProject {
                    name: LocalizedText::same("API Platform"),
                    context: LocalizedText::same("Legacy monolith needed decomposition"),
                    bullets: vec![LocalizedText::same("Built APIs")],
                    tools: vec!["Rust".to_string(), "gRPC".to_string()],
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
                    category: SkillCategory::Framework,
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
            experiences: vec![
                Experience {
                    id: "1".to_string(),
                    company: "AlphaCo".to_string(),
                    projects: vec![ExperienceProject {
                        tools: vec!["Python".to_string()],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                Experience {
                    id: "2".to_string(),
                    company: "BetaCo".to_string(),
                    projects: vec![ExperienceProject {
                        tools: vec!["Go".to_string()],
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
        assert_eq!(SkillCategory::Framework.label(), "Framework");
        assert_eq!(SkillCategory::Tool.label(), "Tool");
        assert_eq!(SkillCategory::Cloud.label(), "Cloud");
        assert_eq!(SkillCategory::Database.label(), "Database");
        assert_eq!(SkillCategory::Soft.label(), "Soft Skill");
        assert_eq!(SkillCategory::Other.label(), "Other Skills");
    }

    #[test]
    fn skill_category_all_covers_every_variant() {
        let all = SkillCategory::all();
        assert_eq!(
            all.len(),
            7,
            "SkillCategory::all() should return all 7 variants"
        );
        assert!(all.contains(&SkillCategory::Programming));
        assert!(all.contains(&SkillCategory::Cloud));
        assert!(all.contains(&SkillCategory::Database));
    }

    // ── SkillLevel ────────────────────────────────────────────────────────────

    #[test]
    fn skill_level_labels_are_correct() {
        assert_eq!(SkillLevel::Beginner.label(), "Beginner");
        assert_eq!(SkillLevel::Intermediate.label(), "Intermediate");
        assert_eq!(SkillLevel::Advanced.label(), "Advanced");
        assert_eq!(SkillLevel::Expert.label(), "Expert");
    }

    #[test]
    fn skill_level_all_covers_every_variant() {
        let all = SkillLevel::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&SkillLevel::Expert));
        assert!(all.contains(&SkillLevel::Beginner));
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
                    tools: vec!["Rust".to_string()],
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
