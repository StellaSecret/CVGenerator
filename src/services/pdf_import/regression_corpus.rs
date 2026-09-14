use super::*;

fn fixture(name: &str) -> String {
    // Fixtures live at `tests/fixtures/pdf_import/<name>` relative to
    // the crate root, alongside (not inside) `src/`, matching normal
    // Rust convention for integration-test data. `include_str!` reads
    // relative to *this source file*, so we go up to the crate root
    // first.
    match name {
        "sidebar_skills_bleed.txt" => {
            include_str!("../../../tests/fixtures/pdf_import/sidebar_skills_bleed.txt").to_string()
        }
        "trait_list_bleed.txt" => {
            include_str!("../../../tests/fixtures/pdf_import/trait_list_bleed.txt").to_string()
        }
        "multi_language_single_line.txt" => {
            include_str!("../../../tests/fixtures/pdf_import/multi_language_single_line.txt")
                .to_string()
        }
        other => panic!("unknown fixture: {other}"),
    }
}

/// No skill entry should read like a sentence fragment (proper nouns
/// and short tags only). This is the general invariant behind the
/// "TECHNICAL SKILLS heading absorbs a stray Project's Situation/
/// Actions/Results bullets" bug class: whatever the specific cause,
/// the symptom is always prose ending up in the Skills list.
fn assert_no_prose_fragments_in_skills(cv: &LifetimeCV) {
    for skill in &cv.skills {
        let word_count = skill.name.split_whitespace().count();
        assert!(
            word_count <= 8 && !skill.name.ends_with('.'),
            "skills list contains a prose fragment, not a skill tag: {:?}",
            skill.name
        );
    }
}

/// No (role, company) pair should appear as the header of more than
/// one Experience entry. This is the general invariant behind the
/// "sidebar bleed splits a job's role+company from its date, and two
/// independent recovery mechanisms both claim it" bug class.
fn assert_no_duplicate_experience_headers(cv: &LifetimeCV) {
    let mut seen = std::collections::HashSet::new();
    for exp in &cv.experiences {
        let key = (exp.role.en.clone(), exp.company.clone());
        assert!(
            seen.insert(key.clone()),
            "duplicate experience header appears twice: {key:?}"
        );
    }
}

/// Every experience's context/bullets should stay under a plausible
/// length and not visibly run two different jobs' text together.
/// Catches the "next job's role+company gets glued onto the tail of
/// this job's last project's context" bleed pattern.
fn assert_no_cross_job_bleed(cv: &LifetimeCV) {
    for exp in &cv.experiences {
        let other_roles: Vec<&str> = cv
            .experiences
            .iter()
            .map(|e| e.role.en.as_str())
            .filter(|r| *r != exp.role.en)
            .collect();
        for project in &exp.projects {
            for other_role in &other_roles {
                if other_role.is_empty() {
                    continue;
                }
                for c in &project.context {
                    assert!(
                        !c.en.contains(other_role),
                        "job {:?}'s project {:?} context contains another job's \
                             role text {:?} — looks like cross-job bleed: {:?}",
                        exp.role.en,
                        project.name.en,
                        other_role,
                        c.en
                    );
                }
            }
        }
    }
}

