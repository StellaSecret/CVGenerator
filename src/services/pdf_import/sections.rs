use super::*;

// ── CV Text Parser ──────────────────────────────────────────────────────────

/// Known section headers (EN + FR) mapped to internal section names.
pub(super) const SECTION_HEADERS: &[(&str, &str)] = &[
    // Experience
    ("experience", "experience"),
    ("experiences", "experience"),
    ("work experience", "experience"),
    ("work history", "experience"),
    ("employment", "experience"),
    ("professional experience", "experience"),
    ("parcours professionnel", "experience"),
    ("expérience professionnelle", "experience"),
    ("expérience", "experience"),
    // Plural "Expériences" — as distinct a heading in the wild as the
    // singular form; French resume templates use both interchangeably.
    ("expériences", "experience"),
    ("emplois", "experience"),
    // Education
    ("education", "education"),
    ("academic background", "education"),
    ("academic experience", "education"),
    ("formation", "education"),
    // Plural "Formations" — same rationale as "Expériences" above.
    ("formations", "education"),
    ("éducation", "education"),
    ("parcours académique", "education"),
    // Skills
    ("skills", "skills"),
    ("technical skills", "skills"),
    ("competencies", "skills"),
    ("core competencies", "skills"),
    ("compétences", "skills"),
    ("compétences techniques", "skills"),
    ("compétence", "skills"),
    ("compétences clés", "skills"),
    // Projects
    ("projects", "projects"),
    ("personal projects", "projects"),
    ("side projects", "projects"),
    ("projets", "projects"),
    ("projets personnels", "projects"),
    // Certifications
    ("certifications", "certifications"),
    ("certificates", "certifications"),
    ("licenses", "certifications"),
    ("certificats", "certifications"),
    // Languages
    ("languages", "languages"),
    ("langues", "languages"),
    // A combined "Languages & Interests" heading is common enough in French
    // templates (interests/hobbies get a couple of lines tacked onto the
    // same section as languages, rather than their own heading) that it's
    // worth recognizing directly rather than letting it fall through
    // unrecognized and bleed into whatever section came before it.
    ("langues et centres d'intérêt", "languages"),
    ("langues et centres d'intérêts", "languages"),
    // Sections we recognize but intentionally don't import into any CV field
    // yet — mapping them here just stops their content from bleeding into
    // whatever the previous real section was (e.g. "OTHERS"/"INTERESTS"
    // text getting appended onto Certifications).
    ("summary", "ignore"),
    ("professional summary", "ignore"),
    ("résumé", "ignore"),
    ("profil", "ignore"),
    ("values", "ignore"),
    ("valeurs", "ignore"),
    ("other", "ignore"),
    ("others", "ignore"),
    ("autres", "ignore"),
    ("interests", "ignore"),
    ("hobbies", "ignore"),
    ("centres d'intérêt", "ignore"),
    ("random skills", "ignore"),
];

/// Regex-ish helpers (no regex crate — keep it simple).
pub(crate) fn extract_email(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let w = word.trim_matches(['<', '>', ',', ';', '(', ')']);
        if w.contains('@') && w.contains('.') && !w.starts_with('@') && !w.ends_with('.') {
            let email: String = w
                .chars()
                .filter(|c| {
                    c.is_alphanumeric()
                        || *c == '@'
                        || *c == '.'
                        || *c == '_'
                        || *c == '-'
                        || *c == '+'
                })
                .collect();
            // No separate recheck after filtering is needed: the filter
            // above explicitly keeps both '@' and '.', and line-w's gate
            // already guaranteed both are present — so the pair can never
            // be filtered away.
            return Some(email);
        }
    }
    None
}

pub(crate) fn extract_phone(text: &str) -> Option<String> {
    // Scan the whole text for phone-like patterns
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_ascii_digit() { c } else { ' ' })
        .collect();
    // Look for sequences of 7-15 digits (with optional leading +)
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    // Build a candidate: start with + if original text has it
    let has_plus = text.contains('+');
    let digits: String = words
        .join("")
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.len() >= 7 && digits.len() <= 15 {
        return if has_plus {
            Some(format!("+{}", digits))
        } else {
            Some(digits)
        };
    }
    None
}

