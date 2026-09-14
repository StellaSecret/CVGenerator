use super::*;

/// Keywords that signal an "institution" line (as opposed to a degree- or
/// field-of-study line) in an education entry.
pub(super) const INSTITUTION_KEYWORDS: &[&str] = &[
    "university",
    "université",
    "universite",
    "école",
    "ecole",
    "institut",
    "institute",
    "college",
    "collège",
    "faculty",
    "faculté",
    "faculte",
    "lycée",
    "lycee",
    // "IUT" (Institut Universitaire de Technologie) is the common French
    // abbreviation — sufficiently common and unambiguous as a school name
    // opener that it's checked separately below rather than folded into
    // this substring list (a bare 3-letter acronym is too easy to
    // false-positive on if matched anywhere in the line; see
    // `looks_like_institution_line`).
];

pub(super) fn looks_like_institution_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("iut ")
        || lower == "iut"
        || INSTITUTION_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Keywords that open a degree line ("Licence Professionnelle...", "BTS
/// Services...", "Master of Science...") — used to detect where a NEW
/// education entry starts even when nothing else (a date range) marks the
/// boundary. Some resumes list every degree with no dates at all, in which
/// case `parse_education`'s only other entry-boundary signals never fire,
/// and every line — across every degree — piles into a single buffer that
/// then gets mis-split as one garbled entry (see the call site).
pub(super) const DEGREE_KEYWORDS: &[&str] = &[
    "licence",
    "bachelor",
    "master",
    "mba",
    "bts",
    "dut",
    "phd",
    "ph.d",
    "doctorate",
    "doctorat",
    "baccalauréat",
    "baccalaureat",
    "diplôme",
    "diplome",
    "diploma",
    "magistère",
    "magistere",
    "associate degree",
    "certificat",
];

pub(super) fn looks_like_degree_line(line: &str) -> bool {
    let lower = line.trim().to_lowercase();
    DEGREE_KEYWORDS.iter().any(|kw| lower.starts_with(kw))
}

/// Build one Education entry from a buffer of plain lines that preceded a
/// date range. Layout: [degree line] [optional field-of-study line(s),
/// which may wrap] [institution line(s), which may also wrap across a
/// trailing city/country line]. The institution is identified by keyword
/// (e.g. "University"); everything between the degree and that point is the
/// field of study. If no institution keyword is found, the last line is
/// used as a fallback institution.
pub(super) fn build_education_from_buffer(
    buffer: &[String],
    start_year: String,
    end_year: String,
) -> Option<Education> {
    if buffer.is_empty() {
        return None;
    }
    // Institution-first layout: "University of X, Location" then a degree
    // line, then (elsewhere) the date — the reverse order from the
    // degree-first layout this function otherwise assumes. Identified by
    // content (an institution keyword), not position, since which comes
    // first varies by PDF/renderer. Delegate to the dedicated
    // institution-first builder rather than duplicating its degree/field
    // splitting here.
    if looks_like_institution_line(&buffer[0]) {
        return build_education_institution_first(
            buffer[0].clone(),
            start_year,
            end_year,
            &buffer[1..],
        );
    }
    // The degree line may itself embed the field of study on a single line,
    // e.g. "Bachelor of Science in Computer Science" or "Licence en Droit".
    let first = &buffer[0];
    let lower_first = first.to_lowercase();
    let (degree, embedded_field) = if let Some(pos) = first.find(" in ") {
        (
            first[..pos].trim().to_string(),
            Some(first[pos + 4..].trim().to_string()),
        )
    } else if let Some(pos) = lower_first.find(" en ") {
        (
            first[..pos].trim().to_string(),
            Some(first[pos + 4..].trim().to_string()),
        )
    } else {
        (first.clone(), None)
    };

    let rest = &buffer[1..];
    let inst_idx = rest.iter().position(|l| looks_like_institution_line(l));
    let (field_lines, institution_lines): (Vec<String>, Vec<String>) = match inst_idx {
        Some(idx) => {
            // Lines AFTER the institution line default to being folded
            // into `institution_lines` too, on the assumption that they're
            // a wrapped continuation of the institution's own name/address
            // (e.g. a trailing city/country). But a resume can just as
            // easily put the field-of-study line AFTER the institution
            // instead of before it (layouts vary) — recognizable because,
            // unlike an address continuation, it starts lowercase (a
            // specialization clause, e.g. this app's own French "option
            // Solutions ...", vs. a capitalized proper noun like "Boston,
            // MA"). Stop folding as soon as one of those appears, and
            // treat it — and everything after — as field instead. Without
            // this, a trailing field-after-institution line permanently
            // merged into the institution name (and the field itself came
            // out empty).
            let mut inst_end = idx + 1;
            while inst_end < rest.len()
                && rest[inst_end]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
            {
                inst_end += 1;
            }
            let mut field: Vec<String> = rest[..idx].to_vec();
            field.extend(rest[inst_end..].iter().cloned());
            (field, rest[idx..inst_end].to_vec())
        }
        None if rest.is_empty() => (Vec::new(), Vec::new()),
        None => (
            rest[..rest.len() - 1].to_vec(),
            rest[rest.len() - 1..].to_vec(),
        ),
    };

    let mut field_parts: Vec<String> = embedded_field
        .into_iter()
        .filter(|f| !f.is_empty())
        .collect();
    field_parts.extend(field_lines.iter().cloned());

    Some(Education {
        id: uuid::Uuid::new_v4().to_string(),
        institution: institution_lines.join(" ").trim().to_string(),
        degree: LocalizedText::same(degree),
        field: LocalizedText::same(field_parts.join(" ").trim()),
        start_year,
        end_year,
        ..Default::default()
    })
}

/// Build an Education entry for the "institution (+ dates) comes first,
/// degree/field line(s) follow" layout — the reverse order from
/// `build_education_from_buffer`, which expects the degree line first.
/// `trailing` is whatever plain lines were collected after the
/// institution+date row and before the next entry started.
pub(super) fn build_education_institution_first(
    institution: String,
    start_year: String,
    end_year: String,
    trailing: &[String],
) -> Option<Education> {
    if institution.is_empty() && trailing.is_empty() {
        return None;
    }
    let (degree, embedded_field) = match trailing.first() {
        Some(first) => {
            let lower_first = first.to_lowercase();
            if let Some(pos) = first.find(" · ") {
                (
                    first[..pos].trim().to_string(),
                    Some(first[pos + 3..].trim().to_string()),
                )
            } else if let Some(pos) = first.find(" in ") {
                (
                    first[..pos].trim().to_string(),
                    Some(first[pos + 4..].trim().to_string()),
                )
            } else if let Some(pos) = lower_first.find(" en ") {
                (
                    first[..pos].trim().to_string(),
                    Some(first[pos + 4..].trim().to_string()),
                )
            } else {
                (first.clone(), None)
            }
        }
        None => (String::new(), None),
    };
    let mut field_parts: Vec<String> = embedded_field
        .into_iter()
        .filter(|f| !f.is_empty())
        .collect();
    field_parts.extend(trailing.iter().skip(1).cloned());

    Some(Education {
        id: uuid::Uuid::new_v4().to_string(),
        institution: institution.trim_end_matches(['·', '|']).trim().to_string(),
        degree: LocalizedText::same(degree.trim_end_matches(['·', '|']).trim()),
        field: LocalizedText::same(field_parts.join(" ").trim()),
        start_year,
        end_year,
        ..Default::default()
    })
}

pub(super) fn parse_education(lines: &[String]) -> Vec<Education> {
    let lines = rejoin_fragmented_date_lines(lines);
    let lines = lines.as_slice();
    let mut educations = Vec::new();
    let mut buffer: Vec<String> = Vec::new();
    // Set when `extract_trailing_date_range_loose` matches an
    // "Institution, Location  Start – End" row: the institution/dates are
    // already known, but this layout's degree/field line(s) haven't been
    // seen yet — they're the plain lines that follow, collected into
    // `buffer` same as usual. Flushed via `build_education_institution_first`
    // (institution-first field order) rather than
    // `build_education_from_buffer` (degree-first) once the *next* entry
    // starts or the lines run out.
    let mut pending: Option<(String, String, String)> = None; // (institution, start, end)

    let flush_pending = |pending: &mut Option<(String, String, String)>,
                         buffer: &mut Vec<String>,
                         educations: &mut Vec<Education>| {
        if let Some((institution, start, end)) = pending.take() {
            if let Some(edu) = build_education_institution_first(institution, start, end, buffer) {
                educations.push(edu);
            }
            buffer.clear();
        }
    };

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((before, start, end)) = extract_trailing_date_range_loose(trimmed) {
            // A new institution+date row always starts a new entry — flush
            // whatever was pending (institution-first layout) or buffered
            // (degree-first layout, dates never found for it) first.
            flush_pending(&mut pending, &mut buffer, &mut educations);
            if !buffer.is_empty() {
                if let Some(edu) =
                    build_education_from_buffer(&buffer, String::new(), String::new())
                {
                    educations.push(edu);
                }
                buffer.clear();
            }
            pending = Some((before, start, end));
            continue;
        }

        if let Some((start, end)) = extract_standalone_date_range_loose(trimmed) {
            // Degree-first layout: buffer already holds [degree, field?,
            // institution]; this standalone date line completes it. Not
            // expected to coincide with a pending institution-first entry,
            // but flush that first too if it somehow does, so nothing is
            // silently dropped.
            flush_pending(&mut pending, &mut buffer, &mut educations);
            if let Some(edu) = build_education_from_buffer(&buffer, start, end) {
                educations.push(edu);
            }
            buffer.clear();
            continue;
        }

        // Year-only line like "2017" (no separator on this line at all,
        // e.g. start and end years printed on their own lines).
        if trimmed.len() == 4 && trimmed.chars().all(|c| c.is_ascii_digit()) {
            flush_pending(&mut pending, &mut buffer, &mut educations);
            if let Some(edu) =
                build_education_from_buffer(&buffer, trimmed.to_string(), String::new())
            {
                educations.push(edu);
            }
            buffer.clear();
            continue;
        }

        // A new degree line (e.g. "BTS Services Informatiques aux
        // Organisations (SIO)") starting while `buffer` already holds a
        // complete-looking prior entry (its own degree line, at
        // `buffer[0]`, plus at least one institution line further down) —
        // or symmetrically, a new INSTITUTION line starting while `buffer`
        // already holds an institution-first entry (institution at
        // `buffer[0]`, degree line further down; this app's own renderer
        // outputs institution before degree, the reverse of how the
        // source resume ordered them, so a round-2 reimport of our own
        // PDF hits this ordering even though round-1 hit the other one) —
        // means we've reached the NEXT entry with no date range ever
        // having marked the boundary — some resumes list every degree
        // with no dates at all. Flush what's pending as its own entry
        // first, rather than letting this line and everything after it
        // pile into the same buffer, where `build_education_from_buffer`
        // has no way to know two entries are in there and mis-splits the
        // lot into one garbled entry (wrong institution, wrong field,
        // duplicated separators on every re-render).
        let starts_new_degree_first_entry = !buffer.is_empty()
            && looks_like_degree_line(&buffer[0])
            && looks_like_degree_line(trimmed)
            && buffer[1..].iter().any(|l| looks_like_institution_line(l));
        let starts_new_institution_first_entry = !buffer.is_empty()
            && looks_like_institution_line(&buffer[0])
            && looks_like_institution_line(trimmed)
            && buffer[1..].iter().any(|l| looks_like_degree_line(l));
        if starts_new_degree_first_entry || starts_new_institution_first_entry {
            flush_pending(&mut pending, &mut buffer, &mut educations);
            if let Some(edu) = build_education_from_buffer(&buffer, String::new(), String::new()) {
                educations.push(edu);
            }
            buffer.clear();
        }

        buffer.push(trimmed.to_string());
    }

    // Trailing buffer/pending entry with no following date range: still
    // record it rather than silently dropping a final entry.
    if pending.is_some() {
        flush_pending(&mut pending, &mut buffer, &mut educations);
    } else if let Some(edu) = build_education_from_buffer(&buffer, String::new(), String::new()) {
        educations.push(edu);
    }

    educations
}

