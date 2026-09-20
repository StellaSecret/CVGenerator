use crate::models::{
    Experience, ExperienceProject, LifetimeCV, Project, Skill, SkillLevel, TailoredCV,
};
use std::collections::{HashMap, HashSet};

// ── Relative-cutoff selection ─────────────────────────────────────────────────
//
// Shared by the experience-level and project-level filtering below. Given a
// list of scores (any order), returns the indices that pass a
// relative-to-the-best-match cutoff: `max_score * fixed_fraction`, optionally
// raised to at least the sample mean.
//
// Why the mean floor is conditional rather than always-on: keyword/TF-IDF
// scores spread widely (0.05-1.0 relative to the best match), so a fixed
// fraction alone already discriminates well — and in cases with several
// genuinely-all-relevant, similarly-scored items, a mean floor can
// over-trim (it doesn't know a tight cluster there reflects real
// near-ties rather than a non-discriminating scorer).
//
// Embedding/Hybrid cosine-similarity scores cluster far more tightly
// regardless of true relevance (a known property of sentence-transformer
// similarity), so the fixed fraction alone barely filters anything —
// hence the mean floor is needed there to keep discriminating. Callers
// pass `use_mean_floor = scorer.mode != ScoreMode::Keyword` so Keyword
// mode's already-validated behavior is untouched.
//
// Near-tie margin: even with the mean floor, real-world scores in
// Embedding/Hybrid mode routinely land within a few hundredths of the
// cutoff of each other (observed directly: a genuinely relevant
// experience missed the cutoff by 0.0094 while an unrelated one cleared
// it, out of a ~0.18-wide overall score range). A gap that small is
// noise relative to the embedding model's actual resolving power, not a
// real semantic distinction — but a hard `>=` cutoff treats "missed by
// 0.01" identically to "missed by 0.15". For a CV tool the two failure
// modes aren't symmetric: silently dropping the single most relevant
// item over a coin-flip-sized gap is worse than including one extra,
// slightly-less-relevant item. So when `use_mean_floor` is on, anything
// within `NEAR_TIE_MARGIN_FRACTION * max_score` below the cutoff is kept
// too. This does NOT apply to Keyword mode, whose wider natural spread
// means a miss by that margin usually IS a real distinction.
const NEAR_TIE_MARGIN_FRACTION: f32 = 0.03;

fn select_by_relative_cutoff(
    scores: &[f32],
    fixed_fraction: f32,
    use_mean_floor: bool,
) -> Vec<usize> {
    if scores.is_empty() {
        return Vec::new();
    }
    let max_score = scores.iter().cloned().fold(0.0_f32, f32::max);
    let fixed_cutoff = max_score * fixed_fraction;
    let cutoff = if use_mean_floor {
        let mean_score = scores.iter().sum::<f32>() / scores.len() as f32;
        fixed_cutoff.max(mean_score)
    } else {
        fixed_cutoff
    };
    let effective_cutoff = if use_mean_floor {
        cutoff - (max_score * NEAR_TIE_MARGIN_FRACTION)
    } else {
        cutoff
    };
    scores
        .iter()
        .enumerate()
        .filter(|(_, s)| **s > 0.0 && **s >= effective_cutoff)
        .map(|(i, _)| i)
        .collect()
}

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
    // ── French stop words ───────────────────────────────────────────────────
    "le",
    "la",
    "les",
    "un",
    "une",
    "des",
    "du",
    "de",
    "et",
    "ou",
    "mais",
    "dans",
    "sur",
    "sous",
    "avec",
    "sans",
    "par",
    "pour",
    "vers",
    "chez",
    "entre",
    "au",
    "aux",
    "ce",
    "ces",
    "cet",
    "cette",
    "son",
    "sa",
    "ses",
    "leur",
    "leurs",
    "nos",
    "notre",
    "votre",
    "vos",
    "que",
    "qui",
    "quoi",
    "dont",
    "est",
    "sont",
    "sera",
    "seront",
    "être",
    "avoir",
    "ont",
    "fait",
    "faire",
    "afin",
    "ainsi",
    "aussi",
    "alors",
    "donc",
    "comme",
    "tout",
    "tous",
    "toute",
    "toutes",
    "plus",
    "moins",
    "même",
    "ensemble",
    "cadre",
    "projet",
    "programme",
    "équipe",
    "équipes",
    "mission",
    "poste",
    "candidat",
    "candidate",
    "recherche",
    "rejoindre",
    "travailler",
    "quelqu",
    "déjà",
];

// ── Synonym / canonicalisation dictionary ─────────────────────────────────────
//
// Domain-specific (DevOps/SRE/infra) FR+EN term variants mapped to one
// canonical token, so e.g. "hardening", "durcissement" and "sécurisation" are
// treated as the same keyword instead of three separate weak signals. This is
// hand-curated and rule-based — no learned weights, easy to extend by adding
// a line. Keys/values are matched against the accent-stripped, lowercased,
// *stemmed* form (see `normalize`), so only add the stemmed form here (e.g.
// "deploi" not "déploiement" — check `stem()` if unsure what a word reduces
// to).
fn synonym_map() -> HashMap<&'static str, &'static str> {
    // NOTE: keys are the *actual output* of `stem()` on the accent-stripped
    // lowercase word, not the word itself — the stemmer here is a simple
    // single-pass suffix stripper, not a real linguistic stemmer, so it
    // doesn't always reduce related words to an intuitively "obvious" shared
    // root (e.g. "sécurisation" → "secur" but "hardening" → "harden"; these
    // don't collide on their own, hence needing an explicit synonym entry).
    // If you add a new variant, run it through `stem()` first to find the
    // real key rather than guessing.
    let pairs: &[(&str, &str)] = &[
        // hardening / sécurisation / durcissement
        ("harden", "hardening"),
        ("durc", "hardening"),
        ("secur", "hardening"),
        // deployment / déploiement
        ("deploi", "deploy"),
        ("deploy", "deploy"),
        ("deployer", "deploy"),
        // versioning / versionning / versionnage
        ("versionn", "versioning"),
        ("version", "versioning"),
        ("versionnage", "versioning"),
        // rollback
        ("rollback", "rollback"),
        // playbook
        ("playbook", "playbook"),
        // audit
        ("audit", "audit"),
        // compliance / conformité
        ("conformite", "compliance"),
        ("conformit", "compliance"),
        ("compliance", "compliance"),
        // fleet / parc / infrastructure
        ("parc", "fleet"),
        ("infrastructure", "fleet"),
        ("infra", "fleet"),
        // dashboard / tableau de bord
        ("dashboard", "dashboard"),
        // tracking / suivi
        ("suivi", "tracking"),
        ("track", "tracking"),
        // batch / lot
        ("lot", "batch"),
        ("lots", "batch"),
        ("batch", "batch"),
        // automation / automatisation
        ("automat", "automation"),
        ("automa", "automation"),
        // implementation / implémentation / implémenter
        ("implementa", "implement"),
        ("implementer", "implement"),
        // inventory / inventaire
        ("inventaire", "inventory"),
        ("inventair", "inventory"),
        ("inventory", "inventory"),
        // cmdb
        ("cmdb", "cmdb"),
        // server / serveur
        ("serv", "server"),
        ("server", "server"),
    ];
    pairs.iter().cloned().collect()
}

