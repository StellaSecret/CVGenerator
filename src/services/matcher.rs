use crate::models::{Experience, ExperienceProject, LifetimeCV, Project, Skill, TailoredCV};
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

fn score_experience(exp: &Experience, keywords: &[(String, usize)], idf: &Idf) -> f32 {
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
        text.push_str(&proj.tools.join(" "));
    }
    score_text(&text, keywords, idf)
}

/// Union (deduplicated, order-preserving) of `tools` across every project
/// within a single `Experience`.
///
/// Why this exists: `tools` is already a per-`ExperienceProject` field (not
/// per-`Experience`) in the data model, and the editor already lets you tag
/// tools on an individual project. But plenty of real CVs — including the
/// one this was built against — are *authored* with one combined tech-stack
/// line per role covering all of that role's sub-projects together, rather
/// than one per sub-project (PDF import has no reliable way to attribute a
/// trailing "Tech: X, Y, Z" line back to a specific sub-project when the
/// source text only ever wrote it once per role, at the very end). The
/// practical effect: whichever project happened to end up holding that
/// combined list gets a large, correct-looking `tools` field, while its
/// siblings — including, concretely, the ones whose own bullets are what
/// actually name AWX/Ansible work — end up with an empty one, and lose out
/// on keyword matches for tools they genuinely used.
///
/// Rather than trying to guess a per-project attribution the source text
/// doesn't actually contain, every project is scored against the pool of
/// tools used anywhere in its parent experience, on top of its own. This
/// can't over-credit a project with a tool used by a *different*
/// experience — only ones already known to belong to the same role.
pub fn pooled_tools(projects: &[ExperienceProject]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut pooled = Vec::new();
    for proj in projects {
        for tool in &proj.tools {
            if seen.insert(tool.clone()) {
                pooled.push(tool.clone());
            }
        }
    }
    pooled
}