/// Parse skills section lines — typically comma-separated or one per line.
/// Category labels this app's own renderer prefixes a skills line with
/// (see `SkillCategory::label` in models/cv.rs and `render_skills` in
/// renderer.rs, which emits `"{label}: skill, skill, …"` per category).
/// Longest-first so e.g. "platforms & infrastructure" is tried before a
/// hypothetical shorter prefix that could partially match it.
///
/// Includes the pre-6-category label strings too (same rationale as
/// `SkillCategory`'s `#[serde(alias = ...)]`, see its doc comment): a PDF
/// exported before that migration still literally has "Framework:",
/// "Tool:", "Cloud & Infrastructure:", "Soft Skill:", "Other Skills:"
/// printed as section headers, and re-importing it should still recognize
/// those, mapped onto their closest surviving category, rather than
/// failing to categorize that skill at all.
pub(super) const SKILL_CATEGORY_LABELS: &[(&str, SkillCategory)] = &[
    (
        "platforms & infrastructure",
        SkillCategory::PlatformsInfrastructure,
    ),
    (
        "cloud & infrastructure", // pre-migration label
        SkillCategory::PlatformsInfrastructure,
    ),
    ("automation & devops", SkillCategory::AutomationDevOps),
    ("other skills", SkillCategory::AutomationDevOps), // pre-migration label
    ("programming", SkillCategory::Programming),
    ("soft skill", SkillCategory::Programming), // pre-migration label
    ("monitoring", SkillCategory::Monitoring),
    ("middleware", SkillCategory::Middleware),
    ("framework", SkillCategory::Programming), // pre-migration label
    ("database", SkillCategory::Database),
    ("tool", SkillCategory::AutomationDevOps), // pre-migration label
    (
        "collaboration & process",
        SkillCategory::CollaborationProcess,
    ),
];

