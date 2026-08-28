use crate::models::cv::{Experience, ExperienceProject, Project, Skill};
use crate::services::embeddings::{cosine_similarity, EmbedItem, EmbeddingEngine};
use crate::services::matcher::Idf;

// ── Scoring mode ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScoreMode {
    #[default]
    Keyword,
    Embedding,
    Hybrid,
}

// ── Scorer ────────────────────────────────────────────────────────────────────

pub struct Scorer {
    pub mode: ScoreMode,
    pub idf: Idf,
    pub engine: Option<EmbeddingEngine>,
    pub hybrid_keyword_weight: f32,
}

impl Scorer {
    pub fn new(mode: ScoreMode) -> Self {
        Scorer {
            mode,
            idf: Idf::build(&[]),
            engine: None,
            hybrid_keyword_weight: 0.6,
        }
    }

    /// Score raw text against JD keywords using the active mode.
    pub fn score_text(
        &mut self,
        text: &str,
        keywords: &[(String, usize)],
        jd_embedding: Option<&[f32]>,
    ) -> f32 {
        match self.mode {
            ScoreMode::Keyword => self.score_text_keyword(text, keywords),
            ScoreMode::Embedding => {
                if let Some(jd_emb) = jd_embedding {
                    if let Some(engine) = self.engine.as_mut() {
                        // Cache key MUST be derived from the text itself, not
                        // a shared placeholder id: `score_text` is called once
                        // per experience/project/skill with no natural stable
                        // id at this granularity. Using an empty (or any
                        // fixed) id here previously meant every call after the
                        // first hit the cache for that same id and silently
                        // got back the FIRST text's embedding regardless of
                        // what text was actually passed — every experience,
                        // project, and skill ended up scored against the JD
                        // using one single shared vector, so nothing could be
                        // discriminated (this was the actual cause of
                        // Embedding/Hybrid mode barely filtering anything).
                        let item = EmbedItem {
                            id: text.to_string(),
                            text: text.to_string(),
                        };
                        match engine.embed_with_cache(&[item]) {
                            Ok(txt_emb) => txt_emb
                                .first()
                                .map_or(0.0, |v| cosine_similarity(v, jd_emb)),
                            Err(_) => 0.0,
                        }
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            }
            ScoreMode::Hybrid => {
                let kw = self.score_text_keyword(text, keywords);
                let emb = if let Some(jd_emb) = jd_embedding {
                    if let Some(engine) = self.engine.as_mut() {
                        // Same fix as the Embedding branch above — see that
                        // comment for why the cache key must be the text
                        // itself, not a shared placeholder.
                        let item = EmbedItem {
                            id: text.to_string(),
                            text: text.to_string(),
                        };
                        match engine.embed_with_cache(&[item]) {
                            Ok(txt_emb) => txt_emb
                                .first()
                                .map_or(0.0, |v| cosine_similarity(v, jd_emb)),
                            Err(_) => 0.0,
                        }
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                self.hybrid_keyword_weight * kw + (1.0 - self.hybrid_keyword_weight) * emb
            }
        }
    }

    pub fn score_text_keyword(&mut self, text: &str, keywords: &[(String, usize)]) -> f32 {
        use crate::services::matcher::{extract_terms, terms_contain};
        use std::collections::HashSet;

        if keywords.is_empty() || text.is_empty() {
            return 0.0;
        }
        let text_terms: HashSet<String> = extract_terms(text).into_iter().collect();

        let total_weight: f32 = keywords
            .iter()
            .map(|(kw, freq)| *freq as f32 * self.idf.get(kw))
            .sum();
        if total_weight <= 0.0 {
            return 0.0;
        }

        let matched_weight: f32 = keywords
            .iter()
            .filter(|(kw, _)| terms_contain(&text_terms, kw))
            .map(|(kw, freq)| *freq as f32 * self.idf.get(kw))
            .sum();

        matched_weight / total_weight
    }

    pub fn score_experience(
        &mut self,
        exp: &Experience,
        keywords: &[(String, usize)],
        jd_embedding: Option<&[f32]>,
        skills: &[Skill],
    ) -> f32 {
        match self.mode {
            ScoreMode::Keyword => self.score_experience_keyword(exp, keywords, skills),
            ScoreMode::Embedding | ScoreMode::Hybrid => {
                // Score each project on its own and take the best match,
                // rather than concatenating every project's role/context/
                // bullets/tools (in BOTH `.en` and `.fr` — literally
                // duplicating the same content) into one flat text.
                //
                // Why this matters specifically for Embedding/Hybrid: the
                // embedding model truncates hard at MAX_SEQ_LEN (128
                // tokens, see embeddings.rs). A multi-project experience
                // easily runs to several hundred words once every
                // project's bullets are concatenated, so whatever's
                // relevant can silently fall past token 128 and never
                // reach the model at all — while a short, single-project
                // experience fits entirely and gets a fair comparison.
                // That's a real, observed failure mode: an experience
                // whose most relevant project (e.g. one literally
                // mentioning the JD's exact tooling) got truncated away
                // scored *lower* than an unrelated but short experience
                // that happened to fit in full. Scoring per-project keeps
                // each individual text well under the token limit, so
                // truncation stops being the deciding factor.
                //
                // Keyword mode is untouched here (still the original
                // concatenated-text scoring): TF-IDF term matching has no
                // token-length limit, so this failure mode doesn't apply
                // to it, and changing its already-validated behavior
                // isn't warranted.
                if exp.projects.is_empty() {
                    let text = format!("{} {} {}", exp.role.en, exp.role.fr, exp.company);
                    return self.score_text(&text, keywords, jd_embedding);
                }
                let shared_tools = crate::services::matcher::pooled_tools(&exp.projects, skills);
                exp.projects
                    .iter()
                    .map(|p| {
                        self.score_experience_project(p, keywords, jd_embedding, &shared_tools)
                    })
                    .fold(0.0_f32, f32::max)
            }
        }
    }

    fn score_experience_keyword(
        &mut self,
        exp: &Experience,
        keywords: &[(String, usize)],
        skills: &[Skill],
    ) -> f32 {
        let mut text = format!("{} {} {}", exp.role.en, exp.role.fr, exp.company);
        for proj in &exp.projects {
            text.push(' ');
            text.push_str(&proj.name.en);
            text.push(' ');
            text.push_str(&proj.name.fr);
            text.push(' ');
            text.push_str(
                &proj
                    .context
                    .iter()
                    .map(|c| c.en.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            text.push(' ');
            text.push_str(
                &proj
                    .context
                    .iter()
                    .map(|c| c.fr.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            text.push(' ');
            text.push_str(
                &proj
                    .bullets
                    .iter()
                    .map(|b| b.en.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            text.push(' ');
            text.push_str(
                &proj
                    .bullets
                    .iter()
                    .map(|b| b.fr.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            text.push(' ');
            text.push_str(
                &proj
                    .skill_ids
                    .iter()
                    .filter_map(|id| skills.iter().find(|s| &s.id == id).map(|s| s.name.as_str()))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        self.score_text_keyword(&text, keywords)
    }

    pub fn score_experience_project(
        &mut self,
        proj: &ExperienceProject,
        keywords: &[(String, usize)],
        jd_embedding: Option<&[f32]>,
        shared_tools: &[String],
    ) -> f32 {
        self.score_text(
            &crate::services::matcher::experience_project_text(proj, shared_tools),
            keywords,
            jd_embedding,
        )
    }

    pub fn score_skill(
        &mut self,
        skill: &Skill,
        keywords: &[(String, usize)],
        jd_embedding: Option<&[f32]>,
    ) -> f32 {
        self.score_text(&skill.name, keywords, jd_embedding)
    }

    pub fn score_project(
        &mut self,
        proj: &Project,
        keywords: &[(String, usize)],
        jd_embedding: Option<&[f32]>,
    ) -> f32 {
        self.score_text(
            &crate::services::matcher::project_text(proj),
            keywords,
            jd_embedding,
        )
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_idf() -> Idf {
        let docs: Vec<Vec<String>> = vec![
            vec!["rust".into(), "wasm".into()],
            vec!["python".into(), "wasm".into()],
            vec!["rust".into(), "linux".into()],
        ];
        Idf::build(&docs)
    }

    fn make_keywords() -> Vec<(String, usize)> {
        vec![("rust".into(), 2), ("wasm".into(), 1), ("linux".into(), 3)]
    }

    // Regression test: `score_experience` in Embedding/Hybrid mode used to
    // concatenate every project's role/context/bullets/tools (in both
    // languages) into one flat text before scoring. For a multi-project
    // experience this routinely exceeds the embedding model's MAX_SEQ_LEN
    // (128 tokens, see embeddings.rs) — whatever's actually relevant can
    // get silently truncated away before the model ever sees it, while an
    // unrelated but short experience scores unaffected. This asserts the
    // fix: the experience's score is the MAX of its per-project scores
    // (each scored independently, so a short irrelevant project can't drag
    // down — or a truncation-prone long one can't hide — the one project
    // that actually matches).
    #[test]
    fn score_experience_hybrid_uses_best_project_not_concatenation() {
        let mut scorer = Scorer::new(ScoreMode::Hybrid);
        scorer.idf = make_idf();
        scorer.hybrid_keyword_weight = 1.0; // isolate the keyword component; no engine attached anyway
        let keywords = make_keywords();

        let strong_project = ExperienceProject {
            name: crate::models::cv::LocalizedText::same("Relevant Project"),
            bullets: vec![crate::models::cv::LocalizedText::same(
                "rust wasm linux project",
            )],
            ..Default::default()
        };
        let weak_project = ExperienceProject {
            name: crate::models::cv::LocalizedText::same("Unrelated Project"),
            bullets: vec![crate::models::cv::LocalizedText::same(
                "totally unrelated cobol mainframe batch job",
            )],
            ..Default::default()
        };

        let exp = Experience {
            id: "exp-test".to_string(),
            company: "TestCo".to_string(),
            role: crate::models::cv::LocalizedText::same("Engineer"),
            projects: vec![weak_project.clone(), strong_project.clone()],
            ..Default::default()
        };

        let exp_score = scorer.score_experience(&exp, &keywords, None, &[]);
        let strong_score = scorer.score_experience_project(&strong_project, &keywords, None, &[]);
        let weak_score = scorer.score_experience_project(&weak_project, &keywords, None, &[]);

        assert!(
            strong_score > weak_score,
            "fixture sanity check: strong project should score higher than weak one"
        );
        assert!(
            (exp_score - strong_score).abs() < 1e-6,
            "experience score ({exp_score}) should equal its best project's score \
             ({strong_score}), not a blended/concatenated value"
        );
    }

    #[test]
    fn keyword_score_basic() {
        let mut scorer = Scorer::new(ScoreMode::Keyword);
        scorer.idf = make_idf();
        let keywords = make_keywords();

        let score = scorer.score_text("rust wasm project", &keywords, None);
        assert!(score > 0.3, "expected positive score, got {score}");

        let score_empty = scorer.score_text("", &keywords, None);
        assert_eq!(score_empty, 0.0);
    }

    #[test]
    fn keyword_score_empty_keywords() {
        let mut scorer = Scorer::new(ScoreMode::Keyword);
        let score = scorer.score_text("anything", &[], None);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn embedding_score_without_engine_returns_zero() {
        let mut scorer = Scorer::new(ScoreMode::Embedding);
        let keywords = make_keywords();
        let score = scorer.score_text("rust", &keywords, None);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn hybrid_score_weighted_average() {
        let mut scorer = Scorer::new(ScoreMode::Hybrid);
        scorer.idf = make_idf();
        scorer.hybrid_keyword_weight = 0.5;
        let keywords = make_keywords();

        let score = scorer.score_text("rust wasm project", &keywords, None);
        // No embedding engine, so embedding part = 0 → score = 0.5 * kw_score
        let kw_score = scorer.score_text_keyword("rust wasm project", &keywords);
        let expected = 0.5 * kw_score;
        assert!(
            (score - expected).abs() < 0.01,
            "expected ~{expected}, got {score}"
        );
    }

    // Regression test for a real bug: `score_text`'s Embedding/Hybrid
    // branches used to build every `EmbedItem` with `id: String::new()`.
    // Since `embed_with_cache` only re-embeds ids it hasn't seen, every
    // call after the first one for a given `Scorer`/engine — for any
    // *different* text — hit the cache under that same shared empty id
    // and silently got back the FIRST text's embedding instead of its
    // own. Concretely: every experience/project/skill ended up scored
    // against the JD using one single shared vector, so nothing could
    // ever be discriminated in Embedding/Hybrid mode, regardless of any
    // selection threshold.
    //
    // This asserts the cache behavior directly rather than comparing the
    // resulting scores: `tiny_test_engine` is built with all-zero weights
    // (see its doc comment — it's meant to sanity-check tensor plumbing,
    // not produce meaningful embeddings), so two different texts can
    // legitimately embed to the same all-zero vector through it. That
    // would make a "scores must differ" assertion fail for an unrelated
    // reason. What must always hold, independent of what the model
    // outputs, is that two distinct texts get two distinct cache entries
    // — with the old bug they'd collapse onto one.
    #[test]
    fn embedding_score_gives_each_distinct_text_its_own_cache_entry() {
        use crate::services::embeddings::tiny_test_engine;

        let mut scorer = Scorer::new(ScoreMode::Embedding);
        scorer.engine = Some(tiny_test_engine());
        let keywords = make_keywords();
        let jd_embedding = vec![0.1_f32; 8];

        scorer.score_text("hello world", &keywords, Some(&jd_embedding));
        scorer.score_text(
            "totally different text here",
            &keywords,
            Some(&jd_embedding),
        );

        assert_eq!(
            scorer.engine.as_ref().unwrap().cache_len(),
            2,
            "each distinct text should get its own cache entry, not share one \
             (the old bug used a shared empty id for every call, so the \
             second text's embedding call would hit the cache instead of \
             actually embedding its own text)"
        );
    }
}
