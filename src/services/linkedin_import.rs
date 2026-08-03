//! Dedicated parser for LinkedIn's own "Save to PDF" profile export.
//!
//! LinkedIn's export is structurally different enough from the resumes
//! `pdf_import::parse_cv` was built and tuned against — this app's own
//! single-column renderer output, and human-authored resumes generally —
//! that reusing the same heuristics does more harm than good:
//!
//! - The raw text stream puts a sidebar column (Contact / Top Skills /
//!   Languages / Certifications) *before* the person's own name, because
//!   that's simply draw order in the source PDF, not reading order.
//! - A company with N years of tenure gets its own standalone "N years M
//!   months" line, distinct from the actual per-role date range.
//! - Job dates are written as "Month Year - Month Year (duration)" with
//!   no icon glyph and no " · " company/location separator — company,
//!   role, dates and location are each their own physical line.
//! - Every page carries a "Page N of M" footer that lands wherever the
//!   text stream happens to be mid-page, frequently splitting a bullet
//!   in two.
//!
//! See `is_linkedin_export` for how this format is detected, and
//! `parse_linkedin_cv` for the entry point.

use crate::models::*;
use crate::services::pdf_import::{
    extract_email, extract_phone, extract_urls, is_project_header, parse_certifications,
    parse_languages, parse_skills,
};

/// LinkedIn's PDF export always starts with a "Contact" sidebar heading
/// and, further down, a "Top Skills" sidebar heading — a combination
/// specific enough to this one export format that no other resume/CV
/// template we've seen produces both. Used to route `import_pdf` here
/// instead of the generic `pdf_import::parse_cv`.
pub fn is_linkedin_export(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines.first() == Some(&"Contact") && lines.iter().any(|l| l.eq_ignore_ascii_case("top skills"))
}