pub(crate) fn parse_skills(lines: &[String]) -> Vec<Skill> {
    let mut skills = Vec::new();

    // Group the section's physical lines into per-category blocks — a new
    // block starts at a line beginning with a known category prefix (see
    // `SKILL_CATEGORY_LABELS`), or implicitly at the very first line.
    //
    // Within a block, decide how to treat line breaks:
    //   - If ANY line in the block contains a comma AND comma-splitting
    //     the joined block produces tag-shaped segments (short — see
    //     `MAX_TAG_WORDS` below), the whole block is a flowing
    //     comma-separated paragraph, exactly what this app's own
    //     renderer emits for a skills category (`render_skills` joins all
    //     of a category's skills into one "{label}: a, b, c" text run).
    //     Chromium then wraps that paragraph at arbitrary word
    //     boundaries when printing to PDF — including in the middle of a
    //     multi-word skill name, e.g. "Version Control" wraps as
    //     "Version" / "Control 5+ yrs". Splitting each physical line
    //     independently (the old behavior) turned that single skill into
    //     two ("Version" and "Control 5+ yrs"), and re-exporting then
    //     rendered a spurious comma between them that wasn't in the
    //     source — so here we rejoin the block into one string with
    //     spaces before splitting on commas.
    //   - If NO line in the block contains a comma, OR the block's
    //     content is full-sentence competency bullets rather than short
    //     tags (a sidebar of "Piloter la gestion des vulnérabilités
    //     (détection, analyse et remédiation)"-style bullet points, each
    //     wrapped across 2-3 physical lines, uses commas as ordinary
    //     prose punctuation *within* one bullet, not as separators
    //     *between* skills — comma-splitting that shreds one bullet into
    //     a dozen word-fragments, and joining the whole block with
    //     spaces first, as the paragraph case does, produces one
    //     enormous run-on "skill" spanning everything up to the first
    //     bullet that happens to contain a comma), each *logical* bullet
    //     — physical lines re-merged where the PDF wrapped one bullet
    //     across several rows, via `merge_wrapped_skill_lines` — becomes
    //     its own skill entry, comma and all.
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in lines {
        let lower = line.to_lowercase();
        let starts_new_block = blocks.is_empty()
            || SKILL_CATEGORY_LABELS
                .iter()
                .any(|(label, _)| lower.starts_with(&format!("{label}:")));
        if starts_new_block {
            blocks.push(vec![line.as_str()]);
        } else if let Some(block) = blocks.last_mut() {
            block.push(line.as_str());
        }
    }

    // A comma-split segment longer than this doesn't look like a skill
    // tag ("GitLab-CI 3+ yrs", "Version Control 5+ yrs") any more — it
    // looks like a fragment of a sentence. Real-world tag categories in
    // this app top out around 4-5 words; real competency-bullet
    // fragments run 10+ words even for the *shortest* fragment between
    // two commas, so there's a wide, safe margin between the two.
    const MAX_TAG_WORDS: usize = 8;

    for block in blocks {
        let joined = block.join(" ");
        let looks_like_tag_list = joined.contains(',')
            && joined
                .split(',')
                .all(|segment| segment.split_whitespace().count() <= MAX_TAG_WORDS);
        if looks_like_tag_list {
            parse_skill_line(&joined, &mut skills);
        } else if block.iter().any(|l| l.contains(',')) {
            for logical_line in merge_wrapped_skill_lines(&block) {
                push_whole_skill_line(&logical_line, &mut skills);
            }
        } else {
            for line in block {
                parse_skill_line(line, &mut skills);
            }
        }
    }
    skills
}