#[test]
fn regression_sidebar_skills_bleed_keeps_projects_under_correct_job() {
    // Real-world source: a two-column CV where a "TECHNICAL SKILLS" /
    // "TOOLS" sidebar interleaves mid-page into the main narrative
    // column, landing between a job's Project 1 and Project 2. This
    // used to (a) swallow Project 2 and Project 3's entire narrative
    // into the Skills section, mangled into pseudo-skill fragments by
    // `parse_skills`'s comma-join logic, and (b) once fixed to
    // reclaim that content back into Experience, duplicate the next
    // job's role+company header, because `split_into_sections`'s own
    // resumption recovery *also* independently recovers it.
    let cv = parse_cv(&fixture("sidebar_skills_bleed.txt"));

    assert_eq!(cv.experiences.len(), 2, "expected both jobs to parse");
    let job1 = &cv.experiences[0];
    assert_eq!(job1.role.en, "Platform Engineer (contractual)");
    assert_eq!(job1.company, "ACME/WIDGETS/QA");

    let project_names: Vec<&str> = job1
        .projects
        .iter()
        .map(|p| p.name.en.as_str())
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(
        project_names,
        vec![
            "Project 1: Core – Socle Team",
            "Project 2: Zenith – Platform Engineering",
            "Project 3: Cross-cutting Initiatives and Strategic Support",
        ],
        "all three projects must stay nested under job 1, in order"
    );

    // The bullet that used to get orphaned mid-Skills (wrapped across
    // two physical lines, the second with no bullet marker of its
    // own) must land back in Project 1's bullets, not in Skills.
    let project1_bullets: Vec<&str> = job1.projects[1]
        .bullets
        .iter()
        .map(|b| b.en.as_str())
        .collect();
    assert!(
        project1_bullets.iter().any(|b| b.contains("ADR framework")),
        "the wrapped ADR bullet should be recovered into Project 1's \
             bullets, got: {project1_bullets:?}"
    );

    assert_no_prose_fragments_in_skills(&cv);
    assert_no_duplicate_experience_headers(&cv);
    assert_no_cross_job_bleed(&cv);
}

#[test]
fn regression_trait_list_bleed_does_not_drop_bullets() {
    // Real-world source: a "RANDOM SKILLS" sidebar (personality-trait
    // tags, not real skills) interrupts a job's own Actions-taken
    // bullet list — not a sub-project's, the job's own top-level
    // list. This used to silently drop the four action bullets that
    // came *after* the interruption: they landed in the "ignore"
    // bucket (correctly, for the trait tags) but took genuine
    // content down with them, since the whole run was treated as one
    // undifferentiated block up to the next recognized boundary.
    let cv = parse_cv(&fixture("trait_list_bleed.txt"));

    assert_eq!(cv.experiences.len(), 1);
    let job = &cv.experiences[0];
    assert_eq!(job.role.en, "Site Reliability Engineer");
    assert_eq!(job.company, "Nimbus");

    let bullets: Vec<&str> = job.projects[0]
        .bullets
        .iter()
        .map(|b| b.en.as_str())
        .collect();
    for expected in [
        "Maintain IaC with Terraform.",
        "Automated repetitive tasks using Ansible, GitLab-CI, Bash.",
        "Supported developers through self-service tooling and documentation.",
        "Participated in on-call rotations, RCAs, and post-mortems.",
    ] {
        assert!(
            bullets.contains(&expected),
            "expected bullet {expected:?} to survive the RANDOM SKILLS \
                 interruption, got: {bullets:?}"
        );
    }

    // The trait tags themselves must NOT show up as bullets or
    // skills — they're genuinely not part of the CV.
    for junk in ["Jack of all Trades", "Fearless Frontliner", "Break things"] {
        assert!(
            !bullets.iter().any(|b| b.contains(junk)),
            "trait-list junk {junk:?} leaked into bullets: {bullets:?}"
        );
        assert!(
            !cv.skills.iter().any(|s| s.name.contains(junk)),
            "trait-list junk {junk:?} leaked into skills"
        );
    }

    assert_no_duplicate_experience_headers(&cv);
}

#[test]
fn regression_multi_language_single_line_recovers_all_languages() {
    // Real-world source: this project's own CV renderer packs every
    // language onto a single output line as repeated "Name (Level)"
    // segments. Re-importing a CV this renderer generated used to
    // keep only the first language on any such line — a same-app
    // round-trip data-loss bug, not a third-party-PDF quirk.
    let cv = parse_cv(&fixture("multi_language_single_line.txt"));

    let langs: Vec<(&str, &LanguageLevel)> = cv
        .languages
        .iter()
        .map(|l| (l.name.as_str(), &l.level))
        .collect();
    assert_eq!(
        langs,
        vec![
            ("Français", &LanguageLevel::Native),
            ("Anglais", &LanguageLevel::Conversational),
        ]
    );
}

