use crate::models::*;

mod dates;
mod experience;
mod harvest;
mod sections;
mod text_extraction;

#[cfg(test)]
mod regression_corpus;
#[cfg(test)]
mod tests;

// Internals shared across the parser tree. Submodules re-import these
// (plus this module's imports) with `use super::*`.
use dates::*;
use experience::*;
use harvest::*;
use sections::*;
#[allow(unused_imports)]
use text_extraction::*;

// Public API — crate-internal callers only (was `pub(crate)` in the
// single-file module).
pub(crate) use harvest::{parse_certifications, parse_languages, parse_skills};
pub(crate) use sections::{extract_email, extract_phone, extract_urls, is_project_header};
pub use text_extraction::extract_text;

/// Parse extracted text into a LifetimeCV.
pub fn parse_cv(text: &str) -> LifetimeCV {
    let lines: Vec<&str> = text.lines().collect();
    let sections =
        merge_duplicate_sections(reclaim_stray_experience_content(split_into_sections(text)));

    let mut cv = LifetimeCV::default();

    // Extract personal info from header (text before first section)
    let header_lines: Vec<&str> = sections
        .iter()
        .filter(|(s, _)| *s == "header")
        .flat_map(|(_, lines)| lines.iter().map(|s| s.as_str()))
        .collect();

    // Name
    if let Some(name) = guess_name(&header_lines) {
        cv.personal.name = name;
    }

    // Title
    if let Some(title) = guess_title(&lines) {
        cv.personal.title = LocalizedText::same(title);
    }

    // Contact info from header
    let header_text = header_lines.join(" ");
    if let Some(email) = extract_email(&header_text) {
        cv.personal.email = email;
    }
    if let Some(phone) = extract_phone(&header_text) {
        cv.personal.phone = phone;
    }
    let (linkedin, github, website) = extract_urls(&header_text);
    if let Some(li) = linkedin {
        cv.personal.linkedin = li;
    }
    if let Some(gh) = github {
        cv.personal.github = gh;
    }
    if let Some(web) = website {
        cv.personal.website = web;
    }

    // Skills harvested from sidebar tool/skill lines caught (and never
    // turned into bullets) while parsing Experience — see parse_experiences.
    let mut harvested_from_parsing: Vec<Skill> = Vec::new();

    // Process each section
    for (section, lines) in &sections {
        match *section {
            "experience" => {
                let (exps, harvested) = parse_experiences(lines);
                cv.experiences = exps;
                harvested_from_parsing = harvested;
            }
            "education" => {
                cv.education = parse_education(lines);
            }
            "skills" => {
                cv.skills = parse_skills(lines);
            }
            "projects" => {
                cv.projects = parse_projects(lines);
            }
            "certifications" => {
                cv.certifications = parse_certifications(lines);
            }
            "languages" => {
                cv.languages = parse_languages(lines);
            }
            _ => {}
        }
    }

    // Multi-column PDFs can interleave a sidebar's tool/skill list into the
    // Experience section's bullet stream, so some bullets are really
    // stray skill entries rather than accomplishments. Rather than try to
    // prevent this geometrically (attempted and found to be unreliable —
    // see decode_operations/run_operations history), detect it by content:
    // these bled-in lines have a distinctive, very specific shape ("<tool
    // name> N+ yrs" repeated) that a genuine accomplishment bullet
    // essentially never has. Most of this is now caught during
    // parse_experiences itself (harvested_from_parsing, above) before it
    // can ever corrupt role/company or context detection; this second pass
    // is a safety net for anything that still slipped through as a real
    // bullet. Either way, it turns a data-corruption bug into a bonus:
    // previously-missing detailed tool/skill entries (not just the
    // top-level category summary) end up in Skills.
    let harvested = harvested_from_parsing
        .into_iter()
        .chain(harvest_skills_from_experiences(&mut cv.experiences));
    for skill in harvested {
        if !cv
            .skills
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(&skill.name))
        {
            cv.skills.push(skill);
        }
    }

    resolve_project_skill_ids(&mut cv);

    cv
}

/// Resolves every `ExperienceProject.skill_ids` from the raw tool NAME
/// strings `flush_project` temporarily staged there (see its call site's
/// comment) into real `Skill.id`s, now that `cv.skills` is finalized.
///
/// Matching is case-insensitive exact-name only — no fuzzy matching, no
/// creating a new `Skill` for a name that doesn't already exist in
/// `cv.skills`. A parsed tool name with no match is simply dropped, not
/// kept as free text: this mirrors the editor's own "strict" tag picker,
/// which only ever lets you attach a project to a skill that's already in
/// `cv.skills` — so a freshly-imported CV and a manually-edited one end up
/// with the same invariant (every `skill_ids` entry is always a real,
/// resolvable skill), rather than import quietly being a looser, parallel
/// path that can produce data the editor itself could never create.
pub(super) fn resolve_project_skill_ids(cv: &mut LifetimeCV) {
    let skills = cv.skills.clone();
    for exp in &mut cv.experiences {
        for proj in &mut exp.projects {
            proj.skill_ids = proj
                .skill_ids
                .iter()
                .filter_map(|raw_name| {
                    skills
                        .iter()
                        .find(|s| s.name.eq_ignore_ascii_case(raw_name))
                        .map(|s| s.id.clone())
                })
                .collect();
        }
    }
}

/// Main entry point: extract text from PDF bytes and parse into a LifetimeCV.
///
/// LinkedIn's own "Save to PDF" profile export uses a structurally
/// different layout from the resumes `parse_cv`'s heuristics were built
/// and tuned against (this app's own renderer, and human-authored
/// single-column resumes generally) — a sidebar column that appears
/// *before* the person's own name in the raw text, multi-line job
/// headers with no icon glyph, and a standalone "N years M months"
/// tenure line. Rather than bend the generic parser to also cover that
/// very different shape (risking regressions in the extensively-tested
/// generic path), route to a dedicated parser — see
/// `linkedin_import::is_linkedin_export` for the detection fingerprint.
pub fn import_pdf(bytes: &[u8]) -> Result<LifetimeCV, String> {
    let text = extract_text(bytes)?;
    if text.trim().is_empty() {
        return Err("No text could be extracted from the PDF".to_string());
    }
    let cv = if crate::services::linkedin_import::is_linkedin_export(&text) {
        crate::services::linkedin_import::parse_linkedin_cv(&text)
    } else {
        parse_cv(&text)
    };
    Ok(cv)
}