/// Re-merges a competency-bullet block's physical lines back into logical
/// bullets, undoing the PDF's mid-bullet line wrapping (see
/// `parse_skills`'s comment on why this block shape needs that instead of
/// comma-splitting). A physical line is treated as the wrapped
/// continuation of the previous one — not a new bullet — when either:
///   - it doesn't start with an uppercase letter (a genuine new bullet in
///     this style always opens with a capitalized word — an infinitive
///     verb in French resumes, a capitalized noun/acronym in English
///     ones — while a line broken mid-phrase continues in lowercase, or
///     with digits/punctuation, e.g. "cloud et" / "on-premise", "…
///     Connect," / "2FA)"); or
///   - the previous line ends with a trailing comma, which unambiguously
///     means the sentence isn't finished yet regardless of how the next
///     line starts — this also catches the rarer case of a wrap landing
///     right before an acronym, e.g. "… suivi de roadmap," / "OKR et
///     KPI", which the capitalization check alone would miss.
pub(super) fn merge_wrapped_skill_lines(lines: &[&str]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    for &line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let starts_uppercase = trimmed.chars().next().is_some_and(|c| c.is_uppercase());
        let prev_ends_with_comma = merged
            .last()
            .is_some_and(|prev: &String| prev.trim_end().ends_with(','));
        let is_continuation = !merged.is_empty() && (!starts_uppercase || prev_ends_with_comma);
        if is_continuation {
            if let Some(prev) = merged.last_mut() {
                prev.push(' ');
                prev.push_str(trimmed);
            }
        } else {
            merged.push(trimmed.to_string());
        }
    }
    merged
}

/// Parse one logical (already line-wrap-resolved) skills line/paragraph:
/// strip this app's own "{Category}: " prefix if present, split the rest
/// on comma and other common delimiters, and push each non-empty item as
/// a `Skill`. See `parse_skills` for how lines are grouped into blocks
/// before reaching here.
pub(super) fn parse_skill_line(line: &str, skills: &mut Vec<Skill>) {
    let (category, rest) = strip_skill_category_prefix(line);

    // Also split on common delimiters
    let mut normalized = rest.replace(" | ", ",");
    normalized = normalized.replace(" · ", ",");
    normalized = normalized.replace(" – ", ",");
    normalized = normalized.replace(" - ", ",");

    for item in normalized.split(',') {
        push_skill_entry(item, category.clone(), skills);
    }
}

/// Like `parse_skill_line`, but for one already-delimited logical bullet
/// from a competency-bullet block (see `parse_skills`'s comment) — adds it
/// as a single skill entry without also comma-splitting it, since here the
/// commas are ordinary sentence punctuation *within* the bullet, not
/// separators *between* skills. Still strips a leading category-label
/// prefix and a leading bullet marker, same as the comma-splitting path,
/// so a stray "Other Skills: " or "• " at the start of a bullet is handled
/// consistently either way.
pub(super) fn push_whole_skill_line(line: &str, skills: &mut Vec<Skill>) {
    let (category, rest) = strip_skill_category_prefix(line);
    push_skill_entry(rest, category, skills);
}

