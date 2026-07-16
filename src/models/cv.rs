use serde::{Deserialize, Serialize};

// ── Personal ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PersonalInfo {
    pub name: String,
    pub title: String, // "Senior Rust Engineer"
    pub email: String,
    pub phone: String,
    pub location: String,
    pub linkedin: String,
    pub github: String,
    pub website: String,
    pub summary: String, // 2-3 sentence bio
}

// ── Experience ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Experience {
    pub id: String,
    pub company: String,
    pub role: String,
    pub location: String,
    pub start_date: String,   // "Jan 2021"
    pub end_date: String,     // "Present" or "Mar 2024"
    pub bullets: Vec<String>, // achievement bullets, user's exact words
    pub tools: Vec<String>,   // ["Rust", "PostgreSQL", "Kubernetes"]
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
            Self::Other => "Other",
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
    pub degree: String, // "MSc", "BEng", "Bootcamp"
    pub field: String,  // "Computer Science"
    pub start_year: String,
    pub end_year: String, // "Present" or year
    pub achievements: Vec<String>,
}

// ── Projects ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub tools: Vec<String>,
    pub bullets: Vec<String>,
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
    pub fn all_text(&self) -> String {
        let mut parts = vec![self.personal.summary.clone(), self.personal.title.clone()];
        for exp in &self.experiences {
            parts.push(exp.role.clone());
            parts.push(exp.company.clone());
            parts.extend(exp.bullets.clone());
            parts.extend(exp.tools.clone());
        }
        for skill in &self.skills {
            parts.push(skill.name.clone());
        }
        for proj in &self.projects {
            parts.push(proj.name.clone());
            parts.push(proj.description.clone());
            parts.extend(proj.tools.clone());
            parts.extend(proj.bullets.clone());
        }
        parts.join(" ")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
                summary: "Experienced developer".to_string(),
                title: "Rust Engineer".to_string(),
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
                role: "Software Engineer".to_string(),
                company: "Acme".to_string(),
                bullets: vec!["Built APIs".to_string()],
                tools: vec!["Rust".to_string(), "gRPC".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = cv.all_text();
        assert!(text.contains("Software Engineer"));
        assert!(text.contains("Acme"));
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
                description: "A Dioxus app".to_string(),
                tools: vec!["Dioxus".to_string()],
                bullets: vec!["Implemented routing".to_string()],
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
                    tools: vec!["Python".to_string()],
                    ..Default::default()
                },
                Experience {
                    id: "2".to_string(),
                    company: "BetaCo".to_string(),
                    tools: vec!["Go".to_string()],
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
        assert_eq!(SkillCategory::Other.label(), "Other");
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
                tools: vec!["Rust".to_string()],
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