/// Builds the scorable text blob for a single sub-project. Shared between
/// scoring (`score_experience_project`) and IDF corpus construction in
/// `tailor_cv`, so the two always see identical text. `shared_tools` should
/// be the parent experience's `pooled_tools()` — see that function's doc
/// comment for why this needs to be pooled rather than using only
/// `proj.tools`.
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
    text.push_str(&proj.tools.join(" "));
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
    pub name: String,
    pub score: f32,
    pub selected: bool,
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
///   - Skills are filtered to those with score > 0, then sorted best-first.
///     Skills with score = 0 are appended at the end (separated).
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
        let shared_tools = pooled_tools(&exp.projects);
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
        .map(|e| (score_experience(e, &top_keywords, &idf), e.clone()))
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
        let shared_tools = pooled_tools(&exp.projects);
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
    let mut matched_skills: Vec<Skill> = cv
        .skills
        .iter()
        .filter(|s| score_skill(s, &top_keywords, &idf) > 0.0)
        .cloned()
        .collect();

    let unmatched_skills: Vec<Skill> = cv
        .skills
        .iter()
        .filter(|s| score_skill(s, &top_keywords, &idf) == 0.0)
        .cloned()
        .collect();

    matched_skills.extend(unmatched_skills);

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
    let debug_scores: Vec<ExperienceScoreDebug> = scored_exp
        .iter()
        .map(|(score, exp)| {
            let shared_tools = pooled_tools(&exp.projects);
            let projects = exp
                .projects
                .iter()
                .map(|p| {
                    let pscore = score_experience_project(p, &top_keywords, &idf, &shared_tools);
                    ProjectScoreDebug {
                        name: display_name(&p.name),
                        score: pscore,
                        selected: pscore > 0.0,
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
        let shared_tools = pooled_tools(&exp.projects);
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
                scorer.score_experience(e, &top_keywords, jd_embedding),
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
        let shared_tools = pooled_tools(&exp.projects);
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

    let mut matched_skills: Vec<Skill> = cv
        .skills
        .iter()
        .zip(&skill_scores)
        .filter(|(_, s)| **s > 0.0)
        .map(|(skill, _)| skill.clone())
        .collect();

    let unmatched_skills: Vec<Skill> = cv
        .skills
        .iter()
        .zip(&skill_scores)
        .filter(|(_, s)| **s == 0.0)
        .map(|(skill, _)| skill.clone())
        .collect();

    matched_skills.extend(unmatched_skills);

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
    let debug_scores: Vec<ExperienceScoreDebug> = scored_exp
        .iter()
        .map(|(score, exp)| {
            let shared_tools = pooled_tools(&exp.projects);
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
                        name: display_name(&p.name),
                        score: pscore,
                        selected: pscore > 0.0,
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
                title: LocalizedText::same("Backend Engineer"),
                summary: LocalizedText::same("Experienced distributed-systems developer"),
                ..Default::default()
            },
            experiences: vec![
                Experience {
                    id: "exp-1".to_string(),
                    company: "Acme Corp".to_string(),
                    role: LocalizedText::same("Software Engineer"),
                    start_date: "Jan 2021".to_string(),
                    end_date: "Present".to_string(),
                    projects: vec![ExperienceProject {
                        name: LocalizedText::same("Distributed Systems"),
                        context: vec![LocalizedText::same(
                            "High-throughput microservices architecture",
                        )],
                        bullets: vec![
                            LocalizedText::same("Built distributed systems using Rust and Tokio"),
                            LocalizedText::same("Reduced API latency by 40% through caching"),
                        ],
                        tools: vec![
                            "Rust".to_string(),
                            "PostgreSQL".to_string(),
                            "Kubernetes".to_string(),
                        ],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                Experience {
                    id: "exp-2".to_string(),
                    company: "Beta Ltd".to_string(),
                    role: LocalizedText::same("Junior Developer"),
                    start_date: "Jun 2019".to_string(),
                    end_date: "Dec 2020".to_string(),
                    projects: vec![ExperienceProject {
                        name: LocalizedText::same("Web Applications"),
                        context: vec![LocalizedText::same("Customer-facing portal overhaul")],
                        bullets: vec![LocalizedText::same(
                            "Developed web applications with React and TypeScript",
                        )],
                        tools: vec!["JavaScript".to_string(), "React".to_string()],
                        ..Default::default()
                    }],
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
                description: LocalizedText::same("CV generator written in Rust using Dioxus"),
                tools: vec!["Rust".to_string(), "Dioxus".to_string()],
                bullets: vec![LocalizedText::same("Keyword matching algorithm")],
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
        // extract_keywords now also emits bigrams/trigrams (e.g. "rust rust"),
        // so the three case variants no longer collapse to a single overall
        // entry — but they must still collapse to a single *unigram* entry,
        // which should rank first since it has the highest frequency.
        let kws = extract_keywords("Rust RUST rust");
        assert_eq!(kws[0].0, "rust");
        assert_eq!(kws[0].1, 3);
        let unigram_entries = kws.iter().filter(|(k, _)| !k.contains(' ')).count();
        assert_eq!(
            unigram_entries, 1,
            "All case variants should collapse to a single unigram entry"
        );
    }

    // ── select_by_relative_cutoff ────────────────────────────────────────────

    #[test]
    fn cutoff_keyword_like_spread_keeps_only_top_without_mean_floor() {
        // Wide spread, e.g. real keyword scores: only the top one clears 0.7*max.
        let scores = vec![0.82, 0.30, 0.10, 0.05];
        let kept = select_by_relative_cutoff(&scores, 0.7, false);
        assert_eq!(kept, vec![0]);
    }

    #[test]
    fn cutoff_keyword_like_near_ties_all_kept_without_mean_floor() {
        // 4 genuinely-similar, all-relevant scores (the case the old
        // project-level code was tuned to preserve) — fixed fraction alone
        // keeps all 4, exactly like before this change.
        let scores = vec![0.82, 0.79, 0.75, 0.71];
        let mut kept = select_by_relative_cutoff(&scores, 0.7, false);
        kept.sort();
        assert_eq!(kept, vec![0, 1, 2, 3]);
    }

    #[test]
    fn cutoff_embedding_like_tight_cluster_trimmed_with_mean_floor() {
        // Tightly clustered cosine-similarity-like scores (everything
        // sits within 0.7x of the max, so the fixed fraction alone can't
        // discriminate at all — this is the exact "6 of 7 experiences
        // included instead of ~5" symptom this fix addresses).
        let scores = vec![0.75, 0.72, 0.70, 0.68, 0.65, 0.60, 0.58];
        let fixed_only = select_by_relative_cutoff(&scores, 0.7, false);
        let with_mean_floor = select_by_relative_cutoff(&scores, 0.7, true);
        assert_eq!(
            fixed_only.len(),
            scores.len(),
            "fixed fraction alone should barely filter a tight cluster"
        );
        assert!(
            with_mean_floor.len() < fixed_only.len(),
            "mean floor should trim the tight cluster down, got {:?}",
            with_mean_floor
        );
        let mut with_mean_floor_sorted = with_mean_floor.clone();
        with_mean_floor_sorted.sort();
        // Index 4 (0.65) is included too: it's within the 3% near-tie
        // margin below the mean-floor cutoff (see NEAR_TIE_MARGIN_FRACTION).
        assert_eq!(with_mean_floor_sorted, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn cutoff_near_tie_margin_rescues_a_narrow_miss_but_not_a_real_gap() {
        // These are the *actual* observed scores from a real embedding run
        // (DTNUM, SIRIUS, proxIT, KAIMAN, BRED IT, CA-GIP, Groupe HN) that
        // motivated this margin: KAIMAN (genuinely the most relevant
        // remaining experience for an AWX/CIS-hardening JD) missed the
        // mean-floor cutoff by 0.0094 while proxIT (pure mainframe, no
        // AWX/Ansible/CIS content at all) cleared it — out of an overall
        // score range of about 0.18. That gap is noise, not signal, and
        // dropping KAIMAN over it was the wrong call.
        let scores = vec![0.4858, 0.4784, 0.4070, 0.3954, 0.3866, 0.3755, 0.3047];
        let kept = select_by_relative_cutoff(&scores, 0.5, true);
        let mut kept_sorted = kept.clone();
        kept_sorted.sort();
        // indices: 0=DTNUM, 1=SIRIUS, 2=proxIT, 3=KAIMAN kept;
        // 4=BRED IT, 5=CA-GIP, 6=Groupe HN correctly still excluded —
        // the margin rescues a narrow miss, it doesn't just keep everyone.
        assert_eq!(kept_sorted, vec![0, 1, 2, 3]);
    }

    #[test]
    fn cutoff_empty_scores_returns_empty() {
        assert!(select_by_relative_cutoff(&[], 0.7, false).is_empty());
        assert!(select_by_relative_cutoff(&[], 0.7, true).is_empty());
    }

    #[test]
    fn cutoff_zero_scores_excluded_even_under_cutoff() {
        // A 0.0 score should never be selected regardless of where the
        // cutoff lands (mirrors the old `*s > 0.0` guard).
        let scores = vec![0.0, 0.0, 0.0];
        assert!(select_by_relative_cutoff(&scores, 0.7, false).is_empty());
        assert!(select_by_relative_cutoff(&scores, 0.7, true).is_empty());
    }

    // Regression coverage for a real data-modeling gap: a CV author (or
    // PDF import) can write one combined tools line covering all of a
    // role's sub-projects, leaving individual `ExperienceProject.tools`
    // fields empty except on whichever project happened to end up holding
    // it. `pooled_tools` is what lets per-project scoring still credit a
    // project for a tool that's only recorded on a sibling within the same
    // experience — see its doc comment for the full rationale.
    #[test]
    fn pooled_tools_unions_across_sibling_projects_without_duplicates() {
        let p1 = ExperienceProject {
            tools: vec!["Ansible".to_string(), "AWX".to_string()],
            ..Default::default()
        };
        let p2 = ExperienceProject {
            // "AWX" repeated on purpose: must not appear twice in the pool.
            tools: vec!["AWX".to_string(), "Terraform".to_string()],
            ..Default::default()
        };
        let p3 = ExperienceProject {
            tools: vec![],
            ..Default::default()
        };
        let pooled = pooled_tools(&[p1, p2, p3]);
        assert_eq!(pooled, vec!["Ansible", "AWX", "Terraform"]);
    }

    #[test]
    fn experience_project_scoring_credits_tools_only_recorded_on_a_sibling() {
        // Mirrors the real KAIMAN scenario: the project whose own bullets
        // actually reference AWX has an EMPTY tools list, while a sibling
        // project (unrelated bullets) holds the full tools list for the
        // whole role. Without pooling, the AWX-mentioning project would
        // get no credit at all for the "awx"/"ansible" keywords beyond
        // whatever it happens to say in prose.
        let keywords = vec![("awx".to_string(), 5), ("ansible".to_string(), 5)];

        let mentions_awx_no_tools = ExperienceProject {
            bullets: vec![crate::models::LocalizedText::same(
                "conçu des playbooks awx pour ce projet",
            )],
            tools: vec![],
            ..Default::default()
        };
        let sibling_holds_the_tools = ExperienceProject {
            bullets: vec![crate::models::LocalizedText::same(
                "support technique sans rapport",
            )],
            tools: vec!["AWX".to_string(), "Ansible".to_string()],
            ..Default::default()
        };

        // A tiny synthetic IDF corpus is enough here — we're only checking
        // that pooling changes the *relative* score, not testing IDF
        // weighting itself.
        let idf = Idf::build(&[
            vec!["awx".to_string(), "ansible".to_string()],
            vec!["unrelated".to_string(), "terms".to_string()],
        ]);

        let shared_tools = pooled_tools(&[
            mentions_awx_no_tools.clone(),
            sibling_holds_the_tools.clone(),
        ]);

        let score_without_pooling =
            score_experience_project(&mentions_awx_no_tools, &keywords, &idf, &[]);
        let score_with_pooling =
            score_experience_project(&mentions_awx_no_tools, &keywords, &idf, &shared_tools);

        assert!(
            score_with_pooling > score_without_pooling,
            "pooling sibling tools should raise this project's score \
             (without: {score_without_pooling}, with: {score_with_pooling})"
        );
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

    // An empty-corpus Idf falls back to a default weight of 1.0 for every
    // term (see `Idf::get`), which makes these tests equivalent to plain
    // frequency weighting — the same behaviour the old (pre-TF-IDF) tests
    // asserted on.
    fn no_idf() -> Idf {
        Idf::build(&[])
    }

    #[test]
    fn score_text_perfect_match_is_one() {
        let kws = vec![("rust".to_string(), 2), ("postgresql".to_string(), 1)];
        let s = score_text("rust postgresql developer", &kws, &no_idf());
        assert_eq!(s, 1.0);
    }

    #[test]
    fn score_text_no_match_is_zero() {
        let kws = vec![("golang".to_string(), 1), ("java".to_string(), 1)];
        let s = score_text("rust postgresql developer", &kws, &no_idf());
        assert_eq!(s, 0.0);
    }

    #[test]
    fn score_text_partial_match_weighted() {
        // rust weight=2, java weight=1, total=3; only rust matches → 2/3
        let kws = vec![("rust".to_string(), 2), ("java".to_string(), 1)];
        let s = score_text("senior rust developer", &kws, &no_idf());
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
        assert_eq!(score_text("", &[], &no_idf()), 0.0);
        assert_eq!(score_text("rust", &[], &no_idf()), 0.0);
        assert_eq!(score_text("", &[("rust".to_string(), 1)], &no_idf()), 0.0);
    }

    #[test]
    fn score_text_is_case_insensitive() {
        let kws = vec![("rust".to_string(), 1)];
        // keyword is lowercase; text has uppercase — should still match
        assert_eq!(score_text("RUST Engineer", &kws, &no_idf()), 1.0);
    }

    // ── stemming / synonyms / fuzzy matching ────────────────────────────────

    #[test]
    fn synonyms_collapse_fr_en_variants() {
        // "hardening" (EN) and "durcissement" (FR) should normalize to the
        // same canonical term via the synonym dictionary.
        assert_eq!(normalize("hardening"), normalize("durcissement"));
        assert_eq!(normalize("hardening"), normalize("sécurisation"));
    }

    #[test]
    fn stemming_collapses_inflections() {
        // French plural/verb-form variants of "déploiement" should share a
        // stem, and so should the English "deploy"/"deployment" family.
        assert_eq!(stem("deploiement"), stem("deploiements"));
        assert_eq!(normalize("deploiement"), normalize("deploying"));
    }

    #[test]
    fn fuzzy_eq_tolerates_small_typos_not_big_ones() {
        assert!(fuzzy_eq("hardening", "hardning")); // dropped letter, len>=8 → tolerance 2
        assert!(!fuzzy_eq("cis", "sql")); // short words, no fuzziness allowed
        assert!(!fuzzy_eq("hardening", "monitoring")); // unrelated words, too far apart
    }

    #[test]
    fn ngrams_capture_multiword_phrases() {
        let kws = extract_keywords("gestion de version rollback");
        let names: Vec<&str> = kws.iter().map(|(k, _)| k.as_str()).collect();
        // "de" is a stop word, so the surviving bigram is "gestion version".
        assert!(
            names.iter().any(|n| n.contains(' ')),
            "expected at least one multi-word term, got {:?}",
            names
        );
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
            degree: LocalizedText::same("MSc"),
            field: LocalizedText::same("Computer Science"),
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