/// Strips this app's own "{Category}: " prefix, if present, and returns
/// the matched category alongside the remaining text — without this,
/// re-importing our own exported PDF bakes the category label into the
/// FIRST skill's name (e.g. "Automation & DevOps: CI/CD"), and the next
/// export prepends the category label again on top of that, compounding
/// into "Automation & DevOps: Automation & DevOps: …" a little further
/// with every import/export round trip.
pub(super) fn strip_skill_category_prefix(line: &str) -> (SkillCategory, &str) {
    let lower = line.to_lowercase();
    for (label, cat) in SKILL_CATEGORY_LABELS {
        let prefix = format!("{label}:");
        if lower.starts_with(&prefix) {
            return (cat.clone(), line[prefix.len()..].trim_start());
        }
    }
    (SkillCategory::default(), line)
}

pub(super) fn push_skill_entry(item: &str, category: SkillCategory, skills: &mut Vec<Skill>) {
    let trimmed = item.trim().trim_start_matches(['•', '·', '-']);
    let trimmed = trimmed.trim();
    if trimmed.is_empty() || trimmed.len() < 2 {
        return;
    }
    // Skip lines that look like headers
    let lower = trimmed.to_lowercase();
    if lower == "skills"
        || lower == "compétences"
        || lower == "technical skills"
        || lower == "compétences techniques"
    {
        return;
    }
    skills.push(Skill {
        id: uuid::Uuid::new_v4().to_string(),
        name: trimmed.to_string(),
        category,
        level: SkillLevel::Intermediate,
    });
}

/// Parse certifications section lines.
/// Parse certifications section lines.
///
/// A single certification's details are commonly spread across several
/// lines — name, year, issuing body, date range — the same
/// "several-lines-per-entry, ending in a standalone date range" layout as
/// Education. Before this fix, every line became its own bogus
/// Certification entry (e.g. one real "ITIL: Foundation certification
/// (2011), PeopleCert, Aug 2018 – No Expiration Date" turned into 4 separate
/// nonsensical entries).
pub(crate) fn parse_certifications(lines: &[String]) -> Vec<Certification> {
    let mut certs = Vec::new();
    let mut buffer: Vec<String> = Vec::new();

    // Re-join physical PDF lines that are really one wrapped "·"-joined
    // logical line before splitting on "·" below. When this app's own
    // render output joins several certifications into one "A · B · C · ..."
    // line (see the flatten step's own comment) and that line is long
    // enough to wrap in the PDF, the wrap point becomes an ordinary space
    // between two words with no "·" of its own — e.g. "...Kubernetes ·
    // Opérer" / "Kubernetes · Cisco..." wrapping mid-name, splitting
    // "Opérer Kubernetes" in two. Naively treating each physical line
    // independently then re-joins the pieces with a *spurious* "·" that
    // was never in the source text, permanently splitting one
    // certification into two — and since the mis-split state renders with
    // an extra "·" of its own, re-importing again keeps compounding it.
    // Continuing to merge forward with a plain space for as long as we're
    // still inside a "·"-joined run (i.e. the accumulated line so far
    // already contains "·") reconstructs the original single line
    // regardless of where the PDF happened to wrap it.
    let mut rejoined: Vec<String> = Vec::new();
    for line in lines {
        if let Some(last) = rejoined.last_mut() {
            if (last as &String).contains(" · ") {
                last.push(' ');
                last.push_str(line.trim());
                continue;
            }
        }
        rejoined.push(line.trim().to_string());
    }

    // Flatten any line that already bundles multiple " · "-joined parts
    // (name, year, issuer, date range) into separate pseudo-lines first —
    // when a certification's full text is short enough to not wrap, this
    // app's own renderer output (and others using the same " · "
    // convention) can produce one single already-merged line rather than
    // one line per part, which the buffer-accumulation loop below can't
    // otherwise tell apart. Also drops empty/pure-punctuation segments
    // (e.g. a trailing ",") rather than treating them as real content.
    let flattened: Vec<String> = rejoined
        .iter()
        .flat_map(|line| {
            if line.contains(" · ") {
                line.split(" · ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| s.chars().filter(|c| c.is_alphanumeric()).count() >= 2)
                    .collect::<Vec<_>>()
            } else {
                vec![line.clone()]
            }
        })
        .collect();

    for line in &flattened {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((start, end)) = extract_standalone_date_range_loose(trimmed) {
            if let Some(cert) = build_certification_from_buffer(&buffer, Some((start, end))) {
                certs.push(cert);
            }
            buffer.clear();
            continue;
        }

        buffer.push(trimmed.to_string());
    }

    if certs.is_empty() && buffer.len() > 2 {
        // No date range was found anywhere in this section, so nothing
        // ever signaled where one certification ends and the next
        // begins — every line piled into this one `buffer`, which used to
        // become a single Certification with the first line as `name` and
        // everything else squashed into `issuer` via " · " joins (e.g.
        // four separate Kubernetes trainings, each its own line in the
        // source resume, ending up as one certification named "Formation
        // Kubernetes" with the other three folded into its "issuer"
        // field). A resume that lists several certifications one per line
        // with no dates at all is common enough that "one per line" is a
        // far more useful — and far less garbled — default than "every
        // line in the section is secretly one record"; each line becomes
        // its own certification instead. (This only applies once the
        // *whole* section turns out to have no date-bounded entries at
        // all — `certs.is_empty()` — so a section that mixes dated and
        // dateless certifications is untouched, and a short 1–2 line
        // buffer, more likely a single name+issuer pair than two
        // unrelated certifications, still merges as before.)
        for line in &buffer {
            if let Some(cert) = build_certification_from_buffer(std::slice::from_ref(line), None) {
                certs.push(cert);
            }
        }
    } else if let Some(cert) = build_certification_from_buffer(&buffer, None) {
        certs.push(cert);
    }

    certs
}

/// Build one Certification from a buffer of plain lines (name, optionally a
/// bare year, optionally an issuing body) plus an optional trailing date
/// range. A bare 4-digit year gets folded onto the name in parentheses
/// (e.g. "ITIL: Foundation certification (2011)"). The issuer (if a second
/// buffer line is present) and the date range are kept in their own
/// fields — matching the model's separate `issuer`/`date` fields — rather
/// than joined into `name`. Joining them into `name` used to be harmless
/// on a first import, but re-exporting our own PDF unconditionally
/// appended "· {issuer}, {date}" again on top (see render_certifications),
/// so a name that already contained the issuer/date from a previous
/// import would end up with it twice, compounding by one more repetition
/// on every subsequent round trip.
pub(super) fn build_certification_from_buffer(
    buffer: &[String],
    date_range: Option<(String, String)>,
) -> Option<Certification> {
    if buffer.is_empty() {
        return None;
    }

    let mut name = buffer[0].clone();
    let mut issuer_parts: Vec<String> = Vec::new();
    for extra in &buffer[1..] {
        if extra.len() == 4 && extra.chars().all(|c| c.is_ascii_digit()) {
            name.push_str(&format!(" ({extra})"));
        } else {
            issuer_parts.push(extra.clone());
        }
    }
    let date = date_range
        .map(|(start, end)| format!("{start} – {end}"))
        .unwrap_or_default();

    Some(Certification {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        issuer: issuer_parts.join(" · "),
        date,
        ..Default::default()
    })
}

/// A line that's purely proficiency-dot decoration (e.g. "○ ○ ○ ○ ○"), with
/// no actual text. Some resume templates render language proficiency as a
/// row of dot/circle glyphs — filled vs. unfilled to show the level — but
/// that fill/unfill distinction is drawn as vector graphics (colored
/// shapes), not as distinguishable text characters, so it's invisible to
/// text extraction. Rather than fabricate a language entry out of these
/// decorative glyphs, we filter them out entirely.
pub(super) fn is_dots_only(s: &str) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c == '○' || c == '●' || c == '•' || c.is_whitespace())
}