// ── Accent stripping ──────────────────────────────────────────────────────────

fn strip_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

// ── Lightweight suffix-stripping stemmer (EN + FR) ────────────────────────────
//
// Not a full Porter/Snowball stemmer — a small, deterministic suffix
// stripper tuned for CV/JD vocabulary, so e.g. "déploiements",
// "déploiement" and "deploying" collapse to the same root instead of
// counting as three unrelated keywords. Longest suffixes are tried first.
fn stem(word: &str) -> String {
    const SUFFIXES: &[&str] = &[
        "issements",
        "isations",
        "issement",
        "isation",
        "ations",
        "ement",
        "ements",
        "ateur",
        "atrice",
        "ateurs",
        "atrices",
        "iser",
        "isee",
        "isees",
        "ise",
        "ises",
        "tion",
        "tions",
        "ing",
        "eurs",
        "euse",
        "euses",
        "eur",
        "ment",
        "ments",
        "able",
        "ables",
        "ible",
        "ibles",
        "ant",
        "ants",
        "ent",
        "ents",
        "ed",
        "es",
        "s",
    ];
    if word.len() <= 4 {
        return word.to_string();
    }
    for suf in SUFFIXES {
        if word.len() > suf.len() + 3 && word.ends_with(suf) {
            return word[..word.len() - suf.len()].to_string();
        }
    }
    word.to_string()
}

/// Normalize a raw token: lowercase, strip accents, stem, then canonicalise
/// via the synonym dictionary if a mapping exists (checked on both the
/// stemmed and un-stemmed form, since some dictionary keys are prefixes).
fn normalize(word: &str) -> String {
    let base = strip_accents(&word.to_lowercase());
    let stemmed = stem(&base);
    let syns = synonym_map();
    if let Some(canon) = syns.get(stemmed.as_str()) {
        return canon.to_string();
    }
    if let Some(canon) = syns.get(base.as_str()) {
        return canon.to_string();
    }
    stemmed
}

// ── Tokeniser ─────────────────────────────────────────────────────────────────

fn raw_tokenise(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '+' && c != '#')
        .map(|w| w.to_string())
        .filter(|w| w.len() >= 3)
        .collect()
}

/// Tokenise, drop stop words, then normalize (accent-strip + stem + synonym
/// canonicalisation) each remaining token.
fn tokenise(text: &str) -> Vec<String> {
    raw_tokenise(text)
        .into_iter()
        .filter(|w| !STOP_WORDS.contains(&strip_accents(&w.to_lowercase()).as_str()))
        .map(|w| normalize(&w))
        .filter(|w| !w.is_empty())
        .collect()
}

/// Build unigrams + bigrams + trigrams from normalized tokens, so phrases
/// like "gestion de version" / "chef de projet" are matched as one unit
/// instead of three independent, weaker single-word matches. Multi-word
/// terms are naturally rarer than single words, so they end up with a
/// higher IDF weight later without needing an artificial bonus multiplier.
pub fn extract_terms(text: &str) -> Vec<String> {
    let tokens = tokenise(text);
    let mut terms = tokens.clone();
    for w in tokens.windows(2) {
        terms.push(format!("{} {}", w[0], w[1]));
    }
    for w in tokens.windows(3) {
        terms.push(format!("{} {} {}", w[0], w[1], w[2]));
    }
    terms
}

// ── Keyword extraction ────────────────────────────────────────────────────────

/// Extract keyword terms (unigrams/bigrams/trigrams) from a JD text, sorted
/// by frequency (desc). Each entry is (term, frequency).
pub fn extract_keywords(text: &str) -> Vec<(String, usize)> {
    let mut freq: HashMap<String, usize> = HashMap::new();
    for term in extract_terms(text) {
        *freq.entry(term).or_insert(0) += 1;
    }
    let mut sorted: Vec<(String, usize)> = freq.into_iter().collect();
    // Sort by frequency desc, then alphabetically for determinism
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted
}