pub(crate) fn extract_urls(text: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut linkedin = None;
    let mut github = None;
    let mut website = None;
    for word in text.split_whitespace() {
        let w = word.trim_matches(['<', '>', ',', ';', '(', ')']);
        if w.is_empty() {
            continue;
        }
        if linkedin.is_none() && (w.contains("linkedin.com") || w.contains("linkedin.")) {
            linkedin = Some(w.to_string());
        } else if github.is_none() && w.contains("github.com") {
            // Require the actual profile/repo domain (github.com), not any
            // domain that merely contains "github." — e.g. a personal site
            // hosted at "name.github.io" is a website, not a GitHub profile.
            github = Some(w.to_string());
        } else if website.is_none()
            && w != linkedin.as_deref().unwrap_or("")
            && (w.starts_with("http://")
                || w.starts_with("https://")
                || w.starts_with("www.")
                || looks_like_bare_domain(w))
        {
            website = Some(w.to_string());
        }
    }
    (linkedin, github, website)
}

/// Heuristic check for a bare domain/URL with no scheme or "www." prefix,
/// e.g. "falltrades.github.io/engineering" — common when a contact line uses
/// an icon glyph instead of "http(s)://" before the URL.
pub(super) fn looks_like_bare_domain(w: &str) -> bool {
    let host = w.split('/').next().unwrap_or(w);
    if !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
        return false;
    }
    if host.contains('@') {
        return false;
    }
    let known_tlds = [
        ".com", ".io", ".dev", ".net", ".org", ".me", ".fr", ".co", ".app", ".tech", ".xyz",
        ".info", ".site",
    ];
    known_tlds
        .iter()
        .any(|tld| host.to_lowercase().ends_with(tld))
        && host
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
}

/// Detect which section a header line belongs to.
/// Max length of the same-line remainder after "Header:"/"Header —" for it
/// to still count as a section heading. Real section headers occasionally
/// carry a short same-line note (e.g. "Skills: React, Go, AWS"), but a body
/// line that merely happens to *start* with a word that's also a section
/// keyword — e.g. our own renderer's "Other:" sub-label for uncategorized
/// skills, followed by the full comma-separated skills list — is not a
/// heading and must not swallow the rest of the section as if it were one.
pub(super) const SECTION_HEADER_INLINE_CONTENT_LIMIT: usize = 40;

pub(super) fn detect_section(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    let lower = trimmed.to_lowercase();
    for (header, section) in SECTION_HEADERS {
        if lower == *header {
            return Some(section);
        }
        for sep in [":", " —"] {
            let prefix = format!("{header}{sep}");
            if let Some(rest) = lower.strip_prefix(&prefix) {
                if rest.trim().len() <= SECTION_HEADER_INLINE_CONTENT_LIMIT {
                    return Some(section);
                }
            }
        }
    }
    None
}

/// True for a short, plain, standalone line that's plausibly a job title
/// sitting on its own line right after a "Company · Location  Start – End"
/// header row (see the comment in `parse_experiences` where this is used).
/// Deliberately conservative: only used to disambiguate a layout question
/// for the line immediately following a freshly-detected date range, so
/// false negatives just fall back to the older role-first interpretation
/// rather than misfiring on unrelated content further down the page.
pub(super) fn looks_like_bare_role_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 100 {
        return false;
    }
    if trimmed.starts_with(['•', '·', '-', '–', '*', '▸', '▪']) {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if lower.starts_with("project ")
        || lower.starts_with("situation")
        || lower.starts_with("tasks")
        || lower.starts_with("techs")
        || lower.starts_with("tools")
    {
        return false;
    }
    // A real role title is short and standalone, not a sentence — bail if
    // it contains ". " (mid-sentence period) suggesting running prose.
    if trimmed.contains(". ") {
        return false;
    }
    // A real role title is capitalized ("Architecte DevOps",
    // "Administrateur système WebOps") — it's never the wrapped remainder
    // of a sentence that started on the PREVIOUS (now-scrolled-off) line,
    // which — in French and English alike — almost always resumes on a
    // lowercase word (e.g. "...concernant les" wrapping onto "volets
    // sécurité et conformité"). That wrapped-tail case is otherwise
    // indistinguishable from a genuine short title by every check above
    // (short, no bullet marker, no mid-sentence period, no date) — it was
    // previously mistaken for the NEXT job's role, silently swallowing the
    // real role line into that job's body text instead. Checking the
    // first letter's case catches it without needing to know anything
    // about what came before.
    if trimmed.chars().next().is_some_and(|c| c.is_lowercase()) {
        return false;
    }
    extract_date_range_from_end(trimmed).is_none()
        && extract_standalone_date_range(trimmed).is_none()
}