/// Main entry point: parse a LinkedIn "Save to PDF" export's extracted
/// text into a LifetimeCV.
pub fn parse_linkedin_cv(text: &str) -> LifetimeCV {
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    lines = strip_page_footers(lines);
    rejoin_wrapped_contact_lines(&mut lines);

    let mut cv = LifetimeCV::default();

    // Locate each known LinkedIn section header by exact (case-insensitive)
    // line match — LinkedIn's own sidebar/body headings are a small, fixed
    // vocabulary, unlike the free-form heading text `detect_section`
    // otherwise has to guess at.
    let find = |keyword: &str| lines.iter().position(|l| l.eq_ignore_ascii_case(keyword));
    let mut headers: Vec<(usize, &'static str)> = Vec::new();
    if let Some(i) = find("Contact") {
        headers.push((i, "contact"));
    }
    if let Some(i) = find("Top Skills") {
        headers.push((i, "top_skills"));
    }
    if let Some(i) = find("Languages") {
        headers.push((i, "languages"));
    }
    if let Some(i) = find("Certifications") {
        headers.push((i, "certifications"));
    }
    let summary_idx = find("Summary");
    if let Some(i) = summary_idx {
        headers.push((i, "summary"));
    }
    let experience_idx = find("Experience");
    if let Some(i) = experience_idx {
        headers.push((i, "experience"));
    }
    if let Some(i) = find("Education") {
        headers.push((i, "education"));
    }
    headers.sort_by_key(|(i, _)| *i);

    let section_body = |label: &str| -> Vec<String> {
        match headers.iter().position(|(_, l)| *l == label) {
            Some(pos) => {
                let start = headers[pos].0 + 1;
                let end = headers.get(pos + 1).map(|(i, _)| *i).unwrap_or(lines.len());
                if start < end {
                    lines[start..end].to_vec()
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        }
    };

    // The Name/Title/Location block always sits immediately before
    // Summary (or Experience, if there's no Summary section) — LinkedIn
    // always renders it there, right after whatever sidebar sections the
    // person has. It has no header of its own, so nothing above ever
    // isolates it as its own section; `extract_name_title_location`
    // finds it directly by walking backward from that boundary.
    let main_header_idx = summary_idx.or(experience_idx);
    let mut consumed = 0usize;
    if let Some(idx) = main_header_idx {
        let (name, title, location, n) = extract_name_title_location(&lines, idx);
        cv.personal.name = name;
        cv.personal.title = LocalizedText::same(title);
        cv.personal.location = location;
        consumed = n;
    }
    // Whichever recognized header comes right before the Name/Title/
    // Location block owns a section body that (per `section_body`,
    // which only knows about *header* boundaries) runs a few lines too
    // long — it unknowingly swallowed the personal-info block too. Trim
    // those trailing lines back off before parsing that section for
    // real content.
    let prev_label = main_header_idx.and_then(|idx| {
        headers
            .iter()
            .rev()
            .find(|(i, _)| *i < idx)
            .map(|(_, l)| *l)
    });
    let strip_name_block = |mut v: Vec<String>, label: &str| -> Vec<String> {
        if prev_label == Some(label) {
            let new_len = v.len().saturating_sub(consumed);
            v.truncate(new_len);
        }
        v
    };

    let contact_lines = strip_name_block(section_body("contact"), "contact");
    let contact_text = contact_lines.join(" ");
    if let Some(email) = extract_email(&contact_text) {
        cv.personal.email = email;
    }
    if let Some(phone) = extract_phone(&contact_text) {
        cv.personal.phone = phone;
    }
    let (linkedin, github, website) = extract_urls(&contact_text);
    if let Some(li) = linkedin {
        cv.personal.linkedin = li;
    }
    if let Some(gh) = github {
        cv.personal.github = gh;
    }
    if let Some(web) = website {
        cv.personal.website = web;
    }

    cv.skills = parse_skills(&strip_name_block(section_body("top_skills"), "top_skills"));
    cv.languages = parse_languages(&strip_name_block(section_body("languages"), "languages"));
    cv.certifications = parse_certifications(&strip_name_block(
        section_body("certifications"),
        "certifications",
    ));

    cv.experiences = parse_linkedin_experiences(&section_body("experience"));
    cv.education = parse_linkedin_education(&section_body("education"));

    cv
}

/// LinkedIn's PDF export prints a "Page N of M" footer on every page,
/// and because the text stream reflects draw order rather than visual
/// layout, that footer can land in the MIDDLE of a wrapped bullet,
/// splitting it in two (e.g. a bullet's last two lines separated by a
/// full "Page 3 of 14"). It always appears as four consecutive lines —
/// "Page", the page number, "of", the page count — so it's safe to strip
/// as a fixed 4-line pattern wherever it occurs, rather than only at
/// section boundaries.
fn strip_page_footers(lines: Vec<String>) -> Vec<String> {
    let is_number = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        if lines[i] == "Page"
            && lines.get(i + 1).is_some_and(|l| is_number(l))
            && lines.get(i + 2).map(String::as_str) == Some("of")
            && lines.get(i + 3).is_some_and(|l| is_number(l))
        {
            i += 4;
            continue;
        }
        out.push(lines[i].clone());
        i += 1;
    }
    out
}

/// LinkedIn wraps the Contact block to a narrow sidebar column, which can
/// break a single "word" (an email address or profile URL) across two
/// lines. Two shapes show up in practice: a genuine wrap-hyphen
/// ("...tk-vincent-" / "nguyen (LinkedIn)") and a bare mid-word break
/// with no hyphen at all ("...nguyen@gmai" / "l.com"). Both would
/// otherwise leave `extract_email`/`extract_urls` looking at two
/// separate, useless tokens instead of one real one. Deliberately
/// narrow — only scans the first handful of lines (the Contact block) —
/// so it can't misfire on a genuine hyphenated compound word or an
/// em/en-dash bullet elsewhere in the document.
fn rejoin_wrapped_contact_lines(lines: &mut Vec<String>) {
    let mut i = 0;
    while i + 1 < lines.len() && i < 12 {
        let cur = lines[i].clone();
        let next = lines[i + 1].clone();

        let merged = if let Some(stripped) = cur.strip_suffix('-') {
            // Genuine wrap-hyphen: the next line continues the word in
            // lowercase, with no space intended before it.
            if next.chars().next().is_some_and(|c| c.is_lowercase()) {
                Some(format!("{stripped}{next}"))
            } else {
                None
            }
        } else if let Some(at_pos) = cur.rfind('@') {
            // Bare mid-word break inside an email domain, no hyphen at
            // all: the text after the last "@" has no "." yet, so the
            // email can't be complete on this line, and the next line is
            // short/URL-safe enough to plausibly be the rest of it.
            let domain_so_far = &cur[at_pos..];
            let looks_incomplete = !domain_so_far.contains('.');
            let next_continues = !next.is_empty()
                && next.chars().count() <= 15
                && next
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '.' || c == '-');
            if looks_incomplete && next_continues {
                Some(format!("{cur}{next}"))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(merged) = merged {
            lines[i] = merged;
            lines.remove(i + 1);
            continue; // re-check the same position for further breaks
        }
        i += 1;
    }
}

/// True for a short, plain line shaped like a person's full name: 2 to 5
/// space-separated words, each starting with an uppercase letter, no
/// digits/`@`/`:`/`(` anywhere on the line. Deliberately permissive
/// about accented and hyphenated names (e.g. "Élise Dupont-Martin").
fn looks_like_person_name(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() < 2 || words.len() > 5 {
        return false;
    }
    if line
        .chars()
        .any(|c| c.is_ascii_digit() || c == '@' || c == ':' || c == '(')
    {
        return false;
    }
    words
        .iter()
        .all(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
}

/// Finds the Name/Title/Location block that LinkedIn always renders
/// immediately before `header_idx` (the index of the "Summary" or
/// "Experience" line). Walks backward collecting up to 3 non-empty
/// lines, then sanity-checks that the first of them actually looks like
/// a person's name (see `looks_like_person_name`) before trusting any of
/// it — if it doesn't, this returns empty strings rather than risking
/// mislabeling a stray certification/sidebar line as someone's name.
/// Returns `(name, title, location, lines_consumed)`; the caller needs
/// `lines_consumed` to trim those lines back out of whichever section
/// they'd otherwise still be counted as part of.
fn extract_name_title_location(
    lines: &[String],
    header_idx: usize,
) -> (String, String, String, usize) {
    let mut start = header_idx;
    let mut block: Vec<String> = Vec::new();
    while block.len() < 3 && start > 0 {
        start -= 1;
        block.push(lines[start].clone());
    }
    block.reverse();

    if block.is_empty() || !looks_like_person_name(&block[0]) {
        return (String::new(), String::new(), String::new(), 0);
    }

    let name = block[0].clone();
    let title = block.get(1).cloned().unwrap_or_default();
    let location = block.get(2).cloned().unwrap_or_default();
    (name, title, location, block.len())
}

/// Month names used to confirm a token actually looks like the start of
/// a date, not just any capitalized word.
const LINKEDIN_MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn starts_with_month(s: &str) -> bool {
    let first_word = s.split_whitespace().next().unwrap_or("").to_lowercase();
    LINKEDIN_MONTHS.contains(&first_word.as_str())
}

/// True for a standalone "N year(s) M month(s)" (or just years, or just
/// months) line — LinkedIn's own summary of a company's *total* tenure
/// across however many roles the person held there. It carries no actual
/// date (no month/year), only a duration, so it's purely informational
/// and safe to skip: the per-role date-range line that follows it
/// carries the real start/end dates.
fn is_duration_only_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 4 {
        return false;
    }
    let mut i = 0;
    let mut saw_unit = false;
    while i < tokens.len() {
        if !tokens[i].chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        i += 1;
        match tokens.get(i) {
            Some(&"year") | Some(&"years") | Some(&"month") | Some(&"months") => {
                saw_unit = true;
                i += 1;
            }
            _ => return false,
        }
    }
    saw_unit
}

/// Parses a LinkedIn experience/project date-range line, shaped like
/// "<Month> <Year> to <Month> <Year> (<duration>)" or "... to Present
/// (<duration>)" but written with a dash instead of "to". That dash is
/// often padded with non-breaking spaces (`\u{a0}-\u{a0}`) rather than
/// plain ASCII spaces, so this normalizes those first instead of relying
/// on a fixed separator string.
fn parse_linkedin_date_line(line: &str) -> Option<(String, String)> {
    let normalized = line.replace('\u{a0}', " ");
    let before_paren = normalized.split('(').next().unwrap_or(&normalized).trim();
    for sep in ['-', '–', '—'] {
        if let Some(pos) = before_paren.find(sep) {
            let start = before_paren[..pos].trim();
            let end = before_paren[pos + sep.len_utf8()..].trim();
            if !start.is_empty()
                && !end.is_empty()
                && starts_with_month(start)
                && (starts_with_month(end) || end.eq_ignore_ascii_case("present"))
            {
                return Some((start.to_string(), end.to_string()));
            }
        }
    }
    None
}

/// True for a line that opens with a bullet marker LinkedIn's export
/// itself uses.
fn is_bullet_start(line: &str) -> bool {
    line.starts_with(['–', '•', '-', '*'])
}

/// A LinkedIn job header: `Company`, then optionally a total-tenure
/// duration line (see `is_duration_only_line`), then `Role`, then a
/// date-range line, then optionally a `Location` line.
struct LinkedinJobHeader {
    company: String,
    role: String,
    start_date: String,
    end_date: String,
    location: String,
    /// Index of the first line of this role's own narrative content
    /// (bullets / "Project N:" sub-entries), i.e. right after the header.
    body_start: usize,
}

/// Attempts to parse a job header starting exactly at `lines[i]`. Returns
/// `None` if `lines[i]` doesn't actually start a new job header — i.e.
/// it's just more of the previous role's narrative — which is the signal
/// `parse_linkedin_experiences` uses both to find where each role starts
/// and, by trying this again at every later index, where its body ends.
fn try_parse_job_header(lines: &[String], i: usize) -> Option<LinkedinJobHeader> {
    let company = lines.get(i)?.clone();
    if company.is_empty() || is_bullet_start(&company) || is_project_header(&company) {
        return None;
    }

    let mut j = i + 1;
    if lines.get(j).is_some_and(|l| is_duration_only_line(l)) {
        j += 1;
    }
    let role = lines.get(j)?.clone();
    if role.is_empty() || is_bullet_start(&role) || is_project_header(&role) {
        return None;
    }
    let date_line = lines.get(j + 1)?;
    let (start_date, end_date) = parse_linkedin_date_line(date_line)?;

    let mut body_start = j + 2;
    let mut location = String::new();
    if let Some(loc_line) = lines.get(body_start) {
        if !is_bullet_start(loc_line)
            && !is_project_header(loc_line)
            && !is_duration_only_line(loc_line)
            && parse_linkedin_date_line(loc_line).is_none()
        {
            location = loc_line.clone();
            body_start += 1;
        }
    }

    Some(LinkedinJobHeader {
        company,
        role,
        start_date,
        end_date,
        location,
        body_start,
    })
}

/// True for a line with no alphanumeric characters at all — the leftover
/// shell of a context label (e.g. "Situation & Enjeux :") whose own words
/// were dropped by the source PDF's font-encoding issue (see
/// `pdf_import::ToUnicodeMap`), leaving only punctuation like ":" or
/// "& :" behind. There's no label text left to recover, so these are
/// dropped rather than kept as empty/junk bullets.
fn is_artifact_only_line(line: &str) -> bool {
    !line.chars().any(|c| c.is_alphanumeric())
}

/// Parses everything between one job header and the next (see
/// `try_parse_job_header`) into that role's own default bullets plus any
/// "Project N: ..." sub-entries, each with their own optional date range
/// and bullets — the same shape `pdf_import::parse_experiences` produces
/// for this app's own renderer format, via `is_project_header`.
fn parse_linkedin_role_body(lines: &[String]) -> (Vec<LocalizedText>, Vec<ExperienceProject>) {
    let mut default_bullets: Vec<LocalizedText> = Vec::new();
    let mut sub_projects: Vec<ExperienceProject> = Vec::new();
    let mut current: Option<ExperienceProject> = None;
    let mut bullet_buf: Option<String> = None;

    fn flush_bullet(
        buf: &mut Option<String>,
        default_bullets: &mut Vec<LocalizedText>,
        current: &mut Option<ExperienceProject>,
    ) {
        if let Some(text) = buf.take() {
            let text = text.trim().to_string();
            if !text.is_empty() {
                match current {
                    Some(proj) => proj.bullets.push(LocalizedText::same(text)),
                    None => default_bullets.push(LocalizedText::same(text)),
                }
            }
        }
    }

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_artifact_only_line(trimmed) {
            continue;
        }

        if is_project_header(trimmed) {
            flush_bullet(&mut bullet_buf, &mut default_bullets, &mut current);
            if let Some(proj) = current.take() {
                sub_projects.push(proj);
            }
            current = Some(ExperienceProject {
                name: LocalizedText::same(trimmed.to_string()),
                ..Default::default()
            });
            continue;
        }

        if let Some((start, end)) = parse_linkedin_date_line(trimmed) {
            // Only a fresh sub-project (no bullets/dates captured yet)
            // can legitimately have its own separate date range right
            // after its header line; a bare date line anywhere else is
            // unexpected and dropped rather than guessed at.
            if let Some(proj) = current.as_mut() {
                if proj.bullets.is_empty() && proj.start_date.is_empty() {
                    proj.start_date = start;
                    proj.end_date = end;
                }
            }
            continue;
        }

        if let Some(bullet_text) = trimmed
            .strip_prefix('–')
            .or_else(|| trimmed.strip_prefix('-'))
        {
            flush_bullet(&mut bullet_buf, &mut default_bullets, &mut current);
            bullet_buf = Some(bullet_text.trim().to_string());
            continue;
        }

        // A wrapped continuation of the currently-open bullet.
        if let Some(buf) = bullet_buf.as_mut() {
            buf.push(' ');
            buf.push_str(trimmed);
        }
    }

    flush_bullet(&mut bullet_buf, &mut default_bullets, &mut current);
    if let Some(proj) = current.take() {
        sub_projects.push(proj);
    }
    (default_bullets, sub_projects)
}

/// Parses a LinkedIn Experience section's lines into `Experience` entries
/// by repeatedly finding the next job header (see `try_parse_job_header`)
/// and consuming everything up to the following one as that role's body.
fn parse_linkedin_experiences(lines: &[String]) -> Vec<Experience> {
    let mut experiences = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(header) = try_parse_job_header(lines, i) else {
            i += 1;
            continue;
        };

        let mut end = lines.len();
        let mut k = header.body_start;
        while k < lines.len() {
            if try_parse_job_header(lines, k).is_some() {
                end = k;
                break;
            }
            k += 1;
        }

        let body = &lines[header.body_start..end];
        let (default_bullets, sub_projects) = parse_linkedin_role_body(body);
        let mut projects = Vec::new();
        if !default_bullets.is_empty() {
            projects.push(ExperienceProject {
                bullets: default_bullets,
                ..Default::default()
            });
        }
        projects.extend(sub_projects);

        experiences.push(Experience {
            id: uuid::Uuid::new_v4().to_string(),
            company: header.company,
            role: LocalizedText::same(header.role),
            location: header.location,
            start_date: header.start_date,
            end_date: header.end_date,
            projects,
        });

        i = end;
    }
    experiences
}

/// Parses a LinkedIn Education section's lines into `Education` entries.
/// LinkedIn renders each entry as exactly two physical lines:
/// `Institution` then `Degree[, Field] · (Start - End)` — the field of
/// study and the date range are each optional (a self-taught or
/// currently-in-progress entry may omit either).
fn parse_linkedin_education(lines: &[String]) -> Vec<Education> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        let institution = lines[i].clone();
        let detail = lines[i + 1].replace('\u{a0}', " ");
        i += 2;

        let (degree_field, years) = match (detail.rfind('('), detail.rfind(')')) {
            (Some(open), Some(close)) if open < close => (
                detail[..open]
                    .trim()
                    .trim_end_matches('·')
                    .trim()
                    .to_string(),
                detail[open + 1..close].to_string(),
            ),
            _ => (detail.trim().to_string(), String::new()),
        };

        let (degree, field) = match degree_field.split_once(',') {
            Some((d, f)) => (d.trim().to_string(), f.trim().to_string()),
            None => (degree_field, String::new()),
        };

        let (start_year, end_year) = match years.split_once('-') {
            Some((s, e)) => (s.trim().to_string(), e.trim().to_string()),
            None => (String::new(), String::new()),
        };

        out.push(Education {
            id: uuid::Uuid::new_v4().to_string(),
            institution,
            degree: LocalizedText::same(degree),
            field: LocalizedText::same(field),
            start_year,
            end_year,
            achievements: Vec::new(),
        });
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal but structurally-faithful LinkedIn export text
    /// block, using placeholder data rather than any real person's
    /// details, so the fixture stays readable and obviously synthetic.
    fn sample_linkedin_text() -> String {
        [
            "Contact",
            "jane.smith@example.com",
            "www.linkedin.com/in/janesmith (LinkedIn)",
            "Top Skills",
            "Kubernetes",
            "Terraform",
            "Languages",
            "English (Native or Bilingual)",
            "Spanish (Professional Working)",
            "Certifications",
            "AWS Certified Solutions Architect (2021)",
            "Jane Smith",
            "Senior Platform Engineer",
            "Greater Boston Area",
            "Summary",
            "Experienced platform engineer focused on reliability.",
            "Experience",
            "Acme Corp",
            "1 year 3 months",
            "Platform Engineer",
            "December 2024 - February 2026 (1 year 3 months)",
            "Paris, France",
            "– Built GitOps pipelines across 7 environments using ArgoCD",
            "and Terraform.",
            "– Reduced incident response time by standardizing runbooks.",
            "Project 1: Platform Stabilization",
            "February 2025 - January 2026 (1 year)",
            "– Migrated legacy services onto Kubernetes.",
            "Beta Industries",
            "Site Reliability Engineer",
            "January 2022 - November 2024 (2 years 11 months)",
            "Remote",
            "– On-call rotation covering production incidents.",
            "Education",
            "State University",
            "Bachelor of Science, Computer Science · (2014 - 2018)",
        ]
        .join("\n")
    }

    #[test]
    fn detects_linkedin_export() {
        assert!(is_linkedin_export(&sample_linkedin_text()));
        assert!(!is_linkedin_export("Experience\nSoftware Engineer\nAcme"));
    }

    #[test]
    fn parses_personal_info() {
        let cv = parse_linkedin_cv(&sample_linkedin_text());
        assert_eq!(cv.personal.name, "Jane Smith");
        assert_eq!(cv.personal.title.en, "Senior Platform Engineer");
        assert_eq!(cv.personal.location, "Greater Boston Area");
        assert_eq!(cv.personal.email, "jane.smith@example.com");
        assert!(cv.personal.linkedin.contains("linkedin.com/in/janesmith"));
    }

    #[test]
    fn parses_top_skills_languages_certifications() {
        let cv = parse_linkedin_cv(&sample_linkedin_text());
        let skill_names: Vec<&str> = cv.skills.iter().map(|s| s.name.as_str()).collect();
        assert!(skill_names.contains(&"Kubernetes"));
        assert!(skill_names.contains(&"Terraform"));

        assert_eq!(cv.languages.len(), 2);
        assert_eq!(cv.languages[0].name, "English");

        assert_eq!(cv.certifications.len(), 1);
        assert!(cv.certifications[0]
            .name
            .contains("AWS Certified Solutions Architect"));
        // The Name/Title/Location block must NOT have leaked into
        // Certifications (the regression this module exists to fix).
        assert!(!cv.certifications[0].name.contains("Jane Smith"));
        assert!(!cv.certifications[0].issuer.contains("Jane Smith"));
    }

    #[test]
    fn parses_two_companies_with_project_subentry() {
        let cv = parse_linkedin_cv(&sample_linkedin_text());
        assert_eq!(cv.experiences.len(), 2);

        let acme = &cv.experiences[0];
        assert_eq!(acme.company, "Acme Corp");
        assert_eq!(acme.role.en, "Platform Engineer");
        assert_eq!(acme.start_date, "December 2024");
        assert_eq!(acme.end_date, "February 2026");
        assert_eq!(acme.location, "Paris, France");
        // One default bullet (merged across its wrapped continuation
        // line) plus one "Project 1" sub-entry.
        assert_eq!(acme.projects.len(), 2);
        let default_bullets: Vec<&str> = acme.projects[0]
            .bullets
            .iter()
            .map(|b| b.en.as_str())
            .collect();
        assert!(default_bullets
            .contains(&"Built GitOps pipelines across 7 environments using ArgoCD and Terraform."));
        assert_eq!(
            acme.projects[1].name.en,
            "Project 1: Platform Stabilization"
        );
        assert_eq!(acme.projects[1].start_date, "February 2025");
        assert_eq!(acme.projects[1].end_date, "January 2026");

        let beta = &cv.experiences[1];
        assert_eq!(beta.company, "Beta Industries");
        assert_eq!(beta.role.en, "Site Reliability Engineer");
        assert_eq!(beta.start_date, "January 2022");
        assert_eq!(beta.end_date, "November 2024");
        assert_eq!(beta.location, "Remote");
    }

    #[test]
    fn parses_education() {
        let cv = parse_linkedin_cv(&sample_linkedin_text());
        assert_eq!(cv.education.len(), 1);
        assert_eq!(cv.education[0].institution, "State University");
        assert_eq!(cv.education[0].degree.en, "Bachelor of Science");
        assert_eq!(cv.education[0].field.en, "Computer Science");
        assert_eq!(cv.education[0].start_year, "2014");
        assert_eq!(cv.education[0].end_year, "2018");
    }

    #[test]
    fn strips_page_footer_mid_bullet() {
        let lines: Vec<String> = [
            "– First half of a bullet that",
            "Page",
            "3",
            "of",
            "14",
            "gets split by a page footer.",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let stripped = strip_page_footers(lines);
        assert_eq!(
            stripped,
            vec![
                "– First half of a bullet that".to_string(),
                "gets split by a page footer.".to_string(),
            ]
        );
    }

    #[test]
    fn rejoins_hyphen_wrapped_url() {
        let mut lines: Vec<String> = ["Contact", "www.linkedin.com/in/jane-", "smith (LinkedIn)"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        rejoin_wrapped_contact_lines(&mut lines);
        assert_eq!(lines[1], "www.linkedin.com/in/janesmith (LinkedIn)");
    }

    #[test]
    fn rejoins_bare_split_email() {
        let mut lines: Vec<String> = ["Contact", "jane.smith@exam", "ple.com"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        rejoin_wrapped_contact_lines(&mut lines);
        assert_eq!(lines[1], "jane.smith@example.com");
    }

    #[test]
    fn company_with_no_total_duration_line_still_parses() {
        // LinkedIn omits the "N years M months" line entirely when a
        // company has only ever had one role.
        let text = [
            "Contact",
            "jane.smith@example.com",
            "Top Skills",
            "Kubernetes",
            "Languages",
            "English (Native or Bilingual)",
            "Certifications",
            "AWS Certified Solutions Architect (2021)",
            "Jane Smith",
            "Senior Platform Engineer",
            "Greater Boston Area",
            "Summary",
            "Bio.",
            "Experience",
            "Acme Corp",
            "Platform Engineer",
            "December 2024 - February 2026 (1 year 3 months)",
            "Paris, France",
            "– Did the thing.",
            "Education",
            "State University",
            "Bachelor of Science, Computer Science · (2014 - 2018)",
        ]
        .join("\n");
        let cv = parse_linkedin_cv(&text);
        assert_eq!(cv.experiences.len(), 1);
        assert_eq!(cv.experiences[0].company, "Acme Corp");
        assert_eq!(cv.experiences[0].start_date, "December 2024");
    }
}
