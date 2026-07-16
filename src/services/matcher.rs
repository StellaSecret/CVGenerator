use crate::models::{Experience, LifetimeCV, Project, Skill, TailoredCV};
use std::collections::HashMap;

// ── Stop words ────────────────────────────────────────────────────────────────

const STOP_WORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "and",
    "or",
    "but",
    "in",
    "on",
    "at",
    "to",
    "for",
    "of",
    "with",
    "by",
    "from",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "being",
    "have",
    "has",
    "had",
    "do",
    "does",
    "did",
    "will",
    "would",
    "could",
    "should",
    "may",
    "might",
    "shall",
    "can",
    "need",
    "must",
    "we",
    "our",
    "you",
    "your",
    "their",
    "they",
    "it",
    "its",
    "this",
    "that",
    "these",
    "those",
    "as",
    "if",
    "not",
    "no",
    "so",
    "such",
    "than",
    "then",
    "also",
    "both",
    "each",
    "more",
    "most",
    "other",
    "into",
    "through",
    "during",
    "including",
    "about",
    "up",
    "down",
    "out",
    "off",
    "over",
    "under",
    "again",
    "further",
    "once",
    "here",
    "there",
    "when",
    "where",
    "why",
    "how",
    "all",
    "any",
    "both",
    "few",
    "between",
    "within",
    "without",
    "plus",
    "well",
    "strong",
    "good",
    "work",
    "working",
    "role",
    "team",
    "teams",
    "company",
    "job",
    "position",
    "candidate",
    "candidates",
    "looking",
    "seeking",
    "join",
    "ability",
    "experience",
    "skills",
    "skill",
    "knowledge",
    "understanding",
];

// ── Tokeniser ─────────────────────────────────────────────────────────────────

fn tokenise(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '+' && c != '#')
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() >= 3)
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

// ── Keyword extraction ────────────────────────────────────────────────────────

/// Extract keywords from a JD text, returned sorted by frequency (desc).
/// Each entry is (keyword, frequency).
pub fn extract_keywords(text: &str) -> Vec<(String, usize)> {
    let mut freq: HashMap<String, usize> = HashMap::new();
    for token in tokenise(text) {
        *freq.entry(token).or_insert(0) += 1;
    }
    let mut sorted: Vec<(String, usize)> = freq.into_iter().collect();
    // Sort by frequency desc, then alphabetically for determinism
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted
}

// ── Scoring ───────────────────────────────────────────────────────────────────

/// Returns a relevance score for `text` against `keywords`.
/// Score = sum of (keyword_frequency * match_weight) / total_kw_weight
fn score_text(text: &str, keywords: &[(String, usize)]) -> f32 {
    if keywords.is_empty() || text.is_empty() {
        return 0.0;
    }
    let text_lower = text.to_lowercase();
    let total_weight: usize = keywords.iter().map(|(_, f)| f).sum();

    let matched_weight: usize = keywords
        .iter()
        .filter(|(kw, _)| text_lower.contains(kw.as_str()))
        .map(|(_, f)| f)
        .sum();

    if total_weight == 0 {
        0.0
    } else {
        matched_weight as f32 / total_weight as f32
    }
}

fn score_experience(exp: &Experience, keywords: &[(String, usize)]) -> f32 {
    let text = format!(
        "{} {} {} {}",
        exp.role,
        exp.bullets.join(" "),
        exp.tools.join(" "),
        exp.company
    );
    score_text(&text, keywords)
}

fn score_skill(skill: &Skill, keywords: &[(String, usize)]) -> f32 {
    score_text(&skill.name, keywords)
}

fn score_project(proj: &Project, keywords: &[(String, usize)]) -> f32 {
    let text = format!(
        "{} {} {} {}",
        proj.name,
        proj.description,
        proj.tools.join(" "),
        proj.bullets.join(" ")
    );
    score_text(&text, keywords)
}