/// Split a "Company · Location" (or "Company, Location" / "Company |
/// Location") string into its two parts. Falls back to treating the whole
/// string as the company with an empty location if no separator is found.
pub(super) fn split_company_and_location(text: &str) -> (String, String) {
    for sep in [" · ", " | ", ", "] {
        if let Some(pos) = text.find(sep) {
            return (
                text[..pos].trim().to_string(),
                text[pos + sep.len()..].trim().to_string(),
            );
        }
    }
    (text.trim().to_string(), String::new())
}

/// Extract a name from the first few lines. Heuristic: first non-empty line
/// that doesn't look like a header/section/contact info.
pub(super) fn guess_name(lines: &[&str]) -> Option<String> {
    // Defensive belt-and-suspenders alongside the ToUnicodeMap fix above:
    // strip any stray control characters before validating the line as a
    // name, rather than requiring the *entire* line to already be clean.
    // A single leftover corrupted byte (font-encoding edge cases aren't
    // fully eliminable) would otherwise fail the alphabetic check below
    // and cause the whole name line to be skipped — falling through to
    // the next candidate line, e.g. the job title, and silently
    // replacing the person's name with their title.
    let cleaned: Vec<String> = lines
        .iter()
        .map(|l| l.chars().filter(|c| !c.is_control()).collect::<String>())
        .collect();
    for line in cleaned.iter().take(5) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if detect_section(trimmed).is_some() {
            continue;
        }
        if extract_email(trimmed).is_some() || extract_phone(trimmed).is_some() {
            continue;
        }
        if trimmed.starts_with("http")
            || trimmed.starts_with("www")
            || trimmed.starts_with("linkedin")
        {
            continue;
        }
        // Likely a name: 2-4 words, mostly alphabetic
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.len() >= 2
            && words.len() <= 5
            && words.iter().all(|w| {
                w.chars().all(|c| {
                    c.is_alphabetic()
                        || c == '.'
                        || c == '-'
                        || c == '\''
                        || c == 'é'
                        || c == 'è'
                        || c == 'ê'
                        || c == 'ë'
                        || c == 'à'
                        || c == 'â'
                        || c == 'ç'
                        || c == 'ô'
                        || c == 'ù'
                        || c == 'û'
                        || c == 'ü'
                        || c == 'ï'
                        || c == 'î'
                })
            })
        {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Try to identify a professional title from early lines (after name).
pub(super) fn guess_title(lines: &[&str]) -> Option<String> {
    let name_line_idx = lines.iter().take(5).position(|l| {
        let t = l.trim();
        !t.is_empty()
            && extract_email(t).is_none()
            && extract_phone(t).is_none()
            && !t.starts_with("http")
    });
    let start_after = name_line_idx.map(|i| i + 1).unwrap_or(0);

    let title_keywords = [
        "engineer",
        "developer",
        "architect",
        "manager",
        "lead",
        "director",
        "consultant",
        "analyst",
        "scientist",
        "designer",
        "ingénieur",
        "développeur",
        "architecte",
        "manager",
        "chef",
        "directeur",
        "consultant",
        "analyste",
        "scientifique",
        "concepteur",
    ];

    for line in &lines[start_after..lines.len().min(start_after + 4)] {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if title_keywords.iter().any(|kw| lower.contains(kw)) {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Split text into sections based on detected headers.
pub(super) fn split_into_sections(text: &str) -> Vec<(&str, Vec<String>)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut sections: Vec<(&str, Vec<String>)> = Vec::new();
    let mut current_section = "header";
    let mut current_lines = Vec::new();

    for i in 0..lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            continue;
        }

        // "Compétences" alone is a recognized (French) synonym for the
        // Skills section — correctly so, since most resumes that use it
        // mean exactly that. But some resumes instead use it as the first
        // half of an unrelated two-line label, "Compétences Globales"
        // ("Global Competencies" — a soft-skills self-rating box, often
        // sitting in a sidebar next to Experience, distinct from that same
        // resume's *actual* skills section elsewhere). Bail out of
        // treating it as a header in that specific case — a false
        // section-boundary here doesn't just mislabel a couple of lines,
        // it truncates whatever section was legitimately still open
        // (commonly Experience) right in the middle of it.
        let is_competences_globales_false_positive = trimmed.to_lowercase() == "compétences"
            && lines[i + 1..]
                .iter()
                .map(|l| l.trim())
                .find(|l| !l.is_empty())
                .is_some_and(|next| next.to_lowercase() == "globales");

        if is_competences_globales_false_positive {
            current_lines.push(trimmed.to_string());
            continue;
        }

        // Recover from a section state that's drifted away from
        // "experience" due to an interrupting sidebar (e.g. a self-rating
        // "Values"/"Core Competencies" box, or a per-job "Tools" list)
        // whose own headers matched real section keywords and permanently
        // flipped `current_section` — with nothing in a purely
        // header-driven state machine to ever flip it back. Concretely:
        // once such a sidebar drags `current_section` to "skills" (or a
        // similar unrelated section), every later job's role, company,
        // and full narrative silently end up filed under "skills" for the
        // rest of the document, rather than as their own Experience
        // entries — a bigger loss than the sidebar mixing into "skills"
        // in the first place.
        //
        // A standalone icon-prefixed date range (this resume's own job-
        // header row shape — see `extract_standalone_date_range`) is a
        // strong, low-false-positive signal that we've reached a new
        // job's header, wherever `current_section` currently claims to
        // be. When we see one outside "experience", recover this job's
        // role+company by scanning backward through whatever accumulated
        // in `current_lines` for the last two lines that don't look like
        // sidebar tool/skill bleed (a category label, a bullet, or a
        // "<tool> N+ yrs" line) — skipping right over an interposed Tools
        // sidebar to reach the actual title+company lines that preceded
        // it. Everything else pending is flushed to the old section as
        // usual (it's mostly genuine skill/tag content anyway).
        //
        // This can produce more than one ("experience", ...) tuple in the
        // returned list (one per resumption); callers that key off section
        // name must merge same-named tuples rather than assume each name
        // appears once — see `merge_duplicate_sections`.
        if current_section != "experience" && extract_standalone_date_range(trimmed).is_some() {
            let mut idx = current_lines.len();
            let mut recovered: Vec<String> = Vec::new();
            while idx > 0 && recovered.len() < 2 {
                idx -= 1;
                if looks_like_tool_bleed_line(&current_lines[idx]) {
                    continue;
                }
                recovered.push(current_lines[idx].clone());
            }
            if recovered.len() == 2 {
                // A recovered pair naming a *project* sub-header (e.g.
                // "Project 2: CITADEL – Platform Engineering"), rather
                // than a job's actual role+company, means we've landed on
                // one of that job's *project* date lines, not the job's
                // own header — `current_lines` at this point is still
                // just as jumbled as it was going in, so don't commit to
                // a bogus "experience" entry here. Leave current_section
                // as-is and keep accumulating; the job's real role+company
                // lines are further back than this scan reached, and a
                // later, real job-header date line will find them once
                // this project's own content also becomes part of the
                // (still wrong) pending run.
                let looks_like_project_subheader = recovered
                    .iter()
                    .any(|l| l.trim_start().to_lowercase().starts_with("project"));
                if !looks_like_project_subheader {
                    recovered.reverse();
                    // `recovered` was built entirely from `current_lines`
                    // and holds exactly 2 lines here, so `current_lines`
                    // is guaranteed non-empty — no emptiness/section guard
                    // needed to avoid pushing an empty ("header", []) tuple.
                    sections.push((current_section, std::mem::take(&mut current_lines)));
                    current_section = "experience";
                    current_lines = recovered;
                    current_lines.push(trimmed.to_string());
                    continue;
                }
            }
        }

        if let Some(section) = detect_section(trimmed) {
            if !current_lines.is_empty() || current_section != "header" {
                sections.push((current_section, std::mem::take(&mut current_lines)));
            }
            current_section = section;
        } else {
            current_lines.push(trimmed.to_string());
        }
    }
    if !current_lines.is_empty() {
        sections.push((current_section, current_lines));
    }
    sections
}

/// Merges tuples that share the same section name into one, preserving
/// each name's first-occurrence position and concatenating their lines in
/// order. `split_into_sections` can legitimately emit more than one tuple
/// for the same name — most commonly "experience" when its
/// resumption-after-interruption recovery (see the comment on that logic)
/// fires more than once — and every downstream consumer that does
/// `match section { "experience" => cv.experiences = parse_experiences(lines), ... }`
/// would otherwise silently keep only the *last* such tuple's content,
/// discarding all the earlier ones it just went to the trouble of
/// recovering.
pub(super) fn merge_duplicate_sections<'a>(
    sections: Vec<(&'a str, Vec<String>)>,
) -> Vec<(&'a str, Vec<String>)> {
    let mut merged: Vec<(&'a str, Vec<String>)> = Vec::new();
    for (name, lines) in sections {
        if let Some(existing) = merged.iter_mut().find(|(n, _)| *n == name) {
            existing.1.extend(lines);
        } else {
            merged.push((name, lines));
        }
    }
    merged
}

/// A single-line, lookahead-free approximation of "this looks like sidebar
/// tool/skill bleed" — used by `split_into_sections`'s experience-resumption
/// recovery (see the comment above its call site) to scan backward past an
/// interposed Tools/skills sidebar and find the real role+company lines
/// that preceded it. Deliberately permissive: a false positive here just
/// means the backward scan keeps looking a little further, which is
/// harmless, whereas a false negative could grab a stray tool name as if
/// it were a job title.
pub(super) fn looks_like_tool_bleed_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with(['•', '·', '-', '–', '*', '▸', '▪']) {
        return true;
    }
    harvest_skill_segments(trimmed).is_some() || is_bare_years_marker(trimmed)
}

/// Common "block label" prefixes used in structured resume bullet groups
/// (e.g. "Situation & Tasks: ...", "Techs: Kubernetes, Docker, ..."). A line
/// starting with one of these should always be treated as the start of a new
/// block, never as a wrapped continuation of the previous bullet.
pub(super) const BLOCK_LABEL_PREFIXES: &[&str] = &[
    "situation",
    "context",
    "task",
    "action",
    "result",
    "achievement",
    "techs",
    "tech stack",
    "technologies",
    "duties",
    "duty",
];

pub(crate) fn is_project_header(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("project") || lower.starts_with("projet")
}

pub(super) fn is_context_label(line: &str) -> bool {
    let lower = line.to_lowercase();
    let Some(colon_idx) = lower.find(':') else {
        return false;
    };
    let prefix = lower[..colon_idx].trim();
    // Keep this reasonably short so we don't mistake a long sentence that
    // merely contains a colon for a block label.
    prefix.len() <= 30 && BLOCK_LABEL_PREFIXES.iter().any(|p| prefix.starts_with(p))
}

/// True for any line that should never be swallowed as a wrapped bullet
/// continuation — either a "Project N: ..." sub-entry header, or a
/// "Situation:"/"Techs:"/etc. context label.
pub(super) fn looks_like_block_label(line: &str) -> bool {
    is_project_header(line) || is_context_label(line)
}

pub(super) fn ends_with_terminal_punct(s: &str) -> bool {
    s.trim_end().ends_with(['.', '!', '?', ':'])
}
