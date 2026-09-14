use super::*;
use lopdf::{Document, Object};

/// Regression test for the "glued text" bug: PDFs (commonly produced by
/// design tools like Canva/Figma) position each word/field as its own
/// `Tj` run via `Td`, with no literal space characters, and use `TJ`
/// kerning numbers instead of spaces between some runs. Before the fix,
/// text extraction concatenated everything with zero separation,
/// producing e.g. "TOOLSCI/CDGitLab-CI3+yrsGitHubActions2+yrs" — which is
/// exactly the kind of mangled text that ended up misparsed into the
/// wrong CV fields (Title, LinkedIn, GitHub, etc).
#[test]
fn run_operations_inserts_spaces_and_newlines_for_positioned_runs() {
    use lopdf::content::Operation;
    use lopdf::Dictionary;

    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), Object::Integer(10)],
        ),
        Operation::new("Td", vec![Object::Integer(50), Object::Integer(700)]),
        Operation::new("Tj", vec![Object::string_literal("TOOLS")]),
        Operation::new("Td", vec![Object::Integer(0), Object::Integer(-14)]), // new line
        Operation::new("Tj", vec![Object::string_literal("CI/CD")]),
        Operation::new("Td", vec![Object::Integer(40), Object::Integer(0)]), // same line, new run
        Operation::new("Tj", vec![Object::string_literal("GitLab-CI")]),
        Operation::new("Td", vec![Object::Integer(30), Object::Integer(0)]), // same line, new run
        Operation::new("Tj", vec![Object::string_literal("3+yrs")]),
        Operation::new("Td", vec![Object::Integer(-70), Object::Integer(-14)]), // new line
        Operation::new("Tj", vec![Object::string_literal("GitHub Actions")]),
        Operation::new(
            "TJ",
            vec![Object::Array(vec![
                Object::string_literal(""),
                Object::Integer(-250), // kerning gap standing in for a space
                Object::string_literal("2+yrs"),
            ])],
        ),
        Operation::new("ET", vec![]),
    ];

    let doc = Document::new();
    let resources = Dictionary::new();
    let encodings = std::collections::BTreeMap::new();
    let mut visited = Vec::new();
    let mut lines: Vec<PositionedLine> = Vec::new();
    run_operations(
        &doc,
        &ops,
        &resources,
        &encodings,
        Matrix::identity(),
        &mut visited,
        &mut lines,
    );
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();

    assert_eq!(
        texts,
        vec!["TOOLS", "CI/CD GitLab-CI 3+yrs", "GitHub Actions 2+yrs",]
    );
}

/// Regression test for the idempotence bug where a font's ligature
/// glyph ("ﬀ", U+FB00) — rendered as a single character position in
/// the PDF, but whose ToUnicode CMap decodes it to the 2-character
/// string "ff" — was mistaken for a multi-character *run*, tripping
/// the word-gap heuristic into inserting a bogus space on both
/// sides. This turned "offboarding" into "off boarding" every time
/// our own rendered PDF got re-imported. Chunk count for the
/// word-gap heuristic must come from the number of glyph *codes* in
/// the operand, not `chars().count()` of the decoded text.
#[test]
fn run_operations_ligature_glyph_does_not_insert_spurious_space() {
    use lopdf::content::Operation;
    use lopdf::{Dictionary, StringFormat};

    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), Object::Integer(10)],
        ),
        Operation::new("Td", vec![Object::Integer(50), Object::Integer(700)]),
        Operation::new("Tj", vec![Object::string_literal("o")]),
        Operation::new("Td", vec![Object::Integer(6), Object::Integer(0)]), // same line, contiguous
        // Single glyph (one byte code), but its ToUnicode CMap maps
        // it to a 2-character string, like a real "ﬀ" ligature glyph.
        Operation::new("Tj", vec![Object::String(vec![1u8], StringFormat::Literal)]),
        Operation::new("Td", vec![Object::Integer(7), Object::Integer(0)]), // same line, contiguous
        Operation::new("Tj", vec![Object::string_literal("boarding")]),
        Operation::new("ET", vec![]),
    ];

    let doc = Document::new();
    let resources = Dictionary::new();
    let mut encodings: std::collections::BTreeMap<Vec<u8>, ToUnicodeMap> =
        std::collections::BTreeMap::new();
    let mut map = std::collections::HashMap::new();
    map.insert(1u32, "ff".to_string());
    encodings.insert(b"F1".to_vec(), ToUnicodeMap { code_bytes: 1, map });
    let mut visited = Vec::new();
    let mut lines: Vec<PositionedLine> = Vec::new();
    run_operations(
        &doc,
        &ops,
        &resources,
        &encodings,
        Matrix::identity(),
        &mut visited,
        &mut lines,
    );
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();

    assert_eq!(texts, vec!["offboarding"]);
}

#[test]
fn extract_email_basic() {
    assert_eq!(
        extract_email("contact me at john@example.com please"),
        Some("john@example.com".to_string())
    );
}

#[test]
fn extract_email_angle_brackets() {
    assert_eq!(
        extract_email("<jane.doe@corp.fr>"),
        Some("jane.doe@corp.fr".to_string())
    );
}

#[test]
fn extract_phone_with_plus() {
    assert_eq!(
        extract_phone("call me at +33 6 12 34 56 78"),
        Some("+33612345678".to_string())
    );
}

#[test]
fn extract_phone_local() {
    assert_eq!(
        extract_phone("tel: 0612345678"),
        Some("0612345678".to_string())
    );
}

#[test]
fn extract_urls_linkedin() {
    let (li, gh, web) = extract_urls("https://linkedin.com/in/john https://github.com/john");
    assert_eq!(li, Some("https://linkedin.com/in/john".to_string()));
    assert_eq!(gh, Some("https://github.com/john".to_string()));
    assert!(web.is_none());
}

#[test]
fn guess_name_simple() {
    let lines = vec!["John Smith", "john@example.com", "+33 6 00 00 00"];
    assert_eq!(guess_name(&lines), Some("John Smith".to_string()));
}

#[test]
fn guess_name_skips_email_line() {
    let lines = vec!["john@example.com", "Jane Doe", "+33 6 00 00 00"];
    assert_eq!(guess_name(&lines), Some("Jane Doe".to_string()));
}

#[test]
fn guess_title_engineer() {
    let lines = vec!["John Smith", "Senior Rust Engineer", "john@example.com"];
    assert_eq!(
        guess_title(&lines),
        Some("Senior Rust Engineer".to_string())
    );
}

#[test]
fn detect_section_experience() {
    assert_eq!(detect_section("Experience"), Some("experience"));
    assert_eq!(detect_section("Work Experience"), Some("experience"));
    assert_eq!(
        detect_section("Expérience professionnelle"),
        Some("experience")
    );
}

#[test]
fn detect_section_education() {
    assert_eq!(detect_section("Education"), Some("education"));
    assert_eq!(detect_section("Formation"), Some("education"));
}

#[test]
fn detect_section_skills() {
    assert_eq!(detect_section("Skills"), Some("skills"));
    assert_eq!(detect_section("Compétences"), Some("skills"));
}

#[test]
fn extract_date_range_dash() {
    let result = extract_date_range("Jan 2021 - Present");
    assert_eq!(
        result,
        Some(("Jan 2021".to_string(), "Present".to_string()))
    );
}

#[test]
fn extract_date_range_en_dash() {
    let result = extract_date_range("Software Engineer · Acme Corp – 2020 – 2024");
    assert!(result.is_some());
    let (_, end) = result.unwrap();
    assert_eq!(end, "2024");
}

#[test]
fn parse_cv_full_sample() {
    let text = r#"
John Smith
Senior Rust Engineer
john@example.com
+33 6 12 34 56 78
linkedin.com/in/johnsmith
github.com/johnsmith

Experience
Software Engineer at Acme Corp - Jan 2021 - Present
• Built distributed systems using Rust
• Reduced latency by 40%

Junior Developer at Beta Ltd - Jun 2019 - Dec 2020
• Developed web applications with React

Education
MSc in Computer Science - MIT - 2017 - 2019
BSc in Computer Science - Stanford - 2013 - 2017

Skills
Rust, PostgreSQL, Kubernetes, React, TypeScript

Languages
French - Native
English - Professional
"#;
    let cv = parse_cv(text);
    assert_eq!(cv.personal.name, "John Smith");
    assert_eq!(cv.personal.email, "john@example.com");
    assert_eq!(cv.personal.phone, "+33612345678");
    assert!(cv.personal.linkedin.contains("linkedin.com"));
    assert!(cv.personal.github.contains("github.com"));
    assert!(!cv.experiences.is_empty());
    assert!(!cv.education.is_empty());
    assert!(!cv.skills.is_empty());
    assert!(!cv.languages.is_empty());
}

#[test]
fn parse_cv_minimal() {
    let text = "Jane Doe\nDeveloper\njane@test.com\n\nSkills\nRust, Python\n";
    let cv = parse_cv(text);
    assert_eq!(cv.personal.name, "Jane Doe");
    assert_eq!(cv.personal.email, "jane@test.com");
    assert_eq!(cv.skills.len(), 2);
}

#[test]
fn parse_skills_comma_separated() {
    let lines = vec!["Rust, Python, JavaScript, TypeScript".to_string()];
    let skills = parse_skills(&lines);
    assert_eq!(skills.len(), 4);
    assert_eq!(skills[0].name, "Rust");
}

/// Regression test for the idempotence bug where re-importing our own
/// rendered PDF split a two-word skill name into two bogus skills.
/// Our renderer emits a whole skills category as one flowing
/// comma-separated paragraph ("Other Skills: a, b, c, ..."), which
/// Chromium then wraps at arbitrary word boundaries when printing —
/// including in the middle of a compound skill name, so "Version
/// Control 5+ yrs" can land as "...Version" / "Control 5+ yrs...".
/// Splitting each physical line independently (the old behavior)
/// turned that into two separate skills, "Version" and "Control 5+
/// yrs", and re-exporting then rendered a comma between them that
/// was never in the source. Lines within one category block must be
/// rejoined before splitting on commas.
#[test]
fn parse_skills_rejoins_compound_skill_wrapped_across_lines() {
    let lines = vec![
        "Other Skills: CI/CD 4+ yrs, Secrets Management 2+ yrs, Version".to_string(),
        "Control 5+ yrs, Artifact Management 7+ yrs".to_string(),
    ];
    let skills = parse_skills(&lines);
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "CI/CD 4+ yrs",
            "Secrets Management 2+ yrs",
            "Version Control 5+ yrs",
            "Artifact Management 7+ yrs",
        ]
    );
}