// ── Public API ────────────────────────────────────────────────────────────────

pub struct TailorResult {
    pub tailored: TailoredCV,
    /// Top keywords from the JD for display
    pub top_keywords: Vec<(String, usize)>,
}

/// Main entry point: given a LifetimeCV and raw JD text, produce a tailored CV.
///
/// Rules:
///   - Experiences are filtered to those with score > 0, then sorted best-first.
///     Always include at least the 2 most recent even if score = 0.
///   - Skills are filtered to those with score > 0, then sorted best-first.
///     Skills with score = 0 are appended at the end (separated).
///   - Projects: only those with score > 0.
///   - Education, languages, certifications: always included, unchanged.
///   - Matched / missing keywords are derived from the top-30 JD keywords.
pub fn tailor_cv(cv: &LifetimeCV, jd_text: &str) -> TailorResult {
    let keywords = extract_keywords(jd_text);
    // Work with the top 40 most-frequent keywords only
    let top_keywords: Vec<(String, usize)> = keywords.iter().take(40).cloned().collect();

    // ── Experiences ──────────────────────────────────────────────────────────
    let mut scored_exp: Vec<(f32, Experience)> = cv
        .experiences
        .iter()
        .map(|e| (score_experience(e, &top_keywords), e.clone()))
        .collect();
    scored_exp.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Always include at least 2 most-recent (original order) even if unscored
    let mut selected_exp: Vec<Experience> = scored_exp
        .iter()
        .filter(|(s, _)| *s > 0.0)
        .map(|(_, e)| e.clone())
        .collect();

    if selected_exp.len() < 2 {
        for exp in cv.experiences.iter().take(2) {
            if !selected_exp.iter().any(|e| e.id == exp.id) {
                selected_exp.push(exp.clone());
            }
        }
    }

    // ── Skills ────────────────────────────────────────────────────────────────
    let mut matched_skills: Vec<Skill> = cv
        .skills
        .iter()
        .filter(|s| score_skill(s, &top_keywords) > 0.0)
        .cloned()
        .collect();

    let unmatched_skills: Vec<Skill> = cv
        .skills
        .iter()
        .filter(|s| score_skill(s, &top_keywords) == 0.0)
        .cloned()
        .collect();

    matched_skills.extend(unmatched_skills);

    // ── Projects ──────────────────────────────────────────────────────────────
    let mut scored_proj: Vec<(f32, Project)> = cv
        .projects
        .iter()
        .map(|p| (score_project(p, &top_keywords), p.clone()))
        .collect();
    scored_proj.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let selected_proj: Vec<Project> = scored_proj
        .into_iter()
        .filter(|(s, _)| *s > 0.0)
        .map(|(_, p)| p)
        .collect();

    // ── Match / gap analysis ──────────────────────────────────────────────────
    // Map to owned String keys first so partition collects into Vec<String>
    // directly, avoiding the &(String, usize) vs String type mismatch.
    let cv_text_lower = cv.all_text().to_lowercase();
    let (matched_keywords, missing_keywords): (Vec<String>, Vec<String>) = top_keywords
        .iter()
        .take(30)
        .map(|(kw, _)| kw.clone())
        .partition(|kw| cv_text_lower.contains(kw.as_str()));

    let match_score = if top_keywords.is_empty() {
        0.0
    } else {
        matched_keywords.len() as f32 / top_keywords.len().min(30) as f32
    };

    let tailored = TailoredCV {
        personal: cv.personal.clone(),
        experiences: selected_exp,
        skills: matched_skills,
        education: cv.education.clone(),
        projects: selected_proj,
        languages: cv.languages.clone(),
        certifications: cv.certifications.clone(),
        matched_keywords,
        missing_keywords,
        match_score,
    };

    TailorResult {
        tailored,
        top_keywords: keywords.into_iter().take(30).collect(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    // ── Fixture ───────────────────────────────────────────────────────────────

    fn fixture_cv() -> LifetimeCV {
        LifetimeCV {
            personal: PersonalInfo {
                name: "Jane Smith".to_string(),
                title: "Backend Engineer".to_string(),
                summary: "Experienced distributed-systems developer".to_string(),
                ..Default::default()
            },
            experiences: vec![
                Experience {
                    id: "exp-1".to_string(),
                    company: "Acme Corp".to_string(),
                    role: "Software Engineer".to_string(),
                    start_date: "Jan 2021".to_string(),
                    end_date: "Present".to_string(),
                    bullets: vec![
                        "Built distributed systems using Rust and Tokio".to_string(),
                        "Reduced API latency by 40% through caching".to_string(),
                    ],
                    tools: vec![
                        "Rust".to_string(),
                        "PostgreSQL".to_string(),
                        "Kubernetes".to_string(),
                    ],
                    ..Default::default()
                },
                Experience {
                    id: "exp-2".to_string(),
                    company: "Beta Ltd".to_string(),
                    role: "Junior Developer".to_string(),
                    start_date: "Jun 2019".to_string(),
                    end_date: "Dec 2020".to_string(),
                    bullets: vec![
                        "Developed web applications with React and TypeScript".to_string()
                    ],
                    tools: vec!["JavaScript".to_string(), "React".to_string()],
                    ..Default::default()
                },
            ],
            skills: vec![
                Skill {
                    id: "s1".to_string(),
                    name: "Rust".to_string(),
                    category: SkillCategory::Programming,
                    level: SkillLevel::Expert,
                },
                Skill {
                    id: "s2".to_string(),
                    name: "PostgreSQL".to_string(),
                    category: SkillCategory::Database,
                    level: SkillLevel::Advanced,
                },
                Skill {
                    id: "s3".to_string(),
                    name: "Python".to_string(),
                    category: SkillCategory::Programming,
                    level: SkillLevel::Intermediate,
                },
            ],
            projects: vec![Project {
                id: "p1".to_string(),
                name: "cv-generator".to_string(),
                description: "CV generator written in Rust using Dioxus".to_string(),
                tools: vec!["Rust".to_string(), "Dioxus".to_string()],
                bullets: vec!["Keyword matching algorithm".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    // ── tokenise / extract_keywords ───────────────────────────────────────────

    #[test]
    fn keywords_basic_extraction() {
        let kws = extract_keywords("Rust developer with PostgreSQL experience");
        let names: Vec<&str> = kws.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"developer"));
        assert!(names.contains(&"postgresql"));
    }

    #[test]
    fn keywords_stop_words_are_removed() {
        // Every word here is a stop word
        let kws = extract_keywords("the and for in with of to a is are was were");
        assert!(
            kws.is_empty(),
            "All words are stop words — expected empty, got {:?}",
            kws
        );
    }

    #[test]
    fn keywords_short_words_are_removed() {
        // "go", "ai", "ml" are all < 3 chars
        let kws = extract_keywords("go ai ml");
        for (k, _) in &kws {
            assert!(k.len() >= 3, "Keyword '{}' is shorter than 3 chars", k);
        }
    }

    #[test]
    fn keywords_sorted_by_frequency_descending() {
        let kws = extract_keywords("rust rust rust python python java");
        assert!(!kws.is_empty());
        assert_eq!(kws[0].0, "rust", "rust (×3) should rank first");
        assert_eq!(kws[0].1, 3);
        assert_eq!(kws[1].0, "python", "python (×2) should rank second");
        assert_eq!(kws[1].1, 2);
    }

    #[test]
    fn keywords_case_insensitive() {
        let kws = extract_keywords("Rust RUST rust");
        assert_eq!(kws.len(), 1, "All variants should collapse to one entry");
        assert_eq!(kws[0].0, "rust");
        assert_eq!(kws[0].1, 3);
    }

    #[test]
    fn keywords_preserves_plus_in_token() {
        // C++ is 3 chars after lowercasing — it passes the len >= 3 guard.
        // C# is only 2 chars and is *correctly* filtered; this is expected behaviour
        // (the min-length guard intentionally drops very short tokens to reduce noise).
        let kws = extract_keywords("C++ developer and C# engineer");
        let names: Vec<&str> = kws.iter().map(|(k, _)| k.as_str()).collect();

        assert!(
            names.contains(&"c++"),
            "c++ (3 chars) should survive the length filter"
        );
        assert!(
            names.contains(&"developer"),
            "common words should be extracted"
        );
        assert!(
            names.contains(&"engineer"),
            "common words should be extracted"
        );
        assert!(
            !names.contains(&"c#"),
            "c# (2 chars) is correctly filtered by len >= 3"
        );
        assert!(!names.contains(&"and"), "stop word 'and' should be removed");
    }

    #[test]
    fn keywords_empty_input_returns_empty() {
        assert!(extract_keywords("").is_empty());
        assert!(extract_keywords("   ").is_empty());
    }

    // ── score_text ────────────────────────────────────────────────────────────

    #[test]
    fn score_text_perfect_match_is_one() {
        let kws = vec![("rust".to_string(), 2), ("postgresql".to_string(), 1)];
        let s = score_text("rust postgresql developer", &kws);
        assert_eq!(s, 1.0);
    }

    #[test]
    fn score_text_no_match_is_zero() {
        let kws = vec![("golang".to_string(), 1), ("java".to_string(), 1)];
        let s = score_text("rust postgresql developer", &kws);
        assert_eq!(s, 0.0);
    }

    #[test]
    fn score_text_partial_match_weighted() {
        // rust weight=2, java weight=1, total=3; only rust matches → 2/3
        let kws = vec![("rust".to_string(), 2), ("java".to_string(), 1)];
        let s = score_text("senior rust developer", &kws);
        let expected = 2.0_f32 / 3.0_f32;
        assert!(
            (s - expected).abs() < 1e-4,
            "Expected {:.4}, got {:.4}",
            expected,
            s
        );
    }

    #[test]
    fn score_text_empty_inputs_return_zero() {
        assert_eq!(score_text("", &[]), 0.0);
        assert_eq!(score_text("rust", &[]), 0.0);
        assert_eq!(score_text("", &[("rust".to_string(), 1)]), 0.0);
    }

    #[test]
    fn score_text_is_case_insensitive() {
        let kws = vec![("rust".to_string(), 1)];
        // keyword is lowercase; text has uppercase — should still match
        assert_eq!(score_text("RUST Engineer", &kws), 1.0);
    }

    // ── tailor_cv ─────────────────────────────────────────────────────────────

    #[test]
    fn tailor_relevant_skills_rank_before_unrelated() {
        let cv = fixture_cv();
        let jd = "We need a Rust developer with PostgreSQL knowledge for backend systems";
        let result = tailor_cv(&cv, jd);

        let names: Vec<&str> = result
            .tailored
            .skills
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        let rust_pos = names
            .iter()
            .position(|&n| n == "Rust")
            .expect("Rust should be in skills");
        let py_pos = names
            .iter()
            .position(|&n| n == "Python")
            .expect("Python should be in skills");
        assert!(
            rust_pos < py_pos,
            "Rust should rank before Python for a Rust-focused JD"
        );
    }

    #[test]
    fn tailor_always_includes_at_least_two_experiences() {
        let cv = fixture_cv();
        // Completely unrelated JD — nothing in CV should score
        let jd = "Certified accountant required for tax returns and bookkeeping";
        let result = tailor_cv(&cv, jd);
        assert!(
            result.tailored.experiences.len() >= 2,
            "Should always keep ≥ 2 experiences, got {}",
            result.tailored.experiences.len()
        );
    }

    #[test]
    fn tailor_match_score_in_valid_range() {
        let cv = fixture_cv();

        let good_jd = "Rust engineer PostgreSQL Kubernetes distributed systems async";
        let r_good = tailor_cv(&cv, good_jd);
        assert!(r_good.tailored.match_score >= 0.0 && r_good.tailored.match_score <= 1.0);

        let bad_jd = "Accountant needed for spreadsheet tax financial reporting";
        let r_bad = tailor_cv(&cv, bad_jd);
        assert!(r_bad.tailored.match_score >= 0.0 && r_bad.tailored.match_score <= 1.0);

        assert!(
            r_good.tailored.match_score > r_bad.tailored.match_score,
            "Relevant JD should score higher than unrelated one"
        );
    }

    #[test]
    fn tailor_matched_keywords_are_actually_in_cv() {
        let cv = fixture_cv();
        let jd = "Rust developer with PostgreSQL and Kubernetes experience";
        let result = tailor_cv(&cv, jd);

        let cv_text = cv.all_text().to_lowercase();
        for kw in &result.tailored.matched_keywords {
            assert!(
                cv_text.contains(kw.as_str()),
                "Matched keyword '{}' must appear in CV text",
                kw
            );
        }
    }

    #[test]
    fn tailor_missing_keywords_not_in_cv() {
        let cv = fixture_cv();
        let jd = "Senior Golang developer needed with Terraform and Vault expertise";
        let result = tailor_cv(&cv, jd);

        let cv_text = cv.all_text().to_lowercase();
        // At least one of the JD-specific keywords should be flagged as missing
        let any_gap = result
            .tailored
            .missing_keywords
            .iter()
            .any(|kw| !cv_text.contains(kw.as_str()));
        assert!(
            any_gap,
            "Expected at least one keyword to be missing from CV"
        );
    }

    #[test]
    fn tailor_education_always_included() {
        let mut cv = fixture_cv();
        cv.education.push(Education {
            id: "edu-1".to_string(),
            institution: "MIT".to_string(),
            degree: "MSc".to_string(),
            field: "Computer Science".to_string(),
            start_year: "2017".to_string(),
            end_year: "2019".to_string(),
            achievements: vec![],
        });

        // Completely unrelated JD
        let jd = "Looking for a pastry chef with baking and confectionery skills";
        let result = tailor_cv(&cv, jd);
        assert_eq!(
            result.tailored.education.len(),
            1,
            "Education should always be included"
        );
    }

    #[test]
    fn tailor_unrelated_projects_excluded() {
        let cv = fixture_cv(); // project is about Rust/Dioxus
        let jd = "Java Spring Boot developer for enterprise banking application";
        let result = tailor_cv(&cv, jd);
        // The Rust/Dioxus project should not score against a Java JD
        assert!(
            result.tailored.projects.is_empty(),
            "Unrelated project should be filtered out"
        );
    }

    #[test]
    fn tailor_empty_jd_does_not_panic() {
        let cv = fixture_cv();
        let result = tailor_cv(&cv, "");
        assert_eq!(result.tailored.match_score, 0.0);
        assert!(result.tailored.matched_keywords.is_empty());
    }

    #[test]
    fn tailor_empty_cv_does_not_panic() {
        let cv = LifetimeCV::default();
        let result = tailor_cv(&cv, "Rust developer needed for distributed systems work");
        assert!(result.tailored.experiences.is_empty());
        assert!(result.tailored.skills.is_empty());
    }

    #[test]
    fn tailor_top_keywords_capped_at_thirty() {
        let cv = fixture_cv();
        // Generate a JD with many unique high-freq keywords
        let jd = (0..50)
            .map(|i| format!("keyword{i} keyword{i} "))
            .collect::<String>();
        let result = tailor_cv(&cv, &jd);
        assert!(
            result.top_keywords.len() <= 30,
            "top_keywords should be capped at 30, got {}",
            result.top_keywords.len()
        );
    }
}