#[test]
fn regression_competency_bullets_with_prose_commas_stay_intact() {
    // Real-world source: a sidebar of full-sentence competency bullets
    // (each a "Verb, verb, verb object" phrase, wrapped across 2-3
    // physical lines by the PDF layout), not a flat list of short
    // "Name N+ yrs" tags. The old block-join logic decided whether to
    // comma-split an entire multi-hundred-word block based only on
    // "does *any* line in it contain a comma" — true here, since these
    // are full sentences — which joined the whole block with spaces
    // and comma-split it as if every comma were a skill separator.
    // Since the first comma doesn't appear until deep into the block,
    // every short standalone tag before it (category headers, soft
    // skills) got fused into one giant run-on "skill" alongside the
    // start of the first real sentence.
    let lines: Vec<String> = [
        "Qualités humaines",
        "Curiosité",
        "Rigoureux",
        "Concevoir, déployer, sécuriser des infrastructures cloud et",
        "on-premise",
        "Administrer des bases de données MySQL/MariaDB,",
        "PostgreSQL, Elasticsearch, OpenSearch, MongoDB",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    let skills = parse_skills(&lines);
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();

    // Short standalone tags stay separate, one per entry — this part
    // already worked before the fix and must keep working.
    assert!(names.contains(&"Qualités humaines"));
    assert!(names.contains(&"Curiosité"));
    assert!(names.contains(&"Rigoureux"));

    // Each wrapped competency sentence survives as ONE entry, physical
    // line-wrap rejoined, prose commas intact — not shredded into
    // word-fragments by comma, and not fused with the unrelated short
    // tags before it.
    assert!(
        names.contains(&"Concevoir, déployer, sécuriser des infrastructures cloud et on-premise"),
        "got: {names:?}"
    );
    assert!(
            names.contains(
                &"Administrer des bases de données MySQL/MariaDB, PostgreSQL, Elasticsearch, OpenSearch, MongoDB"
            ),
            "got: {names:?}"
        );

    // Nothing should look like the old run-on fusion of unrelated tags.
    assert!(
        !names
            .iter()
            .any(|n| n.contains("Qualités humaines Curiosité")),
        "got: {names:?}"
    );
}

#[test]
fn regression_font_glyph_gap_does_not_corrupt_name_detection() {
    // Real-world source: a custom font subset assigned a ligature
    // ("fr") to a glyph ID with no ToUnicode entry at all. The old
    // single-byte fallback inserted a literal NUL character in its
    // place ("Wilfried" -> "Wil\0ied"), and `guess_name`'s alphabetic
    // check then rejected that whole line, silently falling through
    // to the next line — the job title — and using *that* as the
    // person's name instead. Test at the `guess_name` level directly
    // (no PDF needed): a stray control character in the name line
    // must not cause the title to be mistaken for the name.
    let lines = ["Wil\u{0000}ied Maillet", "Ingénieur DevOps"];
    let name = guess_name(&lines).expect("a name should still be found");
    assert_eq!(
        name, "Wilied Maillet",
        "control byte should be dropped, not corrupt the whole line"
    );
    assert_ne!(
        name, "Ingénieur DevOps",
        "name must not fall through to the job title"
    );
}

#[test]
fn regression_tounicode_decode_drops_unmapped_control_bytes() {
    // Same bug, one layer lower: `ToUnicodeMap::decode`'s single-byte
    // fallback must not insert a raw control byte just because it
    // has no map entry — it should drop it, since it never really
    // represented that Latin-1 character in the first place (it was
    // an unresolved glyph ID).
    let map = ToUnicodeMap {
        code_bytes: 1,
        map: std::collections::HashMap::new(), // byte 0x00 unmapped
    };
    let decoded = map.decode(&[0x00]);
    assert!(
        decoded.is_none() || decoded.as_deref() == Some(""),
        "an unmapped control byte should decode to nothing, got: {decoded:?}"
    );
}

// ── ToUnicodeMap::decode / byte helpers ───────────────────────────────────

#[test]
fn tounicode_decode_with_unset_code_bytes_returns_none() {
    // `code_bytes == 0 || bytes.is_empty()` guards the loop; flipping
    // the `||` to `&&` would let a non-empty input fall through and
    // panic on `chunks(0)`. Asserting None kills that mutant.
    let map = ToUnicodeMap {
        code_bytes: 0,
        map: std::collections::HashMap::new(),
    };
    assert_eq!(map.decode(b"abc"), None);
}

#[test]
fn tounicode_decode_stops_at_incomplete_trailing_chunk() {
    // code_bytes = 2; the input has a full 2-byte chunk followed by a
    // single trailing byte. `chunk.len() < code_bytes` must break so
    // the trailing byte is dropped (not fallback-decoded). Flipping the
    // `<` to `>` would never break and would append the trailing byte.
    let mut map = std::collections::HashMap::new();
    map.insert(0x4142, "AB".to_string());
    let map = ToUnicodeMap { code_bytes: 2, map };
    assert_eq!(map.decode(&[0x41, 0x42, 0x43]), Some("AB".to_string()));
}

#[test]
fn tounicode_decode_does_not_latin1_fallback_on_full_unmapped_chunk() {
    // A full 2-byte chunk with no map entry must NOT fall into the
    // single-byte Latin-1 fallback (that is gated on `chunk.len() == 1`).
    let map = ToUnicodeMap {
        code_bytes: 2,
        map: std::collections::HashMap::new(),
    };
    // Correct: no matching 2-byte code, not a 1-byte chunk → None.
    // If `== 1` were flipped to `!= 1`, it would fallback on byte 'A'.
    assert_eq!(map.decode(&[0x41, 0x42]), None);
}

#[test]
fn tounicode_decode_resolves_mapped_one_byte_codes() {
    let mut map = std::collections::HashMap::new();
    map.insert(0x41, "A".to_string());
    map.insert(0x42, "B".to_string());
    let map = ToUnicodeMap { code_bytes: 1, map };
    assert_eq!(map.decode(&[0x41, 0x42, 0x41]), Some("ABA".to_string()));
}

#[test]
fn bytes_to_u32_big_endian_concat() {
    assert_eq!(bytes_to_u32(&[0x12, 0x34, 0x56, 0x78]), 0x12345678);
    assert_eq!(bytes_to_u32(&[0x01, 0x02]), 0x0102);
    assert_eq!(bytes_to_u32(&[0xFF]), 0xFF);
}

#[test]
fn utf16be_bytes_to_string_decodes_pairs() {
    assert_eq!(utf16be_bytes_to_string(&[0x00, 0x41, 0x00, 0x42]), "AB");
    assert_eq!(
        utf16be_bytes_to_string(&[0x20, 0x1E, 0x00, 0x41]),
        "\u{201E}A"
    );
    assert_eq!(utf16be_bytes_to_string(&[]), "");
    // An odd trailing byte is dropped.
    assert_eq!(utf16be_bytes_to_string(&[0x00, 0x41, 0x00]), "A");
}

#[test]
fn parse_hex_token_variants() {
    assert_eq!(parse_hex_token("<4142>"), Some(vec![0x41, 0x42]));
    assert_eq!(parse_hex_token(" <0042> "), Some(vec![0x00, 0x42]));
    // Odd / empty / unterminated forms are rejected.
    assert_eq!(parse_hex_token("<414>"), None);
    assert_eq!(parse_hex_token("<>"), None);
    assert_eq!(parse_hex_token("4142"), None);
    assert_eq!(parse_hex_token("<zz>"), None);
    // A trailing ']' (from a bfrange array) is not a valid closing '>'.
    assert_eq!(parse_hex_token("<4142>]"), None);
}

#[test]
fn parse_tounicode_cmap_bfchar_and_bfrange() {
    let cmap_text = r"
            /CIDInit /ProcSet findresource begin
            12 dict begin
            begincmap
            /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
            /CMapName /Adobe-Identity-UCS def
            /CMapType 2 def
            1 begincodespacerange <00> <ff> endcodespacerange
            2 beginbfchar
            <41> <0041>
            <42> <0042>
            endbfchar
            1 beginbfrange
            <61> <63> <0061>
            endbfrange
            endcmap
            CMapName currentdict /CMap defineresource pop
            end
            end
        ";
    let cmap = parse_tounicode_cmap(cmap_text).expect("cmap should parse");
    assert_eq!(cmap.code_bytes, 1);
    assert_eq!(cmap.map.get(&0x41).map(String::as_str), Some("A"));
    assert_eq!(cmap.map.get(&0x42).map(String::as_str), Some("B"));
    // range 0x61..=0x63 -> a, b, c
    assert_eq!(cmap.map.get(&0x61).map(String::as_str), Some("a"));
    assert_eq!(cmap.map.get(&0x62).map(String::as_str), Some("b"));
    assert_eq!(cmap.map.get(&0x63).map(String::as_str), Some("c"));
}

#[test]
fn parse_tounicode_cmap_bfrange_array_form() {
    let cmap_text = r"
            begincmap
            begincodespacerange <00> <ff> endcodespacerange
            1 beginbfrange
            <61> <63> [ <0041> <0042> <0043> ]
            endbfrange
            endcmap
        ";
    let cmap = parse_tounicode_cmap(cmap_text).expect("cmap should parse");
    assert_eq!(cmap.map.get(&0x61).map(String::as_str), Some("A"));
    assert_eq!(cmap.map.get(&0x62).map(String::as_str), Some("B"));
    assert_eq!(cmap.map.get(&0x63).map(String::as_str), Some("C"));
}

#[test]
fn parse_tounicode_cmap_empty_returns_none() {
    assert!(parse_tounicode_cmap("no cmap constructs here").is_none());
}

/// Smoke test against real PDFs, if any are present locally. This
/// directory is gitignored (see the fixtures README) — nothing here
/// runs in CI unless you've dropped files in yourself. It's meant for
/// manually checking a real problem PDF without ever committing it:
/// drop it in `tests/fixtures/pdf_import/local/`, run
/// `cargo test --release local_corpus_smoke_test -- --ignored --nocapture`,
/// and it'll report which files, if any, violate the general
/// invariants below — no per-file hand-written expectations needed.
#[test]
#[ignore = "only runs against locally-added, gitignored real PDFs"]
fn local_corpus_smoke_test() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf_import/local");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!(
            "no local corpus at {}; nothing to smoke-test",
            dir.display()
        );
        return;
    };
    let mut checked = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pdf") {
            continue;
        }
        checked += 1;
        let bytes = std::fs::read(&path).expect("read fixture PDF");
        let cv = match import_pdf(&bytes) {
            Ok(cv) => cv,
            Err(e) => panic!("{}: import_pdf failed: {e}", path.display()),
        };
        assert!(
            !cv.experiences.is_empty(),
            "{}: found zero experience entries — likely a parse failure",
            path.display()
        );
        assert_no_prose_fragments_in_skills(&cv);
        assert_no_duplicate_experience_headers(&cv);
        assert_no_cross_job_bleed(&cv);
        println!(
            "{}: OK ({} experiences, {} skills, {} languages)",
            path.display(),
            cv.experiences.len(),
            cv.skills.len(),
            cv.languages.len()
        );
    }
    if checked == 0 {
        eprintln!("local corpus dir exists but has no .pdf files");
    }
}