/// Companion regression test: a human resume's one-skill-per-line
/// sidebar layout (no commas anywhere) must NOT be blindly merged
/// into one blob by the same rejoin logic — each line is still a
/// complete, standalone skill entry there.
#[test]
fn parse_skills_keeps_one_skill_per_line_when_no_commas_present() {
    let lines = vec![
        "CI/CD 4+ yrs".to_string(),
        "Infrastructure as Code 3+ yrs".to_string(),
        "Configuration Management 4+ yrs".to_string(),
    ];
    let skills = parse_skills(&lines);
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "CI/CD 4+ yrs",
            "Infrastructure as Code 3+ yrs",
            "Configuration Management 4+ yrs",
        ]
    );
}

#[test]
fn parse_languages_with_levels() {
    let lines = vec![
        "French - Native".to_string(),
        "English - Professional".to_string(),
        "Spanish - Conversational".to_string(),
    ];
    let langs = parse_languages(&lines);
    assert_eq!(langs.len(), 3);
    assert_eq!(langs[0].level, LanguageLevel::Native);
    assert_eq!(langs[1].level, LanguageLevel::Professional);
    assert_eq!(langs[2].level, LanguageLevel::Conversational);
}

/// Regression test: some resume templates render proficiency as a row of
/// dot/circle glyphs on their own line(s). Those must not become bogus
/// "language" entries.
#[test]
fn parse_languages_filters_dot_only_lines() {
    let lines = vec![
        "English".to_string(),
        "○ ○ ○ ○ ○".to_string(),
        "French".to_string(),
        "○ ○ ○ ○ ○".to_string(),
        "Vietnamese".to_string(),
        "○ ○ ○".to_string(),
        "○".to_string(),
    ];
    let langs = parse_languages(&lines);
    assert_eq!(
        langs.len(),
        3,
        "expected only the 3 real languages, got: {:?}",
        langs.iter().map(|l| &l.name).collect::<Vec<_>>()
    );
    assert_eq!(langs[0].name, "English");
    assert_eq!(langs[1].name, "French");
    assert_eq!(langs[2].name, "Vietnamese");
}