/// Parse languages section lines.
/// True when `line` carries no explicit proficiency marker (no "Name -
/// Level", "Name (Level)", "Name, Level", or trailing rating dots) and the
/// following line reads like descriptive prose rather than another short
/// language entry. This is the shape of an "Interests" blurb that's
/// tacked onto a combined "Languages & Interests" heading (see
/// `SECTION_HEADERS`'s "langues et centres d'intérêt" mapping): a short
/// standalone heading word (e.g. "Musique") immediately followed by a
/// full sentence describing the hobby. A genuine bare-word language line
/// (some resumes just list "English" / "French" / "Vietnamese" with no
/// separator at all — see `parse_languages_filters_dot_only_lines`) is
/// never followed by that shape, since its neighbors are more short bare
/// words, not prose — so this only fires on the real boundary.
pub(super) fn looks_like_interest_heading(line: &str, next: Option<&String>) -> bool {
    let has_proficiency_marker =
        line.contains(" - ") || line.contains(" (") || line.contains(", ") || line.contains(':');
    if has_proficiency_marker {
        return false;
    }
    match next {
        Some(next) => {
            let next = next.trim();
            next.len() > 50 || next.matches(',').count() >= 2 || ends_with_terminal_punct(next)
        }
        None => false,
    }
}

pub(crate) fn parse_languages(lines: &[String]) -> Vec<Language> {
    let mut langs = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_dots_only(trimmed) {
            continue;
        }
        if !langs.is_empty() && looks_like_interest_heading(trimmed, lines.get(i + 1)) {
            // Everything from here on is the trailing "& Interests" half
            // of a combined section heading, not more languages — stop
            // rather than mis-parse a hobby blurb as a language entry.
            break;
        }
        // This importer's own renderer packs every language onto a
        // single line as repeated "Name (Level)" segments (e.g.
        // "Français (Native / Bilingual) Anglais (Conversational)") —
        // re-importing a CV this importer generated needs to split that
        // back apart into separate entries, or every language after the
        // first is silently dropped (only one name/level pair is ever
        // extracted per line below). Detect that shape — 2 or more
        // parenthesized groups on one line — and split on it first;
        // every other format this function handles (dash, colon, dot-
        // rating) has at most one paren group per line and falls
        // through unaffected.
        if trimmed.matches('(').count() >= 2 {
            for segment in split_paren_segments(trimmed) {
                if let Some(lang) = parse_single_language_entry(&segment) {
                    langs.push(lang);
                }
            }
            continue;
        }
        if let Some(lang) = parse_single_language_entry(trimmed) {
            langs.push(lang);
        }
    }
    langs
}