// ── Levenshtein distance (small, iterative, no deps) ──────────────────────────

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (la, lb) = (a.len(), b.len());
    if la == 0 {
        return lb;
    }
    if lb == 0 {
        return la;
    }
    let mut prev: Vec<usize> = (0..=lb).collect();
    let mut curr = vec![0usize; lb + 1];
    for i in 1..=la {
        curr[0] = i;
        for j in 1..=lb {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[lb]
}

/// Fuzzy single-word match: exact, or close enough by edit distance relative
/// to word length (longer words tolerate a bigger absolute distance). Only
/// applied to single-word terms — multi-word phrases must match exactly,
/// since fuzzy-matching whole phrases gets unreliable fast (and stop-word
/// stripping/stemming already normalizes most phrase variation away).
fn fuzzy_eq(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if a.contains(' ') || b.contains(' ') {
        return false;
    }
    let max_len = a.len().max(b.len());
    let tolerance = if max_len >= 8 {
        2
    } else if max_len >= 5 {
        1
    } else {
        0
    };
    tolerance > 0 && levenshtein(a, b) <= tolerance
}

/// Does `haystack_terms` contain something matching `needle`, exactly or
/// fuzzily (typos / near-identical spellings)?
pub fn terms_contain(haystack_terms: &HashSet<String>, needle: &str) -> bool {
    if haystack_terms.contains(needle) {
        return true;
    }
    haystack_terms.iter().any(|t| fuzzy_eq(t, needle))
}

// ── TF-IDF ────────────────────────────────────────────────────────────────────
//
// `Idf` is built once per `tailor_cv` call from every independently-scorable
// text block in the candidate's own CV (each experience-project, each
// top-level project, each skill). Weighting keywords by inverse document
// frequency across that corpus means a JD term that shows up in *every*
// block of the CV (generic filler like "team", "deploy") is down-weighted
// relative to a term that's distinctive to one or two blocks (e.g.
// "hardening", "cmdb") — something raw frequency counting can't do, since it
// treats every matched keyword as equally significant regardless of how
// common it is across the candidate's whole CV.
pub struct Idf {
    weights: HashMap<String, f32>,
}

impl Idf {
    pub fn build(documents: &[Vec<String>]) -> Self {
        let n = documents.len().max(1) as f32;
        let mut df: HashMap<String, usize> = HashMap::new();
        for doc in documents {
            let unique: HashSet<&String> = doc.iter().collect();
            for term in unique {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
        }
        let weights = df
            .into_iter()
            .map(|(term, d)| {
                // Smoothed IDF, always >= 1.0 so unseen terms still count.
                let idf = ((n + 1.0) / (d as f32 + 1.0)).ln() + 1.0;
                (term, idf)
            })
            .collect();
        Idf { weights }
    }

    pub fn get(&self, term: &str) -> f32 {
        self.weights.get(term).copied().unwrap_or(1.0)
    }
}

// ── Scoring ───────────────────────────────────────────────────────────────────

/// Returns a relevance score for `text` against `keywords`, TF-IDF weighted:
/// score = sum(jd_frequency * idf) over matched keywords / sum(jd_frequency * idf) over all keywords.
/// Matching is exact-or-fuzzy per term (see `terms_contain`).
fn score_text(text: &str, keywords: &[(String, usize)], idf: &Idf) -> f32 {
    if keywords.is_empty() || text.is_empty() {
        return 0.0;
    }
    let text_terms: HashSet<String> = extract_terms(text).into_iter().collect();

    let total_weight: f32 = keywords
        .iter()
        .map(|(kw, freq)| *freq as f32 * idf.get(kw))
        .sum();
    if total_weight <= 0.0 {
        return 0.0;
    }

    let matched_weight: f32 = keywords
        .iter()
        .filter(|(kw, _)| terms_contain(&text_terms, kw))
        .map(|(kw, freq)| *freq as f32 * idf.get(kw))
        .sum();

    matched_weight / total_weight
}

fn score_experience(
    exp: &Experience,
    keywords: &[(String, usize)],
    idf: &Idf,
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
    score_text(&text, keywords, idf)
}

/// Union (deduplicated, order-preserving) of tool/skill *names*, resolved
/// via `skills` from the `skill_ids` recorded on every project within a
/// single `Experience`.
///
/// Why this exists: `skill_ids` is already a per-`ExperienceProject` field
/// (not per-`Experience`) in the data model, and the editor only lets you
/// pick tools that already exist in `cv.skills` — strict, no free text. But
/// plenty of real CVs are *authored* (or PDF-imported) with one combined
/// tech-stack line per role covering all of that role's sub-projects
/// together, rather than one per sub-project, and a person re-tagging an
/// imported CV project-by-project may not get to every project right away.
/// The practical effect, until every project is individually tagged:
/// whichever project happens to hold most of a role's tags scores well,
/// while siblings — including, concretely, ones whose own bullets are what
/// actually name AWX/Ansible work — end up under-tagged and lose out on
/// keyword matches for tools they genuinely used.
///
/// Rather than only ever scoring a project against its own tags, every
/// project is also scored against the pool of tags used anywhere in its
/// parent experience. This can't over-credit a project with a tool used by
/// a *different* experience — only ones already known to belong to the
/// same role.
pub fn pooled_tools(projects: &[ExperienceProject], skills: &[Skill]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut pooled = Vec::new();
    for proj in projects {
        for skill_id in &proj.skill_ids {
            if let Some(skill) = skills.iter().find(|s| &s.id == skill_id) {
                if seen.insert(skill.name.clone()) {
                    pooled.push(skill.name.clone());
                }
            }
        }
    }
    pooled
}

/// Builds the scorable text blob for a single sub-project. Shared between
/// scoring (`score_experience_project`) and IDF corpus construction in
/// `tailor_cv`, so the two always see identical text. `shared_tools` should
/// be the parent experience's `pooled_tools()` output (already resolved to
/// skill names, and already inclusive of this project's own tags, since
/// pooling iterates every project in the experience including this one) —
/// see that function's doc comment for why pooling is needed at all.
pub fn experience_project_text(proj: &ExperienceProject, shared_tools: &[String]) -> String {
    let mut text = format!("{} {}", proj.name.en, proj.name.fr);
    for c in &proj.context {
        text.push(' ');
        text.push_str(&c.en);
        text.push(' ');
        text.push_str(&c.fr);
    }
    for b in &proj.bullets {
        text.push(' ');
        text.push_str(&b.en);
        text.push(' ');
        text.push_str(&b.fr);
    }
    text.push(' ');
    text.push_str(&shared_tools.join(" "));
    text
}

fn score_experience_project(
    proj: &ExperienceProject,
    keywords: &[(String, usize)],
    idf: &Idf,
    shared_tools: &[String],
) -> f32 {
    score_text(&experience_project_text(proj, shared_tools), keywords, idf)
}

fn score_skill(skill: &Skill, keywords: &[(String, usize)], idf: &Idf) -> f32 {
    score_text(&skill.name, keywords, idf)
}

/// Skills retained for a tailored CV: every skill related to the JD
/// (`is_related` returns true) plus every self-assessed Expert/Mastery
/// skill. Unrelated Beginner/Intermediate/Advanced skills — a CV almost
/// always lists far more tools than a given offer actually needs — are
/// dropped. Related skills come first, then the expert-tier ones (the
/// pre-existing split between matched and score-0 skills), each group in
/// original CV order.
fn select_tailored_skills(skills: &[Skill], is_related: impl Fn(&Skill) -> bool) -> Vec<Skill> {
    let mut out: Vec<Skill> = skills.iter().filter(|s| is_related(s)).cloned().collect();
    out.extend(
        skills
            .iter()
            .filter(|s| {
                !is_related(s) && matches!(s.level, SkillLevel::Expert | SkillLevel::Mastery)
            })
            .cloned(),
    );
    out
}

/// Builds the scorable text blob for a top-level project. Shared between
/// scoring (`score_project`) and IDF corpus construction in `tailor_cv`.
pub fn project_text(proj: &Project) -> String {
    format!(
        "{} {} {} {} {} {}",
        proj.name,
        proj.description.en,
        proj.description.fr,
        proj.tools.join(" "),
        proj.bullets
            .iter()
            .map(|b| b.en.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        proj.bullets
            .iter()
            .map(|b| b.fr.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn score_project(proj: &Project, keywords: &[(String, usize)], idf: &Idf) -> f32 {
    score_text(&project_text(proj), keywords, idf)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Raw per-experience (and per-project) relevance scores, alongside the
/// final include/exclude decision — exposed purely for inspection/debugging.
///
/// Added specifically to answer "is the embedding scoring itself noise, or
/// is a downstream selection/aggregation step still wrong?" without that
/// distinction, every fix to selection logic looks identical from the
/// outside (the tailored CV just changes which experiences appear) and
/// there's no way to tell whether an experience was excluded because it
/// genuinely scored low or because of some other bug — you can only ever
/// see the final in/out list, never the number that produced it.
#[derive(Debug, Clone)]
pub struct ExperienceScoreDebug {
    pub experience_id: String,
    pub company: String,
    /// Whichever of `.fr`/`.en` is non-empty (falls back to `.en` if both
    /// are set) — this is for a human skimming a debug panel, not for
    /// rendering, so it doesn't need to respect the CV's active language.
    pub role: String,
    pub score: f32,
    pub selected: bool,
    pub projects: Vec<ProjectScoreDebug>,
}

#[derive(Debug, Clone)]
pub struct ProjectScoreDebug {
    pub id: String,
    pub name: String,
    pub score: f32,
    pub selected: bool,
}

/// Rebuilds the final experience list from the person's own manual
/// tick/untick choices, overriding whatever the automatic scoring
/// selected. `checked_project_ids` is the full set of `ExperienceProject`
/// ids the person wants kept, across every experience — not just the ones
/// the algorithm originally picked.
///
/// An experience's presence in the result is *derived*, not itself a
/// separate checkbox: it's kept if and only if at least one of its
/// projects is in `checked_project_ids`, and shows only those checked
/// projects (an experience with zero checked projects simply doesn't
/// appear at all, same as the automatic path already behaves for a
/// zero-relevance experience).
///
/// Iterates `cv.experiences` in its own stored order — deliberately NOT
/// score order, matching exactly how the automatic path itself builds its
/// final experience list (see the "Rebuild the selection in the CV's
/// original (reverse-chronological) order" comment above): a CV reads as
/// a timeline, not a relevance ranking, and a person's stored experience
/// order already IS that timeline. An earlier version of this function
/// took an explicit `experience_order` parameter seeded from
/// `debug_scores` (which IS score-sorted) — that silently reordered the
/// whole CV to relevance order on every manual apply, e.g. shoving the
/// most recent role to the middle of the document. Reading order
/// directly from `cv.experiences` instead of from any derived/sorted
/// list can't drift out of sync with it, by construction.
///
/// Reads from `cv` (the full, untailored CV) rather than an
/// already-filtered `TailoredCV`, since a project the algorithm excluded
/// — and that the person now wants to manually re-include — isn't present
/// in the filtered version at all.
pub fn apply_manual_project_selection(
    cv: &LifetimeCV,
    checked_project_ids: &HashSet<String>,
) -> Vec<Experience> {
    let mut result = Vec::new();
    for exp in &cv.experiences {
        let kept_projects: Vec<ExperienceProject> = exp
            .projects
            .iter()
            .filter(|p| checked_project_ids.contains(&p.id))
            .cloned()
            .collect();
        if kept_projects.is_empty() {
            continue;
        }
        let mut kept_exp = exp.clone();
        kept_exp.projects = kept_projects;
        result.push(kept_exp);
    }
    result
}

/// `Skill` ids the person manually checked, applied to the full lifetime
/// skill list. Skills keep the CV's own order (the rendered skills section
/// regroups them by category anyway); an unchecked skill is simply
/// dropped, an empty set drops them all.
pub fn apply_manual_skill_selection(
    cv: &LifetimeCV,
    checked_skill_ids: &HashSet<String>,
) -> Vec<Skill> {
    cv.skills
        .iter()
        .filter(|s| checked_skill_ids.contains(&s.id))
        .cloned()
        .collect()
}

fn display_role(role: &crate::models::LocalizedText) -> String {
    if !role.fr.is_empty() {
        role.fr.clone()
    } else {
        role.en.clone()
    }
}

fn display_name(name: &crate::models::LocalizedText) -> String {
    if !name.fr.is_empty() {
        name.fr.clone()
    } else {
        name.en.clone()
    }
}

pub struct TailorResult {
    pub tailored: TailoredCV,
    /// Top keywords from the JD for display
    pub top_keywords: Vec<(String, usize)>,
    /// Raw scores behind the experience/project selection above — see
    /// `ExperienceScoreDebug`'s doc comment for why this exists.
    pub debug_scores: Vec<ExperienceScoreDebug>,
}

/// Main entry point: given a LifetimeCV and raw JD text, produce a tailored CV.
///
/// Rules:
///   - Experiences are filtered to those with score > 0, then sorted best-first.
///     Always include at least the 2 most recent even if score = 0.
///   - Skills: only JD-related skills (score > 0) plus self-assessed
///     Expert/Mastery skills survive; everything else is dropped. Related
///     ones come first, then the expert-tier rest.
///   - Projects: only those with score > 0.
///   - Education, languages, certifications: always included, unchanged.
///   - Matched / missing keywords are derived from the top-30 JD keywords.
pub fn tailor_cv(cv: &LifetimeCV, jd_text: &str) -> TailorResult {
    let keywords = extract_keywords(jd_text);
    // Work with the top 40 most-frequent keywords only
    let top_keywords: Vec<(String, usize)> = keywords.iter().take(40).cloned().collect();

    // Build the TF-IDF corpus from every independently-scorable block in the
    // candidate's own CV, so keyword weighting can tell a term that's
    // distinctive to one or two blocks apart from one that shows up
    // everywhere (see `Idf` doc comment above).
    let mut documents: Vec<Vec<String>> = Vec::new();
    for exp in &cv.experiences {
        let shared_tools = pooled_tools(&exp.projects, &cv.skills);
        for proj in &exp.projects {
            documents.push(extract_terms(&experience_project_text(proj, &shared_tools)));
        }
    }
    for proj in &cv.projects {
        documents.push(extract_terms(&project_text(proj)));
    }
    for skill in &cv.skills {
        documents.push(extract_terms(&skill.name));
    }
    let idf = Idf::build(&documents);

    // ── Experiences ──────────────────────────────────────────────────────────
    let mut scored_exp: Vec<(f32, Experience)> = cv
        .experiences
        .iter()
        .map(|e| {
            (
                score_experience(e, &top_keywords, &idf, &cv.skills),
                e.clone(),
            )
        })
        .collect();
    scored_exp.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Select experiences relative to the best match rather than any nonzero
    // score: with broad JDs, near-every experience block matches at least one
    // low-signal keyword, so an absolute `> 0.0` cutoff barely filters anything.
    //
    // The cutoff is max(fixed_fraction_of_max, mean_score) rather than just
    // a fixed fraction of the top score. Why: a fixed fraction assumes the
    // score distribution has roughly the same *shape* every time, but it
    // doesn't — TF-IDF/keyword scores tend to spread widely (0.05-1.0
    // relative to the best match), while embedding-based cosine
    // similarities (Embedding/Hybrid modes) tend to cluster much more
    // tightly together (a known property of sentence-transformer
    // similarity scores). A fraction tuned against the wide keyword
    // distribution barely filters anything once scores are all clustered
    // near, say, 0.6-0.75 — which is exactly the bug this fixes (Embedding
    // mode was including 6 of 7 experiences instead of the ~5 that are
    // actually relevant). Using the *mean* as a floor self-adapts to
    // whatever spread the scores actually have: it sits in the middle of
    // the distribution regardless of how wide or narrow that distribution
    // is, so it keeps discriminating even when the fixed fraction doesn't.
    // Taking the max of the two is also safe for the already-validated
    // keyword-mode behavior — it can only make the cutoff stricter than
    // the old fixed-fraction alone, never looser, and empirically
    // reproduces the exact same keyword-mode selection as before.
    const REL_THRESHOLD: f32 = 0.5;
    let max_score = scored_exp.first().map(|(s, _)| *s).unwrap_or(0.0);
    let mean_score = if scored_exp.is_empty() {
        0.0
    } else {
        scored_exp.iter().map(|(s, _)| *s).sum::<f32>() / scored_exp.len() as f32
    };
    let cutoff = (max_score * REL_THRESHOLD).max(mean_score);

    // Which experience ids passed the relevance cutoff.
    let mut selected_ids: Vec<String> = scored_exp
        .iter()
        .filter(|(s, _)| *s > 0.0 && *s >= cutoff)
        .map(|(_, e)| e.id.clone())
        .collect();

    if selected_ids.len() < 2 {
        for exp in cv.experiences.iter().take(2) {
            if !selected_ids.contains(&exp.id) {
                selected_ids.push(exp.id.clone());
            }
        }
    }

    // Rebuild the selection in the CV's original (reverse-chronological) order
    // rather than relevance-score order — readers expect a CV timeline, not a
    // ranking, and the score is only meant to decide inclusion, not ordering.
    let mut selected_exp: Vec<Experience> = cv
        .experiences
        .iter()
        .filter(|e| selected_ids.contains(&e.id))
        .cloned()
        .collect();

    // Filter projects within each selected experience, using the same
    // relative-to-best-match logic as experiences: an absolute `> 0.0`
    // cutoff barely trims anything once a project matches any keyword at all.
    //
    // NOTE: unlike the experience-level cutoff above, this still uses only
    // the fixed fraction, NOT the mean-based hybrid. I tried applying the
    // same hybrid here and verified it breaks a previously-validated case
    // (a real experience with 4 similarly-scored, genuinely-all-relevant
    // projects got wrongly trimmed to 2, because tight clustering at the
    // project level can mean "these are all relevant" rather than "this
    // scoring mode isn't discriminating" — unlike at the experience level,
    // where tight clustering reliably signals the latter). So this is a
    // known, currently-unresolved gap: project-level over-inclusion under
    // Embedding/Hybrid mode is not fixed by this change, only the
    // experience-level over-inclusion is.
    for exp in &mut selected_exp {
        if exp.projects.len() <= 1 {
            continue;
        }
        // Score each project but keep track of its original index so the
        // final selection can be re-ordered back into the CV's own order
        // (chronological / as-entered), matching the experience-level fix.
        //
        // Project scores cluster much more tightly than experience scores,
        // since every project within an already-relevant experience tends to
        // share its vocabulary. Tuned against real data (post TF-IDF/synonym
        // changes): 0.7 correctly reproduces manual project selection for
        // SIRIUS, KAIMAN and BRED IT. It can't perfectly separate near-tied
        // scores (e.g. two DTNUM sub-projects 0.008 apart) — no threshold
        // can, since the algorithm has no way to know which of two
        // similarly-worded projects a human would consider more relevant.
        const PROJECT_REL_THRESHOLD: f32 = 0.7;
        let shared_tools = pooled_tools(&exp.projects, &cv.skills);
        let proj_scores: Vec<f32> = exp
            .projects
            .iter()
            .map(|p| score_experience_project(p, &top_keywords, &idf, &shared_tools))
            .collect();
        let max_proj_score = proj_scores.iter().cloned().fold(0.0_f32, f32::max);
        let proj_cutoff = max_proj_score * PROJECT_REL_THRESHOLD;

        let mut keep_idx: Vec<usize> = proj_scores
            .iter()
            .enumerate()
            .filter(|(_, s)| **s > 0.0 && **s >= proj_cutoff)
            .map(|(i, _)| i)
            .collect();

        if keep_idx.is_empty() {
            // Keep the single best-scoring project if none clears the cutoff
            if let Some((best_i, _)) = proj_scores
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                keep_idx.push(best_i);
            }
        }

        let mut i = 0;
        exp.projects.retain(|_| {
            let keep = keep_idx.contains(&i);
            i += 1;
            keep
        });
    }

    // ── Skills ────────────────────────────────────────────────────────────────
    // Keep only skills related to the JD (score > 0) plus Expert/Mastery
    // ones — a CV lists far more tools than the offer actually needs, so
    // unrelated Beginner/Intermediate/Advanced skills are dropped (see
    // `select_tailored_skills`).
    let matched_skills =
        select_tailored_skills(&cv.skills, |s| score_skill(s, &top_keywords, &idf) > 0.0);

    // ── Projects ──────────────────────────────────────────────────────────────
    let mut scored_proj: Vec<(f32, Project)> = cv
        .projects
        .iter()
        .map(|p| (score_project(p, &top_keywords, &idf), p.clone()))
        .collect();
    scored_proj.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let selected_proj: Vec<Project> = scored_proj
        .into_iter()
        .filter(|(s, _)| *s > 0.0)
        .map(|(_, p)| p)
        .collect();

    // ── Match / gap analysis ──────────────────────────────────────────────────
    // Uses the same normalized-term + fuzzy matching as scoring (rather than
    // plain substring containment on raw lowercased text), so a keyword like
    // "hardening" correctly shows as matched even when the CV only contains
    // "durcissement" / "sécurisation", and a multi-word JD term like "chef
    // hardening" is checked as a phrase, not three independent substrings.
    let cv_terms: HashSet<String> = extract_terms(&cv.all_text()).into_iter().collect();
    let (matched_keywords, missing_keywords): (Vec<String>, Vec<String>) = top_keywords
        .iter()
        .take(30)
        .map(|(kw, _)| kw.clone())
        .partition(|kw| terms_contain(&cv_terms, kw));

    let match_score = if top_keywords.is_empty() {
        0.0
    } else {
        matched_keywords.len() as f32 / top_keywords.len().min(30) as f32
    };

    let tailored = TailoredCV {
        personal: cv.personal.clone(),
        experiences: selected_exp,
        all_experiences: cv.experiences.clone(),
        skills: matched_skills,
        education: cv.education.clone(),
        projects: selected_proj,
        languages: cv.languages.clone(),
        certifications: cv.certifications.clone(),
        matched_keywords,
        missing_keywords,
        match_score,
    };

    // Raw scores for every experience/project, independent of the
    // selection above — see `ExperienceScoreDebug`'s doc comment.
    //
    // `selected` here is derived from `tailored.experiences` itself
    // (which project ids actually survived), NOT recomputed from
    // `pscore` — a project can score above zero (almost everything does,
    // especially in Keyword mode where shared common words alone give
    // some nonzero overlap) without clearing the relative cutoff that
    // actually determines inclusion. Using `pscore > 0.0` here previously
    // made nearly everything show as "selected" regardless of what the
    // real tailored result contained.
    let kept_project_ids: HashSet<String> = tailored
        .experiences
        .iter()
        .flat_map(|e| e.projects.iter())
        .map(|p| p.id.clone())
        .collect();
    let debug_scores: Vec<ExperienceScoreDebug> = scored_exp
        .iter()
        .map(|(score, exp)| {
            let shared_tools = pooled_tools(&exp.projects, &cv.skills);
            let projects = exp
                .projects
                .iter()
                .map(|p| {
                    let pscore = score_experience_project(p, &top_keywords, &idf, &shared_tools);
                    ProjectScoreDebug {
                        id: p.id.clone(),
                        name: display_name(&p.name),
                        score: pscore,
                        selected: kept_project_ids.contains(&p.id),
                    }
                })
                .collect();
            ExperienceScoreDebug {
                experience_id: exp.id.clone(),
                company: exp.company.clone(),
                role: display_role(&exp.role),
                score: *score,
                selected: selected_ids.contains(&exp.id),
                projects,
            }
        })
        .collect();

    TailorResult {
        tailored,
        top_keywords: keywords.into_iter().take(30).collect(),
        debug_scores,
    }
}

pub fn tailor_cv_with_scorer(
    cv: &LifetimeCV,
    jd_text: &str,
    scorer: &mut crate::services::score::Scorer,
    jd_embedding: Option<&[f32]>,
) -> TailorResult {
    let keywords = extract_keywords(jd_text);
    let top_keywords: Vec<(String, usize)> = keywords.iter().take(40).cloned().collect();

    // Build the TF-IDF corpus from every independently-scorable block in the
    // candidate's own CV and set it on the scorer.
    let mut documents: Vec<Vec<String>> = Vec::new();
    for exp in &cv.experiences {
        let shared_tools = pooled_tools(&exp.projects, &cv.skills);
        for proj in &exp.projects {
            documents.push(extract_terms(&experience_project_text(proj, &shared_tools)));
        }
    }
    for proj in &cv.projects {
        documents.push(extract_terms(&project_text(proj)));
    }
    for skill in &cv.skills {
        documents.push(extract_terms(&skill.name));
    }
    scorer.idf = Idf::build(&documents);

    // ── Experiences ──────────────────────────────────────────────────────────
    let mut scored_exp: Vec<(f32, Experience)> = cv
        .experiences
        .iter()
        .map(|e| {
            (
                scorer.score_experience(e, &top_keywords, jd_embedding, &cv.skills),
                e.clone(),
            )
        })
        .collect();
    scored_exp.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // See the long comment on the equivalent block in `tailor_cv` above for
    // why this uses max(fixed_fraction, mean) rather than just a fixed
    // fraction — this hybrid cutoff is what actually matters here, since
    // this function (not `tailor_cv`) is the one the live app calls for
    // Embedding/Hybrid mode, where the bug this fixes was observed.
    const REL_THRESHOLD: f32 = 0.5;
    let exp_scores: Vec<f32> = scored_exp.iter().map(|(s, _)| *s).collect();
    let use_mean_floor = scorer.mode != crate::services::score::ScoreMode::Keyword;
    let keep_idx = select_by_relative_cutoff(&exp_scores, REL_THRESHOLD, use_mean_floor);
    // Also kept on its own (not just folded into the cutoff): used again
    // below as one component of the overall `match_score` badge.
    let mean_score = if exp_scores.is_empty() {
        0.0
    } else {
        exp_scores.iter().sum::<f32>() / exp_scores.len() as f32
    };

    let mut selected_ids: Vec<String> = keep_idx
        .iter()
        .map(|&i| scored_exp[i].1.id.clone())
        .collect();

    if selected_ids.len() < 2 {
        for exp in cv.experiences.iter().take(2) {
            if !selected_ids.contains(&exp.id) {
                selected_ids.push(exp.id.clone());
            }
        }
    }

    let mut selected_exp: Vec<Experience> = cv
        .experiences
        .iter()
        .filter(|e| selected_ids.contains(&e.id))
        .cloned()
        .collect();

    // Filter projects within each selected experience.
    //
    // For Keyword mode this stays the original fixed-fraction-of-max
    // cutoff, exactly as validated (4 similarly-scored, genuinely-all-
    // relevant projects correctly survive because keyword scores rarely
    // cluster that tightly by accident, so 0.7*max is already a real
    // discriminator).
    //
    // For Embedding/Hybrid mode, cosine similarities cluster far more
    // tightly than keyword scores (a known property of sentence-
    // transformer similarity, same reasoning as the experience-level
    // cutoff above), so 0.7*max alone barely trims anything and this
    // over-includes projects — the gap called out (and left open) in an
    // earlier pass at this function. Fix: raise the floor to the mean
    // score too, same recipe already validated at the experience level,
    // but *only* outside Keyword mode, so the keyword-mode case that
    // motivated the "don't apply this here" comment is left untouched —
    // it's gated on `scorer.mode`, not applied unconditionally.
    for exp in &mut selected_exp {
        if exp.projects.len() <= 1 {
            continue;
        }
        const PROJECT_REL_THRESHOLD: f32 = 0.7;
        let shared_tools = pooled_tools(&exp.projects, &cv.skills);
        let proj_scores: Vec<f32> = exp
            .projects
            .iter()
            .map(|p| scorer.score_experience_project(p, &top_keywords, jd_embedding, &shared_tools))
            .collect();
        let use_mean_floor = scorer.mode != crate::services::score::ScoreMode::Keyword;
        let mut keep_idx =
            select_by_relative_cutoff(&proj_scores, PROJECT_REL_THRESHOLD, use_mean_floor);

        if keep_idx.is_empty() {
            if let Some((best_i, _)) = proj_scores
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                keep_idx.push(best_i);
            }
        }

        let mut i = 0;
        exp.projects.retain(|_| {
            let keep = keep_idx.contains(&i);
            i += 1;
            keep
        });
    }

    // ── Skills ────────────────────────────────────────────────────────────────
    // Score each skill once (was previously computed twice — once per
    // filter closure below — purely wasted work; capturing the scores also
    // lets match_score below include skill relevance, not just experience
    // relevance).
    let skill_scores: Vec<f32> = cv
        .skills
        .iter()
        .map(|s| scorer.score_skill(s, &top_keywords, jd_embedding))
        .collect();

    // Keep only skills related to the JD (score > 0) plus Expert/Mastery
    // ones — unrelated Beginner/Intermediate/Advanced skills are dropped
    // (see `select_tailored_skills`). Uses the precomputed `skill_scores`
    // so the embedding scorer runs once per skill.
    let related_ids: HashSet<&str> = cv
        .skills
        .iter()
        .zip(&skill_scores)
        .filter(|(_, s)| **s > 0.0)
        .map(|(sk, _)| sk.id.as_str())
        .collect();
    let matched_skills =
        select_tailored_skills(&cv.skills, |s| related_ids.contains(s.id.as_str()));

    // ── Projects ──────────────────────────────────────────────────────────────
    let mut scored_proj: Vec<(f32, Project)> = cv
        .projects
        .iter()
        .map(|p| {
            (
                scorer.score_project(p, &top_keywords, jd_embedding),
                p.clone(),
            )
        })
        .collect();
    scored_proj.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let selected_proj: Vec<Project> = scored_proj
        .into_iter()
        .filter(|(s, _)| *s > 0.0)
        .map(|(_, p)| p)
        .collect();

    // ── Match / gap analysis ──────────────────────────────────────────────────
    // The matched/missing keyword TAGS are deliberately always keyword-based,
    // in every mode (including Embedding/Hybrid) — there's no meaningful way
    // to show "semantic similarity" as a list of discrete matched/unmatched
    // keyword chips, since embeddings don't work in terms of individual
    // words at all. This is an intentional design choice, not a bug.
    let cv_terms: HashSet<String> = extract_terms(&cv.all_text()).into_iter().collect();
    let (matched_keywords, missing_keywords): (Vec<String>, Vec<String>) = top_keywords
        .iter()
        .take(30)
        .map(|(kw, _)| kw.clone())
        .partition(|kw| terms_contain(&cv_terms, kw));

    // `match_score` (the headline "X% CORRESPONDANCE" badge in the UI), on
    // the other hand, previously used the SAME keyword-only ratio as the
    // tags above — completely disconnected from `scorer`/`mode`. That meant
    // Embedding and Hybrid mode always showed the exact same percentage as
    // Keyword mode, silently, with nothing indicating the badge wasn't
    // reflecting the mode actually selected (this was caught because a
    // real Embedding-mode run produced byte-for-byte the same 57% and the
    // same matched/missing tag lists as an earlier Keyword-mode run against
    // the same JD — not a coincidence, the code path was identical).
    //
    // Fix: combine `mean_score` (experience relevance, already computed
    // above from `scorer.score_experience(...)`, mode-aware) with the mean
    // of `skill_scores` (also mode-aware, computed above). Using
    // experience-relevance alone regressed CVs with matching skills but
    // few/no experiences down to a flat 0% regardless of skill fit — caught
    // by a test using a minimal fixture (one skill, zero experiences)
    // expecting a nonzero score. Blending both means a skills-heavy CV
    // still registers appropriately, while an experience-heavy CV's score
    // is barely affected (skills contribute one term to the average
    // alongside however many experience scores there are).
    //
    // Clamped to [0.0, 1.0] since cosine similarity (Embedding/Hybrid
    // modes) is mathematically unbounded to [-1, 1], unlike the keyword
    // ratio, which is naturally already bounded to [0, 1] — this keeps the
    // badge from ever showing a negative or over-100% percentage
    // regardless of mode.
    let mean_skill_score = {
        // Only average over skills that actually scored > 0 (the ones
        // ending up in `matched_skills`), not every skill on the CV.
        // Averaging over all skills — including the many that will
        // legitimately score 0 against any single JD in a broad/diverse
        // skill list — would punish having a comprehensive skill list: a
        // CV with 50 skills where 10 are perfectly relevant would score
        // worse on this component than one listing only those same 10.
        // "How well do your *relevant* skills fit" is closer to what a
        // person reads a % match badge as meaning than "average
        // relevance across literally everything you've ever listed."
        let matched: Vec<f32> = skill_scores.iter().copied().filter(|s| *s > 0.0).collect();
        if matched.is_empty() {
            0.0
        } else {
            matched.iter().sum::<f32>() / matched.len() as f32
        }
    };
    // Only include each component if it actually has data — otherwise an
    // empty category (e.g. zero experiences) would contribute a phantom
    // 0.0 into the average and needlessly dilute the other component's
    // score, rather than being cleanly excluded.
    //
    // Weighted, not a flat average: experience and skill scores are on
    // structurally different scales under this scoring scheme — a single
    // short skill name (e.g. "Ansible") can only ever match one or two of
    // the JD's ~40 top keywords, capping its own score low no matter how
    // relevant it is, while a full experience paragraph naturally
    // accumulates many more weighted matches. A flat 50/50 average
    // measurably dragged real-CV scores down (verified: ~39% → ~20% on a
    // real CV/JD pair) purely from this scale mismatch, not from any
    // actual drop in relevance. Weighting experience heavily (0.85) when
    // present keeps the badge close to what experience-relevance alone
    // would show, with skills as a modest secondary signal (0.15) — and
    // if only one category has data (e.g. the skills-only CV that caught
    // the original bug), it's used alone at its own scale, unaffected by
    // the weighting, since the weights cancel out via normalization below.
    const EXPERIENCE_WEIGHT: f32 = 0.85;
    const SKILL_WEIGHT: f32 = 0.15;
    let mut weighted_sum = 0.0;
    let mut weight_total = 0.0;
    if !scored_exp.is_empty() {
        weighted_sum += mean_score * EXPERIENCE_WEIGHT;
        weight_total += EXPERIENCE_WEIGHT;
    }
    if !skill_scores.is_empty() {
        weighted_sum += mean_skill_score * SKILL_WEIGHT;
        weight_total += SKILL_WEIGHT;
    }
    let match_score = if weight_total > 0.0 {
        (weighted_sum / weight_total).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let tailored = TailoredCV {
        personal: cv.personal.clone(),
        experiences: selected_exp,
        all_experiences: cv.experiences.clone(),
        skills: matched_skills,
        education: cv.education.clone(),
        projects: selected_proj,
        languages: cv.languages.clone(),
        certifications: cv.certifications.clone(),
        matched_keywords,
        missing_keywords,
        match_score,
    };

    // Raw scores for every experience/project, independent of the
    // selection above — see `ExperienceScoreDebug`'s doc comment. This is
    // the live (Embedding/Hybrid-capable) path, so this is what actually
    // lets you see whether e.g. KAIMAN scored low (an embedding-quality
    // problem) or scored fine but still lost the cutoff (a selection-logic
    // problem) instead of only ever seeing the final in/out list.
    //
    // `selected` is derived from `tailored.experiences` itself (which
    // project ids actually survived), not recomputed from `pscore` — see
    // the identical comment in `tailor_cv`'s debug_scores construction for
    // why `pscore > 0.0` was wrong here too.
    let kept_project_ids: HashSet<String> = tailored
        .experiences
        .iter()
        .flat_map(|e| e.projects.iter())
        .map(|p| p.id.clone())
        .collect();
    let debug_scores: Vec<ExperienceScoreDebug> = scored_exp
        .iter()
        .map(|(score, exp)| {
            let shared_tools = pooled_tools(&exp.projects, &cv.skills);
            let projects = exp
                .projects
                .iter()
                .map(|p| {
                    let pscore = scorer.score_experience_project(
                        p,
                        &top_keywords,
                        jd_embedding,
                        &shared_tools,
                    );
                    ProjectScoreDebug {
                        id: p.id.clone(),
                        name: display_name(&p.name),
                        score: pscore,
                        selected: kept_project_ids.contains(&p.id),
                    }
                })
                .collect();
            ExperienceScoreDebug {
                experience_id: exp.id.clone(),
                company: exp.company.clone(),
                role: display_role(&exp.role),
                score: *score,
                selected: selected_ids.contains(&exp.id),
                projects,
            }
        })
        .collect();

    TailorResult {
        tailored,
        top_keywords: keywords.into_iter().take(30).collect(),
        debug_scores,
    }
}

/// Placeholder a summary (the base one or a named variant) can contain:
/// the Tailor page expands it to the JD-pertinent skills of the last run.
pub const SUMMARY_SKILLS_PLACEHOLDER: &str = "{{skills}}";

/// How many JD-pertinent skills a `{{skills}}` placeholder expands to.
pub const SUMMARY_SKILLS_CAP: usize = 5;

/// Replaces every `SUMMARY_SKILLS_PLACEHOLDER` in `text` with the first
/// `cap` skills joined by ", ". The list comes from `TailoredCV::skills`,
/// which has already dropped everything unrelated to the offer and is
/// sorted best-first, so a top-`cap` slice is exactly the list the offer
/// cares about. No-op when the placeholder is absent or when there are no
/// skills to show (the placeholder stays literal then, so nothing vanishes
/// silently). Language-independent: `Skill::name` is a single string.
pub fn expand_summary_skills(text: &str, skills: &[Skill], cap: usize) -> String {
    if !text.contains(SUMMARY_SKILLS_PLACEHOLDER) {
        return text.to_string();
    }
    let list = skills
        .iter()
        .take(cap)
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if list.is_empty() {
        return text.to_string();
    }
    text.replace(SUMMARY_SKILLS_PLACEHOLDER, &list)
}

/// Resolves the professional summary a tailored CV should render: either the
/// base "Default" `personal.summary`, or the named variant selected via
/// `choice` (falling back to Default when the name matches nothing). Both
/// languages are run through `expand_summary_skills`, so a `{{skills}}`
/// placeholder becomes this run's JD-pertinent skill list in each language.
pub fn resolve_summary(
    personal: &crate::models::PersonalInfo,
    choice: Option<&str>,
    skills: &[Skill],
) -> crate::models::LocalizedText {
    let src = choice
        .and_then(|name| {
            personal
                .summaries
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.text.clone())
        })
        .unwrap_or_else(|| personal.summary.clone());
    crate::models::LocalizedText {
        en: expand_summary_skills(&src.en, skills, SUMMARY_SKILLS_CAP),
        fr: expand_summary_skills(&src.fr, skills, SUMMARY_SKILLS_CAP),
    }
}

/// Every independently-scorable block of a CV, one entry per document —
/// the corpus `Idf` weighs keywords against. This is the same list the two
/// tailoring entry points build inline before scoring; a dedicated helper
/// so the `{{skills}}` auto-ordering below reuses the exact same corpus.
fn cv_documents(cv: &LifetimeCV) -> Vec<Vec<String>> {
    let mut documents: Vec<Vec<String>> = Vec::new();
    for exp in &cv.experiences {
        let shared_tools = pooled_tools(&exp.projects, &cv.skills);
        for proj in &exp.projects {
            documents.push(extract_terms(&experience_project_text(proj, &shared_tools)));
        }
    }
    for proj in &cv.projects {
        documents.push(extract_terms(&project_text(proj)));
    }
    for skill in &cv.skills {
        documents.push(extract_terms(&skill.name));
    }
    documents
}

/// Re-scores `skills` against a job description with the same keyword
/// machinery as `tailor_cv` and returns them sorted by relevance, best
/// first, dropping anything that doesn't match (`score <= 0`). This is the
/// "automatic" order a `{{skills}}` placeholder expands to — not the CV
/// list order, so a skill the offer clearly wants isn't pushed out of the
/// top-`SUMMARY_SKILLS_CAP` merely by sitting later in the CV.
pub fn sort_skills_by_relevance(cv: &LifetimeCV, skills: &[Skill], jd_text: &str) -> Vec<Skill> {
    let keywords = extract_keywords(jd_text);
    let top_keywords: Vec<(String, usize)> = keywords.iter().take(40).cloned().collect();
    let idf = Idf::build(&cv_documents(cv));
    let mut scored: Vec<(f32, Skill)> = skills
        .iter()
        .map(|s| (score_skill(s, &top_keywords, &idf), s.clone()))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .filter(|(s, _)| *s > 0.0)
        .map(|(_, s)| s)
        .collect()
}

/// The skill list a `{{skills}}` placeholder expands to: the person's
/// explicit `override_ids` when non-empty (resolved to the matching CV
/// skills, in CV order), otherwise the algorithm's choice — `tailored_skills`
/// re-scored against the JD, best first, via `sort_skills_by_relevance`.
pub fn summary_skills_for(
    cv: &LifetimeCV,
    tailored_skills: &[Skill],
    jd_text: &str,
    override_ids: &[String],
) -> Vec<Skill> {
    if override_ids.is_empty() {
        sort_skills_by_relevance(cv, tailored_skills, jd_text)
    } else {
        cv.skills
            .iter()
            .filter(|s| override_ids.contains(&s.id))
            .cloned()
            .collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