/// Regression test: a certification's name/year/issuer/date range spread
/// across several lines must become ONE Certification entry, not one
/// bogus entry per line — and issuer/date must land in their own
/// fields (not get joined into `name`), so re-exporting doesn't bolt
/// the same issuer/date onto the name a second time (see
/// render_certifications, which shows them separately from `name`).
#[test]
fn parse_certifications_merges_multi_line_entry() {
    let lines = vec![
        "ITIL: Foundation certification".to_string(),
        "2011".to_string(),
        "PeopleCert".to_string(),
        "\u{0011} Aug 2018 – No Expiration Date".to_string(),
    ];
    let certs = parse_certifications(&lines);
    assert_eq!(
        certs.len(),
        1,
        "expected exactly 1 merged certification, got: {:?}",
        certs.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    assert_eq!(certs[0].name, "ITIL: Foundation certification (2011)");
    assert_eq!(certs[0].issuer, "PeopleCert");
    assert_eq!(certs[0].date, "Aug 2018 – No Expiration Date");
}

/// Regression test for the "Techs: chip list gets eaten by an unrelated
/// bullet" bug. tools_row_html (renderer.rs) renders each chip as its
/// own flex item, which print-to-PDF fragments into one PDF line per
/// chip (plus one for the "Techs:" label, and one per separating
/// comma) — this simulates exactly that fragmentation, with an
/// unterminated (no trailing period) prior bullet immediately before
/// it, which is what triggers the wrapped-bullet-continuation merge
/// that was swallowing the whole list. Every tool name must survive as
/// its own entry in `tools`, and the unrelated prior bullet must come
/// through completely unmodified — not extended with any of this
/// content.
#[test]
fn parse_experiences_techs_chip_list_survives_fragmentation_after_open_bullet() {
    let lines = vec![
        "Some Role at Acme - Jan 2021 - Present".to_string(),
        "• Cloud AWS1+ yrs".to_string(), // unterminated — no trailing period
        "TECHS:".to_string(),
        "Openstack".to_string(),
        ",".to_string(),
        "Scaleway".to_string(),
        ",".to_string(),
        "Debian".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    // No "Project N:" header appears in `lines`, but flush_project()
    // always emits a (possibly unnamed) project to carry whatever
    // bullets/tools/context accumulated — bullets never land directly
    // on the Experience itself, only inside exps[i].projects[j].
    assert_eq!(exps[0].projects.len(), 1);
    let proj = &exps[0].projects[0];
    assert_eq!(
        proj.bullets.len(),
        1,
        "expected exactly the one original bullet, got: {:?}",
        proj.bullets.iter().map(|b| &b.en).collect::<Vec<_>>()
    );
    assert_eq!(
        proj.bullets[0].en, "Cloud AWS1+ yrs",
        "the unrelated prior bullet must not have absorbed any tools-list content"
    );
    assert_eq!(
        proj.skill_ids,
        vec!["Openstack", "Scaleway", "Debian"],
        "every tool name must survive as its own entry (staged in \
             skill_ids pre-resolution — see flush_project's comment), got: {:?}",
        proj.skill_ids
    );
}

#[test]
fn parse_experiences_with_bullets() {
    let lines = vec![
        "Software Engineer at Acme - Jan 2021 - Present".to_string(),
        "• Built APIs".to_string(),
        "• Reduced latency".to_string(),
        "Junior Dev at Beta - 2019 - 2020".to_string(),
        "• Made websites".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 2);
    assert_eq!(exps[0].role.en, "Software Engineer");
    assert_eq!(exps[0].company, "Acme");
    assert_eq!(exps[0].start_date, "Jan 2021");
    assert_eq!(exps[0].end_date, "Present");
    assert!(!exps[0].id.is_empty());
}

/// Regression test: layout (b) — "Company · Location - Start - End" on
/// one line with the role on its OWN following line. The location picked
/// up by `split_company_and_location` must survive onto the Experience
/// (covers the `location` field of the populated struct literal).
#[test]
fn parse_experiences_layout_b_location_from_company_line() {
    let lines = vec![
        "Acme Corp · Paris, France - Jan 2021 - Present".to_string(),
        "Software Engineer".to_string(),
        "• Built APIs".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0].role.en, "Software Engineer");
    assert_eq!(exps[0].company, "Acme Corp");
    assert_eq!(exps[0].location, "Paris, France");
    assert_eq!(exps[0].start_date, "Jan 2021");
    assert_eq!(exps[0].end_date, "Present");
}

/// Regression test: a common CV layout puts role, company, and dates on
/// three SEPARATE lines (rather than one "Role - Date - Date" line).
/// Before this fix, this pattern was never recognized at all and the
/// whole Experience section imported empty.
#[test]
fn parse_experiences_three_line_role_company_dates() {
    let lines = vec![
        "Platform Engineer (contractual)".to_string(),
        "DTNUM/SDAN/BFO".to_string(),
        "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
        "– Implemented GitOps deployment for the platform.".to_string(),
        "Site Reliability Engineer".to_string(),
        "DTNUM/SDAN/BFO".to_string(),
        "\u{0011} January 2024 – November 2024 ½ Paris, France".to_string(),
        "– Structured SRE practices.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 2);
    assert_eq!(exps[0].role.en, "Platform Engineer (contractual)");
    assert_eq!(exps[0].company, "DTNUM/SDAN/BFO");
    assert_eq!(exps[0].start_date, "December 2024");
    assert_eq!(exps[0].end_date, "February 2026");
    assert!(!exps[0].id.is_empty(), "experience id must be non-empty");
    assert_eq!(exps[1].role.en, "Site Reliability Engineer");
    assert_eq!(exps[1].company, "DTNUM/SDAN/BFO");
    assert!(!exps[1].id.is_empty(), "experience id must be non-empty");
}

/// Regression test: a "Project N: ..." sub-entry inside a job may have its
/// own standalone date range. That must NOT be mistaken for a new job —
/// it should stay attached to the same experience.
///
/// Also regression-tests: a bullet that doesn't end in terminal
/// punctuation (e.g. one ending in a version number, "* Kubernetes:
/// 1.20.2 → 1.23.8") must NOT swallow the next "Project N: ..." header
/// line as a continuation. If it does, that project's own date range
/// gets misattributed as a brand-new spurious job entry.
#[test]
fn project_header_after_non_terminal_bullet_does_not_spawn_spurious_job() {
    let lines = vec![
        "Site Reliability Engineer".to_string(),
        "Sirius".to_string(),
        "\u{0011} October 2022 – January 2024 ½ Bangkok, Thailand".to_string(),
        "Project 2: GitLab Administration".to_string(),
        "\u{0011} February 2023 – December 2023".to_string(),
        "– Migrated GitLab Runners with version".to_string(),
        "upgrades:".to_string(),
        "* Kubernetes: 1.20.2 → 1.23.8".to_string(),
        "Project 3: R&D – Cloud Migration".to_string(),
        "\u{0011} October 2022 – December 2023".to_string(),
        "– Automated migration of 274 nodes.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(
        exps.len(),
        1,
        "expected exactly one job, got: {:?}",
        exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
    );
    assert_eq!(exps[0].role.en, "Site Reliability Engineer");
    assert_eq!(exps[0].company, "Sirius");
}

/// Regression test: the standalone-date-range line ("Role\nCompany\nDates
/// Location") also carries a trailing location, which must end up on
/// the Experience — and, critically, a LATER job's date range must
/// still correctly start a NEW experience even though an EARLIER job
/// had its own "Project N:" sub-entries (this is exactly the bug fixed
/// alongside proper project-name attachment: naively checking "is a
/// project currently open" instead of "was the immediately preceding
/// line a project header" caused every job after the first one to be
/// silently swallowed).
#[test]
fn parse_experiences_captures_location_and_still_splits_later_jobs() {
    let lines = vec![
        "Platform Engineer".to_string(),
        "Acme Corp".to_string(),
        "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
        "Project 1: Migration".to_string(),
        "\u{0011} January 2025 – June 2025".to_string(),
        "– Did the migration.".to_string(),
        "Site Reliability Engineer".to_string(),
        "Globex".to_string(),
        "\u{0011} January 2020 – November 2024 ½ Bangkok, Thailand".to_string(),
        "– Kept things running.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(
        exps.len(),
        2,
        "expected both jobs, got: {:?}",
        exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
    );
    assert_eq!(exps[0].location, "Paris, France");
    assert_eq!(exps[1].role.en, "Site Reliability Engineer");
    assert_eq!(exps[1].company, "Globex");
    assert_eq!(exps[1].location, "Bangkok, Thailand");
}

/// Regression test: a "Project N: ..." header's name must actually be
/// attached to the ExperienceProject that gets its bullets, and
/// "Situation:"/"Tasks:"/etc. intro sentences must end up in the
/// project's own `context` field (not silently discarded, and not
/// mixed into `bullets`).
#[test]
fn parse_experiences_attaches_project_name_and_keeps_context() {
    let lines = vec![
        "Platform Engineer".to_string(),
        "Acme Corp".to_string(),
        "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
        "Project 1: Cloud Migration".to_string(),
        "\u{0011} January 2025 – June 2025".to_string(),
        "Situation: The team needed a cloud migration.".to_string(),
        "– Migrated 50 services to the cloud.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0].projects.len(), 1);
    assert_eq!(exps[0].projects[0].name.en, "Project 1: Cloud Migration");
    assert_eq!(
        exps[0].projects[0].context[0].en,
        "Situation: The team needed a cloud migration."
    );
    let bullet_texts: Vec<&str> = exps[0].projects[0]
        .bullets
        .iter()
        .map(|b| b.en.as_str())
        .collect();
    assert_eq!(bullet_texts, vec!["Migrated 50 services to the cloud."]);
}

/// Regression test: a wrapped multi-line "Situation & Tasks: ..." intro
/// paragraph must be merged into ONE coherent context sentence, not
/// left as several disconnected one-line fragments (each PDF line is a
/// visually-wrapped row, not a separate sentence).
#[test]
fn parse_experiences_merges_wrapped_context_paragraph() {
    let lines = vec![
        "Platform Engineer".to_string(),
        "Acme Corp".to_string(),
        "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
        "Situation & Tasks: As part of the DevOps/SRE transformation within the IT department, I"
            .to_string(),
        "joined the Socle Team of the Cloud π Native project while also acting as the".to_string(),
        "technical lead for the CITADEL platform.".to_string(),
        "Actions taken:".to_string(),
        "– Implemented GitOps deployment.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0].projects.len(), 1);
    assert_eq!(
            exps[0].projects[0].context[0].en,
            "Situation & Tasks: As part of the DevOps/SRE transformation within the IT department, I joined the Socle Team of the Cloud π Native project while also acting as the technical lead for the CITADEL platform."
        );
    let bullet_texts: Vec<&str> = exps[0].projects[0]
        .bullets
        .iter()
        .map(|b| b.en.as_str())
        .collect();
    assert_eq!(bullet_texts, vec!["Implemented GitOps deployment."]);
}

/// Regression test: a "Techs: A, B, C." line (possibly wrapped across
/// several PDF lines) must be parsed into project.skill_ids (staged as
/// raw names pre-resolution — see flush_project's comment) as
/// individual technology names, not left as raw context text.
#[test]
fn parse_experiences_extracts_tools_from_techs_line() {
    let lines = vec![
        "Platform Engineer".to_string(),
        "Acme Corp".to_string(),
        "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
        "– Migrated the platform.".to_string(),
        "Techs: Openstack, Scaleway, Debian, Kyverno, Keycloak, Vault, Redis, CNPG, Kubernetes,"
            .to_string(),
        "Openshift, Docker, Containerd.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0].projects.len(), 1);
    assert_eq!(
        exps[0].projects[0].skill_ids,
        vec![
            "Openstack",
            "Scaleway",
            "Debian",
            "Kyverno",
            "Keycloak",
            "Vault",
            "Redis",
            "CNPG",
            "Kubernetes",
            "Openshift",
            "Docker",
            "Containerd",
        ]
    );
}

/// Regression test: a sidebar tool/skill list line that bled into
/// Experience bullets (e.g. from a multi-column layout) must be
/// detected by its distinctive "<tool> N+ yrs" shape, stripped out of
/// the bullet list, and harvested as real Skill entries instead.
#[test]
fn harvest_skill_segments_splits_tool_year_pairs() {
    let segs =
        harvest_skill_segments("CI/CD GitLab-CI 3+ yrs GitHub Actions 2+ yrs Jenkins 1+ yrs");
    assert_eq!(
        segs,
        Some(vec![
            "CI/CD GitLab-CI 3+ yrs".to_string(),
            "GitHub Actions 2+ yrs".to_string(),
            "Jenkins 1+ yrs".to_string(),
        ])
    );

    // A genuine accomplishment bullet must not match at all.
    assert_eq!(
        harvest_skill_segments("Reduced incidents and improved platform stability via GitOps."),
        None
    );
}

#[test]
fn split_fused_name_and_marker_splits_letters_digits_plus() {
    assert_eq!(
        split_fused_name_and_marker("Kustomize2+"),
        Some(("Kustomize", "2+"))
    );
    assert_eq!(
        split_fused_name_and_marker("Dynatrace10+"),
        Some(("Dynatrace", "10+"))
    );
    // Plain digits-only "2+" is NOT split — that's already handled
    // directly as its own token by harvest_skill_segments.
    assert_eq!(split_fused_name_and_marker("2+"), None);
    // 3+ digit trailing runs are left alone (more likely a version
    // number / product name than a years count).
    assert_eq!(split_fused_name_and_marker("Log4j2023+"), None);
    // No trailing '+' at all.
    assert_eq!(split_fused_name_and_marker("Kubernetes"), None);
    // '+' with nothing digit-like before it.
    assert_eq!(split_fused_name_and_marker("C++"), None);
}

/// Regression test for the specific real-world pattern found in
/// Input_Resume.pdf's own TOOLS sidebar: a tool name fused directly
/// onto its own "N+" marker with zero space between them (verified
/// against its actual glyph positions — "Kustomize2+ yrs" is genuinely
/// how it's typeset, not a reconstruction artifact). Before this fix,
/// harvest_skill_segments required the "<N>+" marker to be its own
/// clean token, so "Kustomize2+" never matched at all — the whole line
/// bled through as if it were narrative text.
#[test]
fn harvest_skill_segments_splits_fused_name_and_marker() {
    assert_eq!(
        harvest_skill_segments("Kustomize2+ yrs"),
        Some(vec!["Kustomize 2+ yrs".to_string()])
    );
    // The real case from GeneratedCV.pdf: category header sharing a
    // reconstructed line with the fused entry.
    assert_eq!(
        harvest_skill_segments("TOOLS Kustomize2+ yrs"),
        Some(vec!["TOOLS Kustomize 2+ yrs".to_string()])
    );
    // Multiple fused entries on one line, matching the other observed
    // real case (Cloud category: AWS1+ yrs Scaleway1+ yrs Dynatrace2+
    // yrs).
    assert_eq!(
        harvest_skill_segments("AWS1+ yrs Scaleway1+ yrs Dynatrace2+ yrs"),
        Some(vec![
            "AWS 1+ yrs".to_string(),
            "Scaleway 1+ yrs".to_string(),
            "Dynatrace 2+ yrs".to_string(),
        ])
    );
}

#[test]
fn is_bare_years_marker_matches_fused_and_spaced_forms() {
    // Fused, no space before "yrs" — exactly how Input_Resume.pdf's own
    // TOOLS sidebar typesets it (verified against its glyph positions).
    assert!(is_bare_years_marker("2+yrs"));
    assert!(is_bare_years_marker("10+years"));
    // Spaced form too, in case some other source resume does it this
    // way instead.
    assert!(is_bare_years_marker("2+ yrs"));
    // Must NOT match once a name is attached — that's
    // harvest_skill_segments's job, on the same line.
    assert!(!is_bare_years_marker("Kustomize 2+yrs"));
    assert!(!is_bare_years_marker("Kustomize"));
    assert!(!is_bare_years_marker(""));
    assert!(!is_bare_years_marker("+yrs")); // no digits at all
}

/// Regression test for the specific pattern found in Input_Resume.pdf's
/// own TOOLS sidebar: a tool name and its "N+yrs" badge are the same
/// visual row, but end up as two separate reconstructed PDF lines
/// because their glyph "top" coordinates differ by just enough to miss
/// SAME_ROW_Y_EPSILON (a left-aligned name vs. a right-aligned badge in
/// a fixed-width column). Before this fix, neither line matched
/// harvest_skill_segments on its own (the name has no marker; the
/// marker has no name), so both bled straight through as if they were
/// genuine narrative text.
#[test]
fn parse_experiences_recovers_name_and_marker_split_across_two_lines() {
    let lines = vec![
        "Some Role at Acme - Jan 2021 - Present".to_string(),
        "• A genuine accomplishment bullet that ends properly.".to_string(),
        "Kustomize".to_string(),
        "2+yrs".to_string(),
        "• Another genuine bullet, unrelated to the sidebar noise.".to_string(),
    ];
    let (exps, skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    let skill_names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        skill_names,
        vec!["Kustomize 2+yrs"],
        "the split name+marker must be recovered as one harvested skill"
    );
    // And critically: "Kustomize" / "2+yrs" must not show up anywhere
    // in the actual bullets — neither as their own bogus bullets nor
    // glued onto a real one.
    let all_bullet_text: String = exps[0]
        .projects
        .iter()
        .flat_map(|p| p.bullets.iter())
        .map(|b| b.en.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(!all_bullet_text.contains("Kustomize"));
    assert!(!all_bullet_text.contains("2+yrs"));
    assert!(all_bullet_text.contains("A genuine accomplishment bullet"));
    assert!(all_bullet_text.contains("Another genuine bullet"));
}

/// Same as the test above, but for the fused-single-line variant
/// ("Kustomize2+ yrs" as one reconstructed PDF line, no line split at
/// all) — the other real pattern verified against Input_Resume.pdf's
/// own TOOLS sidebar glyph data. Confirms the fix integrates correctly
/// through the full parse_experiences pipeline, not just at the
/// harvest_skill_segments unit level.
#[test]
fn parse_experiences_recovers_fused_name_and_marker_on_one_line() {
    let lines = vec![
        "Some Role at Acme - Jan 2021 - Present".to_string(),
        "• A genuine accomplishment bullet that ends properly.".to_string(),
        "TOOLS Kustomize2+ yrs".to_string(),
        "• Another genuine bullet, unrelated to the sidebar noise.".to_string(),
    ];
    let (exps, skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    let skill_names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        skill_names,
        vec!["TOOLS Kustomize 2+ yrs"],
        "the fused name+marker must be recovered as one harvested skill"
    );
    let all_bullet_text: String = exps[0]
        .projects
        .iter()
        .flat_map(|p| p.bullets.iter())
        .map(|b| b.en.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(!all_bullet_text.contains("Kustomize"));
    assert!(all_bullet_text.contains("A genuine accomplishment bullet"));
    assert!(all_bullet_text.contains("Another genuine bullet"));
}

#[test]
fn harvest_skills_from_experiences_strips_bled_bullets_and_populates_skills() {
    let mut experiences = vec![Experience {
        id: "1".to_string(),
        role: LocalizedText::same("Platform Engineer"),
        company: "Acme".to_string(),
        projects: vec![ExperienceProject {
            name: LocalizedText::default(),
            bullets: vec![
                LocalizedText::same("Reduced incidents via GitOps automation."),
                LocalizedText::same("CI/CD GitLab-CI 3+ yrs GitHub Actions 2+ yrs"),
            ],
            ..Default::default()
        }],
        ..Default::default()
    }];
    let harvested = harvest_skills_from_experiences(&mut experiences);
    assert_eq!(harvested.len(), 2);
    assert!(harvested.iter().any(|s| s.name == "CI/CD GitLab-CI 3+ yrs"));
    assert!(harvested.iter().any(|s| s.name == "GitHub Actions 2+ yrs"));
    // The genuine bullet must remain; the bled-in one must be gone.
    let remaining: Vec<&str> = experiences[0].projects[0]
        .bullets
        .iter()
        .map(|b| b.en.as_str())
        .collect();
    assert_eq!(remaining, vec!["Reduced incidents via GitOps automation."]);
}

#[test]
fn parse_experiences_project_date_range_does_not_split_job() {
    let lines = vec![
        "Platform Engineer".to_string(),
        "Acme Corp".to_string(),
        "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
        "– Led the platform migration.".to_string(),
        "Project 1: Internal Tooling".to_string(),
        "\u{0011} February 2025 – February 2026".to_string(),
        "– Built the internal dashboard.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(
        exps.len(),
        1,
        "the project's own date range must not create a second job"
    );
    assert_eq!(exps[0].role.en, "Platform Engineer");
    assert_eq!(exps[0].company, "Acme Corp");
    // Regression: this date range used to be silently discarded
    // entirely rather than attached to the project — losing content
    // on every re-import of our own rendered PDF, since our renderer
    // draws this line and a re-import then hits this exact shape.
    let project = exps[0]
        .projects
        .iter()
        .find(|p| p.name.en == "Project 1: Internal Tooling")
        .expect("named project should be present");
    assert_eq!(project.start_date, "February 2025");
    assert_eq!(project.end_date, "February 2026");
}

/// Regression test for the bug found in the *next* round of idempotence
/// testing: once a project's dates parse correctly (see the test
/// above), our own renderer draws them inline on the SAME line as the
/// header, i.e. "Project N: Title – Subtitle  Start – End" — and the
/// title itself often contains its own " – " ("Cloud πNative – Socle
/// Team"). `extract_date_range_from_end`'s fast path ("another
/// occurrence of the same separator marks off the start too") found
/// that internal dash and mistook it for the name/date boundary,
/// splitting the line into a bogus new job (role = the first half of
/// the title) instead of leaving it as one project header with its
/// name intact and dates attached. Confirmed via a hand-built native
/// harness against the real `pdf_import.rs` on an actual generated PDF
/// that this exact shape appears once dates are non-empty, and that
/// re-rendering it broke idempotence a second time.
#[test]
fn parse_experiences_inline_project_header_with_internal_dash_does_not_split_job() {
    let lines = vec![
        "Platform Engineer".to_string(),
        "Acme Corp".to_string(),
        "December 2024 – February 2026 ½ Paris, France".to_string(),
        "– Led the platform migration.".to_string(),
        "Project 1: Cloud πNative – Socle Team February 2025 – February 2026".to_string(),
        "– Built the internal dashboard.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(
        exps.len(),
        1,
        "an inline-dated project header with its own internal dash must not spawn a second job"
    );
    assert_eq!(exps[0].role.en, "Platform Engineer");
    assert_eq!(exps[0].company, "Acme Corp");
    let project = exps[0]
        .projects
        .iter()
        .find(|p| !p.name.en.is_empty())
        .expect("named project should be present");
    assert_eq!(project.name.en, "Project 1: Cloud πNative – Socle Team");
    assert_eq!(project.start_date, "February 2025");
    assert_eq!(project.end_date, "February 2026");
}

/// Regression test for the idempotence-breaking bug where a project's
/// own icon-prefixed date range, with NO space between the icon glyph
/// and the month name (e.g. "\u{11}February 2025 – February 2026" — as
/// opposed to "\u{11} February 2025 – ...", which was already handled),
/// was misread by `extract_date_range_from_end`'s fallback path: unable
/// to recognize "\u{11}February" as the start of a date, it treated
/// just the bare "2025" as the start and mistook the icon+month for
/// leftover role/company text — spawning a bogus, mostly-empty new
/// "job" (role "\u{11}February", no company) instead of attaching the
/// date to Project 1 where it belongs. Our own renderer draws this
/// exact icon-glued-to-month shape for a project header's date line, so
/// this corrupted every re-import of a PDF we generated ourselves —
/// the phantom job then got rendered as a stray visible block, and a
/// second re-import produced yet another different result, breaking
/// idempotence.
#[test]
fn parse_experiences_glued_icon_month_project_date_does_not_spawn_phantom_job() {
    let lines = vec![
        "Platform Engineer".to_string(),
        "Acme Corp".to_string(),
        "\u{0011}December 2024 – February 2026 ½ Paris, France".to_string(),
        "– Led the platform migration.".to_string(),
        "Project 1: Internal Tooling".to_string(),
        "\u{0011}February 2025 – February 2026".to_string(),
        "– Built the internal dashboard.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(
        exps.len(),
        1,
        "the project's own glued icon+month date range must not spawn a second, phantom job"
    );
    assert_eq!(exps[0].role.en, "Platform Engineer");
    assert_eq!(exps[0].company, "Acme Corp");
    let project = exps[0]
        .projects
        .iter()
        .find(|p| p.name.en == "Project 1: Internal Tooling")
        .expect("named project should be present");
    assert_eq!(project.start_date, "February 2025");
    assert_eq!(project.end_date, "February 2026");
}

/// Regression test: a common CV layout puts role, company, and dates on
/// three SEPARATE lines. Before this fix, this pattern was never
/// recognized and the whole Experience section imported empty.
#[test]
fn parse_experiences_three_line_role_company_dates_persisted() {
    let lines = vec![
        "Platform Engineer (contractual)".to_string(),
        "DTNUM/SDAN/BFO".to_string(),
        "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
        "– Implemented GitOps deployment for the platform.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0].role.en, "Platform Engineer (contractual)");
    assert_eq!(exps[0].company, "DTNUM/SDAN/BFO");
    assert_eq!(exps[0].start_date, "December 2024");
    assert_eq!(exps[0].end_date, "February 2026");
}

/// Regression test: a long bullet sentence that wraps to 2-3 lines in
/// the source PDF must be merged back into one bullet, not truncated.
#[test]
fn parse_experiences_merges_wrapped_bullet_continuation_lines() {
    let lines = vec![
        "Platform Engineer (contractual)".to_string(),
        "DTNUM/SDAN/BFO".to_string(),
        "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
        "– Implemented GitOps deployment for the Cloud π Native Socle (ArgoCD) with regular"
            .to_string(),
        "version upgrades.".to_string(),
        "Techs: Openstack, Scaleway, Debian.".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    let bullets: Vec<&str> = exps[0]
        .projects
        .iter()
        .flat_map(|p| p.bullets.iter().map(|b| b.en.as_str()))
        .collect();
    assert!(bullets.contains(&"Implemented GitOps deployment for the Cloud π Native Socle (ArgoCD) with regular version upgrades."),
            "expected merged bullet, got: {:?}", bullets);
}

/// Regression test: a common CV layout puts the degree, a wrapped
/// field-of-study, and a wrapped institution+country each on their own
/// lines, followed by a standalone (often abbreviated-month) date
/// range. Before this fix, the date always completed the WRONG entry
/// (an off-by-one), institution/field got scrambled, and a spurious
/// trailing entry with only dates appeared.
#[test]
fn parse_education_multi_line_degree_field_institution() {
    let lines = vec![
        "Magistère of Mathematics".to_string(),
        "University of Paris-sud, Orsay,".to_string(),
        "France".to_string(),
        "\u{0011} Sept 2014 – Oct 2017".to_string(),
        "Master of Mathematics (MA)".to_string(),
        "Fundamental and applied".to_string(),
        "Mathematics".to_string(),
        "University of Paris-Saclay, Orsay,".to_string(),
        "France".to_string(),
        "\u{0011} Sept 2015 – May 2017".to_string(),
    ];
    let edus = parse_education(&lines);
    assert_eq!(
        edus.len(),
        2,
        "expected exactly 2 entries, got: {:?}",
        edus.iter().map(|e| &e.degree.en).collect::<Vec<_>>()
    );

    assert_eq!(edus[0].degree.en, "Magistère of Mathematics");
    assert_eq!(
        edus[0].institution,
        "University of Paris-sud, Orsay, France"
    );
    assert_eq!(edus[0].start_year, "Sept 2014");
    assert_eq!(edus[0].end_year, "Oct 2017");

    assert_eq!(edus[1].degree.en, "Master of Mathematics (MA)");
    assert_eq!(edus[1].field.en, "Fundamental and applied Mathematics");
    assert_eq!(
        edus[1].institution,
        "University of Paris-Saclay, Orsay, France"
    );
    assert_eq!(edus[1].start_year, "Sept 2015");
    assert_eq!(edus[1].end_year, "May 2017");
}

/// Regression test, education-section counterpart to
/// `parse_experiences_glued_icon_month_project_date_does_not_spawn_phantom_job`:
/// with NO space between the icon glyph and the abbreviated month
/// (e.g. "\u{11}Sept 2014 – Oct 2017"), `extract_trailing_date_range_loose`
/// used to swallow the whole institution/location line as if it were
/// "institution text ending in the bare year 2014", leaving
/// "\u{11}Sept" to be misread as if it were itself the institution
/// name for start="2014" (dropping the month) — corrupting the
/// institution and its start date on any PDF (produced by a tool other
/// than our own renderer, which happens to always put a space there)
/// that glues the icon directly onto the month.
#[test]
fn parse_education_glued_icon_month_date_does_not_corrupt_institution() {
    let lines = vec![
        "Magistère of Mathematics".to_string(),
        "University of Paris-sud, Orsay,".to_string(),
        "France".to_string(),
        "\u{0011}Sept 2014 – Oct 2017".to_string(),
    ];
    let edus = parse_education(&lines);
    assert_eq!(edus.len(), 1, "expected exactly 1 entry, got: {:?}", edus);
    assert_eq!(edus[0].degree.en, "Magistère of Mathematics");
    assert_eq!(
        edus[0].institution,
        "University of Paris-sud, Orsay, France"
    );
    assert_eq!(edus[0].start_year, "Sept 2014");
    assert_eq!(edus[0].end_year, "Oct 2017");
}

/// Regression test: "OTHERS"/"INTERESTS" sections must not swallow
/// whatever section came before them.
#[test]
fn ignore_sections_do_not_bleed_into_certifications() {
    let text =
        "CERTIFICATIONS\nITIL Foundation\nOTHERS\nDriving License B\nINTERESTS\nChess\nManga";
    let sections = split_into_sections(text);
    let certs: Vec<&String> = sections
        .iter()
        .filter(|(s, _)| *s == "certifications")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(certs, vec!["ITIL Foundation"]);
    let ignored: Vec<&str> = sections
        .iter()
        .filter(|(s, _)| *s == "ignore")
        .map(|(s, _)| *s)
        .collect();
    assert_eq!(ignored.len(), 2);
}

/// Regression test for the real bug: a multi-column PDF interleaves a
/// sidebar header (e.g. "INTERESTS") into the middle of the document,
/// stranding entire subsequent job entries in a section that gets
/// dropped. The reclaim pass must recover them into Experience.
#[test]
fn reclaim_stray_experience_content_recovers_stranded_jobs() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("header", vec!["Vincent".to_string()]),
        (
            "experience",
            vec![
                "Site Reliability Engineer".to_string(),
                "Sirius".to_string(),
                "\u{0011} October 2022 – January 2024 ½ Bangkok, Thailand".to_string(),
                "– Maintained IaC on AWS.".to_string(),
            ],
        ),
        (
            "ignore",
            vec![
                "INTERESTS".to_string(),
                "Chess".to_string(),
                "Manga".to_string(),
                "DevOps Engineer, Database Developer".to_string(),
                "BRED IT (Thailand) Ltd".to_string(),
                "\u{0011} May 2021 – October 2022 ½ Bangkok, Thailand".to_string(),
                "– L2 Linux and Mainframe support.".to_string(),
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert!(
        exp_lines
            .iter()
            .any(|l| l.as_str() == "DevOps Engineer, Database Developer"),
        "expected the stranded job to be reclaimed into experience, got: {:?}",
        exp_lines
    );
    let (exps, _skills) =
        parse_experiences(&exp_lines.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(
        exps.len(),
        2,
        "expected both jobs to parse, got: {:?}",
        exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
    );
    assert_eq!(exps[1].role.en, "DevOps Engineer, Database Developer");
    assert_eq!(exps[1].company, "BRED IT (Thailand) Ltd");
}

// ── extract_email ─────────────────────────────────────────────────────

#[test]
fn extract_email_returns_none_when_no_dot_after_filter() {
    assert_eq!(extract_email("user@com"), None);
}

#[test]
fn extract_email_filters_trailing_dot_leaving_no_dot() {
    assert_eq!(extract_email("user@."), None);
}

#[test]
fn extract_email_rejects_leading_at() {
    assert_eq!(extract_email("@example.com"), None);
}

#[test]
fn extract_email_rejects_trailing_dot() {
    assert_eq!(extract_email("user@example.com."), None);
}

#[test]
fn extract_email_strips_angle_brackets_and_semicolons() {
    assert_eq!(
        extract_email("<user@host.com>;"),
        Some("user@host.com".to_string())
    );
}

#[test]
fn extract_email_preserves_plus_and_dash_and_underscore() {
    assert_eq!(
        extract_email("a+b-c_d@host.com"),
        Some("a+b-c_d@host.com".to_string())
    );
}

#[test]
fn extract_email_none_when_only_at_sign() {
    assert_eq!(extract_email("@"), None);
}

#[test]
fn extract_email_returns_none_for_empty_input() {
    assert_eq!(extract_email(""), None);
}

// ── extract_phone ─────────────────────────────────────────────────────

#[test]
fn extract_phone_6_digits_returns_none() {
    assert_eq!(extract_phone("123456"), None);
}

#[test]
fn extract_phone_16_digits_returns_none() {
    assert_eq!(extract_phone("1234567890123456"), None);
}

#[test]
fn extract_phone_exactly_15_digits_with_plus() {
    assert_eq!(
        extract_phone("+123456789012345"),
        Some("+123456789012345".to_string())
    );
}

#[test]
fn extract_phone_no_plus_returns_digits_without_prefix() {
    assert_eq!(
        extract_phone("call 1234567890"),
        Some("1234567890".to_string())
    );
}

#[test]
fn extract_phone_empty_returns_none() {
    assert_eq!(extract_phone(""), None);
}

#[test]
fn extract_phone_exactly_7_digits() {
    assert_eq!(extract_phone("1234567"), Some("1234567".to_string()));
}

// ── extract_urls ──────────────────────────────────────────────────────

#[test]
fn extract_urls_linkedin_dot_only_sets_linkedin() {
    let (li, _gh, _web) = extract_urls("linkedin.com/in/john");
    assert_eq!(li, Some("linkedin.com/in/john".to_string()));
}

#[test]
fn extract_urls_github_with_profile_path_sets_github() {
    let (_li, gh, _web) = extract_urls("github.com/john");
    assert_eq!(gh, Some("github.com/john".to_string()));
}

#[test]
fn extract_urls_name_github_io_does_not_set_github() {
    let (_li, gh, web) = extract_urls("name.github.io");
    assert!(gh.is_none());
    assert!(web.is_some());
}

#[test]
fn extract_urls_bare_domain_sets_website() {
    let (_li, _gh, web) = extract_urls("falltrades.github.io/path");
    assert_eq!(web, Some("falltrades.github.io/path".to_string()));
}

#[test]
fn extract_urls_http_prefix_sets_website() {
    let (_li, _gh, web) = extract_urls("http://example.com");
    assert_eq!(web, Some("http://example.com".to_string()));
}

#[test]
fn extract_urls_www_prefix_sets_website() {
    let (_li, _gh, web) = extract_urls("www.example.com");
    assert_eq!(web, Some("www.example.com".to_string()));
}

#[test]
fn extract_urls_duplicate_linkedin_not_added_to_website() {
    let (_li, _gh, web) = extract_urls("linkedin.com/in/john linkedin.com/in/john");
    assert!(web.is_none());
}

#[test]
fn extract_urls_all_none_for_empty() {
    let (li, gh, web) = extract_urls("");
    assert!(li.is_none());
    assert!(gh.is_none());
    assert!(web.is_none());
}

// ── looks_like_bare_domain ────────────────────────────────────────────

#[test]
fn looks_like_bare_domain_host_without_dot_false() {
    assert!(!looks_like_bare_domain("nodotcom"));
}

#[test]
fn looks_like_bare_domain_starts_with_dot_false() {
    assert!(!looks_like_bare_domain(".example.com"));
}

#[test]
fn looks_like_bare_domain_ends_with_dot_false() {
    assert!(!looks_like_bare_domain("example.com."));
}

#[test]
fn looks_like_bare_domain_contains_at_false() {
    assert!(!looks_like_bare_domain("user@host.com"));
}

#[test]
fn looks_like_bare_domain_valid_io() {
    assert!(looks_like_bare_domain("example.io"));
}

#[test]
fn looks_like_bare_domain_valid_fr() {
    assert!(looks_like_bare_domain("example.fr"));
}

#[test]
fn looks_like_bare_domain_unknown_tld_false() {
    assert!(!looks_like_bare_domain("example.xyz123"));
}

#[test]
fn looks_like_bare_domain_with_path() {
    assert!(looks_like_bare_domain("example.com/foo"));
}

#[test]
fn looks_like_bare_domain_hyphen_allowed() {
    assert!(looks_like_bare_domain("my-site.dev"));
}

#[test]
fn looks_like_bare_domain_non_alnum_hyphen_dot_false() {
    assert!(!looks_like_bare_domain("my site.com"));
}

// ── looks_like_bare_role_line ─────────────────────────────────────────

#[test]
fn looks_like_bare_role_line_empty_false() {
    assert!(!looks_like_bare_role_line(""));
    assert!(!looks_like_bare_role_line("   "));
}

#[test]
fn looks_like_bare_role_line_over_100_chars_false() {
    let line = "A".repeat(101);
    assert!(!looks_like_bare_role_line(&line));
}

#[test]
fn looks_like_bare_role_line_starts_with_bullet_false() {
    assert!(!looks_like_bare_role_line("• Engineer"));
    assert!(!looks_like_bare_role_line("- Developer"));
    assert!(!looks_like_bare_role_line("* Architect"));
}

#[test]
fn looks_like_bare_role_line_starts_with_project_false() {
    assert!(!looks_like_bare_role_line("Project 1: something"));
}

#[test]
fn looks_like_bare_role_line_starts_with_tasks_false() {
    assert!(!looks_like_bare_role_line("Tasks: did stuff"));
}

#[test]
fn looks_like_bare_role_line_starts_with_tools_false() {
    assert!(!looks_like_bare_role_line("Tools: Rust, Go"));
}

#[test]
fn looks_like_bare_role_line_contains_dot_space_false() {
    assert!(!looks_like_bare_role_line("Engineer. Did things"));
}

#[test]
fn looks_like_bare_role_line_lowercase_first_false() {
    assert!(!looks_like_bare_role_line("engineer"));
}

#[test]
fn looks_like_bare_role_line_with_date_range_at_end_false() {
    assert!(!looks_like_bare_role_line("Engineer Jan 2021 - Feb 2022"));
}

#[test]
fn looks_like_bare_role_line_clean_title_true() {
    assert!(looks_like_bare_role_line("Architecte DevOps"));
}

#[test]
fn looks_like_bare_role_line_single_word_title_true() {
    assert!(looks_like_bare_role_line("Engineer"));
}

// ── split_company_and_location ────────────────────────────────────────

#[test]
fn split_company_and_location_middle_dot() {
    let (c, l) = split_company_and_location("Acme · Paris");
    assert_eq!(c, "Acme");
    assert_eq!(l, "Paris");
}

#[test]
fn split_company_and_location_pipe() {
    let (c, l) = split_company_and_location("Acme | London");
    assert_eq!(c, "Acme");
    assert_eq!(l, "London");
}

#[test]
fn split_company_and_location_comma() {
    let (c, l) = split_company_and_location("Acme, Berlin");
    assert_eq!(c, "Acme");
    assert_eq!(l, "Berlin");
}

#[test]
fn split_company_and_location_no_sep() {
    let (c, l) = split_company_and_location("Acme");
    assert_eq!(c, "Acme");
    assert_eq!(l, "");
}

#[test]
fn split_company_and_location_first_sep_wins() {
    let (c, l) = split_company_and_location("X · Y, Z");
    assert_eq!(c, "X");
    assert_eq!(l, "Y, Z");
}

// ── extract_date_range_from_end ───────────────────────────────────────

#[test]
fn extract_date_range_from_end_acme_present() {
    assert_eq!(
        extract_date_range_from_end("Acme - Jan 2021 - Present"),
        Some(("Jan 2021".to_string(), "Present".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_two_dates() {
    assert_eq!(
        extract_date_range_from_end("Acme - Jan 2021 - Feb 2022"),
        Some(("Jan 2021".to_string(), "Feb 2022".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_whitespace_sep() {
    assert_eq!(
        extract_date_range_from_end("Company France December 2024 - February 2026"),
        Some(("December 2024".to_string(), "February 2026".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_bare_year() {
    assert_eq!(
        extract_date_range_from_end("Acme Corp - 2021 - 2024"),
        Some(("2021".to_string(), "2024".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_end_over_3_words_skips() {
    assert_eq!(
        extract_date_range_from_end("Acme - Jan 2021 ABCD - Feb 2022"),
        Some(("Jan 2021 ABCD".to_string(), "Feb 2022".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_no_date() {
    assert!(extract_date_range_from_end("Just a plain line").is_none());
}

#[test]
fn extract_date_range_from_end_en_dash_separator() {
    assert_eq!(
        extract_date_range_from_end("Acme – Jan 2021 – Feb 2022"),
        Some(("Jan 2021".to_string(), "Feb 2022".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_actuel_present() {
    assert_eq!(
        extract_date_range_from_end("Acme - Jan 2021 - actuel"),
        Some(("Jan 2021".to_string(), "Present".to_string()))
    );
}

// ── tokenize_with_spans ───────────────────────────────────────────────

#[test]
fn tokenize_with_spans_empty() {
    assert_eq!(tokenize_with_spans(""), vec![]);
}

#[test]
fn tokenize_with_spans_whitespace_only() {
    assert_eq!(tokenize_with_spans("   "), vec![]);
}

#[test]
fn tokenize_with_spans_leading_trailing_ws() {
    let result = tokenize_with_spans("  hello world  ");
    assert_eq!(result, vec![(2, 7, "hello"), (8, 13, "world")]);
}

#[test]
fn tokenize_with_spans_single_token() {
    let result = tokenize_with_spans("hello");
    assert_eq!(result, vec![(0, 5, "hello")]);
}

#[test]
fn tokenize_with_spans_preserves_byte_spans() {
    let line = "Jan 2021 - Feb 2022";
    let result = tokenize_with_spans(line);
    assert_eq!(result.len(), 5);
    assert_eq!(result[0], (0, 3, "Jan"));
    assert_eq!(result[1], (4, 8, "2021"));
    assert_eq!(result[2], (9, 10, "-"));
    assert_eq!(result[3], (11, 14, "Feb"));
    assert_eq!(result[4], (15, 19, "2022"));
}

// ── find_date_range_span ──────────────────────────────────────────────

#[test]
fn find_date_range_span_since_month_year() {
    let r = find_date_range_span("Role at Acme Since January 2020");
    assert!(r.is_some());
    let (start, end, s, e) = r.unwrap();
    assert_eq!(s, "January 2020");
    assert_eq!(e, "Present");
    assert_eq!(
        &"Role at Acme Since January 2020"[start..end],
        "Since January 2020"
    );
}

#[test]
fn find_date_range_span_depuis_bare_year() {
    let r = find_date_range_span("Depuis 2019");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "2019");
    assert_eq!(e, "Present");
}

#[test]
fn find_date_range_span_month_year_dash_month_year() {
    let r = find_date_range_span("January 2021 - February 2022");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "January 2021");
    assert_eq!(e, "February 2022");
}

#[test]
fn find_date_range_span_bare_year_dash_bare_year() {
    let r = find_date_range_span("2020 – 2024");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "2020");
    assert_eq!(e, "2024");
}

#[test]
fn find_date_range_span_bare_year_dash_present() {
    let r = find_date_range_span("2020 - Present");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "2020");
    assert_eq!(e, "Present");
}

#[test]
fn find_date_range_span_au_separator() {
    let r = find_date_range_span("January 2021 au February 2022");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "January 2021");
    assert_eq!(e, "February 2022");
}

#[test]
fn find_date_range_span_to_separator() {
    let r = find_date_range_span("January 2021 to February 2022");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "January 2021");
    assert_eq!(e, "February 2022");
}

#[test]
fn find_date_range_span_a_separator() {
    let r = find_date_range_span("January 2021 à February 2022");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "January 2021");
    assert_eq!(e, "February 2022");
}

#[test]
fn find_date_range_span_month_year_sep_present() {
    let r = find_date_range_span("January 2021 – Present");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "January 2021");
    assert_eq!(e, "Present");
}

#[test]
fn find_date_range_span_month_year_sep_bare_year() {
    let r = find_date_range_span("January 2021 - 2022");
    assert!(r.is_some());
    let (_, _, s, e) = r.unwrap();
    assert_eq!(s, "January 2021");
    assert_eq!(e, "2022");
}

#[test]
fn find_date_range_span_no_date_none() {
    assert!(find_date_range_span("Just a plain line").is_none());
}

// ── extract_trailing_date_range_from_title ────────────────────────────

#[test]
fn extract_trailing_date_range_from_title_month_year_present() {
    assert_eq!(
        extract_trailing_date_range_from_title(
            "Project 1: Title – Subtitle  January 2021 – Present"
        ),
        Some((
            "Project 1: Title – Subtitle".to_string(),
            "January 2021".to_string(),
            "Present".to_string()
        ))
    );
}

#[test]
fn extract_trailing_date_range_from_title_bare_year() {
    assert_eq!(
        extract_trailing_date_range_from_title("My Project 2020 - 2024"),
        Some((
            "My Project".to_string(),
            "2020".to_string(),
            "2024".to_string()
        ))
    );
}

#[test]
fn extract_trailing_date_range_from_title_two_dates() {
    assert_eq!(
        extract_trailing_date_range_from_title("Tool X – Sub  January 2021 - February 2022"),
        Some((
            "Tool X – Sub".to_string(),
            "January 2021".to_string(),
            "February 2022".to_string()
        ))
    );
}

#[test]
fn extract_trailing_date_range_from_title_empty_name_none() {
    assert!(extract_trailing_date_range_from_title("2021-2024").is_none());
}

#[test]
fn extract_trailing_date_range_from_title_no_date_none() {
    assert!(extract_trailing_date_range_from_title("Just a title").is_none());
}

#[test]
fn extract_trailing_date_range_from_title_end_with_three_words() {
    // `end.split_whitespace().count() > 3` must accept an end part of
    // exactly 3 words; flipping `>` to `>=` would skip it.
    assert_eq!(
        extract_trailing_date_range_from_title("My Tool 2021 - Dec 31 2022"),
        Some((
            "My Tool".to_string(),
            "2021".to_string(),
            "Dec 31 2022".to_string()
        ))
    );
}

// ── find_date_range_span end-of-token boundary ─────────────────────────
//
// These pin the `<` guards that keep a date range from indexing past the
// last token. Each line ends its date pattern exactly at the last token
// (no trailing token), so:
//   - "<" correctly declines (None) — never an out-of-bounds panic;
//   - mutating "<" to "<=" lets the guard pass and then panics on the
//     out-of-range token read, killing the mutant.
#[test]
fn find_date_range_span_since_month_at_end_is_none() {
    // "Since <Month>" with no following year and no trailing token.
    assert!(find_date_range_span("Since January").is_none());
    assert!(find_date_range_span("Depuis Janvier").is_none());
}

#[test]
fn find_date_range_span_since_bare_word_at_end_is_none() {
    // A bare "Since"/"Depuis" as the very last token: the `t + 1 < len`
    // guard must decline (None), not index past the last token.
    assert!(find_date_range_span("Depuis").is_none());
    assert!(find_date_range_span("Since").is_none());
    // Non-year token right after "Since" is not a date either.
    assert!(find_date_range_span("Depuis quelques").is_none());
}

#[test]
fn find_date_range_span_month_year_sep_at_end_is_none() {
    // Month-Year separator with the month exactly at the last token.
    assert!(find_date_range_span("January – ").is_none());
    // Month at end, separator at end (no second date).
    assert!(find_date_range_span("January 2021 – ").is_none());
    assert!(find_date_range_span("January 2021 – February").is_none());
}

#[test]
fn find_date_range_span_bare_year_sep_at_end_is_none() {
    // Bare-year separator with the year at the very last token.
    assert!(find_date_range_span("2020 – ").is_none());
    // Year at end (a single date).
    assert!(find_date_range_span("2020").is_none());
}

#[test]
fn find_date_range_span_since_non_date_is_none() {
    // A "since"/"depuis" word followed by non-date tokens must not be
    // parsed as a range (mutating the range's inner `&&` to `||` would
    // wrongly accept it).
    assert!(find_date_range_span("Role Depuis Acme Corp").is_none());
    assert!(find_date_range_span("Since Acme").is_none());
}

// ── guess_title ────────────────────────────────────────────────────────

#[test]
fn guess_title_uses_line_after_first_non_header_contact() {
    // The title must be scanned starting on the line right after the
    // detected name/contact, not from the top, and must not mistake the
    // first contact/URL line for the boundary.
    assert_eq!(
        guess_title(&[
            "john@test.com",
            "http://example.com",
            "Jane Doe",
            "Senior Engineer",
            "Anything"
        ]),
        Some("Senior Engineer".to_string())
    );
}

#[test]
fn guess_title_scans_up_to_four_lines_after_name() {
    assert_eq!(
        guess_title(&[
            "Jane Doe",
            "line one",
            "line two",
            "line three",
            "engineering manager",
        ]),
        Some("engineering manager".to_string())
    );
}

#[test]
fn guess_title_name_line_is_not_itself_a_title() {
    // A name line that happens to contain a title keyword must not be
    // reported as the title when a following line carries the real one.
    assert_eq!(
        guess_title(&["Alice Engineer", "Architect"]),
        Some("Architect".to_string())
    );
}

#[test]
fn extract_date_range_present() {
    assert_eq!(
        extract_date_range("Jan 2021 - Present"),
        Some(("Jan 2021".to_string(), "Present".to_string()))
    );
}

#[test]
fn extract_date_range_bare_years() {
    assert_eq!(
        extract_date_range("2020 - 2024"),
        Some(("2020".to_string(), "2024".to_string()))
    );
}

#[test]
fn extract_date_range_left_too_short() {
    assert!(extract_date_range("A - B").is_none());
}

#[test]
fn extract_date_range_left_exactly_three_chars() {
    // `left.len() < 3` must accept a left of exactly 3 chars; flipping
    // `<` to `<=` would reject it.
    assert_eq!(
        extract_date_range("Jan - Feb"),
        Some(("Jan".to_string(), "Feb".to_string()))
    );
}

#[test]
fn extract_date_range_to_separator() {
    assert_eq!(
        extract_date_range("Jan 2021 to Dec 2022"),
        Some(("Jan 2021".to_string(), "Dec 2022".to_string()))
    );
}

#[test]
fn extract_date_range_a_separator() {
    assert_eq!(
        extract_date_range("Jan 2021 à Dec 2022"),
        Some(("Jan 2021".to_string(), "Dec 2022".to_string()))
    );
}

#[test]
fn extract_date_range_au_separator() {
    assert_eq!(
        extract_date_range("Jan 2021 au Dec 2022"),
        Some(("Jan 2021".to_string(), "Dec 2022".to_string()))
    );
}

#[test]
fn extract_date_range_fr_present_words() {
    assert_eq!(
        extract_date_range("Jan 2021 - actuel"),
        Some(("Jan 2021".to_string(), "Present".to_string()))
    );
    assert_eq!(
        extract_date_range("Jan 2021 - aujourd'hui"),
        Some(("Jan 2021".to_string(), "Present".to_string()))
    );
    assert_eq!(
        extract_date_range("Jan 2021 - current"),
        Some(("Jan 2021".to_string(), "Present".to_string()))
    );
}

// ── guess_name ────────────────────────────────────────────────────────

#[test]
fn guess_name_picks_first_plausible_line() {
    assert_eq!(
        guess_name(&["Alice Bob", "Engineer", "alice@test.com"]),
        Some("Alice Bob".to_string())
    );
}

#[test]
fn guess_name_skips_empty() {
    assert_eq!(
        guess_name(&["", "  ", "Charlie D"]),
        Some("Charlie D".to_string())
    );
}

#[test]
fn guess_name_skips_section_header() {
    assert_eq!(
        guess_name(&["Experience", "Dana F"]),
        Some("Dana F".to_string())
    );
}

#[test]
fn guess_name_skips_email() {
    assert_eq!(
        guess_name(&["bob@test.com", "Eve G"]),
        Some("Eve G".to_string())
    );
}

#[test]
fn guess_name_skips_phone() {
    assert_eq!(
        guess_name(&["+33 6 12 34 56 78", "Frank H"]),
        Some("Frank H".to_string())
    );
}

#[test]
fn guess_name_skips_http() {
    assert_eq!(
        guess_name(&["http://example.com", "Grace I"]),
        Some("Grace I".to_string())
    );
}

#[test]
fn guess_name_skips_www() {
    assert_eq!(
        guess_name(&["www.example.com", "Hank J"]),
        Some("Hank J".to_string())
    );
}

#[test]
fn guess_name_skips_linkedin() {
    assert_eq!(
        guess_name(&["linkedin.com/in/john", "Iris K"]),
        Some("Iris K".to_string())
    );
}

#[test]
fn guess_name_too_many_words_none() {
    assert!(guess_name(&["One Two Three Four Five Six"]).is_none());
}

#[test]
fn guess_name_one_word_none() {
    assert!(guess_name(&["Engineer"]).is_none());
}

#[test]
fn guess_name_accented_chars_allowed() {
    assert_eq!(
        guess_name(&["Jean-Luc François"]),
        Some("Jean-Luc François".to_string())
    );
}

#[test]
fn guess_name_control_chars_cleaned() {
    assert_eq!(
        guess_name(&["John\u{0003} Smith"]),
        Some("John Smith".to_string())
    );
}

// ── guess_title ───────────────────────────────────────────────────────

#[test]
fn guess_title_finds_keyword_after_name() {
    assert_eq!(
        guess_title(&["John Smith", "Senior Engineer"]),
        Some("Senior Engineer".to_string())
    );
}

#[test]
fn guess_title_finds_french_keyword() {
    assert_eq!(
        guess_title(&["Jean Dupont", "Développeur Rust"]),
        Some("Développeur Rust".to_string())
    );
}

#[test]
fn guess_title_none_when_no_keyword() {
    assert!(guess_title(&["John Smith", "Nothing here"]).is_none());
}

#[test]
fn guess_title_skips_email_before_name() {
    assert_eq!(
        guess_title(&["john@test.com", "Jane Doe", "Architect"]),
        Some("Architect".to_string())
    );
}

// ── looks_like_date_token ─────────────────────────────────────────────

#[test]
fn looks_like_date_token_month() {
    assert!(looks_like_date_token("January"));
    assert!(looks_like_date_token("janvier"));
    assert!(looks_like_date_token("août"));
}

#[test]
fn looks_like_date_token_bare_year() {
    assert!(looks_like_date_token("2021"));
}

#[test]
fn looks_like_date_token_present_word() {
    assert!(looks_like_date_token("Present"));
    assert!(looks_like_date_token("actuel"));
}

#[test]
fn looks_like_date_token_leading_icon_glyph() {
    assert!(looks_like_date_token("\u{11}January"));
}

#[test]
fn looks_like_date_token_non_date_false() {
    assert!(!looks_like_date_token("hello"));
}

#[test]
fn looks_like_date_token_empty_after_strip_false() {
    assert!(!looks_like_date_token("!."));
}

#[test]
fn looks_like_date_token_abbreviated_month_not_recognized() {
    assert!(!looks_like_date_token("Jan"));
}

// ── looks_like_date_token_loose ───────────────────────────────────────

#[test]
fn looks_like_date_token_loose_full_month_passthrough() {
    assert!(looks_like_date_token_loose("January"));
}

#[test]
fn looks_like_date_token_loose_abbreviated_month() {
    assert!(looks_like_date_token_loose("Sept"));
    assert!(looks_like_date_token_loose("janv"));
}

#[test]
fn looks_like_date_token_loose_with_leading_icon() {
    assert!(looks_like_date_token_loose("\u{11}Sept"));
}

#[test]
fn looks_like_date_token_loose_non_date_false() {
    assert!(!looks_like_date_token_loose("hello"));
}

// ── looks_like_institution_line ───────────────────────────────────────

#[test]
fn looks_like_institution_line_iut_space() {
    assert!(looks_like_institution_line("IUT Informatique"));
}

#[test]
fn looks_like_institution_line_iut_exact() {
    assert!(looks_like_institution_line("IUT"));
}

#[test]
fn looks_like_institution_line_iut_lowercase() {
    assert!(looks_like_institution_line("iut paris"));
}

#[test]
fn looks_like_institution_line_university_of() {
    assert!(looks_like_institution_line("University of Cambridge"));
}

#[test]
fn looks_like_institution_line_ecole() {
    assert!(looks_like_institution_line("École Supérieure"));
}

#[test]
fn looks_like_institution_line_college() {
    assert!(looks_like_institution_line("Community College"));
}

#[test]
fn looks_like_institution_line_non_institution_false() {
    assert!(!looks_like_institution_line("Computer Science"));
}

// ── looks_like_degree_line ────────────────────────────────────────────

#[test]
fn looks_like_degree_line_licence() {
    assert!(looks_like_degree_line(
        "Licence Professionnelle Informatique"
    ));
}

#[test]
fn looks_like_degree_line_master_of_science() {
    assert!(looks_like_degree_line("Master of Science in AI"));
}

#[test]
fn looks_like_degree_line_bts() {
    assert!(looks_like_degree_line("BTS Services Informatiques"));
}

#[test]
fn looks_like_degree_line_mba() {
    assert!(looks_like_degree_line("MBA Finance"));
}

#[test]
fn looks_like_degree_line_phd() {
    assert!(looks_like_degree_line("PhD in Physics"));
}

#[test]
fn looks_like_degree_line_doctorat() {
    assert!(looks_like_degree_line("Doctorat Informatique"));
}

#[test]
fn looks_like_degree_line_diplome() {
    assert!(looks_like_degree_line("Diplôme d'ingénieur"));
}

#[test]
fn looks_like_degree_line_certificat() {
    assert!(looks_like_degree_line("Certificat AWS"));
}

#[test]
fn looks_like_degree_line_non_degree_false() {
    assert!(!looks_like_degree_line("University of Paris"));
}

// ── is_context_label ──────────────────────────────────────────────────

#[test]
fn is_context_label_with_situation_prefix() {
    assert!(is_context_label("Situation: The project needed help."));
}

#[test]
fn is_context_label_with_techs_prefix() {
    assert!(is_context_label("Techs: Rust, Go, Docker."));
}

#[test]
fn is_context_label_with_context_prefix() {
    assert!(is_context_label("Context: Cloud migration project."));
}

#[test]
fn is_context_label_no_colon_false() {
    assert!(!is_context_label("No colon here"));
}

#[test]
fn is_context_label_prefix_too_long_false() {
    assert!(!is_context_label(
        "A very long prefix that exceeds thirty chars: value"
    ));
}

#[test]
fn is_context_label_unknown_prefix_false() {
    assert!(!is_context_label("RandomWord: value"));
}

#[test]
fn is_context_label_lowercase_situation() {
    assert!(is_context_label("situation: details"));
}

// ── looks_like_tool_bleed_line ────────────────────────────────────────

#[test]
fn looks_like_tool_bleed_line_bullet_true() {
    assert!(looks_like_tool_bleed_line("• Docker 3+ yrs"));
}

#[test]
fn looks_like_tool_bleed_line_harvestable_skill_true() {
    assert!(looks_like_tool_bleed_line("Rust 5+ yrs Go 3+ yrs"));
}

#[test]
fn looks_like_tool_bleed_line_bare_years_true() {
    assert!(looks_like_tool_bleed_line("2+yrs"));
}

#[test]
fn looks_like_tool_bleed_line_clean_role_false() {
    assert!(!looks_like_tool_bleed_line("Architecte DevOps"));
}

#[test]
fn looks_like_tool_bleed_line_en_dash_bullet_true() {
    assert!(looks_like_tool_bleed_line("– Docker"));
}

// ── is_bare_years_marker ──────────────────────────────────────────────

#[test]
fn is_bare_years_marker_fused_two_token() {
    assert!(is_bare_years_marker("2+ yrs"));
    assert!(is_bare_years_marker("10+ years"));
    assert!(is_bare_years_marker("1+ yr"));
    assert!(is_bare_years_marker("5+ year"));
}

#[test]
fn is_bare_years_marker_single_token_fused() {
    assert!(is_bare_years_marker("2+yrs"));
    assert!(is_bare_years_marker("10+years"));
}

#[test]
fn is_bare_years_marker_no_digits_false() {
    assert!(!is_bare_years_marker("+yrs"));
}

#[test]
fn is_bare_years_marker_empty_false() {
    assert!(!is_bare_years_marker(""));
}

#[test]
fn is_bare_years_marker_three_tokens_false() {
    assert!(!is_bare_years_marker("2+ yrs extra"));
}

#[test]
fn is_bare_years_marker_name_with_marker_false() {
    assert!(!is_bare_years_marker("Kustomize 2+yrs"));
}

// ── is_project_header ─────────────────────────────────────────────────

#[test]
fn is_project_header_english() {
    assert!(is_project_header("Project 1: Cloud Migration"));
}

#[test]
fn is_project_header_french() {
    assert!(is_project_header("Projet 2: Migración"));
}

#[test]
fn is_project_header_non_project() {
    assert!(!is_project_header("Just a regular line"));
}

// ── parse_projects (delete-field mutants) ─────────────────────────────

#[test]
fn parse_projects_name_with_description() {
    let projects = parse_projects(&[
        "My App: A tool for tracking things".to_string(),
        "• helps you stay organised".to_string(),
    ]);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "My App");
    assert_eq!(projects[0].description.en, "A tool for tracking things");
    assert_eq!(projects[0].bullets.len(), 1);
    assert!(!projects[0].id.is_empty(), "project id must be populated");
}

#[test]
fn parse_projects_bare_name() {
    let projects = parse_projects(&["Side Project".to_string()]);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "Side Project");
    assert!(projects[0].description.en.is_empty());
    assert!(!projects[0].id.is_empty(), "project id must be populated");
}

#[test]
fn parse_projects_multiple_and_bullet_context() {
    let projects = parse_projects(&[
        "Alpha: first".to_string(),
        "• bullet one".to_string(),
        "Beta".to_string(),
        "• bullet two".to_string(),
    ]);
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].name, "Alpha");
    assert_eq!(projects[0].bullets.len(), 1);
    assert_eq!(projects[0].bullets[0].en, "bullet one");
    assert_eq!(projects[1].name, "Beta");
    assert_eq!(projects[1].bullets.len(), 1);
}

// ── build_education_institution_first (delete-field mutants) ──────────

#[test]
fn build_education_institution_first_all_fields() {
    let edu = build_education_institution_first(
        "Université Paris-Sud |".to_string(),
        "Sept 2014".to_string(),
        "Oct 2017".to_string(),
        &[
            "Master of Science in Computer Science".to_string(),
            "Algorithm Design".to_string(),
        ],
    )
    .expect("education should build");
    assert!(!edu.id.is_empty(), "education id must be populated");
    assert_eq!(edu.institution, "Université Paris-Sud");
    assert_eq!(edu.degree.en, "Master of Science");
    assert_eq!(edu.field.en, "Computer Science Algorithm Design");
    assert_eq!(edu.start_year, "Sept 2014");
    assert_eq!(edu.end_year, "Oct 2017");
}

#[test]
fn build_education_institution_first_embedded_field_en() {
    let edu = build_education_institution_first(
        "MIT".to_string(),
        "2015".to_string(),
        "2019".to_string(),
        &["Bachelor of Arts in Economics".to_string()],
    )
    .expect("education should build");
    assert_eq!(edu.degree.en, "Bachelor of Arts");
    assert_eq!(edu.field.en, "Economics");
}

#[test]
fn build_education_institution_first_embedded_field_fr_and_none() {
    let edu = build_education_institution_first(
        "ENS".to_string(),
        "2010".to_string(),
        "2013".to_string(),
        &["Licence en Mathématiques".to_string()],
    )
    .expect("education should build");
    assert_eq!(edu.degree.en, "Licence");
    assert_eq!(edu.field.en, "Mathématiques");

    let bare = build_education_institution_first(
        "College".to_string(),
        "2000".to_string(),
        "2004".to_string(),
        &["Diploma".to_string()],
    )
    .expect("education should build");
    assert_eq!(bare.degree.en, "Diploma");
    assert!(bare.field.en.is_empty());
}

#[test]
fn build_education_institution_first_empty_returns_none() {
    assert!(
        build_education_institution_first(String::new(), String::new(), String::new(), &[],)
            .is_none()
    );
}

// ── build_certification_from_buffer (delete-field mutants) ────────────

#[test]
fn build_certification_from_buffer_all_fields() {
    let cert = build_certification_from_buffer(
        &[
            "AWS Solutions Architect".to_string(),
            "2021".to_string(),
            "Amazon".to_string(),
            "Coursera".to_string(),
        ],
        Some(("Aug 2021".to_string(), "Aug 2023".to_string())),
    )
    .expect("certification should build");
    assert!(!cert.id.is_empty(), "certification id must be populated");
    assert_eq!(cert.name, "AWS Solutions Architect (2021)");
    assert_eq!(cert.issuer, "Amazon · Coursera");
    assert_eq!(cert.date, "Aug 2021 – Aug 2023");
}

/// Layout (c) of `parse_experiences`: a date-range row that carries
/// only a leading separator and a location before the dates, e.g.
/// "· Paris, France Jan 2024 – Nov 2024". The resulting experience has
/// a non-empty location and start/end dates; deleting any of those
/// struct fields must be caught.
#[test]
fn parse_experiences_layout_c_location_and_dates() {
    let (exps, _skills) = parse_experiences(&[
        "Software Engineer".to_string(),
        "ACME Corp".to_string(),
        "· Paris, France Jan 2024 – Nov 2024".to_string(),
    ]);
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0].location, "Paris, France");
    assert_eq!(exps[0].start_date, "Jan 2024");
    assert_eq!(exps[0].end_date, "Nov 2024");
    assert!(!exps[0].id.is_empty(), "experience id must be populated");
}

// ── id-population across builders (delete id-field mutants) ───────────

#[test]
fn built_structs_populate_ids() {
    let (exps, _skills) = parse_experiences(&[
        "Platform Engineer (contractual)".to_string(),
        "DTNUM/SDAN/BFO".to_string(),
        "Dec 2024 – Feb 2026".to_string(),
    ]);
    assert!(!exps[0].id.is_empty(), "experience id must be populated");

    let edu = build_education_from_buffer(
        &[
            "BSc".to_string(),
            "in Mathematics".to_string(),
            "Univ".to_string(),
        ],
        "2017".to_string(),
        "2020".to_string(),
    )
    .expect("education should build");
    assert!(!edu.id.is_empty(), "education id must be populated");
}