/// Splits a line into segments, breaking right after each balanced
/// "(...)" group closes — e.g. "Français (Native / Bilingual) Anglais
/// (Conversational)" becomes ["Français (Native / Bilingual)", "Anglais
/// (Conversational)"]. Any trailing text with no closing paren (not
/// expected for the renderer's own format, but kept defensively) forms
/// a final segment on its own.
pub(super) fn split_paren_segments(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut segments = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth <= 0 {
                    let seg: String = chars[start..=i].iter().collect();
                    let seg = seg.trim().to_string();
                    if !seg.is_empty() {
                        segments.push(seg);
                    }
                    start = i + 1;
                }
            }
            _ => {}
        }
    }
    let tail: String = chars[start..].iter().collect();
    let tail = tail.trim().to_string();
    if !tail.is_empty() {
        segments.push(tail);
    }
    segments
}

/// Parses one "Name - Level" / "Name (Level)" / "Name : Level" / bare
/// "Name" language entry. Shared by both the single-language-per-line
/// path and the multi-segment path above.
pub(super) fn parse_single_language_entry(trimmed: &str) -> Option<Language> {
    let lower = trimmed.to_lowercase();
    let level =
        if lower.contains("native") || lower.contains("bilingue") || lower.contains("maternelle") {
            LanguageLevel::Native
        } else if lower.contains("professional")
            || lower.contains("professionnel")
            || lower.contains("fluent")
            || lower.contains("courant")
        {
            LanguageLevel::Professional
        } else {
            LanguageLevel::Conversational
        };

    // Split "French - Native", "French (Native)", or the French
    // "Anglais : Technique" colon style — using whichever separator
    // occurs *earliest* in the line, not whichever is checked first.
    // A fixed priority order (checking " (" before " : ", say) picks
    // the wrong split point whenever a line has more than one kind of
    // separator, e.g. "Anglais : Technique (niveau B1, ...)" has both
    // " : " and " (" — checking " (" first would keep "Anglais :
    // Technique" as the name instead of just "Anglais".
    let name = [" - ", " (", " : ", ", "]
        .iter()
        .filter_map(|sep| trimmed.find(sep))
        .min()
        .map(|pos| trimmed[..pos].trim().to_string())
        .unwrap_or_else(|| trimmed.to_string());
    // Strip a trailing rating-dot cluster glued onto the same line as
    // the name (e.g. "English ○ ○ ○ ○ ○") — a proficiency-dial
    // rendered as text lands right after the name with no separator
    // `is_dots_only` (which only catches a dots-only *line*) can
    // recognize, so without this the dots end up baked into the
    // name itself.
    let name = name
        .trim_end_matches([' ', '○', '●', '•'])
        .trim()
        .to_string();

    if name.is_empty() {
        None
    } else {
        Some(Language {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            level,
        })
    }
}

/// Scan every bullet in every Experience/project for the "<tool> N+ yrs"
/// (repeated) pattern; remove matching bullets and return the harvested
/// entries as proper Skills.
pub(super) fn harvest_skills_from_experiences(experiences: &mut [Experience]) -> Vec<Skill> {
    let mut harvested = Vec::new();
    for exp in experiences.iter_mut() {
        for proj in exp.projects.iter_mut() {
            let mut kept = Vec::with_capacity(proj.bullets.len());
            for bullet in proj.bullets.drain(..) {
                if let Some(segments) = harvest_skill_segments(&bullet.en) {
                    for seg in segments {
                        harvested.push(Skill {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: seg,
                            category: SkillCategory::default(),
                            level: SkillLevel::Intermediate,
                        });
                    }
                } else {
                    kept.push(bullet);
                }
            }
            proj.bullets = kept;
        }
    }
    harvested
}

/// True if `line` is *only* a "<N>+yrs" years-experience marker with no
/// name attached — e.g. "2+yrs" (one fused token, exactly how
/// Input_Resume.pdf's own sidebar typesets it — verified directly against
/// its glyph positions, no space before "yrs") or "2+ yrs" (two tokens, in
/// case some other source resume spaces it out instead).
/// `harvest_skill_segments` requires the name and marker on the *same*
/// line — genuinely true for most sidebar tag rows, but not always: a name
/// that's left-aligned and a "N+yrs" badge that's right-aligned in a
/// fixed-width sidebar column can have glyph "top" coordinates that differ
/// by more than SAME_ROW_Y_EPSILON purely from sub-pixel baseline drift
/// between the two differently-positioned spans, even though they're the
/// same visual row (also verified directly — this is exactly what happens
/// to "Kustomize" / "2+yrs" in Input_Resume.pdf's TOOLS sidebar) — landing
/// the marker on its own separate reconstructed PDF line, split from its
/// name. This recognizes that marker-only line so the name immediately
/// before it (see the lookahead using this, above) can still be matched up
/// with it instead of neither ever being caught at all.
pub(super) fn is_bare_years_marker(line: &str) -> bool {
    let is_years_word = |t: &str| {
        let tl = t.to_lowercase();
        tl == "yrs" || tl == "yr" || tl == "years" || tl == "year"
    };
    let digits_plus = |t: &str| -> bool {
        t.ends_with('+') && t.len() > 1 && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit())
    };
    match line.split_whitespace().collect::<Vec<_>>().as_slice() {
        // Fused single token, e.g. "2+yrs".
        [one] => {
            let tl = one.to_lowercase();
            ["+yrs", "+years", "+yr", "+year"].iter().any(|suffix| {
                tl.strip_suffix(suffix)
                    .map(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false)
            })
        }
        // Two tokens, e.g. "2+" "yrs".
        [num, unit] => digits_plus(num) && is_years_word(unit),
        _ => false,
    }
}

/// If `token` ends with "<digits>+" (1 or 2 digits) with at least one
/// non-digit character immediately before those digits — e.g.
/// "Kustomize2+" — splits it into the name part and the marker part:
/// `("Kustomize", "2+")`. Returns None for a token that's *only* digits and
/// '+' (like plain "2+"): that's already handled directly as its own token
/// by harvest_skill_segments's marker scan, without needing a split.
///
/// This is a real, verified pattern — not a guess: Input_Resume.pdf's own
/// TOOLS sidebar genuinely typesets some entries this way (checked
/// directly against its glyph positions — "Kustomize" and "2+" share one
/// contiguous run with no space between them, while there IS a real space
/// before "yrs"). harvest_skill_segments's marker scan needs the "<N>+"
/// marker as its own whitespace-delimited token, so without this split
/// these entries — and the sentence they happen to land next to, since
/// they're stray sidebar bleed rather than a deliberate line break — never
/// get recognized as tool/skill entries at all.
///
/// Deliberately conservative about the digit run: only 1-2 digits count
/// (a realistic "years of experience" value). A longer run is far more
/// likely to be a genuine part of a product name/version — e.g. "iOS17+"
/// or "Log4j2023+" — so those are left untouched rather than risking a
/// false split.
pub(super) fn split_fused_name_and_marker(token: &str) -> Option<(&str, &str)> {
    let before_plus = token.strip_suffix('+')?;
    let digit_bytes = before_plus
        .bytes()
        .rev()
        .take_while(u8::is_ascii_digit)
        .count();
    if digit_bytes == 0 || digit_bytes > 2 {
        return None;
    }
    let digit_start = before_plus.len() - digit_bytes;
    if digit_start == 0 {
        return None; // the whole token before '+' is just digits — plain "2+"
    }
    // Safe to slice here: everything from digit_start to the end of `token`
    // is single-byte ASCII (the digit run plus the trailing '+'), so
    // digit_start can't land inside a multi-byte character.
    let name_part = &before_plus[..digit_start];
    if !name_part.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    Some((name_part, &token[digit_start..]))
}

/// Detect a line that's really a run of "<tool name> N+ yrs" segments (e.g.
/// "GitLab-CI 3+ yrs GitHub Actions 2+ yrs Jenkins 1+ yrs") rather than a
/// genuine accomplishment bullet, and split it into individual "<tool> N+
/// yrs" skill entries. Returns None if the line doesn't contain any such
/// "N+ yrs" marker at all.
pub(super) fn harvest_skill_segments(line: &str) -> Option<Vec<String>> {
    let mut tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    // Pre-split any token fusing a tool name directly onto its own "N+"
    // marker with no space at all (see split_fused_name_and_marker's doc
    // comment) into two tokens, so the marker scan below — which expects
    // the "<N>+" marker as its own token — still recognizes it.
    let mut i = 0;
    while i < tokens.len() {
        if let Some((name_part, marker_part)) = split_fused_name_and_marker(tokens[i]) {
            tokens.splice(i..=i, [name_part, marker_part]);
            i += 2;
        } else {
            i += 1;
        }
    }

    // A marker is either two tokens "3+" "yrs"/"years", or one token
    // "3+yrs"/"3+years". `marker_end` is the token index of the LAST token
    // of the marker; `marker_start` is the FIRST.
    let is_years_word = |t: &str| {
        let tl = t.to_lowercase();
        tl == "yrs" || tl == "yr" || tl == "years" || tl == "year"
    };
    let digits_plus = |t: &str| -> bool {
        t.ends_with('+') && t.len() > 1 && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit())
    };

    let mut markers: Vec<(usize, usize)> = Vec::new(); // (start_idx, end_idx) inclusive
    for i in 0..tokens.len() {
        if is_years_word(tokens[i]) && i > 0 && digits_plus(tokens[i - 1]) {
            markers.push((i - 1, i));
            continue;
        }
        let tl = tokens[i].to_lowercase();
        for suffix in ["+yrs", "+years", "+yr", "+year"] {
            if let Some(digits) = tl.strip_suffix(suffix) {
                if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                    markers.push((i, i));
                    break;
                }
            }
        }
    }

    if markers.is_empty() {
        return None;
    }

    let mut out = Vec::new();
    let mut start = 0;
    for (marker_start, marker_end) in markers {
        if marker_start < start {
            continue; // overlapping/malformed, skip defensively
        }
        let name = tokens[start..marker_start].join(" ").trim().to_string();
        let years = tokens[marker_start..=marker_end].join(" ");
        // The length cap is a safety net specifically for the fused-token
        // split above: it relies on stray sidebar bleed consistently
        // landing on its own short reconstructed PDF line (verified true
        // for every case examined so far — see split_fused_name_and_marker
        // and the skill-bleed handling above), never actually fused into a
        // long genuine sentence. If that assumption is ever wrong for some
        // other document, this stops it from swallowing a whole paragraph
        // as a bogus "skill name" instead of just failing to harvest that
        // one entry.
        if !name.is_empty() && name.len() <= 60 {
            out.push(format!("{name} {years}"));
        }
        start = marker_end + 1;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
